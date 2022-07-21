use std::sync::Arc;

use serde::Deserialize;
use yew::prelude::*;

#[derive(Deserialize, Clone)]
pub(crate) struct Build {
    pub id: String,
    #[serde(rename = "project_id")]
    pub proj: String,
    #[serde(rename = "target_id")]
    pub tag: String,
    pub status: String,
}

impl Build {
    pub(crate) async fn list(limit: usize, page: usize) -> Result<Vec<Build>, Arc<reqwest::Error>> {
        Ok(reqwest::get(format!(
            "{}/builds/?{}&{}",
            env!("ANDA_ENDPOINT"),
            limit,
            page
        )).await?
        .json::<Vec<Build>>().await?)
    }
    pub(crate) fn format(builds: Vec<Build>) -> Html {
        builds
            .iter()
            .map(|b| {
                html! {
                    <a href={ format!("/b/{}", &b.id) }>
                        <tr class="hover:shadow-xl">
                            <th>{ &b.id }</th>
                            <th>{ &b.proj }</th>
                            <th>{ &b.tag }</th>
                            <th>{ &b.status }</th>
                        </tr>
                    </a>
                }
            })
            .collect::<Html>()
    }
}
