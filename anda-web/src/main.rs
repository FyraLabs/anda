use yew::prelude::*;

enum Msg {
    AddOne,
}

struct Model {
    value: i64,
}

struct Build {
    id: String,
    proj: String,
    tag: String,
    status: String,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self { value: 0 }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::AddOne => {
                self.value += 1;
                // the value has changed so we need to
                // re-render for it to appear on the page
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        // This gives us a component's "`Scope`" which allows us to send messages, etc to the component.
        let link = ctx.link();
        let builds: Vec<Build> = vec![]; // get builds here
        let builds = builds
            .iter()
            .map(|b| {
                html! {
                    <tr>
                        <th>{ &b.id }</th>
                        <th>{ &b.proj }</th>
                        <th>{ &b.tag }</th>
                        <th>{ &b.status }</th>
                    </tr>
                }
            })
            .collect::<Html>();
        html! {
            // <div>
            //     <button onclick={link.callback(|_| Msg::AddOne)}>{ "+1" }</button>
            //     <p>{ self.value }</p>
            // </div>
            <>
            <h1>{ "Andaman Build System" }</h1>
            <div id="builds" class="section">
                <h2>{ "Builds" }</h2>
                <table>
                    <tr>
                        <th>{ "ID" }</th>
                        <th>{ "Project" }</th>
                        <th>{ "Target" }</th>
                        <th>{ "Status" }</th>
                    </tr>
                    { builds }
                </table>
            </div>
            // <div id="artifacts" class="section">
            //     <h2>Artifacts</h2>
            //     <table>
            //         <tr>
            //             <th>Name</th>
            //             <th>Build</th>
            //             <th>Time</th>
            //         </tr>
            //         {{#each artifacts ~}}
            //         <tr>
            //             <td><a href="/a/{{id}}">{{name}}</a></td>
            //             <td>{{build}}</td>
            //             <td>{{time}}</td>
            //         </tr>
            //         {{~/each}}
            //     </table>
            // </div>
            // <div id="projects" class="section">
            //     <h2>Project</h2>
            //     <table>
            //         <tr>
            //             <th>Name</th>
            //             <th>Description</th>
            //         </tr>
            //         {{#each projects ~}}
            //         <tr>
            //             <td><a href="/p/{{id}}">{{name}}</a></td>
            //             <td>{{description}}</td>
            //         </tr>
            //         {{~/each}}
            </>
        }
    }
}

fn main() {
    yew::start_app::<Model>();
}
