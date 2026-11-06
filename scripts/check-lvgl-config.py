#!/usr/bin/env python3
"""Run native tree inspection against reduced application LVGL configurations."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target" / "validation"
OUTPUT.mkdir(parents=True, exist_ok=True)
WORK = Path(tempfile.mkdtemp(prefix="lvgl-config-", dir=OUTPUT))


def run(args, name, env=None):
    result = subprocess.run(args, cwd=ROOT, env=env, text=True, capture_output=True)
    (WORK / f"{name}.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(f"{name} failed ({result.returncode}): {WORK / (name + '.log')}")
    return result.stdout


metadata = json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1", "--features", "escope-lvgl/native"], "metadata"))
provider = next(p for p in metadata["packages"] if p["name"] == "rs_lvgl_sys")
native = Path(provider["manifest_path"]).parent
base = native / "vendor" / "include-9.6" / "lv_conf.h"
widgets = re.findall(r'widget\(\s*"[^"]+"\s*,\s*"([^"]+)"', (native / "build.rs").read_text())
if not widgets or provider["version"] != "0.6.0":
    raise RuntimeError("expected the inspected rs_lvgl_sys 0.6.0 configuration contract")

consumer = WORK / "consumer"
(consumer / "src").mkdir(parents=True)
(consumer / "Cargo.toml").write_text(f"""[package]
name = "escope-native-config-contract"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[features]
grid = []
flex = []

[dependencies]
escope-lvgl = {{ path = {json.dumps(str(ROOT / 'crates/escope-lvgl'))}, features = ["native"] }}
rs_lvgl_sys = "=0.6.0"
""")
(consumer / "src" / "main.rs").write_text("""use core::ptr::NonNull;
use escope_lvgl::{native::NativeTree, TreeSource};
use rs_lvgl_sys as sys;

fn main() {
    // This process exclusively owns LVGL and all objects remain live during reads.
    unsafe {
        sys::lv_init();
        let display = NonNull::new(sys::lv_display_create(64, 64)).unwrap();
        let root = NonNull::new(sys::lv_screen_active()).unwrap();
        let object = sys::lv_obj_create(root.as_ptr());
        #[cfg(feature = "grid")]
        sys::lv_obj_set_style_layout(object, sys::lv_layout_t_LV_LAYOUT_GRID as u16, 0);
        #[cfg(feature = "flex")]
        sys::lv_obj_set_flex_flow(object, sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN);
        let label = sys::lv_label_create(object);
        sys::lv_label_set_text(label, c"Native consumer".as_ptr());
        sys::lv_button_create(object);
        {
            let mut tree = NativeTree::<8>::new(root);
            let kind = if cfg!(feature = "grid") { "grid" }
                       else if cfg!(feature = "flex") { "column" }
                       else { "lv_obj" };
            assert_eq!(tree.read_node(1).unwrap().kind, kind);
            assert_eq!(tree.read_node(2).unwrap().kind, "label");
            assert_eq!(tree.read_node(3).unwrap().kind, "button");
        }
        sys::lv_display_delete(display.as_ptr());
        sys::lv_deinit();
    }
    println!("native-config-ok");
}
""")
run(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(consumer / "Cargo.toml")], "lock")

cases = []
for case, flex, grid in [("no-layout", 0, 0), ("grid-only", 0, 1), ("flex-only", 1, 0)]:
    header = WORK / f"{case}.h"
    lines = [f"#include {json.dumps(str(base))}"]
    for option in widgets:
        lines.extend([f"#undef LV_USE_{option}", f"#define LV_USE_{option} 0"])
    for option, enabled in [("FLEX", flex), ("GRID", grid)]:
        lines.extend([f"#undef LV_USE_{option}", f"#define LV_USE_{option} {enabled}"])
    header.write_text("\n".join(lines) + "\n")
    args = ["cargo", "run", "--locked", "--manifest-path", str(consumer / "Cargo.toml"), "--target-dir", str(ROOT / "target")]
    if grid or flex:
        args.extend(["--features", "grid" if grid else "flex"])
    env = os.environ.copy()
    env["LVGL_CONFIG_PATH"] = str(header)
    output = run(args, case, env)
    if "native-config-ok" not in output:
        raise RuntimeError(f"{case} did not complete native assertions")
    cases.append({"case": case, "flex": flex, "grid": grid, "header_sha256": hashlib.sha256(header.read_bytes()).hexdigest(), "status": "passed"})

receipt = WORK / "receipt.json"
receipt.write_text(json.dumps({"schema": 1, "provider": {"name": provider["name"], "version": provider["version"], "source": provider["source"]}, "cases": cases}, indent=2) + "\n")
print(f"LVGL application configurations: {len(cases)} passed; {receipt}")
