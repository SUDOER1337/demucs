#!/bin/bash
# Batch process multiple audio files with Demucs
# Usage:
#   ./demucs-batch.sh *.mp3                                  # all mp3s in current dir
#   ./demucs-batch.sh -o ~/separated ~/Music/*.flac          # output to custom dir
#   ./demucs-batch.sh --two-stems vocals songs/*.mp3         # karaoke mode for many files
#   ./demucs-batch.sh --find ~/Music                          # recursively find audio files

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEMUCS="$REPO_DIR/demucs.sh"

# Default output
OUT_DIR="separated"

# Collect positional args vs flags
DEMUCS_ARGS=()
FILES=()
FIND_MODE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        -o|--out)
            OUT_DIR="$2"
            shift 2
            ;;
        --find)
            FIND_MODE=true
            SEARCH_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Demucs Batch Processor"
            echo ""
            echo "Usage: $0 [demucs-options] [-o <dir>] [--find <dir>] <files...>"
            echo ""
            echo "  Processes all matching audio files through Demucs."
            echo "  Any flags before files are passed to demucs."
            echo ""
            echo "Examples:"
            echo "  $0 *.mp3"
            echo "  $0 --two-stems vocals song*.mp3"
            echo "  $0 --mp3 --mp3-bitrate 256 *.wav"
            echo "  $0 --find ~/Music -o ~/separated"
            echo ""
            echo "Models (use -n): htdemucs (default), htdemucs_ft, hdemucs_mmi, mdx, mdx_extra"
            exit 0
            ;;
        *)
            if [[ "$1" == -* ]]; then
                DEMUCS_ARGS+=("$1")
            else
                FILES+=("$1")
            fi
            shift
            ;;
    esac
done

if $FIND_MODE; then
    if [ -z "${SEARCH_DIR:-}" ]; then
        SEARCH_DIR="."
    fi
    echo "Searching for audio files in: $SEARCH_DIR"
    while IFS= read -r -d '' f; do
        FILES+=("$f")
    done < <(find "$SEARCH_DIR" \( -iname "*.mp3" -o -iname "*.wav" -o -iname "*.flac" -o -iname "*.ogg" -o -iname "*.m4a" -o -iname "*.aac" -o -iname "*.wma" \) -print0 2>/dev/null | sort -z)
fi

if [ ${#FILES[@]} -eq 0 ]; then
    echo "No audio files specified. Run with --help for usage." >&2
    exit 1
fi

echo "=== Demucs Batch ==="
echo "Files to process: ${#FILES[@]}"
echo "Output dir: $OUT_DIR"
echo "Demucs args: ${DEMUCS_ARGS[*]:-(none)}"
echo ""

TOTAL=${#FILES[@]}
COUNT=0
FAILED=0

for f in "${FILES[@]}"; do
    COUNT=$((COUNT + 1))
    echo "[$COUNT/$TOTAL] Processing: $f"
    if ! "$DEMUCS" -o "$OUT_DIR" "${DEMUCS_ARGS[@]}" "$f"; then
        echo "[FAILED] $f" >&2
        FAILED=$((FAILED + 1))
    fi
    echo ""
done

echo "=== Done ==="
echo "Processed: $COUNT, Failed: $FAILED"
echo "Output in: $OUT_DIR/"
