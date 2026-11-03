#!/usr/bin/env python3
"""Assemble verified workflow artifacts and publish an immutable GitHub Release."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from release_support.binaries import TARGETS, checksums, distribution_names
from release_support.common import PACKAGES, ROOT, release_version, require_tag, sha256

REPOSITORY = "umbrella22/EmbedScope"


def gh(*args, check=True):
    result = subprocess.run(["gh", *args, "--repo", REPOSITORY], cwd=ROOT, capture_output=True, text=True, check=False)
    if check and result.returncode:
        raise RuntimeError(result.stderr.strip() or "GitHub release command failed")
    return result


def collect(source, destination, version, commit):
    groups = [("release-source", "verify", [f"{name}-{version}.crate" for name in PACKAGES]
        + [f"embedscope-c-sdk-{version}.zip"])]
    groups.extend((f"release-{target}", "binaries", distribution_names(version, target))
        for target in sorted(TARGETS))
    destination.mkdir(parents=True, exist_ok=False)
    for group, name, filenames in groups:
        directory = source / group
        receipt = json.loads((directory / f"{name}.json").read_text())
        if receipt["version"] != version or receipt["commit"] != commit or receipt["dirty"]:
            raise RuntimeError(f"{group} source identity differs from the clean version tag")
        required = ({"cargo-package", "consumer-sdk", "consumer-cli-install", "consumer-cli-e2e",
            "c-sdk-host-build", "c-sdk-native-libs", "c-sdk-c-link", "c-sdk-c-harness", "c-sdk-no-std"}
            if name == "verify" else {"native-build", "shipped-cli-version", "shipped-cli-e2e",
                "shipped-inspector-version", "shipped-inspector-smoke"})
        if group.endswith("linux-gnu"):
            required.update({"appimage-deploy", "appimage-build", "appimage-extract"})
        if group.endswith("darwin"):
            required.add("shipped-inspector-signature")
        commands = {command["name"] for command in receipt["commands"] if command["exit"] == 0}
        if not required.issubset(commands) or any(command["exit"] for command in receipt["commands"]):
            raise RuntimeError(f"{group} validation is incomplete")
        if name == "binaries" and receipt.get("target") != group.removeprefix("release-"):
            raise RuntimeError(f"{group} target differs from its build receipt")
        for filename in filenames:
            path = directory / filename
            if sha256(path.read_bytes()) != receipt["artifacts"][filename]["sha256"]:
                raise RuntimeError(f"Artifact checksum differs: {filename}")
            shutil.copy2(path, destination / filename)
    checksums(destination)


def publish_assets(tag, stage, version):
    files = {path.name: sha256(path.read_bytes()) for path in stage.iterdir() if path.is_file()}
    result = gh("release", "view", tag, "--json", "tagName,isDraft,isPrerelease,assets", check=False)
    if result.returncode:
        if "release not found" not in result.stderr.lower() and "HTTP 404" not in result.stderr:
            raise RuntimeError(result.stderr)
        notes = ROOT / "notes" / f"{tag}.md"
        if not notes.is_file():
            raise RuntimeError(f"Release notes are missing: {notes.name}")
        gh("release", "create", tag, "--verify-tag", "--draft", "--title", f"EmbedScope {version}", "--notes-file", str(notes))
        result = gh("release", "view", tag, "--json", "tagName,isDraft,isPrerelease,assets")
    record = json.loads(result.stdout)
    if record["tagName"] != tag or record["isPrerelease"]:
        raise RuntimeError("Existing release metadata conflicts with this stable release")
    existing = {asset["name"] for asset in record["assets"]}
    if existing - files.keys():
        raise RuntimeError("Existing release contains unexpected assets")
    with tempfile.TemporaryDirectory(prefix="embedscope-public-assets-") as directory:
        for name in sorted(existing):
            gh("release", "download", tag, "--pattern", name, "--dir", directory)
            if sha256((Path(directory) / name).read_bytes()) != files[name]:
                raise RuntimeError(f"Existing asset conflicts with the verified build: {name}")
    missing = files.keys() - existing
    if missing:
        if not record["isDraft"]:
            raise RuntimeError("Published release is incomplete; refusing to change public assets")
        gh("release", "upload", tag, *(str(stage / name) for name in sorted(missing)))
    # Download all draft assets before making the release public. This also
    # verifies bytes uploaded during a resumed run, without replacing assets.
    with tempfile.TemporaryDirectory(prefix="embedscope-release-check-") as directory:
        gh("release", "download", tag, "--dir", directory)
        if {path.name: sha256(path.read_bytes()) for path in Path(directory).iterdir()} != files:
            raise RuntimeError("Downloaded release assets differ from the verified archives")
    if record["isDraft"]:
        gh("release", "edit", tag, "--draft=false", "--latest")
    final = json.loads(gh("release", "view", tag, "--json", "url,isDraft").stdout)
    if final["isDraft"]:
        raise RuntimeError("GitHub Release is still a draft")
    print(final["url"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("--collected", required=True, type=Path)
    args = parser.parse_args()
    version = release_version(args.tag)
    commit = require_tag(version)
    with tempfile.TemporaryDirectory(prefix="embedscope-release-") as directory:
        stage = Path(directory) / "assets"
        collect(args.collected.resolve(), stage, version, commit)
        publish_assets(args.tag, stage, version)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from None
