use std::{
    io::Write,
    os::fd::FromRawFd,
    sync::{atomic::AtomicBool, Arc},
};

use color_eyre::eyre::{eyre, Context};
use nix::sys::signal;
use tokio::{
    process::Child,
    sync::oneshot,
    time::{timeout, Duration, Instant},
};

/// Dropping a command future asks its owner to terminate and reap it. The
/// supervisor remains alive until that work (and PTY reader shutdown) finishes.
pub struct CancelOnDrop {
    pub sender: Option<oneshot::Sender<()>>,
    pub cancelled: Arc<AtomicBool>,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            self.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            sender.send(()).ok();
        }
    }
}

pub struct AbortOnDrop(pub tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn signal_group(group: nix::unistd::Pid, sig: signal::Signal) {
    if let Err(error) = signal::killpg(group, sig) {
        if error != nix::errno::Errno::ESRCH {
            tracing::warn!(%error, ?sig, "Cannot signal command process group");
        }
    }
}

/// Teardown could not establish that it is safe to reuse/remove the root.
#[derive(Debug)]
pub struct TeardownFailed(pub String);

impl std::fmt::Display for TeardownFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command teardown failed: {}", self.0)
    }
}

impl std::error::Error for TeardownFailed {}

fn group_has_live_members(group: nix::unistd::Pid) -> std::io::Result<bool> {
    match signal::killpg(group, None) {
        Err(nix::errno::Errno::ESRCH) => return Ok(false),
        // A hidden privileged process group is not proof of quiescence. Never
        // let visibility-filtered /proc enumeration override a denied probe.
        Err(error) => return Err(std::io::Error::from_raw_os_error(error as i32)),
        Ok(()) => {}
    }
    // killpg(0) also sees zombies, which cannot mutate the root. After SIGKILL
    // wait for all *live* group members, not just the leader, before cleanup.
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let fields =
            stat.rsplit_once(')').ok_or_else(|| std::io::Error::other("Invalid proc stat"))?.1;
        let mut fields = fields.split_whitespace();
        let state = fields.next();
        fields.next(); // ppid
        let process_group = fields.next().and_then(|field| field.parse::<i32>().ok());
        if process_group == Some(group.as_raw()) && !matches!(state, Some("Z" | "X")) {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn terminate_and_reap(
    child: &mut Child,
    group: nix::unistd::Pid,
    grace: Duration,
) -> color_eyre::Result<()> {
    // Give Mock a chance to unmount/finish its own teardown before escalating.
    // Signal the entire group, including helpers, not just the Python leader.
    for sig in [signal::Signal::SIGINT, signal::Signal::SIGTERM] {
        signal_group(group, sig);
        let started = Instant::now();
        loop {
            child.try_wait().context("Cannot reap command")?;
            if signal::killpg(group, None) == Err(nix::errno::Errno::ESRCH) {
                child.wait().await.context("Cannot reap command")?;
                return Ok(());
            }
            if started.elapsed() >= grace {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    signal_group(group, signal::Signal::SIGKILL);
    timeout(grace, child.wait())
        .await
        .context("Timed out reaping killed command")?
        .context("Cannot reap killed command")?;
    timeout(grace, async {
        while group_has_live_members(group)? {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .context("Timed out waiting for killed command descendants")??;
    Ok(())
}

async fn teardown(child: &mut Child, group: nix::unistd::Pid) -> color_eyre::Result<()> {
    terminate_and_reap(child, group, Duration::from_secs(2))
        .await
        .map_err(|error| eyre!(TeardownFailed(error.to_string())))
}

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
        // Amortize PTY reads, stdout locking, and writes for verbose compilers.
        // Reads still return immediately when less data is available.
        let mut buf = [0u8; 16 * 1024];
        let mut newbuf = Vec::with_capacity(buf.len());
        while let Ok(Some(len @ 1..)) = self.read(&mut buf).transpose().inspect_err(|e| {
            // Linux PTY masters report EIO when the last slave closes.
            // This is normal end-of-stream, not a failed build/log reader.
            if *e != nix::errno::Errno::EIO {
                tracing::error!("read(): {e:?}");
            }
        }) {
            newbuf.clear();
            newbuf.reserve(len.saturating_add(prefix.len()));

            self.transform_log(
                buf.get(..len).expect("out of range buf slicing from read()"),
                &mut newbuf,
                prefix,
            );

            // A closed output pipe must not panic a reader and hide process
            // teardown state. Stop forwarding, but keep draining the child.
            if std::io::stdout().write_all(&newbuf).is_err() {
                while let Some(Ok(len)) = self.read(&mut buf) {
                    if len == 0 {
                        break;
                    }
                }
                return;
            }
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
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let process = cmd.as_std().get_program();
        let process_str = process.to_string_lossy().into_owned();
        let (stdout_prefix, stderr_prefix) = Self::get_prefix(process.as_encoded_bytes());

        cmd.process_group(0)
            .kill_on_drop(true)
            .stdout(self.stdout.slave_stdio())
            .stderr(self.stderr.slave_stdio());
        let spawned = cmd.spawn().context(eyre!("Cannot run {process_str}"));
        // Command retains its configured Stdio; don't leave slave FDs open in
        // the caller after spawn (or a failed spawn).
        cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let mut out = spawned?;
        let group =
            nix::unistd::Pid::from_raw(i32::try_from(out.id().expect("newly spawned child"))?);
        let (sender, receiver) = oneshot::channel();
        let mut guard =
            CancelOnDrop { sender: Some(sender), cancelled: Arc::new(AtomicBool::new(false)) };
        let supervisor = tokio::spawn(async move {
            let (stdout_stop, stderr_stop) =
                (Arc::clone(&self.stdout.stop), Arc::clone(&self.stderr.stop));
            let stdout = tokio::task::spawn_blocking(move || self.stdout.print_log(&stdout_prefix));
            let stderr = tokio::task::spawn_blocking(move || self.stderr.print_log(&stderr_prefix));
            let res = tokio::select! {
                () = async {
                    tokio::select! {
                        _ = receiver => {},
                        () = cancellation => {},
                        _ = tokio::signal::ctrl_c(), if handle_ctrl_c => {},
                    }
                } => {
                    tracing::info!("Command cancelled; terminating process group");
                    teardown(&mut out, group).await
                        .and_then(|()| Err(eyre!("Command cancelled")))
                }
                status = out.wait() => {
                    let status = status.context("Cannot wait for command");
                    // Failed tools can leave helpers alive too. Quiesce them
                    // before returning to a root cleanup or dropping tempdirs.
                    let teardown = if signal::killpg(group, None) == Err(nix::errno::Errno::ESRCH) {
                        Ok(())
                    } else {
                        teardown(&mut out, group).await
                    };
                    teardown.and(status).and_then(|status| {
                        if status.success() {
                            Ok(())
                        } else {
                            Err(eyre!("Command exited with status: {status}"))
                        }
                    })
                }
            };
            stdout_stop.store(true, std::sync::atomic::Ordering::Relaxed);
            stderr_stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let (stdout, stderr) = tokio::join!(stdout, stderr);
            // Teardown safety takes precedence over secondary logging errors:
            // callers must still recognize TeardownFailed and retain the root.
            res?;
            stdout?;
            stderr?;
            // The command is already complete; a closed output pipe cannot
            // change its result or process-teardown status.
            drop(std::io::stdout().write_all(b"\n"));
            Ok(())
        });
        let result = supervisor.await?;
        guard.sender.take();
        result
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
    async fn escalation_signals_descendants_and_reaps_leader() {
        let control = tempfile::tempdir().unwrap();
        let ready = control.path().join("ready");
        let reaped = control.path().join("reaped");
        let mut cmd = tokio::process::Command::new("python3");
        cmd.arg("-c")
            .arg(
                "
import os, signal, sys, time
from pathlib import Path
signal.signal(signal.SIGINT, signal.SIG_IGN)
pid = os.fork()
if pid == 0:
    while True: time.sleep(.02)
signal.signal(signal.SIGTERM, signal.SIG_IGN)
Path(sys.argv[1]).write_text(str(pid))
os.waitpid(pid, 0)
Path(sys.argv[2]).write_text('reaped')
while True: time.sleep(.02)
",
            )
            .arg(&ready)
            .arg(&reaped)
            .process_group(0)
            .kill_on_drop(true);
        let mut child = cmd.spawn().unwrap();
        let pid = child.id().unwrap();
        wait_for(&ready).await;
        let helper: u32 = std::fs::read_to_string(&ready).unwrap().parse().unwrap();
        timeout(
            Duration::from_secs(3),
            terminate_and_reap(
                &mut child,
                nix::unistd::Pid::from_raw(i32::try_from(pid).unwrap()),
                Duration::from_millis(200),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(reaped.exists(), "SIGTERM was not delivered to descendant");
        assert_eq!(child.wait().await.unwrap().code(), None);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(!std::path::Path::new(&format!("/proc/{helper}")).exists());
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
