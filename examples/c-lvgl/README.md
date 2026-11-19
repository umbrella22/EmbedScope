# C application with an LVGL tree

This POSIX reference mounts a settings page on a real LVGL 9.6 memory display
in C. It links a caller-supplied LVGL source tree and the EmbedScope C SDK static
library. It does not link `rs_lvgl` or a second LVGL instance. The SDK wrapper
still requires Rust to produce the host archive.

From the EmbedScope repository root, with CMake, a host C compiler, Rust and
Python 3.9 or later:

```sh
bash scripts/check-c-lvgl.sh /path/to/lvgl-9.6-source
```

The source directory must contain `include/lvgl/lvgl.h` and `src/`. The runner
uses LVGL's internal default configuration with `LV_CONF_SKIP`; it does not fetch
LVGL or use an installed binary provider. It starts a one-client instance on an
ephemeral loopback port, runs the CLI in a single session, checks identity,
native node hierarchy and geometry, capability bits and a delivered log, then
waits for the C process to exit. The report is under `target/validation/c-lvgl/`.

After the build, run `target/validation/c-lvgl/cmake/escope-c-lvgl` without
`--once` to accept sequential loopback connections on port 9340:

```sh
escope -d tcp://127.0.0.1:9340 --json ui tree
escope -d tcp://127.0.0.1:9340 logs --count 1
```

The SDK's callbacks read LVGL coordinates on the application thread. The
application's static tree uses one revision for the lifetime of the page. `tree.c`
encodes only the CBOR payloads for `UiTree`; framing, subscriptions and session
reset use the SDK. The display finishes flushes in memory and discards pixels.
The only capabilities are `ui.tree` and `logs`: requests for taps, screenshots,
metrics and crash summaries are unsupported. This fixture does not demonstrate
firmware adoption or physical panel behavior.
