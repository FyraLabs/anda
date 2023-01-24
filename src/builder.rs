use crate::{
    artifacts::Artifacts,
    cli::{Cli, FlatpakOpts, OciOpts, PackageType, RpmOpts},
    flatpak::{FlatpakArtifact, FlatpakBuilder},
    oci::{build_oci, OCIBackend},
    rpm_spec::{RPMBuilder, RPMExtraOptions, RPMOptions},
    update::run_scripts,
    util::{get_commit_id_cwd, get_date},
};
use anda_config::{Docker, Flatpak, Project, RpmBuild};
use cmd_lib::run_cmd;
use color_eyre::{eyre::eyre, eyre::Context, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tracing::{debug, error, info, trace};

pub async fn build_rpm(
    opts: &mut RPMOptions,
    spec: &Path,
    builder: RPMBuilder,
    output_dir: &Path,
    rpmb_opts: &RpmOpts,
) -> Result<Vec<PathBuf>> {
    let repo_path = output_dir.join("rpm");
    println!("Building RPMs in {}", repo_path.display());
    let repodata_path = repo_path.join("repodata");

    if repodata_path.exists() {
        let repo_path = repo_path.canonicalize()?;

        let repo_path = format!("file://{}", repo_path.display());
        if opts.extra_repos.is_none() {
            opts.extra_repos = Some(vec![repo_path]);
        } else {
            opts.extra_repos.as_mut().unwrap().push(repo_path);
        }
    } else {
        debug!("No repodata found, skipping");
    }

    for repo in &rpmb_opts.extra_repos {
        if opts.extra_repos.is_none() {
            opts.extra_repos = Some(vec![repo.clone()]);
        } else {
            opts.extra_repos.as_mut().unwrap().push(repo.clone());
        }
    }

    for rpmmacro in &rpmb_opts.rpm_macro {
        let split = rpmmacro.split_once(' ');
        if let Some((key, value)) = split {
            opts.def_macro(key, value);
        } else {
            return Err(eyre!("Invalid rpm macro: {rpmmacro}"));
        }
    }
    {
        // HACK: Define macro for autogitversion
        // get git version
        let commit_id = get_commit_id_cwd();

        let date = get_date();
        let mut tmp = String::new();

        let autogitversion = commit_id
            .as_ref()
            .map(|commit| {
                tmp = format!("{date}.{}", commit.chars().take(8).collect::<String>());
                &tmp
            })
            .unwrap_or(&date);

        // limit to 16 chars

        opts.def_macro("autogitversion", autogitversion);

        opts.def_macro("autogitcommit", &commit_id.unwrap_or_else(|| "unknown".into()));

        opts.def_macro("autogitdate", &date);
    }

    trace!("Building RPMs with {opts:?}");

    let builder = builder.build(spec, opts).await;

    run_cmd!(createrepo_c --quiet --update ${repo_path})?;

    builder
}

pub async fn build_flatpak(
    output_dir: &Path,
    manifest: &Path,
    flatpak_opts: &mut FlatpakOpts,
) -> Result<Vec<FlatpakArtifact>> {
    let mut artifacts = Vec::new();

    let out = output_dir.join("flatpak");

    let flat_out = out.join("build");
    let flat_repo = out.join("repo");
    let flat_bundles = out.join("bundles");

    let mut builder = FlatpakBuilder::new(flat_out, flat_repo, flat_bundles);

    for extra_source in flatpak_opts.flatpak_extra_sources.iter_mut() {
        builder.add_extra_source(PathBuf::from(std::mem::take(extra_source)));
    }

    for extra_source_url in flatpak_opts.flatpak_extra_sources_url.iter_mut() {
        builder.add_extra_source_url(std::mem::take(extra_source_url));
    }

    if !flatpak_opts.flatpak_dont_delete_build_dir {
        builder.add_extra_args("--delete-build-dirs".to_string());
    }

    let flatpak = builder.build(manifest).await?;
    artifacts.push(FlatpakArtifact::Ref(flatpak.clone()));
    artifacts.push(FlatpakArtifact::Bundle(builder.bundle(&flatpak).await?));

    Ok(artifacts)
}

macro_rules! script {
    ($name:expr, $scr:expr, $( $var:ident ),*) => {
        let sc = andax::run(
            $name,
            &$scr,
            std::collections::BTreeMap::new(),
            |_sc| {
                $( _sc.push(stringify!($var), $var); )*
            },
        );
        #[allow(unused_assignments)]
        if let Some(_sc) = sc {
            $( $var = _sc.get_value(stringify!($var)).expect(concat!("No `{}` in scope", stringify!($var))); )*
        } else {
            error!(
                scr = $scr.display().to_string(),
                concat!(stringify!($scr), " —— failed with aforementioned exception.")
            );
            return Err(eyre!(concat!(stringify!($scr), " failed")));
        }
    };
}

// Functions to actually call the builds
// yeah this is ugly and relies on side effects, but it reduces code duplication
// to anyone working on this, please rewrite this call to make it more readable
pub async fn build_rpm_call(
    cli: &Cli,
    mut opts: RPMOptions,
    rpmbuild: &RpmBuild,
    mut rpm_builder: RPMBuilder,
    artifact_store: &mut Artifacts,
    rpmb_opts: &RpmOpts,
) -> Result<()> {
    // run pre-build script
    if let Some(pre_script) = &rpmbuild.pre_script {
        if pre_script.extension().unwrap_or_default() == ".rhai" {
            script!(
                rpmbuild.spec.as_os_str().to_str().unwrap_or(""),
                pre_script,
                opts,
                rpm_builder
            );
        } else {
            let p = Command::new("sh").arg("-c").arg(pre_script).status()?;
            if !p.success() {
                return Err(eyre!(p));
            }
        }
    }

    let art = build_rpm(&mut opts, &rpmbuild.spec, rpm_builder, &cli.target_dir, rpmb_opts).await?;

    // `opts` is consumed in build_rpm()/build()
    if let Some(post_script) = &rpmbuild.post_script {
        if post_script.extension().unwrap_or_default() == ".rhai" {
            script!(
                rpmbuild.spec.as_os_str().to_str().unwrap_or(""),
                post_script,
                opts,
                rpm_builder
            );
        } else {
            let p = Command::new("sh").arg("-c").arg(post_script).status()?;
            if !p.success() {
                return Err(eyre!(p));
            }
        }
    }

    for artifact in art {
        artifact_store.add(artifact.to_string_lossy().to_string(), PackageType::Rpm);
    }

    Ok(())
}

pub async fn build_flatpak_call(
    cli: &Cli,
    flatpak: &Flatpak,
    artifact_store: &mut Artifacts,
    mut flatpak_opts: FlatpakOpts,
) -> Result<()> {
    if let Some(pre_script) = &flatpak.pre_script {
        script!(
            flatpak.manifest.as_path().to_str().unwrap_or("<flatpak>"),
            pre_script,
            flatpak_opts
        );
    }

    let art = build_flatpak(&cli.target_dir, &flatpak.manifest, &mut flatpak_opts).await.unwrap();

    for artifact in art {
        artifact_store.add(artifact.to_string(), PackageType::Flatpak);
    }

    if let Some(post_script) = &flatpak.post_script {
        script!(flatpak.manifest.as_path().to_str().unwrap_or("<flatpak>"), post_script,);
    }

    Ok(())
}

pub fn build_oci_call(
    backend: OCIBackend,
    _cli: &Cli,
    manifest: &mut Docker,
    artifact_store: &mut Artifacts,
) -> Result<()> {
    let art_type = match backend {
        OCIBackend::Docker => PackageType::Docker,
        OCIBackend::Podman => PackageType::Podman,
    };

    for (tag, image) in std::mem::take(&mut manifest.image).into_iter() {
        let art = build_oci(
            backend,
            image.dockerfile.unwrap(),
            image.tag_latest.unwrap_or(false),
            tag,
            image.version.unwrap_or_else(|| "latest".to_string()),
            image.context,
        );

        for artifact in art {
            artifact_store.add(artifact.to_string(), art_type);
        }
    }

    Ok(())
}

// project parser

pub async fn build_project(
    cli: &Cli,
    project: Project,
    package: PackageType,
    rpmb_opts: &RpmOpts,
    flatpak_opts: &FlatpakOpts,
    _oci_opts: &OciOpts,
) -> Result<()> {
    let cwd = std::env::current_dir().unwrap();

    let mut rpm_opts = RPMOptions::new(rpmb_opts.mock_config.clone(), cwd, cli.target_dir.clone());

    if let Some(pre_script) = &project.pre_script {
        script!(
            "pre_script",
            pre_script,
        );
    }

    if let Some(rpmbuild) = &project.rpm {
        if let Some(srcdir) = &rpmbuild.sources {
            rpm_opts.sources = srcdir.to_path_buf();
        }
        rpm_opts.no_mirror = rpmb_opts.no_mirrors;
        rpm_opts.def_macro("_disable_source_fetch", "0");
        rpm_opts.config_opts.push("external_buildrequires=True".to_string());

        // Enable SCM sources
        if let Some(bool) = rpmbuild.enable_scm {
            rpm_opts.scm_enable = bool;
        }

        // load SCM options
        if let Some(scm_opt) = &rpmbuild.scm_opts {
            rpm_opts.scm_opts =
                scm_opt.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<String>>();
        }

        // load extra config options

        if let Some(cfg) = &rpmbuild.config {
            rpm_opts
                .config_opts
                .extend(cfg.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<String>>());
        }

        // Plugin opts for RPM, contains some extra plugin options, with some special
        // characters like `:`
        if let Some(plugin_opt) = &rpmbuild.plugin_opts {
            rpm_opts.plugin_opts =
                plugin_opt.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<String>>();
        }

        if rpmb_opts.mock_config.is_none() {
            if let Some(mockcfg) = &rpmbuild.mock_config {
                rpm_opts.mock_config = Some(mockcfg.to_string());
            }
            // TODO: Implement global settings
        }
    }
    let mut artifacts = Artifacts::new();

    // get project
    match package {
        PackageType::All => {
            // build all packages
            if let Some(rpmbuild) = &project.rpm {
                build_rpm_call(
                    cli,
                    rpm_opts,
                    rpmbuild,
                    rpmb_opts.rpm_builder.into(),
                    &mut artifacts,
                    rpmb_opts,
                )
                .await
                .with_context(|| "Failed to build RPMs".to_string())?;
            }
            if let Some(flatpak) = &project.flatpak {
                build_flatpak_call(cli, flatpak, &mut artifacts, flatpak_opts.clone())
                    .await
                    .with_context(|| "Failed to build Flatpaks".to_string())?;
            }

            if let Some(mut podman) = project.podman {
                build_oci_call(OCIBackend::Podman, cli, &mut podman, &mut artifacts)
                    .with_context(|| "Failed to build Podman images".to_string())?;
            }

            if let Some(mut docker) = project.docker {
                build_oci_call(OCIBackend::Docker, cli, &mut docker, &mut artifacts)
                    .with_context(|| "Failed to build Docker images".to_string())?;
            }
            if let Some(scripts) = &project.scripts {
                info!("Running build scripts");
                run_scripts(
                    scripts
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect::<Vec<String>>()
                        .as_slice(),
                    project.labels,
                )?;
            }
        }
        PackageType::Rpm => {
            if let Some(rpmbuild) = &project.rpm {
                build_rpm_call(
                    cli,
                    rpm_opts,
                    rpmbuild,
                    rpmb_opts.rpm_builder.into(),
                    &mut artifacts,
                    rpmb_opts,
                )
                .await
                .with_context(|| "Failed to build RPMs".to_string())?;
            } else {
                println!("No RPM build defined for project");
            }
        }
        PackageType::Docker => {
            if let Some(mut docker) = project.docker {
                build_oci_call(OCIBackend::Docker, cli, &mut docker, &mut artifacts)
                    .with_context(|| "Failed to build Docker images".to_string())?;
            } else {
                println!("No Docker build defined for project");
            }
        }
        PackageType::Podman => {
            if let Some(mut podman) = project.podman {
                build_oci_call(OCIBackend::Podman, cli, &mut podman, &mut artifacts)
                    .with_context(|| "Failed to build Podman images".to_string())?;
            } else {
                println!("No Podman build defined for project");
            }
        }
        PackageType::Flatpak => {
            if let Some(flatpak) = &project.flatpak {
                build_flatpak_call(cli, flatpak, &mut artifacts, flatpak_opts.clone())
                    .await
                    .with_context(|| "Failed to build Flatpaks".to_string())?;
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

    if let Some(post_script) = &project.post_script {
        script!(
            "post_script",
            post_script,
        );
    }

    Ok(())
}

pub async fn builder(
    cli: &Cli,
    rpm_opts: RpmOpts,
    all: bool,
    project: Option<String>,
    package: PackageType,
    flatpak_opts: FlatpakOpts,
    oci_opts: OciOpts,
) -> Result<()> {
    // Parse the project manifest
    // todo
    // ? can we assume cli.config won't be modified?
    let config = anda_config::load_from_file(&cli.config.clone())?;
    trace!("all: {all}");
    trace!("project: {project:?}");
    trace!("package: {package:?}");
    if all {
        for (name, project) in config.project {
            println!("Building project: {}", name);
            build_project(cli, project, package, &rpm_opts, &flatpak_opts, &oci_opts).await?;
        }
    } else {
        // find project named project
        if let Some(name) = project {
            if let Some(project) = config.get_project(&name) {
                // cannot take: get_project() returns immut ref
                build_project(cli, project.clone(), package, &rpm_opts, &flatpak_opts, &oci_opts)
                    .await?;
            } else {
                return Err(eyre!("Project not found: {name}"));
            }
        } else {
            return Err(eyre!("No project specified"));
        }
    }
    Ok(())
}
