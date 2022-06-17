

![Andaman Package Manager](assets/anda-medium.png)

# Andaman Package Manager

The Andaman Package Manager is a meta-package manager that allows you to easily build and manage packages from multiple sources.

It is based on Ultramarine Linux's `umpkg` project, and allows you to easily manage packages from multiple different package managers.

Andaman mainly uses a ports-like system as its main repository source, with a repository directly containing spec files for each package, which will then be built into RPMs by umpkg and Andaman.
It then integrates those with your existing Yum/DNF repositories, for extra binary dependency solving.

It is intended to be used as an extension of DNF and umpkg, and to supplement them with AUR/BSDPorts/Portage like features.

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