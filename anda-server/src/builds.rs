// This code is licensed under the MIT License.
// Copyright (c) 2022 the Ultramarine Project and Fyra Labs.

use crate::entity::prelude::*;

#[derive(Debug, Clone)]
pub struct Artifact {
    pub art_type: String,
    pub id: String,
    pub build_id: i32,
    pub name: String,
    pub timestamp: i32,
}


#[derive(Clone, Debug)]
pub struct Build {
    pub id: i32,
    pub name: String,
    pub package_id: Option<i32>,
    pub artifacts: Vec<Artifact>,
    pub build_type: String,
    pub owner: Option<String>,
    pub version: String,
    pub timestamp: i32,
    pub target_id: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct Package {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub builds: Vec<Build>,
    pub latest_build: Option<Build>,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: i32,
    pub parent: Option<i32>,
    pub task_type: String,
    pub worker: Option<String>,
}


#[derive(Clone, Debug)]
pub struct Compose {
    pub id: String,
    pub target_id: i32,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: String,
    pub packages: Vec<Package>,
    pub external_repos: Vec<String>,
}