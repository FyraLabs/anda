use std::{
    io::Write,
    os::fd::FromRawFd,
    sync::{atomic::AtomicBool, Arc},
};

use color_eyre::eyre::{eyre, Context};
use nix::sys::signal;

type PeekBuf<'a> = std::iter::Peekable<std::slice::Iter<'a, u8>>;
const PRINT_LOG_TIMEOUT: i32 = 50;

pub struct PseudoTerminal {
    fd: std::ffi::c_int,
    stop: Arc<AtomicBool>,
    lf: Arc<AtomicBool>,
    cr: Arc<AtomicBool>,
}

impl PseudoTerminal {
    pub fn new(lf: Arc<AtomicBool>, cr: Arc<AtomicBool>) -> Self {
        // SAFETY: new pseudoterminal device
        let fd = unsafe { nix::libc::posix_openpt(nix::libc::O_RDWR | nix::libc::O_NONBLOCK) };
        // SAFETY: grant access to the slave pseudoterminal
        assert_eq!(unsafe { nix::libc::grantpt(fd) }, 0, "grantpt failed");
        // SAFETY: unlock a pseudoterminal master/slave pair
        assert_eq!(unsafe { nix::libc::unlockpt(fd) }, 0, "unlockpt failed");

        Self { fd, stop: Arc::new(AtomicBool::new(false)), lf, cr }
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

    pub fn slave_stdio(&self) -> std::process::Stdio {
        // SAFETY: turning into Stdio
        unsafe { std::process::Stdio::from_raw_fd(self.slave()) }
    }

    pub fn read(&self, buf: &mut [u8]) -> Option<nix::Result<usize>> {
        let mut pollfd = nix::libc::pollfd { fd: self.fd, events: nix::libc::POLLIN, revents: 0 };
        while !self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            // SAFETY: int poll(struct pollfd *fds, nfds_t nfds, int timeout);
            // `fds` should be an array of `struct pollfd` with size `nfds`
            // negative `timeout` results in infinite waiting
            // return value is number of modified objects in the `pollfd` array
            // here the return value must be either 1 (something happened) or 0 (timeout)
            match unsafe { nix::libc::poll(&raw mut pollfd, 1, PRINT_LOG_TIMEOUT) } {
                0 => {}
                1 => return Some(nix::unistd::read(self, buf)),
                rc => panic!("unexpected return value from poll(): {rc}"),
            }
        }
        None
    }

    // BUG: in extreme cases, comparing bytechars would not work for unicode that spans multiple
    // bytes, or in specific terminal modes that absolutely ignore them (e.g. sixel)
    #[tracing::instrument(skip(self))]
    pub fn print_log(&self, prefix: &[u8]) {
        let mut buf = [0u8; 256];
        let mut newbuf = Vec::with_capacity(256);
        while let Ok(Some(len @ 1..)) =
            self.read(&mut buf).transpose().inspect_err(|e| tracing::error!("read(): {e:?}"))
        {
            newbuf.clear();
            newbuf.reserve(len.saturating_add_signed(64));

            self.transform_log(
                buf.get(..len).expect("out of range buf slicing from read()"),
                &mut newbuf,
                prefix,
            );

            std::io::stdout().write_all(&newbuf).expect("cannot write to stdout");
        }
    }

