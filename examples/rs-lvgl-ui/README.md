# rs_lvgl_ui inspection contract

This independent consumer checks EmbedScope 0.4.0 against the published
`rs_lvgl` / `rs_lvgl_ui` 0.6.0 crates using a single `rs_lvgl_sys` provider.
It mounts an `App::Action` application on a real 320×240 LVGL memory display,
then reads the native tree through `UiBridge` and the binary protocol.

```sh
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --locked -- --test-threads=1
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --features composition --locked -- --test-threads=1
cargo test --manifest-path examples/rs-lvgl-ui/Cargo.toml --target-dir target --features api-0-6 --locked -- --test-threads=1
```

The base test uses the core label/button feature set and flat layout modifiers.
It checks widget and layout kinds, native host objects, parent IDs,
padding-derived positions, dimensions, and stale revisions after an update.

The rs_lvgl_ui 0.6 constructor names do not rename native Inspector kinds:
`container()` produces `lv_obj` (or a layout kind), `progress_bar(...)` produces
`bar`, `message_box(...)` produces `msgbox`, and `animated_image(...)` produces
`animimage`. Low-level rs_lvgl/C names and the `widget-bar`, `widget-msgbox`, and
`widget-animimage` feature names are unchanged. Application style, unit, and part
migrations follow the [rs_lvgl_ui 0.6 migration guide](https://umbrella22.github.io/rs_lvgl_website/en/guide/ui-api-migration.html).

The optional `composition` test enables the required component and widget
features. It checks typed constructors, component slots, conditional children,
controlled input after native events, and dialogs mounted on LVGL's top layer.
Screen and overlay trees use separate roots. View keys and component boundaries
are not exported as extra semantic nodes; names remain empty.

The `api-0-6` feature includes `composition` and enables the renamed widgets.
Its fixture checks `Style::plain`, independent Flex alignment fields, and
`thumb_style` / `progress_style` with the native tree. A real LVGL Pointer
press/release delivers one application action, changes progress from 25 to 75,
and updates the RGB display pixels. Native kinds and geometry remain stable,
and the previous tree revision is rejected. The fixture supplies display and
input; `NativeTree` reads the resulting objects.

The release verifier runs the `api-0-6` consumer against extracted SDK
archives before publication and against crates.io dependencies after publication.
Both paths require the published rs_lvgl 0.6.0 provider. No window or physical
device is required.
