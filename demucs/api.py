# Copyright (c) Meta Platforms, Inc. and affiliates.
# All rights reserved.
#
# This source code is licensed under the license found in the
# LICENSE file in the root directory of this source tree.

"""API methods for demucs

Classes
-------
`demucs.api.Separator`: The base separator class

Functions
---------
`demucs.api.save_audio`: Save an audio
`demucs.api.save_separated_stems`: Save separated stems to disk
`demucs.api.list_models`: Get models list

Examples
--------
See the end of this module (if __name__ == "__main__")
"""

import subprocess

import torch as th
import torchaudio as ta

from pathlib import Path
from typing import Optional, Callable, Dict, Tuple, Union

from .apply import apply_model, _replace_dict
from .audio import AudioFile, convert_audio, save_audio
from .pretrained import get_model, build_repos
from .repo import ModelLoadingError  # noqa: F401 — used in docstrings


class LoadAudioError(Exception):
    pass


class LoadModelError(Exception):
    pass


class _NotProvided:
    pass


NotProvided = _NotProvided()


class Separator:
    def __init__(
        self,
        model: str = "htdemucs",
        repo: Optional[Path] = None,
        device: str = "cuda" if th.cuda.is_available() else "cpu",
        shifts: int = 1,
        overlap: float = 0.25,
        split: bool = True,
        segment: Optional[int] = None,
        jobs: int = 0,
        progress: bool = False,
        callback: Optional[Callable[[dict], None]] = None,
        callback_arg: Optional[dict] = None,
    ):
        """
        `class Separator`
        =================

        Parameters
        ----------
        model: Pretrained model name or signature. Default is htdemucs.
        repo: Folder containing all pre-trained models for use.
        segment: Length (in seconds) of each segment (only available if `split` is `True`). If \
            not specified, will use the command line option.
        shifts: If > 0, will shift in time `wav` by a random amount between 0 and 0.5 sec and \
            apply the opposite shift to the output. This is repeated `shifts` time and all \
            predictions are averaged. This effectively makes the model time equivariant and \
            improves SDR by up to 0.2 points. If not specified, will use the command line option.
        split: If True, the input will be broken down into small chunks (length set by `segment`) \
            and predictions will be performed individually on each and concatenated. Useful for \
            model with large memory footprint like Tasnet. If not specified, will use the command \
            line option.
        overlap: The overlap between the splits. If not specified, will use the command line \
            option.
        device (torch.device, str, or None): If provided, device on which to execute the \
            computation, otherwise `wav.device` is assumed. When `device` is different from \
            `wav.device`, only local computations will be on `device`, while the entire tracks \
            will be stored on `wav.device`. If not specified, will use the command line option.
        jobs: Number of jobs. This can increase memory usage but will be much faster when \
            multiple cores are available. If not specified, will use the command line option.
        callback: A function will be called when the separation of a chunk starts or finished. \
            The argument passed to the function will be a dict. For more information, please see \
            the Callback section.
        callback_arg: A dict containing private parameters to be passed to callback function. For \
            more information, please see the Callback section.
        progress: If true, show a progress bar.

        Callback
        --------
        The function will be called with only one positional parameter whose type is `dict`. The
        `callback_arg` will be combined with information of current separation progress. The
        progress information will override the values in `callback_arg` if same key has been used.
        To abort the separation, raise `KeyboardInterrupt`.

        Progress information contains several keys (These keys will always exist):
        - `model_idx_in_bag`: The index of the submodel in `BagOfModels`. Starts from 0.
        - `shift_idx`: The index of shifts. Starts from 0.
        - `segment_offset`: The offset of current segment. If the number is 441000, it doesn't
            mean that it is at the 441000 second of the audio, but the "frame" of the tensor.
        - `state`: Could be `"start"` or `"end"`.
        - `audio_length`: Length of the audio (in "frame" of the tensor).
        - `models`: Count of submodels in the model.
        """
        self._name = model
        self._repo = repo
        self._load_model()
        self.update_parameter(
            device=device,
            shifts=shifts,
            overlap=overlap,
            split=split,
            segment=segment,
            jobs=jobs,
            progress=progress,
            callback=callback,
            callback_arg=callback_arg,
        )

    def update_parameter(
        self,
        device: Union[str, _NotProvided] = NotProvided,
        shifts: Union[int, _NotProvided] = NotProvided,
        overlap: Union[float, _NotProvided] = NotProvided,
        split: Union[bool, _NotProvided] = NotProvided,
        segment: Optional[Union[int, _NotProvided]] = NotProvided,
        jobs: Union[int, _NotProvided] = NotProvided,
        progress: Union[bool, _NotProvided] = NotProvided,
        callback: Optional[Union[Callable[[dict], None], _NotProvided]] = NotProvided,
        callback_arg: Optional[Union[dict, _NotProvided]] = NotProvided,
    ):
        """
        Update the parameters of separation.

        Parameters
        ----------
        segment: Length (in seconds) of each segment (only available if `split` is `True`). If \
            not specified, will use the command line option.
        shifts: If > 0, will shift in time `wav` by a random amount between 0 and 0.5 sec and \
            apply the opposite shift to the output. This is repeated `shifts` time and all \
            predictions are averaged. This effectively makes the model time equivariant and \
            improves SDR by up to 0.2 points. If not specified, will use the command line option.
        split: If True, the input will be broken down into small chunks (length set by `segment`) \
            and predictions will be performed individually on each and concatenated. Useful for \
            model with large memory footprint like Tasnet. If not specified, will use the command \
            line option.
        overlap: The overlap between the splits. If not specified, will use the command line \
            option.
        device (torch.device, str, or None): If provided, device on which to execute the \
            computation, otherwise `wav.device` is assumed. When `device` is different from \
            `wav.device`, only local computations will be on `device`, while the entire tracks \
            will be stored on `wav.device`. If not specified, will use the command line option.
        jobs: Number of jobs. This can increase memory usage but will be much faster when \
            multiple cores are available. If not specified, will use the command line option.
        callback: A function will be called when the separation of a chunk starts or finished. \
            The argument passed to the function will be a dict. For more information, please see \
            the Callback section.
        callback_arg: A dict containing private parameters to be passed to callback function. For \
            more information, please see the Callback section.
        progress: If true, show a progress bar.

        Callback
        --------
        The function will be called with only one positional parameter whose type is `dict`. The
        `callback_arg` will be combined with information of current separation progress. The
        progress information will override the values in `callback_arg` if same key has been used.
        To abort the separation, raise `KeyboardInterrupt`.

        Progress information contains several keys (These keys will always exist):
        - `model_idx_in_bag`: The index of the submodel in `BagOfModels`. Starts from 0.
        - `shift_idx`: The index of shifts. Starts from 0.
        - `segment_offset`: The offset of current segment. If the number is 441000, it doesn't
            mean that it is at the 441000 second of the audio, but the "frame" of the tensor.
        - `state`: Could be `"start"` or `"end"`.
        - `audio_length`: Length of the audio (in "frame" of the tensor).
        - `models`: Count of submodels in the model.
        """
        if not isinstance(device, _NotProvided):
            self._device = device
        if not isinstance(shifts, _NotProvided):
            self._shifts = shifts
        if not isinstance(overlap, _NotProvided):
            self._overlap = overlap
        if not isinstance(split, _NotProvided):
            self._split = split
        if not isinstance(segment, _NotProvided):
            self._segment = segment
        if not isinstance(jobs, _NotProvided):
            self._jobs = jobs
        if not isinstance(progress, _NotProvided):
            self._progress = progress
        if not isinstance(callback, _NotProvided):
            self._callback = callback
        if not isinstance(callback_arg, _NotProvided):
            self._callback_arg = callback_arg

    def _load_model(self):
        self._model = get_model(name=self._name, repo=self._repo)
        if self._model is None:
            raise LoadModelError("Failed to load model")
        self._audio_channels = self._model.audio_channels
        self._samplerate = self._model.samplerate

    def _load_audio(self, track: Path):
        errors = {}
        wav = None

        try:
            wav = AudioFile(track).read(
                streams=0, samplerate=self._samplerate, channels=self._audio_channels
            )
        except FileNotFoundError:
            errors["ffmpeg"] = "FFmpeg is not installed."
        except subprocess.CalledProcessError:
            errors["ffmpeg"] = "FFmpeg could not read the file."

        if wav is None:
            try:
                wav, sr = ta.load(str(track))
            except RuntimeError as err:
                errors["torchaudio"] = err.args[0]
            else:
                wav = convert_audio(wav, sr, self._samplerate, self._audio_channels)

        if wav is None:
            raise LoadAudioError(
                "\n".join(
                    "When trying to load using {}, got the following error: {}".format(
                        backend, error
                    )
                    for backend, error in errors.items()
                )
            )
        return wav

    def separate_tensor(
        self, wav: th.Tensor, sr: Optional[int] = None
    ) -> Tuple[th.Tensor, Dict[str, th.Tensor]]:
        """
        Separate a loaded tensor.

        Parameters
        ----------
        wav: Waveform of the audio. Should have 2 dimensions, the first is each audio channel, \
            while the second is the waveform of each channel. Type should be float32. \
            e.g. `tuple(wav.shape) == (2, 884000)` means the audio has 2 channels.
        sr: Sample rate of the original audio, the wave will be resampled if it doesn't match the \
            model.

        Returns
        -------
        A tuple, whose first element is the original wave and second element is a dict, whose keys
        are the name of stems and values are separated waves. The original wave will have already
        been resampled.

        Notes
        -----
        Use this function with cautiousness. This function does not provide data verifying.
        """
        if sr is not None and sr != self.samplerate:
            wav = convert_audio(wav, sr, self._samplerate, self._audio_channels)
        ref = wav.mean(0)
        wav -= ref.mean()
        wav /= ref.std() + 1e-8
        out = apply_model(
            self._model,
            wav[None],
            segment=self._segment,
            shifts=self._shifts,
            split=self._split,
            overlap=self._overlap,
            device=self._device,
            num_workers=self._jobs,
            callback=self._callback,
            callback_arg=_replace_dict(
                self._callback_arg, ("audio_length", wav.shape[1])
            ),
            progress=self._progress,
        )
        if out is None:
            raise KeyboardInterrupt
        out *= ref.std() + 1e-8
        out += ref.mean()
        wav *= ref.std() + 1e-8
        wav += ref.mean()
        return (wav, dict(zip(self._model.sources, out[0])))

    def separate_audio_file(self, file: Path):
        """
        Separate an audio file. The method will automatically read the file.

        Parameters
        ----------
        wav: Path of the file to be separated.

        Returns
        -------
        A tuple, whose first element is the original wave and second element is a dict, whose keys
        are the name of stems and values are separated waves. The original wave will have already
        been resampled.
        """
        return self.separate_tensor(self._load_audio(file), self.samplerate)

    @property
    def samplerate(self):
        return self._samplerate

    @property
    def audio_channels(self):
        return self._audio_channels

    @property
    def model(self):
        return self._model


