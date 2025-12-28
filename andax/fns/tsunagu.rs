use crate::{error::AndaxRes, run::rf};
use rhai::{
    plugin::{
        export_module, mem, Dynamic, EvalAltResult, FnNamespace, ImmutableString, Module,
        NativeCallContext, PluginFunc, RhaiResult, TypeId,
    },
    CustomType, FuncRegistration,
};
use serde_json::Value;
use std::env::VarError;
use tracing::trace;

type Res<T> = Result<T, Box<EvalAltResult>>;

pub const USER_AGENT: &str = "AndaX";
#[export_module]
pub mod ar {
    type E = Box<rhai::EvalAltResult>;

    static AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
        ureq::Agent::new_with_config(ureq::Agent::config_builder().build())
    });

    #[rhai_fn(return_raw, global)]
    pub fn get_json(ctx: NativeCallContext, url: &str) -> Res<Dynamic> {
        let resp = AGENT.get(url).header("User-Agent", USER_AGENT).call().ehdl(&ctx)?;
        resp.into_body().read_json().ehdl(&ctx)
    }

    fn get_json_value(ctx: NativeCallContext, url: &str) -> Res<Value> {
        let resp = AGENT.get(url).header("User-Agent", USER_AGENT).call().ehdl(&ctx)?;
        resp.into_body().read_json().ehdl(&ctx)
    }

    #[rhai_fn(return_raw, global)]
    pub fn get(ctx: NativeCallContext, url: &str) -> Res<String> {
        let resp = AGENT.get(url).header("User-Agent", USER_AGENT).call().ehdl(&ctx)?;
        resp.into_body().read_to_string().ehdl(&ctx)
    }

    #[rhai_fn(return_raw, global)]
    pub fn gh(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/releases/latest")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["tag_name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/tags")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let v = (v.as_array())
            .ok_or_else(|| E::from("gh_tag received not array"))
            .map(|a| a.first().ok_or_else(|| E::from("gh_tag no tags")))??;
        Ok(v["name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_commit(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/commits/HEAD")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["sha"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_rawfile(ctx: NativeCallContext, repo: &str, branch: &str, file: &str) -> Res<String> {
        get(ctx, &format!("https://raw.githubusercontent.com/{repo}/{branch}/{file}"))
    }

    #[rhai_fn(return_raw, name = "gitlab", global)]
    pub fn gitlab_domain(ctx: NativeCallContext, domain: &str, id: &str) -> Res<String> {
        let v = get_json_value(ctx, &format!("https://{domain}/api/v4/projects/{id}/releases/"))?;
        trace!("Got json from {id}:\n{v}");
        Ok(v[0]["tag_name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gitlab(ctx: NativeCallContext, id: &str) -> Res<String> {
        gitlab_domain(ctx, "gitlab.com", id)
    }
    #[rhai_fn(return_raw, name = "gitlab_tag", global)]
    pub fn gitlab_tag_domain(ctx: NativeCallContext, domain: &str, id: &str) -> Res<String> {
        let v =
            get_json_value(ctx, &format!("https://{domain}/api/v4/projects/{id}/repository/tags"))?;
        trace!("Got json from {id}:\n{v}");
        Ok(v[0]["name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gitlab_tag(ctx: NativeCallContext, id: &str) -> Res<String> {
        gitlab_tag_domain(ctx, "gitlab.com", id)
    }
    #[rhai_fn(return_raw, name = "gitlab_commit", global)]
    pub fn gitlab_commit_domain(
        ctx: NativeCallContext,
        domain: &str,
        id: &str,
        branch: &str,
    ) -> Res<String> {
        let v = get_json_value(
            ctx,
            &format!("https://{domain}/api/v4/projects/{id}/repository/branches/{branch}"),
        )?;
        trace!("Got json from {id}:\n{v}");
        Ok(v["commit"]["id"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gitlab_commit(ctx: NativeCallContext, id: &str, branch: &str) -> Res<String> {
        gitlab_commit_domain(ctx, "gitlab.com", id, branch)
    }

    #[rhai_fn(return_raw, global)]
    pub fn pypi(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://pypi.org/pypi/{name}/json"))?;
        let obj = obj.get("info").ok_or_else(|| E::from("No json[`info`]?"))?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`info`][`version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crates.io/api/v1/crates/{name}"))?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("max_stable_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`max_stable_version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates_max(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crates.io/api/v1/crates/{name}"))?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("max_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`max_version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates_newest(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crates.io/api/v1/crates/{name}"))?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("newest_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`newest_version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }
    #[rhai_fn(return_raw, global)]
    pub fn npm(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://registry.npmjs.org/{name}/latest"))?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`version`]?"))?;
        obj.as_str().map(std::string::ToString::to_string).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn hackage(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(
            ctx,
            &format!("https://hackage.haskell.org/package/{name}/preferred.json"),
        )?;
        let versions = obj.get("normal-version").ok_or(E::from("No json[`normal-version`]"))?;
        let latest = versions
            .as_array()
            .ok_or(E::from("`normal-version` is not an array"))?
            .get(0)
            .ok_or(E::from("No normal package versions avaialble"))?;
        latest
            .as_str()
            .map(std::string::ToString::to_string)
            .ok_or(E::from("Package version is not a string"))
    }

    #[rhai_fn(skip)]
    pub fn internal_env(key: &str) -> Res<String> {
        trace!("env(`{key}`) = {:?}", std::env::var(key));
        match std::env::var(key) {
            Ok(s) => Ok(s),
            Err(VarError::NotPresent) => Err(format!("env(`{key}`) not present").into()),
            Err(VarError::NotUnicode(o)) => Err(format!("env(`{key}`): invalid UTF: {o:?}").into()),
        }
    }

    #[rhai_fn(global)]
    pub fn env(key: &str) -> String {
        trace!("env(`{key}`) = {:?}", std::env::var_os(key));
        std::env::var_os(key).map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
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
        let cfg = ureq::Agent::config_builder().max_redirects(self.redirects.try_into()?).build();
        let r = ureq::Agent::new_with_config(cfg).get(&self.url);
        let mut r = r.header("User-Agent", USER_AGENT);
        for (k, v) in self.headers {
            r = r.header(k.as_str(), v.as_str());
        }
        Ok(r.call()?.into_body().read_to_string()?)
    }
    pub fn head(&mut self, key: String, val: String) {
        self.headers.push((key, val));
    }
    pub const fn redirects(&mut self, i: i64) {
        self.redirects = i;
    }
}
