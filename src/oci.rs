//! OCI Builder backend
//! Supports Docker and Podman
use std::process::Command;

#[derive(Clone, Copy)]
pub enum OCIBackend {
    Docker,
    Podman,
}

impl OCIBackend {
    pub fn command(self) -> Command {
        let cmd = match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        };

        Command::new(cmd)
    }
}

pub struct OCIBuilder {
    context: String,
    tag: String,
    version: String,
    label: Vec<String>,
}

impl OCIBuilder {
    pub const fn new(context: String, tag: String, version: String) -> Self {
        Self { context, tag, version, label: Vec::new() }
    }

    pub fn add_label(&mut self, label: String) {
        self.label.push(label);
    }

    // We use string here because we want to let people use stuff like git contexts
    pub fn build(&self, dockerfile: &str, backend: OCIBackend, latest: bool) {
        let mut cmd = backend.command();

        let real_tag = &format!("{}:{}", &self.tag, self.version);

        cmd.arg("build")
            .arg(&self.context)
            .arg("-f")
            .arg(dockerfile)
            .arg("-t")
            .env("DOCKER_BUILDKIT", "1")
            .arg(real_tag);

        if latest {
            cmd.arg("-t").arg(format!("{}:latest", &self.tag));
        }

        for label in &self.label {
            cmd.arg("--label").arg(label);
        }
    }
}

pub fn build_oci(
    backend: OCIBackend,
    dockerfile: &str,
    latest: bool,
    tag: &str,
    version: &str,
    context: &str,
) -> Vec<String> {
    let mut builder = OCIBuilder::new(context.to_owned(), tag.to_owned(), version.to_owned());
    builder.add_label(format!("com.fyralabs.anda.version={}", env!("CARGO_PKG_VERSION")));

    builder.build(dockerfile, backend, latest);

    let mut tags = vec![format!("{tag}:{version}")];

    if latest {
        tags.push(format!("{tag}:latest"));
    }
    tags
}
