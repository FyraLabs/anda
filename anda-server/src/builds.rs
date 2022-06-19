// This code is licensed under the MIT License.
// Copyright (c) 2022 the Ultramarine Project and Fyra Labs.

pub struct Artifact {
    art_type: String,
    path: String,
}


pub struct Build {
    id: u64,
    build_type: String,
    artifacts: Vec<Artifact>,
}

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
