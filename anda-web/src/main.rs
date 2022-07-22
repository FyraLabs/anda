use yew::prelude::*;
pub mod artifacts;
pub mod builds;
pub mod projects;
use reqwasm::http::Request;
use yew_hooks::use_async;
//use wasm_bindgen_futures::spawn_local;
use log::{info, debug};

#[function_component(BuildsTable)]
fn builds_table() -> Html {
    let mut loading = false;
    let builds = use_state(|| vec![]);
    {
        let builds = builds.clone();
        use_effect_with_deps(
            move |_| {
                let builds = builds.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let fetched_builds =
                        Request::get(&format!("/api/builds/?{}&{}", 10, 0))
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
                <table class="table-auto w-full">
                    <tr>
                        <th class="px-4 py-2">{ "ID" }</th>
                        <th class="px-4 py-2">{ "Project" }</th>
                        <th class="px-4 py-2">{ "Target" }</th>
                        <th class="px-4 py-2">{ "Status" }</th>
                    </tr>
                    <tbody>{ builds::Build::format({(*builds).clone()}) }</tbody>
                </table>
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
                <table class="table-auto w-full">
                    <tr>
                        <th class="px-4 py-2">{ "ID" }</th>
                        <th class="px-4 py-2">{ "Name" }</th>
                        <th class="px-4 py-2">{ "Build" }</th>
                        <th class="px-4 py-2">{ "Timestamp" }</th>
                    </tr>
                    <tbody>{ artifacts::Artifact::format((*artifacts).clone()) }</tbody>
                </table>
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
                <table class="table-auto w-full">
                    <tr>
                        <th class="px-4 py-2">{ "ID" }</th>
                        <th class="px-4 py-2">{ "Name" }</th>
                        <th class="px-4 py-2">{"Description"}</th>
                    </tr>
                    <tbody>{ projects::Project::format(data.clone()) }</tbody>
                </table>
            }
            if let Some(error) = &projects.error { { error } }
        </div>
    }
}

pub enum Msg {}
pub struct Main {}

impl Component for Main {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        // match msg {
        //     Msg::AddOne => {
        //         self.value += 1;
        //         // the value has changed so we need to
        //         // re-render for it to appear on the page
        //         true
        //     }
        // }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        // This gives us a component's "`Scope`" which allows us to send messages, etc to the component.
        // let link = ctx.link();
        //spawn_local();
        //wasm_bindgen_futures::spawn_local(future)
        html! {
            // <div>
            //     <button onclick={link.callback(|_| Msg::AddOne)}>{ "+1" }</button>
            //     <p>{ self.value }</p>
            // </div>
            <>
            <h1 class="self-center w-full">{ "Andaman Build System" }</h1>
            <div>
                <h1>{"Builds"}</h1>
                <BuildsTable/>
                /* <div id="artifacts" class="section">
                    <h2>{ "Artifacts" }</h2>
                    <table>
                        <tr>
                            <th>{ "ID" }</th>
                            <th>{ "Name" }</th>
                            <th>{ "Build" }</th>
                            <th>{ "Timestamp" }</th>
                        </tr>
                        { artifacts }
                    </table>
                </div> */
                <h1>{"Artifacts"}</h1>
                <ArtifactsTable/>
                /* <div id="projects" class="section">
                    <h2>{ "Projects" }</h2>
                    <table>
                        <tr>
                            <th>{ "ID" }</th>
                            <th>{ "Name" }</th>
                            <th>{ "Description" }</th>
                        </tr>
                        { projects }
                    </table>
                </div> */
                <h1>{"Projects"}</h1>
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
