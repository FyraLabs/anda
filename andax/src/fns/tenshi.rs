/// Tenshi module for AndaX
/// Various utility functions for Andaman Scripting
use crate::error::AndaxRes;

use rhai::{plugin::*, Dynamic, EvalAltResult as RhaiE};
type Res<T = ()> = Result<T, Box<RhaiE>>;

#[export_module]
pub mod ar {
    use hcl::eval::Evaluate;

    use std::io::Read;
    use tracing::{debug, trace};
    #[rhai_fn(return_raw, global)]
    pub(crate) fn template(ctx: NativeCallContext, tmpl: rhai::Map, input: String) -> Res<String> {
        let mut hcl = anda_config::context::hcl_context();
        for (k, v) in tmpl.into_iter() {
            let key = k.to_string();
            // turn value into a hcl::Value::Object
            let value = hcl::value::to_value(v).ehdl(&ctx)?;

            let span = tracing::debug_span!("hcl.declare_var", ?key, ?value);
            span.in_scope(|| {
                hcl.declare_var(key, value);
            });
            // let value = hcl::value::Value::try_from(_val);
        }
        println!("{:?}", ctx.source());

        let template =
            <hcl::template::Template as std::str::FromStr>::from_str(&input).ehdl(&ctx)?;

        let res = template.evaluate(&hcl).ehdl(&ctx)?;

        // ok, so we usually build from RPM spec files.
        // the issue here is that: rpm macros are defined using %{}
        // which coincidentally, is also the syntax for hcl template interpolation.
        //
        // We will be doing a stopgap solution for now, which is requiring the user to use
        // @{} instead of %{} for rpm macros, then replace them after evaluation
        // FIXME
        let res = res.replace("@{", "%{");

        trace!(?res, "Template Result");
        // write the result to out

        Ok(res)
    }

    /// Function that takes in an object map and a file path
    #[rhai_fn(return_raw, global)]
    pub(crate) fn template_file(
        ctx: NativeCallContext,
        map: rhai::Map,
        path: String,
    ) -> Res<String> {
        let mut file = std::fs::File::open(&path).ehdl(&ctx)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf).ehdl(&ctx)?;
        // template is a HCL templated file
        debug!("Templating file: {:#?}", path);
        debug!(?map, "Loading Template");
        template(ctx, map, buf)
    }

    /// turns a map into json
    #[rhai_fn(return_raw, global)]
    pub(crate) fn to_json(ctx: NativeCallContext, map: rhai::Map) -> Res<String> {
        let json = serde_json::to_string(&map).ehdl(&ctx)?;
        Ok(json)
    }

    /// turns a json string into a map
    #[rhai_fn(return_raw, global)]
    pub(crate) fn from_json(ctx: NativeCallContext, json: String) -> Res<rhai::Map> {
        let map = serde_json::from_str(&json).ehdl(&ctx)?;
        Ok(map)
    }

}
