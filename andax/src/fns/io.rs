use crate::error::AndaxRes;
use rhai::{
    plugin::{
        export_module, mem, Dynamic, FnAccess, FnNamespace, ImmutableString, Module,
        NativeCallContext, PluginFunction, RhaiResult, TypeId,
    },
    EvalAltResult,
};
use std::io::Write;
use std::process::Command;
use tracing::{debug, instrument};

macro_rules! _sh_out {
    ($ctx:expr, $o:expr) => {
        Ok((
            _sh_out!($o)?,
            String::from_utf8($o.stdout).ehdl($ctx)?,
            String::from_utf8($o.stderr).ehdl($ctx)?,
        ))
    };
    ($o:expr) => {{
        $o.status.code().ok_or::<Box<EvalAltResult>>("No exit code".into())
    }};
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

macro_rules! _stream_cmd {
    ($cmd:expr) => {{
        _cmd!($cmd).stdout(Stdio::inherit()).stderr(Stdio::inherit())
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
    use core::str::FromStr;
    use std::process::Stdio;

    macro_rules! die {
        ($id:literal, $expect:expr, $found:expr) => {{
            let mut e = rhai::Map::new();
            let mut inner = std::collections::BTreeMap::new();
            e.insert("outcome".into(), rhai::Dynamic::from_str("fatal").unwrap());
            inner.insert("kind".into(), rhai::Dynamic::from_str($id).unwrap());
            inner.insert("expect".into(), rhai::Dynamic::from_str($expect).unwrap());
            inner.insert("found".into(), rhai::Dynamic::from_str($found).unwrap());
            e.insert("ctx".into(), rhai::Dynamic::from_map(inner));
            e
        }};
    }

    /// get the return code from the return value of `sh()`
    #[rhai_fn(global)]
    pub fn sh_rc(o: (i32, String, String)) -> i32 {
        o.0
    }
    /// get stdout from the return value of `sh()`
    #[rhai_fn(global)]
    pub fn sh_stdout(o: (i32, String, String)) -> String {
        o.1
    }
    /// get stderr from the return value of `sh()`
    #[rhai_fn(global)]
    pub fn sh_stderr(o: (i32, String, String)) -> String {
        o.2
    }

    fn _parse_io_opt(opt: Option<&mut rhai::Dynamic>) -> Result<impl Into<Stdio>, rhai::Map> {
        let Some(s) = opt else { return Ok(Stdio::inherit()) };
        let s = match std::mem::take(s).into_string() {
            Ok(s) => s,
            Err(e) => return Err(die!("bad_stdio_type", r#""inherit" | "null" | "piped""#, e)),
        };
        Ok(match &*s {
            "inherit" => Stdio::inherit(),
            "null" => Stdio::null(),
            "piped" => Stdio::piped(),
            _ => return Err(die!("bad_stdio_opt", r#""inherit" | "null" | "piped""#, &s)),
        })
    }

    /// Run a command
    #[instrument]
    #[rhai_fn(global, name = "sh")]
    pub fn exec_cmd(command: Dynamic, mut opts: rhai::Map) -> rhai::Map {
        let mut cmd: Command;
        if command.is_string() {
            cmd = Command::new("sh");
            cmd.arg("-c").arg(command.into_string().unwrap())
        } else {
            let res = command.into_typed_array();
            let Ok(arr) = res else {
                return die!("bad_param_type", "String | Vec<String>", res.unwrap_err());
            };
            let [exec, args @ ..]: &[&str] = &arr[..] else {
                return die!("empty_cmd_arr", "cmd.len() >= 1", "cmd.len() == 0");
            };
            cmd = Command::new(exec);
            cmd.args(args)
        };

        cmd.stdout(match _parse_io_opt(opts.get_mut("stdout")) {
            Ok(io) => io,
            Err(e) => return e,
        });
        cmd.stderr(match _parse_io_opt(opts.get_mut("stderr")) {
            Ok(io) => io,
            Err(e) => return e,
        });

        if let Some(cwd) = opts.get_mut("cwd") {
            match std::mem::take(cwd).into_string() {
                Ok(cwd) => _ = cmd.current_dir(cwd),
                Err(e) => return die!("bad_cwd_type", "String", e),
            }
        }

        let out = match cmd.output() {
            Ok(x) => x,
            Err(err) => {
                let mut e = rhai::Map::new();
                let mut inner = rhai::Map::new();
                e.insert("outcome".into(), rhai::Dynamic::from_str("failure").unwrap());
                inner.insert("error".into(), rhai::Dynamic::from_str(&err.to_string()).unwrap());
                e.insert("ctx".into(), rhai::Dynamic::from_map(inner));
                return e;
            }
        };

        let mut ret = rhai::Map::new();
        let mut inner = rhai::Map::new();
        ret.insert("outcome".into(), rhai::Dynamic::from_str("success").unwrap());
        inner.insert(
            "stdout".into(),
            rhai::Dynamic::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap(),
        );
        inner.insert(
            "stderr".into(),
            rhai::Dynamic::from_str(&String::from_utf8_lossy(&out.stderr)).unwrap(),
        );
        inner.insert(
            "rc".into(),
            rhai::Dynamic::from_int(i64::from(out.status.code().unwrap_or(0))),
        );
        ret.insert("ctx".into(), rhai::Dynamic::from_map(inner));
        ret
    }

    /// run a command using `cmd` on Windows and `sh` on other systems
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub fn shell(ctx: NativeCallContext, cmd: &str) -> T {
        debug!("Running in shell");
        _sh_out!(&ctx, _cmd!(cmd).output().ehdl(&ctx)?)
    }
    /// run a command using `cmd` on Windows and `sh` on other systems in working dir
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub fn shell_cwd(ctx: NativeCallContext, cmd: &str, cwd: &str) -> T {
        debug!("Running in shell");
        _sh_out!(&ctx, _cmd!(cmd).current_dir(cwd).output().ehdl(&ctx)?)
    }
    /// run an executable
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub fn sh(ctx: NativeCallContext, cmd: Vec<&str>) -> T {
        debug!("Running executable");
        _sh_out!(&ctx, Command::new(cmd[0]).args(&cmd[1..]).output().ehdl(&ctx)?)
    }
    /// run an executable in working directory
    #[instrument(skip(ctx))]
    #[rhai_fn(return_raw, name = "sh", global)]
    pub fn sh_cwd(ctx: NativeCallContext, cmd: Vec<&str>, cwd: &str) -> T {
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
    pub fn ls(
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
    pub fn write(
        ctx: NativeCallContext,
        data: Dynamic,
        file: &str,
    ) -> Result<(), Box<EvalAltResult>> {
        let mut f = std::fs::File::create(file).ehdl(&ctx)?;
        let data = {
            if data.is_map() {
                // turn into JSON
                serde_json::to_string(&data).ehdl(&ctx)?
            } else {
                data.to_string()
            }
        };
        f.write_all(data.as_bytes()).ehdl(&ctx)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn shells() -> Result<(), Box<EvalAltResult>> {
        let (en, _) = crate::run::gen_en();
        en.run(
            r#"
            let a = sh("echo hai > test");
            let b = sh(["echo", "hai"]);
            let c = sh(["rm", "-rf", "test"]);
            let d = sh("ls -al", "/");
            let pwd = sh("pwd").sh_stdout();
            let e = sh(["grep", "hai", "test"], pwd);
            if a.sh_stderr() != "" {
                throw "error!?";
            }
            if b.sh_stdout() != "hai\n" {
                throw "bad echo?";
            }
            if c.sh_rc() != 0 {
                throw "cannot rm?";
            }
            if d.sh_stdout().is_empty() {
                throw "why is out empty?";
            }
        "#,
        )?;
        Ok(())
    }
}
