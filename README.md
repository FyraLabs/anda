# Andaman Package Manager

The Andaman Package Manager is a meta-package manager that allows you to easily build and manage packages from multiple sources.

It is based on Ultramarine Linux's umpkg project, and allows you to easily manage packages from multiple different package managers.

Andaman mainly uses a ports-like system as its main repository source, with a repository directly containing spec files for each package, which will then be built into RPMs by umpkg and Andaman.

It is intended to be used as an extension of DNF and umpkg, and to supplement them with AUR/BSDPorts/Portage like features.