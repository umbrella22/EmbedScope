"""Portable C SDK source kit with a target-owned Rust staticlib wrapper."""

from pathlib import Path
import re
import shutil
import shlex
import tempfile

from .binaries import archive_directory, extract_zip
from .common import DEVICE_PACKAGES, ROOT
from .packages import archive_files


def c_sdk(context, archives):
    with tempfile.TemporaryDirectory(prefix="embedscope-c-sdk-") as temporary:
        stage = Path(temporary) / f"embedscope-c-sdk-{context.version}"
        stage.mkdir()
        for name in DEVICE_PACKAGES:
            if name == "escope-lvgl":
                continue
            files = archive_files(archives[name].read_bytes(), name, context.version)
            for relative, data in files.items():
                path = stage / "vendor" / name / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
        (stage / "include").mkdir()
        shutil.copy2(stage / "vendor/escope-c-api/include/escope.h", stage / "include/escope.h")
        shutil.copy2(ROOT / "LICENSE", stage / "LICENSE")
        (stage / "src").mkdir()
        (stage / "src/lib.rs").write_text('''#![cfg_attr(not(feature = "std"), no_std)]

pub use escope_c_api::*;

// Standalone C firmware fallback. Replace with the board's reset/watchdog
// policy, or remove it when another Rust runtime supplies the panic handler.
#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
''')
        (stage / "Cargo.toml").write_text(f'''[package]
name = "escope-c-sdk"
version = "{context.version}"
edition = "2021"
rust-version = "1.88"
publish = false

[workspace]

[lib]
crate-type = ["staticlib"]

[features]
default = []
std = []

[dependencies]
escope-c-api = "={context.version}"

[patch.crates-io]
escope-protocol = {{ path = "vendor/escope-protocol" }}
escope-agent = {{ path = "vendor/escope-agent" }}
escope-c-api = {{ path = "vendor/escope-c-api" }}

[profile.release]
panic = "abort"
codegen-units = 1
''')
        (stage / "README.md").write_text(f'''# EmbedScope C SDK {context.version}

The ZIP contains the C header, three EmbedScope Rust source packages, a locked
dependency graph and a static-library wrapper. Cargo fetches third-party
dependencies when building for the firmware target.

Install Rust 1.88 or newer and the Rust target supported by the board toolchain:

```sh
rustup target add riscv32imafc-unknown-none-elf
cargo build --release --locked --target riscv32imafc-unknown-none-elf
```

The output is `target/<target>/release/libescope_c_sdk.a`. Match the target triple,
CPU flags, ABI, linker script and runtime to the C/C++ firmware. ESP Xtensa needs
the Espressif Rust toolchain instead of a generic rustup target. Include
`include/escope.h` and link the library into the firmware; the application owns
transport, scheduling, callbacks and memory. See `vendor/escope-c-api/README.md`
for the lifecycle contract.

The wrapper's default panic handler spins forever. Replace it with the board's
reset/watchdog policy. Keep exactly one Rust panic handler and the board's linker
integration. `--features std` is for host verification only.

A custom wrapper can also depend on `escope-c-api = "={context.version}"` from
crates.io. The included wrapper uses the SDK sources under `vendor/`.
Keep the C header and Rust SDK on the same release version. Package versions,
wire protocol major, and target ABI have separate compatibility requirements.

Source commit: `{context.receipt['commit']}`.
MIT licensed; third-party dependencies retain their licenses.
''')
        manifest = stage / "Cargo.toml"
        context.run("c-sdk-host-build", ["cargo", "build", "--release", "--features", "std", "--manifest-path", manifest])
        context.run("c-sdk-native-libs", ["cargo", "rustc", "--release", "--locked", "--features", "std",
            "--manifest-path", manifest, "--lib", "--", "--print", "native-static-libs"])
        log = (context.out / f"{context.name}-c-sdk-native-libs.log").read_text()
        matches = re.findall(r"native-static-libs: (.+)", log)
        if not matches:
            raise RuntimeError("Rust did not report C SDK native link dependencies")
        harness = context.out / "c-sdk-harness"
        context.run("c-sdk-c-link", ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", stage / "include",
            ROOT / "examples/c/harness.c", context.target / "release/libescope_c_sdk.a", *shlex.split(matches[-1]), "-o", harness])
        context.run("c-sdk-c-harness", [harness])
        destination = context.out / f"embedscope-c-sdk-{context.version}.zip"
        archive_directory(stage, destination)
        context.record(destination)
        extracted = Path(temporary) / "unpacked"
        extract_zip(destination, extracted)
        context.run("c-sdk-no-std", ["cargo", "build", "--release", "--locked", "--manifest-path",
            extracted / stage.name / "Cargo.toml", "--target", "riscv32imafc-unknown-none-elf"])
