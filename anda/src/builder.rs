use crate::{
    artifacts::{Artifacts, PackageType},
    rpm_spec::{RPMBuilder, RPMOptions, RPMExtraOptions}, Cli,
};
use anda_config::{AndaConfig, Project};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub fn build_rpm(opts: RPMOptions, spec: &Path, builder: RPMBuilder) -> Result<Vec<PathBuf>> {
    builder.build(spec, &opts)
}

// project parser

pub fn build_project(
    cli: &Cli,
    project: Project,
    package: PackageType,
    no_mirrors: bool,
    rpm_builder: RPMBuilder,
    mock_config: Option<String>,
) {
    let cwd = std::env::current_dir().unwrap();

    let mut rpm_opts = RPMOptions::new(mock_config, cwd,cli.target_dir.clone());

    if let Some(rpmbuild) = &project.rpm {
        if let Some(srcdir) = &rpmbuild.sources {
            rpm_opts.sources = srcdir.to_path_buf();
        }
        rpm_opts.no_mirror = no_mirrors;
        rpm_opts.def_macro("_disable_source_fetch", "0");
    }

    let mut artifacts = Artifacts::new();

    // get project
    match package {
        PackageType::All => {
            // build all packages
            if let Some(rpmbuild) = &project.rpm {
                let art = build_rpm(rpm_opts, &rpmbuild.spec, rpm_builder).unwrap();

                for artifact in art {
                    artifacts.add(artifact.to_string_lossy().to_string(), PackageType::Rpm);
                }
            }
        },
        PackageType::Rpm => {
            if let Some(rpmbuild) = &project.rpm {
                build_rpm(rpm_opts, &rpmbuild.spec, rpm_builder).unwrap();
            } else {
                println!("No RPM build defined for project");
            }
        },
        PackageType::Docker => todo!(),
        PackageType::Podman => todo!(),
        PackageType::Flatpak => todo!(),
        PackageType::RpmOstree => todo!(),
    }

    for (path, arttype) in artifacts.packages {
        let type_string = match arttype {
            PackageType::Rpm => "RPM",
            PackageType::Docker => "Docker image",
            PackageType::Podman => "Podman image",
            PackageType::Flatpak => "flatpak",
            PackageType::RpmOstree => "rpm-ostree compose",
            _ => "unknown artifact",
        };

        println!("Built {}: {}", type_string, path);
    }
}

pub fn builder(
    cli: &Cli,
    all: bool,
    project: Option<String>,
    package: PackageType,
    no_mirrors: bool,
    rpm_builder: RPMBuilder,
    mock_config: Option<String>,
) -> Result<()>{
    // Parse the project manifest
    let config = anda_config::load_from_file(&cli.config.clone()).map_err(|e| anyhow!(e))?;
    println!("Build command");
    println!("all: {}", all);
    println!("project: {:?}", project);
    println!("package: {:?}", package);
    if all {
        for (name, project) in config.project {
            println!("Building project: {}", name);
            build_project(cli,project, package, no_mirrors, rpm_builder, mock_config.clone());
        }
    } else {
        // find project named project
        if let Some(name) = project {
            if let Some(project) = config.project.get(&name) {
                build_project(cli,project.clone(), package, no_mirrors, rpm_builder, mock_config);
            } else {
                return Err(anyhow!("Project not found: {}", name));
            }
        } else {
            return Err(anyhow!("No project specified"));
        }
    }
    Ok(())
}
