use crate::update::tsunagu::ehdl;
use rhai::plugin::*;
use rhai::EvalAltResult;
use std::process::Command;

macro_rules! _sh_out {
    ($o:expr) => {
        Ok((
            $o.status
                .code()
                .ok_or::<Box<EvalAltResult>>("No exit code".into())?,
            ehdl(String::from_utf8($o.stdout))?,
            ehdl(String::from_utf8($o.stderr))?,
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
    pub(crate) fn shell(cmd: &str) -> T {
        _sh_out!(ehdl(_cmd!(cmd).output())?)
    }
    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn shell_cwd(cmd: &str, cwd: &str) -> T {
        _sh_out!(ehdl(_cmd!(cmd).current_dir(cwd).output())?)
    }
    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn sh(cmd: Vec<&str>) -> T {
        _sh_out!(ehdl(Command::new(cmd[0]).args(&cmd[1..]).output())?)
    }
    #[rhai_fn(return_raw, name = "sh")]
    pub(crate) fn sh_cwd(cmd: Vec<&str>, cwd: &str) -> T {
        _sh_out!(ehdl(
            Command::new(cmd[0])
                .args(&cmd[1..])
                .current_dir(cwd)
                .output()
        )?)
    }
}
