use std::{
    future::Future,
    process::ExitStatus,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use color_eyre::{
    eyre::{eyre, Context},
    Result,
};
use nix::{
    errno::Errno,
    sys::signal::{self, Signal},
    unistd::Pid,
};
use procfs::{process::ProcState, ProcError};
use tokio::{
    process::Child,
    sync::oneshot,
    time::{sleep, timeout, Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const TEARDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct TeardownFailed(String);

impl std::fmt::Display for TeardownFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command teardown failed: {}", self.0)
    }
}

impl std::error::Error for TeardownFailed {}

pub struct CancelOnDrop {
    sender: Option<oneshot::Sender<()>>,
    cancelled: Arc<AtomicBool>,
}

impl CancelOnDrop {
    pub fn new(cancelled: Arc<AtomicBool>) -> (Self, oneshot::Receiver<()>) {
        let (sender, receiver) = oneshot::channel();
        (Self { sender: Some(sender), cancelled }, receiver)
    }

    pub fn disarm(&mut self) {
        self.sender.take();
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            self.cancelled.store(true, Ordering::Relaxed);
            sender.send(()).ok();
        }
    }
}

pub struct InterruptWatch(tokio::task::JoinHandle<()>);

impl InterruptWatch {
    pub fn spawn(cancelled: Arc<AtomicBool>) -> (Self, oneshot::Receiver<()>) {
        let (sender, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancelled.store(true, Ordering::Relaxed);
                sender.send(()).ok();
            }
        });
        (Self(task), receiver)
    }
}

impl Drop for InterruptWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct ProcessGroupChild {
    leader: Child,
    group: Pid,
}

impl ProcessGroupChild {
    pub fn new(leader: Child) -> Result<Self> {
        let pid = leader.id().ok_or_else(|| eyre!("Child exited before it could be supervised"))?;
        Ok(Self { leader, group: Pid::from_raw(i32::try_from(pid)?) })
    }

    pub async fn run_until<F: Future<Output = ()>>(&mut self, cancelled: F) -> Result<()> {
        tokio::select! {
            () = cancelled => {
                tracing::info!("Command cancelled; terminating process group");
                self.stop().await?;
                Err(eyre!("Command cancelled"))
            }
            status = self.leader.wait() => {
                let status = status.context("Cannot wait for command");
                self.stop_leftover_helpers().await?;
                Self::check_exit(status?)
            }
        }
    }

    fn check_exit(status: ExitStatus) -> Result<()> {
        if status.success() {
            Ok(())
        } else {
            Err(eyre!("Command exited with status: {status}"))
        }
    }

    async fn stop(&mut self) -> Result<(), TeardownFailed> {
        self.terminate(TEARDOWN_GRACE).await.map_err(|error| TeardownFailed(error.to_string()))
    }

    async fn stop_leftover_helpers(&mut self) -> Result<(), TeardownFailed> {
        if self.is_empty() {
            Ok(())
        } else {
            self.stop().await
        }
    }

    async fn terminate(&mut self, grace: Duration) -> Result<()> {
        for sig in [Signal::SIGINT, Signal::SIGTERM] {
            self.signal(sig);
            if self.wait_until_empty(grace).await? {
                self.leader.wait().await.context("Cannot reap command")?;
                return Ok(());
            }
        }
        self.kill(grace).await
    }

    async fn kill(&mut self, grace: Duration) -> Result<()> {
        self.signal(Signal::SIGKILL);
        timeout(grace, self.leader.wait())
            .await
            .context("Timed out reaping killed command")?
            .context("Cannot reap killed command")?;
        timeout(grace, async {
            while self.has_running_members()? {
                sleep(POLL_INTERVAL).await;
            }
            Ok::<_, ProcError>(())
        })
        .await
        .context("Timed out waiting for killed command descendants")??;
        Ok(())
    }

    async fn wait_until_empty(&mut self, grace: Duration) -> Result<bool> {
        let started = Instant::now();
        loop {
            self.leader.try_wait().context("Cannot reap command")?;
            if self.is_empty() {
                return Ok(true);
            }
            if started.elapsed() >= grace {
                return Ok(false);
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    fn signal(&self, sig: Signal) {
        if let Err(error) = signal::killpg(self.group, sig) {
            if error != Errno::ESRCH {
                tracing::warn!(%error, ?sig, "Cannot signal command process group");
            }
        }
    }

    fn is_empty(&self) -> bool {
        signal::killpg(self.group, None) == Err(Errno::ESRCH)
    }

    fn has_running_members(&self) -> Result<bool, ProcError> {
        match signal::killpg(self.group, None) {
            Err(Errno::ESRCH) => return Ok(false),
            Err(error) => return Err(std::io::Error::from(error).into()),
            Ok(()) => {}
        }
        for process in procfs::process::all_processes()? {
            let stat = match process.and_then(|process| process.stat()) {
                Ok(stat) => stat,
                Err(ProcError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            };
            if stat.pgrp == self.group.as_raw()
                && !matches!(stat.state(), Ok(ProcState::Zombie | ProcState::Dead))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_for(path: &std::path::Path) {
        timeout(Duration::from_secs(5), async {
            while !path.exists() {
                sleep(Duration::from_millis(10)).await;
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
        let mut child = ProcessGroupChild::new(cmd.spawn().unwrap()).unwrap();
        let pid = child.group;
        wait_for(&ready).await;
        let helper: u32 = std::fs::read_to_string(&ready).unwrap().parse().unwrap();
        timeout(Duration::from_secs(3), child.terminate(Duration::from_millis(200)))
            .await
            .unwrap()
            .unwrap();
        assert!(reaped.exists(), "SIGTERM was not delivered to descendant");
        assert_eq!(child.leader.wait().await.unwrap().code(), None);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(!std::path::Path::new(&format!("/proc/{helper}")).exists());
    }
}
