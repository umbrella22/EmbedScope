"""Preserve packaged dependency license and notice files in binary archives."""

import json
from pathlib import Path
import re
import shutil
import subprocess

from .common import PACKAGES, ROOT

NOTICE = re.compile(r"(?:^|[-_.])(licen[sc]e|copying|copyright|notice|ofl|ufl)(?:[._-]|$)", re.I)


def dependency_notices(stage, target, env):
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version=1",
        "--filter-platform", target], cwd=ROOT, env=env, stderr=subprocess.DEVNULL))
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending = [key for key, package in packages.items() if package["name"] in ("escope-cli", "escope-desktop")]
    used = set()
    while pending:
        key = pending.pop()
        if key in used:
            continue
        used.add(key)
        pending.extend(dependency["pkg"] for dependency in nodes[key]["deps"]
            if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]))
    notices = []
    for key in sorted(used):
        package = packages[key]
        if package["name"] in (*PACKAGES, "escope-desktop"):
            continue
        root = Path(package["manifest_path"]).parent
        name = f"{package['name']}-{package['version']}"
        paths = {path for path in root.rglob("*") if path.is_file()
            and (NOTICE.search(path.name) or ("fonts" in path.relative_to(root).parts and path.suffix == ".txt"))}
        if package.get("license_file"):
            paths.add(root / package["license_file"])
        # Preserve the registry package's attribution and SPDX declaration too;
        # some older crates ship no separate license text.
        if not any(path.parent == root for path in paths):
            if (root / "README.md").is_file():
                paths.add(root / "README.md")
        paths.add(root / "Cargo.toml")
        files = []
        for path in sorted(paths):
            relative = path.relative_to(root)
            destination = stage / "licenses" / name / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)
            files.append(relative.as_posix())
        notices.append({"name": package["name"], "version": package["version"],
            "license": package["license"], "repository": package["repository"], "files": files})
    (stage / "dependency-licenses.json").write_text(json.dumps(notices, indent=2) + "\n")
