use crate::{
    artifacts::Artifacts,
    cli::{Cli, FlatpakOpts, OciOpts, PackageType, RpmOpts},
    cmd,
    flatpak::{FlatpakArtifact, FlatpakBuilder},
    oci::{build_oci, OCIBackend},
    rpm_spec::{RPMBuilder, RPMExtraOptions, RPMOptions},
};
use anda_config::{Docker, Flatpak, Project};
use color_eyre::{eyre::eyre, eyre::Context, Result};
use itertools::Itertools;
use std::path::{Path, PathBuf};
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

    opts.set_target(rpmb_opts.rpm_target.clone());

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
        let commit_id = crate::util::get_commit_id_cwd();

        let date = crate::util::get_date();
        let mut tmp = String::new();

        let autogitversion = commit_id.as_ref().map_or(&date, |commit| {
            tmp = format!("{date}.{}", commit.chars().take(8).collect::<String>());
            &tmp
        });

        // limit to 16 chars

        opts.def_macro("autogitversion", autogitversion);

        opts.def_macro("autogitcommit", &commit_id.unwrap_or_else(|| "unknown".into()));

        opts.def_macro("autogitdate", &date);
    };

    trace!("Building RPMs with {opts:?}");

    let builder = builder.build(spec, opts).await?;

    cmd!(? "createrepo_c" "--quiet" "--update" {{repo_path.display()}})?;

    Ok(builder)
}

