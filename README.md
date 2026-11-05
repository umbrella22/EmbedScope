# EmbedScope

**Embedded Diagnostics & Control** — 用 Rust 为嵌入式设备提供日志、指标、UI 检查、显示捕获与输入控制。命令行入口为 `escope`，桌面 Inspector 提供可视化调试工作区。

设备侧运行无堆分配的 `no_std` Agent，通过串口或 TCP 字节流连接主机。Rust 与 C/C++ 固件均可接入，LVGL 适配器支持复用应用的原生实例。团子 OS 的接入覆盖日志、内存指标、UI 树、最终显示捕获、模拟输入与 RTC 崩溃摘要。

## 下载与运行

从 [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases/latest) 选择与操作系统和处理器架构匹配的文件：

| 工具 | Linux | Windows | macOS |
| --- | --- | --- | --- |
| Inspector | AppImage | 免安装 ZIP | `.app` 应用 ZIP，提供 Apple Silicon 与 Intel 版本 |
| `escope` CLI | ZIP | ZIP | ZIP，提供 Apple Silicon 与 Intel 版本 |
| C/C++ SDK | 包含头文件、Rust 源码和 staticlib 包装工程的 ZIP，可按固件目标交叉编译 | 同左 | 同左 |

CLI 解压后可直接执行 `./escope --version`，Windows 使用 `.\escope.exe --version`。将可执行文件所在目录加入 `PATH` 后，即可在任意工作目录使用 `escope`。下载包无需安装 Rust。各文件的完整性可用同一 Release 的 `SHA256SUMS` 校验。

Inspector 在 Windows 解压后运行程序，在 macOS 解压后打开应用；Linux 先为 AppImage 添加执行权限，再运行。具体操作见[快速开始](https://umbrella22.github.io/EmbedScope_website/guide/start/quick-start)与[桌面 Inspector](https://umbrella22.github.io/EmbedScope_website/guide/basic/inspector)。

## 第一次调试

使用软件参考设备体验完整会话：

```sh
escope simulate
```

保持服务运行，在另一个终端执行：

```sh
escope -d tcp://127.0.0.1:9337 info
escope -d tcp://127.0.0.1:9337 logs --count 3
escope -d tcp://127.0.0.1:9337 metrics
escope -d tcp://127.0.0.1:9337 ui tree
escope -d tcp://127.0.0.1:9337 input tap 100 140
escope -d tcp://127.0.0.1:9337 display capture screen.png
escope -d tcp://127.0.0.1:9337 diagnose --output diagnosis --logs 8
```

参考设备提供 synthetic 数据。点击按钮区域后，计数、frame ID 和屏幕像素会改变。每次保存使用新的截图文件名和诊断目录。

在 Inspector 顶部填写 `tcp://127.0.0.1:9337` 并连接，即可使用 UI、日志、指标和设备信息四个工作区。对象树、画面选点和属性检查相互联动；默认“检查 / Inspect”模式只选择对象，开启“发送点击 / Send taps”后才会向设备发送输入。

左下角地球菜单支持“跟随系统 / English / 简体中文”，主题按钮切换深浅外观。PNG 保存、日志筛选和自动刷新可以在同一会话中使用。

## 设备接入

Rust 固件依赖 `escope-agent` 和 `escope-protocol`，实现 `Backend`，将传输字节交给有界帧解码器。应用提供任务调度、HAL、显示与输入，断线时调用 `Agent::reset` 释放会话资源。`escope-lvgl` 提供 UI 线程桥和可选原生树适配器，适配器与应用使用同一 LVGL provider、ABI 和配置。

C/C++ 固件从 Release 下载 C SDK ZIP，使用配套头文件和包装工程，按目标 triple、CPU/ABI、panic 策略和链接配置构建静态库。头文件与 SDK 使用同一发布版本；设备协议和目标 ABI 分别具有兼容性要求。具体步骤见 [C/C++ 接入](https://umbrella22.github.io/EmbedScope_website/integration/c)。

团子 OS 通过默认关闭的 `embedscope` feature 控制诊断功能。开发与问题复现使用 `cargo fw --features embedscope`，正式交付使用 `cargo fw`。诊断通过第二路 USB CDC 提供，连接时选择实际诊断接口并启用 DTR。完整说明见[团子 OS 接入](https://umbrella22.github.io/EmbedScope_website/integration/dango)。

## 开发与构建

已有 Rust 环境时，也可从 crates.io 安装 CLI：

```sh
cargo install escope-cli --version '=0.3.0' --locked
```

CLI 与设备侧库最低需要 Rust 1.88；Inspector 使用 eframe/egui 0.37，最低需要 Rust 1.95。平台构建依赖见[构建与测试](https://umbrella22.github.io/EmbedScope_website/development/testing)。

```sh
cargo build -p escope-cli
cargo run -p escope-desktop -- tcp://127.0.0.1:9337
bash scripts/check.sh
```

`check.sh` 检查 workspace、真实 LVGL 内存显示、`rs_lvgl_ui` 0.5 消费者、裁剪后的 LVGL 配置、RISC-V `no_std`、CLI 独立进程和 C harness。桌面源码结构与语言扩展见[桌面开发说明](crates/escope-desktop/README.md)，分发与发布流程见[发布说明](RELEASING.md)。

## 文档与 skills

- [介绍](https://umbrella22.github.io/EmbedScope_website/guide/start/introduction) / [快速开始](https://umbrella22.github.io/EmbedScope_website/guide/start/quick-start)
- [Rust 接入](https://umbrella22.github.io/EmbedScope_website/integration/rust) / [C/C++ 接入](https://umbrella22.github.io/EmbedScope_website/integration/c) / [LVGL 接入](https://umbrella22.github.io/EmbedScope_website/integration/lvgl)
- [协议规范](https://umbrella22.github.io/EmbedScope_website/reference/protocol) / [CLI](https://umbrella22.github.io/EmbedScope_website/reference/cli) / [主机 SDK](https://umbrella22.github.io/EmbedScope_website/reference/sdk)
- [调试与接入 skills](https://umbrella22.github.io/EmbedScope_website/guide/advanced/skills)
- [架构与生命周期](https://umbrella22.github.io/EmbedScope_website/development/architecture) / [构建与测试](https://umbrella22.github.io/EmbedScope_website/development/testing)

文档站源码位于 [EmbedScope_website](https://github.com/umbrella22/EmbedScope_website)。

MIT licensed. LVGL and other dependencies retain their own licenses.
