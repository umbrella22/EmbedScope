# escope-cli

Command-line interface for [EmbedScope](https://github.com/umbrella22/EmbedScope), an embedded diagnostics and control toolkit. The executable is named **`escope`**.

## Install

Download the CLI ZIP for your operating system and architecture from [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases/latest). Extract it and run `./escope --version`, or `.\escope.exe --version` on Windows. Add its directory to `PATH` to use `escope` from any terminal. The prebuilt CLI does not require Rust.

For an existing Rust environment, Cargo is an alternative:

```sh
cargo install escope-cli --locked
```

Source builds require Rust 1.88 or newer. Linux source builds also need `pkg-config` and `libudev` development files.

## First session

```sh
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

- [Quick start](https://umbrella22.github.io/EmbedScope_website/guide/start/quick-start)
- [CLI reference](https://umbrella22.github.io/EmbedScope_website/reference/cli)
- [Desktop Inspector](https://umbrella22.github.io/EmbedScope_website/guide/basic/inspector)

MIT licensed.
