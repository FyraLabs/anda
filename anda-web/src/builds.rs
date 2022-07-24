use log::info;
use patternfly_yew::{
    ColumnIndex, SharedTableModel, Table, TableColumn, TableHeader, TableMode, TableRenderer,
};
use reqwasm::http::Request;
use serde::Deserialize;
use uuid::Uuid;
use yew::prelude::*;

#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub(crate) struct Build {
    pub id: Uuid,
    #[serde(rename = "project_id")]
    pub proj: Option<Uuid>,
    pub status: usize,
    #[serde(rename = "target_id")]
    pub tag: Option<Uuid>,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub(crate) struct BuildDisplay {
    pub id: String,
    #[serde(rename = "project_id")]
    pub proj: String,
    #[serde(rename = "target_id")]
    pub status: usize,
    pub tag: String,
}

impl Build {
    pub(crate) fn list(
        limit: usize,
        page: usize,
    ) -> Result<UseStateHandle<Vec<Self>>, Box<dyn std::error::Error>> {
        let builds = use_state(|| vec![]);
        {
            let builds = builds.clone();
            use_effect_with_deps(
                move |_| {
                    let builds = builds.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let fetched_builds =
                            Request::get(&format!("/api/builds/?{}&{}", limit, page))
                                .send()
                                .await
                                .unwrap();
                        let a = &fetched_builds.text().await.unwrap();
                        info!("{}", a);
                        match serde_json::from_str::<Vec<Self>>(&a) {
                            Ok(fetched_builds) => builds.set(fetched_builds),
                            Err(err) => panic!(
                                "Failed to parse fetched builds; is the API down?\n{:?}",
                                err
                            ),
                        }
                    });
                    || ()
                },
                (),
            );
        }
        Ok(builds)
    }

    pub(crate) fn formats(builds: UseStateHandle<Vec<Build>>) -> Vec<BuildDisplay> {
        builds.iter().map(|b| Self::format(b)).collect()
    }
    pub(crate) fn format(build: &Build) -> BuildDisplay {
        // unwrap project_id and then simplify it, or use a blank string if it's None
        BuildDisplay {
            id: build.id.as_simple().to_string(),
            proj: build
                .proj
                .map_or("".to_string(), |p| p.simple().to_string()),
            status: build.status,
            tag: build.tag.map_or("".to_string(), |t| t.simple().to_string()),
        }
    }
}

impl TableRenderer for BuildDisplay {
    fn render(&self, column: ColumnIndex) -> Html {
        // let entry = Build::format(self.clone());
        let entry = self;
        match column.index {
            0 => html! { <a href={ format!("/b/{}", &entry.id) }>{ &entry.id }</a> },
            1 => html! { { &entry.proj } },
            2 => html! { { &entry.status } },
            3 => html! { { &entry.tag } },
            _ => html! {},
        }
    }
    // fn render_details(&self) -> Vec<Span> {
    //     vec![Span::max(html! {
    //         <>
    //             { "So many details for " }{ "idk" }
    //         </>
    //     })]
    // }
}

#[function_component(Builds)]
pub(crate) fn builds() -> Html {
    let header = html_nested! {
        <TableHeader>
            <TableColumn label="ID"/>
            <TableColumn label="Project"/>
            <TableColumn label="Status"/>
            <TableColumn label="Target"/>
        </TableHeader>
    };
    let entries = Build::list(10, 0).unwrap();
    let entries = Build::formats(entries);
    let model: SharedTableModel<_> = entries.into();
    html! {
        <Table<SharedTableModel<BuildDisplay>>
            mode={TableMode::CompactExpandable}
            header={header}
            entries={model}
        />
    }
}
