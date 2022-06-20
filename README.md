

![Andaman Package Manager](assets/anda-medium.png)

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