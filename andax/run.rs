use crate::{
    error::{
        AndaxError as AErr,
        TbErr::{self, Arb, Report, Rhai},
    },
    fns as f,
};
use directories::BaseDirs;
use regex::Regex;
use rhai::{
    module_resolvers::ModuleResolversCollection,
    packages::Package,
    plugin::{exported_module, Dynamic, EvalAltResult, Position},
    Engine, EvalAltResult as RhaiE, NativeCallContext as Ctx, Scope,
};
use std::{fmt::Write, sync::LazyLock};
use std::{io::BufRead, path::Path};
use tracing::{debug, error, instrument, trace, warn};

/// # Errors
/// Turns a color_eyre Result into the Rhai Result
pub fn rf<T>(ctx: &Ctx, res: color_eyre::Result<T>) -> Result<T, Box<RhaiE>>
where
    T: rhai::Variant + Clone,
{
    res.map_err(|err| {
        Box::new(RhaiE::ErrorRuntime(
            Dynamic::from(AErr::RustReport(
                ctx.fn_name().into(),
                ctx.fn_source().unwrap_or("").into(),
                std::rc::Rc::from(err),
            )),
            ctx.call_position(),
        ))
    })
}

fn module_resolver() -> ModuleResolversCollection {
    let mut resolv = ModuleResolversCollection::default();

    let mut base_modules = rhai::module_resolvers::StaticModuleResolver::new();

    // todo: rewrite all these stuff to make use of the new resolver

    base_modules.insert("io", exported_module!(f::io::ar));
    base_modules.insert("tsunagu", exported_module!(f::tsunagu::ar));
    base_modules.insert("kokoro", exported_module!(f::kokoro::ar));
    base_modules.insert("tenshi", exported_module!(f::tenshi::ar));
    base_modules.insert("anda::rpmbuild", exported_module!(f::build::ar));
    base_modules.insert("anda::cfg", exported_module!(f::cfg::ar));

    resolv.push(base_modules);

    let sys_modules = vec![
        "/usr/lib/anda",
        "/usr/local/lib/anda",
        // "/lib/anda",
        "/usr/lib64/anda",
        "/usr/local/lib64/anda",
        // "/lib64/anda",
    ];

    for path in sys_modules {
        let mut sys_resolv = rhai::module_resolvers::FileModuleResolver::new_with_path(path);
        sys_resolv.enable_cache(true);
        resolv.push(sys_resolv);
    }

    if let Some(base_dirs) = BaseDirs::new() {
        let user_libs = base_dirs.home_dir().join(".local/lib/anda");
        if user_libs.is_dir() {
            let mut local_resolv =
                rhai::module_resolvers::FileModuleResolver::new_with_path(user_libs);
            local_resolv.enable_cache(true);
            resolv.push(local_resolv);
        }
    }

    let std_resolv = rhai::module_resolvers::FileModuleResolver::new();
    resolv.push(std_resolv);

    resolv
}
pub fn gen_en() -> (Engine, Scope<'static>) {
    let mut sc = Scope::new();
    sc.push("USER_AGENT", f::tsunagu::USER_AGENT);
    sc.push("IS_WIN32", cfg!(windows));
    sc.push("ANDAX_VER", env!("CARGO_PKG_VERSION"));
    let mut en = Engine::new();
    en.set_optimization_level(rhai::OptimizationLevel::Simple);

    let resolv = module_resolver();
    en.set_module_resolver(resolv)
        .register_global_module(exported_module!(f::io::ar).into())
        .register_global_module(exported_module!(f::tsunagu::ar).into())
        .register_global_module(exported_module!(f::kokoro::ar).into())
        .register_global_module(exported_module!(f::tenshi::ar).into())
        .register_static_module("anda::rpmbuild", exported_module!(f::build::ar).into())
        .register_static_module("anda::cfg", exported_module!(f::cfg::ar).into())
        .build_type::<f::tsunagu::Req>()
        .build_type::<f::rpm::RPMSpec>();
    rhai_fs::FilesystemPackage::new().register_into_engine(&mut en);
    rhai_url::UrlPackage::new().register_into_engine(&mut en);
    trace!(?en, "Engine created");
    (en, sc)
}

