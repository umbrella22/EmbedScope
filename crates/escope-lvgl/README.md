# escope-lvgl

UI-thread bridge between an existing LVGL application and [EmbedScope](https://github.com/umbrella22/EmbedScope).

The default `no_std` crate provides `TreeSource`, `UiBridge` and a bounded `NodeRegistry`. It does not compile LVGL. Queue UI requests to the application's UI thread and publish replies after layout and reconciliation have completed.

```toml
[dependencies]
escope-lvgl = "0.6.0"
```

The `native` feature selects `rs_lvgl_sys` 0.8, as used by `rs_lvgl` /
`rs_lvgl_ui` 0.8. Apps using rs_lvgl 0.7 can select `escope-lvgl` 0.5 with
`native`. Apps with their own `TreeSource` can leave `native` disabled.
Cargo allows one `links = "lvgl"` provider. The application and adapter must
resolve the same provider, source and configuration. Native bindings require a
C compiler and libclang. Display capture and input remain platform responsibilities.

`NativeTree<N>` reports native widget kinds such as `label`, `button`, `slider`
and `image`. Plain objects using flex or grid report `row`, `column` or `grid`;
other objects report `lv_obj`. Classification follows the application's effective
LVGL configuration and does not enable additional widgets. A button with its own
flex layout remains a `button`.

The UI `range_slider` reports `slider`; `chart(ChartSpec)` reports one
`chart`. A `custom(widget, props)` subtree reports the LVGL classes the widget
creates, such as `arc`. View keys, chart series IDs and custom Props do not add
virtual nodes or replace the session/revision-scoped node IDs. Flex wrap/reverse
changes final geometry while preserving the native child order.

The tree includes native host objects and widget internals. It does not recover
Rust function names, component boundaries or `View` keys. Names remain empty;
implement an application-owned `TreeSource` to supply text or semantic metadata.
Node IDs remain scoped to a session and revision. Call `changed()` after tree,
layout, property or input-state changes, and serve requests on the LVGL thread.

- [LVGL integration](https://umbrella22.github.io/EmbedScope_website/integration/lvgl)
- [Rust integration](https://umbrella22.github.io/EmbedScope_website/integration/rust)

Rust 1.88 or newer. MIT licensed. LVGL and rs_lvgl retain their own licenses.
