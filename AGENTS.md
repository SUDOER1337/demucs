# Demucs + demucs-tui — AGENTS.md

## Repo layout

```
repo root (demucs/)
├── demucs/              # Python package — core library (pip install -e .)
│   ├── separate.py      # CLI entrypoint (python -m demucs)
│   ├── api.py           # Separator class, save_audio, list_models
│   ├── apply.py         # apply_model (bag-of-models inference)
│   ├── pretrained.py    # Model loading / listing
│   ├── train.py         # Training loop
│   └── evaluate.py      # Evaluation on MUSDB
├── demucs-tui/          # Rust TUI frontend
│   ├── src/main.rs      # Binary with ratatui + crossterm
│   ├── worker/demucs_worker.py  # Python child process (JSON-line IPC)
│   └── run.sh           # Build then run from repo root
├── demucs.sh            # Shell wrapper: exec venv/bin/python3 -m demucs "$@"
├── demucs-batch.sh      # Batch wrapper with --find mode
├── install.sh           # One-shot: venv + pip install -e ".[dev]" + ROCm torchaudio
├── flake.nix            # Nix dev shell: Python 3.13, flake8, mypy, cargo, etc.
├── flake.lock           # Pinned flake inputs (commit to repo)
├── test.mp3             # 20s test track used by CI & quick test
├── conf/                # Hydra configs (training)
├── docs/                # api.md, linux.md, mac.md, windows.md, training.md, etc.
├── tools/               # Bench, export, convert, automix scripts
└── venv/                # Local venv (gitignored, created by install.sh)
```

## Project ancestry

- Upstream: https://github.com/facebookresearch/demucs (unmaintained, v4.1.0a2)
- Active fork: https://github.com/adefossez/demucs
- This repo is a **local fork** customized for ROCm and with a Rust TUI added

## Setup (always from repo root)

### NixOS (recommended)

```bash
# Enter dev shell (provides Python 3.13, flake8, mypy, cargo, ffmpeg)
nix develop

# Create venv and install deps
make install-python

# Or full install with ROCm torchaudio
bash install.sh
```

The `flake.nix` provides a dev shell with Python 3.13, pip, flake8, mypy, yapf, cargo, rustfmt, clippy, just, and ffmpeg. ROCm env vars (`HSA_OVERRIDE_GFX_VERSION`, etc.) are set automatically.

### Non-NixOS

```bash
python3 -m venv venv --system-site-packages
HSA_OVERRIDE_GFX_VERSION=10.3.0 ./venv/bin/pip install -e ".[dev]"
```

- `install.sh` does the above, plus ROCm torchaudio from `https://download.pytorch.org/whl/rocm7.2`
- `requirements_minimal.txt` = runtime deps, `requirements.txt` = +dev (flake8, mypy, museval, etc.)
- Torch >=1.8.1, torchaudio >=0.8, Python >=3.8

## Key commands

| What                     | How                                                                             |
| ------------------------ | ------------------------------------------------------------------------------- |
| Run CLI separation       | `./demucs.sh song.mp3` (sets ROCm env, uses venv)                               |
| Run CLI separation (RAW) | `./venv/bin/python3 -m demucs -n htdemucs_ft song.mp3`                          |
| List available models    | `./venv/bin/python3 -m demucs --list-models`                                    |
| Use Python API           | `./venv/bin/python3 -c "import demucs.api; s=demucs.api.Separator(); ..."`      |
| Build TUI                | `cd demucs-tui && cargo build`                                                  |
| Run TUI                  | `./demucs-tui/run.sh` (from repo root — sets `REPO_DIR` and execs debug binary) |
| Lint                     | `make linter` → `flake8 demucs && mypy demucs`                                  |
| Eval tests               | `make test_eval` (separates test.mp3 with various flags)                        |
| Training test            | `make test_train` (needs MUSDB dataset, see Makefile)                           |
| Clean build artifacts    | `make clean`                                                                    |

**Architecture note:** The TUI binary (`demucs-tui/target/debug/demucs-tui`) finds the repo root by walking up from the binary's location or cwd looking for a `demucs/` directory. It then spawns `venv/bin/python3 demucs-tui/worker/demucs_worker.py` as a child process.

## ROCm / AMD GPU quirks

Every invocation of Demucs needs these env vars set:

```bash
HSA_OVERRIDE_GFX_VERSION=10.3.0
MIOPEN_LOG_LEVEL=0
MIOPEN_ENABLE_LOGGING=0
```

Pip installs for torch/torchaudio also need `HSA_OVERRIDE_GFX_VERSION`. The worker script (`demucs-tui/worker/demucs_worker.py`) sets these via `os.environ.setdefault()`.

## Linting & typechecking rules

- `flake8` max line length: 100 (in `setup.cfg`)
- `yapf` column_limit: 100
- `mypy` ignores missing imports for: `treetable`, `torchaudio.*`, `diffq`, `yaml`, `tqdm`, `lameenc`, `musdb`, `museval`, `openunmix.*`, `einops`, `xformers.*`
- CI runs `make linter` (flake8 + mypy on `demucs/` only)

## TUI IPC protocol

The Python worker writes JSON lines to stdout:

```json
{"type": "progress", "pct": 50.0, "msg": "Model 1/1 | Shift 1 | Segment 0"}
{"type": "done", "msg": "Separation complete!", "files": [...], "output_dir": "...", "track": "..."}
{"type": "error", "msg": "..."}
```

The Rust binary reads these from the child's stdout via `BufReader` + `mpsc` channel.

## Testing quirks

- `test_eval` tests use `demucs_unittest` model (a tiny model in the repo, not downloaded)
- Training tests need `tests/musdb/` — download with `make tests/musdb` (requires musdb + musdbconvert)
- CI installs `ffmpeg` via apt (required by torchaudio 0.12+ for mp3 decoding)

## Output

Default output: `separated/{model_name}/{track_name}/{stem}.{ext}` (e.g. `separated/htdemucs/test/vocals.wav`)
