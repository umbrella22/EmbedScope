#!/usr/bin/env python3
"""Fingerprint reviewable source across EmbedScope and its documentation site."""
import argparse
import hashlib
import json
import os
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--website", type=Path, required=True)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
source = Path(__file__).resolve().parents[1]
website = args.website.resolve()
if not (website / "rspress.config.ts").is_file():
    parser.error("--website must contain rspress.config.ts")
excluded = {".git", "target", "node_modules", "doc_build", ".rspress", "__pycache__", ".DS_Store", "artifacts"}
files = []
for namespace, root in [("EmbedScope", source), ("EmbedScope_website", website)]:
    for directory, children, names in os.walk(root):
        children[:] = sorted(name for name in children if name not in excluded)
        for name in sorted(names):
            if name in excluded:
                continue
            entry = Path(directory) / name
            relative = entry.relative_to(root)
            if namespace == "EmbedScope" and relative.as_posix() == "docs/execution-ledger.md":
                continue
            files.append({"path": namespace + "/" + relative.as_posix(), "sha256": hashlib.sha256(entry.read_bytes()).hexdigest()})
files.sort(key=lambda item: item["path"])
digest = hashlib.sha256()
for item in files:
    digest.update((item["path"] + "\0" + item["sha256"] + "\n").encode())
result = {
    "schema": 1,
    "algorithm": "SHA-256 of sorted UTF-8 path + NUL + file SHA-256 + LF",
    "source_sha256": digest.hexdigest(),
    "file_count": len(files),
    "excluded": sorted(excluded) + ["EmbedScope/docs/execution-ledger.md"],
    "files": files,
}
content = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
if args.output:
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(content)
    print(json.dumps({"source_sha256": result["source_sha256"], "file_count": len(files), "output": str(args.output)}))
else:
    print(content, end="")
