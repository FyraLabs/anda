use crate::error::AndaxRes;
use rhai::{plugin::*, EvalAltResult};
use std::io::Write;
use std::process::Command;
use tracing::{debug, instrument};
macro_rules! _sh_out {
    ($ctx:expr, $o:expr) => {
        Ok((
            $o.status.code().ok_or::<Box<EvalAltResult>>("No exit code".into())?,
            String::from_utf8($o.stdout).ehdl($ctx)?,
            String::from_utf8($o.stderr).ehdl($ctx)?,
        ))
    };
}
macro_rules! _cmd {
    ($cmd:expr) => {{
        let mut x;
        if cfg!(target_os = "windows") {
            x = Command::new("cmd");
            x.args(["/C", $cmd]);
        } else {
            x = Command::new("sh");
            x.args(["-c", $cmd]);
        }
        x
    }};
}

type T = Result<(i32, String, String), Box<EvalAltResult>>;

/// for andax, shell():
/// ```
/// sh("echo hai");
/// sh(["echo", "hai"]);
/// sh(["rm", "-rf", "/path/with/some space"]);
/// sh("ls -al", "/current/working/directory");
/// sh(["grep", "andaman", "file"], "/working/dir");
/// ```
/// Returns (rc, stdout, stderr)
/// We will let rhai handle all the nasty things.
#[export_module]
pub mod ar {

    /// run a command using `cmd` on Windows and `sh` on other systems
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub(crate) fn shell(ctx: NativeCallContext, cmd: &str) -> T {
        debug!("Running in shell");
        _sh_out!(&ctx, _cmd!(cmd).output().ehdl(&ctx)?)
    }
    /// run a command using `cmd` on Windows and `sh` on other systems in working dir
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub(crate) fn shell_cwd(ctx: NativeCallContext, cmd: &str, cwd: &str) -> T {
        debug!("Running in shell");
        _sh_out!(&ctx, _cmd!(cmd).current_dir(cwd).output().ehdl(&ctx)?)
    }
    /// run an executable
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub(crate) fn sh(ctx: NativeCallContext, cmd: Vec<&str>) -> T {
        debug!("Running executable");
        _sh_out!(&ctx, Command::new(cmd[0]).args(&cmd[1..]).output().ehdl(&ctx)?)
    }
    /// run an executable in working directory
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub(crate) fn sh_cwd(ctx: NativeCallContext, cmd: Vec<&str>, cwd: &str) -> T {
        debug!("Running executable");
        _sh_out!(&ctx, Command::new(cmd[0]).args(&cmd[1..]).current_dir(cwd).output().ehdl(&ctx)?)
    }
    /// list files and folders in directory
    /// ## Example
    /// ```rhai
    /// for x in ls("/") {
    ///     if x == "bin" {
    ///         print("I found the `/bin` folder!");
    ///     }
    /// }
    /// ```
    #[rhai_fn(return_raw, global)]
    pub(crate) fn ls(
        ctx: NativeCallContext,
        dir: Option<&str>,
    ) -> Result<Vec<String>, Box<EvalAltResult>> {
        let mut res = vec![];
        for dir in std::fs::read_dir(dir.unwrap_or(".")).ehdl(&ctx)? {
            res.push(dir.ehdl(&ctx)?.path().to_string_lossy().to_string());
        }
        Ok(res)
    }
    /// write data to file
    ///
    /// ## Example
    /// ```rhai
    /// let foo = "bar";
    /// foo.write("bar.txt")
    /// ```
    #[rhai_fn(name = "write", return_raw, global)]
    pub(crate) fn write(
        ctx: NativeCallContext,
        data: &str,
        file: &str,
    ) -> Result<(), Box<EvalAltResult>> {
        let mut f = std::fs::File::create(file).ehdl(&ctx)?;
        f.write_all(data.as_bytes()).ehdl(&ctx)?;
        Ok(())
    }
}
