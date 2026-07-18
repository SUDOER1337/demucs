# Copyright (c) Meta Platforms, Inc. and affiliates.
# All rights reserved.
#
# This source code is licensed under the license found in the
# LICENSE file in the root directory of this source tree.
"""Loading pretrained models."""

import logging
from pathlib import Path
import typing as tp

from .hdemucs import HDemucs
from .repo import (
    RemoteRepo,
    LocalRepo,
    ModelOnlyRepo,
    BagOnlyRepo,
    AnyModelRepo,
    ModelLoadingError,
)
from .states import _check_diffq

logger = logging.getLogger(__name__)
ROOT_URL = "https://dl.fbaipublicfiles.com/demucs/"
REMOTE_ROOT = Path(__file__).parent / "remote"

SOURCES = ["drums", "bass", "other", "vocals"]
DEFAULT_MODEL = "htdemucs"


def demucs_unittest():
    model = HDemucs(channels=4, sources=SOURCES)
    return model


def add_model_flags(parser):
    group = parser.add_mutually_exclusive_group(required=False)
    group.add_argument("-s", "--sig", help="Locally trained XP signature.")
    group.add_argument(
        "-n",
        "--name",
        default="htdemucs",
        help="Pretrained model name or signature. Default is htdemucs.",
    )
    parser.add_argument(
        "--repo",
        type=Path,
        help="Folder containing all pre-trained models for use with -n.",
    )


def _parse_remote_files(remote_file_list) -> tp.Dict[str, str]:
    root: str = ""
    models: tp.Dict[str, str] = {}
    for line in remote_file_list.read_text().split("\n"):
        line = line.strip()
        if line.startswith("#"):
            continue
        elif len(line) == 0:
            continue
        elif line.startswith("root:"):
            root = line.split(":", 1)[1].strip()
        else:
            sig = line.split("-", 1)[0]
            assert sig not in models
            models[sig] = ROOT_URL + root + line
    return models


def build_repos(repo: tp.Optional[Path] = None) -> tp.Tuple[ModelOnlyRepo, BagOnlyRepo]:
    """Construct the model and bag repos for a given local repo path.

    Args:
        repo: Local repo directory, or None for the remote default repo.

    Returns:
        A tuple of (model_repo, bag_repo).

    Raises:
        ModelLoadingError: If repo is specified but does not exist or is not a directory.
    """
    if repo is None:
        models = _parse_remote_files(REMOTE_ROOT / "files.txt")
        model_repo: ModelOnlyRepo = RemoteRepo(models)
        bag_repo = BagOnlyRepo(REMOTE_ROOT, model_repo)
    else:
        if not repo.is_dir():
            raise ModelLoadingError(f"{repo} must exist and be a directory.")
        model_repo = LocalRepo(repo)
        bag_repo = BagOnlyRepo(repo, model_repo)
    return model_repo, bag_repo


def get_model(name: str, repo: tp.Optional[Path] = None):
    """Load a pretrained model by name or signature.

    Args:
        name: Model name (e.g. 'htdemucs') or signature, or 'demucs_unittest'.
        repo: Local repo directory, or None for the remote default repo.

    Returns:
        The loaded model in eval mode.

    Raises:
        ModelLoadingError: If the repo path is invalid.
        ImportError: If diffq is required but not installed.
    """
    if name == "demucs_unittest":
        return demucs_unittest()
    model_repo, bag_repo = build_repos(repo)
    any_repo = AnyModelRepo(model_repo, bag_repo)
    try:
        model = any_repo.get_model(name)
    except ImportError as exc:
        if "diffq" in exc.args[0]:
            _check_diffq()
        raise

    model.eval()
    return model


def get_model_from_args(args):
    """
    Load local model package or pre-trained model.
    """
    if args.name is None:
        args.name = DEFAULT_MODEL
        logger.warning(
            "Important: the default model was recently changed to `htdemucs`, "
            "the latest Hybrid Transformer Demucs model. In some cases, this model can "
            "actually perform worse than previous models. To get back the old default model "
            "use `-n mdx_extra_q`."
        )
    return get_model(name=args.name, repo=args.repo)
