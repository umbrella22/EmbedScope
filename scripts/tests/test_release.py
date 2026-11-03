"""Regression tests for irreversible release failure boundaries."""

import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_support.common import PACKAGES, release_version, sha256
from release_support.packages import archive_files
from release_support.registry import reconcile_archive

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


class DistributionTests(unittest.TestCase):
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
            (stage / "asset.zip").write_bytes(b"verified")
            result = type("Result", (), {"returncode": 0, "stdout": json.dumps({
                "tagName": "v0.1.0", "isDraft": False, "isPrerelease": False, "assets": []})})()
            with patch.object(github_release, "gh", return_value=result) as gh:
                with self.assertRaisesRegex(RuntimeError, "Published release is incomplete"):
                    github_release.publish_assets("v0.1.0", stage, VERSION)
                self.assertEqual(gh.call_count, 1)


if __name__ == "__main__":
    unittest.main()
