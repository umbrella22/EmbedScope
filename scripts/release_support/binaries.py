"""Native CLI/Inspector distribution archives and macOS application bundles."""

import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

from .common import ROOT, sha256
from .licenses import dependency_notices
from .appimage import appimage

TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"}
WINDOWS_SMOKE_MESA = {
    "version": "26.2.6",
    "url": "https://github.com/pal1000/mesa-dist-win/releases/download/26.2.6/mesa3d-26.2.6-release-msvc.7z",
    "sha256": "b2e00615feaba45942343c265cc8c5f576142ca682c1ac6661922d4b72492877",
    "dlls": ("opengl32.dll", "libgallium_wgl.dll"),
}


def distribution_names(version, target):
    return [f"escope-{version}-{target}.zip", f"embedscope-inspector-{version}-{target}" +
        (".AppImage" if target.endswith("linux-gnu") else ".zip")]


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
    if destination.suffix != ".zip":
        raise RuntimeError("Distribution directory archives must use ZIP")
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        for path in sorted(directory.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(directory.parent).as_posix())


def extract_zip(source, destination):
    with zipfile.ZipFile(source) as archive:
        for member in archive.infolist():
            path = destination / member.filename
            if (not path.resolve().is_relative_to(destination.resolve()) or
                    "\\" in member.filename or ":" in member.filename):
                raise RuntimeError("Unsafe ZIP distribution member")
            # Python's ZIP extractor does not restore Unix executable bits.
            archive.extract(member, destination)
            if not member.is_dir() and member.create_system == 3:
                path.chmod((member.external_attr >> 16) & 0o777)


def prepare_windows_smoke_mesa(context, shipped):
    """Provide software GL only beside the already-unpacked CI smoke executable."""
    if sys.platform != "win32":
        raise RuntimeError("Windows smoke Mesa must be prepared on Windows")
    cache = context.target / "windows-smoke-mesa"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / f"mesa3d-{WINDOWS_SMOKE_MESA['version']}-release-msvc.7z"
    if not archive.exists():
        request = urllib.request.Request(WINDOWS_SMOKE_MESA["url"], headers={"User-Agent": "EmbedScope release"})
        with urllib.request.urlopen(request, timeout=60) as response:
            data = response.read()
        if sha256(data) != WINDOWS_SMOKE_MESA["sha256"]:
            raise RuntimeError("Windows smoke Mesa download checksum differs")
        archive.write_bytes(data)
    if sha256(archive.read_bytes()) != WINDOWS_SMOKE_MESA["sha256"]:
        raise RuntimeError("Windows smoke Mesa cached checksum differs")
    context.run("inspector-smoke-mesa-extract", ["7z", "e", "-y", f"-o{shipped.parent}", archive,
        *[f"x64/{name}" for name in WINDOWS_SMOKE_MESA["dlls"]]])
    dlls = {}
    for name in WINDOWS_SMOKE_MESA["dlls"]:
        path = shipped.parent / name
        if not path.is_file():
            raise RuntimeError(f"Windows smoke Mesa DLL is missing: {name}")
        dlls[name] = sha256(path.read_bytes())
    context.receipt["inspector_smoke_renderer"] = {
        "name": "Mesa llvmpipe", "version": WINDOWS_SMOKE_MESA["version"],
        "url": WINDOWS_SMOKE_MESA["url"], "archive_sha256": WINDOWS_SMOKE_MESA["sha256"],
        "dll_sha256": dlls, "distributed": False,
    }
    context.save()
    return {"GALLIUM_DRIVER": "llvmpipe", "LIBGL_ALWAYS_SOFTWARE": "1",
        "GLUTIN_WGL_OPENGL_DLL": str(shipped.parent / "opengl32.dll")}


def smoke_inspector(context, shipped, target):
    environment = context.env.copy()
    try:
        if target.endswith("msvc") and context.env.get("EMBEDSCOPE_WINDOWS_SMOKE_MESA") == "1":
            context.env.update(prepare_windows_smoke_mesa(context, shipped))
        context.run("shipped-inspector-smoke", [shipped, "--smoke-test"], timeout=45)
        context.expect_output("shipped-inspector-smoke", "embedscope-inspector-smoke-ok")
    finally:
        context.env = environment


