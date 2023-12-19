use crate::{
    error::{
        AndaxError as AErr,
        TbErr::{self, Arb, Report, Rhai},
    },
    fns as f,
};
use directories::BaseDirs;
use lazy_static::lazy_static;
use regex::Regex;
use rhai::{
    module_resolvers::ModuleResolversCollection,
    packages::Package,
    plugin::{exported_module, Dynamic, EvalAltResult, Position},
    Engine, EvalAltResult as RhaiE, NativeCallContext as Ctx, Scope,
};
use std::{collections::BTreeMap, io::BufRead, path::Path};
use tracing::{debug, error, instrument, trace, warn};

pub fn rf<T>(ctx: &Ctx, res: color_eyre::Result<T>) -> Result<T, Box<RhaiE>>
where
    T: rhai::Variant + Clone,
{
    res.map_err(|err| {
        Box::new(RhaiE::ErrorRuntime(
            Dynamic::from(AErr::RustReport(
                ctx.fn_name().into(),
                ctx.source().unwrap_or("").into(),
                std::rc::Rc::from(err),
            )),
            ctx.position(),
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
            let mut local_resolv = rhai::module_resolvers::FileModuleResolver::new_with_path(
                user_libs.to_str().unwrap(),
            );
            local_resolv.enable_cache(true);
            resolv.push(local_resolv);
        }
    }

    let std_resolv = rhai::module_resolvers::FileModuleResolver::new();
    resolv.push(std_resolv);

    resolv
}
fn gen_en() -> (Engine, Scope<'static>) {
    let mut sc = Scope::new();
    sc.push("USER_AGENT", f::tsunagu::USER_AGENT);
    sc.push("IS_WIN32", cfg!(windows));
    sc.push("ANDAX_VER", env!("CARGO_PKG_VERSION"));
    let mut en = Engine::new();

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
fn _gpos(p: Position) -> Option<(usize, usize)> {
    p.line().map(|l| (l, p.position().unwrap_or(0)))
}
lazy_static! {
    static ref WORD_REGEX: Regex = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap();
}

#[instrument(name = "traceback")]
pub fn _tb(proj: &str, scr: &Path, nanitozo: TbErr, pos: Position, rhai_fn: &str, fn_src: &str) {
    if let Some((line, col)) = _gpos(pos) {
        // Print code
        let f = std::fs::File::open(scr);
        let scr = scr.display();
        macro_rules! die {
            ($var:expr, $msg:expr) => {{
                if let Err(e) = $var {
                    error!($msg, e);
                    return error!("{proj}: {scr} (no position data)\n{nanitozo}");
                }
                $var.unwrap()
            }};
        }
        let f = die!(f, "{proj}: Cannot open `{scr}`: {}");
        for (n, sl) in std::io::BufReader::new(f).lines().enumerate() {
            if n != line - 1 {
                continue;
            }
            // replace tabs to avoid wrong position when print
            let sl = die!(sl, "{proj}: Cannot read line: {}").replace('\t', " ");
            let m = WORD_REGEX.find_at(sl.as_str(), col - 1).map_or(1, |x| {
                let r = x.range();
                if r.start == col - 1 {
                    r.len()
                } else {
                    1
                }
            });
            let ln = line.to_string().len();
            let lns = " ".repeat(ln);
            let l = "─".repeat(ln);
            let r = "─".repeat(sl.len() + 2);
            let mut code = format!(
                "─{l}─┬{r}\n {lns} │ {scr}:{line}:{col}\n─{l}─┼{r}\n {line} │ {sl}\n {lns} │ {}{}",
                " ".repeat(col - 1),
                "🭶".repeat(m)
            );
            if !rhai_fn.is_empty() {
                code += &format!("\n {lns} └─═ When invoking: {rhai_fn}()");
            }
            if !fn_src.is_empty() {
                code += &format!("\n {lns} └─═ Function source: {fn_src}");
            }
            code += &format!("\n {lns} └─═ {nanitozo}");
            code += &hint(&sl, &lns, &nanitozo, rhai_fn).unwrap_or_default();
            let c = code.matches('└').count();
            if c > 0 {
                code = code.replacen('└', "├", c - 1);
            }
            return error!("Script Exception —— {proj}\n{code}");
        }
        error!("{proj}: Non-existence exception at {scr}:{line}:{col}");
    }
    error!("{proj}: {scr:?} (no position data)\n{nanitozo}");
}

/// Handles an exception thrown while executing an AndaX script.
pub fn errhdl(name: &str, scr: &Path, err: EvalAltResult) {
    trace!("{name}: Generating traceback");
    if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
        match run_err.clone().try_cast::<AErr>() {
            Some(AErr::RustReport(rhai_fn, fn_src, others)) => {
                return _tb(name, scr, Report(others), pos, rhai_fn.as_str(), fn_src.as_str());
            }
            Some(AErr::RustError(rhai_fn, fn_src, others)) => {
                return _tb(name, scr, Arb(others), pos, rhai_fn.as_str(), fn_src.as_str());
            }
            Some(AErr::Exit(b)) => {
                if b {
                    warn!("世界を壊している。\n{}", crate::error::EARTH);
                    error!("生存係為咗喵？打程式幾好呀。仲喵要咁憤世嫉俗喎。還掂おこちゃま戦争係政治家嘅事……");
                    trace!("あなたは世界の終わりにずんだを食べるのだ");
                }
                return debug!("Exit from rhai at: {pos}");
            }
            None => {}
        }
    }
    trace!("Rhai moment: {err:#?}");
    let pos = err.position();
    _tb(name, scr, Rhai(err), pos, "", "");
}

/// Executes an AndaX script.
pub fn run<'a>(
    name: &'a str,
    scr: &'a Path,
    labels: BTreeMap<String, String>,
    f: impl FnOnce(&mut Scope<'a>),
) -> Option<Scope<'a>> {
    let (en, mut sc) = gen_en();
    f(&mut sc);
    let mut lbls = rhai::Map::new();
    for (k, v) in labels {
        lbls.insert(k.into(), v.into());
    }
    sc.push("labels", lbls);
    exec(name, scr, sc, en)
}

#[instrument(skip(sc, en))]
fn exec<'a>(name: &'a str, scr: &'a Path, mut sc: Scope<'a>, en: Engine) -> Option<Scope<'a>> {
    debug!("Running {name}");
    match en.run_file_with_scope(&mut sc, scr.to_path_buf()) {
        Ok(()) => Some(sc),
        Err(err) => {
            errhdl(name, scr, *err);
            None
        }
    }
}

