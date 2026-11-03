"""Dependency-order upload and public registry/source reconciliation."""

import json
import re
import time
import urllib.error
import urllib.request

from .common import PACKAGES, require_clean, require_tag, sha256
from .packages import archive_files, consumer, package

USER_AGENT = "EmbedScope release (https://github.com/umbrella22/EmbedScope)"


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def registry_record(name, version):
    try:
        record = json.loads(download(f"https://crates.io/api/v1/crates/{name}/{version}"))["version"]
    except urllib.error.HTTPError as error:
        if error.code == 404:
            error.close()
            return None
        raise
    if (record.get("crate") != name or record.get("num") != version or record.get("yanked")
            or not re.fullmatch(r"[0-9a-f]{64}", record.get("checksum", ""))):
        raise RuntimeError(f"Invalid or yanked crates.io record for {name} {version}")
    return record


def reconcile_archive(local, remote, checksum, name, version, commit):
    if sha256(remote) != checksum:
        raise RuntimeError(f"Registry checksum differs for {name} {version}")
    expected = archive_files(local, name, version)
    published = archive_files(remote, name, version)
    for files in (expected, published):
        vcs = json.loads(files.get(".cargo_vcs_info.json", b"{}"))
        if vcs.get("git", {}).get("sha1") != commit or vcs.get("git", {}).get("dirty"):
            raise RuntimeError(f"{name} was not packaged from clean commit {commit}")
        # Cargo may regenerate the lockfile; all shipped source must match.
        files.pop("Cargo.lock", None)
        files.pop(".cargo_vcs_info.json", None)
    if expected != published:
        raise RuntimeError(f"Published {name} source differs from this release")


def reconcile(context, name, archive, commit, record):
    remote = download(f"https://static.crates.io/crates/{name}/{name}-{context.version}.crate")
    reconcile_archive(archive.read_bytes(), remote, record["checksum"], name, context.version, commit)
    context.receipt.setdefault("registry", {})[name] = {"checksum": record["checksum"], "source_commit": commit}
    context.save()
    print(f"{name}: public source and checksum verified", flush=True)


def publish(context, local=False):
    identity = require_clean if local else lambda: require_tag(context.version)
    commit = identity()
    archives = package(context)
    # Check the whole existing set before writing anything. A conflicting
    # published version must never be silently skipped during a resumed run.
    records = {name: registry_record(name, context.version) for name in PACKAGES}
    for name, record in records.items():
        if record:
            reconcile(context, name, archives[name], commit, record)
    for name in PACKAGES:
        if records[name]:
            continue
        if identity() != commit:
            raise RuntimeError("Source identity changed during publication")
        context.run(f"publish-{name}", ["cargo", "publish", "--package", name, "--locked", "--registry", "crates-io"])
        for attempt in range(24):
            record = registry_record(name, context.version)
            if record:
                break
            time.sleep(5)
        else:
            raise RuntimeError(f"{name} upload returned successfully but is not visible; resume the same commit/version")
        reconcile(context, name, archives[name], commit, record)
    consumer(context)


def verify_registry(context):
    commit = require_clean()
    archives = package(context)
    for name in PACKAGES:
        record = registry_record(name, context.version)
        if not record:
            raise RuntimeError(f"{name} {context.version} is not published")
        reconcile(context, name, archives[name], commit, record)
    consumer(context)
