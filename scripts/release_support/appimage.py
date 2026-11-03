"""Build a Linux AppImage using checksum-pinned deployment tools."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

from .common import ROOT, sha256

TOOLS = {
    "linuxdeploy": (
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage",
        "c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d",
    ),
    "appimagetool": (
        "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage",
        "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0",
    ),
}


def system_notices(appdir, ldconfig):
    """Retain Ubuntu copyright/source metadata for dynamically bundled libraries."""
    origins = {}
    for line in ldconfig.splitlines():
        if "=>" in line and "x86-64" in line:
            origin = line.split("=>", 1)[1].strip()
            for name in (line.split()[0], Path(origin).name, Path(origin).resolve().name):
                origins[name] = origin
    packages = set()
    for library in sorted((appdir / "usr/lib").rglob("*.so*")):
        if not library.is_file():
            continue
        origin = origins.get(library.name)
        if not origin:
            raise RuntimeError(f"Bundled library has no host provenance: {library.name}")
        paths = {origin, str(Path(origin).resolve())}
        paths.update(path.replace("/usr/lib/", "/lib/") for path in list(paths))
        result = subprocess.run(["dpkg-query", "-S", *sorted(paths)], capture_output=True, text=True)
        owners = [line.split(": ", 1)[0] for line in result.stdout.splitlines() if ": " in line]
        if not owners:
            raise RuntimeError(f"Bundled library has no Ubuntu package owner: {library.name}")
        packages.update(owners)
    destination = appdir / "usr/share/doc/embedscope/system-libraries"
    destination.mkdir(parents=True)
    records = []
    for package in sorted(packages):
        fields = subprocess.check_output(["dpkg-query", "-W",
            "-f=${Package}\t${Version}\t${source:Package}\t${source:Version}", package], text=True).split("\t")
        name, version, source, source_version = fields
        source = source or name
        source_version = source_version or version
        shutil.copy2(f"/usr/share/doc/{name}/copyright", destination / f"{name}.copyright")
        records.append({"package": name, "version": version, "source": source,
            "source_version": source_version,
            "source_url": f"https://launchpad.net/ubuntu/+source/{source}/{source_version}"})
    (destination / "packages.json").write_text(json.dumps(records, indent=2) + "\n")


def fetch_tool(name, directory):
    url, digest = TOOLS[name]
    destination = directory / f"{name}.AppImage"
    if not destination.exists():
        request = urllib.request.Request(url, headers={"User-Agent": "EmbedScope release"})
        with urllib.request.urlopen(request, timeout=60) as response:
            data = response.read()
        if sha256(data) != digest:
            raise RuntimeError(f"{name} download checksum differs")
        destination.write_bytes(data)
    if sha256(destination.read_bytes()) != digest:
        raise RuntimeError(f"{name} cached checksum differs")
    destination.chmod(0o755)
    return destination


def appimage(context, binary, documentation, destination):
    if sys.platform != "linux":
        raise RuntimeError("AppImages must be built and tested on Linux")
    tools = context.target / "appimage-tools"
    tools.mkdir(parents=True, exist_ok=True)
    deploy = fetch_tool("linuxdeploy", tools)
    pack = fetch_tool("appimagetool", tools)
    appdir = documentation.parent / "EmbedScope.AppDir"
    appdir.mkdir()
    inputs = documentation.parent / "appimage-inputs"
    inputs.mkdir()
    desktop = inputs / "embedscope-inspector.desktop"
    desktop.write_text(
        "[Desktop Entry]\nType=Application\nName=EmbedScope Inspector\n"
        "Exec=escope-desktop\nIcon=embedscope-inspector\nCategories=Development;Utility;\n"
        "Terminal=false\nStartupWMClass=io.github.umbrella22.embedscope.inspector\n"
    )
    icon = inputs / "embedscope-inspector.png"
    shutil.copy2(ROOT / "crates/escope-desktop/assets/brand/app-icon.png", icon)
    # eframe loads these through dlopen, so an executable-only dependency scan
    # would miss them. linuxdeploy also resolves their transitive dependencies.
    libraries = ["libudev.so.1", "libxkbcommon.so.0", "libxkbcommon-x11.so.0",
        "libX11.so.6", "libXcursor.so.1", "libXi.so.6", "libXrandr.so.2",
        "libwayland-client.so.0", "libwayland-cursor.so.0", "libwayland-egl.so.1"]
    ldconfig = subprocess.check_output(["ldconfig", "-p"], text=True)
    arguments = []
    for name in libraries:
        matches = [line.split("=>", 1)[1].strip() for line in ldconfig.splitlines()
            if line.strip().startswith(name + " ") and "x86-64" in line]
        if not matches:
            raise RuntimeError(f"AppImage build dependency is missing: {name}")
        arguments.extend(["--library", matches[0]])
    context.env["APPIMAGE_EXTRACT_AND_RUN"] = "1"
    context.env["ARCH"] = "x86_64"
    context.run("appimage-deploy", [deploy, "--appdir", appdir, "--executable", binary,
        "--desktop-file", desktop, "--icon-file", icon, *arguments])
    system_notices(appdir, ldconfig)
    fonts = appdir / "usr/share/fonts"
    fonts.mkdir(parents=True, exist_ok=True)
    shutil.copy2("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", fonts)
    notices = appdir / "usr/share/doc/fonts-noto-cjk"
    notices.mkdir(parents=True)
    shutil.copy2("/usr/share/doc/fonts-noto-cjk/copyright", notices)
    shutil.copytree(documentation, appdir / "usr/share/doc/embedscope")
    context.run("appimage-build", [pack, "--no-appstream", appdir, destination])
    destination.chmod(0o755)
    context.record(destination)
    # Extraction works without a FUSE mount and tests the actual shipped image.
    extracted = documentation.parent / "appimage-unpacked"
    extracted.mkdir()
    context.run("appimage-extract", [destination, "--appimage-extract"], cwd=extracted)
    app_run = extracted / "squashfs-root/AppRun"
    context.run("shipped-inspector-version", [app_run, "--version"])
    context.expect_output("shipped-inspector-version", f"EmbedScope Inspector {context.version}")
    context.env["LIBGL_ALWAYS_SOFTWARE"] = "1"
    context.run("shipped-inspector-smoke", ["xvfb-run", "--auto-servernum", app_run, "--smoke-test"], timeout=45)
    context.expect_output("shipped-inspector-smoke", "embedscope-inspector-smoke-ok")
    context.receipt["appimage_tools"] = {name: {"url": url, "sha256": digest}
        for name, (url, digest) in TOOLS.items()}
    context.save()
