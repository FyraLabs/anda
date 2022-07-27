# See here for image contents: https://github.com/microsoft/vscode-dev-containers/tree/v0.238.1/containers/rust/.devcontainer/base.Dockerfile

FROM fedora:36

COPY library-scripts/common-redhat.sh /tmp/library-scripts/
RUN bash /tmp/library-scripts/common-redhat.sh

COPY scripts/postinstall.sh /usr/share/scripts/

RUN dnf groupinstall -y "Development Tools"
RUN dnf groupinstall -y "RPM Development Tools"
RUN dnf groupinstall -y openssl-devel