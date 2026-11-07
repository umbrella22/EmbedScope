# escope-protocol

Allocation-free, `no_std` wire protocol for [EmbedScope](https://github.com/umbrella22/EmbedScope), an embedded diagnostics and control toolkit.

The crate provides COBS framing, CRC validation, a bounded frame decoder, and typed CBOR payloads for device identity, logs, metrics, UI inspection, display capture and input. It has no HAL, executor or allocator dependency.

```toml
[dependencies]
escope-protocol = "0.5.0"
```

Applications normally pair this crate with `escope-agent` on the device or `escope-host` on the host. Crate versions and the wire protocol major are separate: protocol compatibility is negotiated during Hello.

- [Protocol reference](https://umbrella22.github.io/EmbedScope_website/reference/protocol)
- [Rust integration](https://umbrella22.github.io/EmbedScope_website/integration/rust)

Rust 1.88 or newer. MIT licensed.
