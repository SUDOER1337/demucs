# Demucs + demucs-tui — just command runner
# Run `just --list` to see available recipes.

# ─── Default ───────────────────────────────────────────────────────────────────
default: linter tui-check

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

# Build + run the TUI from repo root
tui-run: tui-build
    ./demucs-tui/target/debug/demucs-tui

# Run rustfmt on the TUI
tui-format:
    cd demucs-tui && cargo fmt

# Install the TUI binary to ~/.cargo/bin/ via cargo
tui-install:
    cargo install --path demucs-tui

# ─── Cleanup ───────────────────────────────────────────────────────────────────

# Clean Python build artifacts
clean:
    rm -rf build/ dist/ *.egg-info
    find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true

# Clean everything (Python + Rust)
clean-all: clean
    cd demucs-tui && cargo clean
