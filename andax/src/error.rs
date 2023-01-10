#[derive(Clone)]
pub enum AndaxError {
    // rhai_fn, fn_src, E
    RustReport(String, String, std::rc::Rc<color_eyre::Report>),
    RustError(String, String, std::rc::Rc<dyn std::error::Error>),
}

pub(crate) trait AndaxRes<T> {
    fn ehdl(self, ctx: &rhai::NativeCallContext) -> Result<T, Box<rhai::EvalAltResult>>;
}

macro_rules! impl_ehdl {
    ($x:ty) => {
        impl<T> AndaxRes<T> for $x {
            fn ehdl(self, ctx: &rhai::NativeCallContext) -> Result<T, Box<rhai::EvalAltResult>>
            where
                Self: Sized,
            {
                self.map_err(|err| {
                    Box::new(rhai::EvalAltResult::ErrorRuntime(
                        rhai::Dynamic::from(AndaxError::RustError(
                            ctx.fn_name().to_string(),
                            ctx.source().unwrap_or("").to_string(),
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
