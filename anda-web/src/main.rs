use tokio::task::spawn_local;
use yew::prelude::*;
pub mod artifacts;
pub mod builds;
pub mod projects;
use yew_hooks::use_async;
//use wasm_bindgen_futures::spawn_local;

#[function_component(BuildsTable)]
fn builds_table() -> Html {
    //let builds = builds::Build::list(10, 0).unwrap_or(vec![]);

    let builds = use_async(async move { builds::Build::list(10, 0).await });

    //let builds = builds::Build::format(builds);

    html! {
        <div id="builds" class="section">
            if builds.loading {
                <p>{ "Loading..." }</p>
            } else {
                    <></>
            }
            {
                if let Some(data) = &builds.data {
                    html! {
                        <table class="table-auto w-full">
                            <thead>
                                <tr>
                                    <th class="px-4 py-2">{ "ID" }</th>
                                    <th class="px-4 py-2">{ "Project" }</th>
                                    <th class="px-4 py-2">{"Target"}</th>
                                    <th class="px-4 py-2">{"Status"}</th>
                                </tr>
                            </thead>
                            <tbody>

                            {
                                builds::Build::format(data.clone())
                            }

                            </tbody>
                        </table>
                    }
                } else {
                    html! {
                        <></>
                    }
                }
            }
            {
                if let Some(error) = &builds.error {
                    html! { error }
                } else {
                    html! {
                        <></>
                    }
                }
            }
        </div>
    }


        /* <table>
                    <tr>
                        <th>{ "ID" }</th>
                        <th>{ "Project" }</th>
                        <th>{ "Target" }</th>
                        <th>{ "Status" }</th>
                    </tr>
                        { builds }
        </table> */
}

#[function_component(ArtifactsTable)]
fn artifacts_table() -> Html {
    //let artifacts = artifacts::Artifact::list(10, 0).unwrap_or(vec![]);

    let artifacts = use_async(async move { artifacts::Artifact::list(10, 0).await });

    //let artifacts = artifacts::Artifact::format(artifacts);

    html! {
        <div id="artifacts" class="section">
            if artifacts.loading {
                <p>{ "Loading..." }</p>
            } else {
                    <></>
            }
            {
                if let Some(data) = &artifacts.data {
                    html! {
                        <table class="table-auto w-full">
                            <thead>
                                <tr>
                                    <th class="px-4 py-2">{ "ID" }</th>
                                    <th class="px-4 py-2">{ "Name" }</th>
                                    <th class="px-4 py-2">{"Build"}</th>
                                    <th class="px-4 py-2">{"Timestamp"}</th>
                                </tr>
                            </thead>
                            <tbody>

                            {
                                artifacts::Artifact::format(data.clone())
                            }

                            </tbody>
                        </table>
                    }
                } else {
                    html! {
                        <></>
                    }
                }
            }
            {
                if let Some(error) = &artifacts.error {
                    html! { error }
                } else {
                    html! {
                        <></>
                    }
                }
            }
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
            if projects.loading {
                <p>{ "Loading..." }</p>
            } else {
                    <></>
            }
            {
                if let Some(data) = &projects.data {
                    html! {
                        <table class="table-auto w-full">
                            <thead>
                                <tr>
                                    <th class="px-4 py-2">{ "ID" }</th>
                                    <th class="px-4 py-2">{ "Name" }</th>
                                    <th class="px-4 py-2">{"Description"}</th>
                                </tr>
                            </thead>
                            <tbody>

                            {
                                projects::Project::format(data.clone())
                            }

                            </tbody>
                        </table>
                    }
                } else {
                    html! {
                        <></>
                    }
                }
            }
            {
                if let Some(error) = &projects.error {
                    html! { error }
                } else {
                    html! {
                        <></>
                    }
                }
            }
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
            
            <div>
            
            <h1 class="self-center w-full">{ "Andaman Build System" }</h1>
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
            <BuildsTable/>
            </div>
        }
    }
}
#[tokio::main]
async fn main() {
    yew::start_app::<Main>();
}
