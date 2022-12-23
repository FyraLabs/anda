use std::process::Command;
use rhai::EvalAltResult;
use crate::update::tsunagu::ehdl;

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
pub(crate) fn shell(cmd: &str) -> T {
    _sh_out!(ehdl(_cmd!(cmd).output())?)
}
pub(crate) fn shell_cwd(cmd: &str, cwd: &str) -> T {
    _sh_out!(ehdl(_cmd!(cmd).current_dir(cwd).output())?)
}
pub(crate) fn sh(cmd: Vec<&str>) -> T {
    _sh_out!(ehdl(Command::new(cmd[0]).args(&cmd[1..]).output())?)
}
pub(crate) fn sh_cwd(cmd: Vec<&str>, cwd: &str) -> T {
    _sh_out!(ehdl(Command::new(cmd[0]).args(&cmd[1..]).current_dir(cwd).output())?)
}
