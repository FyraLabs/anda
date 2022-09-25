use crate::{
    artifacts::{Artifacts, PackageType},
    flatpak::{FlatpakArtifact, FlatpakBuilder},
    oci::{build_oci, OCIBackend},
    rpm_spec::{RPMBuilder, RPMExtraOptions, RPMOptions},
    Cli,
};
use anda_config::Project;
use anyhow::{anyhow, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn build_rpm(
    opts: RPMOptions,
    spec: &Path,
    builder: RPMBuilder,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let repo_path = output_dir.join("rpm");
    println!("Building RPMs in {}", repo_path.display());
    let repodata_path = repo_path.join("repodata");

    let mut opts2 = opts;

    if repodata_path.exists() {
        let repo_path = repo_path.canonicalize()?;

        let repo_path = format!("file://{}", repo_path.canonicalize().unwrap().display());
        if opts2.extra_repos.is_none() {
            opts2.extra_repos = Some(vec![repo_path]);
        } else {
            opts2.extra_repos.as_mut().unwrap().push(repo_path);
        }
    } else {
        println!("No repodata found, skipping");
    }

    println!("Building RPMs with {:?}", opts2);

    let builder = builder.build(spec, &opts2);
    // createrepo at the end builder
    let mut createrepo = Command::new("createrepo_c");
    createrepo.arg(&repo_path).arg("--quiet").arg("--update");

    createrepo.status().map_err(|e| anyhow!(e))?;

    //println!("builder: {:?}", builder);

    builder
}

pub fn build_flatpak(output_dir: &Path, manifest: &Path) -> Result<Vec<FlatpakArtifact>> {
    let mut artifacts = Vec::new();

    let out = output_dir.join("flatpak");

    let flat_out = out.join("build");
    let flat_repo = out.join("repo");
    let flat_bundles = out.join("bundles");

    let builder = FlatpakBuilder::new(flat_out, flat_repo, flat_bundles);

    let flatpak = builder.build(manifest)?;
    artifacts.push(FlatpakArtifact::Ref(flatpak.clone()));
    artifacts.push(FlatpakArtifact::Bundle(builder.bundle(&flatpak)?));

    Ok(artifacts)
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

    let mut rpm_opts = RPMOptions::new(mock_config, cwd, cli.target_dir.clone());

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
                let art =
                    build_rpm(rpm_opts, &rpmbuild.spec, rpm_builder, &cli.target_dir).unwrap();

                for artifact in art {
                    artifacts.add(artifact.to_string_lossy().to_string(), PackageType::Rpm);
                }
            }
            if let Some(flatpak) = &project.flatpak {
                let art = build_flatpak(&cli.target_dir, &flatpak.manifest).unwrap();
                for artifact in art {
                    artifacts.add(artifact.to_string(), PackageType::Flatpak);
                }
            }

            if let Some(podman) = &project.podman {
                let oci_backend = OCIBackend::Podman;
                for (tag, image) in &podman.image {
                    let art = build_oci(
                        oci_backend,
                        image.dockerfile.as_ref().unwrap().to_string(),
                        image.tag_latest.unwrap_or(false),
                        tag.to_string(),
                        image
                            .version
                            .as_ref()
                            .unwrap_or(&"latest".to_string())
                            .to_string(),
                        image.context.clone(),
                    );
                    for artifact in art {
                        artifacts.add(artifact.to_string(), PackageType::Podman);
                    }
                }
            }

            if let Some(docker) = &project.docker {
                let oci_backend = OCIBackend::Docker;
                for (tag, image) in &docker.image {
                    let art = build_oci(
                        oci_backend,
                        image.dockerfile.as_ref().unwrap().to_string(),
                        image.tag_latest.unwrap_or(false),
                        tag.to_string(),
                        image
                            .version
                            .as_ref()
                            .unwrap_or(&"latest".to_string())
                            .to_string(),
                        image.context.clone(),
                    );
                    for artifact in art {
                        artifacts.add(artifact.to_string(), PackageType::Podman);
                    }
                }
            }
        }
        PackageType::Rpm => {
            if let Some(rpmbuild) = &project.rpm {
                let art =
                    build_rpm(rpm_opts, &rpmbuild.spec, rpm_builder, &cli.target_dir).unwrap();

                for artifact in art {
                    artifacts.add(artifact.to_string_lossy().to_string(), PackageType::Rpm);
                }
            } else {
                println!("No RPM build defined for project");
            }
        }
        PackageType::Docker => {
            if let Some(docker) = &project.docker {
                let oci_backend = OCIBackend::Podman;
                for (tag, image) in &docker.image {
                    let art = build_oci(
                        oci_backend,
                        image.dockerfile.as_ref().unwrap().to_string(),
                        image.tag_latest.unwrap_or(false),
                        tag.to_string(),
                        image
                            .version
                            .as_ref()
                            .unwrap_or(&"latest".to_string())
                            .to_string(),
                        image.context.clone(),
                    );
                    for artifact in art {
                        artifacts.add(artifact.to_string(), PackageType::Docker);
                    }
                }
            } else {
                println!("No Docker build defined for project");
            }
        }
        PackageType::Podman => {
            if let Some(podman) = &project.podman {
                let oci_backend = OCIBackend::Podman;
                for (tag, image) in &podman.image {
                    let art = build_oci(
                        oci_backend,
                        image.dockerfile.as_ref().unwrap().to_string(),
                        image.tag_latest.unwrap_or(false),
                        tag.to_string(),
                        image
                            .version
                            .as_ref()
                            .unwrap_or(&"latest".to_string())
                            .to_string(),
                        image.context.clone(),
                    );
                    for artifact in art {
                        artifacts.add(artifact.to_string(), PackageType::Podman);
                    }
                }
            } else {
                println!("No Podman build defined for project");
            }
        }
        PackageType::Flatpak => {
            if let Some(flatpak) = &project.flatpak {
                let art = build_flatpak(&cli.target_dir, &flatpak.manifest).unwrap();
                for artifact in art {
                    artifacts.add(artifact.to_string(), PackageType::Flatpak);
                }
            } else {
                println!("No Flatpak build defined for project");
            }
        }
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
) -> Result<()> {
    // Parse the project manifest
    let config = anda_config::load_from_file(&cli.config.clone()).map_err(|e| anyhow!(e))?;
    println!("Build command");
    println!("all: {}", all);
    println!("project: {:?}", project);
    println!("package: {:?}", package);
    if all {
        for (name, project) in config.project {
            println!("Building project: {}", name);
            build_project(
                cli,
                project,
                package,
                no_mirrors,
                rpm_builder,
                mock_config.clone(),
            );
        }
    } else {
        // find project named project
        if let Some(name) = project {
            if let Some(project) = config.project.get(&name) {
                build_project(
                    cli,
                    project.clone(),
                    package,
                    no_mirrors,
                    rpm_builder,
                    mock_config,
                );
            } else {
                return Err(anyhow!("Project not found: {}", name));
            }
        } else {
            return Err(anyhow!("No project specified"));
        }
    }
    Ok(())
}
