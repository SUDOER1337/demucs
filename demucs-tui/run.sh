#!/bin/bash
# Build and run demucs-tui from the repo root.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_DIR"

cargo build --manifest-path demucs-tui/Cargo.toml
export DEMUCS_REPO_DIR="$REPO_DIR"
exec ./demucs-tui/target/debug/demucs-tui
