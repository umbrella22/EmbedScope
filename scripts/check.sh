#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p escope-lvgl --features native --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p escope-lvgl --features native --locked -- --test-threads=1
cargo check -p escope-protocol -p escope-agent -p escope-lvgl -p escope-c-api --target riscv32imafc-unknown-none-elf --locked
cargo build -p escope-cli --locked
python3 scripts/check-e2e.py
bash scripts/check-c-api.sh
