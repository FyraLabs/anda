use hcl::eval::{Context, Evaluate};
use hcl::template::Template;
use std::str::FromStr;
use crate::context::hcl_context;

pub fn parse_template(template: &str) -> Result<String, String> {
    let template = Template::from_str(template).map_err(|e| e.to_string())?;
    let ctx = hcl_context();
    let value = template.evaluate(&ctx).map_err(|e| e.to_string())?;
    Ok(value.as_str().to_string()) 
}


#[test]
fn test_templ() {
    let template = "hello ${env.USER}";
    let result = parse_template(template).unwrap();
    println!("{}", result);
    // get current username
    let username = std::env::var("USER").unwrap();
    assert_eq!(result, format!("hello {}", username));
}