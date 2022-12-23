use log::debug;
use rhai::{CustomType, EvalAltResult};
use serde_json::Value;
use rhai::plugin::*;

type RhaiRes<T> = Result<T, Box<EvalAltResult>>;
pub fn ehdl<A, B>(o: Result<A, B>) -> RhaiRes<A>
where
    B: std::fmt::Debug + std::fmt::Display,
{
    if let Err(e) = o {
        return Err(e.to_string().into());
    }
    Ok(o.unwrap())
}
pub(crate) const USER_AGENT: &str = "Anda-update";
#[export_module]
pub mod anda_rhai {

    #[rhai_fn(return_raw)]
    pub fn get(url: &str) -> RhaiRes<String> {
        ehdl(ehdl(ureq::AgentBuilder::new()
            .redirects(0)
            .build()
            .get(url)
            .set("User-Agent", USER_AGENT)
            .call())?
            .into_string())
    }

    #[rhai_fn(return_raw)]
    pub fn gh(repo: &str) -> RhaiRes<String> {
        let v: Value =
            ehdl(ehdl(ureq::get(format!("https://api.github.com/repos/{}/releases/latest", repo).as_str())
                .set(
                    "Authorization",
                    format!("Bearer {}", env("GITHUB_TOKEN")?).as_str(),
                )
                .set("User-Agent", USER_AGENT)
                .call())?
                .into_json())?;
        debug!("Got json from {repo}:\n{v}");
        let binding = v["tag_name"].to_owned();
        let ver = binding.as_str().unwrap_or_default();
        if let Some(ver) = ver.strip_prefix('v') {
            return Ok(ver.to_string());
        }
        Ok(ver.to_string())
    }
    #[rhai_fn(return_raw)]
    pub(crate) fn env(key: &str) -> RhaiRes<String> {
        ehdl(std::env::var(key))
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
                .with_fn("get", |x: Self| ehdl(x.get()))
                .with_fn("redirects", |x: &mut Self, i: i64| x.redirects(i))
                .with_fn("head", |x: &mut Self, k: String, v: String| x.head(k, v));
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
        pub fn get(self) -> RhaiRes<String> {
            let r = ureq::AgentBuilder::new()
                .redirects(ehdl(self.redirects.try_into())?)
                .build()
                .get(&self.url);
            let mut r = r.set("User-Agent", USER_AGENT);
            for (k, v) in self.headers {
                r = r.set(k.as_str(), v.as_str());
            }
            ehdl(ehdl(r.call())?.into_string())
        }
        pub fn head(&mut self, key: String, val: String) -> &mut Self {
            self.headers.push((key, val));
            self
        }
        pub fn redirects(&mut self, i: i64) -> &mut Self {
            self.redirects = i;
            self
        }
    }
}
