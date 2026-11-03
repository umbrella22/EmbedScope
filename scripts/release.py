#!/usr/bin/env python3
"""Prepare, verify and publish EmbedScope release artifacts."""

import argparse
from pathlib import Path
import subprocess
import urllib.error

from release_support.binaries import binaries, checksums
from release_support.common import Context, ROOT, release_version, require_tag
from release_support.c_sdk import c_sdk
from release_support.packages import consumer, package
from release_support.registry import publish, verify_registry


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("version", "identity", "verify", "binaries", "checksums", "publish", "registry"))
    parser.add_argument("version", nargs="?")
    parser.add_argument("--artifacts", type=Path, default=ROOT / "target/dist")
    parser.add_argument("--target", help="Native Rust target triple for binary packaging")
    parser.add_argument("--allow-dirty", action="store_true", help="Permit local edits for verification/binaries only")
    parser.add_argument("--local", action="store_true", help="Publish a clean local commit without requiring a pushed version tag")
    args = parser.parse_args()
    if args.allow_dirty and args.command not in ("verify", "binaries"):
        parser.error("--allow-dirty applies only to verify and binaries")
    if args.local and args.command != "publish":
        parser.error("--local applies only to publish")
    version = release_version(args.version)
    if args.command == "version":
        print(version)
        return
    if args.command == "identity":
        print(f"v{version}: {require_tag(version)}")
        return
    if args.command == "checksums":
        print(checksums(args.artifacts.resolve()))
        return
    context = Context(version, args.artifacts, args.command)
    if context.receipt["dirty"] and not args.allow_dirty:
        raise RuntimeError("Working tree is dirty; commit first or use --allow-dirty for local verification")
    if args.command == "verify":
        archives = package(context, args.allow_dirty)
        consumer(context, archives)
        c_sdk(context, archives)
        checksums(context.out)
    elif args.command == "binaries":
        binaries(context, args.target)
        checksums(context.out)
    elif args.command == "publish":
        publish(context, args.local)
    elif args.command == "registry":
        verify_registry(context)
    print(f"EmbedScope {version}: {args.command} passed", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, urllib.error.URLError) as error:
        raise SystemExit(str(error)) from None
