use log::info;
use patternfly_yew::{
    ColumnIndex, SharedTableModel, Table, TableColumn, TableHeader, TableMode, TableRenderer,
};
use reqwasm::http::Request;
use serde::Deserialize;
use uuid::Uuid;
use yew::prelude::*;

#[derive(Deserialize, Clone)]
pub(crate) struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub build_id: Uuid,
    pub timestamp: String,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct ArtifactDisplay<'a> {
    pub id: String,
    pub name: String,
    pub build_id: String,
    pub timestamp: &'a String,
}

impl Artifact {
    pub(crate) fn list(
        limit: usize,
        page: usize,
    ) -> Result<UseStateHandle<Vec<Self>>, Box<dyn std::error::Error>> {
        let artifacts = use_state(|| vec![]);
        {
            let artifacts = artifacts.clone();
            use_effect_with_deps(
                move |_| {
                    let artifacts = artifacts.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let fetched = Request::get(&format!("/api/artifacts/?{}&{}", limit, page))
                            .send()
                            .await
                            .unwrap();
                        let a = &fetched.text().await.unwrap();
                        info!("{}", a);
                        match serde_json::from_str::<Vec<Self>>(&a) {
                            Ok(fetched) => artifacts.set(fetched),
                            Err(err) => panic!(
                                "Failed to parse fetched artifacts; is the API down?\n{:?}",
                                err
                            ),
                        }
                    });
                    || ()
                },
                (),
            );
        }
        Ok(artifacts)
    }
    pub(crate) fn formats(artifacts: UseStateHandle<Vec<Self>>) -> Vec<ArtifactDisplay<'static>> {
        artifacts.iter().map(|a| Self::format(a)).collect()
    }
    pub(crate) fn format(a: &Self) -> ArtifactDisplay {
        ArtifactDisplay {
            id: a.id.as_simple().to_string(),
            build_id: a.build_id.as_simple().to_string(),
            name: a.name,
            timestamp: &a.timestamp,
        }
    }
}

impl TableRenderer for ArtifactDisplay<'_> {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => html! { <a href={ format!("/a/{}", &self.id) }>{ &self.name }</a> },
            1 => html! { { &self.build_id } },
            2 => html! { { &self.timestamp } },
            _ => html! {},
        }
    }
}

#[function_component(Artifacts)]
pub(crate) fn artifacts() -> Html {
    let header = html_nested! {
        <TableHeader>
            <TableColumn label="Name"/>
            <TableColumn label="Build"/>
            <TableColumn label="Timestamp"/>
        </TableHeader>
    };
    let entries = Artifact::list(10, 0).unwrap();
    let entries = Artifact::formats(entries);
    let model: SharedTableModel<_> = entries.into();
    html! {
        <Table<SharedTableModel<ArtifactDisplay>>
            mode={TableMode::CompactExpandable}
            header={header}
            entries={model}
        />
    }
}
