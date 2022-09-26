use crate::{
    artifacts::{Artifacts, PackageType},
    flatpak::{FlatpakArtifact, FlatpakBuilder},
    oci::{build_oci, OCIBackend},
    rpm_spec::{RPMBuilder, RPMExtraOptions, RPMOptions},
    Cli, RpmOpts, OciOpts, FlatpakOpts,
};
use anda_config::{Project, RpmBuild, Flatpak, Docker};
use anyhow::{anyhow, Result, Context};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use cmd_lib::{run_cmd, run_fun};

pub fn build_rpm(
    opts: RPMOptions,
    spec: &Path,
    builder: RPMBuilder,
    output_dir: &Path,
    rpmb_opts: RpmOpts,
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

    for rpmmacro in rpmb_opts.rpm_macro {
        let split = rpmmacro.split_once(' ');
        if let Some((key, value)) = split {
            opts2.def_macro(key, value);
        } else {
            return Err(anyhow!("Invalid rpm macro: {}", rpmmacro));
        }
    }

    println!("Building RPMs with {:?}", opts2);

    let builder = builder.build(spec, &opts2);
    // createrepo at the end builder
    // let mut createrepo = Command::new("createrepo_c");
    // createrepo.arg(&repo_path).arg("--quiet").arg("--update");

    let a = run_cmd!(createrepo_c --quiet --update ${repo_path});

    a.map_err(|e| anyhow!(e))?;
    // createrepo.status().map_err(|e| anyhow!(e))?;

    //println!("builder: {:?}", builder);

    builder
}

pub fn build_flatpak(output_dir: &Path, manifest: &Path, flatpak_opts: FlatpakOpts) -> Result<Vec<FlatpakArtifact>> {
    let mut artifacts = Vec::new();

    let out = output_dir.join("flatpak");

    let flat_out = out.join("build");
    let flat_repo = out.join("repo");
    let flat_bundles = out.join("bundles");

    let mut builder = FlatpakBuilder::new(flat_out, flat_repo, flat_bundles);

    for extra_source in flatpak_opts.flatpak_extra_sources {
        builder.add_extra_source(PathBuf::from(extra_source));
    }

    for extra_source_url in flatpak_opts.flatpak_extra_sources_url {
        builder.add_extra_source_url(extra_source_url);
    }

    if !flatpak_opts.flatpak_dont_delete_build_dir {
        builder.add_extra_args("--delete-build-dirs".to_string());
    }


    let flatpak = builder.build(manifest)?;
    artifacts.push(FlatpakArtifact::Ref(flatpak.clone()));
    artifacts.push(FlatpakArtifact::Bundle(builder.bundle(&flatpak)?));

    Ok(artifacts)
}

// Functions to actually call the builds
// yeah this is ugly and relies on side effects, but it reduces code duplication
// to anyone working on this, please rewrite this call to make it more readable
pub fn build_rpm_call(
    cli: &Cli,
    opts: RPMOptions,
    rpmbuild: &RpmBuild,
    rpm_builder: RPMBuilder,
    artifact_store: &mut Artifacts,
    rpmb_opts: RpmOpts,
) -> Result<()> {
    // run pre-build script
    if let Some(pre_script) = &rpmbuild.pre_script {
        for script in pre_script.commands.iter() {
            let mut cmd = Command::new("sh");
            cmd.arg("-x").arg("-c").arg(script);
            cmd.status().map_err(|e| anyhow!(e))?;
        }
    }

    let art = build_rpm(opts, &rpmbuild.spec, rpm_builder, &cli.target_dir, rpmb_opts)?;


    // run post-build script
    if let Some(post_script) = &rpmbuild.post_script {
        for script in post_script.commands.iter() {
            let mut cmd = Command::new("sh");
            cmd.arg("-x").arg("-c").arg(script);
            cmd.status().map_err(|e| anyhow!(e))?;
        }
    }

    for artifact in art {
        artifact_store.add(artifact.to_string_lossy().to_string(), PackageType::Rpm);
    }

    Ok(())
}

