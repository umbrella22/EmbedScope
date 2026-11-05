# rs_lvgl_ui inspection contract

This independent consumer checks EmbedScope 0.3.0 against the published
`rs_lvgl` / `rs_lvgl_ui` 0.5.0 crates using a single `rs_lvgl_sys` provider.
It mounts an `App::Action` application on a real 320×240 LVGL memory display,
then reads the native tree through `UiBridge` and the binary protocol.

```sh
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --locked -- --test-threads=1
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --features composition --locked -- --test-threads=1
```

The base test uses the core label/button feature set and flat layout modifiers.
It checks widget and layout kinds, native host objects, parent IDs,
padding-derived positions, dimensions, and stale revisions after an update.

The optional `composition` test enables the required component and widget
features. It checks typed constructors, component slots, conditional children,
controlled input after native events, and dialogs mounted on LVGL's top layer.
Screen and overlay trees use separate roots. View keys and component boundaries
are not exported as extra semantic nodes; names remain empty.

The release verifier runs the composition consumer against extracted SDK
archives before publication and against crates.io dependencies after publication.
Both paths require the published rs_lvgl 0.5.0 provider. No window or physical
device is required.
