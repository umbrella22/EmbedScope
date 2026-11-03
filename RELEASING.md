# 发布 EmbedScope

EmbedScope 的六个 SDK/CLI crate 共享版本号。Inspector 使用相同产品版本，以平台二进制分发。C SDK 源码包携带同版本头文件、设备端 Rust 源码和静态库包装工程。

| 分发单元 | 发布位置 | 使用方式 |
| --- | --- | --- |
| `escope-protocol`、`escope-agent` | crates.io | Rust 固件依赖 |
| `escope-lvgl` | crates.io | 可选 UI 线程桥，`native` 接入宿主的 rs_lvgl |
| `escope-c-api` | crates.io | C ABI 的 Rust 实现，用目标工具链生成最终静态库 |
| `escope-host` | crates.io | Rust 主机 SDK |
| `escope-cli` | crates.io、GitHub Releases | `cargo install escope-cli --locked` 或下载 `escope` |
| Inspector | GitHub Releases | 原生应用或可执行文件 |
| C SDK | GitHub Releases | 解压源码包，针对固件目标交叉编译 |
| skills、文档 | Git 仓库、文档站 | 不作为 crate 发布 |

## 已发布版本

| 版本 | SDK 源码提交 | crates.io |
| --- | --- | --- |
| 0.1.0 | [`0bc53fc82401dd1f8253dbb8787f9360887008d8`](https://github.com/umbrella22/EmbedScope/commit/0bc53fc82401dd1f8253dbb8787f9360887008d8) | 六个 crate 均已发布，公开归档与源码提交一致，registry 消费验证通过 |

公开 crate 的 `.cargo_vcs_info.json` 记录 SDK 源码提交。后续文档提交不改变发布身份；恢复发布或重新校验已发布版本时，检出表中的源码提交。

## 本地验证与打包

需要 Rust 1.99.1、Python 3.11+、C 编译器、libclang 和裸机 RISC-V target。设备 SDK 与 CLI 的最低版本分别通过 Rust 1.88 检查，Inspector 通过 Rust 1.95 检查。Linux 的完整构建依赖见 `.github/actions/setup-build/action.yml`。

完整源码包验证在 Linux/macOS 执行，C harness 使用 POSIX C 环境。Windows 运行主机测试、CLI 闭环与原生二进制打包。

```sh
rustup target add riscv32imafc-unknown-none-elf
bash scripts/check.sh
python3 -m unittest discover -s scripts/tests -v
python3 scripts/release.py verify
python3 scripts/release.py binaries
python3 scripts/release.py checksums
```

`verify` 对六个 `.crate` 执行 Cargo 打包构建，再在仓库之外提取源码包，运行 SDK 消费工程、安装 CLI 并执行软件参考设备闭环。C SDK 使用生成的包装工程运行真实 C harness，并构建裸机 RISC-V 静态库。`binaries` 使用 release profile，在本机平台构建 CLI 和 Inspector，对 CLI 分发归档解包后再次运行闭环。

产物位于 `target/dist/`，包括源码包、C SDK、平台归档、SHA256SUMS、构建记录和日志。`--artifacts <目录>` 可以指定新的输出目录。开发验证允许 `verify --allow-dirty` 和 `binaries --allow-dirty`，记录中的 `dirty` 为 true；公开发布要求干净提交。

二进制支持 macOS arm64、macOS x64、Linux x64 GNU 和 Windows x64 MSVC，必须在对应本机环境构建和执行。macOS 归档包含 ad hoc 签名的 `.app`，尚未进行 Apple 开发者签名和公证。Linux 以 Ubuntu 24.04 为构建基线，需要兼容的 glibc；Inspector 运行还需要桌面会话与 OpenGL。平台测试覆盖主机逻辑及 CLI，原生窗口的人工验收需单独执行。

## 从本机发布 crates.io

首次发布六个名称时，可以使用本机 Cargo 凭据。凭据通过 Cargo 的标准 credential provider 或 `CARGO_REGISTRY_TOKEN` 提供，不写入仓库和命令参数。

```sh
git status --short
python3 scripts/release.py verify
python3 scripts/release.py publish --local
```

发布工具按 `protocol → agent → lvgl → c-api → host → cli` 顺序上传。每个版本可见后，下载公开 `.crate`，校验 registry checksum、源码和 `.cargo_vcs_info.json` 中的提交，再运行仅依赖 crates.io 的消费验证。

同一提交的重试会先校验全部已有版本，再跳过已确认的包。遇到已发布内容冲突立即终止。上传已接受但可见性超时，应检查该版本并重试同一提交；不要仅为重试而递增版本或移动已发布的标签。

## GitHub Actions

`ci.yml` 在 master push、PR 和手动运行时执行 Linux 完整检查、源码包独立消费、macOS/Windows 主机测试与 CLI 闭环，以及 Rust 1.88/1.95 的最低版本检查。

`release.yml` 由 `vMAJOR.MINOR.PATCH` 标签或手动指定已有标签启动：

1. 确认标签版本与 workspace 一致，远程标签指向当前提交，该提交位于远程 master 历史中。
2. 运行完整源码验证，四个平台分别测试并构建 CLI、Inspector。
3. 启用 Trusted Publishing 时，获取短期 crates.io token 并发布六个包。
4. 校验 crates.io 的公开包和 registry 消费工程。未启用 Trusted Publishing 时，这一步校验此前从本机发布的相同提交。
5. 校验各平台产物记录、源码提交及 SHA-256；创建草稿 Release、上传并下载比对全部资产，最后公开 Release。

源码验证、平台构建或 registry 校验失败时，不公开 GitHub Release。平台产物和诊断记录保留为 workflow artifacts。工作流权限按 job 分配；只有发布 crate 的 job 拥有 `id-token: write`，只有 GitHub Release job 拥有 `contents: write`。

## 配置 Trusted Publishing

在六个 crate 的 crates.io 设置中分别添加 GitHub Trusted Publisher：

| 字段 | 值 |
| --- | --- |
| GitHub owner | `umbrella22` |
| Repository | `EmbedScope` |
| Workflow filename | `release.yml` |
| GitHub environment | 留空，当前发布 job 不指定 environment |

配置全部完成后，在 GitHub 仓库 **Settings → Secrets and variables → Actions → Variables** 增加 repository variable：

```text
CRATES_IO_TRUSTED_PUBLISHING=true
```

未设置或值不是 `true` 时，工作流跳过上传，仍要求同一提交的六个 crate 已在 crates.io 通过校验。无需配置长期 Cargo secret。认证由 `rust-lang/crates-io-auth-action` 完成，job 结束时撤销短期 token。

0.1.0 已在本机完成首次发布，可以为六个 crate 配置上述 Trusted Publisher。需要发布 0.1.0 的二进制时，从对应的 SDK 源码提交创建标签：

```sh
git tag -a v0.1.0 0bc53fc82401dd1f8253dbb8787f9360887008d8 -m "EmbedScope 0.1.0"
git push origin v0.1.0
```

后续版本在完成版本修改、验证与提交后，为对应源码提交创建版本标签。标签版本号和 workspace 版本必须一致。手动恢复使用 Release 工作流的 `release_tag` 输入；保留原始标签，不覆盖已发布的公共资产。

## C/C++ 目标产物

C SDK 源码包无需克隆完整仓库。包装工程默认是 `no_std` 静态库，Cargo 使用包内的 EmbedScope 源码，只从 registry 获取第三方依赖。目标 triple、CPU、ABI、panic/runtime 和链接脚本由固件确定；ESP Xtensa 等目标使用相应厂商 Rust 工具链。

```sh
cargo build --release --locked --target riscv32imafc-unknown-none-elf
```

输出 `target/<target>/release/libescope_c_sdk.a`，C/C++ 工程包含 `include/escope.h` 并链接该静态库。包装工程附带的 panic handler 持续自旋，集成时替换为板级复位/watchdog 策略；整个固件只保留一份 Rust panic handler。

crates.io 发布后也可以只创建自己的 Rust staticlib 包装工程，依赖 `escope-c-api = "=0.1.0"`，无需使用 vendored SDK。C ABI 调用方匹配头文件与构建产物；内部 Rust 依赖升级后需要重新构建目标库，协议兼容性则由 Hello 协商的协议主版本判断。