pub fn build_flatpak_call(
    cli: &Cli,
    flatpak: &Flatpak,
    artifact_store: &mut Artifacts,
    flatpak_opts: FlatpakOpts,
) -> Result<()> {
    if let Some(pre_script) = &flatpak.pre_script {
        for script in pre_script.commands.iter() {
            let mut cmd = Command::new("sh");
            cmd.arg("-x").arg("-c").arg(script);
            cmd.status().map_err(|e| anyhow!(e))?;
        }
    }

    let art = build_flatpak(&cli.target_dir, &flatpak.manifest, flatpak_opts).unwrap();

    for artifact in art {
        artifact_store.add(artifact.to_string(), PackageType::Flatpak);
    }

    if let Some(post_script) = &flatpak.post_script {
        for script in post_script.commands.iter() {
            let mut cmd = Command::new("sh");
            cmd.arg("-x").arg("-c").arg(script);
            cmd.status().map_err(|e| anyhow!(e))?;
        }
    }

    Ok(())
}


pub fn build_oci_call(
    backend: OCIBackend,
    cli: &Cli,
    manifest: &Docker,
    artifact_store: &mut Artifacts,
) -> Result<()> {

    let art_type = match backend {
        OCIBackend::Docker => PackageType::Docker,
        OCIBackend::Podman => PackageType::Podman,
    };

    for (tag,image) in &manifest.image {
        let art = build_oci(
            backend,
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
            artifact_store.add(artifact.to_string(), art_type);
        }
    }

    Ok(())
}


// project parser

pub fn build_project(
    cli: &Cli,
    project: Project,
    package: PackageType,
    rpmb_opts: RpmOpts,
    flatpak_opts: FlatpakOpts,
    oci_opts: OciOpts,
) {
    let cwd = std::env::current_dir().unwrap();

    let mut rpm_opts = RPMOptions::new(rpmb_opts.clone().mock_config, cwd, cli.target_dir.clone());

    if let Some(rpmbuild) = &project.rpm {
        if let Some(srcdir) = &rpmbuild.sources {
            rpm_opts.sources = srcdir.to_path_buf();
        }
        rpm_opts.no_mirror = rpmb_opts.no_mirrors;
        rpm_opts.def_macro("_disable_source_fetch", "0");
    }

    let mut artifacts = Artifacts::new();

    // get project
    match package {
        PackageType::All => {
            // build all packages
            if let Some(rpmbuild) = &project.rpm {
                build_rpm_call(cli, rpm_opts, rpmbuild, rpmb_opts.rpm_builder, &mut artifacts, rpmb_opts).with_context(|| "Failed to build RPMs".to_string()).unwrap();
            }
            if let Some(flatpak) = &project.flatpak {
                build_flatpak_call(cli, flatpak, &mut artifacts, flatpak_opts).with_context(|| "Failed to build Flatpaks".to_string()).unwrap();
            }

            if let Some(podman) = &project.podman {
                build_oci_call(OCIBackend::Podman, cli, podman, &mut artifacts).with_context(|| "Failed to build Podman images".to_string()).unwrap();
            }

            if let Some(docker) = &project.docker {
                build_oci_call(OCIBackend::Docker, cli, docker, &mut artifacts).with_context(|| "Failed to build Docker images".to_string()).unwrap();
            }
        }
        PackageType::Rpm => {
            if let Some(rpmbuild) = &project.rpm {
                build_rpm_call(cli, rpm_opts, rpmbuild, rpmb_opts.rpm_builder, &mut artifacts, rpmb_opts).with_context(|| "Failed to build RPMs".to_string()).unwrap();
            } else {
                println!("No RPM build defined for project");
            }
        }
        PackageType::Docker => {
            if let Some(docker) = &project.docker {
                build_oci_call(OCIBackend::Docker, cli, docker, &mut artifacts).with_context(|| "Failed to build Docker images".to_string()).unwrap();
            } else {
                println!("No Docker build defined for project");
            }
        }
        PackageType::Podman => {
            if let Some(podman) = &project.podman {
                build_oci_call(OCIBackend::Podman, cli, podman, &mut artifacts).with_context(|| "Failed to build Podman images".to_string()).unwrap();
            } else {
                println!("No Podman build defined for project");
            }
        }
        PackageType::Flatpak => {
            if let Some(flatpak) = &project.flatpak {
                build_flatpak_call(cli, flatpak, &mut artifacts, flatpak_opts).with_context(|| "Failed to build Flatpaks".to_string()).unwrap();
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
    rpm_opts: RpmOpts,
    all: bool,
    project: Option<String>,
    package: PackageType,
    flatpak_opts: FlatpakOpts,
    oci_opts: OciOpts,
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
                rpm_opts.clone(),
                flatpak_opts.clone(),
                oci_opts.clone(),
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
                    rpm_opts,
                    flatpak_opts,
                    oci_opts,
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
