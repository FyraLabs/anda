use std::sync::Arc;
use serde::Deserialize;
use yew::prelude::*;
use reqwasm::http::Request;
use uuid::Uuid;
#[derive(Deserialize, Clone)]
pub(crate) struct Build {
    pub id: Uuid,
    #[serde(rename = "project_id")]
    pub proj: Option<Uuid>,
    #[serde(rename = "target_id")]
    pub tag: Option<Uuid>,
    pub status: usize,
}

impl Build {
    pub(crate) async fn list(limit: usize, page: usize) -> Result<Vec<Build>, Arc<reqwasm::Error>> {
        Ok(Request::get(&format!(
            "{}/builds/?{}&{}",
            env!("ANDA_ENDPOINT"),
            limit,
            page
        )).send().await?
        .json::<Vec<Build>>().await?)
    }

    pub(crate) fn format(builds: Vec<Build>) -> Html {
        builds
            .iter()
            .map(|b| {
                // unwrap project_id and then simplify it, or use a blank string if it's None
                let proj = b.proj.map_or("".to_string(), |p| p.simple().to_string());
                let tag = b.tag.map_or("".to_string(), |t| t.simple().to_string());
                html! {
                    <tr class="hover:shadow-xl">
                        <th><a href={ format!("/b/{}", &b.id) }>{ &b.id.simple() }</a></th>
                        <th>{ proj }</th>
                        <th>{ &b.status }</th>
                        <th>{ tag }</th>
                    </tr>
                }
            })
            .collect::<Html>()
    }
}