#[inline]
fn gpos(p: Position) -> Option<(usize, usize)> {
    Some((p.line()?, p.position().unwrap_or(0)))
}
static WORD_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[A-Za-z_][A-Za-z0-9_]*").unwrap());

// proj: project name, scr: script path, nntz (nanitozo): just give me the error
// pos: error position, rhai_fn: function that caused the issue, fn_src: idk…
#[allow(clippy::arithmetic_side_effects)]
#[instrument]
pub fn traceback(proj: &str, scr: &Path, nntz: &TbErr, pos: Position, rhai_fn: &str, fn_src: &str) {
    trace!("Formulating traceback");
    let Some((line, col)) = gpos(pos) else {
        return error!("{proj}: {scr:?} (no position data)\n{nntz}");
    };
    let fi = std::fs::File::open(scr);
    let scr = scr.display();
    macro_rules! die {
        ($var:expr, $msg:expr) => {{
            if let Err(e) = $var {
                error!($msg, e);
                return error!("{proj}: {scr} (no position data)\n{nntz}");
            }
            $var.unwrap()
        }};
    }
    let fi = die!(fi, "{proj}: Cannot open `{scr}`: {}");
    let Some(sl) = std::io::BufReader::new(fi).lines().nth(line - 1) else {
        error!("{proj}: Non-existence exception at {scr}:{line}:{col}");
        return error!("{proj}: {scr} (no position data)\n{nntz}");
    };
    // replace tabs to avoid wrong position when print
    let sl = die!(sl, "{proj}: Cannot read line: {}").replace('\t', " ");
    let ma = WORD_REGEX.find_at(sl.as_str(), col - 1).map_or(1, |x| {
        let ri = x.range();
        if ri.start == col - 1 {
            ri.len()
        } else {
            1
        }
    }); // number of underline chars
    let ln = line.to_string().len(); // length of the string of the line number
    let lns = " ".repeat(ln); // spaces for padding the left hand side line number place
    let li = "─".repeat(ln); // padding for the top of line number display
    let ri = "─".repeat(sl.len() + 2); // right hand side padding
    let mut code = format!(
        "─{li}─┬{ri}\n {lns} │ {scr}:{line}:{col}\n─{li}─┼{ri}\n {line} │ {sl}\n {lns} │ {}{}",
        " ".repeat(col - 1), // padding at left of underline
        "🭶".repeat(ma)       // underline the word
    );
    if !rhai_fn.is_empty() {
        _ = write!(code, "\n {lns} └─═ When invoking: {rhai_fn}()");
    }
    if !fn_src.is_empty() {
        _ = write!(code, "\n {lns} └─═ Function source: {fn_src}");
    }
    _ = write!(code, "\n {lns} └─═ {nntz}");
    code += &hint(&sl, &lns, nntz, rhai_fn).unwrap_or_default();
    // slow but works!
    let cm = code.matches('└').count();
    if cm > 0 {
        code = code.replacen('└', "├", cm - 1);
    }
    error!("Script Exception —— {proj}\n{code}");
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Terminated,
    Error(ScriptError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub message: String,
    pub function: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl ScriptError {
    fn new(nntz: &TbErr, pos: Position, rhai_fn: &str) -> Self {
        let message = match nntz {
            Report(report) => format!("{report:#}"),
            Arb(err) => err.to_string(),
            Rhai(EvalAltResult::ErrorRuntime(value, _)) => value.to_string(),
            Rhai(err) => err.to_string(),
        };
        Self {
            message,
            function: (!rhai_fn.is_empty()).then(|| rhai_fn.to_owned()),
            line: pos.line(),
            column: pos.position(),
        }
    }
}

fn report_error(
    name: &str,
    scr: &Path,
    nntz: &TbErr,
    pos: Position,
    rhai_fn: &str,
    fn_src: &str,
) -> Failure {
    traceback(name, scr, nntz, pos, rhai_fn, fn_src);
    Failure::Error(ScriptError::new(nntz, pos, rhai_fn))
}

/// Handles an exception thrown while executing an AndaX script.
pub fn errhdl(name: &str, scr: &Path, err: EvalAltResult) -> Failure {
    trace!("{name}: Generating traceback");
    if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
        match run_err.clone().try_cast::<AErr>() {
            Some(AErr::RustReport(rhai_fn, fn_src, others)) => {
                return report_error(name, scr, &Report(others), pos, &rhai_fn, &fn_src);
            }
            Some(AErr::RustError(rhai_fn, fn_src, others)) => {
                return report_error(name, scr, &Arb(others), pos, &rhai_fn, &fn_src);
            }
            Some(AErr::Exit(b)) => {
                if b {
                    warn!("世界を壊している。\n{}", crate::error::EARTH);
                    error!("生存係為咗喵？打程式幾好呀。仲喵要咁憤世嫉俗喎。還掂おこちゃま戦争係政治家嘅事……");
                    trace!("あなたは世界の終わりにずんだを食べるのだ");
                }
                debug!("Exit from rhai at: {pos}");
                return Failure::Terminated;
            }
            None => {}
        }
    }
    trace!("Rhai moment: {err:#?}");
    let mut err = err;
    let pos = err.take_position();
    report_error(name, scr, &Rhai(err), pos, "", "")
}

/// Executes an AndaX script.
pub fn run<
    'a,
    F: FnOnce(&mut Scope<'static>),
    K: Into<rhai::Identifier>,
    V: Into<rhai::Dynamic>,
    L: Iterator<Item = (K, V)>,
>(
    name: &'a str,
    scr: &'a Path,
    labels: L,
    f: F,
) -> Option<Scope<'a>> {
    try_run(name, scr, labels, f).ok()
}

/// # Errors
/// The script threw an exception or terminated itself. The traceback is logged either way.
pub fn try_run<
    'a,
    F: FnOnce(&mut Scope<'static>),
    K: Into<rhai::Identifier>,
    V: Into<Dynamic>,
    L: Iterator<Item = (K, V)>,
>(
    name: &'a str,
    scr: &'a Path,
    labels: L,
    f: F,
) -> Result<Scope<'a>, Failure> {
    let (en, mut sc) = gen_en();
    f(&mut sc);
    let lbls: rhai::Map = labels.map(|(k, v)| (k.into(), v.into())).collect();
    sc.push("labels", lbls);
    sc.push("__script_path", format!("{}", scr.display()));
    sc.push("get_sc", {
        let sc = sc.clone();
        move |k: &str| sc.get(k).cloned().unwrap_or(Dynamic::UNIT)
    });
    exec(name, scr, sc, &en)
}

#[instrument(skip(sc, en))]
fn exec<'a>(
    name: &'a str,
    scr: &'a Path,
    mut sc: Scope<'a>,
    en: &Engine,
) -> Result<Scope<'a>, Failure> {
    debug!("Running {name}");
    en.run_file_with_scope(&mut sc, scr.to_path_buf()).map_err(|err| errhdl(name, scr, *err))?;
    Ok(sc)
}

