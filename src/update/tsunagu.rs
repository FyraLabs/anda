use rhai::EvalAltResult;
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

pub fn gh<T: Into<String>>(repo: T) -> Result<String, Box<EvalAltResult>> {
    let txt = ehdl(
        ehdl(
            ehdl(reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build())?
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
