use std::rc::Rc;
use rhai::EvalAltResult;
use smartstring::{LazyCompact, SmartString};

type SStr = SmartString<LazyCompact>;

#[derive(Clone)]
pub enum AndaxError {
    // rhai_fn, fn_src, E
    RustReport(SStr, SStr, Rc<color_eyre::Report>),
    RustError(SStr, SStr, Rc<dyn std::error::Error>),
}
pub enum TbErr {
    Report(Rc<color_eyre::Report>),
    Arb(Rc<dyn std::error::Error>),
    Rhai(EvalAltResult),
}

impl std::fmt::Debug for TbErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Report(r) => f.write_fmt(format_args!("{r:#}")),
            Self::Arb(e) => e.fmt(f),
            Self::Rhai(e) => e.fmt(f),
        }
    }
}

impl std::fmt::Display for TbErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Report(r) => f.write_fmt(format_args!("{r:#}")),
            Self::Arb(e) => e.fmt(f),
            Self::Rhai(e) => e.fmt(f),
        }
    }
}

pub(crate) trait AndaxRes<T> {
    fn ehdl(self, ctx: &rhai::NativeCallContext) -> Result<T, Box<EvalAltResult>>;
}

macro_rules! impl_ehdl {
    ($x:ty) => {
        impl<T> AndaxRes<T> for $x {
            fn ehdl(self, ctx: &rhai::NativeCallContext) -> Result<T, Box<EvalAltResult>>
            where
                Self: Sized,
            {
                self.map_err(|err| {
                    Box::new(EvalAltResult::ErrorRuntime(
                        rhai::Dynamic::from(AndaxError::RustError(
                            ctx.fn_name().into(),
                            ctx.source().unwrap_or("").into(),
                            std::rc::Rc::from(err),
                        )),
                        ctx.position(),
                    ))
                })
            }
        }
    };
}

impl_ehdl!(Result<T, std::string::FromUtf8Error>);
impl_ehdl!(std::io::Result<T>);
impl_ehdl!(Result<T, ureq::Error>);
impl_ehdl!(Result<T, serde_json::Error>);
