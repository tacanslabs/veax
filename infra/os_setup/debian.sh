#!/usr/bin/env bash
set -eo pipefail

sudo apt update
sudo apt install -y curl git pkg-config libssl-dev diffutils gcc m4 make
"$(dirname "$0")"/common/rustup_cargo.sh
