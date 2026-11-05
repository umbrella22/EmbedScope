"""Cargo archives and independent SDK/CLI consumers."""

import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

from .common import PACKAGES, ROOT


def archive_files(data, name, version):
    prefix = f"{name}-{version}/"
    files = {}
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            path = PurePosixPath(member.name)
            if not member.name.startswith(prefix) or not member.isfile() or ".." in path.parts:
                raise RuntimeError(f"Unexpected archive member: {member.name}")
            relative = member.name[len(prefix):]
            if (not relative or PurePosixPath(relative).is_absolute()
                    or "\\" in relative or ":" in relative):
                raise RuntimeError(f"Unsafe archive member: {member.name}")
            if relative in files:
                raise RuntimeError(f"Duplicate archive member: {member.name}")
            files[relative] = archive.extractfile(member).read()
    required = ["Cargo.toml", "Cargo.toml.orig", "README.md", "LICENSE",
                "src/main.rs" if name == "escope-cli" else "src/lib.rs"]
    if name == "escope-c-api":
        required.append("include/escope.h")
    if name == "escope-agent":
        required.append("examples/poll_device.rs")
    if name == "escope-lvgl":
        required.append("build.rs")
    for path in required:
        if path not in files:
            raise RuntimeError(f"{name} archive is missing {path}")
    if any(path.startswith(("docs/", "target/", ".git/")) for path in files):
        raise RuntimeError(f"{name} archive contains internal build or planning files")
    package = tomllib.loads(files["Cargo.toml"].decode())["package"]
    if (package["name"], package["version"]) != (name, version):
        raise RuntimeError(f"{name} archive identity differs from the release")
    return files


def package(context, allow_dirty=False):
    command = ["cargo", "package", "--workspace", "--exclude", "escope-desktop", "--locked", "--registry", "crates-io"]
    if allow_dirty:
        command.append("--allow-dirty")
    context.run("cargo-package", command)
    archives = {}
    for name in PACKAGES:
        filename = f"{name}-{context.version}.crate"
        source = context.target / "package" / filename
        # Workspace packaging stages unpublished siblings in a temporary registry.
        if not source.is_file():
            source = context.target / "package/tmp-crate" / filename
        data = source.read_bytes()
        archive_files(data, name, context.version)
        destination = context.out / filename
        destination.write_bytes(data)
        context.record(destination)
        archives[name] = destination
    return archives


def check_native_dependencies(metadata, version, patches):
    """Require one shared registry LVGL provider and the intended SDK source."""
    expected = {"escope-protocol": version, "escope-lvgl": version,
        "rs_lvgl": "0.5.0", "rs_lvgl_ui": "0.5.0", "rs_lvgl_sys": "0.5.0"}
    registry = "registry+https://github.com/rust-lang/crates.io-index"
    for name, required in expected.items():
        packages = [package for package in metadata["packages"] if package["name"] == name]
        if len(packages) != 1 or packages[0]["version"] != required:
            raise RuntimeError(f"Native consumer requires exactly one {name} {required}")
        package = packages[0]
        if name in patches:
            if package["source"] is not None or Path(package["manifest_path"]).parent != patches[name]:
                raise RuntimeError(f"Native consumer did not use the extracted {name} archive")
        elif package["source"] != registry:
            raise RuntimeError(f"Native consumer did not resolve {name} from crates.io")


def native_consumer(context, root, patches, patch):
    fixture = root / "native-ui"
    (fixture / "src").mkdir(parents=True)
    (fixture / "Cargo.toml").write_text('''[package]
name = "escope-release-native-ui"
version = "0.0.0"
edition = "2021"
[workspace]
[features]
composition = ["rs_lvgl_ui/components", "rs_lvgl_ui/widget-slider", "rs_lvgl_ui/widget-switch"]
[dependencies]
''' + f'escope-protocol = "={context.version}"\n'
        + f'escope-lvgl = {{ version = "={context.version}", features = ["native"] }}\n'
        + '''rs_lvgl = { version = "=0.5.0", default-features = false, features = ["alloc", "unsafe_no_autoinit"] }
rs_lvgl_ui = { version = "=0.5.0", default-features = false, features = ["alloc", "no-autoinit"] }
''' + patch)
    shutil.copytree(ROOT / "examples/rs-lvgl-ui/src", fixture / "src", dirs_exist_ok=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version=1"],
        cwd=fixture, env=context.env, stderr=subprocess.DEVNULL))
    check_native_dependencies(metadata, context.version, patches)
    context.run("consumer-native-ui", ["cargo", "test", "--manifest-path", fixture / "Cargo.toml",
        "--features", "composition", "--locked", "--", "--test-threads=1"])
    context.receipt["native_provider"] = {"name": "rs_lvgl_sys", "version": "0.5.0", "source": "crates.io"}
    context.save()


