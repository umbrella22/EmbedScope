#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p escope-lvgl --features native --all-targets -- -D warnings
cargo test --workspace
cargo test -p escope-lvgl --features native -- --test-threads=1
cargo check -p escope-protocol -p escope-agent -p escope-lvgl -p escope-c-api --target riscv32imafc-unknown-none-elf
cargo build -p escope-cli
python3 scripts/check-e2e.py
bash scripts/check-c-api.sh
