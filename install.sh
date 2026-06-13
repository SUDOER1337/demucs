#!/bin/bash
# One-shot install & setup for Demucs with ROCm GPU support.
# Run: bash install.sh [--skip-test] [--skip-rocm-torchaudio] [--python-only]

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: bash install.sh [options]

Options:
  --skip-test              Do not run the quick test.mp3 separation after setup.
  --skip-rocm-torchaudio   Do not reinstall torchaudio from the ROCm wheel index.
  --python-only            Only create/update the venv and install Demucs dev deps.
                           Implies --skip-test and --skip-rocm-torchaudio.
  -h, --help               Show this help message.

Environment variables:
  SKIP_TEST=1              Same as --skip-test.
  SKIP_ROCM_TORCHAUDIO=1   Same as --skip-rocm-torchaudio.
  PYTHON_ONLY=1            Same as --python-only.
EOF
}

SKIP_TEST="${SKIP_TEST:-0}"
SKIP_ROCM_TORCHAUDIO="${SKIP_ROCM_TORCHAUDIO:-0}"
PYTHON_ONLY="${PYTHON_ONLY:-0}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-test)
            SKIP_TEST=1
            ;;
        --skip-rocm-torchaudio)
            SKIP_ROCM_TORCHAUDIO=1
            ;;
        --python-only)
            PYTHON_ONLY=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [[ "$PYTHON_ONLY" == "1" ]]; then
    SKIP_TEST=1
    SKIP_ROCM_TORCHAUDIO=1
fi

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_DIR"

export HSA_OVERRIDE_GFX_VERSION="${HSA_OVERRIDE_GFX_VERSION:-10.3.0}"
export MIOPEN_LOG_LEVEL="${MIOPEN_LOG_LEVEL:-0}"
export MIOPEN_ENABLE_LOGGING="${MIOPEN_ENABLE_LOGGING:-0}"

PIP="./venv/bin/pip"
PYTHON="./venv/bin/python3"

echo "=== Creating/updating Python virtual environment ==="
python3 -m venv venv --system-site-packages

echo "=== Installing Demucs from source ==="
"$PIP" install -e ".[dev]"

if [[ "$SKIP_ROCM_TORCHAUDIO" == "1" ]]; then
    echo "=== Skipping ROCm torchaudio install ==="
else
    echo "=== Installing ROCm torchaudio ==="
    "$PIP" install --index-url https://download.pytorch.org/whl/rocm7.2 --force-reinstall --no-deps torchaudio
fi

echo "=== Making local scripts executable ==="
chmod +x demucs.sh demucs-batch.sh demucs-tui/run.sh

echo ""
echo "=== Setup complete! ==="
echo ""
echo "Usage options:"
echo ""
echo "  1) Run from repo:"
echo "     ./demucs.sh song.mp3"
echo "     ./demucs.sh --two-stems vocals song.mp3"
echo ""
echo "  2) Run via Python module:"
echo "     $PYTHON -m demucs -n htdemucs_ft song.mp3"
echo ""
echo "  3) Run the TUI:"
echo "     ./demucs-tui/run.sh"
echo ""
echo "  4) Add to PATH (run once):"
echo '     ln -sf "'"$REPO_DIR"'/demucs.sh" ~/.local/bin/demucs'
echo '     hash -r  # refresh command cache'
echo '     # Now you can run: demucs song.mp3'
echo ""
echo "  5) Or add this to your ~/.bashrc or ~/.zshrc:"
echo '     export PATH="'"$REPO_DIR"':$PATH"'
echo '     alias demucs="HSA_OVERRIDE_GFX_VERSION=10.3.0 '"$REPO_DIR"'/venv/bin/python3 -m demucs"'
echo ""

if [[ "$SKIP_TEST" == "1" ]]; then
    echo "=== Skipping quick test ==="
else
    echo "=== Quick test (20s demo track) ==="
    ./demucs.sh test.mp3 2>&1
    echo ""
    echo "=== Test complete! Output in: separated/htdemucs/test/ ==="
fi
