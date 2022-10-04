# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.238.1/containers/rust/.devcontainer/base.Dockerfile

FROM registry.fedoraproject.org/fedora:37

COPY library-scripts/ /tmp/library-scripts/
RUN bash /tmp/library-scripts/common-redhat.sh
RUN bash /tmp/library-scripts/docker-redhat.sh
COPY scripts/postinstall.sh /usr/share/scripts/

RUN dnf groupinstall -y "Development Tools" "RPM Development Tools"
RUN dnf install -y openssl-devel flatpak flatpak-builder dnf-plugins-core
RUN flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo

# Add Andaman repo
RUN dnf config-manager --add-repo https://raw.githubusercontent.com/andaman-common-pkgs/subatomic-repos/main/andaman.repo

RUN dnf install -y anda-mock-configs anda-srpm-macros

ENTRYPOINT ["/usr/local/share/docker-init.sh"]
VOLUME [ "/var/lib/docker" ]
CMD ["sleep", "infinity"]