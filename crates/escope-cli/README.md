# escope-cli

Command-line interface for [EmbedScope](https://github.com/umbrella22/EmbedScope), an embedded diagnostics and control toolkit. The executable is named **`escope`**.

```sh
cargo install escope-cli --locked
escope --help
escope simulate
```

In another terminal:

```sh
escope -d tcp://127.0.0.1:9337 info
escope -d tcp://127.0.0.1:9337 ui tree
escope -d tcp://127.0.0.1:9337 logs --count 3
escope -d tcp://127.0.0.1:9337 diagnose --output diagnosis --logs 8
```

The simulator uses synthetic data. Physical devices need an integrated EmbedScope Agent. The TCP transport is intended for local development and controlled test networks.

Rust 1.88 or newer. Linux source builds need `pkg-config` and `libudev` development files. Prebuilt CLI binaries and the separate desktop Inspector are distributed through [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases).

- [Quick start](https://umbrella22.github.io/EmbedScope_website/guide/start/quick-start)
- [CLI reference](https://umbrella22.github.io/EmbedScope_website/reference/cli)

MIT licensed.
