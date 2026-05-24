#!/usr/bin/env python3
"""Demucs worker — spawned by demucs-tui.
Reads config from JSON file at sys.argv[1], prints JSON progress to stdout.
"""

import json
import os
import sys
from pathlib import Path


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"type": "error", "msg": "Missing config path"}))
        sys.exit(1)

    with open(sys.argv[1]) as f:
        cfg = json.load(f)

    # Set ROCm env
    os.environ.setdefault("HSA_OVERRIDE_GFX_VERSION", "10.3.0")
    os.environ.setdefault("MIOPEN_LOG_LEVEL", "0")
    os.environ["MIOPEN_ENABLE_LOGGING"] = "0"

    track = cfg["track"]
    model_name = cfg.get("model", "htdemucs")
    device = cfg.get("device", "cuda")
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

    from demucs.api import Separator, save_audio
    from demucs.pretrained import ModelLoadingError

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
    except ModelLoadingError as e:
        _send({"type": "error", "msg": f"Model loading failed: {e}"})
        sys.exit(1)
    except Exception as e:
        _send({"type": "error", "msg": f"Error: {e}"})
        sys.exit(1)

    progress = {"state": "separating", "pct": 0.0, "msg": "Separating..."}
    _send(progress)

    try:
        origin, sources = separator.separate_audio_file(Path(track))
    except Exception as e:
        _send({"type": "error", "msg": f"Separation failed: {e}"})
        sys.exit(1)

    # Save output
    out_root = Path(output_dir) / model_name
    track_name = Path(track).stem
    out_dir = out_root / track_name
    out_dir.mkdir(parents=True, exist_ok=True)

    ext = out_format
    saved = []
    stems_to_save = list(sources.items())

    if two_stems:
        # Only save the requested stem and "no_{stem}"
        if two_stems not in sources:
            _send({"type": "error", "msg": f"Stem '{two_stems}' not in model"})
            sys.exit(1)
        # Save selected stem
        stem_name = two_stems
        stem_path = out_dir / f"{stem_name}.{ext}"
        save_audio(
            sources[stem_name],
            str(stem_path),
            samplerate=separator.samplerate,
            clip=clip_mode,
            bits_per_sample=bits,
            as_float=as_float,
        )
        saved.append(str(stem_path))

        # Build "no_{stem}" from all other sources
        import torch

        other = torch.zeros_like(next(iter(sources.values())))
        for name, source in sources.items():
            if name != two_stems:
                other += source
        other_path = out_dir / f"no_{two_stems}.{ext}"
        save_audio(
            other,
            str(other_path),
            samplerate=separator.samplerate,
            clip=clip_mode,
            bits_per_sample=bits,
            as_float=as_float,
        )
        saved.append(str(other_path))
    else:
        for stem_name, source in stems_to_save:
            stem_path = out_dir / f"{stem_name}.{ext}"
            save_audio(
                source,
                str(stem_path),
                samplerate=separator.samplerate,
                clip=clip_mode,
                bits_per_sample=bits,
                as_float=as_float,
            )
            saved.append(str(stem_path))

    _send(
        {
            "type": "done",
            "msg": "Separation complete!",
            "files": saved,
            "output_dir": str(out_dir),
            "track": track,
        }
    )


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
    main()
