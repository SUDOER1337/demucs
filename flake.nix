{
  description = "Demucs — music source separation (Python + Rust TUI)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        python = pkgs.python313;

        # ─── Packages not in nixpkgs ──────────────────────────────────────────
        lameenc = python.pkgs.buildPythonPackage rec {
          pname = "lameenc";
          version = "1.8.4";
          format = "setuptools";
          src = pkgs.fetchFromGitHub {
            owner = "chrisstaite";
            repo = "lameenc";
            rev = "v${version}";
            hash = "sha256-x7TGwIEpKgDkIeMFVc/RHhXqisy0UyRqMxl/MUsFDoE=";
          };
          postPatch = ''
            cat > setup.py << 'PYEOF'
            import setuptools
            lameenc = setuptools.Extension(
                'lameenc',
                include_dirs=['${pkgs.lame}/include/lame'],
                libraries=['mp3lame'],
                sources=['lameenc.c']
            )
            setuptools.setup(
                name='lameenc',
                version='${version}',
                ext_modules=[lameenc],
            )
            PYEOF
          '';
          nativeBuildInputs = [pkgs.gcc pkgs.pkg-config];
          buildInputs = [pkgs.lame.lib];
          doCheck = false;
          meta = with pkgs.lib; {
            description = "LAME encoder Python bindings";
            homepage = "https://github.com/chrisstaite/lameenc";
            license = licenses.lgpl21;
          };
        };

        treetable = python.pkgs.buildPythonPackage rec {
          pname = "treetable";
          version = "0.2.6";
          format = "pyproject";
          src = pkgs.fetchFromGitHub {
            owner = "adefossez";
            repo = "treetable";
            rev = "v${version}";
            hash = "sha256-UNc0DeKcuSnTnMHdn3wS3smuuqGfPArnBCMCV7a3vMA=";
          };
          nativeBuildInputs = with python.pkgs; [setuptools];
          doCheck = false;
          meta = with pkgs.lib; {
            description = "Tree rendering for terminal tables";
            homepage = "https://github.com/adefossez/treetable";
            license = licenses.mit;
          };
        };

        dora-search = python.pkgs.buildPythonPackage rec {
          pname = "dora-search";
          version = "0.1.12";
          format = "setuptools";
          src = pkgs.fetchFromGitHub {
            owner = "facebookresearch";
            repo = "dora";
            rev = "v${version}";
            hash = "sha256-v18FgiBdlNSGQmCnq63wCxcO8kJCPsUt0VznUlSPyoM=";
          };
          propagatedBuildInputs = with python.pkgs; [treetable omegaconf submitit retrying];
          doCheck = false;
          meta = with pkgs.lib; {
            description = "Easy grid searches for ML";
            homepage = "https://github.com/facebookresearch/dora";
            license = licenses.mit;
          };
        };

        openunmix = python.pkgs.buildPythonPackage rec {
          pname = "openunmix";
          version = "1.3.0";
          format = "pyproject";
          src = pkgs.fetchFromGitHub {
            owner = "sigsep";
            repo = "open-unmix-pytorch";
            rev = "v${version}";
            hash = "sha256-7jsyQhDUAeQmD+cvPoOUbxOW5YBlAoe+IjDebS+GXaw=";
          };
          nativeBuildInputs = with python.pkgs; [setuptools];
          propagatedBuildInputs = with python.pkgs; [numpy tqdm torch torchaudio];
          doCheck = false;
          meta = with pkgs.lib; {
            description = "Open-Unmix music source separation";
            homepage = "https://github.com/sigsep/open-unmix-pytorch";
            license = licenses.mit;
          };
        };

        # ─── Demucs Python package ───────────────────────────────────────────
        demucs = python.pkgs.buildPythonPackage {
          pname = "demucs";
          version = "4.1.0a2";
          src = ./.;
          format = "setuptools";
          propagatedBuildInputs = [
            dora-search
            openunmix
            lameenc
            python.pkgs.einops
            python.pkgs.julius
            python.pkgs.pyyaml
            python.pkgs.soundfile
            python.pkgs.tqdm
            python.pkgs.scipy
            python.pkgs.torch
            python.pkgs.torchaudio
          ];
          nativeCheckInputs = [pkgs.ffmpeg];
          doCheck = false;
          meta = with pkgs.lib; {
            description = "Music source separation in the waveform domain";
            homepage = "https://github.com/facebookresearch/demucs";
            license = licenses.mit;
            mainProgram = "demucs";
          };
        };

        # ─── Python env with demucs (for TUI worker) ────────────────────────────
        demucs-python = python.withPackages (ps: [demucs]);

        # ─── Demucs TUI ──────────────────────────────────────────────────────
        demucs-tui = pkgs.rustPlatform.buildRustPackage {
          pname = "demucs-tui";
          version = "0.1.0";
          src = ./demucs-tui;
          cargoLock.lockFile = ./demucs-tui/Cargo.lock;
          nativeBuildInputs = [pkgs.pkg-config pkgs.makeWrapper];
          buildInputs = [pkgs.openssl pkgs.fontconfig];

          postInstall = ''
            mkdir -p $out/share/demucs-tui
            cp ${./demucs-tui/worker/demucs_worker.py} $out/share/demucs-tui/demucs_worker.py

            mv $out/bin/demucs-tui $out/bin/demucs-tui-unwrapped
            makeWrapper $out/bin/demucs-tui-unwrapped $out/bin/demucs-tui \
              --set DEMUCS_PYTHON "${demucs-python}/bin/python3" \
              --set DEMUCS_WORKER_PATH "$out/share/demucs-tui/demucs_worker.py"
          '';

          meta = with pkgs.lib; {
            description = "TUI for Demucs music source separation";
            homepage = "https://github.com/adefossez/demucs";
            license = licenses.mit;
            mainProgram = "demucs-tui-unwrapped";
          };
        };

        # ─── Full bundle: demucs CLI + TUI ────────────────────────────────────
        demucs-full = pkgs.symlinkJoin {
          name = "demucs-full";
          paths = [demucs demucs-tui];
          nativeBuildInputs = [pkgs.makeWrapper];
          postBuild = ''
            wrapProgram $out/bin/demucs-tui-unwrapped \
              --set DEMUCS_PYTHON "${demucs-python}/bin/python3" \
              --set DEMUCS_WORKER_PATH "${demucs-tui}/share/demucs-tui/demucs_worker.py"
          '';
          meta.mainProgram = "demucs";
        };

      in {
        packages = {
          default = demucs-tui;
          inherit demucs demucs-tui demucs-full;
        };

        apps = {
          demucs = {
            type = "app";
            program = "${demucs}/bin/demucs";
          };
          demucs-tui = {
            type = "app";
            program = "${demucs-tui}/bin/demucs-tui-unwrapped";
          };
        };

        devShells.default = pkgs.mkShell {
          name = "demucs-dev";
          buildInputs = [
            (python.withPackages (ps: with ps; [pip setuptools wheel]))
            python.pkgs.flake8
            python.pkgs.mypy
            python.pkgs.yapf
            pkgs.rustc
            pkgs.cargo
            pkgs.rustfmt
            pkgs.clippy
            pkgs.just
            pkgs.ffmpeg
            pkgs.pkg-config
            pkgs.openssl
          ];
          shellHook = ''
            export HSA_OVERRIDE_GFX_VERSION="''${HSA_OVERRIDE_GFX_VERSION:-10.3.0}"
            export MIOPEN_LOG_LEVEL="''${MIOPEN_LOG_LEVEL:-0}"
            export MIOPEN_ENABLE_LOGGING="''${MIOPEN_ENABLE_LOGGING:-0}"
            echo "demucs dev shell — Python $(python3 --version | awk '{print $2}'), Rust $(rustc --version | awk '{print $2}')"
            echo "  nix build .#demucs       # build demucs CLI"
            echo "  nix build .#demucs-tui   # build TUI"
            echo "  nix build .#demucs-full  # build both"
            echo "  make install-python      # create venv"
            echo "  make linter              # flake8 + mypy"
          '';
        };
      });
}
