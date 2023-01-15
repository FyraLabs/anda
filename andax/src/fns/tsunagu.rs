use crate::{error::AndaxRes, run::rf};
use rhai::{plugin::*, CustomType};
use serde_json::Value;
use std::env::VarError;
use tracing::trace;

type Res<T> = Result<T, Box<EvalAltResult>>;

pub(crate) const USER_AGENT: &str = "andax";
#[export_module]
pub mod ar {
    #[rhai_fn(return_raw)]
    pub(crate) fn get(ctx: NativeCallContext, url: &str) -> Res<String> {
        ureq::AgentBuilder::new()
            .redirects(0)
            .build()
            .get(url)
            .set("User-Agent", USER_AGENT)
            .call()
            .ehdl(&ctx)?
            .into_string()
            .ehdl(&ctx)
    }

    #[rhai_fn(return_raw)]
    pub(crate) fn gh(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let v: Value = ureq::get(&format!("https://api.github.com/repos/{repo}/releases/latest"))
            .set("Authorization", &format!("Bearer {}", env("GITHUB_TOKEN")?))
            .set("User-Agent", USER_AGENT)
            .call()
            .ehdl(&ctx)?
            .into_json()
            .ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let binding = &v["tag_name"];
        let ver = binding.as_str().unwrap_or("");
        if let Some(ver) = ver.strip_prefix('v') {
            return Ok(ver.to_string());
        }
        Ok(ver.to_string())
    }
    #[rhai_fn(return_raw)]
    pub(crate) fn gh_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let v: Value = ureq::get(&format!("https://api.github.com/repos/{repo}/tags"))
            .set("Authorization", &format!("Bearer {}", env("GITHUB_TOKEN")?))
            .set("User-Agent", USER_AGENT)
            .call()
            .ehdl(&ctx)?
            .into_json()
            .ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let binding = &v[0]["name"];
        let ver = binding.as_str().unwrap_or("");
        if let Some(ver) = ver.strip_prefix('v') {
            return Ok(ver.to_string());
        }
        Ok(ver.to_string())
    }

    #[rhai_fn(return_raw)]
    pub(crate) fn pypi(ctx: NativeCallContext, name: &str) -> Res<String> {
        ctx.engine().eval(&format!("get(`https://pypi.org/pypi/{name}/json`).json().info.version"))
    }

    #[rhai_fn(return_raw)]
    pub(crate) fn crates(ctx: NativeCallContext, name: &str) -> Res<String> {
        ctx.engine().eval(&format!(
            "get(`https://crates.io/api/v1/crates/{name}`).json().crate.max_stable_version"
        ))
    }

    #[rhai_fn(return_raw)]
    pub(crate) fn crates_max(ctx: NativeCallContext, name: &str) -> Res<String> {
        ctx.engine().eval(&format!(
            "get(`https://crates.io/api/v1/crates/{name}`).json().crate.max_version"
        ))
    }

    #[rhai_fn(return_raw)]
    pub(crate) fn crates_newest(ctx: NativeCallContext, name: &str) -> Res<String> {
        ctx.engine().eval(&format!(
            "get(`https://crates.io/api/v1/crates/{name}`).json().crate.newest_version"
        ))
    }

    #[rhai_fn(return_raw)]
    pub(crate) fn env(key: &str) -> Res<String> {
        match std::env::var(key) {
            Ok(s) => Ok(s),
            Err(VarError::NotPresent) => Err(format!("env(`{key}`) not present").into()),
            Err(VarError::NotUnicode(o)) => Err(format!("env(`{key}`): invalid UTF: {o:?}").into()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Req {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub redirects: i64,
}

impl CustomType for Req {
    fn build(mut builder: rhai::TypeBuilder<'_, Self>) {
        builder
            .with_name("Req")
            .with_fn("new_req", Self::new)
            .with_fn("get", |ctx: NativeCallContext, x: Self| rf(ctx, x.get()))
            .with_fn("redirects", Self::redirects)
            .with_fn("head", Self::head);
    }
}

impl Req {
    pub fn new(url: String) -> Self {
        Self { url, headers: vec![], redirects: 0 }
    }
    pub fn get(self) -> color_eyre::Result<String> {
        let r =
            ureq::AgentBuilder::new().redirects(self.redirects.try_into()?).build().get(&self.url);
        let mut r = r.set("User-Agent", USER_AGENT);
        for (k, v) in self.headers {
            r = r.set(k.as_str(), v.as_str());
        }
        Ok(r.call()?.into_string()?)
    }
    pub fn head(&mut self, key: String, val: String) {
        self.headers.push((key, val));
    }
    pub fn redirects(&mut self, i: i64) {
        self.redirects = i;
    }
}
