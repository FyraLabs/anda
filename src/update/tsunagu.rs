use rhai::{CustomType, EvalAltResult};
use serde_json::Value;

fn ehdl<A, B>(o: Result<A, B>) -> Result<A, Box<EvalAltResult>>
where
    B: std::fmt::Debug + std::fmt::Display,
{
    if let Err(e) = o {
        return Err(e.to_string().into());
    }
    Ok(o.unwrap())
}

pub fn get<T: reqwest::IntoUrl>(url: T) -> Result<String, Box<EvalAltResult>> {
    let client = ehdl(
        reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build(),
    )?;
    let res = ehdl(client.get(url).send())?;
    ehdl(res.text())
}

pub fn json<T: Into<String>>(txt: T) -> Result<Value, Box<EvalAltResult>> {
    let s: String = txt.into();
    ehdl(serde_json::from_str(s.as_str()))
}

pub fn get_json<I: serde_json::value::Index>(obj: Value, index: I) -> Result<Value, Box<EvalAltResult>> {
    ehdl(obj.get(index).ok_or("Invalid index (json)").map(|o| o.to_owned()))
}

pub fn get_json_i(obj: Value, index: i64) -> Result<Value, Box<EvalAltResult>> {
    get_json(obj, ehdl(usize::try_from(index))?)
}

pub fn string_json(obj: Value) -> Result<String, Box<EvalAltResult>> {
    ehdl(obj.as_str().ok_or("Can't convert json to &str").map(|s| s.to_string()))
}
pub fn i64_json(obj: Value) -> Result<i64, Box<EvalAltResult>> {
    ehdl(obj.as_i64().ok_or("Can't convert json to i64"))
}
pub fn f64_json(obj: Value) -> Result<f64, Box<EvalAltResult>> {
    ehdl(obj.as_f64().ok_or("Can't convert json to f64"))
}
pub fn bool_json(obj: Value) -> Result<bool, Box<EvalAltResult>> {
    ehdl(obj.as_bool().ok_or("Can't convert json to bool"))
}
 
pub fn gh<T: Into<String>>(repo: T) -> Result<String, Box<EvalAltResult>> {
    let txt = ehdl(
        ehdl(
            ehdl(
                reqwest::blocking::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build(),
            )?
            .get(format!(
                "https://github.com/{}/releases/latest",
                repo.into()
            ))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", ehdl(std::env::var("GITHUB_TOKEN"))?),
            )
            .send(),
        )?
        .text(),
    )?;
    let v: Value = ehdl(serde_json::from_str(&txt))?;
    let ver = v["tag_name"].to_string();
    if let Some(ver) = ver.strip_prefix('v') {
        return Ok(ver.to_string());
    }
    Ok(ver)
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
            .with_fn("get", Self::get)
            .with_fn("head", Self::head);
    }
}

impl Req {
    pub fn new(url: String) -> Self {
        Self {
            url,
            headers: reqwest::header::HeaderMap::new(),
        }
    }
    pub fn get(self) -> Result<String, Box<EvalAltResult>> {
        ehdl(
            ehdl(
                ehdl(
                    reqwest::blocking::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .build(),
                )?
                .get(self.url)
                .headers(self.headers)
                .send(),
            )?
            .text(),
        )
    }
    pub fn head(&mut self, key: String, val: String) -> Result<(), Box<EvalAltResult>> {
        let x = ehdl(self.headers.try_entry(key))?;
        x.or_insert(ehdl(val.parse())?);
        Ok(())
    }
}
