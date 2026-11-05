# Desktop Inspector development

The Inspector uses eframe/egui 0.37 and requires Rust 1.95 or newer. CLI and device-side packages retain their own Rust 1.88 minimum. Build from the workspace root:

```sh
cargo run -p escope-desktop -- tcp://127.0.0.1:9337
cargo test -p escope-desktop
```

## Source map

| Path | Responsibility |
| --- | --- |
| `src/app.rs` | Session state, incoming updates, page dispatch and saved preferences |
| `src/app/chrome.rs` | Navigation, language menu, connection controls and status |
| `src/app/inspector.rs` | Object tree, attributes, display selection, input and PNG controls |
| `src/app/tree.rs` | Hierarchy order, ancestor-preserving search and paint-order selection |
| `src/app/diagnostics.rs` | Logs, metrics and device pages |
| `src/i18n.rs` | Locale registry, system-language matching, Fluent formatting and fallback |
| `locales/*.ftl` | Embedded interface translations |
| `src/worker.rs` | Device connection and protocol work on a separate thread |
| `src/png_save.rs` | Background export of a retained capture |
| `src/fonts.rs` | AppImage or installed-system CJK font fallback |
| `src/theme.rs`, `src/icons.rs` | Shared native visual primitives |
| `src/brand.rs`, `assets/brand/` | Embedded brand mark, editable SVG and platform icon assets |

## Control layout

`theme::menu_style` restores menu padding after egui applies its compact popup
defaults. Apply it to both `Popup::menu` and `ComboBox` popups. Shared text fields
use `theme::text_field`; icon/text buttons and selectable menu rows use a
40-point control height with 12-point horizontal insets. Read-only status rows
keep their own compact spacing.

The capture export panel measures its controls and wrapped status independently
of the display canvas. Avoid a fixed footer reservation when changing this
area. Layout regressions in `src/app/tests/layout.rs` cover translated controls,
menu insets, full-row pointer selection and long object IDs; capture geometry
tests also verify that export/status labels stay inside their clips.

## Language selection

The globe menu offers **Follow system**, **English**, and **简体中文**. System mode matches the first supported language in the operating system's preference list and falls back to English. Manual selection overrides the system language. Switching language changes interface copy and documentation links while preserving the connection, node selection, capture, filters and background export.

The eframe application storage records `embedscope.locale` as `system` or a locale ID, such as `zh-CN`. It saves during normal shutdown and periodic autosave. An unknown saved ID restores system mode. The stable application ID is `io.github.umbrella22.embedscope.inspector`.

Fluent bundles are created once per Inspector. Message or formatting failures fall back to English; an unknown English key displays its message ID. Catalog syntax errors and duplicate IDs fail the catalog tests. No locale state crosses into the host SDK or device worker.

## Add a language

1. Copy `locales/en-US.ftl` to a BCP 47 language file, for example `locales/ja-JP.ftl`. Translate message values, retaining message IDs and named parameters.
2. Add one `Locale` entry to `LOCALES` in `src/i18n.rs`: its language ID, native language name, embedded source and documentation prefix. English remains the first entry and fallback. The documentation prefix must point to a deployed locale; use `en/` if translated documentation is unavailable.
3. Run `cargo test -p escope-desktop`. Catalog checks compare IDs and argument sets, reject syntax errors and duplicates, format every message, and verify literal UI references. UI tests iterate over registered languages at 900 × 620 and 1380 × 900.
4. Inspect all four pages and the language menu in a native window. Confirm installed font coverage and translated controls at the minimum window size. A right-to-left language additionally requires reviewing layout direction and bidirectional isolation before support is declared.

No language-specific branches belong in views. The registry provides the menu options and locale matching metadata.

## Add or change copy

Use stable semantic message IDs and named arguments:

```ftl
frame-number = Frame { $id }
```

```rust,ignore
tr!(self.i18n, "frame-number", id = meta.frame_id.to_string())
```

Translate the entire message in every catalog. Avoid assembling sentences from separately translated fragments. Fluent supports selectors and plural forms when a message needs them. Pass numeric quantities as numbers only when their range is exactly representable; Fluent numbers use `f64`. Pass `u64` identifiers and exact counters as decimal strings to preserve all digits.

Device names, object kinds and names, log targets and messages, firmware/build IDs, endpoints, file paths and original error details remain literal. Only their surrounding interface labels are translated. Log severity codes remain `ERROR`, `WARN`, `INFO`, `DEBUG`, and `TRACE`; the severity filter's descriptions are localized.

The Linux AppImage includes Noto Sans CJK and its license. Windows and macOS use installed system CJK fonts. The application does not download fonts; other source-build environments need an installed fallback for Chinese glyphs. Font rendering is verified separately from catalog correctness.
