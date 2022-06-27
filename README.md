

![Andaman Project](assets/anda-medium.png)

# Andaman

Andaman is a package manager and build system that allows you to easily manage dependencies and different kinds of artifacts

It is based on Ultramarine Linux's `umpkg` project, and allows you to easily manage packages from multiple different package managers.

Andaman is planned to have the following features:

- Building, resolving and installing RPM packages
- Support for NPM, Cargo and PyPI packages
- Signing packages
- Build artifacts for:
    - Disk images and live media (powered by Lorax)
    - OSTree composes
    - RPM-OSTree composes
    - Docker images
    - Flatpak
- Generating whole repositories and composes from the above mentioned artifacts
- An extra user repository for packages that cannot be included in the main repositories (like the AUR)

It is planed to be the centerpiece of Ultramarine Linux and its ecosystem, and a replacement for the existing [Koji](https://koji.build) system.

## Why?

Currently, Ultramarine Linux uses the [Koji](https://koji.build) system to build packages.

The build system itself is a bit of a mess, and requires a lot of manual setup to get a working system.
Koji contains a lot of legacy code and while flexible, it is very hard to use and maintain.
Fedora's packaging stack consists of many complicated services such as Bodhi and Pungi to add extra functionality to the system.
Which means that small communities like Ultramarine cannot use the same large stack of services.

The case is the same for Fyra Labs, who simply resorted to a series of very hacky solutions using Teleport to automatically build packages and push them to their
own repositories.
This is not very robust however, and scalability will become an issue when the number of repositories grows.

The Andaman project tries to resolve this problem by providing a simple, modern and robust build system
that can be used to easily build, sign, and test packages or other kinds of artifacts like OS images, Flatpak, or OCI images.


### Clientside

DNF is a great package manager, however, it is not very flexible.
We wanted to create a hybrid package manager that accepts packages from multiple package managers, and source-only packages (like how the AUR or MPR works).
All of these can provide dependencies for each other, without having to constantly repackage the same packages all over again, just because one application requires
a dependency that simply has not been packaged yet.


## The architecture

Andaman is a meta-package manager and build system that can build and install packages from multiple sources at once.

For RPMs, it parses an existing Yum repository as a base repository for packages, and integrates them with its own artifact repository.
It does this by reading the repo metadata for the yum repository, and then resolving the dependencies with the artifact repository it has.

## Roadmap

* [ ] Yum repository parsing
* [ ] Building RPMs and installing them
* [ ] Cargo support
* [ ] NPM/Yarn support
* [ ] PyPI support
* [ ] OCI containers
* [ ] (optional) building RPMs from a PKGBUILD-like script
* [ ] Support for non-Fedora/Ultramarine Linux dependencies (openSUSE)
* [ ] (optional) cross-distro support