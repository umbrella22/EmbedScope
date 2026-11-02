#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
example_manifest="$repo_root/examples/c/Cargo.toml"
artifact_dir="$repo_root/target/validation/c-api"
mkdir -p "$artifact_dir"

cargo build --manifest-path "$example_manifest" --target-dir "$artifact_dir/rust"
cargo rustc --manifest-path "$example_manifest" --target-dir "$artifact_dir/rust" --lib -- --print native-static-libs >"$artifact_dir/native-libs.log" 2>&1
native_libs=$(sed -n 's/^note: native-static-libs: //p' "$artifact_dir/native-libs.log" | tail -n 1)
if [ -z "$native_libs" ]; then
  printf '%s\n' 'Rust did not report native static link dependencies' >&2
  exit 1
fi

# rustc reports trusted linker arguments, split into individual command args.
# shellcheck disable=SC2086
${CC:-cc} -std=c11 -Wall -Wextra -Werror \
  -I"$repo_root/crates/escope-c-api/include" \
  "$repo_root/examples/c/harness.c" \
  "$artifact_dir/rust/debug/libescope_c_example.a" \
  $native_libs -o "$artifact_dir/harness"
"$artifact_dir/harness" >"$artifact_dir/harness.log"
cat "$artifact_dir/harness.log"
