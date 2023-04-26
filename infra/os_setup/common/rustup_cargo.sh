#!/usr/bin/env bash
set -eo pipefail

command -v rustup || curl --proto '=https' --tlsv1.2 -sSf "https://sh.rustup.rs" | sh -s -- -y
source "${HOME}/.cargo/env"

# Install sccache if not present.
# For some reason, `cargo install sccache` installs it from scratch even if it's present - which is not normal
command -v sccache || RUSTC_WRAPPER="" cargo install sccache

cargo install cargo-run-script
cargo install grcov
cargo install cargo-print
