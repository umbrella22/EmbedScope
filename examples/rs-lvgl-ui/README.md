# rs_lvgl_ui inspection contract

This independent consumer checks EmbedScope against the published
`rs_lvgl` / `rs_lvgl_ui` 0.4.0 crates. It mounts an `App::Action` application
with flat layout modifiers on a real 320×240 LVGL memory display, then reads
the tree through `UiBridge` and the binary protocol.

```sh
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --locked -- --test-threads=1
```

The checks cover the single native provider, widget and layout kinds, native
host objects, parent IDs, padding-derived positions, dimensions and stale
revisions after an update. They use the core label/button feature set without
enabling the optional widget catalog. No window or physical device is required.
