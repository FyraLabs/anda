use crate::builds::Build;

pub enum Repo {
    RPM {
        id: String,
        builds: Vec<Build>,
    },
    Image {
        id: String,
        builds: Vec<Build>,
    },
    OSTree {
        id: String,
        refs: Vec<String>,
        template: String,
    },
}