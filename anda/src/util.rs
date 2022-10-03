use async_trait::async_trait;
use log::{debug, info};
use std::ffi::{OsStr, OsString};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

#[async_trait]
pub trait CommandLog {
    async fn log(&mut self);
}
#[async_trait]
impl CommandLog for Command {
    async fn log(&mut self) {
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
        });

        tasks.push(stderr_handle);

        for task in tasks {
            task.await.unwrap();
        }

        output.wait().await.unwrap();
    }
}
