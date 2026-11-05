# escope-agent

Allocation-free, `no_std` device Agent for [EmbedScope](https://github.com/umbrella22/EmbedScope).

Implement `Backend` for device identity and the capabilities supported by the firmware. Feed decoded requests to `Agent`, transmit its bounded replies, and call `Agent::reset` when a connection ends. The application owns byte transport, scheduling, the UI thread and all hardware resources. The Agent creates no tasks or allocator.

```toml
[dependencies]
escope-agent = "0.2.0"
escope-protocol = "0.2.0"
```

The packaged `poll_device` example demonstrates the transport loop on a host:

```sh
cargo run --example poll_device
```

- [Rust integration](https://umbrella22.github.io/EmbedScope_website/integration/rust)
- [Agent lifecycle](https://umbrella22.github.io/EmbedScope_website/development/architecture)

Rust 1.88 or newer. MIT licensed.