/// Build a flatpak package.
///
/// # Errors
/// - cannot create bundle
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

    for extra_source in &mut flatpak_opts.extra_sources {
        builder.add_extra_source(PathBuf::from(std::mem::take(extra_source)));
    }

    for extra_source_url in &mut flatpak_opts.extra_sources_url {
        builder.add_extra_source_url(std::mem::take(extra_source_url));
    }

    if !flatpak_opts.dont_delete_build_dir {
        builder.add_extra_args("--delete-build-dirs".to_owned());
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
            std::iter::once(("script_path".to_string(), $scr.to_string_lossy().to_string())),
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
    rpmbuild: &anda_config::RpmBuild,
    mut rpm_builder: RPMBuilder,
    artifact_store: &mut Artifacts,
    rpmb_opts: &RpmOpts,
) -> Result<()> {
    // run pre-build script
    if let Some(pre_script) = &rpmbuild.pre_script {
        if pre_script.extension().unwrap_or_default() == "rhai" {
            script!(
                rpmbuild.spec.as_os_str().to_str().unwrap_or(""),
                pre_script,
                opts,
                rpm_builder
            );
        } else {
            cmd!(? "sh" "-c" {{ pre_script.display() }})?;
        }
    }

    let art = build_rpm(&mut opts, &rpmbuild.spec, rpm_builder, &cli.target_dir, rpmb_opts).await?;

    // `opts` is consumed in build_rpm()/build()
    if let Some(post_script) = &rpmbuild.post_script {
        if post_script.extension().unwrap_or_default() == "rhai" {
            script!(
                rpmbuild.spec.as_os_str().to_str().unwrap_or(""),
                post_script,
                opts,
                rpm_builder
            );
        } else {
            cmd!(? "sh" "-c" {{ post_script.display() }})?;
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
) {
    let art_type = match backend {
        OCIBackend::Docker => PackageType::Docker,
        OCIBackend::Podman => PackageType::Podman,
    };

    for (tag, image) in std::mem::take(&mut manifest.image) {
        let art = build_oci(
            backend,
            &image.dockerfile.unwrap(),
            image.tag_latest.unwrap_or(false),
            &tag,
            &image.version.unwrap_or_else(|| "latest".into()),
            &image.context,
        );

        for artifact in art {
            artifact_store.add(artifact.clone(), art_type);
        }
    }
}

// project parser

pub async fn build_project(
    cli: &Cli,
    mut proj: Project,
    package: PackageType,
    rbopts: &RpmOpts,
    fpopts: &FlatpakOpts,
    _oci_opts: &OciOpts,
) -> Result<()> {
    let cwd = std::env::current_dir().unwrap();

    let mut rpm_opts = RPMOptions::new(rbopts.mock_config.clone(), cwd, cli.target_dir.clone());

    // export environment variables
    if let Some(env) = proj.env.as_ref() {
        env.iter().for_each(|(k, v)| std::env::set_var(k, v));
    }

    if let Some(pre_script) = &proj.pre_script {
        if pre_script.extension().unwrap_or_default() == "rhai" {
            script!("pre_script", pre_script,);
        } else {
            cmd!(? "sh" "-c" {{ pre_script.display() }})?;
        }
    }

    if let Some(rpmbuild) = &proj.rpm {
        rpm_opts.extra_repos = Some(rpmbuild.extra_repos.clone());
        if let Some(srcdir) = &rpmbuild.sources {
            rpm_opts.sources.clone_from(srcdir);
        }
        rpm_opts.no_mirror = rbopts.no_mirrors;
        rpm_opts.def_macro("_disable_source_fetch", "0");
        rpm_opts.config_opts.push("external_buildrequires=True".to_owned());

        if let Some(bool) = rpmbuild.enable_scm {
            rpm_opts.scm_enable = bool;
        }

        if let Some(scm_opt) = &rpmbuild.scm_opts {
            rpm_opts.scm_opts = scm_opt.iter().map(|(k, v)| format!("{k}={v}")).collect();
        }

        if let Some(cfg) = &rpmbuild.config {
            rpm_opts.config_opts.extend(cfg.iter().map(|(k, v)| format!("{k}={v}")).collect_vec());
        }

        if let Some(plugin_opt) = &rpmbuild.plugin_opts {
            rpm_opts.plugin_opts = plugin_opt.iter().map(|(k, v)| format!("{k}={v}")).collect();
        }

        if rbopts.mock_config.is_none() {
            if let Some(mockcfg) = &rbopts.mock_config {
                rpm_opts.mock_config = Some(mockcfg.to_owned());
            }
            // TODO: Implement global settings
        }
    }
    let mut arts = Artifacts::new();

    _build_pkg(package, &mut proj, cli, rpm_opts, rbopts, &mut arts, fpopts).await?;

    for (path, arttype) in arts.packages {
        let type_string = match arttype {
            PackageType::Rpm => "RPM",
            PackageType::Docker => "Docker image",
            PackageType::Podman => "Podman image",
            PackageType::Flatpak => "flatpak",
            // PackageType::RpmOstree => "rpm-ostree compose",
            PackageType::All => unreachable!(),
        };
        println!("Built {type_string}: {path}");
    }

    if let Some(post_script) = &proj.post_script {
        if post_script.extension().unwrap_or_default() == "rhai" {
            script!("post_script", post_script,);
        } else {
            cmd!(? "sh" "-c" {{ post_script.display() }})?;
        }
    }

    Ok(())
}

async fn _build_pkg(
    package: PackageType,
    proj: &mut Project,
    cli: &Cli,
    rpm_opts: RPMOptions,
    rbopts: &RpmOpts,
    arts: &mut Artifacts,
    fpopts: &FlatpakOpts,
) -> Result<(), color_eyre::Report> {
    match package {
        PackageType::All => build_all(proj, cli, rpm_opts, rbopts, arts, fpopts).await?,
        PackageType::Rpm => {
            if let Some(rpmbuild) = &proj.rpm {
                build_rpm_call(cli, rpm_opts, rpmbuild, rbopts.rpm_builder.into(), arts, rbopts)
                    .await
                    .with_context(|| "Failed to build RPMs".to_owned())?;
            } else {
                println!("No RPM build defined for project");
            }
        }
        PackageType::Docker => {
            proj.docker.as_mut().map_or_else(
                || println!("No Docker build defined for project"),
                |docker| build_oci_call(OCIBackend::Docker, cli, docker, arts),
            );
        }
        PackageType::Podman => {
            proj.podman.as_mut().map_or_else(
                || println!("No Podman build defined for project"),
                |podman| build_oci_call(OCIBackend::Podman, cli, podman, arts),
            );
        }
        PackageType::Flatpak => {
            if let Some(flatpak) = &proj.flatpak {
                build_flatpak_call(cli, flatpak, arts, fpopts.clone())
                    .await
                    .with_context(|| "Failed to build Flatpaks".to_owned())?;
            } else {
                println!("No Flatpak build defined for project");
            }
        } // PackageType::RpmOstree => todo!(),
    };
    Ok(())
}

async fn build_all(
    project: &mut Project,
    cli: &Cli,
    rpm_opts: RPMOptions,
    rbopts: &RpmOpts,
    artifacts: &mut Artifacts,
    flatpak_opts: &FlatpakOpts,
) -> Result<(), color_eyre::Report> {
    if let Some(rpmbuild) = &project.rpm {
        build_rpm_call(cli, rpm_opts, rpmbuild, rbopts.rpm_builder.into(), artifacts, rbopts)
            .await
            .with_context(|| "Failed to build RPMs".to_owned())?;
    }
    if let Some(flatpak) = &project.flatpak {
        build_flatpak_call(cli, flatpak, artifacts, flatpak_opts.clone())
            .await
            .with_context(|| "Failed to build Flatpaks".to_owned())?;
    }
    if let Some(podman) = project.podman.as_mut() {
        build_oci_call(OCIBackend::Podman, cli, podman, artifacts);
    }
    if let Some(docker) = project.docker.as_mut() {
        build_oci_call(OCIBackend::Docker, cli, docker, artifacts);
    }
    if let Some(scripts) = &project.scripts {
        info!("Running build scripts");
        crate::update::run_scripts(
            scripts
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<String>>()
                .as_slice(),
            project.labels.iter().map(|(a, b)| (a.clone(), b.clone())).collect(),
        )?;
    };
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
    // export envars for CLI environment
    std::env::set_var("ANDA_TARGET_DIR", &cli.target_dir);
    std::env::set_var("ANDA_CONFIG_PATH", &cli.config);

    if all {
        for (name, project) in config.project {
            println!("Building project: {name}");
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
