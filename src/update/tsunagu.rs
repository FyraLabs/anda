use anyhow::{anyhow, Result};
use log::debug;
use rhai::{CustomType, EvalAltResult};
use serde_json::Value;

pub const USER_AGENT: &str = "Anda-update";
pub fn ehdl<A, B>(o: Result<A, B>) -> Result<A, Box<EvalAltResult>>
where
    B: std::fmt::Debug + std::fmt::Display,
{
    if let Err(e) = o {
        return Err(e.to_string().into());
    }
    Ok(o.unwrap())
}
pub fn get(url: &str) -> Result<String> {
    Ok(ureq::AgentBuilder::new()
        .redirects(0)
        .build()
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()?
        .into_string()?)
}

pub fn json<T: Into<String>>(txt: T) -> Result<Value> {
    Ok(serde_json::from_str(txt.into().as_str())?)
}

pub fn get_json<I: serde_json::value::Index>(obj: Value, index: I) -> Result<Value> {
    obj.get(index)
        .ok_or_else(|| anyhow!("Invalid index (json)"))
        .map(|o| o.to_owned())
}

pub fn get_json_i(obj: Value, index: i64) -> Result<Value> {
    get_json(obj, usize::try_from(index)?)
}

pub fn string_json(obj: Value) -> Result<String> {
    obj.as_str()
        .ok_or_else(|| anyhow!("Can't convert json to &str"))
        .map(|s| s.to_string())
}
pub fn i64_json(obj: Value) -> Result<i64> {
    obj.as_i64()
        .ok_or_else(|| anyhow!("Can't convert json to i64"))
}
pub fn f64_json(obj: Value) -> Result<f64> {
    obj.as_f64()
        .ok_or_else(|| anyhow!("Can't convert json to f64"))
}
pub fn bool_json(obj: Value) -> Result<bool> {
    obj.as_bool()
        .ok_or_else(|| anyhow!("Can't convert json to bool"))
}

pub fn gh<T: Into<String>>(repo: T) -> Result<String> {
    let repo = repo.into();
    let v: Value =
        ureq::get(format!("https://api.github.com/repos/{}/releases/latest", repo).as_str())
            .set(
                "Authorization",
                format!("Bearer {}", std::env::var("GITHUB_TOKEN")?).as_str(),
            )
            .set("User-Agent", USER_AGENT)
            .call()?
            .into_json()?;
    debug!("Got json from {repo}:\n{v}");
    let ver = string_json(v["tag_name"].to_owned())?;
    if let Some(ver) = ver.strip_prefix('v') {
        return Ok(ver.to_string());
    }
    Ok(ver)
}
pub fn env(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
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
    pub fn get(self) -> Result<String> {
        let r = ureq::AgentBuilder::new().redirects(self.redirects.try_into()?).build().get(&self.url);
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
