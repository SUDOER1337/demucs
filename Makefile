PYTHON ?= ./venv/bin/python3
PIP ?= ./venv/bin/pip
YAPF ?= ./venv/bin/yapf
CARGO ?= cargo
MUSDBCONVERT ?= ./venv/bin/musdbconvert
ROCM_TORCHAUDIO_INDEX ?= https://download.pytorch.org/whl/rocm7.2

ROCM_ENV = HSA_OVERRIDE_GFX_VERSION=10.3.0 MIOPEN_LOG_LEVEL=0 MIOPEN_ENABLE_LOGGING=0

all: linter tests

help: ## Show available development targets.
	@awk 'BEGIN {FS = ":.*## "; printf "Available targets:\n"} /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-24s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Full local install: venv, dev deps, ROCm torchaudio, and quick separation test.
	bash ./install.sh

install-no-test: ## Install everything but skip the quick separation test.
	SKIP_TEST=1 bash ./install.sh

install-python: ## Create/update venv and install Demucs with dev dependencies.
	python3 -m venv venv --system-site-packages
	$(ROCM_ENV) $(PIP) install -e ".[dev]"

install-rocm-torchaudio: ## Reinstall ROCm torchaudio wheel into the local venv.
	$(ROCM_ENV) $(PIP) install --index-url $(ROCM_TORCHAUDIO_INDEX) --force-reinstall --no-deps torchaudio

linter: ## Run Python linting and type checking.
	$(PYTHON) -m flake8 demucs
	$(PYTHON) -m mypy demucs

format: format-python format-rust ## Format Python and Rust sources.

format-python: ## Format Python sources with yapf.
	$(YAPF) -ri demucs/ demucs-tui/worker/

format-rust: ## Format Rust TUI sources.
	$(CARGO) fmt --manifest-path demucs-tui/Cargo.toml

format-check: format-rust-check ## Check formatting without modifying files.

format-rust-check: ## Check Rust formatting without modifying files.
	$(CARGO) fmt --manifest-path demucs-tui/Cargo.toml -- --check

rust-check: ## Run Rust formatting check, clippy, and tests for the TUI.
	$(CARGO) fmt --manifest-path demucs-tui/Cargo.toml -- --check
	$(CARGO) clippy --manifest-path demucs-tui/Cargo.toml
	$(CARGO) test --manifest-path demucs-tui/Cargo.toml

tui-build: ## Build the Rust TUI.
	$(CARGO) build --manifest-path demucs-tui/Cargo.toml

tui-run: ## Build and run the Rust TUI.
	./demucs-tui/run.sh

tui-install: ## Install the Rust TUI binary with cargo install.
	$(CARGO) install --path demucs-tui

smoke: ## Fast import/list-model smoke test using the local venv.
	$(ROCM_ENV) $(PYTHON) -c "import demucs.api; print(demucs.api.list_models())"
	$(ROCM_ENV) $(PYTHON) -m demucs --list-models

list-models: ## List available Demucs models.
	$(ROCM_ENV) $(PYTHON) -m demucs --list-models

tests: test_train test_eval

test_eval_one: ## Run one quick separation against the tiny unittest model.
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest test.mp3

test_eval: ## Run separation evaluation checks against the tiny unittest model.
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest --two-stems=vocals test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest --mp3 test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest --flac --int24 test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest --int24 --clip-mode clamp test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs -n demucs_unittest --segment 8 test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs.api -n demucs_unittest --segment 8 test.mp3
	$(ROCM_ENV) $(PYTHON) -m demucs --list-models

test_train: tests/musdb ## Run the small training test. Requires MUSDB test data.
	_DORA_TEST_PATH=/tmp/demucs $(ROCM_ENV) $(PYTHON) -m dora run --clear \
		dset.musdb=./tests/musdb dset.segment=4 dset.shift=2 epochs=2 model=demucs \
		demucs.depth=2 demucs.channels=4 test.sdr=false misc.num_workers=0 test.workers=0 \
		test.shifts=0

tests/musdb:
	test -e tests || mkdir tests
	$(PYTHON) -c 'import musdb; musdb.DB("tests/tmp", download=True)'
	$(MUSDBCONVERT) tests/tmp tests/musdb

dist: ## Build a source distribution.
	$(PYTHON) setup.py sdist

clean: clean-python clean-rust clean-outputs ## Remove generated build/test/output artifacts.

clean-python: ## Remove Python build/typecheck artifacts.
	rm -rf dist build *.egg-info .mypy_cache

clean-rust: ## Remove Rust build artifacts.
	rm -rf demucs-tui/target

clean-outputs: ## Remove generated separation/test outputs.
	rm -rf separated outputs tests/tmp

.PHONY: all help install install-no-test install-python install-rocm-torchaudio linter \
	format format-python format-rust format-check format-rust-check rust-check \
	tui-build tui-run tui-install smoke list-models tests test_eval_one test_eval test_train dist \
	clean clean-python clean-rust clean-outputs
