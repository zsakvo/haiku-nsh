#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# NSH builds in .nsh/rust and launches artifacts from its explicit --target-dir.
# huacnlee/cargo ignores that option, leaving NSH to launch an older binary.
# Use Cargo from the active Rust toolchain without changing the global install.
toolchain_cargo="$(rustup which cargo)"
export PATH="$(dirname "$toolchain_cargo"):$PATH"
export CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=false

exec nsh "$@"
