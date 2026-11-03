# escope-host

Rust host SDK for [EmbedScope](https://github.com/umbrella22/EmbedScope). The CLI and desktop Inspector use this SDK for TCP and serial connections.

`Session` multiplexes bounded requests and events, negotiates device capabilities and manages connection shutdown. The SDK exposes device information, logs, metrics, UI trees, display capture and input. A software reference device is included for integration tests; its data is synthetic.

```toml
[dependencies]
escope-host = "0.1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Linux builds require `pkg-config` and the `libudev` development package for serial enumeration. The device-side SDK crates do not require these host libraries.

- [Host SDK reference](https://umbrella22.github.io/EmbedScope_website/reference/sdk)
- [Project and examples](https://github.com/umbrella22/EmbedScope)

Rust 1.88 or newer. MIT licensed.
