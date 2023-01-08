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

/// for andax, shell():
/// ```
/// shell("echo hai");
/// shell(["echo", "hai"]);
/// shell(["rm", "-rf", "/path/with/some space"]);
/// // cwd
/// shell("ls -al", "/current/working/directory");
/// shell(["grep", "andaman", "file"], "/working/dir");
/// ```
/// Returns (rc, stdout, stderr)
/// We will let rhai handle all the nasty things.
#[export_module]
pub mod anda_rhai {

    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn shell(ctx: NativeCallContext, cmd: &str) -> T {
        _sh_out!(&ctx, _cmd!(cmd).output().ehdl(&ctx)?)
    }
    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn shell_cwd(ctx: NativeCallContext, cmd: &str, cwd: &str) -> T {
        _sh_out!(&ctx, _cmd!(cmd).current_dir(cwd).output().ehdl(&ctx)?)
    }
    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn sh(ctx: NativeCallContext, cmd: Vec<&str>) -> T {
        _sh_out!(
            &ctx,
            Command::new(cmd[0]).args(&cmd[1..]).output().ehdl(&ctx)?
        )
    }
    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn sh_cwd(ctx: NativeCallContext, cmd: Vec<&str>, cwd: &str) -> T {
        _sh_out!(
            &ctx,
            Command::new(cmd[0])
                .args(&cmd[1..])
                .current_dir(cwd)
                .output()
                .ehdl(&ctx)?
        )
    }
}
