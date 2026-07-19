#!/usr/bin/env python3
"""Demucs worker — spawned by demucs-tui.
Reads config from JSON file at sys.argv[1], prints JSON progress to stdout.
"""

import json
import os
import sys
from pathlib import Path


LOG_FILE = "/tmp/demucs-worker.log"
_log_handle = None


def _get_log_handle():
    global _log_handle
    if _log_handle is None or _log_handle.closed:
        _log_handle = open(LOG_FILE, "a")
    return _log_handle


def _worker_log(msg: str) -> None:
    import datetime

    ts = datetime.datetime.now().isoformat(timespec="milliseconds")
    f = _get_log_handle()
    f.write(f"[{ts}] {msg}\n")
    f.flush()


def main():
    _worker_log("=== worker start ===")
    _worker_log(f"argv={sys.argv}")
    _worker_log(f"pid={os.getpid()}")
    _worker_log(f"cwd={os.getcwd()}")
    _worker_log(
        f"HSA_OVERRIDE_GFX_VERSION={os.environ.get('HSA_OVERRIDE_GFX_VERSION', 'NOT SET')}"
    )
    _worker_log(
        f"CUDA_VISIBLE_DEVICES={os.environ.get('CUDA_VISIBLE_DEVICES', 'NOT SET')}"
    )
    _worker_log(
        f"ROCR_VISIBLE_DEVICES={os.environ.get('ROCR_VISIBLE_DEVICES', 'NOT SET')}"
    )

    if len(sys.argv) < 2:
        _worker_log("ERROR: Missing config path")
        print(json.dumps({"type": "error", "msg": "Missing config path"}))
        sys.exit(1)

    config_path = sys.argv[1]
    _worker_log(f"config_path={config_path}")

    with open(config_path) as f:
        cfg = json.load(f)
    _worker_log(f"config={json.dumps(cfg, default=str)}")

    # Set ROCm env
    os.environ.setdefault("HSA_OVERRIDE_GFX_VERSION", "10.3.0")
    os.environ.setdefault("MIOPEN_LOG_LEVEL", "0")
    os.environ["MIOPEN_ENABLE_LOGGING"] = "0"
    _worker_log(
        f"ROCm env after setdefault: HSA_OVERRIDE_GFX_VERSION={os.environ['HSA_OVERRIDE_GFX_VERSION']}"
    )

    track = cfg["track"]
    model_name = cfg.get("model", "htdemucs")
    device = cfg.get("device", "auto")
    if device == "auto":
        try:
            import torch

            device = "cuda" if torch.cuda.is_available() else "cpu"
            _worker_log(
                f"auto-detect: torch.cuda.is_available()={torch.cuda.is_available()}, device={device}"
            )
        except ImportError:
            device = "cpu"
            _worker_log("auto-detect: torch not available, falling back to cpu")
    shifts = cfg.get("shifts", 1)
    overlap = cfg.get("overlap", 0.25)
    segment = cfg.get("segment")
    split = cfg.get("split", True)
    two_stems = cfg.get("two_stems")
    output_dir = cfg.get("output_dir", "separated")
    out_format = cfg.get("format", "wav")
    mp3_bitrate = cfg.get("mp3_bitrate", 320)
    jobs = cfg.get("jobs", 0)
    clip_mode = cfg.get("clip_mode", "rescale")
    bits = cfg.get("bits_per_sample", 16)
    as_float = cfg.get("as_float", False)

    _worker_log(
        f"params: model={model_name} device={device} shifts={shifts} overlap={overlap} "
        f"segment={segment} split={split} two_stems={two_stems} output_dir={output_dir} "
        f"format={out_format} jobs={jobs} clip_mode={clip_mode} bits={bits} float={as_float}"
    )

    try:
        _worker_log("importing demucs.api...")
        from demucs.api import Separator
        from demucs.pretrained import ModelLoadingError

        _worker_log("imported demucs.api successfully")
    except Exception as e:
        _worker_log(f"FAILED to import demucs.api: {e}")
        import traceback

        _worker_log(traceback.format_exc())
        raise

    _send(
        {
            "type": "progress",
            "pct": 0.0,
            "model_idx": 0,
            "total_models": 1,
            "shift_idx": 0,
            "state": "start",
            "msg": "Loading model...",
        }
    )

    try:
        _worker_log("creating Separator instance...")
        separator = Separator(
            model=model_name,
            device=device,
            shifts=shifts,
            overlap=overlap,
            split=split,
            segment=segment,
            jobs=jobs,
            progress=False,
            callback=lambda d: _progress_cb(d),
        )
        _worker_log(f"Separator created, samplerate={separator.samplerate}")
    except ModelLoadingError as e:
        _worker_log(f"Model loading failed: {e}")
        import traceback

        _worker_log(traceback.format_exc())
        _send({"type": "error", "msg": f"Model loading failed: {e}"})
        sys.exit(1)
    except Exception as e:
        _worker_log(f"Error creating Separator: {e}")
        import traceback

        _worker_log(traceback.format_exc())
        _send({"type": "error", "msg": f"Error: {e}"})
        sys.exit(1)

    progress = {"state": "separating", "pct": 0.0, "msg": "Separating..."}
    _send(progress)

    try:
        _worker_log(f"calling separate_audio_file(track={track})...")
        origin, sources = separator.separate_audio_file(Path(track))
        _worker_log(
            f"separation done, sources={list(sources.keys())} | origin shape={origin.shape}"
        )
    except Exception as e:
        _worker_log(f"Separation failed: {e}")
        import traceback

        _worker_log(traceback.format_exc())
        _send({"type": "error", "msg": f"Separation failed: {e}"})
        sys.exit(1)

    # Save output
    out_root = Path(output_dir) / model_name
    track_name = Path(track).stem
    out_dir = out_root / track_name
    out_dir.mkdir(parents=True, exist_ok=True)
    _worker_log(f"output dir created: {out_dir}")

    try:
        from demucs.api import save_separated_stems

        saved = save_separated_stems(
            sources=sources,
            output_dir=out_dir,
            track_name="",
            samplerate=separator.samplerate,
            ext=out_format,
            two_stems=two_stems,
            two_stems_method="add",
            bitrate=320,
            clip=clip_mode,
            as_float=as_float,
            bits_per_sample=bits,
            filename_template="{stem}.{ext}",
        )
        _worker_log(f"saved {len(saved)} files: {saved}")
    except Exception as e:
        _worker_log(f"Failed to save stems: {e}")
        import traceback

        _worker_log(traceback.format_exc())
        _send({"type": "error", "msg": f"Failed to save stems: {e}"})
        sys.exit(1)

    _worker_log("sending done message")
    _send(
        {
            "type": "done",
            "msg": "Separation complete!",
            "files": saved,
            "output_dir": str(out_dir),
            "track": track,
        }
    )
    _worker_log("=== worker done (clean exit) ===")


