#!/bin/bash

# Install rust using rustup
curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain stable --profile complete -y

source "$HOME/.cargo/env"

# cargo install sea-orm-cli

# curl -fsSL https://get.pnpm.io/install.sh | sh -
# curl -s https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | bash

echo "export PNPM_HOME=\"/home/vscode/.local/share/pnpm\"" >> "$HOME/.zshrc"
echo "export PATH=\"\$PNPM_HOME:$PATH\"" >> "$HOME/.zshrc"

sudo dnf module enable nodejs:16 -y

sudo dnf install -y just nodejs dotnet-runtime-3.1 htop

#sudo /usr/local/share/docker-init.sh
sudo ln -sf /usr/libexec/docker/docker-proxy /usr/bin/docker-proxy

# cat <<EOF | sudo tee /etc/yum.repos.d/kubernetes.repo
# [kubernetes]
# name=Kubernetes
# baseurl=https://packages.cloud.google.com/yum/repos/kubernetes-el7-\$basearch
# enabled=1
# gpgcheck=1
# gpgkey=https://packages.cloud.google.com/yum/doc/yum-key.gpg https://packages.cloud.google.com/yum/doc/rpm-package-key.gpg
# EOF
# sudo yum install -y kubectl

# wget https://github.com/moby/buildkit/releases/download/v0.10.3/buildkit-v0.10.3.linux-amd64.tar.gz -O /tmp/buildkit.tar.gz

# sudo tar -xzf /tmp/buildkit.tar.gz -C /usr/local/

# wget https://dl.min.io/client/mc/release/linux-amd64/mc
# chmod +x mc
# sudo mv mc /usr/local/bin/mc

# echo "127.0.0.1 local-registry" | sudo tee -a /etc/hosts