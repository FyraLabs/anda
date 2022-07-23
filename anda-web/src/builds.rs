use patternfly_yew::{ColumnIndex, SharedTableModel, Span, TableRenderer};
use reqwasm::http::Request;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use yew::prelude::*;

#[derive(Deserialize, Clone, Debug, PartialEq)]
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
        ))
        .send()
        .await?
        .json::<Vec<Build>>()
        .await?)
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

pub(crate) struct BuildTable {
    model4: SharedTableModel<Build>,
}

impl TableRenderer for BuildTable {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => html! { <a href={ format!("/b/{}", &self.id) }>{ &self.id.simple() }</a> },
            1 => html! { { &self.model4.proj } },
            2 => html! { { &self.status } },
            3 => html! { { &self.tag } },
            _ => html! {},
        }
    }
    fn render_details(&self) -> Vec<Span> {
        vec![Span::max(html! {
            <>
                { "So many details for " }{ "idk" }
            </>
        })]
    }
}
