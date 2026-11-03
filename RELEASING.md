# 发布 EmbedScope

六个 SDK/CLI crate、Inspector 和 C SDK 使用同一个产品版本。

| 分发单元 | 发布位置 | 产物 |
| --- | --- | --- |
| Rust 设备与主机 SDK | crates.io | `escope-protocol`、`escope-agent`、`escope-lvgl`、`escope-c-api`、`escope-host` |
| CLI | GitHub Releases、crates.io | 各平台 ZIP；`escope-cli` 为 Rust 用户提供可选安装方式 |
| Inspector | GitHub Releases | Linux x64 AppImage、Windows x64 ZIP、macOS arm64/x64 `.app` ZIP |
| C/C++ SDK | GitHub Releases | 头文件、设备 SDK 源码、锁文件与 staticlib 包装工程的 ZIP |
| 文档与 skills | Git 仓库、文档站 | 指南、API 参考、接入与调试技能 |

## 安装分发包

从 [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases/latest) 选择系统与 CPU 架构对应的文件。

- CLI：解压 `escope-VERSION-TARGET.zip`，直接运行 `escope --help`。Windows 使用 `escope.exe`。将可执行文件所在目录加入 `PATH` 后，可以在任意终端使用 `escope`。
- Inspector：Windows/macOS 解压后启动应用；Linux 对 `.AppImage` 执行 `chmod +x` 后运行。
- C/C++：解压 `embedscope-c-sdk-VERSION.zip`，按目标芯片构建静态库，使用同包的 `include/escope.h`。

使用 CLI 与 Inspector 分发包不需要安装 Rust。Linux 二进制要求 glibc 2.35+，CLI 使用系统 `libudev`，Inspector 使用系统 OpenGL 驱动。AppImage 内含显示协议依赖和 CJK 字体；无 FUSE 的系统可使用 `--appimage-extract` 后运行 `squashfs-root/AppRun`。Windows 使用静态 MSVC runtime。macOS `.app` 使用 ad hoc 签名，尚未公证；首次打开可能需要在系统“隐私与安全性”中允许。

每个 Release 包含 `SHA256SUMS`。归档内记录版本、源码提交、目标平台和依赖许可证。

GitHub Release 提供 4 个 Inspector 分发包、4 个 CLI ZIP、1 个 C/C++ SDK ZIP 和
`SHA256SUMS`，共 10 个资产。六个 `.crate` 保留在 CI 构建产物中，用于源码、哈希和
registry 校验；Rust 包通过 crates.io 分发。Release 的 `SHA256SUMS` 覆盖其附带的分发包。

## 版本与本地验证

修改 workspace 版本、内部依赖版本和 `Cargo.lock`，新增 `notes/vVERSION.md`。六个 crate 与 Inspector 的版本必须一致。已发布版本、tag 和公共资产不可覆盖。

需要 Rust 1.99.1、Python 3.11+、原生 C 编译器、libclang 和裸机 RISC-V target。Linux 依赖列于 `.github/actions/setup-build/action.yml`。SDK/CLI 的 MSRV 为 Rust 1.88，Inspector 为 Rust 1.95。

```sh
rustup target add riscv32imafc-unknown-none-elf
python3 -m unittest discover -s scripts/tests -v
bash scripts/check.sh
python3 scripts/release.py verify
python3 scripts/release.py binaries
```

`verify` 构建六个 `.crate`，再通过独立消费工程验证 SDK 与 CLI。C SDK 使用生成的工程运行真实 C harness，并构建裸机 RISC-V 静态库。

`binaries` 在匹配的本机平台构建 release 产物，解压 CLI 后执行完整命令闭环，并启动分发包内的真实 Inspector 完成窗口渲染检查。macOS 还校验解压后的代码签名；Linux 解出实际 AppImage，在 Xvfb 中渲染。AppImage 构建工具使用固定版本与 SHA-256。

产物和验证记录位于 `target/dist/`。`--artifacts DIRECTORY` 可指定输出目录。开发验证允许 `verify --allow-dirty`、`binaries --allow-dirty`；公开发布要求干净提交。

## Trusted Publishing

在每个 crate 的 crates.io 设置中添加 GitHub Trusted Publisher：

| 字段 | 值 |
| --- | --- |
| GitHub owner | `umbrella22` |
| Repository | `EmbedScope` |
| Workflow filename | `release.yml` |
| GitHub environment | 留空 |

GitHub Actions repository variable `CRATES_IO_TRUSTED_PUBLISHING=true` 启用上传。发布 job 使用 `rust-lang/crates-io-auth-action` 获取短期凭据，不使用长期 Cargo secret。

## 发布流程

在完成本地验证并推送 master 后，为对应提交创建 annotated tag：

```sh
git tag -a vVERSION -m "EmbedScope VERSION"
git push origin vVERSION
```

`release.yml` 按以下顺序执行：

1. 校验 tag、workspace 版本和远程 master 的源码身份。
2. 运行完整源码验证，在 macOS arm64/x64、Linux x64 和 Windows x64 分别构建与测试分发包。
3. 使用 OIDC 按 `protocol → agent → lvgl → c-api → host → cli` 顺序发布六个 crate。
4. 下载公开 `.crate`，比对 registry checksum、全部源码和 `.cargo_vcs_info.json` 的提交，再运行 registry 消费工程。
5. 核对各平台构建记录、窗口检查和资产哈希，创建草稿 GitHub Release。
6. 上传并重新下载比对全部资产，确认一致后公开 Release。

源码、平台、registry 或资产校验失败时，工作流不会公开 Release。只有 crate 发布 job 拥有 `id-token: write`，只有 GitHub Release job 拥有 `contents: write`。

手动恢复通过 Release workflow 的 `release_tag` 输入指定原 tag。重试先校验已存在的 crate 与资产，仅补齐未完成部分；来源或内容冲突立即停止。不要为恢复任务移动 tag、覆盖资产或递增版本。

关闭 Trusted Publishing 时，工作流要求六个 crate 已从相同提交发布。维护者可通过 Cargo 标准 credential provider 执行 `python3 scripts/release.py publish --local`；该路径同样校验公开源码与独立消费者。

## C/C++ 目标库

SDK ZIP 中的 `vendor/` 提供匹配版本的设备源码，Cargo 从 registry 获取第三方依赖。固件决定目标 triple、CPU/ABI、panic/runtime 和链接脚本；ESP Xtensa 使用相应 Espressif Rust 工具链。

```sh
cargo build --release --locked --target riscv32imafc-unknown-none-elf
```

输出 `target/<target>/release/libescope_c_sdk.a`。C/C++ 工程包含 `include/escope.h` 并链接该库。包装工程默认 panic handler 持续自旋，集成时替换为板级 reset/watchdog 策略，并确保固件只保留一份 Rust panic handler。

自定义 staticlib 工程也可以依赖 crates.io 的 `escope-c-api`。头文件与库使用相同产品版本；目标 ABI、内部 Rust 依赖与 Hello 协商的协议主版本分别遵循各自的兼容要求。
