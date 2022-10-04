//! Utility functions and types

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, info};
use nix::sys::signal;
use nix::unistd::Pid;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

/// Command Logging
///
/// This trait implements custom logging for commands in a format of `{command} | {line}`
/// It also implements Ctrl-C handling for the command, and will send a SIGINT to the command
#[async_trait]
pub trait CommandLog {
    async fn log(&mut self) -> Result<()>;
}
#[async_trait]
impl CommandLog for Command {
    async fn log(&mut self) -> Result<()> {
        // let cmd_name = self;
        // make process name a constant string that we can reuse every time we call print_log
        let process = self
            .as_std()
            .get_program()
            .to_owned()
            .into_string()
            .unwrap();
        let args = self
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap())
            .collect::<Vec<&str>>()
            .join(" ");
        debug!("Running command: {process} {args}",);
        let c = self
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // copy self

        let mut output = c.spawn().unwrap();

        fn print_log(process: &str, output: String) {
            let formatter = format!("{}\t| {}", process, output);
            println!("{}", formatter);
        }

        // handles so we can run both at the same time

        let mut tasks = vec![];
        // stream stdout

        let stdout = output.stdout.take().unwrap();
        let stdout_reader = tokio::io::BufReader::new(stdout);
        let mut stdout_lines = stdout_reader.lines();

        // HACK: Rust ownership is very fun.
        let t = process.clone();
        let stdout_handle = tokio::spawn(async move {
            while let Some(line) = stdout_lines.next_line().await.unwrap() {
                print_log(&t, line);
            }
            Ok(())
        });

        tasks.push(stdout_handle);

        // stream stderr

        debug!("Streaming stderr");
        let stderr = output.stderr.take().unwrap();
        let stderr_reader = tokio::io::BufReader::new(stderr).lines();
        let mut stderr_lines = stderr_reader;

        debug!("stderr: {:?}", stderr_lines);

        let stderr_handle = tokio::spawn(async move {
            while let Some(line) = stderr_lines.next_line().await.unwrap() {
                print_log(&process, line);
            }
            Ok(())
        });

        // send sigint to child process when we ctrl-c
        tasks.push(stderr_handle);

        // sigint handle

        let sigint_handle = tokio::spawn(async move {
            // wait for ctrl-c or child process to finish

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received ctrl-c, sending sigint to child process");
                    signal::kill(Pid::from_raw(output.id().unwrap() as i32), signal::Signal::SIGINT).unwrap();

                    // exit program
                    eprintln!("Received ctrl-c, exiting");
                    // std::process::exit(127);
                    Err(anyhow::anyhow!("Received ctrl-c, exiting"))
                }
                w = output.wait() => {

                    // check exit status
                    let status = w.unwrap();
                    if status.success() {
                        info!("Command exited successfully");
                        Ok(())
                    } else {
                        info!("Command exited with status: {}", status);
                        Err(anyhow::anyhow!("Command exited with status: {}", status))
                    }
                    // info!("Child process finished");
                }
            }
        });

        tasks.push(sigint_handle);

        for task in tasks {
            task.await??;
        }

        Ok(())

        // output.wait().await.unwrap();
    }
}

// utility functions for spec templating

use git2::Repository;
/// Get the current commit id from the current git repository (cwd)
pub fn get_commit_id_cwd() -> Option<String> {
    let repo = Repository::open(".").ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let id = commit.id();
    Some(id.to_string())
}

/// Get the current commit id from a git repository
pub fn _get_commit_id(path: &str) -> Option<String> {
    let repo = Repository::open(path).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let id = commit.id();
    Some(id.to_string())
}


/// Formats the current time in the format of YYYYMMDD
use chrono::prelude::*;
pub fn get_date() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.format("%Y%m%d").to_string()
}