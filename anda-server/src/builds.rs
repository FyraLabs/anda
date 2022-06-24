// This code is licensed under the MIT License.
// Copyright (c) 2022 the Ultramarine Project and Fyra Labs.

#[derive(Debug, Clone)]
pub struct Artifact {
    art_type: String,
    path: String,
}


#[derive(Clone, Debug)]
pub struct Build {
    id: u64,
    build_type: String,
    artifacts: Vec<Artifact>,
}

