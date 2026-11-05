"""Regression tests for irreversible release failure boundaries."""

import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tarfile
import subprocess
import tempfile
import unittest
import zipfile
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_support.common import PACKAGES, release_version, sha256
from release_support.packages import archive_files, check_native_dependencies
from release_support.registry import reconcile_archive
from release_support.binaries import (archive_directory, binaries, checksums, extract_zip,
    distribution_names, prepare_windows_smoke_mesa, smoke_inspector, TARGETS, WINDOWS_SMOKE_MESA)
from release_support.appimage import appimage, appimage_icon, fetch_tool
from release_support.common import Context

spec = importlib.util.spec_from_file_location("github_release", SCRIPTS / "publish-github-release.py")
github_release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(github_release)

VERSION = "0.1.0"
COMMIT = "a" * 40


def metadata():
    packages = [{"name": name, "version": VERSION, "publish": None,
        "description": "SDK", "readme": "README.md", "license": "MIT",
        "repository": "https://github.com/umbrella22/EmbedScope", "homepage": "https://example.org",
        "rust_version": "1.88", "dependencies": []} for name in PACKAGES]
    packages.append({"name": "escope-desktop", "version": VERSION, "publish": []})
    return {"packages": packages}


def crate(extra=None, source=b"pub const VALUE: u8 = 1;", dirty=False):
    files = {"Cargo.toml": b'[package]\nname="escope-protocol"\nversion="0.1.0"\n',
        "Cargo.toml.orig": b"original", "README.md": b"readme", "LICENSE": b"MIT",
        "src/lib.rs": source, ".cargo_vcs_info.json": json.dumps({"git": {"sha1": COMMIT, "dirty": dirty}}).encode()}
    files.update(extra or {})
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for path, data in files.items():
            info = tarfile.TarInfo("escope-protocol-0.1.0/" + path)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return output.getvalue()


class ReleaseIdentityTests(unittest.TestCase):
    def check_metadata(self, value):
        with patch("release_support.common.subprocess.check_output", return_value=json.dumps(value)):
            return release_version()

    def test_skewed_package_version_is_rejected(self):
        value = metadata()
        value["packages"][2]["version"] = "0.2.0"
        with self.assertRaisesRegex(RuntimeError, "versions must agree"):
            self.check_metadata(value)

    def test_wrong_internal_requirement_is_rejected(self):
        value = metadata()
        value["packages"][1]["dependencies"] = [{"name": "escope-protocol", "req": "^0.0.9"}]
        with self.assertRaisesRegex(RuntimeError, "internal dependency"):
            self.check_metadata(value)

    def test_accidental_desktop_publication_is_rejected(self):
        value = metadata()
        value["packages"][-1]["publish"] = None
        with self.assertRaisesRegex(RuntimeError, "Inspector is distributed as binaries"):
            self.check_metadata(value)


class ArchiveTests(unittest.TestCase):
    def test_path_escape_and_windows_paths_are_rejected(self):
        for path in ("../escaped", "/absolute", "C:\\escaped", "nested/../../escaped"):
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                archive_files(crate({path: b"bad"}), "escope-protocol", VERSION)

    def test_missing_license_is_rejected(self):
        data = crate()
        output = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(data)) as source, tarfile.open(fileobj=output, mode="w:gz") as target:
            for member in source:
                if not member.name.endswith("/LICENSE"):
                    target.addfile(member, source.extractfile(member))
        with self.assertRaisesRegex(RuntimeError, "missing LICENSE"):
            archive_files(output.getvalue(), "escope-protocol", VERSION)

    def test_conflicting_public_source_is_rejected_even_with_valid_checksum(self):
        local, remote = crate(), crate(source=b"pub const VALUE: u8 = 2;")
        with self.assertRaisesRegex(RuntimeError, "source differs"):
            reconcile_archive(local, remote, sha256(remote), "escope-protocol", VERSION, COMMIT)

    def test_public_checksum_and_source_commit_are_both_required(self):
        remote = crate()
        with self.assertRaisesRegex(RuntimeError, "checksum"):
            reconcile_archive(remote, remote, "0" * 64, "escope-protocol", VERSION, COMMIT)
        with self.assertRaisesRegex(RuntimeError, "clean commit"):
            reconcile_archive(remote, remote, sha256(remote), "escope-protocol", VERSION, "b" * 40)

    def test_dirty_archive_cannot_be_treated_as_a_public_release(self):
        remote = crate(dirty=True)
        with self.assertRaisesRegex(RuntimeError, "clean commit"):
            reconcile_archive(remote, remote, sha256(remote), "escope-protocol", VERSION, COMMIT)

    def test_generated_lockfile_does_not_change_source_identity(self):
        local, remote = crate({"Cargo.lock": b"local"}), crate({"Cargo.lock": b"registry"})
        reconcile_archive(local, remote, sha256(remote), "escope-protocol", VERSION, COMMIT)


