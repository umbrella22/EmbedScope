# escope-lvgl

UI-thread bridge between an existing LVGL application and [EmbedScope](https://github.com/umbrella22/EmbedScope).

The default `no_std` crate provides `TreeSource`, `UiBridge` and a bounded `NodeRegistry`. It does not compile LVGL. Queue UI requests to the application's UI thread and publish replies after layout and reconciliation have completed.

```toml
[dependencies]
escope-lvgl = "0.3.0"
```

The optional `native` feature uses the `rs_lvgl_sys` 0.5.0 provider shared by
`rs_lvgl` / `rs_lvgl_ui` 0.5.0 to access the application's LVGL instance. The
application must use the same provider and compatible configuration; do not link
a second LVGL copy. A C compiler and libclang are required by the native bindings.
Display capture and input remain platform integration responsibilities.

`NativeTree<N>` reports native widget kinds such as `label`, `button`, `slider`
and `image`. Plain objects using flex or grid report `row`, `column` or `grid`;
other objects report `lv_obj`. Classification follows the application's effective
LVGL configuration and does not enable additional widgets. A button with its own
flex layout remains a `button`.

The tree includes native host objects and widget internals. It does not recover
Rust function names, component boundaries or `View` keys. Names remain empty;
implement an application-owned `TreeSource` to supply text or semantic metadata.
Node IDs remain scoped to a session and revision. Call `changed()` after tree,
layout, property or input-state changes, and serve requests on the LVGL thread.

- [LVGL integration](https://umbrella22.github.io/EmbedScope_website/integration/lvgl)
- [Rust integration](https://umbrella22.github.io/EmbedScope_website/integration/rust)

Rust 1.88 or newer. MIT licensed. LVGL and rs_lvgl retain their own licenses.
