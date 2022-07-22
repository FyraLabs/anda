use std::sync::Arc;

use serde::Deserialize;
use uuid::Uuid;
use yew::prelude::*;
use reqwasm::http::Request;


#[derive(Deserialize, Clone)]
pub struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: String,
}

impl Artifact {
    pub(crate) async fn list(limit: usize, page: usize) -> Result<Vec<Artifact>, Arc<reqwasm::Error>> {
        Ok(Request::get(&format!(
            "{}/artifacts/?{}&{}",
            env!("ANDA_ENDPOINT"),
            limit,
            page
        ))
        .send()
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
                            <th>{ &a.id.simple() }</th>
                            <th>{ &a.name }</th>
                            <th>{ &a.build_id.simple() }</th>
                            <th>{ &a.timestamp }</th>
                        </tr>
                    </a>
                }
            })
            .collect::<Html>()
    }
}
