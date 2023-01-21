use crate::error::AndaxRes;
use anda_config::{load_from_file, Manifest};
use rhai::{plugin::*, EvalAltResult};
use std::path::PathBuf;

type Res<T> = Result<T, Box<EvalAltResult>>;

#[export_module]
pub mod ar {
    #[rhai_fn(return_raw)]
    pub(crate) fn file_cfg(ctx: NativeCallContext, path: &str) -> Res<Manifest> {
        Ok(load_from_file(&PathBuf::from(path)).ehdl(&ctx)?)
    }
}
