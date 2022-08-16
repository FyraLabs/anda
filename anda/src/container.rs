use anyhow::{anyhow, Result};
use bollard::container::{Config, RemoveContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};



use bollard::Docker;
use buildkit_llb::{
    prelude::{
        fs::SequenceOperation, source::{ImageSource, LocalSource}, Command as LLBCommand, MultiOwnedOutput, *,
    },
    utils::{OperationOutput, OutputIdx, OwnOutputIdx},
};
//use buildkit_llb::prelude::*;
use owo_colors::OwoColorize;

use std::{
    io::{stdout, BufRead},
    path::PathBuf, collections::BTreeMap,
};
use std::{collections::HashMap, sync::Arc};
use tokio_stream::StreamExt;

use crate::{build, BuildkitLog};

const IMAGE: &str = "fedora:36";

pub struct ContainerHdl {
    docker: Docker,
    config: Config<String>,
}

pub struct Container {
    hdl: ContainerHdl,
    id: String,
}

impl ContainerHdl {
    pub fn new() -> ContainerHdl {
        ContainerHdl {
            // TODO will fix later
            docker: Docker::connect_with_socket_defaults().unwrap(),
            config: Config {
                tty: Some(true),
                working_dir: Some("/".to_owned()),
                ..Default::default()
            },
        }
    }

    // pub async fn build_image(&self, dockerfile: String, t: String) -> Result<&Self> {
    //     let opt = BuildImageOptions {
    //         dockerfile,
    //         t,
    //         rm: false,
    //         ..Default::default()
    //     };
    //     self.docker.build_image(opt, None, None);
    //     // TODO pretty print
    //     Ok(self)
    // }

    pub async fn create_container(&self) -> Result<String> {
        /* self.docker
        .create_image(
            Some(CreateImageOptions {
                from_image: IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .collect::<Vec<_>>()
        .await; */

        let id = self
            .docker
            .create_container::<String, String>(None, self.config.to_owned())
            .await?
            .id;
        Ok(id)
    }

    pub fn image(&mut self, image: &str) -> &mut Self {
        self.config.image = Some(image.to_owned());
        self
    }

    pub fn volumes(&mut self, volumes: HashMap<String, HashMap<(), ()>>) -> &mut Self {
        self.config.volumes = Some(volumes);
        self
    }

    pub fn working_dir(&mut self, working_dir: &str) -> &mut Self {
        self.config.working_dir = Some(working_dir.to_owned());
        self
    }

    pub fn env(&mut self, env: Vec<String>) -> &mut Self {
        self.config.env = Some(env);
        self
    }
}

impl Container {
    pub async fn new(mut hdl: ContainerHdl, config: Option<Config<String>>) -> Result<Self> {
        if let Some(config) = config {
            hdl.config = config;
        }

        let id = hdl.create_container().await?;
        Ok(Container { hdl, id })
    }
    pub async fn finish(&self) -> Result<()> {
        self.hdl
            .docker
            .remove_container(
                &self.id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;
        Ok(())
    }

    pub async fn start(self) -> Result<Container> {
        self.hdl
            .docker
            .start_container::<String>(&self.id, None)
            .await?;
        Ok(self)
    }

    pub async fn run_cmd(&self, command: Vec<&str>) -> Result<&Container> {
        eprintln!("{}", format!("$ {}", command.join(" ")).black());

        let exec = self
            .hdl
            .docker
            .create_exec(
                &self.id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command),
                    privileged: Some(true),
                    //working_dir: Some(env::current_dir().unwrap().to_str().unwrap()),
                    ..Default::default()
                },
            )
            .await?;
        if let StartExecResults::Attached { mut output, .. } =
            self.hdl.docker.start_exec(&exec.id, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                print!("{} {}", "[DOCKER]".blue(), msg);
                // TODO will improve appearance later
            }
        } else {
            unreachable!();
        }
        let exit = self
            .hdl
            .docker
            .inspect_exec(&exec.id)
            .await?
            .exit_code
            .expect("No exit code");
        if exit != 0 {
            return Err(anyhow!("Exit code was {}", exit));
        }
        Ok(self.to_owned())
    }

    //? https://github.com/fussybeaver/bollard/blob/master/examples/exec.rs
    pub async fn run_cmds(&self, commands: Vec<&str>) -> Result<&Container> {
        for command in commands {
            eprintln!("{}", format!("$ {}", command).black());
            let exec = self
                .hdl
                .docker
                .create_exec(
                    &self.id,
                    CreateExecOptions {
                        attach_stdout: Some(true),
                        attach_stderr: Some(true),
                        cmd: Some(vec!["sh", "-c", command]),
                        privileged: Some(true),
                        //working_dir: Some(env::current_dir().unwrap().to_str().unwrap()),
                        ..Default::default()
                    },
                )
                .await?;

            // get output
            if let StartExecResults::Attached { mut output, .. } =
                self.hdl.docker.start_exec(&exec.id, None).await?
            {
                while let Some(Ok(msg)) = output.next().await {
                    print!("{} {}", "[DOCKER]".blue(), msg);
                    // TODO will improve appearance later
                }
            } else {
                unreachable!();
            }
            let exit = self
                .hdl
                .docker
                .inspect_exec(&exec.id)
                .await?
                .exit_code
                .expect("No exit code");
            if exit != 0 {
                return Err(anyhow!("Exit code was {}", exit));
            }
        }
        Ok(self.to_owned())
    }
}

