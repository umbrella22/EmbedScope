"""Native CLI/Inspector distribution archives and macOS application bundles."""

import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile

from .common import ROOT, sha256
from .licenses import dependency_notices

TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"}


def host_target():
    output = subprocess.check_output(["rustc", "-vV"], text=True)
    return next(line.removeprefix("host: ") for line in output.splitlines() if line.startswith("host: "))


def bundle_macos(binary, destination, version):
    if sys.platform != "darwin":
        raise RuntimeError("macOS bundles must be sealed on macOS")
    if destination.exists():
        shutil.rmtree(destination)
    contents = destination / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    shutil.copy2(binary, contents / "MacOS/escope-desktop")
    shutil.copy2(ROOT / "crates/escope-desktop/assets/brand/app-icon.icns", contents / "Resources/EmbedScope.icns")
    info = {"CFBundleExecutable": "escope-desktop", "CFBundleIdentifier": "dev.embedscope.inspector",
        "CFBundleName": "EmbedScope Inspector", "CFBundleIconFile": "EmbedScope.icns",
        "CFBundlePackageType": "APPL", "CFBundleVersion": version,
        "CFBundleShortVersionString": version, "NSHighResolutionCapable": True}
    (contents / "Info.plist").write_bytes(plistlib.dumps(info))
    subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(destination)], check=True)
    subprocess.run(["codesign", "--verify", "--strict", str(destination)], check=True)


def archive_directory(directory, destination):
    if destination.name.endswith(".zip"):
        with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(directory.rglob("*")):
                if path.is_file():
                    archive.write(path, path.relative_to(directory.parent).as_posix())
    else:
        with tarfile.open(destination, "w:gz") as archive:
            archive.add(directory, arcname=directory.name)


def binaries(context, target=None):
    target = target or host_target()
    if target not in TARGETS or target != host_target():
        raise RuntimeError("Build and validate release binaries on a matching supported native runner")
    context.run("native-build", ["cargo", "build", "--release", "--locked", "--target", target,
        "-p", "escope-cli", "-p", "escope-desktop"])
    extension = ".exe" if target.endswith("msvc") else ""
    build = context.target / target / "release"
    with tempfile.TemporaryDirectory(prefix="embedscope-binary-") as temporary:
        temporary = Path(temporary)
        for product, executable in (("escope", "escope"), ("embedscope-inspector", "escope-desktop")):
            name = f"{product}-{context.version}-{target}"
            stage = temporary / name
            stage.mkdir()
            binary = build / (executable + extension)
            if product == "embedscope-inspector" and target.endswith("darwin"):
                bundle_macos(binary, stage / "EmbedScope Inspector.app", context.version)
            else:
                shutil.copy2(binary, stage / binary.name)
            shutil.copy2(ROOT / "LICENSE", stage / "LICENSE")
            (stage / "README.txt").write_text(
                f"EmbedScope {context.version}\nTarget: {target}\nSource: {context.receipt['commit']}\n\n"
                "Documentation: https://umbrella22.github.io/EmbedScope_website/\n"
                "CLI: escope --help; escope simulate\n"
                "Inspector: launch the application, then connect to a TCP or serial device.\n"
                "Linux Inspector needs a desktop session, OpenGL, libudev and xkbcommon.\n"
                "Chinese text uses an installed system CJK font (Noto Sans CJK on Linux).\n"
                "macOS builds use ad hoc signatures; they are not notarized by Apple.\n"
                "EmbedScope is MIT licensed. Dependency licenses are listed in THIRD_PARTY.md.\n")
            shutil.copy2(ROOT / "THIRD_PARTY.md", stage / "THIRD_PARTY.md")
            dependency_notices(stage, target, context.env)
            (stage / "build.json").write_text(json.dumps({"version": context.version, "target": target,
                "source_commit": context.receipt["commit"], "source_dirty": context.receipt["dirty"],
                "build_binary_sha256": sha256(binary.read_bytes())}, indent=2) + "\n")
            destination = context.out / (name + (".zip" if extension else ".tar.gz"))
            archive_directory(stage, destination)
            context.record(destination)
            if product == "escope":
                # Verify the executable after a distribution round trip.
                extracted = temporary / "unpacked"
                if extension:
                    with zipfile.ZipFile(destination) as archive:
                        archive.extractall(extracted)
                else:
                    with tarfile.open(destination) as archive:
                        archive.extractall(extracted, filter="data")
                shipped = extracted / name / ("escope" + extension)
                context.run("shipped-cli-version", [shipped, "--version"])
                context.run("shipped-cli-e2e", [sys.executable, ROOT / "scripts/check-e2e.py", "--binary", shipped,
                    "--output", context.out / "binary-e2e"])
    context.receipt["target"] = target
    context.save()


def checksums(directory):
    paths = sorted(path for path in directory.iterdir() if path.name.endswith((".crate", ".tar.gz", ".zip")))
    if not paths:
        raise RuntimeError("No release archives found")
    destination = directory / "SHA256SUMS"
    destination.write_text("".join(f"{sha256(path.read_bytes())}  {path.name}\n" for path in paths))
    return destination
