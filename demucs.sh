#!/bin/bash
# Demucs Music Source Separation - ROCm GPU wrapper
# Usage: demucs [options] <audio-file>...
# Run `demucs --help` for full options
# Examples:
#   demucs song.mp3                          # split into drums, bass, vocals, other
#   demucs --two-stems vocals song.mp3       # karaoke mode (vocals + no_vocals)
#   demucs --mp3 --mp3-bitrate 256 song.mp3  # output as mp3
#   demucs --flac -o ~/Music/separated *.mp3 # batch process all mp3s

set -euo pipefail

REPO_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
VENV_PYTHON="$REPO_DIR/venv/bin/python3"

if [ ! -f "$VENV_PYTHON" ]; then
    echo "Error: venv not found. Run setup first:" >&2
    echo "  python3 -m venv venv --system-site-packages" >&2
    echo "  ./venv/bin/pip install -e ." >&2
    exit 1
fi

# Silence AMD ROCm/MIOpen workspace tuning warnings (they're harmless)
export MIOPEN_LOG_LEVEL=0  # quiet mode
export HSA_OVERRIDE_GFX_VERSION=10.3.0
export TF_CPP_MIN_LOG_LEVEL=3

exec "$VENV_PYTHON" -m demucs "$@"
