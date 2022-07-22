use std::sync::Arc;

use serde::Deserialize;
use yew::prelude::*;
use reqwasm::http::Request;

#[derive(Deserialize, Clone)]
pub(crate) struct Project {
    id: String,
    name: String,
    description: String,
}

impl Project {
    pub(crate) async fn list(limit: usize, page: usize) -> Result<Vec<Project>, Arc<reqwasm::Error>> {
        Ok(Request::get(&format!(
            "/projects/?{}&{}", 
            limit,
            page
        )).send()
        .await?
        .json::<Vec<Project>>()
        .await?)
    }
    pub(crate) fn format(projects: Vec<Project>) -> Html {
        projects
            .iter()
            .map(|p| {
                html! {
                    <a href={ format!("/p/{}", &p.id) }>
                        <tr>
                            <th>{ &p.id }</th>
                            <th>{ &p.name }</th>
                            <th>{ &p.description }</th>
                        </tr>
                    </a>
                }
            })
            .collect::<Html>()
    }
}
