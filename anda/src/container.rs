use anyhow::{anyhow, Result};
use bollard::container::{Config, RemoveContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::BuildImageOptions;
use bollard::image::CreateImageOptions;
use bollard::service::HostConfig;
use bollard::Docker;
use buildkit_llb::{prelude::{MultiOwnedOutput, source::ImageSource, fs::SequenceOperation, Command as LLBCommand, *}, utils::{OperationOutput, OutputIdx, OwnOutputIdx}};
//use buildkit_llb::prelude::*;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use std::env;
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

pub struct Buildkit;

impl Buildkit {
    pub fn new() -> Buildkit {
        Self
    }

    fn image() -> ImageSource {
        let builder_image = Source::image("library/alpine:latest")
            .custom_name("Using alpine:latest as a builder");
        builder_image
    }
    fn build_command(image: &ImageSource) -> Arc<LLBCommand<'_>> {
        LLBCommand::run("/bin/sh")
                .args(&["-c", "echo 'test string 5' > /out/file0"])
                .custom_name("create a dummy file")
                .mount(Mount::ReadOnlyLayer(image.output(), "/"))
                .mount(Mount::Scratch(OutputIdx(0), "/out"))
                .ref_counted()
    }

    fn build_graph(command: Arc<LLBCommand<'static>>) -> OperationOutput<'static> {
        let fs: SequenceOperation<'static> = {
            FileSystem::sequence()
                .custom_name("do multiple file system manipulations")
                .append(
                    FileSystem::copy()
                        .from(LayerPath::Other(command.output(0), "/file0"))
                        .to(OutputIdx(0), LayerPath::Other(command.output(0), "/file1")),
                )
                .append(
                    FileSystem::copy()
                        .from(LayerPath::Own(OwnOutputIdx(0), "/file0"))
                        .to(OutputIdx(1), LayerPath::Own(OwnOutputIdx(0), "/file2")),
                )
        };

        fs.ref_counted().output(1)
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
