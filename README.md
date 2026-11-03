# EmbedScope

**Embedded Diagnostics & Control** — 用 Rust 为嵌入式设备提供日志、指标、UI 检查、显示捕获与输入控制。命令行入口为 `escope`，桌面 Inspector 复用同一主机 SDK。

当前版本包含无堆分配的 `no_std` Agent、TCP/串口主机传输、软件参考设备、原生桌面程序、可选 LVGL 接入和 C ABI。已在 ESP-Mosaico S31 上接入团子 OS、LVGL 9.6 与 rs_lvgl 0.3.1，验证真实日志、内存指标、UI 树、最终显示捕获和模拟输入。软件参考设备的数据标为 synthetic。SDK 与 CLI 的 0.1.0 已发布到 [crates.io](https://crates.io/crates/escope-cli/0.1.0)，Inspector 可从源码构建。

## 快速开始

CLI 与设备侧库最低需要 Rust 1.88，桌面 Inspector 使用 eframe/egui 0.37，最低需要 Rust 1.95。已在 macOS arm64、Rust 1.99.1 上验证。默认命令行构建需要平台串口系统库；原生 LVGL 可选测试还需要 C 编译器和 libclang。

从 crates.io 安装 CLI：

```sh
cargo install escope-cli --version 0.1.0 --locked
escope --help
```

从源码构建并启动软件参考设备：

```sh
cargo build -p escope-cli
./target/debug/escope simulate
```

在另一个终端连接参考设备：

```sh
./target/debug/escope -d tcp://127.0.0.1:9337 info
./target/debug/escope -d tcp://127.0.0.1:9337 logs --count 3
./target/debug/escope -d tcp://127.0.0.1:9337 metrics
./target/debug/escope -d tcp://127.0.0.1:9337 ui tree
./target/debug/escope -d tcp://127.0.0.1:9337 input tap 100 140
./target/debug/escope -d tcp://127.0.0.1:9337 display capture screen.png
./target/debug/escope -d tcp://127.0.0.1:9337 diagnose --output diagnosis --logs 8
```

参考设备在同一服务进程内保留计数；点击按钮区域后，树中的计数、frame ID 和屏幕像素都会改变。`screen.png` 和 `diagnosis` 需要使用尚不存在的路径。

```sh
cargo run -p escope-desktop -- tcp://127.0.0.1:9337
# macOS 本地开发应用包（ad hoc 签名）
bash scripts/package-macos.sh
```

Inspector 提供 UI、日志、指标和设备信息四个工作区，支持对象树搜索、树/屏幕选点联动、属性检查、日志筛选、自动刷新和 PNG 保存。通过“发送点击 / Send taps”显式开启输入发送；默认“检查 / Inspect”模式点击画面只选中对象。支持深浅主题，PNG 导出在后台执行。

左下角地球按钮提供“跟随系统 / English / 简体中文”，切换立即生效，正常退出后保留语言选择。界面文案使用 Fluent；设备日志、对象名称、路径与原始错误保持原样。新增语言与桌面源码导航见 [桌面开发说明](crates/escope-desktop/README.md)。

## 设备接入

在固件中依赖 `escope-agent` 和 `escope-protocol`，实现 `Backend`，将字节交给有界帧解码器，并在断线时调用 `Agent::reset`。Agent 不创建 HAL、任务、执行器或 allocator。完整可运行的最小参考：

```sh
cargo run -p escope-agent --example poll_device
bash scripts/check-c-api.sh
```

`escope-lvgl` 默认提供不依赖 native LVGL 的 UI 线程桥和节点注册表。`native` feature 通过宿主使用的 `rs_lvgl` 供应者访问 LVGL；适配器不重复编译另一份 LVGL。显示捕获与输入通过平台分别接入。

C/C++ 消费者使用 EmbedScope 头文件和构建产物，无需跟随内部 Rust crate 的依赖版本。集成时仍需匹配头文件、目标 ABI、链接配置与设备协议；从源码构建产物时遵守对应 Rust 工具链要求。

发布工具会生成包含头文件、SDK 源码与 Rust staticlib 包装工程的 C SDK 源码包，可以脱离仓库针对固件目标编译。也可用自建的薄包装工程直接依赖 crates.io 上的 `escope-c-api`。不提供跨 MCU/ABI 通用的静态库。分发范围、本地打包与 Trusted Publishing 配置见 [发布说明](RELEASING.md)。

团子 OS 的可选 `embedscope` feature 使用独立的第二路 USB CDC。先通过 `escope --json devices` 区分控制台与诊断接口，再使用 `--dtr on` 连接诊断端口。原控制台继续处理应用管理与 A/B OTA。接入结构和构建方式见 [团子 OS 接入](https://umbrella22.github.io/EmbedScope_website/integration/dango)。

## 文档与 skills

- [介绍](https://umbrella22.github.io/EmbedScope_website/guide/start/introduction) / [快速开始](https://umbrella22.github.io/EmbedScope_website/guide/start/quick-start)
- [Rust 接入](https://umbrella22.github.io/EmbedScope_website/integration/rust) / [C/C++ 接入](https://umbrella22.github.io/EmbedScope_website/integration/c) / [LVGL 接入](https://umbrella22.github.io/EmbedScope_website/integration/lvgl)
- [协议规范](https://umbrella22.github.io/EmbedScope_website/reference/protocol) / [CLI](https://umbrella22.github.io/EmbedScope_website/reference/cli) / [主机 SDK](https://umbrella22.github.io/EmbedScope_website/reference/sdk)
- [调试与接入 skills](https://umbrella22.github.io/EmbedScope_website/guide/advanced/skills)
- [架构与生命周期](https://umbrella22.github.io/EmbedScope_website/development/architecture) / [构建与测试](https://umbrella22.github.io/EmbedScope_website/development/testing)

Rspress 文档站位于独立的 [EmbedScope_website 仓库](https://github.com/umbrella22/EmbedScope_website)。站点源码可在其仓库内运行 `npm ci && npm run build`。

## 验证

```sh
bash scripts/check.sh
```

验证包含 workspace 测试、真实 LVGL 内存显示、RISC-V `no_std` 检查、CLI 独立进程闭环和真实 C harness。实机记录另行覆盖双 CDC、Home/原生/Lua/转场显示、原生与 Lua 模拟输入、断线回收及异常帧恢复。物理手指触控、主动崩溃后的 RTC 保留、原生 Windows/Linux 窗口、长期内存碎片和托管部署仍需要对应环境验证。

当前文件管理、OTA、ELF 符号解析、网络配对和多进程 broker 仍属于后续扩展。当前 TCP 面向本地开发与受控测试连接。

MIT licensed. LVGL and other dependencies retain their own licenses.
