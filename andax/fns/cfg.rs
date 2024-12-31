use crate::error::AndaxRes;
use anda_config::load_from_file;
use rhai::{
    plugin::{
        export_module, mem, Dynamic, ImmutableString, Module, NativeCallContext, PluginFunc,
        RhaiResult, TypeId,
    },
    EvalAltResult, FuncRegistration,
};
use std::path::PathBuf;

type Res<T> = Result<T, Box<EvalAltResult>>;

#[export_module]
pub mod ar {
    #[rhai_fn(return_raw)]
    pub fn load_file(ctx: NativeCallContext, path: &str) -> Res<rhai::Map> {
        let m = load_from_file(&PathBuf::from(path)).ehdl(&ctx)?;
        ctx.engine().parse_json(serde_json::to_string(&m).ehdl(&ctx)?, true)
    }
}
