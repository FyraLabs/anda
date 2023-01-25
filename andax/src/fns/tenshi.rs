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

    /// Function that takes in an object map and a file path
    #[rhai_fn(return_raw)]
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
        let mut hcl = anda_config::context::hcl_context();

        for (k, v) in map.into_iter() {
            let key = k.to_string();
            // turn value into a hcl::Value::Object
            let value = hcl::value::to_value(v).ehdl(&ctx)?;

            let span = tracing::debug_span!("hcl.declare_var", key = ?key, value = ?value);
            span.in_scope(|| {
                hcl.declare_var(key, value);
            });

            // let value = hcl::value::Value::try_from(_val);
        }

        let template = <hcl::template::Template as std::str::FromStr>::from_str(&buf).ehdl(&ctx)?;

        let res = template.evaluate(&hcl).ehdl(&ctx)?;

        trace!(?res, "Template Result");
        // write the result to out

        Ok(res)
    }
}
