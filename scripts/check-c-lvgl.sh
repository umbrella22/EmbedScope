#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 1 || ! -f "$1/include/lvgl/lvgl.h" ]]; then
    printf 'Usage: %s /path/to/lvgl-9.6-source\n' "$0" >&2
    exit 2
fi
lvgl_source=$(cd "$1" && pwd)
cd "$(dirname "$0")/.."
repo_root=$PWD
artifact_dir="$repo_root/target/validation/c-lvgl"
host=$(rustc -vV | sed -n 's/^host: //p')
[[ -n "$host" ]]
mkdir -p "$artifact_dir"

# Use the same host C SDK wrapper and linker-flag discovery as check-c-api.sh.
cargo build --locked --manifest-path examples/c/Cargo.toml \
    --target "$host" --target-dir "$artifact_dir/rust"
cargo rustc --color never --locked --manifest-path examples/c/Cargo.toml \
    --target "$host" --target-dir "$artifact_dir/rust" --lib -- --print native-static-libs \
    >"$artifact_dir/native-libs.log" 2>&1 || { cat "$artifact_dir/native-libs.log" >&2; exit 1; }
native_libs=$(sed -n 's/^note: native-static-libs: //p' "$artifact_dir/native-libs.log" | tail -n 1)
if [[ -z "$native_libs" ]]; then
    printf '%s\n' 'Rust did not report native static link dependencies' >&2
    exit 1
fi

cmake -S examples/c-lvgl -B "$artifact_dir/cmake" \
    -DLVGL_SOURCE="$lvgl_source" \
    -DESCOPE_LIBRARY="$artifact_dir/rust/$host/debug/libescope_c_example.a" \
    -DESCOPE_INCLUDE="$repo_root/crates/escope-c-api/include" \
    -DESCOPE_NATIVE_LIBS="$native_libs"
cmake --build "$artifact_dir/cmake" --parallel "${CMAKE_BUILD_PARALLEL_LEVEL:-4}"
cargo build --locked -p escope-cli --target "$host" --target-dir "$artifact_dir/rust"

python3 - "$artifact_dir" "$artifact_dir/rust/$host/debug/escope" <<'PY'
import json
import select
import subprocess
import sys
import tempfile
from pathlib import Path

root = Path(sys.argv[1])
report_dir = Path(tempfile.mkdtemp(prefix="session-", dir=root)) / "diagnosis"
with subprocess.Popen([root / "cmake/escope-c-lvgl", "--once"], stdout=subprocess.PIPE, text=True) as device:
    try:
        if not select.select([device.stdout], [], [], 5)[0]:
            raise RuntimeError("C LVGL process did not open its listener")
        endpoint = device.stdout.readline().strip().removeprefix("LISTEN ")
        if not endpoint.startswith("tcp://127.0.0.1:"):
            raise RuntimeError(f"Unexpected startup output: {endpoint!r}")
        subprocess.run(
            [sys.argv[2], "-d", endpoint, "--json", "diagnose", "--logs", "1", "--output", report_dir],
            check=True, timeout=15,
        )
        if device.wait(timeout=5) != 0:
            raise RuntimeError("C LVGL teardown failed")
    finally:
        if device.poll() is None:
            device.kill()
            device.wait()

report = json.loads((report_dir / "report.json").read_text())
assert report["complete"] and report["issues"] == [], report["issues"]
assert report["info"]["model"] == "LVGL 9.6 / C host"
assert report["info"]["build_id"] == "c-lvgl-example" and report["info"]["boot_id"] > 0
assert report["capabilities"] == ["logs", "ui.tree"] and report["hello"]["capabilities"] == 5
assert "display" not in report and "metrics" not in report and "crash" not in report
nodes = report["ui_tree"]
assert [(n["id"], n["parent"], n["kind"], n["name"]) for n in nodes] == [
    (1, 0, "screen", "settings"),
    (2, 1, "label", "settings.title"),
    (3, 1, "button", "settings.close"),
    (4, 3, "label", "settings.close-label"),
]
assert all(n["revision"] == 1 and n["width"] > 0 and n["height"] > 0 for n in nodes)
assert tuple(nodes[0][k] for k in ("x", "y", "width", "height")) == (0, 0, 320, 240)
assert tuple(nodes[2][k] for k in ("x", "y", "width", "height")) == (20, 60, 140, 40)
log, = report["logs"]
assert log["session"] == report["hello"]["session"]
assert log["target"] == "c-lvgl" and log["message"] == "LVGL tree ready" and not log["truncated"]
print(f"C LVGL tree, capabilities, logs, and teardown verified: {report_dir}")
PY
