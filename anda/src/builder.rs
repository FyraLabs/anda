use std::path::PathBuf;
use anyhow::{anyhow, Result};
use crate::{artifacts::{PackageType, Artifacts}, rpm_spec::{RPMOptions, self, RPMBuilder}};

pub fn build_rpm(opts: RPMOptions, spec: &PathBuf, builder: RPMBuilder) -> Result<Vec<PathBuf>> {
    builder.build(spec, &opts)
}

