#!/bin/bash
# One-shot install & setup for Demucs with ROCm GPU support
# Run: bash install.sh

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_DIR"

echo "=== Creating Python virtual environment ==="
python3 -m venv venv --system-site-packages

echo "=== Installing Demucs from source ==="
HSA_OVERRIDE_GFX_VERSION=10.3.0 ./venv/bin/pip install -e ".[dev]"

echo "=== Installing ROCm torchaudio ==="
HSA_OVERRIDE_GFX_VERSION=10.3.0 ./venv/bin/pip install --index-url https://download.pytorch.org/whl/rocm7.2 --force-reinstall --no-deps torchaudio

echo "=== Making demucs.sh executable ==="
chmod +x demucs.sh

echo ""
echo "=== Setup complete! ==="
echo ""
echo "Usage options:"
echo ""
echo "  1) Run from repo (recommended for now):"
echo "     ./demucs.sh song.mp3"
echo "     ./demucs.sh --two-stems vocals song.mp3"
echo ""
echo "  2) Add to PATH (run once):"
echo '     ln -sf "'"$REPO_DIR"'/demucs.sh" ~/.local/bin/demucs'
echo '     hash -r  # refresh command cache'
echo '     # Now you can run: demucs song.mp3'
echo ""
echo "  3) Or add this to your ~/.bashrc or ~/.zshrc:"
echo '     export PATH="'"$REPO_DIR"':$PATH"'
echo '     alias demucs="HSA_OVERRIDE_GFX_VERSION=10.3.0 '"$REPO_DIR"'/venv/bin/python3 -m demucs"'
echo ""

# Test with included test file
echo "=== Quick test (20s demo track) ==="
./demucs.sh test.mp3 2>&1

echo ""
echo "=== Test complete! Output in: separated/htdemucs/test/ ==="
