#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo fmt --manifest-path examples/rs-lvgl-ui/Cargo.toml -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p escope-lvgl --features native --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p escope-lvgl --features native --locked -- --test-threads=1
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --locked -- --test-threads=1
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --features composition --locked -- --test-threads=1
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --features api-0-6 --locked -- --test-threads=1
python3 scripts/check-lvgl-config.py
cargo check -p escope-protocol -p escope-agent -p escope-lvgl -p escope-c-api --target riscv32imafc-unknown-none-elf --locked
cargo build -p escope-cli --locked
python3 scripts/check-e2e.py
bash scripts/check-c-api.sh