macro_rules! gen_h {
    // nyeshu
    ($lns:ident) => {
        macro_rules! h {
            ($s:expr) => {
                let left = " ".repeat(7 + $lns.len());
                let mut s = String::new();
                let mut first = true;
                for l in $s.lines() {
                    let l = l.trim();
                    if first {
                        s = format!("\n {} └─═ Hint: {l}", $lns);
                        first = false;
                        continue;
                    }
                    s += &format!("\n{left}...: {l}");
                }
                return Some(s);
            };
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
fn hint_ear(sl: &str, lns: &str, ear: &EvalAltResult, rhai_fn: &str) -> Option<String> {
    use EvalAltResult::{ErrorMismatchOutputType, ErrorRuntime};
    trace!(?rhai_fn, "Hinting for EvalAltResult");
    gen_h!(lns);
    match ear {
        ErrorRuntime(d, _) => {
            if d.is_string() {
                let s = d.clone().into_string().expect("sting.");
                if s == "env(`GITHUB_TOKEN`) not present" {
                    h!(
                        r"gh() requires the environment variable `GITHUB_TOKEN` to be set as a Github token so as to avoid rate-limits:
                        https://docs.github.com/en/rest/overview/resources-in-the-rest-api#rate-limiting
                        To create a Github token, see:
                        https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token"
                    );
                }
            }
        }
        ErrorMismatchOutputType(req, actual, _) => {
            if sl.contains("json(") && req == "map" && actual == "array" {
                h!("If the json root is an array `[]`, use json_arr() instead.");
            }
        }
        _ => {}
    }
    trace!("No hints");
    None
}
