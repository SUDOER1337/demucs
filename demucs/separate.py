# Copyright (c) Meta Platforms, Inc. and affiliates.
# All rights reserved.
#
# This source code is licensed under the license found in the
# LICENSE file in the root directory of this source tree.

import argparse
import logging
import sys
from pathlib import Path

from dora.log import fatal
import torch as th

from .api import Separator, list_models, save_separated_stems

from .apply import BagOfModels
from .htdemucs import HTDemucs
from .pretrained import add_model_flags, ModelLoadingError

logger = logging.getLogger(__name__)


def get_parser():
    parser = argparse.ArgumentParser(
        "demucs.separate", description="Separate the sources for the given tracks"
    )
    parser.add_argument(
        "tracks", nargs="*", type=Path, default=None, help="Path to tracks"
    )
    add_model_flags(parser)
    parser.add_argument(
        "--list-models",
        action="store_true",
        help="List available models from current repo and exit",
    )
    parser.add_argument("-v", "--verbose", action="store_true")
    parser.add_argument(
        "-o",
        "--out",
        type=Path,
        default=Path("separated"),
        help="Folder where to put extracted tracks. A subfolder "
        "with the model name will be created.",
    )
    parser.add_argument(
        "--filename",
        default="{track}/{stem}.{ext}",
        help="Set the name of output file. \n"
        'Use "{track}", "{trackext}", "{stem}", "{ext}" to use '
        "variables of track name without extension, track extension, "
        "stem name and default output file extension. \n"
        'Default is "{track}/{stem}.{ext}".',
    )
    parser.add_argument(
        "-d",
        "--device",
        default="cuda" if th.cuda.is_available() else "cpu",
        help="Device to use, default is cuda if available else cpu",
    )
    parser.add_argument(
        "--shifts",
        default=1,
        type=int,
        help="Number of random shifts for equivariant stabilization."
        "Increase separation time but improves quality for Demucs. 10 was used "
        "in the original paper.",
    )
    parser.add_argument(
        "--overlap", default=0.25, type=float, help="Overlap between the splits."
    )
    split_group = parser.add_mutually_exclusive_group()
    split_group.add_argument(
        "--no-split",
        action="store_false",
        dest="split",
        default=True,
        help="Doesn't split audio in chunks. This can use large amounts of memory.",
    )
    split_group.add_argument(
        "--segment",
        type=int,
        help="Set split size of each chunk. "
        "This can help save memory of graphic card. ",
    )
    parser.add_argument(
        "--two-stems",
        dest="stem",
        metavar="STEM",
        help="Only separate audio into {STEM} and no_{STEM}. ",
    )
    parser.add_argument(
        "--other-method",
        dest="other_method",
        choices=["none", "add", "minus"],
        default="add",
        help='Decide how to get "no_{STEM}". "none" will not save '
        '"no_{STEM}". "add" will add all the other stems. "minus" will use the '
        "original track minus the selected stem.",
    )
    depth_group = parser.add_mutually_exclusive_group()
    depth_group.add_argument(
        "--int24", action="store_true", help="Save wav output as 24 bits wav."
    )
    depth_group.add_argument(
        "--float32", action="store_true", help="Save wav output as float32 (2x bigger)."
    )
    parser.add_argument(
        "--clip-mode",
        default="rescale",
        choices=["rescale", "clamp", "none"],
        help="Strategy for avoiding clipping: rescaling entire signal "
        "if necessary  (rescale) or hard clipping (clamp).",
    )
    format_group = parser.add_mutually_exclusive_group()
    format_group.add_argument(
        "--flac", action="store_true", help="Convert the output wavs to flac."
    )
    format_group.add_argument(
        "--mp3", action="store_true", help="Convert the output wavs to mp3."
    )
    parser.add_argument(
        "--mp3-bitrate", default=320, type=int, help="Bitrate of converted mp3."
    )
    parser.add_argument(
        "--mp3-preset",
        choices=range(2, 8),
        type=int,
        default=2,
        help="Encoder preset of MP3, 2 for highest quality, 7 for "
        "fastest speed. Default is 2",
    )
    parser.add_argument(
        "-j",
        "--jobs",
        default=0,
        type=int,
        help="Number of jobs. This can increase memory usage but will "
        "be much faster when multiple cores are available.",
    )

    return parser


def main(opts=None):
    parser = get_parser()
    args = parser.parse_args(opts)
    if args.list_models:
        try:
            models = list_models(args.repo)
        except ModelLoadingError as error:
            fatal(str(error))
        print("Bag of models:", end="\n    ")
        print("\n    ".join(models["bag"]))
        print("Single models:", end="\n    ")
        print("\n    ".join(models["single"]))
        sys.exit(0)
    args.tracks = args.tracks or []
    if not args.tracks:
        logger.error("the following arguments are required: tracks")
        sys.exit(1)

    try:
        separator = Separator(
            model=args.name,
            repo=args.repo,
            device=args.device,
            shifts=args.shifts,
            split=args.split,
            overlap=args.overlap,
            progress=True,
            jobs=args.jobs,
            segment=args.segment,
        )
    except ModelLoadingError as error:
        fatal(error.args[0])

    max_allowed_segment = float("inf")
    if isinstance(separator.model, HTDemucs):
        max_allowed_segment = float(separator.model.segment)
    elif isinstance(separator.model, BagOfModels):
        max_allowed_segment = separator.model.max_allowed_segment
    if args.segment is not None and args.segment > max_allowed_segment:
        fatal(
            "Cannot use a Transformer model with a longer segment "
            f"than it was trained for. Maximum segment is: {max_allowed_segment}"
        )

    if isinstance(separator.model, BagOfModels):
        logger.info(
            "Selected model is a bag of %d models. "
            "You will see that many progress bars per track.",
            len(separator.model.models),
        )

    if args.stem is not None and args.stem not in separator.model.sources:
        fatal(
            'error: stem "{stem}" is not in selected model. '
            "STEM must be one of {sources}.".format(
                stem=args.stem, sources=", ".join(separator.model.sources)
            )
        )
    out = args.out / args.name
    out.mkdir(parents=True, exist_ok=True)
    logger.info("Separated tracks will be stored in %s", out.resolve())
    for track in args.tracks:
        if not track.exists():
            logger.error(
                "File %s does not exist. If the path contains spaces, "
                'please try again after surrounding the entire path with quotes "".',
                track,
            )
            continue
        logger.info("Separating track %s", track)

        origin, res = separator.separate_audio_file(track)

        ext = "mp3" if args.mp3 else ("flac" if args.flac else "wav")
        two_stems_method = args.other_method if args.stem else None
        save_separated_stems(
            sources=res,
            output_dir=out,
            track_name=track.name.rsplit(".", 1)[0],
            samplerate=separator.samplerate,
            ext=ext,
            two_stems=args.stem,
            two_stems_method=two_stems_method or "add",
            origin=origin if two_stems_method == "minus" else None,
            bitrate=args.mp3_bitrate,
            preset=args.mp3_preset,
            clip=args.clip_mode,
            as_float=args.float32,
            bits_per_sample=24 if args.int24 else 16,
            filename_template=args.filename,
        )


if __name__ == "__main__":
    main()