class NativeConsumerTests(unittest.TestCase):
    def registry_metadata(self):
        return {"packages": [{"name": name, "version": version,
            "source": "registry+https://github.com/rust-lang/crates.io-index",
            "manifest_path": f"/registry/{name}/Cargo.toml"}
            for name, version in [("escope-protocol", VERSION), ("escope-lvgl", VERSION),
                ("rs_lvgl", "0.5.0"), ("rs_lvgl_ui", "0.5.0"), ("rs_lvgl_sys", "0.5.0")]]}

    def test_mixed_native_provider_versions_are_rejected(self):
        value = self.registry_metadata()
        value["packages"].append({**value["packages"][-1], "version": "0.3.1"})
        with self.assertRaisesRegex(RuntimeError, "exactly one rs_lvgl_sys"):
            check_native_dependencies(value, VERSION, {})

    def test_registry_consumer_rejects_local_sdk_and_provider(self):
        for name in ("escope-lvgl", "rs_lvgl_sys"):
            with self.subTest(name=name):
                value = self.registry_metadata()
                next(package for package in value["packages"] if package["name"] == name)["source"] = None
                with self.assertRaisesRegex(RuntimeError, f"resolve {name} from crates.io"):
                    check_native_dependencies(value, VERSION, {})

    def test_archive_consumer_requires_the_extracted_sdk(self):
        value = self.registry_metadata()
        patches = {"escope-lvgl": Path("/extracted/escope-lvgl")}
        with self.assertRaisesRegex(RuntimeError, "extracted escope-lvgl archive"):
            check_native_dependencies(value, VERSION, patches)
        package = next(package for package in value["packages"] if package["name"] == "escope-lvgl")
        package.update(source=None, manifest_path="/extracted/escope-lvgl/Cargo.toml")
        check_native_dependencies(value, VERSION, patches)


