use std::collections::BTreeMap;

use hcl::eval::{Context, FuncArgs, FuncDef};
use hcl::Value;

// once_cell for global context
use once_cell::sync::OnceCell;
use parking_lot::Mutex;

// todo: let this be mutable
static GLOBAL_CONTEXT: OnceCell<Mutex<Context>> = OnceCell::new();

/// Generate Context for HCL evaluation
/// 
/// # Panics
/// - cannot lock mutex (poison?)
/// - cannot convert FuncArgs to str
/// - cannot find FuncArgs as key in environment variables
pub fn hcl_context() -> Context<'static> {
    let env_func = |args: FuncArgs| {
        let env = std::env::vars().collect::<BTreeMap<String, String>>();
        let key = args[0].as_str().unwrap();
        let value = env.get(key).unwrap();
        Ok(Value::String(value.to_string()))
    };
    let c = GLOBAL_CONTEXT.get_or_init(|| {
        dotenv::dotenv().ok();
        let mut ctx = Context::new();
        let env_func = FuncDef::builder().param(hcl::eval::ParamType::String).build(env_func);
        ctx.declare_func("env", env_func);

        let env = std::env::vars().collect::<BTreeMap<String, String>>();
        let mut map = hcl::Map::new();

        map.extend(env.into_iter().map(|(k,v)| (k, Value::String(v))));

        ctx.declare_var("env", Value::Object(map));

        Mutex::new(ctx)
    });
    c.lock().clone()
}
