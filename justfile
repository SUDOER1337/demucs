# Demucs + demucs-tui — just command runner
# Run `just --list` to see available recipes.

# ─── Default ───────────────────────────────────────────────────────────────────
default: install

# ─── Install ───────────────────────────────────────────────────────────────────

# Full install: venv + Python deps + build TUI + quick test
install:
    bash install.sh
    just tui-build
    echo "=== TUI binary ready at demucs-tui/target/debug/demucs-tui ==="

# Full install without the quick separation test
install-no-test:
    SKIP_TEST=1 bash install.sh
    just tui-build

# Python-only: venv + pip install, skip ROCm torchaudio and test
install-python:
    bash install.sh --python-only

# Install TUI binary to ~/.cargo/bin/ via cargo install
install-tui:
    cargo install --path demucs-tui
    pwd > "$HOME/.demucs_repo"
    echo "=== Installed ~/.cargo/bin/demucs-tui ==="
    echo "=== Wrote repo root to ~/.demucs_repo ==="
    echo "=== Run 'demucs-tui' from anywhere — it finds the venv via that file ==="

# Full everything: venv + pip + ROCm torchaudio + cargo install + test
install-full: install
    just install-tui

# ─── Python ────────────────────────────────────────────────────────────────────

# Lint Python: flake8 + mypy
linter:
    flake8 demucs
    mypy demucs

# Format Python with yapf
format:
    yapf -ri demucs/ demucs-tui/worker/

# Quick separation test (uses bundled test.mp3 + demucs_unittest model)
test-eval:
    python3 -m demucs -n demucs_unittest test.mp3
    python3 -m demucs -n demucs_unittest --two-stems=vocals test.mp3
    python3 -m demucs -n demucs_unittest --mp3 test.mp3
    python3 -m demucs -n demucs_unittest --flac --int24 test.mp3
    python3 -m demucs -n demucs_unittest --segment 8 test.mp3
    python3 -m demucs.api -n demucs_unittest --segment 8 test.mp3
    python3 -m demucs --list-models

# Separate an audio file (usage: just separate path/to/song.mp3)
separate track="test.mp3" model="htdemucs":
    ./demucs.sh -n {{model}} {{track}}

# List available models
models:
    ./venv/bin/python3 -m demucs --list-models

# ─── Rust TUI ─────────────────────────────────────────────────────────────────

# Build the TUI (debug)
tui-build:
    cd demucs-tui && cargo build

# Build the TUI (release)
tui-release:
    cd demucs-tui && cargo build --release

# Run clippy on the TUI
tui-check:
    cd demucs-tui && cargo clippy

# Build + run the TUI from repo root (exports DEMUCS_REPO_DIR so the binary can find the venv)
tui-run: tui-build
    DEMUCS_REPO_DIR=. ./demucs-tui/target/debug/demucs-tui

# Run the TUI via the shell wrapper (builds first, sets env automatically)
tui-wrapper:
    ./demucs-tui/run.sh

# Run rustfmt on the TUI
tui-format:
    cd demucs-tui && cargo fmt

# ─── Cleanup ───────────────────────────────────────────────────────────────────

# Clean Python build artifacts
clean:
    rm -rf build/ dist/ *.egg-info
    find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true

# Clean everything (Python + Rust)
clean-all: clean
    cd demucs-tui && cargo clean
