use std::collections::BTreeMap;

use hcl::Value;
use hcl::eval::{Context, FuncArgs, FuncDef};


/// HCL Function for loading environment variables
pub fn env_func(args: FuncArgs)  -> Result<Value, String> {
    let env = std::env::vars().collect::<BTreeMap<String, String>>();
    let key = args[0].as_str().unwrap();
    let value = env.get(key).unwrap();
    Ok(Value::String(value.to_string()))
}

/// Generate Context for HCL evaluation
pub fn hcl_context() -> Context<'static> {
    dotenv::dotenv().ok();
    let mut ctx = Context::new();
    let env_func = FuncDef::builder()
        .param(hcl::eval::ParamType::String)
        .build(env_func);
    ctx.declare_func("env", env_func);
    ctx
}
