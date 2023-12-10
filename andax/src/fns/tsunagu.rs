use crate::{error::AndaxRes, run::rf};
use rhai::{
    plugin::{
        export_module, mem, Dynamic, EvalAltResult, FnAccess, FnNamespace, ImmutableString, Module,
        NativeCallContext, PluginFunction, RhaiResult, TypeId,
    },
    CustomType,
};
use serde_json::Value;
use std::env::VarError;
use tracing::trace;

type Res<T> = Result<T, Box<EvalAltResult>>;

pub const USER_AGENT: &str = "AndaX";
#[export_module]
pub mod ar {
    type E = Box<rhai::EvalAltResult>;

    #[rhai_fn(return_raw, global)]
    pub fn get(ctx: NativeCallContext, url: &str) -> Res<String> {
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

    #[rhai_fn(return_raw, global)]
    pub fn gh(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let v: Value = ureq::get(&format!("https://api.github.com/repos/{repo}/releases/latest"))
            .set("Authorization", &format!("Bearer {}", env("GITHUB_TOKEN")?))
            .set("User-Agent", USER_AGENT)
            .call()
            .ehdl(&ctx)?
            .into_json()
            .ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["tag_name"].as_str().unwrap_or("").to_string())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let v: Value = ureq::get(&format!("https://api.github.com/repos/{repo}/tags"))
            .set("Authorization", &format!("Bearer {}", env("GITHUB_TOKEN")?))
            .set("User-Agent", USER_AGENT)
            .call()
            .ehdl(&ctx)?
            .into_json()
            .ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let v = v
            .as_array()
            .ok_or_else(|| E::from("gh_tag received not array"))
            .map(|a| a.first().ok_or_else(|| E::from("gh_tag no tags")))??;
        Ok(v["name"].as_str().unwrap_or("").to_string())
    }

    #[rhai_fn(return_raw, global)]
    pub fn pypi(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = ureq::get(&format!("https://pypi.org/pypi/{name}/json"));
        let obj: serde_json::Value =
            obj.set("User-Agent", USER_AGENT).call().ehdl(&ctx)?.into_json().ehdl(&ctx)?;
        let obj = obj.get("info").ok_or_else(|| E::from("No json[`info`]?"))?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`info`][`version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = ureq::get(&format!("https://crates.io/api/v1/crates/{name}"));
        let obj: serde_json::Value =
            obj.set("User-Agent", USER_AGENT).call().ehdl(&ctx)?.into_json().ehdl(&ctx)?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("max_stable_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`max_stable_version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates_max(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = ureq::get(&format!("https://crates.io/api/v1/crates/{name}"));
        let obj: serde_json::Value =
            obj.set("User-Agent", USER_AGENT).call().ehdl(&ctx)?.into_json().ehdl(&ctx)?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("max_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`max_version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates_newest(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = ureq::get(&format!("https://crates.io/api/v1/crates/{name}"));
        let obj: serde_json::Value =
            obj.set("User-Agent", USER_AGENT).call().ehdl(&ctx)?.into_json().ehdl(&ctx)?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("newest_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`newest_version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }
    #[rhai_fn(return_raw, global)]
    pub fn npm(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = ureq::get(&format!("https://registry.npmjs.org/{name}/latest"));
        let obj: serde_json::Value =
            obj.set("User-Agent", USER_AGENT).call().ehdl(&ctx)?.into_json().ehdl(&ctx)?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn env(key: &str) -> Res<String> {
        trace!("env(`{key}`) = {:?}", std::env::var(key));
        match std::env::var(key) {
            Ok(s) => Ok(s),
            Err(VarError::NotPresent) => Err(format!("env(`{key}`) not present").into()),
            Err(VarError::NotUnicode(o)) => Err(format!("env(`{key}`): invalid UTF: {o:?}").into()),
        }
    }

    #[rhai_fn(return_raw, global)]
    pub fn rust2rpm(name: &str) -> Res<()> {
        let cmd = std::process::Command::new("rust2rpm").arg(name).output();
        Ok(())
    }
}

#[derive(Clone)]
pub struct Req {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub redirects: i64,
}

impl CustomType for Req {
    fn build(mut builder: rhai::TypeBuilder<'_, Self>) {
        builder
            .with_name("Req")
            .with_fn("new_req", Self::new)
            .with_fn("get", |ctx: NativeCallContext, x: Self| rf(&ctx, x.get()))
            .with_fn("redirects", Self::redirects)
            .with_fn("head", Self::head);
    }
}

impl Req {
    pub const fn new(url: String) -> Self {
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
