# Third-party components

EmbedScope is MIT licensed. Its dependencies retain their respective licenses.
Binary distributions include `dependency-licenses.json` and the license and
notice files shipped by their resolved Cargo dependencies in `licenses/`.
`Cargo.lock` records the exact source versions used by repository builds.

The main components are:

| Component | Role | License |
| --- | --- | --- |
| [egui / eframe](https://github.com/emilk/egui) | Desktop Inspector | MIT OR Apache-2.0 |
| [Tokio](https://github.com/tokio-rs/tokio) | Host asynchronous runtime | MIT |
| [minicbor](https://github.com/twittner/minicbor) | Protocol CBOR encoding | BlueOak-1.0.0 |
| [Fluent](https://github.com/projectfluent/fluent-rs) | Inspector localization | MIT OR Apache-2.0 |
| [serialport](https://github.com/serialport/serialport-rs) | Serial enumeration and I/O | MPL-2.0 |
| [rs_lvgl](https://github.com/umbrella22/rs_lvgl) | Optional native LVGL adapter | MIT |
| [LVGL](https://github.com/lvgl/lvgl) | Optional native LVGL integration | MIT |

The Inspector reads optional CJK fonts already installed on the system. These
system fonts are not redistributed. Default fonts embedded by egui keep the
notices supplied by `epaint_default_fonts`.
