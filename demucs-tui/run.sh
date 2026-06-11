#!/bin/bash
# Build & run demucs-tui from the repo root
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_DIR"
exec ./demucs-tui/target/debug/demucs-tui
