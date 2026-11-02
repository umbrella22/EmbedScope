#!/usr/bin/env python3
"""Install the two versioned EmbedScope skills without replacing local work."""
import argparse
import os
from pathlib import Path
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--destination", type=Path, default=Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex"))) / "skills")
args = parser.parse_args()
source = Path(__file__).resolve().parents[1] / "skills"
names = ("embedscope-debug", "embedscope-integrate")

def inventory(directory):
    return {str(p.relative_to(directory)): p.read_bytes() for p in directory.rglob("*") if p.is_file()}

# Check both destinations before installing either.
for name in names:
    target = args.destination / name
    if target.exists() and (not target.is_dir() or inventory(target) != inventory(source / name)):
        parser.error(f"existing skill differs; preserved: {target}")
args.destination.mkdir(parents=True, exist_ok=True)
for name in names:
    target = args.destination / name
    if not target.exists():
        shutil.copytree(source / name, target)
    print(target)
