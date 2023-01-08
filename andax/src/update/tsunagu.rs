use crate::{
    error::AndaxRes,
    run::{ehdl, rf},
};
use rhai::{plugin::*, CustomType, EvalAltResult};
use serde_json::Value;
use tracing::debug;

type RhaiRes<T> = Result<T, Box<EvalAltResult>>;

pub(crate) const USER_AGENT: &str = "Anda-update";
#[export_module]
pub mod anda_rhai {

    #[rhai_fn(return_raw)]
    pub fn get(ctx: NativeCallContext, url: &str) -> RhaiRes<String> {
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
    pub fn gh(ctx: NativeCallContext, repo: &str) -> RhaiRes<String> {
        let v: Value =
            ureq::get(format!("https://api.github.com/repos/{}/releases/latest", repo).as_str())
                .set(
                    "Authorization",
                    format!("Bearer {}", std::env::var("GITHUB_TOKEN").ehdl(&ctx)?).as_str(),
                )
                .set("User-Agent", USER_AGENT)
                .call()
                .ehdl(&ctx)?
                .into_json()
                .ehdl(&ctx)?;
        debug!("Got json from {repo}:\n{v}");
        let binding = v["tag_name"].to_owned();
        let ver = binding.as_str().unwrap_or_default();
        if let Some(ver) = ver.strip_prefix('v') {
            return Ok(ver.to_string());
        }
        Ok(ver.to_string())
    }
    #[rhai_fn(return_raw)]
    pub(crate) fn env(ctx: NativeCallContext, key: &str) -> RhaiRes<String> {
        std::env::var(key).ehdl(&ctx)
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
            Self {
                url,
                headers: vec![],
                redirects: 0,
            }
        }
        pub fn get(self) -> color_eyre::Result<String> {
            let r = ureq::AgentBuilder::new()
                .redirects(self.redirects.try_into()?)
                .build()
                .get(&self.url);
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
}
