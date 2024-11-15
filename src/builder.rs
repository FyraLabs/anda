use crate::{
    artifacts::Artifacts,
    cli::{Cli, PackageType, RpmOpts},
    cmd,
    rpm_spec::{RPMBuilder, RPMExtraOptions, RPMOptions},
};
use anda_config::Project;
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

macro_rules! script {
    ($name:expr, $scr:expr, $( $var:ident ),*) => {
        let sc = andax::run(
            $name,
            &$scr,
            std::iter::empty::<(String, String)>(),
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

// project parser

pub async fn build_project(
    cli: &Cli,
    proj: Project,
    package: PackageType,
    rbopts: &RpmOpts,
) -> Result<()> {
    let cwd = std::env::current_dir().unwrap();

    let mut rpm_opts = RPMOptions {
        mock_config: rbopts.mock_config.clone(),
        sources: cwd,
        resultdir: cli.target_dir.clone(),
        ..RPMOptions::default()
    };

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

    _build_pkg(package, &proj, cli, rpm_opts, rbopts, &mut arts).await?;

    for (path, arttype) in arts.packages {
        let type_string = match arttype {
            PackageType::Rpm => "RPM",
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
    proj: &Project,
    cli: &Cli,
    rpm_opts: RPMOptions,
    rbopts: &RpmOpts,
    arts: &mut Artifacts,
) -> Result<(), color_eyre::Report> {
    match package {
        PackageType::All => build_all(proj, cli, rpm_opts, rbopts, arts).await?,
        PackageType::Rpm => {
            if let Some(rpmbuild) = &proj.rpm {
                build_rpm_call(cli, rpm_opts, rpmbuild, rbopts.rpm_builder.into(), arts, rbopts)
                    .await
                    .with_context(|| "Failed to build RPMs".to_owned())?;
            } else {
                println!("No RPM build defined for project");
            }
        }
    }
    Ok(())
}

async fn build_all(
    project: &Project,
    cli: &Cli,
    rpm_opts: RPMOptions,
    rbopts: &RpmOpts,
    artifacts: &mut Artifacts,
) -> Result<(), color_eyre::Report> {
    if let Some(rpmbuild) = &project.rpm {
        build_rpm_call(cli, rpm_opts, rpmbuild, rbopts.rpm_builder.into(), artifacts, rbopts)
            .await
            .with_context(|| "Failed to build RPMs".to_owned())?;
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
            build_project(cli, project, package, &rpm_opts).await?;
        }
    } else {
        // find project named project
        if let Some(name) = project {
            if let Some(project) = config.get_project(&name) {
                // cannot take: get_project() returns immut ref
                build_project(cli, project.clone(), package, &rpm_opts).await?;
            } else {
                return Err(eyre!("Project not found: {name}"));
            }
        } else {
            return Err(eyre!("No project specified"));
        }
    }
    Ok(())
}
