use anyhow::{anyhow, Result};
use regex::Regex;

pub fn find(r: &str, text: &str, group: i64) -> Result<String> {
    let regex = Regex::new(r)?;
    let cap = regex
        .captures(text)
        .ok_or_else(|| anyhow!("Can't match regex: {r}\nText: {text}"))?;
    Ok(cap
        .get(group.try_into()?)
        .ok_or_else(|| anyhow!("Can't get group: {r}\nText: {text}"))?
        .as_str()
        .to_string())
}

pub fn sub(r: &str, rep: &str, text: &str) -> Result<String> {
    let regex = Regex::new(r)?;
    Ok(regex.replace_all(text, rep).to_string())
}