macro_rules! gen_h {
    // nyeshu
    ($lns:ident) => {
        macro_rules! h {
            ($s:expr) => {{
                #[allow(clippy::arithmetic_side_effects)]
                let left = " ".repeat(7 + $lns.len());
                let mut s = String::new();
                let mut first = true;
                for l in $s.lines() {
                    if first {
                        s = format!("\n {} └─═ Hint: {l}", $lns);
                        first = false;
                        continue;
                    }
                    _ = write!(s, "\n{left}...: {l}");
                }
                return Some(s);
            }};
        }
    };
}

#[instrument(skip(sl, lns, nanitozo, rhai_fn))]
fn hint(sl: &str, lns: &str, nanitozo: &TbErr, rhai_fn: &str) -> Option<String> {
    trace!("Matching hints");
    gen_h!(lns);
    match nanitozo {
        Arb(err) => {
            if let Some(err) = (**err).downcast_ref::<EvalAltResult>() {
                return hint_ear(sl, lns, err, rhai_fn);
            }
            let s = format!("{err}");
            if rhai_fn == "gh"
                && s.starts_with("https://api.github.com/repos/")
                && s.ends_with("/releases/latest: status code 404")
            {
                h!("Check if the repo is valid. Only releases are supported; use gh_tag() for tags.");
            }
            if rhai_fn.starts_with("gh") && s.ends_with(": status code 403") {
                h!("Maybe you have reached the ratelimit: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api");
            }
            None
        }
        Report(report) => {
            if let Some(err) = report.source() {
                if let Some(err) = err.downcast_ref::<EvalAltResult>() {
                    return hint_ear(sl, lns, err, rhai_fn);
                }
            }
            None
        }
        Rhai(err) => hint_ear(sl, lns, err, rhai_fn),
    }
}
/// # Panics
/// This function will never panic.
fn hint_ear(sl: &str, lns: &str, ear: &EvalAltResult, rhai_fn: &str) -> Option<String> {
    use rhai::ParseErrorType::MissingToken;
    use EvalAltResult::{ErrorMismatchOutputType, ErrorParsing, ErrorRuntime};
    trace!(?rhai_fn, "Hinting for EvalAltResult");
    gen_h!(lns);
    match ear {
        ErrorRuntime(d, _) => {
            if let Some(s) = d.read_lock::<String>() {
                if s.as_str() == "env(`GITHUB_TOKEN`) not present" {
                    h!(include_str!("hints/gh_token_not_present.txt"))
                }
            }
        }
        ErrorMismatchOutputType(req, actual, _) => {
            if sl.contains("json(") && req == "map" && actual == "array" {
                h!("If the json root is an array `[]`, use json_arr() instead.");
            }
        }
        ErrorParsing(MissingToken(token, _), _) if token == ";" => {
            h!("You most likely forgot to add a semicolon to the end of the last line.");
        }
        _ => {}
    }
    trace!("No hints");
    None
}

