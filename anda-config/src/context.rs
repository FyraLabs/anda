use std::collections::BTreeMap;

use hcl::Value;
use hcl::eval::{Context, FuncArgs, FuncDef};


// once_cell for global context
use once_cell::sync::OnceCell;
use std::sync::Mutex;

// todo: let this be mutable
static GLOBAL_CONTEXT: OnceCell<Mutex<Context>> = OnceCell::new();



/// HCL Function for loading environment variables
pub fn env_func(args: FuncArgs)  -> Result<Value, String> {
    let env = std::env::vars().collect::<BTreeMap<String, String>>();
    let key = args[0].as_str().unwrap();
    let value = env.get(key).unwrap();
    Ok(Value::String(value.to_string()))
}


/// Generate Context for HCL evaluation
pub fn hcl_context() -> Context<'static> {
    let c = GLOBAL_CONTEXT.get_or_init(|| {
        dotenv::dotenv().ok();
        let mut ctx = Context::new();
        let env_func = FuncDef::builder()
            .param(hcl::eval::ParamType::String)
            .build(env_func);
        ctx.declare_func("env", env_func);
    
        let env = std::env::vars().collect::<BTreeMap<String, String>>();
        let mut map = hcl::Map::new();
    
        for (key, value) in env.iter() {
            map.insert(key.to_string(), Value::String(value.to_string()));
        }
    
        ctx.declare_var("env", Value::Object(map));
    
        Mutex::new(ctx)
    });
    c.lock().unwrap().clone()
}
