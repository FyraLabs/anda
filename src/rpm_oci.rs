//! Experimental support for building RPMs inside Podman containers instead of Mock
use nix::libc;
use podman_api::api::{Containers, Exec};
use podman_api::models::ContainerMount;
use podman_api::opts::{ContainerCreateOpts, ContainerDeleteOpts, ExecCreateOpts};
use podman_api::Podman;
use std::path::PathBuf;
const DEFAULT_PKGMGR: &str = "dnf";

fn get_podman_socket() -> String {
    std::env::var("PODMAN_SOCKET").map_or_else(
        |_| {
            // get UID of the current user
            // yup this is unsafe libc call, but who cares
            let uid = unsafe { libc::getuid() };

            // if the UID is 0, we are root, so we can use the default socket
            if uid == 0 {
                "unix:///var/run/podman/podman.sock".to_string()
            } else {
                // if we are not root, we need to use the user namespace socket
                format!("unix:///run/user/{uid}/podman/podman.sock")
            }
        },
        |socket| socket,
    )
}

fn podman() -> Podman {
    Podman::unix(get_podman_socket())
}

fn containers() -> Containers {
    podman().containers()
}

/// Gets the package manager to use
/// default is `const DEFAULT_PKGMGR`, aka `dnf`
fn get_pkgmgr() -> String {
    std::env::var("PKGMGR").unwrap_or_else(|_| DEFAULT_PKGMGR.to_string())
}

pub struct PodRPMBuilder {
    context: PathBuf,
    container: String, // string with tag
    id: String,        // container id
}

impl Drop for PodRPMBuilder {
    fn drop(&mut self) {
        // try to find container by id
        let container = containers().get(&self.id);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let e = container.delete(&ContainerDeleteOpts::builder().volumes(true).build()).await;
            if let Err(e) = e {
                tracing::error!("Failed to delete container `{id}`: {e}", id = self.id);
            }
        });

        unsafe {
            // deallocate self
            std::ptr::drop_in_place(self);
        }
    }
}

impl PodRPMBuilder {
    pub async fn new(context: PathBuf, container: String) -> Self {
        // let container_mount = podman_api::opts::VolumeCreateOpts::builder()
        //     .name("anda-build")
        //     .build();
        let opts = ContainerCreateOpts::builder()
            .image(&container) // todo: change this
            // .mounts(vec![ContainerMount::builder()
            //     .source(format!("{context}", context = context.to_str().unwrap()))
            //     .destination("/run/host")
            //     .options("Z")
            //     .build()]
            // )
            .build();

        let id = containers().create(&opts).await.unwrap();
        // Assign the result of podman.containers().create(&opts) to a variable before using it

        Self { context, container, id: id.id }
    }

    pub async fn run(&self, exec_opts: ExecCreateOpts) -> color_eyre::Result<String> {
        // let cmd = format!("{} {}", get_pkgmgr(), cmd);
        let exec = containers().get(&self.id).create_exec(&exec_opts).await?;
        // let output = exec.start().await?;
        // Ok(output.stdout)
        // Ok(exec.id)
        todo!()
    }
}

// todo: type that implements `impl IntoIterator<Item = ContainerMount>`
