"""Release identity and command evidence shared by local and hosted builds."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
PACKAGES = ("escope-protocol", "escope-agent", "escope-lvgl", "escope-c-api", "escope-host", "escope-cli")
DEVICE_PACKAGES = PACKAGES[:4]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def release_version(requested=None):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version=1"], cwd=ROOT, text=True))
    packages = {package["name"]: package for package in metadata["packages"]}
    versions = {packages[name]["version"] for name in (*PACKAGES, "escope-desktop")}
    if len(versions) != 1:
        raise RuntimeError("SDK, CLI and Inspector versions must agree")
    version = versions.pop()
    if not re.fullmatch(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)", version):
        raise RuntimeError("Releases require a stable MAJOR.MINOR.PATCH version")
    if requested and requested not in (version, f"v{version}"):
        raise RuntimeError(f"Expected {version} or v{version}, got {requested}")
    for name in PACKAGES:
        package = packages[name]
        if package["publish"] == []:
            raise RuntimeError(f"{name} must be publishable")
        for field in ("description", "readme", "license", "repository", "homepage", "rust_version"):
            if not package.get(field):
                raise RuntimeError(f"{name} is missing {field}")
        for dependency in package["dependencies"]:
            if dependency["name"] in PACKAGES and dependency["req"] != f"^{version}":
                raise RuntimeError(f"{name}: internal dependency {dependency['name']} must use {version}")
    if packages["escope-desktop"]["publish"] != []:
        raise RuntimeError("Inspector is distributed as binaries, not a registry crate")
    return version


def require_clean():
    if git("status", "--porcelain", "--untracked-files=all"):
        raise RuntimeError("Release publication requires a clean checkout")
    return git("rev-parse", "HEAD")


def require_tag(version):
    commit = require_clean()
    tag = f"v{version}"
    if git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}") != commit:
        raise RuntimeError(f"HEAD must be the exact {tag} commit")
    refs = dict(line.split()[::-1] for line in git("ls-remote", "origin",
        f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}", "refs/heads/master").splitlines())
    if refs.get(f"refs/tags/{tag}^{{}}", refs.get(f"refs/tags/{tag}")) != commit:
        raise RuntimeError("Remote release tag differs from HEAD")
    if refs.get("refs/heads/master") != git("rev-parse", "origin/master"):
        raise RuntimeError("origin/master is stale; fetch before releasing")
    subprocess.run(["git", "merge-base", "--is-ancestor", commit, "origin/master"], cwd=ROOT, check=True)
    return commit


class Context:
    def __init__(self, version, out, name):
        self.version = version
        self.out = out.resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.name = name
        self.env = os.environ.copy()
        self.env["CARGO_TERM_COLOR"] = "never"
        self.target = Path(self.env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
        self.env["CARGO_TARGET_DIR"] = str(self.target)
        # Native LVGL verification must not inherit an unrelated firmware config.
        for key in ("LVGL_CONFIG_PATH", "LVGL_EXTRA_C_DIR", "LVGL_EXTRA_HEADERS", "LVGL_CFLAGS"):
            self.env.pop(key, None)
        self.receipt = {"version": version, "commit": git("rev-parse", "HEAD"),
            "dirty": bool(git("status", "--porcelain", "--untracked-files=all")),
            "lock_sha256": sha256((ROOT / "Cargo.lock").read_bytes()), "commands": [], "artifacts": {}}
        self.save()

    def save(self):
        (self.out / f"{self.name}.json").write_text(json.dumps(self.receipt, indent=2) + "\n")

    def run(self, name, command, cwd=ROOT, timeout=1800):
        log = self.out / f"{self.name}-{name}.log"
        print(f"{name}: running", flush=True)
        started = time.time()
        with log.open("w") as output:
            try:
                result = subprocess.run([str(part) for part in command], cwd=cwd, env=self.env,
                    stdout=output, stderr=subprocess.STDOUT, timeout=timeout, check=False)
                code = result.returncode
            except subprocess.TimeoutExpired:
                output.write(f"\nCommand exceeded {timeout} seconds.\n")
                code = 124
        self.receipt["commands"].append({"name": name, "command": [str(part) for part in command],
            "seconds": round(time.time() - started, 3), "exit": code,
            "log_sha256": sha256(log.read_bytes())})
        self.save()
        if code:
            raise RuntimeError(f"{name} failed; see {log}\n{log.read_text(errors='replace')[-3500:]}")
        print(f"{name}: PASS", flush=True)

    def expect_output(self, name, expected):
        if expected not in (self.out / f"{self.name}-{name}.log").read_text():
            raise RuntimeError(f"{name} did not produce the expected output: {expected}")

    def record(self, path):
        self.receipt["artifacts"][path.name] = {"sha256": sha256(path.read_bytes()), "bytes": path.stat().st_size}
        self.save()
