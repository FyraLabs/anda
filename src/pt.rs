use std::{io::Write, sync::atomic::AtomicBool};

static LF: AtomicBool = AtomicBool::new(true);
static CR: AtomicBool = AtomicBool::new(false);
type PeekBuf<'a> = std::iter::Peekable<std::slice::Iter<'a, u8>>;
const PRINT_LOG_TIMEOUT: i32 = 50;

pub struct PseudoTerminal {
    fd: std::ffi::c_int,
}

impl PseudoTerminal {
    pub fn new() -> Self {
        // SAFETY: new pseudoterminal device
        let fd = unsafe { nix::libc::posix_openpt(nix::libc::O_RDWR | nix::libc::O_NONBLOCK) };
        // SAFETY: grant access to the slave pseudoterminal
        assert_eq!(unsafe { nix::libc::grantpt(fd) }, 0, "grantpt failed");
        // SAFETY: unlock a pseudoterminal master/slave pair
        assert_eq!(unsafe { nix::libc::unlockpt(fd) }, 0, "unlockpt failed");

        Self { fd }
    }
    pub fn set_winsize(&self, winsize: nix::libc::winsize) {
        assert_eq!(
            // SAFETY: set terminal window size
            unsafe {
                nix::libc::ioctl(self.fd, nix::libc::TIOCSWINSZ, std::ptr::from_ref(&winsize))
            },
            0,
            "ioctl failed"
        );
    }

    pub fn slave(&self) -> std::ffi::c_int {
        // SAFETY: obtain name of slave pseudoterminal
        let slave_name = unsafe { nix::libc::ptsname(self.fd) };
        // SAFETY: open fd
        let slave_fd = unsafe { nix::libc::open(slave_name, nix::libc::O_RDWR) };
        assert_eq!(
            // SAFETY: set FD_CLOEXEC (close-on-exec)
            unsafe { nix::libc::fcntl(slave_fd, nix::libc::F_SETFD, nix::libc::FD_CLOEXEC) },
            0,
            "cannot set fcntl FD_CLOEXEC"
        );
        slave_fd
    }

    pub fn read(&self, buf: &mut [u8]) -> Option<nix::Result<usize>> {
        let mut pollfd = nix::libc::pollfd { fd: self.fd, events: nix::libc::POLLIN, revents: 0 };
        loop {
            // SAFETY: int poll(struct pollfd *fds, nfds_t nfds, int timeout);
            // `fds` should be an array of `struct pollfd` with size `nfds`
            // negative `timeout` results in infinite waiting
            // return value is number of modified objects in the `pollfd` array
            // here the return value must be either 1 (something happened) or 0 (timeout)
            match unsafe { nix::libc::poll(&raw mut pollfd, 1, PRINT_LOG_TIMEOUT) } {
                0 if crate::util::STOP.load(std::sync::atomic::Ordering::Relaxed) => break None,
                0 => {}
                1 => {
                    // in some cases a 1 is reported even when there isnt an error but the stream is just empty or ended
                    if pollfd.revents & nix::libc::POLLHUP != 0 {
                        break match nix::unistd::read(self, buf) {
                            Ok(0) | Err(nix::errno::Errno::EIO) => None,
                            other => Some(other),
                        };
                    }
                    break Some(nix::unistd::read(self, buf));
                }
                rc => panic!("unexpected return value from poll(): {rc}"),
            }
        }
    }

    fn get_prefix(process: &std::ffi::OsStr, out: crate::util::ConsoleOut) -> String {
        let process = process.to_str().expect("to_str() returns None");

        let process = {
            if std::env::var("NO_COLOR").is_ok() {
                console::style(process)
            } else {
                match out {
                    crate::util::ConsoleOut::Stdout => console::style(process).cyan(),
                    crate::util::ConsoleOut::Stderr => console::style(process).yellow(),
                }
            }
        };

        format!("{process} │ ")
    }

    // BUG: in extreme cases, comparing bytechars would not work for unicode that spans multiple
    // bytes, or in specific terminal modes that absolutely ignore them (e.g. sixel)
    #[tracing::instrument(skip(self))]
    pub fn print_log(&self, process: &std::ffi::OsStr, out: crate::util::ConsoleOut) {
        let prefix = Self::get_prefix(process, out);
        let mut buf = [0u8; 256];
        let mut newbuf = Vec::with_capacity(256);
        let mut len;
        while {
            let Some(read) = self.read(&mut buf) else { return };
            len = read.expect("read() failed");
            len != 0
        } {
            newbuf.clear();
            newbuf.reserve(len.saturating_add_signed(64));

            Self::transform_log(
                buf.get(..len).expect("out of range buf slicing from read()"),
                &mut newbuf,
                prefix.as_bytes(),
            );

            std::io::stdout().write_all(&newbuf).expect("cannot write to stdout");
        }
    }

    fn transform_log(buf: &[u8], newbuf: &mut Vec<u8>, prefix: &[u8]) {
        let mut buf = buf.iter().peekable();

        if Self::flag_was_true_then_set_false(&LF) {
            newbuf.extend_from_slice(prefix);
        }

        if Self::flag_was_true_then_set_false(&CR) {
            if buf.peek() == Some(&&b'\n') {
                buf.next(); // consume
                newbuf.push(b'\n');
            }
            newbuf.extend_from_slice(prefix);
        }

        while let Some(&c) = buf.next() {
            newbuf.push(c);
            if c == b'\n' {
                Self::transform_lf(newbuf, prefix, &mut buf);
            }
            if c == b'\r' {
                Self::transform_cr(newbuf, prefix, &mut buf);
            }
        }
    }

    fn flag_was_true_then_set_false(flag: &AtomicBool) -> bool {
        flag.fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |b| b.then_some(false),
        ) == Ok(true)
    }

    fn transform_lf(newbuf: &mut Vec<u8>, prefix: &[u8], buf: &mut PeekBuf<'_>) {
        if buf.peek().is_none() {
            LF.store(true, std::sync::atomic::Ordering::Relaxed);
        } else {
            newbuf.extend_from_slice(prefix);
        }
    }

    fn transform_cr(newbuf: &mut Vec<u8>, prefix: &[u8], buf: &mut PeekBuf<'_>) {
        let Some(&&next) = buf.peek() else {
            CR.store(true, std::sync::atomic::Ordering::Relaxed);
            return;
        };
        if next == b'\n' {
            buf.next(); // consume
            *newbuf.last_mut().unwrap() = b'\n'; // ignore \r
            Self::transform_lf(newbuf, prefix, buf);
        }
    }
}

impl std::os::fd::AsFd for PseudoTerminal {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        // SAFETY: `self.fd` is a valid, open file descriptor
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.fd) }
    }
}

impl std::os::fd::AsRawFd for PseudoTerminal {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.fd
    }
}

impl Drop for PseudoTerminal {
    fn drop(&mut self) {
        // SAFETY: close fd
        // Pray to Linus Torvalds that nobody thinks they also own our マスター！！！！()
        let c = unsafe { nix::libc::close(self.fd) };
        if c != 0 {
            tracing::error!("cannot close fd: libc::close() returned {c}");
        }
    }
}