def list_models(repo: Optional[Path] = None) -> Dict[str, Dict[str, Union[str, Path]]]:
    """List the available models from a repo.

    Parameters
    ----------
    repo: The repo whose models are to be listed. None for the remote default repo.

    Returns
    -------
    A dict with two keys ("single" for single models and "bag" for bag of models). The values are
    dicts mapping model names to their paths or URLs.

    Raises
    ------
    ModelLoadingError: If the repo path is invalid.
    """
    model_repo, bag_repo = build_repos(repo)
    return {"single": model_repo.list_model(), "bag": bag_repo.list_model()}


def save_separated_stems(
    sources: Dict[str, "th.Tensor"],
    output_dir: Path,
    track_name: str,
    samplerate: int,
    ext: str = "wav",
    two_stems: Optional[str] = None,
    two_stems_method: str = "add",
    origin: Optional["th.Tensor"] = None,
    bitrate: int = 320,
    clip: str = "rescale",
    as_float: bool = False,
    bits_per_sample: int = 16,
    preset: int = 2,
    filename_template: str = "{track}/{stem}.{ext}",
) -> list:
    """Save separated stems to disk, handling two-stems mode.

    This is the shared stem-saving logic used by the CLI, the API __main__,
    and the TUI worker.

    Args:
        sources: Dict mapping stem names to audio tensors.
        output_dir: Root output directory (model subdirectory).
        track_name: Name of the track (without extension).
        samplerate: Audio sample rate.
        ext: Output file extension (wav, flac, mp3).
        two_stems: If set, only save this stem + the complement.
        two_stems_method: How to produce the complement: "add" (sum others),
            "minus" (origin - stem), or "none" (skip complement).
        origin: Original mix tensor, required when two_stems_method is "minus".
        bitrate: MP3 bitrate.
        clip: Clipping strategy.
        as_float: Save as float32.
        bits_per_sample: Bit depth for wav/flac.
        preset: MP3 encoder preset.
        filename_template: Output filename template with {track}, {stem}, {ext}.

    Returns:
        List of saved file paths as strings.
    """
    saved = []
    save_kwargs = {
        "samplerate": samplerate,
        "bitrate": bitrate,
        "clip": clip,
        "as_float": as_float,
        "bits_per_sample": bits_per_sample,
        "preset": preset,
    }

    if two_stems:
        if two_stems not in sources:
            raise ValueError(
                f"Stem '{two_stems}' not in model sources: {list(sources.keys())}"
            )
        # Save the selected stem
        stem_path = output_dir / filename_template.format(
            track=track_name, stem=two_stems, ext=ext
        )
        stem_path.parent.mkdir(parents=True, exist_ok=True)
        save_audio(sources[two_stems], str(stem_path), **save_kwargs)
        saved.append(str(stem_path))

        if two_stems_method == "minus":
            if origin is None:
                raise ValueError(
                    "origin tensor is required for two_stems_method='minus'"
                )
            complement = origin - sources[two_stems]
            comp_path = output_dir / filename_template.format(
                track=track_name, stem=f"minus_{two_stems}", ext=ext
            )
            comp_path.parent.mkdir(parents=True, exist_ok=True)
            save_audio(complement, str(comp_path), **save_kwargs)
            saved.append(str(comp_path))
        elif two_stems_method == "add":
            complement = th.zeros_like(next(iter(sources.values())))
            for name, source in sources.items():
                if name != two_stems:
                    complement += source
            comp_path = output_dir / filename_template.format(
                track=track_name, stem=f"no_{two_stems}", ext=ext
            )
            comp_path.parent.mkdir(parents=True, exist_ok=True)
            save_audio(complement, str(comp_path), **save_kwargs)
            saved.append(str(comp_path))
        # two_stems_method == "none" → skip complement
    else:
        for stem_name, source in sources.items():
            stem_path = output_dir / filename_template.format(
                track=track_name, stem=stem_name, ext=ext
            )
            stem_path.parent.mkdir(parents=True, exist_ok=True)
            save_audio(source, str(stem_path), **save_kwargs)
            saved.append(str(stem_path))

    return saved


if __name__ == "__main__":
    # Test API functions

    from .separate import get_parser

    args = get_parser().parse_args()
    separator = Separator(
        model=args.name,
        repo=args.repo,
        device=args.device,
        shifts=args.shifts,
        overlap=args.overlap,
        split=args.split,
        segment=args.segment,
        jobs=args.jobs,
        callback=print,
    )
    out = args.out / args.name
    out.mkdir(parents=True, exist_ok=True)
    ext = "mp3" if args.mp3 else ("flac" if args.flac else "wav")
    for file in args.tracks:
        track_name = Path(file).name.rsplit(".", 1)[0]
        separated = separator.separate_audio_file(file)[1]
        save_separated_stems(
            sources=separated,
            output_dir=out,
            track_name=track_name,
            samplerate=separator.samplerate,
            ext=ext,
            bitrate=args.mp3_bitrate,
            clip=args.clip_mode,
            as_float=args.float32,
            bits_per_sample=24 if args.int24 else 16,
            preset=args.mp3_preset,
            filename_template=args.filename,
        )
