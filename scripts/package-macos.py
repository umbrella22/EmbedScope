#!/usr/bin/env python3
"""Build a development or release Inspector.app with an ad hoc signature."""

import argparse
import json
from pathlib import Path
import subprocess

from release_support.binaries import bundle_macos, host_target
from release_support.common import ROOT, release_version

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--release", action="store_true")
parser.add_argument("--target", default=host_target())
parser.add_argument("--output", type=Path, default=ROOT / "target/EmbedScope Inspector.app")
args = parser.parse_args()
command = ["cargo", "build", "-p", "escope-desktop", "--locked", "--target", args.target]
if args.release:
    command.append("--release")
subprocess.run(command, cwd=ROOT, check=True)
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version=1"], cwd=ROOT, text=True))
binary = Path(metadata["target_directory"]) / args.target / ("release" if args.release else "debug") / "escope-desktop"
bundle_macos(binary, args.output.resolve(), release_version())
print(args.output.resolve())