    fn transform_log(&self, buf: &[u8], newbuf: &mut Vec<u8>, prefix: &[u8]) {
        let mut buf = buf.iter().peekable();

        if Self::flag_was_true_then_set_false(&self.lf) {
            newbuf.extend_from_slice(prefix);
        }

        if Self::flag_was_true_then_set_false(&self.cr) {
            if buf.peek() == Some(&&b'\n') {
                buf.next(); // consume
                newbuf.push(b'\n');
            }
            newbuf.extend_from_slice(prefix);
        }

        while let Some(&c) = buf.next() {
            newbuf.push(c);
            if c == b'\n' {
                self.transform_lf(newbuf, prefix, &mut buf);
            } else if c == b'\r' {
                self.transform_cr(newbuf, prefix, &mut buf);
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

    fn transform_lf(&self, newbuf: &mut Vec<u8>, prefix: &[u8], buf: &mut PeekBuf<'_>) {
        if buf.peek().is_none() {
            self.lf.store(true, std::sync::atomic::Ordering::Relaxed);
        } else {
            newbuf.extend_from_slice(prefix);
        }
    }

    fn transform_cr(&self, newbuf: &mut Vec<u8>, prefix: &[u8], buf: &mut PeekBuf<'_>) {
        let Some(&&next) = buf.peek() else {
            self.cr.store(true, std::sync::atomic::Ordering::Relaxed);
            return;
        };
        if next == b'\n' {
            buf.next(); // consume
            *newbuf.last_mut().unwrap() = b'\n'; // ignore \r (added by the caller)
            self.transform_lf(newbuf, prefix, buf);
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

pub struct PseudoTerminalCtl {
    stdout: PseudoTerminal,
    stderr: PseudoTerminal,
}

impl PseudoTerminalCtl {
    pub fn new(process: &std::ffi::OsStr) -> Self {
        let (lf, cr) = (Arc::new(AtomicBool::new(true)), Arc::new(AtomicBool::new(false)));
        let ret = Self {
            stdout: PseudoTerminal::new(Arc::clone(&lf), Arc::clone(&cr)),
            stderr: PseudoTerminal::new(Arc::clone(&lf), Arc::clone(&cr)),
        };

        let mut winsize = Self::real_winsize();
        winsize.ws_col -= u16::try_from(process.len()).expect("process name too long");
        winsize.ws_col -= 3; // ` │ ` ← 3 cols
        ret.stdout.set_winsize(winsize);
        ret.stderr.set_winsize(winsize);

        ret
    }

    fn real_winsize() -> nix::libc::winsize {
        let mut winsize = nix::libc::winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };

        // SAFETY: see documentations for TIOCGWINSZ. This obtains the terminal window size
        if unsafe {
            nix::libc::ioctl(nix::libc::STDOUT_FILENO, nix::libc::TIOCGWINSZ, &raw mut winsize)
        } != 0
        {
            // default to 80x24 if no tty is connected (ex: ci)
            winsize = nix::libc::winsize { ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
        }

        winsize
    }

    pub async fn run(self, cmd: &mut tokio::process::Command) -> color_eyre::Result<()> {
        let process = cmd.as_std().get_program();
        let process_str = process.to_string_lossy().into_owned();
        let (stdout_prefix, stderr_prefix) = Self::get_prefix(process.as_encoded_bytes());

        let mut out = cmd
            .stdout(self.stdout.slave_stdio())
            .stderr(self.stderr.slave_stdio())
            .spawn()
            .context(eyre!("Cannot run {process_str}"))?;

        let (stdout_stop, stderr_stop) =
            (Arc::clone(&self.stdout.stop), Arc::clone(&self.stderr.stop));

        tokio::join!(
            tokio::task::spawn_blocking(move || {
                self.stdout.print_log(&stdout_prefix);
            }),
            tokio::task::spawn_blocking(move || {
                self.stderr.print_log(&stderr_prefix);
            }),
            tokio::spawn(async move {
                let res = tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("Received ctrl-c, sending sigint to child process");
                        #[allow(clippy::cast_possible_wrap)]
                        signal::kill(nix::unistd::Pid::from_raw(out.id().unwrap() as i32), signal::Signal::SIGINT).unwrap();
                        Err(eyre!("Received ctrl-c, exiting"))
                    }
                    w = out.wait() => {
                        let status = w.unwrap();
                        if status.success() {
                            tracing::info!("Command exited successfully");
                            Ok(())
                        } else {
                            Err(eyre!("Command exited with status: {status}"))
                        }
                    }
                };
                stdout_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                stderr_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                println!("");
                res
            })
        ).2?
    }

    fn get_prefix(process: &[u8]) -> (Vec<u8>, Vec<u8>) {
        const CYAN: &[u8] = b"\x1b[36m";
        const YELLOW: &[u8] = b"\x1b[33m";
        const RESET: &[u8] = b"\x1b[0m";

        if std::env::var("NO_COLOR").is_ok() {
            return (process.to_owned(), process.to_owned());
        }

        (
            [CYAN, process, RESET, " │ ".as_bytes()].concat(),
            [YELLOW, process, RESET, " │ ".as_bytes()].concat(),
        )
    }
}
