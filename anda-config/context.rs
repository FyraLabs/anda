use std::sync::LazyLock;

use hcl::eval::{Context, FuncDef};
use hcl::Value;

static GLOBAL_CONTEXT: LazyLock<Context> = LazyLock::new(|| {
    dotenv::dotenv().ok();
    let mut ctx = Context::new();
    let env_func = FuncDef::builder().param(hcl::eval::ParamType::String).build(|args| {
        let [Value::String(key)] = &args.into_values()[..] else {
            return Err("Invalid argument, expected 1 string argument".into());
        };
        let value = std::env::var(key).map_err(|e| format!("env(`${key}`): {e:?}"))?;
        Ok(Value::String(value))
    });
    ctx.declare_func("env", env_func);

    ctx.declare_var(
        "env",
        Value::Object(std::env::vars().map(|(k, v)| (k, Value::String(v))).collect()),
    );
    ctx
});

/// Generate Context for HCL evaluation
pub fn hcl_context() -> Context<'static> {
    GLOBAL_CONTEXT.clone()
}
