use crate::{
    error::{AndaxError, AndaxRes, TbErr},
    io,
    update::{self, re, rpm, tsunagu},
};
use lazy_static::lazy_static;
use regex::Regex;
use rhai::{plugin::*, Engine, EvalAltResult, NativeCallContext as CallCtx, Scope};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    rc::Rc,
};
use tracing::{debug, error, instrument, trace, warn};

fn json(ctx: CallCtx, a: String) -> Result<rhai::Map, Box<EvalAltResult>> {
    ctx.engine().parse_json(a, true)
}
fn json_arr(ctx: CallCtx, a: String) -> Result<rhai::Array, Box<EvalAltResult>> {
    serde_json::from_str(&a).ehdl(&ctx)
}

pub(crate) fn rf<T>(ctx: CallCtx, res: color_eyre::Result<T>) -> Result<T, Box<EvalAltResult>>
where
    T: rhai::Variant + Clone,
{
    res.map_err(|err| {
        Box::new(EvalAltResult::ErrorRuntime(
            Dynamic::from(AndaxError::RustReport(
                ctx.fn_name().into(),
                ctx.source().unwrap_or("").into(),
                Rc::from(err),
            )),
            ctx.position(),
        ))
    })
}

fn gen_en() -> (Engine, Scope<'static>) {
    let mut sc = Scope::new();
    sc.push("USER_AGENT", tsunagu::USER_AGENT);
    sc.push("IS_WIN32", cfg!(windows));
    let mut en = Engine::new();
    en.register_fn("json", json)
        .register_fn("json_arr", json_arr)
        .register_fn("find", |ctx: CallCtx, a, b, c| rf(ctx, re::find(a, b, c)))
        .register_fn("sub", |ctx: CallCtx, a, b, c| rf(ctx, re::sub(a, b, c)))
        .register_global_module(exported_module!(io::anda_rhai).into())
        .register_global_module(exported_module!(update::tsunagu::anda_rhai).into())
        .register_static_module("rpmbuild", exported_module!(crate::build::anda_rhai).into())
        .build_type::<update::tsunagu::Req>()
        .build_type::<rpm::RPMSpec>();
    (en, sc)
}

/// Generates Error description from nanitozo \
/// used in `_tb()`
fn _gemsg(nanitozo: &TbErr) -> String {
    match nanitozo {
        TbErr::Report(o) => format!("From: {o}"),
        TbErr::Arb(o) => format!("From: {o}"),
        TbErr::Rhai(o) => format!("Rhai: {o}"),
    }
}

fn _gpos(p: Position) -> Option<(usize, usize)> {
    p.line().map(|l| (l, p.position().unwrap_or(0)))
}

pub fn _tb_fb(p: &str, s: std::path::Display, nntz: TbErr) {
    error!("{p}: {s} (no position data)\n{}", _gemsg(&nntz));
}

lazy_static! {
    static ref WORD_REGEX: Regex = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap();
}

#[instrument(name = "traceback")]
pub fn _tb(proj: &str, scr: &Path, nanitozo: TbErr, pos: Position, rhai_fn: &str, fn_src: &str) {
    if let Some((line, col)) = _gpos(pos) {
        // Print code
        let f = File::open(scr);
        let scr = scr.display();
        macro_rules! die {
            ($var:expr, $msg:expr) => {{
                if let Err(e) = $var {
                    error!($msg, e);
                    return _tb_fb(proj, scr, nanitozo);
                }
                $var.unwrap()
            }};
        }
        let f = die!(f, "{proj}: Cannot open `{scr}`: {}");
        for (n, sl) in BufReader::new(f).lines().enumerate() {
            if n != line - 1 {
                continue;
            }
            // replace tabs to avoid wrong position when print
            let sl = die!(sl, "{proj}: Cannot read line: {}").replace('\t', " ");
            let m = if let Some(x) = WORD_REGEX.find_at(sl.as_str(), col - 1) {
                let r = x.range();
                if r.start != col - 1 {
                    1
                } else {
                    r.len()
                }
            } else {
                1
            };
            let ln = line.to_string().len();
            let lns = " ".repeat(ln);
            let _l = "─".repeat(ln);
            let _r = "─".repeat(sl.len() + 2);
            let mut code = format!(
                "─{_l}─┬{_r}\n {lns} │ {scr}:{line}:{col}\n─{_l}─┼{_r}\n {line} │ {sl}\n {lns} │ {}{}",
                " ".repeat(col - 1),
                "🭶".repeat(m)
            );
            if !rhai_fn.is_empty() {
                code += &*format!("\n {lns} └─═ When invoking: {rhai_fn}()");
            }
            if !fn_src.is_empty() {
                code += &*format!("\n {lns} └─═ Function source: {fn_src}");
            }
            code += &*format!("\n {lns} └─═ {}", _gemsg(&nanitozo));
            let c = code.matches('└').count();
            if c > 0 {
                code = code.replacen('└', "├", c - 1);
            }
            return error!("Script Exception —— {proj}\n{code}");
        }
        error!("{proj}: nonexistance Exception line {line} in file {scr}");
    }
    _tb_fb(proj, scr.display(), nanitozo)
}

pub fn traceback(name: &str, scr: &Path, err: EvalAltResult) {
    trace!("{name}: Generating traceback");
    let pos = err.position();
    if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
        if let Some(AndaxError::RustReport(rhai_fn, fn_src, oerr)) =
            run_err.clone().try_cast::<AndaxError>()
        {
            _tb(
                name,
                scr,
                TbErr::Report(oerr),
                pos,
                rhai_fn.as_str(),
                fn_src.as_str(),
            );
            return;
        }
        if let Some(AndaxError::RustError(rhai_fn, fn_src, oerr)) =
            run_err.clone().try_cast::<AndaxError>()
        {
            _tb(
                name,
                scr,
                TbErr::Arb(oerr),
                pos,
                rhai_fn.as_str(),
                fn_src.as_str(),
            );
            return;
        }
    }
    _tb(name, scr, TbErr::Rhai(err), pos, "", "");
}

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
fn exec<'a>(
    name: &'a str,
    scr: &'a Path,
    mut sc: Scope<'a>,
    en: Engine
) -> Option<Scope<'a>> {
    debug!("Running {name}");
    match en.run_file_with_scope(&mut sc, scr.to_path_buf()) {
        Ok(()) => Some(sc),
        Err(err) => {
            traceback(name, scr, *err);
            None
        }
    }
}