def consumer(context, archives=None):
    """Build outside the checkout. Local mode patches only extracted archives."""
    with tempfile.TemporaryDirectory(prefix="embedscope-consumer-") as directory:
        root = Path(directory)
        patches = {}
        if archives:
            for name, archive in archives.items():
                files = archive_files(archive.read_bytes(), name, context.version)
                destination = root / f"{name}-{context.version}"
                for relative, data in files.items():
                    path = destination / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                patches[name] = destination
        dependencies = "".join(f'{name} = "={context.version}"\n' for name in PACKAGES if name != "escope-cli")
        patch = "\n[patch.crates-io]\n" + "".join(
            f"{name} = {{ path = {json.dumps(str(path))} }}\n" for name, path in patches.items()) if patches else ""
        fixture = root / "sdk"
        (fixture / "src").mkdir(parents=True)
        (fixture / "Cargo.toml").write_text('[package]\nname = "escope-release-consumer"\nversion = "0.0.0"\nedition = "2021"\n[workspace]\n[dependencies]\n' + dependencies + patch)
        (fixture / "src/main.rs").write_text('''fn main() {
    assert_eq!(escope_protocol::VERSION, 1);
    let agent = escope_agent::Agent::new(42);
    assert_eq!(agent.session_id(), 42);
    let mut nodes = escope_lvgl::NodeRegistry::<2>::new();
    let id = nodes.insert(0x1000).unwrap();
    assert_eq!(nodes.resolve(id), Ok(0x1000));
    assert!(escope_c_api::escope_context_size() > 0);
    let _options = escope_host::ConnectOptions::default();
    println!("SDK_CONSUMER_OK");
}
''')
        context.run("consumer-sdk", ["cargo", "run", "--manifest-path", fixture / "Cargo.toml"])
        metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version=1"],
            cwd=fixture, env=context.env, stderr=subprocess.DEVNULL))
        found = set()
        for package in metadata["packages"]:
            name = package["name"]
            if name not in PACKAGES:
                continue
            found.add(name)
            if package["version"] != context.version:
                raise RuntimeError("Consumer resolved the wrong EmbedScope version")
            if patches:
                if Path(package["manifest_path"]).parent != patches[name]:
                    raise RuntimeError("Consumer did not use the extracted archive")
            elif package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
                raise RuntimeError("Consumer did not resolve EmbedScope from crates.io")
        if found != set(PACKAGES) - {"escope-cli"}:
            raise RuntimeError("Consumer dependency graph is incomplete")
        native_consumer(context, root, patches, patch)
        install = root / "installed"
        if patches:
            # Cargo install reads configuration from its caller, not the package.
            config = root / ".cargo/config.toml"
            config.parent.mkdir()
            config.write_text(patch.lstrip())
            command = ["cargo", "install", "--path", patches["escope-cli"]]
        else:
            command = ["cargo", "install", "escope-cli", "--version", f"={context.version}", "--registry", "crates-io"]
        context.run("consumer-cli-install", [*command, "--locked", "--root", install], cwd=root)
        executable = install / "bin" / ("escope.exe" if os.name == "nt" else "escope")
        context.run("consumer-cli-e2e", [sys.executable, ROOT / "scripts/check-e2e.py", "--binary", executable,
            "--output", context.out / "consumer-e2e"])
        context.receipt["consumer_source"] = "extracted archives" if patches else "crates.io only"
        context.save()
