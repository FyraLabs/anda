use std::sync::Arc;

use serde::Deserialize;
use yew::prelude::*;


#[derive(Deserialize, Clone)]
pub(crate) struct Artifact {
    id: String,
    name: String,
    #[serde(rename = "build_id")]
    build: String,
    timestamp: String,
}

impl Artifact {
    pub(crate) async fn list(limit: usize, page: usize) -> Result<Vec<Artifact>, Arc<reqwest::Error>> {
        Ok(reqwest::get(format!(
            "{}/artifacts/?{}&{}",
            env!("ANDA_ENDPOINT"),
            limit,
            page
        ))
        .await?
        .json::<Vec<Artifact>>().await?)
    }
    pub(crate) fn format(artifacts: Vec<Artifact>) -> Html {
        artifacts
            .iter()
            .map(|a| {
                html! {
                    <a href={ format!("/a/{}", &a.id) }>
                        <tr>
                            <th>{ &a.id }</th>
                            <th>{ &a.name }</th>
                            <th>{ &a.build }</th>
                            <th>{ &a.timestamp }</th>
                        </tr>
                    </a>
                }
            })
            .collect::<Html>()
    }
}