def _progress_cb(data):
    """Called by demucs for each chunk/shift progress."""
    model_idx = data.get("model_idx_in_bag", 0)
    shift_idx = data.get("shift_idx", 0)
    segment_offset = data.get("segment_offset", 0)
    audio_length = data.get("audio_length", 1)
    state = data.get("state", "")
    total_models = data.get("models", 1)
    total_shifts = data.get("shifts", 1)

    if audio_length > 0:
        pct = min(segment_offset / audio_length, 1.0)
    else:
        pct = 0.0

    _worker_log(
        f"progress: pct={pct:.3f} model={model_idx}/{total_models} shift={shift_idx}/{total_shifts} "
        f"seg={segment_offset}/{audio_length} state={state}"
    )

    _send(
        {
            "type": "progress",
            "pct": round(pct * 100, 1),
            "model_idx": model_idx,
            "total_models": total_models,
            "shift_idx": shift_idx,
            "state": state,
            "msg": f"Model {model_idx + 1}/{total_models} | Shift {shift_idx + 1} | Segment {segment_offset}",
        }
    )


def _send(data):
    """Write JSON line to stdout and flush."""
    line = json.dumps(data, default=str)
    sys.stdout.write(line + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        _worker_log(f"Unhandled exception in main: {e}")
        import traceback

        _worker_log(traceback.format_exc())
        # Last resort: try to send error via stdout
        try:
            print(json.dumps({"type": "error", "msg": f"Worker crashed: {e}"}))
            sys.stdout.flush()
        except Exception:
            pass
        sys.exit(1)
