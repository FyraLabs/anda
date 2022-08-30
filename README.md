

![Andaman Project](assets/anda-medium.png)

# Andaman

Andaman is a package build system and CI (Continuous Integration) toolchain written in Rust, powered by BuildKit.
It is inspired by Ultramarine Linux's `umpkg` project, and allows you to quickly build multiple types of projects into different artifacts.

It also supports monorepos with configuration files, and can be run completely standalone without a server (Although it comes with one for distributed builds).

Andaman is planned to have the following features:

- Building artifact
- Signing packages
- Build artifacts for:
    - Disk images and live media (powered by Lorax)
    - OSTree composes
    - RPM-OSTree composes
    - Docker images
    - Flatpak
- Generating whole repositories and composes from the above mentioned artifacts
- An extra user repository for packages that cannot be included in the main repositories (like the AUR)

It is planed to be the centerpiece of Ultramarine Linux (and tauOS) and its ecosystem, and a replacement for the existing [Koji](https://koji.build) system.

## Why?

Currently, Ultramarine Linux uses the [Koji](https://koji.build) system to build packages.

The build system itself is a bit of a mess, and requires a lot of manual setup to get a working system.
Koji contains a lot of legacy code and while flexible, it is very hard to use and maintain.
Fedora's packaging stack consists of many complicated services such as Bodhi and Pungi to add extra functionality to the system.
Which means that small communities like Ultramarine cannot use the same large stack of services.

The case is the same for Fyra Labs, who simply resorted to a series of very hacky solutions using Teleport to automatically build packages and push them to their
own repositories.
This is not very robust however, and scalability will become an issue when the number of repositories grows.

We want to create a stable, robust, and scalable build system that is easy to use and easy to maintain as an alternative to the Koji project.


## The architecture

Andaman is a build system written in Rust, and is powered by the following components:

- [Minio](https://min.io) (and S3 powered by the AWS Rust SDK) for storing artifacts
- [Rocket](https://rocket.rs) for the server
- [React](https://reactjs.org) for the web interface
- [TailwindCSS](https://tailwindcss.com) for the web interface
- [RPM](https://rpm.org) the RPM package manager for building RPM packages
- [DNF](https://github.com/rpm-software-management/dnf) for resolving RPM packages and installing them (until we have a proper RPM frontend)
- [Kubernetes](https://kubernetes.io) for build orchestration
- [PostgreSQL](https://www.postgresql.org) for storing build metadata
- [SeaORM](https://www.sea-ql.org/SeaORM/) for database access
- [HCL (HashiCorp Configuration Language)](https://github.com/hashicorp/hcl) for project manifests

## Roadmap

* [x] Building RPM packages
* [x] Build artifact management
* [x] Task scheduling for builds
* [ ] Full repository composition
* [ ] Build artifact signing
* [x] OCI containers
* [ ] OSTree composes (Flatpak and RPM-OSTree)
* [x] Building RPMs using an alternative package spec format (see `cargo-generate-rpm`)


## Build Instructions

To build and use Andaman, you need the following:

- [Rust](https://www.rust-lang.org) & Cargo `rustc` `cargo`
- [BuildKit](https://github.com/moby/buildkit)
- [PNPM](https://pnpm.io)
- [Docker/Moby Engine](https://www.docker.com) `moby-engine`
- [Minio](https://min.io)
- [OpenSSL](https://www.openssl.org) `openssl-devel`
- [PostgreSQL](https://www.postgresql.org) `libpq-devel`

After installing the above, you can build Andaman by running `cargo build`.

You can also self-host Andaman (Build using itself) by running `cargo anda build -s anda`.
This builds Andaman using Cargo, and builds itself using the provided `anda.hcl` config file.

For hacking Andaman, see the [Hacking guide](README.developers.md).