class DistributionTests(unittest.TestCase):
    def test_appimage_retains_system_notices_when_installing_distribution_docs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            context = Context(VERSION, root / "dist", "binaries")
            context.target = root / "target"
            docs = root / "documentation"
            (docs / "licenses/example-sdk").mkdir(parents=True)
            (docs / "README.txt").write_text("distribution instructions")
            (docs / "dependency-licenses.json").write_text('[{"name":"example-sdk"}]')
            (docs / "licenses/example-sdk/LICENSE").write_text("SDK attribution")
            binary = root / "escope-desktop"
            binary.write_bytes(b"Inspector fixture")
            image = context.out / "inspector.AppImage"
            appdir = root / "EmbedScope.AppDir"
            libraries = ("libudev.so.1", "libxkbcommon.so.0", "libxkbcommon-x11.so.0",
                "libX11.so.6", "libXcursor.so.1", "libXi.so.6", "libXrandr.so.2",
                "libwayland-client.so.0", "libwayland-cursor.so.0", "libwayland-egl.so.1")
            ldconfig = "\n".join(f"{name} (libc6,x86-64) => /usr/lib/{name}" for name in libraries)
            copy2 = shutil.copy2

            def retain_system_notices(directory, unused):
                notices = directory / "usr/share/doc/embedscope/system-libraries"
                notices.mkdir(parents=True)
                (notices / "packages.json").write_text('[{"package":"example-system-lib"}]')
                (notices / "example-system-lib.copyright").write_text("system attribution")

            def copy_host_font(source, destination, **kwargs):
                if str(source).startswith("/usr/share/"):
                    destination = Path(destination)
                    if destination.is_dir():
                        destination /= Path(source).name
                    destination.write_bytes(b"host font or font copyright fixture")
                    return destination
                return copy2(source, destination, **kwargs)

            def run(name, command, **kwargs):
                if name == "appimage-build":
                    image.write_bytes(b"built AppImage fixture")

            with patch("release_support.appimage.sys.platform", "linux"), \
                    patch("release_support.appimage.fetch_tool", return_value=root / "tool.AppImage"), \
                    patch("release_support.appimage.subprocess.check_output", return_value=ldconfig), \
                    patch("release_support.appimage.system_notices", side_effect=retain_system_notices), \
                    patch("release_support.appimage.shutil.copy2", side_effect=copy_host_font), \
                    patch.object(context, "run", side_effect=run), patch.object(context, "expect_output"):
                appimage(context, binary, docs, image)
            installed = appdir / "usr/share/doc/embedscope"
            self.assertEqual((installed / "README.txt").read_text(), "distribution instructions")
            self.assertEqual((installed / "dependency-licenses.json").read_text(), '[{"name":"example-sdk"}]')
            self.assertEqual((installed / "licenses/example-sdk/LICENSE").read_text(), "SDK attribution")
            self.assertEqual((installed / "system-libraries/packages.json").read_text(), '[{"package":"example-system-lib"}]')
            self.assertEqual((installed / "system-libraries/example-system-lib.copyright").read_text(), "system attribution")
            self.assertTrue((appdir / "usr/share/fonts/NotoSansCJK-Regular.ttc").is_file())
            self.assertTrue((appdir / "usr/share/doc/fonts-noto-cjk/copyright").is_file())

    def test_appimage_uses_512px_brand_icon_without_changing_window_icon(self):
        brand = SCRIPTS.parent / "crates/escope-desktop/assets/brand"
        window_icon = (brand / "app-icon.png").read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "appimage.png"
            appimage_icon(brand / "app-icon.icns", destination)
            self.assertEqual(destination.read_bytes()[16:24], (512).to_bytes(4, "big") * 2)
            self.assertEqual((brand / "app-icon.png").read_bytes(), window_icon)
            self.assertEqual(window_icon[16:24], (1024).to_bytes(4, "big") * 2)

    def test_appimage_icon_rejects_truncated_members_and_wrong_dimensions(self):
        png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" + (1024).to_bytes(4, "big") * 2
        for member in (b"ic09" + (100).to_bytes(4, "big"),
                b"ic09" + (len(png) + 8).to_bytes(4, "big") + png):
            with self.subTest(member=member), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "icon.icns"
                source.write_bytes(b"icns" + (len(member) + 8).to_bytes(4, "big") + member)
                with self.assertRaises(RuntimeError):
                    appimage_icon(source, root / "icon.png")
                self.assertFalse((root / "icon.png").exists())

    def test_windows_smoke_mesa_is_added_after_zip_and_never_distributed(self):
        target = "x86_64-pc-windows-msvc"
        data = b"checksum-pinned Mesa test fixture"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            context = Context(VERSION, root / "dist", "binaries")
            context.target = root / "target"
            context.env["EMBEDSCOPE_WINDOWS_SMOKE_MESA"] = "1"
            build = context.target / target / "release"
            build.mkdir(parents=True)
            for name in ("escope.exe", "escope-desktop.exe"):
                (build / name).write_bytes(b"native executable fixture")
            smoke_environments = []
            published_zip = context.out / distribution_names(VERSION, target)[1]
            archived_digest = None

            def run(name, command, **kwargs):
                nonlocal archived_digest
                if name == "inspector-smoke-mesa-extract":
                    # The immutable published ZIP must already exist at this point.
                    archived_digest = sha256(published_zip.read_bytes())
                    unpacked = Path(str(command[3]).removeprefix("-o"))
                    self.assertIn("inspector-unpacked", unpacked.parts)
                    self.assertEqual(command[5:], [f"x64/{dll}" for dll in WINDOWS_SMOKE_MESA["dlls"]])
                    for dll in WINDOWS_SMOKE_MESA["dlls"]:
                        (unpacked / dll).write_bytes(b"Mesa " + dll.encode())
                elif name == "shipped-inspector-smoke":
                    smoke_environments.append(context.env.copy())
                    self.assertEqual(command[1], "--smoke-test")
                    self.assertEqual(kwargs["timeout"], 45)
                    self.assertTrue(Path(context.env["GLUTIN_WGL_OPENGL_DLL"]).is_file())

            with patch("release_support.binaries.sys.platform", "win32"), \
                    patch("release_support.binaries.host_target", return_value=target), \
                    patch("release_support.binaries.dependency_notices"), \
                    patch("release_support.binaries.urllib.request.urlopen", return_value=io.BytesIO(data)), \
                    patch.dict(WINDOWS_SMOKE_MESA, {"sha256": sha256(data)}), \
                    patch.object(context, "run", side_effect=run), \
                    patch.object(context, "expect_output") as expect:
                binaries(context, target)
            self.assertEqual(len(smoke_environments), 1)
            self.assertEqual(smoke_environments[0]["GALLIUM_DRIVER"], "llvmpipe")
            self.assertEqual(smoke_environments[0]["LIBGL_ALWAYS_SOFTWARE"], "1")
            self.assertNotIn("GLUTIN_WGL_OPENGL_DLL", context.env)
            self.assertFalse(context.receipt["inspector_smoke_renderer"]["distributed"])
            self.assertEqual(sha256(published_zip.read_bytes()), archived_digest)
            with zipfile.ZipFile(published_zip) as archive:
                self.assertFalse(any(name.endswith(".dll") for name in archive.namelist()))
            expect.assert_any_call("shipped-inspector-smoke", "embedscope-inspector-smoke-ok")

    def test_modified_windows_smoke_mesa_cache_is_rejected_before_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            context = Context(VERSION, root / "dist", "binaries")
            context.target = root
            cache = root / "windows-smoke-mesa"
            cache.mkdir()
            (cache / WINDOWS_SMOKE_MESA["archive"]).write_bytes(b"changed")
            with patch("release_support.binaries.sys.platform", "win32"), patch.object(context, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "cached checksum differs"):
                    prepare_windows_smoke_mesa(context, root / "unpacked/escope-desktop.exe")
                run.assert_not_called()

    def test_windows_smoke_mesa_missing_dll_stops_before_ui_launch(self):
        data = b"checksum-pinned but incomplete Mesa fixture"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            context = Context(VERSION, root / "dist", "binaries")
            context.target = root
            context.env["EMBEDSCOPE_WINDOWS_SMOKE_MESA"] = "1"
            shipped = root / "inspector-unpacked/escope-desktop.exe"
            shipped.parent.mkdir()
            with patch("release_support.binaries.sys.platform", "win32"), \
                    patch("release_support.binaries.urllib.request.urlopen", return_value=io.BytesIO(data)), \
                    patch.dict(WINDOWS_SMOKE_MESA, {"sha256": sha256(data)}), \
                    patch.object(context, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "Mesa DLL is missing"):
                    smoke_inspector(context, shipped, "x86_64-pc-windows-msvc")
            self.assertEqual([call.args[0] for call in run.call_args_list], ["inspector-smoke-mesa-extract"])

    def test_failed_windows_ui_smoke_still_fails_and_restores_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            context = Context(VERSION, Path(directory), "binaries")
            context.env["EMBEDSCOPE_WINDOWS_SMOKE_MESA"] = "1"
            original = context.env.copy()
            with patch("release_support.binaries.prepare_windows_smoke_mesa", return_value={"GALLIUM_DRIVER": "llvmpipe"}), \
                    patch.object(context, "run", side_effect=RuntimeError("real UI smoke failed")):
                with self.assertRaisesRegex(RuntimeError, "real UI smoke failed"):
                    smoke_inspector(context, Path(directory) / "escope-desktop.exe", "x86_64-pc-windows-msvc")
            self.assertEqual(context.env, original)

    @unittest.skipIf(os.name == "nt", "Unix ZIP executable permission contract")
    def test_cli_remains_executable_after_zip_round_trip(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / "escope-unix"
            stage.mkdir()
            binary = stage / "escope"
            binary.write_text("#!/bin/sh\necho CLI_ZIP_OK\n")
            binary.chmod(0o755)
            # Registry dependency files can use an epoch timestamp.
            os.utime(binary, (0, 0))
            archive_directory(stage, root / "escope.zip")
            extract_zip(root / "escope.zip", root / "unpacked")
            result = subprocess.run([root / "unpacked/escope-unix/escope"],
                capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout.strip(), "CLI_ZIP_OK")

    def test_distribution_zip_cannot_write_outside_extraction_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with zipfile.ZipFile(root / "bad.zip", "w") as archive:
                archive.writestr("../escaped", "unexpected")
            with self.assertRaisesRegex(RuntimeError, "Unsafe ZIP"):
                extract_zip(root / "bad.zip", root / "unpacked")
            self.assertFalse((root / "escaped").exists())

    def test_appimage_is_included_in_public_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "inspector.AppImage"
            image.write_bytes(b"AppImage")
            self.assertIn(sha256(b"AppImage") + "  inspector.AppImage", checksums(root).read_text())

    def test_modified_appimage_tool_cache_is_rejected_before_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "linuxdeploy.AppImage").write_bytes(b"modified tool")
            with self.assertRaisesRegex(RuntimeError, "cached checksum differs"):
                fetch_tool("linuxdeploy", root)

    def test_inspector_smoke_gate_is_required_for_asset_collection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "collected/release-source"
            source.mkdir(parents=True)
            names = [f"{name}-{VERSION}.crate" for name in PACKAGES] + [f"embedscope-c-sdk-{VERSION}.zip"]
            for name in names:
                (source / name).write_bytes(b"source")
            (source / "verify.json").write_text(json.dumps({"version": VERSION, "commit": COMMIT,
                "dirty": False, "commands": [{"name": name, "exit": 0} for name in [
                    "cargo-package", "consumer-sdk", "consumer-native-ui", "consumer-cli-install", "consumer-cli-e2e",
                    "c-sdk-host-build", "c-sdk-native-libs", "c-sdk-c-link", "c-sdk-c-harness", "c-sdk-no-std"]],
                "artifacts": {name: {"sha256": sha256(b"source")} for name in names}}))
            target = sorted(TARGETS)[0]
            platform = root / f"collected/release-{target}"
            platform.mkdir()
            (platform / "binaries.json").write_text(json.dumps({"version": VERSION, "commit": COMMIT,
                "target": target, "dirty": False, "commands": [{"name": name, "exit": 0} for name in [
                    "native-build", "shipped-cli-version", "shipped-cli-e2e", "shipped-inspector-version",
                    "shipped-inspector-signature"]], "artifacts": {}}))
            with self.assertRaisesRegex(RuntimeError, "validation is incomplete"):
                github_release.collect(root / "collected", root / "assets", VERSION, COMMIT)

    def test_native_launch_timeout_is_recorded_as_a_failed_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            context = Context(VERSION, Path(directory), "timeout")
            with self.assertRaisesRegex(RuntimeError, "launch failed"):
                context.run("launch", [sys.executable, "-c", "import time; time.sleep(2)"], timeout=0.05)
            receipt = json.loads((Path(directory) / "timeout.json").read_text())
            self.assertEqual(receipt["commands"][-1]["exit"], 124)

    def test_missing_validation_gate_stops_asset_collection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "collected/release-source"
            source.mkdir(parents=True)
            (source / "verify.json").write_text(json.dumps({"version": VERSION, "commit": COMMIT,
                "dirty": False, "commands": [{"name": "cargo-package", "exit": 0}], "artifacts": {}}))
            with self.assertRaisesRegex(RuntimeError, "validation is incomplete"):
                github_release.collect(root / "collected", root / "assets", VERSION, COMMIT)

    def test_published_release_assets_are_never_silently_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory)
            for name in github_release.public_asset_names(VERSION):
                (stage / name).write_bytes(b"verified")
            result = type("Result", (), {"returncode": 0, "stdout": json.dumps({
                "tagName": "v0.1.0", "isDraft": False, "isPrerelease": False, "assets": []})})()
            with patch.object(github_release, "gh", return_value=result) as gh:
                with self.assertRaisesRegex(RuntimeError, "Published release is incomplete"):
                    github_release.publish_assets("v0.1.0", stage, VERSION)
                self.assertEqual(gh.call_count, 1)