#[cfg(test)]
mod tests {
    use super::{run, try_run, Failure, ScriptError};
    use crate::fns::rpm::RPMSpec;
    use std::fs;

    fn failure_of(source: &str) -> Failure {
        let dir = tempfile::tempdir().expect("create temporary directory");
        let script = dir.path().join("update.rhai");
        fs::write(&script, source).expect("write script");
        try_run("test", &script, std::iter::empty::<(String, String)>(), |_| {})
            .expect_err("script should fail")
    }

    #[test]
    fn reports_thrown_errors_without_position_noise() {
        assert_eq!(
            failure_of("let x = 1;\nthrow \"upstream moved\";\n"),
            Failure::Error(ScriptError {
                message: "upstream moved".to_owned(),
                function: None,
                line: Some(2),
                column: Some(1),
            })
        );
    }

    #[test]
    fn reports_the_failing_function() {
        let Failure::Error(err) = failure_of(r#"let v = json_arr("{");"#) else {
            panic!("expected a script error");
        };
        assert_eq!(err.function.as_deref(), Some("json_arr"));
        assert_eq!((err.line, err.column), (Some(1), Some(9)));
    }

    #[test]
    fn reports_terminate_as_terminated() {
        assert_eq!(failure_of("terminate();"), Failure::Terminated);
    }

    #[test]
    fn exposes_rpm_spec_to_scripts() {
        let dir = tempfile::tempdir().expect("create temporary directory");
        let spec = dir.path().join("test.spec");
        let script = dir.path().join("pre.rhai");
        fs::write(&spec, "Name: test\nVersion: 1\nRelease: 1\n%global bootstrap 1\n")
            .expect("write spec");
        fs::write(&script, r#"rpm.global("bootstrap", "0");"#).expect("write script");

        let rpm = RPMSpec::new("test".to_owned(), &script, &spec);
        let scope = run("test", &script, std::iter::empty::<(String, String)>(), |scope| {
            scope.push("rpm", rpm);
        })
        .expect("script should run");
        let rpm: RPMSpec = scope.get_value("rpm").expect("rpm should remain in scope");

        assert!(rpm.f.contains("%global bootstrap 0"));
    }
}
