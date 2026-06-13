# Development

This fork keeps the upstream Demucs Python package and adds a local Rust TUI in `demucs-tui/`.
Most commands below assume you are running from the repository root.

## Setup

Full local setup, including a quick separation test:

```bash
make install
```

Faster setup without the quick test:

```bash
make install-no-test
```

Python-only setup, useful when you do not want to reinstall ROCm torchaudio:

```bash
./install.sh --python-only
```

The installer also accepts environment flags:

```bash
SKIP_TEST=1 ./install.sh
SKIP_ROCM_TORCHAUDIO=1 ./install.sh
PYTHON_ONLY=1 ./install.sh
```

## ROCm environment

Demucs invocations in this fork expect these defaults:

```bash
export HSA_OVERRIDE_GFX_VERSION=10.3.0
export MIOPEN_LOG_LEVEL=0
export MIOPEN_ENABLE_LOGGING=0
```

`install.sh`, `demucs.sh`, and the TUI worker set these where needed. If you use `direnv`, copy the example:

```bash
cp .envrc.example .envrc
direnv allow
```

## Common commands

```bash
make help                    # list Makefile targets
make linter                  # flake8 + mypy on demucs/
make smoke                   # fast import/list-model check
make list-models             # list Demucs models with ROCm env vars
make test_eval_one           # one tiny-model separation
make test_eval               # full tiny-model separation matrix
make format                  # format Python and Rust sources
make rust-check              # cargo fmt --check + clippy + tests
make clean                   # remove generated artifacts
```

The Makefile uses the local virtualenv by default:

```make
PYTHON ?= ./venv/bin/python3
PIP ?= ./venv/bin/pip
```

Override these if needed:

```bash
make PYTHON=python3 linter
```

## TUI development

Build the TUI:

```bash
make tui-build
```

Build and run it:

```bash
make tui-run
```

Install the binary with Cargo:

```bash
make tui-install
```

`demucs-tui/run.sh` builds first, then runs `demucs-tui/target/debug/demucs-tui`.
The TUI spawns `demucs-tui/worker/demucs_worker.py` and communicates with it over JSON lines.

## Zed tasks

Zed tasks live in `.zed/tasks.json` and mostly call Makefile targets so terminal and editor workflows stay aligned.
Open Zed's task picker and use tasks such as:

- `Install project (skip quick test)`
- `Smoke test`
- `Test separation (single)`
- `Build TUI`
- `Run TUI`
- `Check Rust TUI`

## Pre-commit hooks

Optional hooks are configured in `.pre-commit-config.yaml`.
Install and run them with:

```bash
./venv/bin/pip install pre-commit
./venv/bin/pre-commit install
./venv/bin/pre-commit run --all-files
```

The hooks check JSON/YAML, final newlines, trailing whitespace, Python formatting via `yapf`, and Rust formatting via `cargo fmt --check`.

## CI

GitHub Actions now run on this fork without the old `facebookresearch/demucs` repository guard. They create a local `venv/`, matching the default Makefile commands.