class GitHubReleaseTests(unittest.TestCase):
    def collected_artifacts(self, root):
        groups = [("release-source", "verify",
            [f"{name}-{VERSION}.crate" for name in PACKAGES] + [f"embedscope-c-sdk-{VERSION}.zip"],
            ["cargo-package", "consumer-sdk", "consumer-native-ui", "consumer-cli-install", "consumer-cli-e2e",
                "c-sdk-host-build", "c-sdk-native-libs", "c-sdk-c-link", "c-sdk-c-harness", "c-sdk-no-std"], None)]
        for target in TARGETS:
            commands = ["native-build", "shipped-cli-version", "shipped-cli-e2e",
                "shipped-inspector-version", "shipped-inspector-smoke"]
            if target.endswith("darwin"):
                commands.append("shipped-inspector-signature")
            if target.endswith("linux-gnu"):
                commands.extend(["appimage-deploy", "appimage-build", "appimage-extract"])
            groups.append((f"release-{target}", "binaries", distribution_names(VERSION, target), commands, target))
        for group, receipt_name, names, commands, target in groups:
            directory = root / group
            directory.mkdir(parents=True)
            for name in names:
                (directory / name).write_bytes(name.encode())
            (directory / f"{receipt_name}.json").write_text(json.dumps({
                "version": VERSION, "commit": COMMIT, "dirty": False, "target": target,
                "commands": [{"name": name, "exit": 0} for name in commands],
                "artifacts": {name: {"sha256": sha256(name.encode())} for name in names}}))

    def test_public_assets_and_checksums_contain_only_distributions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, assets = root / "collected", root / "public"
            self.collected_artifacts(source)
            github_release.collect(source, assets, VERSION, COMMIT)
            expected = {"SHA256SUMS", f"embedscope-c-sdk-{VERSION}.zip"}
            for target in TARGETS:
                expected.update(distribution_names(VERSION, target))
            self.assertEqual(len(expected), 10)
            self.assertEqual({path.name for path in assets.iterdir()}, expected)
            sums = dict((name, checksum) for checksum, name in
                (line.split("  ", 1) for line in (assets / "SHA256SUMS").read_text().splitlines()))
            self.assertEqual(set(sums), expected - {"SHA256SUMS"})
            for name, checksum in sums.items():
                self.assertEqual(checksum, sha256((assets / name).read_bytes()))
            for package in PACKAGES:
                name = f"{package}-{VERSION}.crate"
                self.assertEqual((source / "release-source" / name).read_bytes(), name.encode())
                self.assertFalse((assets / name).exists())

    def test_registry_archives_still_require_valid_ci_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.collected_artifacts(root / "collected")
            archive = root / "collected/release-source" / f"escope-agent-{VERSION}.crate"
            archive.write_bytes(b"changed registry archive")
            with self.assertRaisesRegex(RuntimeError, "Artifact checksum differs"):
                github_release.collect(root / "collected", root / "public", VERSION, COMMIT)

    def test_native_ui_consumer_is_required_for_asset_collection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.collected_artifacts(root / "collected")
            receipt = root / "collected/release-source/verify.json"
            value = json.loads(receipt.read_text())
            value["commands"] = [command for command in value["commands"] if command["name"] != "consumer-native-ui"]
            receipt.write_text(json.dumps(value))
            with self.assertRaisesRegex(RuntimeError, "validation is incomplete"):
                github_release.collect(root / "collected", root / "public", VERSION, COMMIT)

    def test_extra_or_missing_assets_are_rejected_before_github_access(self):
        for change in ("extra-crate", "missing-cli"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                stage = Path(directory)
                for name in github_release.public_asset_names(VERSION):
                    (stage / name).write_bytes(b"verified")
                if change == "extra-crate":
                    (stage / f"escope-agent-{VERSION}.crate").write_bytes(b"internal archive")
                else:
                    (stage / f"escope-{VERSION}-aarch64-apple-darwin.zip").unlink()
                with patch.object(github_release, "gh") as gh:
                    with self.assertRaisesRegex(RuntimeError, "GitHub Release asset set differs"):
                        github_release.publish_assets(f"v{VERSION}", stage, VERSION)
                    gh.assert_not_called()


if __name__ == "__main__":
    unittest.main()
