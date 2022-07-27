#!/bin/bash

# Install rust using rustup
curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain stable --profile complete -y

source "$HOME/.cargo/env"

cargo install sea-orm-cli 

curl -fsSL https://get.pnpm.io/install.sh | sh -

echo "export PNPM_HOME=\"/home/vscode/.local/share/pnpm\"" >> "$HOME/.zshrc"
echo "export PATH=\"\$PNPM_HOME:$PATH\"" >> "$HOME/.zshrc"

sudo dnf module enable nodejs:16 -y

sudo dnf install -y just nodejs 