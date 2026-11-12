use std::{
    future::Future,
    io::Write,
    os::fd::FromRawFd,
    sync::{atomic::AtomicBool, Arc},
};

use color_eyre::eyre::{eyre, Context};

use crate::process_group::{CancelOnDrop, ProcessGroupChild};

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
                -1 if std::io::Error::last_os_error().raw_os_error() == Some(nix::libc::EINTR) => {}
                rc => panic!("unexpected return value from poll(): {rc}"),
            }
        }
        None
    }

    // BUG: in extreme cases, comparing bytechars would not work for unicode that spans multiple
    // bytes, or in specific terminal modes that absolutely ignore them (e.g. sixel)
    #[tracing::instrument(skip(self))]
    pub fn print_log(&self, prefix: &[u8]) {
        let mut buf = [0u8; 16 * 1024];
        let mut newbuf = Vec::with_capacity(buf.len());
        while let Ok(Some(len @ 1..)) =
            self.read(&mut buf).transpose().inspect_err(|error| Self::report_read_error(*error))
        {
            newbuf.clear();
            newbuf.reserve(len.saturating_add(prefix.len()));

            self.transform_log(
                buf.get(..len).expect("out of range buf slicing from read()"),
                &mut newbuf,
                prefix,
            );

            if std::io::stdout().write_all(&newbuf).is_err() {
                self.discard_remaining_output(&mut buf);
                return;
            }
        }
    }

    fn report_read_error(error: nix::errno::Errno) {
        // the PTY master reports EIO once the last slave closes, which is a normal end of stream
        if error != nix::errno::Errno::EIO {
            tracing::error!("read(): {error:?}");
        }
    }

    fn discard_remaining_output(&self, buf: &mut [u8]) {
        while let Some(Ok(1..)) = self.read(buf) {}
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
        flag.try_update(
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
        let process_width = u16::try_from(process.len()).unwrap_or(u16::MAX);
        winsize.ws_col = winsize.ws_col.saturating_sub(process_width).saturating_sub(3); // ` │ ` ← 3 cols
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
        self.run_with_cancellation(cmd, std::future::pending(), true).await
    }

    pub async fn run_with_cancellation<F>(
        self,
        cmd: &mut tokio::process::Command,
        cancellation: F,
        handle_ctrl_c: bool,
    ) -> color_eyre::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let child = self.spawn(cmd)?;
        let (mut guard, dropped) = CancelOnDrop::new(Arc::default());
        let cancelled = async move {
            tokio::select! {
                _ = dropped => {},
                () = cancellation => {},
                _ = tokio::signal::ctrl_c(), if handle_ctrl_c => {},
            }
        };
        let process = cmd.as_std().get_program().as_encoded_bytes().to_owned();
        let result = tokio::spawn(self.supervise(child, cancelled, process)).await?;
        guard.disarm();
        result
    }

    fn spawn(&self, cmd: &mut tokio::process::Command) -> color_eyre::Result<ProcessGroupChild> {
        let program = cmd.as_std().get_program().to_string_lossy().into_owned();
        cmd.process_group(0)
            .kill_on_drop(true)
            .stdout(self.stdout.slave_stdio())
            .stderr(self.stderr.slave_stdio());
        let spawned = cmd.spawn();
        Self::release_slave_fds(cmd);
        ProcessGroupChild::new(spawned.context(eyre!("Cannot run {program}"))?)
    }

    fn release_slave_fds(cmd: &mut tokio::process::Command) {
        cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    }

    async fn supervise(
        self,
        mut child: ProcessGroupChild,
        cancelled: impl Future<Output = ()>,
        process: Vec<u8>,
    ) -> color_eyre::Result<()> {
        let (stdout_prefix, stderr_prefix) = Self::get_prefix(&process);
        let (stdout_stop, stderr_stop) =
            (Arc::clone(&self.stdout.stop), Arc::clone(&self.stderr.stop));
        let stdout = tokio::task::spawn_blocking(move || self.stdout.print_log(&stdout_prefix));
        let stderr = tokio::task::spawn_blocking(move || self.stderr.print_log(&stderr_prefix));
        let result = child.run_until(cancelled).await;
        stdout_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        stderr_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let (stdout, stderr) = tokio::join!(stdout, stderr);
        result?;
        stdout?;
        stderr?;
        drop(std::io::stdout().write_all(b"\n"));
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    async fn wait_for(path: &std::path::Path) {
        timeout(Duration::from_secs(5), async {
            while !path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("synthetic child failed to become ready");
    }

    #[tokio::test]
    async fn dropped_logger_future_terminates_and_reaps_its_child() {
        let control = tempfile::tempdir().unwrap();
        let ready = control.path().join("ready");
        let stopped = control.path().join("stopped");
        let mut cmd = tokio::process::Command::new("python3");
        cmd.arg("-c")
            .arg(
                "
import os, signal, sys, time
from pathlib import Path
def stop(*_):
    Path(sys.argv[2]).write_text('stopped')
    sys.exit(0)
signal.signal(signal.SIGINT, stop)
Path(sys.argv[1]).write_text(str(os.getpid()))
while True: time.sleep(.02)
",
            )
            .arg(&ready)
            .arg(&stopped);
        let caller = tokio::spawn(async move {
            PseudoTerminalCtl::new(cmd.as_std().get_program()).run(&mut cmd).await
        });
        wait_for(&ready).await;
        let pid = std::fs::read_to_string(&ready).unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        wait_for(&stopped).await;
        timeout(Duration::from_secs(3), async {
            while std::path::Path::new(&format!("/proc/{pid}")).exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("cancelled child was not reaped");
    }

    #[tokio::test]
    async fn spawn_and_exit_failures_are_reported_without_hanging_readers() {
        let mut missing = tokio::process::Command::new("/anda-test-no-such-tool");
        let result = timeout(
            Duration::from_secs(2),
            PseudoTerminalCtl::new(missing.as_std().get_program()).run(&mut missing),
        )
        .await
        .unwrap();
        assert!(result.unwrap_err().to_string().contains("Cannot run"));
        let mut failure = tokio::process::Command::new("python3");
        failure.arg("-c").arg("import sys; sys.exit(37)");
        let result = timeout(
            Duration::from_secs(2),
            PseudoTerminalCtl::new(failure.as_std().get_program()).run(&mut failure),
        )
        .await
        .unwrap();
        assert!(result.unwrap_err().to_string().contains("37"));
    }
}
