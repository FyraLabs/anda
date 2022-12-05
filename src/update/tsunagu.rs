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
pub fn get<T: reqwest::IntoUrl>(url: T) -> Result<String> {
    Ok(reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()?
        .get(url)
        .send()?
        .text()?)
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
    let txt = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()?
        .get(format!(
            "https://api.github.com/repos/{}/releases/latest",
            repo
        ))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", std::env::var("GITHUB_TOKEN")?),
        )
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()?
        .text()?;
    debug!("Got json from {repo}:\n{txt}");
    let v: Value = serde_json::from_str(&txt)?;
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
    pub headers: reqwest::header::HeaderMap,
}

impl CustomType for Req {
    fn build(mut builder: rhai::TypeBuilder<'_, Self>) {
        builder
            .with_name("Req")
            .with_fn("new_req", Self::new)
            .with_fn("get", |x: Self| ehdl(x.get()))
            .with_fn("head", |x: &mut Self, k: String, v: String| {
                ehdl(x.head(k, v))
            });
    }
}

impl Req {
    pub fn new(url: String) -> Self {
        Self {
            url,
            headers: reqwest::header::HeaderMap::new(),
        }
    }
    pub fn get(self) -> Result<String> {
        Ok(reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .get(self.url)
            .headers(self.headers)
            .send()?
            .text()?)
    }
    pub fn head(&mut self, key: String, val: String) -> Result<()> {
        let x = self.headers.try_entry(key)?;
        x.or_insert(val.parse()?);
        Ok(())
    }
}
