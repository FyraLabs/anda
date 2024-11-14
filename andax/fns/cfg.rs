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
        let mut manifest = rhai::Map::new();
        let mut conf = rhai::Map::new();
        conf.insert("mock_config".into(), m.config.mock_config.unwrap_or_default().into());
        conf.insert("strip_prefix".into(), m.config.strip_prefix.unwrap_or_default().into());
        conf.insert("strip_suffix".into(), m.config.strip_suffix.unwrap_or_default().into());
        conf.insert("project_regex".into(), m.config.project_regex.unwrap_or_default().into());
        manifest.insert("config".into(), conf.into());
        let mut p = rhai::Map::new();
        for (name, proj) in m.project {
            p.insert(name.into(), {
                let mut p = rhai::Map::new();
                p.insert("rpm".into(), _rpm(proj.rpm));
                p.insert("pre_script".into(), _pb(proj.pre_script));
                p.insert("post_script".into(), _pb(proj.post_script));
                p.insert("env".into(), proj.env.unwrap_or_default().into());
                p.insert("alias".into(), proj.alias.unwrap_or_default().into());
                p.insert("scripts".into(), proj.scripts.unwrap_or_default().into());
                p.insert("labels".into(), proj.labels.into());
                p.insert("update".into(), _pb(proj.update));
                p.into()
            });
        }
        manifest.insert("project".into(), p.into());
        Ok(manifest)
    }
}

fn _pb(pb: Option<PathBuf>) -> Dynamic {
    pb.map_or(().into(), |s| s.to_str().unwrap_or("").into())
}
fn _rpm(o: Option<anda_config::RpmBuild>) -> Dynamic {
    o.map_or(().into(), |r| {
        let mut m = rhai::Map::new();
        m.insert("spec".into(), r.spec.to_str().unwrap_or("").into());
        m.insert("sources".into(), _pb(r.sources));
        m.insert("package".into(), r.package.unwrap_or_default().into());
        m.insert("pre_script".into(), _pb(r.pre_script));
        m.insert("post_script".into(), _pb(r.post_script));
        m.insert("enable_scm".into(), r.enable_scm.unwrap_or(false).into());
        m.insert("scm_opts".into(), r.scm_opts.unwrap_or_default().into());
        m.insert("config".into(), r.config.unwrap_or_default().into());
        m.insert("mock_config".into(), r.mock_config.unwrap_or_default().into());
        m.insert("plugin_opts".into(), r.plugin_opts.unwrap_or_default().into());
        m.insert("macros".into(), r.macros.unwrap_or_default().into());
        m.insert("opts".into(), r.opts.unwrap_or_default().into());
        m.into()
    })
}
