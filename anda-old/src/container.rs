use anda_types::RpmBuild;
use anyhow::Result;

use buildkit_llb::{
    prelude::{
        fs::SequenceOperation,
        source::{ImageSource, LocalSource},
        Command as LLBCommand, MultiOwnedOutput, *,
    },
    utils::{OperationOutput, OutputIdx, OwnOutputIdx},
};
//use buildkit_llb::prelude::*;
use owo_colors::OwoColorize;

use std::{
    collections::BTreeMap,
    io::{stdout, BufRead},
    path::PathBuf,
};
use std::{env, sync::Arc};

use crate::{build, BuildkitLog};

#[derive(Default)]
pub struct BuildkitOptions {
    pub env: Option<BTreeMap<String, String>>,
    pub cwd: Option<String>,
    pub progress: Option<String>,
    pub transfer_artifacts: Option<bool>,
    pub context_name: Option<String>,
}

pub struct Buildkit {
    image: Option<Arc<ImageSource>>,
    cmd: Option<Arc<Command<'static>>>,
    options: BuildkitOptions,
    context: Option<OperationOutput<'static>>,
    pub artifact_cache: Option<OperationOutput<'static>>,
    pub project_cache: Option<OperationOutput<'static>>,
}

impl Buildkit {
    pub fn new(opts: Option<BuildkitOptions>) -> Buildkit {
        let opts = if let Some(opts) = opts {
            opts
        } else {
            Default::default()
        };
        Buildkit {
            image: None,
            cmd: None,
            options: opts,
            context: None,
            artifact_cache: None,
            project_cache: None,
        }
    }

    // pub fn dependency_context(mut self, switch: bool) -> Buildkit {
    //     if switch {
    //         self.options.transfer_artifacts = Some(true);
    //     }
    //     self
    // }

    pub fn context(mut self, ctx: LocalSource) -> Buildkit {
        let mut context = ctx;

        let dockerignore_path = PathBuf::from("./").join(".dockerignore");
        if dockerignore_path.exists() {
            // read dockerignore file
            let dockerignore_file = std::fs::File::open(dockerignore_path).unwrap();
            let dockerignore_file = std::io::BufReader::new(dockerignore_file);
            let dockerignore_file = dockerignore_file.lines();
            for line in dockerignore_file {
                let line = line.unwrap();
                if line.starts_with('#') {
                    continue;
                }
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                //let line = PathBuf::from("./").join(line);
                context = context.add_exclude_pattern(line);
            }
        }

        let context = context.ref_counted().output();

        let fs: SequenceOperation<'_> = {
            FileSystem::sequence()
                .custom_name("Getting build context")
                .append(
                    FileSystem::copy()
                        .from(LayerPath::Other(context, "/"))
                        .to(OutputIdx(0), LayerPath::Scratch("/")),
                )
        };
        //fs.ref_counted().output(0);
        self.context = Some(fs.ref_counted().output(0));

        let mut artifact_cache = {
            FileSystem::sequence()
                .custom_name("Getting artifact cache")
                .append(FileSystem::mkdir(OutputIdx(0), LayerPath::Scratch("/")).make_parents(true))
                .ref_counted()
                .output(0)
        };

        if let Some(switch) = self.options.transfer_artifacts {
            if switch {
                artifact_cache = Source::local("artifacts").ref_counted().output();
            }
            // else go up the scope and use the default artifact cache
        }
        let cache = {
            FileSystem::sequence()
                .custom_name("Getting artifact cache")
                .append(FileSystem::mkdir(OutputIdx(0), LayerPath::Scratch("/")).make_parents(true))
                .append(
                    FileSystem::copy()
                        .from(LayerPath::Other(artifact_cache, "/"))
                        .to(OutputIdx(1), LayerPath::Own(OwnOutputIdx(0), "/")),
                )
        };
        self.artifact_cache = Some(cache.ref_counted().output(1));

        let project_cache = {
            FileSystem::sequence()
                .custom_name("Getting project collection cache")
                .append(FileSystem::mkdir(OutputIdx(0), LayerPath::Scratch("/")).make_parents(true))
                .ref_counted()
                .output(0)
        };

        self.project_cache = Some(project_cache);

