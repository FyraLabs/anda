use crate::error::AndaxRes;
use anda_config::load_from_file;
use rhai::{plugin::*, EvalAltResult};
use std::path::PathBuf;

type Res<T> = Result<T, Box<EvalAltResult>>;

#[export_module]
pub mod ar {
    #[rhai_fn(return_raw)]
    pub(crate) fn load_file(ctx: NativeCallContext, path: &str) -> Res<rhai::Map> {
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
                p.insert("podman".into(), _docker(proj.podman));
                p.insert("docker".into(), _docker(proj.docker));
                p.insert("flatpak".into(), _flatpak(proj.flatpak));
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
    o.map(|r| {
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
    .unwrap_or_else(|| ().into())
}
fn _docker(o: Option<anda_config::Docker>) -> Dynamic {
    o.map(|d| {
        let mut m = rhai::Map::new();
        m.insert(
            "image".into(),
            d.image
                .into_iter()
                .map(|(n, i)| {
                    let mut a = rhai::Map::new();
                    a.insert("dockerfile".into(), i.dockerfile.unwrap_or_default().into());
                    a.insert("import".into(), _pb(i.import));
                    a.insert("tag_latest".into(), i.tag_latest.unwrap_or(false).into());
                    a.insert("context".into(), i.context.into());
                    a.insert("version".into(), i.version.unwrap_or_default().into());
                    (n, a)
                })
                .collect(),
        );
        m.into()
    })
    .unwrap_or_else(|| ().into())
}
fn _flatpak(o: Option<anda_config::Flatpak>) -> Dynamic {
    o.map(|f| {
        let mut m = rhai::Map::new();
        m.insert("manifest".into(), _pb(Some(f.manifest)));
        m.insert("pre_script".into(), _pb(f.pre_script));
        m.insert("post_script".into(), _pb(f.post_script));
        m.into()
    })
    .unwrap_or_else(|| ().into())
}
