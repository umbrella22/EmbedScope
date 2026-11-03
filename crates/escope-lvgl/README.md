# escope-lvgl

UI-thread bridge between an existing LVGL application and [EmbedScope](https://github.com/umbrella22/EmbedScope).

The default `no_std` crate provides `TreeSource`, `UiBridge` and a bounded `NodeRegistry`. It does not compile LVGL. Queue UI requests to the application's UI thread and publish replies after layout and reconciliation have completed.

```toml
[dependencies]
escope-lvgl = "0.1.0"
```

The optional `native` feature uses `rs_lvgl` 0.3.1 to access the application's LVGL instance. The application must use the same LVGL provider and compatible configuration; do not link a second LVGL copy. A C compiler and libclang are required by the native bindings. Display capture and input remain platform integration responsibilities.

- [LVGL integration](https://umbrella22.github.io/EmbedScope_website/integration/lvgl)
- [Rust integration](https://umbrella22.github.io/EmbedScope_website/integration/rust)

Rust 1.88 or newer. MIT licensed. LVGL and rs_lvgl retain their own licenses.
