use anyhow::{anyhow, Result};
use bollard::container::{Config, RemoveContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::BuildImageOptions;
use bollard::image::CreateImageOptions;
use bollard::service::HostConfig;
use bollard::Docker;
use buildkit_llb::{
    prelude::{
        fs::SequenceOperation, source::ImageSource, Command as LLBCommand, MultiOwnedOutput, *,
    },
    utils::{OperationOutput, OutputIdx, OwnOutputIdx},
};
//use buildkit_llb::prelude::*;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::{borrow::BorrowMut, env, io::BufRead, path::PathBuf, process::ExitStatus};
use std::{collections::HashMap, sync::Arc};
use tokio_stream::StreamExt;

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
        println!("{}", format!("$ {}", command.join(" ")).black());

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
            println!("{}", format!("$ {}", command).black());
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
    pub env: Option<HashMap<String, String>>,
    pub cwd: Option<String>,
    pub progress: Option<String>,
}
pub struct Buildkit {
    image: Option<Arc<ImageSource>>,
    cmd: Option<Arc<Command<'static>>>,
    options: BuildkitOptions,
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
        }
    }

    pub fn image(&mut self, image_name: &str) -> &mut Buildkit {
        let img = Source::image("fedora:latest")
            .custom_name(format!("Using image {}", image_name))
            .ref_counted();

        self.image = Some(img);
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

        let local = local.ref_counted();

        if let Some(image) = &self.image {
            if let Some(cmd) = &self.cmd {
                let cmd = LLBCommand::run("/bin/sh")
                    .args(&["-c", command])
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), cmd.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), cmd.output(1), "/src/anda-build"))
                    .mount(Mount::Layer(OutputIdx(2), local.output(), "/src"))
                    .mount(Mount::SharedCache("/var/cache/dnf"))
                    .cwd("/src")
                    .env_iter(self.options.env.as_ref().unwrap_or(&HashMap::new()));
                //.env("FOO", "BAR");
                let cmd = cmd.ref_counted();
                self.cmd = Some(cmd);
            } else {
                let cmd = LLBCommand::run("/bin/sh")
                    .args(&["-c", command])
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"))
                    .mount(Mount::Layer(OutputIdx(2), local.output(), "/src"))
                    //TODO: Make this a list of shared caches so it's distro-agnostic
                    .mount(Mount::SharedCache("/var/cache/dnf"))
                    .cwd("/src")
                    .env_iter(self.options.env.as_ref().unwrap_or(&HashMap::new()));

                let cmd = cmd.ref_counted();
                self.cmd = Some(cmd);
            }
        }
        self
    }

    pub fn command_nocontext(&mut self, command: &str) -> &mut Buildkit {
        if let Some(image) = &self.image {
            if let Some(cmd) = &self.cmd {
                let mut cmd = LLBCommand::run("/bin/sh")
                    .args(&["-c", command])
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), cmd.output(0), "/"))
                    .mount(Mount::Layer(OutputIdx(1), cmd.output(1), "/src/anda-build"))
                    .mount(Mount::SharedCache("/var/cache/dnf"))
                    .cwd("/src")
                    .env_iter(self.options.env.as_ref().unwrap_or(&HashMap::new()));
                //.env("FOO", "BAR");
                let cmd = cmd.ref_counted();
                self.cmd = Some(cmd);
            } else {
                let cmd = LLBCommand::run("/bin/sh")
                    .args(&["-c", command])
                    //.mount(Mount::ReadOnlyLayer(image.output(), "/"))
                    .mount(Mount::Layer(OutputIdx(0), image.output(), "/"))
                    .mount(Mount::Scratch(OutputIdx(1), "/src/anda-build"))
                    .mount(Mount::SharedCache("/var/cache/dnf"))
                    .cwd("/src")
                    .env_iter(self.options.env.as_ref().unwrap_or(&HashMap::new()));

                let cmd = cmd.ref_counted();
                self.cmd = Some(cmd);
            }
        }
        self
    }

    pub fn build_graph(&mut self) -> OperationOutput<'_> {
        if self.cmd.is_none() {
            panic!("No output specified");
        }

        let local = Source::local(".");
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

    pub fn execute(&mut self) -> Result<ExitStatus> {
        let mut extra_args = Vec::new();

        if let Some(opt) = &self.options.progress {
            match opt.as_str() {
                "tty" => {
                    extra_args.push("--progress=tty");
                }
                "auto" => {
                    extra_args.push("--progress=auto");
                }
                "plain" => {
                    extra_args.push("--progress=plain");
                }
                _ => {
                    panic!("Unknown progress option");
                }
            }
        };

        let mut cmd = std::process::Command::new("buildctl")
            .arg("build")
            .arg("--output")
            .arg("type=local,dest=anda-build")
            .args(&["--local", "context=."])
            .args(&extra_args)
            //.arg("--opt")
            .env("BUILDKIT_HOST", "docker-container://buildkitd")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        let stdin = cmd.stdin.as_mut().unwrap();

        Terminal::with(self.build_graph())
            .write_definition(stdin)
            .unwrap();

        Ok(cmd.wait()?)
    }
}

#[cfg(test)]
mod test_docker {
    use bollard::service::HostConfig;
    use std::io::stdout;

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