def binaries(context, target=None):
    target = target or host_target()
    if target not in TARGETS or target != host_target():
        raise RuntimeError("Build and validate release binaries on a matching supported native runner")
    if target.endswith("msvc"):
        context.env["RUSTFLAGS"] = "-C target-feature=+crt-static"
    context.run("native-build", ["cargo", "build", "--release", "--locked", "--target", target,
        "-p", "escope-cli", "-p", "escope-desktop"])
    extension = ".exe" if target.endswith("msvc") else ""
    build = context.target / target / "release"
    with tempfile.TemporaryDirectory(prefix="embedscope-binary-") as temporary:
        temporary = Path(temporary)
        for (product, executable), filename in zip(
                (("escope", "escope"), ("embedscope-inspector", "escope-desktop")),
                distribution_names(context.version, target)):
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
                "CLI: extract the ZIP, then run escope --help; no Rust installation is needed.\n"
                "Add the executable directory to PATH to use escope from any terminal.\n"
                "Inspector: launch the application, then connect to a TCP or serial device.\n"
                "Linux AppImage: chmod +x the file, then run it in a desktop session.\n"
                "Linux builds require glibc 2.35+; CLI also uses the system libudev.\n"
                "Inspector needs a system OpenGL driver.\n"
                "Linux AppImage includes Noto Sans CJK; macOS/Windows use system fonts.\n"
                "macOS builds use ad hoc signatures; they are not notarized by Apple.\n"
                "EmbedScope is MIT licensed. Dependency licenses are listed in THIRD_PARTY.md.\n")
            shutil.copy2(ROOT / "THIRD_PARTY.md", stage / "THIRD_PARTY.md")
            dependency_notices(stage, target, context.env)
            (stage / "build.json").write_text(json.dumps({"version": context.version, "target": target,
                "source_commit": context.receipt["commit"], "source_dirty": context.receipt["dirty"],
                "build_binary_sha256": sha256(binary.read_bytes())}, indent=2) + "\n")
            destination = context.out / filename
            if destination.suffix == ".AppImage":
                appimage(context, binary, stage, destination)
            else:
                archive_directory(stage, destination)
                context.record(destination)
            if product == "escope":
                # Verify the executable after a distribution round trip.
                extracted = temporary / "unpacked"
                extract_zip(destination, extracted)
                shipped = extracted / name / ("escope" + extension)
                context.run("shipped-cli-version", [shipped, "--version"])
                context.expect_output("shipped-cli-version", f"escope {context.version}")
                context.run("shipped-cli-e2e", [sys.executable, ROOT / "scripts/check-e2e.py", "--binary", shipped,
                    "--output", context.out / "binary-e2e"])
            elif destination.suffix == ".zip":
                extracted = temporary / "inspector-unpacked"
                extract_zip(destination, extracted)
                if target.endswith("darwin"):
                    bundle = extracted / name / "EmbedScope Inspector.app"
                    context.run("shipped-inspector-signature", ["codesign", "--verify", "--strict", bundle])
                    shipped = bundle / "Contents/MacOS/escope-desktop"
                else:
                    shipped = extracted / name / "escope-desktop.exe"
                context.run("shipped-inspector-version", [shipped, "--version"])
                context.expect_output("shipped-inspector-version", f"EmbedScope Inspector {context.version}")
                smoke_inspector(context, shipped, target)
    context.receipt["target"] = target
    context.save()


def checksums(directory):
    paths = sorted(path for path in directory.iterdir() if path.name.endswith((".crate", ".zip", ".AppImage")))
    if not paths:
        raise RuntimeError("No release archives found")
    destination = directory / "SHA256SUMS"
    destination.write_text("".join(f"{sha256(path.read_bytes())}  {path.name}\n" for path in paths))
    return destination