        self
    }

    pub fn image(mut self, image_name: &str) -> Buildkit {
        let img = Source::image(image_name)
            .custom_name(format!("Using image {}", image_name))
            .ref_counted();

        self.image = Some(img);
        self
    }

    pub fn command_args(&mut self, command: Vec<&str>) -> &mut Buildkit {
        // find dockerignore file
        //let mut local = self.context_source("context");

        //let local = local.ref_counted();
        if let Some(image) = &self.image {
            // split the first command
            let (arg1, argn) = command.split_first().unwrap();
            eprintln!("{}", format!("$ {}", arg1).black());
            let mut cmd = LLBCommand::run(arg1.to_owned())
                .args(argn)
                .cwd("/src")
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            if let Some(out) = &self.cmd {
                cmd = cmd
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), out.output(1), "/src/anda-build"))
            } else {
                cmd = cmd
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"));
                //TODO: Make this a list of shared caches so it's distro-agnostic
            }
            cmd = cmd
                .mount(Mount::Layer(
                    OutputIdx(2),
                    self.context.as_ref().unwrap().to_owned(),
                    "/src",
                ))
                .mount(Mount::SharedCache("/var/cache/dnf"));

            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn context_source(&mut self, context: &str) -> LocalSource {
        let mut local = Source::local(context);
        let dockerignore_path = PathBuf::from("./").join(".dockerignore");
        if dockerignore_path.exists() {
            // read dockerignore file
            let dockerignore_file = std::fs::File::open(dockerignore_path).unwrap();
            let dockerignore_file = std::io::BufReader::new(dockerignore_file);
            let dockerignore_file = dockerignore_file.lines();
            for line in dockerignore_file {
                let line = line.unwrap();
                if line.starts_with('#') {
                    continue;
                }
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                //let line = PathBuf::from("./").join(line);
                local = local.add_exclude_pattern(line);
            }
        }
        local
    }

    pub fn command(&mut self, command: &str) -> &mut Buildkit {
        // find dockerignore file
        //let mut local = self.context_source("context");

        let name = if let Some(context_name) = &self.options.context_name {
            format!("[{}] {}", context_name, command)
        } else {
            command.to_owned()
        };

        if let Some(image) = &self.image {
            let mut cmd = LLBCommand::run("/bin/sh")
                .args(&["-c", command])
                .cwd("/src")
                .custom_name(name)
                .insecure(true)
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            if let Some(out) = &self.cmd {
                cmd = cmd
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
            } else {
                cmd = cmd.mount(Mount::Layer(OutputIdx(0), image.output(), "/"));
            }
            let art = self.artifact_cache.as_ref().unwrap();
            cmd = cmd
                .mount(Mount::Layer(
                    OutputIdx(1),
                    art.to_owned(),
                    "/src/anda-build",
                ))
                .mount(Mount::Layer(
                    OutputIdx(2),
                    self.context.as_ref().unwrap().to_owned(),
                    "/src",
                ))
                .mount(Mount::SharedCache("/var/cache/dnf"))
                .mount(Mount::SharedCache("/var/cache/anda"));

            let cmd = cmd.ref_counted();
            self.artifact_cache = Some(cmd.output(1));
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn command_nocontext(&mut self, command: &str) -> &mut Buildkit {
        let name = if let Some(context_name) = &self.options.context_name {
            format!("[{}] {}", context_name, command)
        } else {
            command.to_owned()
        };
        if let Some(image) = &self.image {
            let mut cmd = LLBCommand::run("/bin/sh")
                .args(&["-c", command])
                .cwd("/src")
                .custom_name(name)
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            //let art = self.artifact_cache.as_ref().unwrap();
            if let Some(out) = &self.cmd {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), out.output(1), "/src/anda-build"))
                //.env("FOO", "BAR");
            } else {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"))
            }
            cmd = cmd
                .mount(Mount::SharedCache("/var/cache/dnf"))
                .mount(Mount::SharedCache("/var/cache/anda"));

            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn cargo_builddeps(&mut self) -> &mut Buildkit {
        if let Some(image) = &self.image {
            let mut cmd = LLBCommand::run("cargo")
                .args(&["install", "cargo-generate-rpm", "--root", "/usr/local"])
                .cwd("/src")
                .custom_name("Installing Andaman build dependencies")
                .env("CARGO_HOME", "/var/cache/anda/cargo")
                .env("CARGO_TARGET_DIR", "/var/cache/anda/target")
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            //let art = self.artifact_cache.as_ref().unwrap();
            if let Some(out) = &self.cmd {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), out.output(1), "/src/anda-build"))
                //.env("FOO", "BAR");
            } else {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"))
            }
            cmd = cmd
                .mount(Mount::SharedCache("/var/cache/dnf"))
                .mount(Mount::SharedCache("/var/cache/anda"));

            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn inject_rpm_script(&mut self) -> &mut Buildkit {
        let script = include_str!("anda_build_rpm.sh");
        let command = format!(
            r#"cat << 'EOF' > /usr/local/bin/anda_build_rpm
        {}
        "#,
            script
        );

        if let Some(image) = &self.image {
            let mut cmd = LLBCommand::run("/bin/sh")
                .args(&["-c", &command])
                //.custom_name("Inject RPM script into image")
                .cwd("/src")
                .custom_name("Installing RPM builder script")
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            if let Some(out) = &self.cmd {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), out.output(1), "/src/anda-build"))
                //.env("FOO", "BAR");
            } else {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"))
            }
            cmd = cmd.mount(Mount::SharedCache("/var/cache/dnf"));
            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);

            let mut cmd = LLBCommand::run("/bin/sh")
                .args(&["-c", "argbash -i /usr/local/bin/anda_build_rpm && chmod +x /usr/local/bin/anda_build_rpm"])
                //.custom_name("Inject RPM script into image")
                .cwd("/src")
                .custom_name("Marking RPM builder script as executable")
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            if let Some(out) = &self.cmd {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), out.output(1), "/src/anda-build"))
                //.env("FOO", "BAR");
            } else {
                cmd = cmd
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"))
            }
            cmd = cmd.mount(Mount::SharedCache("/var/cache/dnf"));
            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn build_rpm(
        &mut self,
        rpm: &str,
        mode: anda_types::config::RpmBuildMode,
        pre_buildreqs: Option<&Vec<String>>,
        build: &RpmBuild,
    ) -> &mut Buildkit {
        if let Some(image) = &self.image.clone() {
            let name = if let Some(context_name) = &self.options.context_name {
                format!("[{}] Building RPM using Andaman build script", context_name)
            } else {
                "Building RPM using Andaman build script".to_owned()
            };
            self.command_nocontext("echo 'keepcache=true' >> /etc/dnf/dnf.conf");
            self.command_nocontext(
                "sudo dnf install -y rpm-build dnf-plugins-core rpmdevtools argbash rustc cargo createrepo_c mock",
            );
            self.cargo_builddeps();
            self.inject_rpm_script();

            if let Some(pre_buildreqs) = pre_buildreqs {
                if !pre_buildreqs.is_empty() {
                    let mut c = vec!["dnf", "install", "-y"];
                    c.extend(pre_buildreqs.iter().map(|x| x.as_str()));
                    self.command_args(c);
                }
            }

            if let Some(pre_script) = build.pre_script.as_ref() {
                for line in pre_script.commands.iter() {
                    self.command(line);
                }
            }

            let mut cmd = LLBCommand::run("/bin/bash")
                .custom_name(name)
                .ignore_cache(true)
                .insecure(true)
                .env_iter(self.options.env.as_ref().unwrap_or(&BTreeMap::new()));
            match mode {
                anda_types::config::RpmBuildMode::Standard => {
                    //self.command(&format!("sudo dnf builddep -y {}", rpm));
                    cmd = cmd.args(&["anda_build_rpm", "rpmbuild", "-p", rpm]);
                }
                anda_types::config::RpmBuildMode::Cargo => {
                    cmd = cmd.args(&["anda_build_rpm", "cargo", "-p", rpm]);
                }
            }
            let art = self.artifact_cache.as_ref().unwrap();
            if let Some(out) = &self.cmd {
                cmd = cmd
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), out.output(0), "/"))
            } else {
                cmd = cmd.mount(Mount::Layer(OutputIdx(0), image.output(), "/"));
            }
            cmd = cmd
                .mount(Mount::Layer(
                    OutputIdx(1),
                    art.to_owned(),
                    "/src/anda-build",
                ))
                .mount(Mount::Layer(
                    OutputIdx(2),
                    self.context.as_ref().unwrap().to_owned(),
                    "/src",
                ))
                .cwd("/src")
                .mount(Mount::SharedCache("/var/cache/dnf"))
                .mount(Mount::SharedCache("/var/cache/anda"));
            let cmd = cmd.ref_counted();
            self.artifact_cache = Some(cmd.output(1));
            self.cmd = Some(cmd);
            if let Some(post_script) = build.post_script.as_ref() {
                for line in post_script.commands.iter() {
                    self.command(line);
                }
            }
        } else {
            panic!("No image specified");
        }
        self
    }

    // Merge artifact outputs into one single artifact cache
    // This is useful if you want to generate one big LLB that fetches the artifact cache from all the other builds
    // To use this, call it for each output you recieve.
    // TODO: Implement this for the builds
    pub fn merge_artifact_output(
        &mut self,
        output_merge: OperationOutput<'static>,
    ) -> &mut Buildkit {
        let fs: SequenceOperation<'_> = {
            FileSystem::sequence()
                .custom_name("Merging artifact cache")
                .append(
                    FileSystem::copy()
                        .from(LayerPath::Other(output_merge, "/"))
                        .to(
                            OutputIdx(0),
                            LayerPath::Other(self.artifact_cache.as_ref().unwrap().to_owned(), "/"),
                        )
                        .recursive(true),
                )
        };
        self.artifact_cache = Some(fs.ref_counted().output(0));
        self
    }

    pub fn build_graph(&mut self) -> OperationOutput<'_> {
        /* if self.cmd.is_none() {
            panic!("No output specified");
        } */

        let output = self.artifact_cache.as_ref().unwrap().to_owned();
        let fs: SequenceOperation<'_> = {
            FileSystem::sequence()
                .custom_name("Copy over artifacts")
                .append(
                    FileSystem::copy()
                        .from(LayerPath::Other(output.clone(), "/"))
                        .to(OutputIdx(0), LayerPath::Other(output, "/")),
                )
        };

        fs.ref_counted().output(0)
    }

    // Simply outputs the raw output, no artifact copying is done
    pub fn build_graph_builder(self) -> OperationOutput<'static> {
        let output = self.artifact_cache.as_ref().unwrap().to_owned();
        output
    }

    pub fn execute(&mut self, builder_opts: &build::BuilderOptions) -> Result<()> {
        //println!("{:#?}", self.artifact_cache);
        if builder_opts.display_llb {
            Terminal::with(self.build_graph())
                .write_definition(stdout())
                .unwrap();

            return Ok(());
        }

        let mut extra_args = Vec::new();

        match builder_opts.buildkit_log {
            BuildkitLog::Tty => {
                extra_args.push("--progress=tty");
            }
            BuildkitLog::Auto => {
                extra_args.push("--progress=auto");
            }
            BuildkitLog::Plain => {
                extra_args.push("--progress=plain");
            }
        }
        if let Some(opt) = self.options.transfer_artifacts {
            if opt {
                extra_args.push("--local");
                extra_args.push("artifacts=anda-build");
            }
        }
        let buildkit_host = if let Ok(buildkit_host) = env::var("BUILDKIT_HOST") {
            buildkit_host
        } else {
            "docker-container://anda-buildkitd".to_string()
        };
        let mut cmd = std::process::Command::new("buildctl");

        cmd.arg("build")
            .arg("--output")
            .arg("type=local,dest=anda-build")
            .args(&["--local", "context=."])
            .args(&["--allow=security.insecure"])
            .args(&extra_args)
            //.arg("--opt")
            .env("BUILDKIT_HOST", buildkit_host)
            .stdin(std::process::Stdio::piped());

        let mut cmd = cmd.spawn()?;

        Terminal::with(self.build_graph())
            .write_definition(cmd.stdin.as_mut().unwrap())
            .unwrap();
        let ret = cmd.wait()?;

        if !ret.success() {
            return Err(anyhow::anyhow!("Build failed"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod test_docker {}
