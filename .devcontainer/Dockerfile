# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.238.1/containers/rust/.devcontainer/base.Dockerfile

FROM fedora:36

COPY library-scripts/ /tmp/library-scripts/
RUN bash /tmp/library-scripts/common-redhat.sh
RUN bash /tmp/library-scripts/docker-redhat.sh
COPY scripts/postinstall.sh /usr/share/scripts/

RUN dnf groupinstall -y "Development Tools"
RUN dnf groupinstall -y "RPM Development Tools"
RUN dnf install -y openssl-devel flatpak flatpak-builder
RUN flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
ENTRYPOINT ["/usr/local/share/docker-init.sh"]
VOLUME [ "/var/lib/docker" ]
CMD ["sleep", "infinity"]