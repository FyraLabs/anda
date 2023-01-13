use crate::error::AndaxRes;
use rhai::{plugin::*, EvalAltResult};
use std::process::Command;

macro_rules! _sh_out {
    ($ctx:expr, $o:expr) => {
        Ok((
            $o.status
                .code()
                .ok_or::<Box<EvalAltResult>>("No exit code".into())?,
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
type Ctx<'a> = NativeCallContext<'a>;

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
#[allow(dead_code)]
pub mod anda_rhai {
    use std::{
        fs::File,
        io::{BufRead, BufReader, Lines},
    };
    /// run a command using `cmd` on Windows and `sh` on other systems
    #[rhai_fn(return_raw, name = "sh")]
    fn shell(ctx: Ctx, cmd: &str) -> T {
        _sh_out!(&ctx, _cmd!(cmd).output().ehdl(&ctx)?)
    }
    /// run a command using `cmd` on Windows and `sh` on other systems in working dir
    #[rhai_fn(return_raw, name = "sh")]
    fn shell_cwd(ctx: Ctx, cmd: &str, cwd: &str) -> T {
        _sh_out!(&ctx, _cmd!(cmd).current_dir(cwd).output().ehdl(&ctx)?)
    }
    /// run an executable
    #[rhai_fn(return_raw, name = "sh")]
    fn sh(ctx: Ctx, cmd: Vec<&str>) -> T {
        _sh_out!(
            &ctx,
            Command::new(cmd[0]).args(&cmd[1..]).output().ehdl(&ctx)?
        )
    }
    /// run an executable in working directory
    #[rhai_fn(return_raw, name = "sh")]
    fn sh_cwd(ctx: Ctx, cmd: Vec<&str>, cwd: &str) -> T {
        _sh_out!(
            &ctx,
            Command::new(cmd[0])
                .args(&cmd[1..])
                .current_dir(cwd)
                .output()
                .ehdl(&ctx)?
        )
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
    #[rhai_fn(return_raw)]
    fn ls(ctx: Ctx, dir: Option<&str>) -> Result<Vec<String>, Box<EvalAltResult>> {
        let mut res = vec![];
        for dir in std::fs::read_dir(dir.unwrap_or(".")).ehdl(&ctx)? {
            res.push(dir.ehdl(&ctx)?.path().to_string_lossy().to_string());
        }
        Ok(res)
    }
    /// iterator for lines in a file
    /// ## Example
    /// ```rhai
    /// for line in flines("/path/to/file.txt") {
    ///     print(line);
    /// }
    /// ```
    #[rhai_fn(return_raw)]
    fn flines(ctx: Ctx, path: &str) -> Result<Lines<BufReader<File>>, Box<EvalAltResult>> {
        Ok(BufReader::new(File::open(path).ehdl(&ctx)?).lines())
    }
}
