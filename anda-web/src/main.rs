use yew::prelude::*;
pub mod artifacts;
pub mod builds;
pub mod projects;
use log::{debug, info};
use patternfly_yew::{Title, Table};
use reqwasm::http::Request;
use yew_hooks::use_async;

#[function_component(BuildsTable)]
fn builds_table() -> Html {
    let builds = use_state(|| vec![]);
    {
        let builds = builds.clone();
        use_effect_with_deps(
            move |_| {
                let builds = builds.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let fetched_builds = Request::get(&format!("/api/builds/?{}&{}", 10, 0))
                        .send()
                        .await
                        .unwrap();
                    let a = &fetched_builds.text().await.unwrap();
                    info!("{}", a);
                    let fetched_builds = serde_json::from_str::<Vec<builds::Build>>(&a).unwrap();
                    builds.set(fetched_builds);
                });
                || ()
            },
            (),
        );
    }

    html! {
        <div id="builds" class="section">
            <Table<SharedTableModel<builds::Build>> >
                <tr>
                    <th>{ "ID" }</th>
                    <th>{ "Project" }</th>
                    <th>{ "Target" }</th>
                    <th>{ "Status" }</th>
                </tr>
                <tbody>{ builds::Build::format({(*builds).clone()}) }</tbody>
            </Table>
        </div>
    }
}

#[function_component(ArtifactsTable)]
fn artifacts_table() -> Html {
    let artifacts = use_state(|| vec![]);
    {
        let artifacts = artifacts.clone();
        use_effect_with_deps(
            move |_| {
                let artifacts = artifacts.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let fetched_artifacts: Vec<artifacts::Artifact> =
                        Request::get(&format!("/api/artifacts/?{}&{}", 10, 0))
                            .send()
                            .await
                            .unwrap()
                            .json()
                            .await
                            .unwrap();
                    artifacts.set(fetched_artifacts);
                });
                || ()
            },
            (),
        );
    }

    html! {
        <div id="artifacts" class="section">
            <Table>
                <tr>
                    <th>{ "ID" }</th>
                    <th>{ "Name" }</th>
                    <th>{ "Build" }</th>
                    <th>{ "Timestamp" }</th>
                </tr>
                <tbody>{ artifacts::Artifact::format((*artifacts).clone()) }</tbody>
            </Table>
        </div>
    }
}

#[function_component(ProjectsTable)]
fn projects_table() -> Html {
    //let projects = projects::Project::list(10, 0).unwrap_or(vec![]);

    let projects = use_async(async move { projects::Project::list(10, 0).await });

    //let projects = projects::Project::format(projects);

    html! {
        <div id="projects" class="section">
            if projects.loading { <p>{ "Loading..." }</p> }
            if let Some(data) = &projects.data {
                <tr>
                    <th>{ "ID" }</th>
                    <th>{ "Name" }</th>
                    <th>{"Description"}</th>
                </tr>
                <tbody>{ projects::Project::format(data.clone()) }</tbody>
            }
            if let Some(error) = &projects.error { { error } }
        </div>
    }
}

pub enum Msg {
    // BuildReqFinish(Vec<builds::Build>),
    // ArtifactReqFinish(Vec<artifacts::Artifact>),
    // ProjectReqFinish(Vec<projects::Project>)
}
pub struct Main {}

impl Component for Main {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        // match msg {
        //     _ => {}
        // }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        // This gives us a component's "`Scope`" which allows us to send messages, etc to the component.
        // let link = ctx.link();
        //spawn_local();
        //wasm_bindgen_futures::spawn_local(future)
        html! {
            <>
            <Title>{ "Andaman Build System" }</Title>
            <div>
                <Title level={Level::H2}>{ "Builds" }</Title>
                <BuildsTable/>
                <Title level={Level::H2}>{ "Artifacts" }</Title>
                <ArtifactsTable/>
                <Title level={Level::H2}>{ "Projects" }</Title>
                <ProjectsTable/>
            </div>
            </>
        }
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    yew::start_app::<Main>();
}