#[derive(Default)]
pub struct BuildkitOptions {
    pub env: Option<BTreeMap<String, String>>,
    pub cwd: Option<String>,
    pub progress: Option<String>,
    pub transfer_artifacts: Option<bool>,
}

pub struct Buildkit {
    image: Option<Arc<ImageSource>>,
    cmd: Option<Arc<Command<'static>>>,
    options: BuildkitOptions,
    context: Option<OperationOutput<'static>>,
    artifact_cache: Option<OperationOutput<'static>>,
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
        }
    }

    pub fn dependency_context(mut self, switch: bool) -> Buildkit {
        if switch {
            self.options.transfer_artifacts = Some(true);
        }
        self
    }


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


        if let Some(switch) = self.options.transfer_artifacts {
            let artifact_cache = Source::local("artifacts").ref_counted().output();
            if switch {
                let artifact_cache = {
                    FileSystem::sequence()
                        .custom_name("Getting artifact cache")
                        .append(
                            FileSystem::mkdir(OutputIdx(0), LayerPath::Scratch("/")).make_parents(true),
                        )
                        .append(
                            FileSystem::copy()
                                .from(LayerPath::Other(artifact_cache, "/"))
                                .to(OutputIdx(1), LayerPath::Own(OwnOutputIdx(0), "/")),
                        )
                };
                self.artifact_cache = Some(artifact_cache.ref_counted().output(1));
            }
        }
        self
    }

    pub fn image(mut self, image_name: &str) -> Buildkit {
        let img = Source::image("fedora:latest")
            .custom_name(format!("Using image {}", image_name))
            .ref_counted();

        self.image = Some(img);
        self
    }

    pub fn command_args(&mut self, command: Vec<&str>) -> &mut Buildkit {
        // find dockerignore file
        let mut local = Source::local("context");
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
                .mount(Mount::Layer(OutputIdx(2), self.context.as_ref().unwrap().to_owned(), "/src"))
                .mount(Mount::SharedCache("/var/cache/dnf"));

            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn command(&mut self, command: &str) -> &mut Buildkit {
        // find dockerignore file
        let mut local = Source::local("context");
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

        if let Some(image) = &self.image {
            let mut cmd = LLBCommand::run("/bin/sh")
                .args(&["-c", command])
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
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"));

                    if let Some(switch) = self.options.transfer_artifacts {
                        if switch {
                            let art = self.artifact_cache.as_ref().unwrap();
                            cmd = cmd.mount(Mount::Layer(OutputIdx(1), art.to_owned(), "/src/anda-build"));
                        }
                    } else {
                        cmd = cmd.mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"));
                    }
                //TODO: Make this a list of shared caches so it's distro-agnostic
            }
            cmd = cmd
                .mount(Mount::Layer(OutputIdx(2), self.context.as_ref().unwrap().to_owned(), "/src"))
                .mount(Mount::SharedCache("/var/cache/dnf"));

            let cmd = cmd.ref_counted();
            self.cmd = Some(cmd);
        } else {
            panic!("No image specified");
        }
        self
    }

    pub fn command_nocontext(&mut self, command: &str) -> &mut Buildkit {
        if let Some(image) = &self.image {
            let mut cmd = LLBCommand::run("/bin/sh")
                .args(&["-c", command])
                .cwd("/src")
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

    pub fn inject_rpm_script(&mut self) -> &mut Buildkit {

        let script = include_str!("anda_build_rpm.sh");
        let command = format!(r#"cat << 'EOF' > /usr/local/bin/anda_build_rpm
        {}
        "#, script);

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
                .args(&["-c", "chmod +x /usr/local/bin/anda_build_rpm"])
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

    pub fn build_graph(&mut self) -> OperationOutput<'_> {
        if self.cmd.is_none() {
            panic!("No output specified");
        }

        let output = self.cmd.take().unwrap().output(1);
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

    pub fn execute(&mut self, builder_opts: &build::BuilderOptions) -> Result<()> {

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
        let mut cmd = std::process::Command::new("buildctl")
            .arg("build")
            .arg("--output")
            .arg("type=local,dest=anda-build")
            .args(&["--local", "context=."])
            .args(&extra_args)
            //.arg("--opt")
            //.env("BUILDKIT_HOST", "docker-container://buildkitd")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

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
mod test_docker {
    use bollard::service::HostConfig;
    use super::*;

    #[tokio::test]
    async fn container_run_hello_world() -> Result<()> {
        let conhdl = ContainerHdl::new();
        // volume is HashMap<T, HashMap<(), ()>>

        let cwd = env::current_dir().unwrap();
        let hostconf = HostConfig {
            binds: Some(vec![format!("{}:{}", cwd.display(), cwd.display())]),
            ..Default::default()
        };

        let cfg = Config {
            image: Some("fedora:latest".to_owned()),
            hostname: Some("test".to_owned()),
            tty: Some(true),
            working_dir: Some("/".to_owned()),
            host_config: Some(hostconf),
            ..Default::default()
        };
        Container::new(conhdl, Some(cfg))
            .await?
            .start()
            .await?
            .run_cmds(vec!["echo hello world", "ls -la", "aaa"])
            .await?
            .finish()
            .await?;
        Ok(())
    }
}
