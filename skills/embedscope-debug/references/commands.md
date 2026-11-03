# 已实现的调试接口

## 获取主机工具

从 [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases/latest) 下载匹配平台和架构的 CLI ZIP，解压后执行 `escope`；将其目录加入 PATH 后可在任意位置使用。已有 Rust 环境时可选择 Cargo 安装。Inspector 使用同一 Release 中的 AppImage、Windows 免安装 ZIP 或 macOS 应用 ZIP。

## 基本命令

```sh
escope --json devices
escope --device ENDPOINT --json info
escope --device ENDPOINT --json logs --count 8
escope --device ENDPOINT --json metrics --count 3 --interval-ms 1000
escope --device ENDPOINT --json ui tree
escope --device ENDPOINT display capture NEW_SCREEN.png
escope --device ENDPOINT --json crash show
escope --device ENDPOINT diagnose --output NEW_DIRECTORY --logs 8
```

串口 DTR 默认 preserve；目标 CDC 等待 DTR 时使用 `--dtr on`。devices 的 usb_vid、usb_pid、serial_number 与 usb_interface 可区分同一设备上的多个接口，缺测字段为 null。

`--json` 成功输出到 stdout，流每行一条 JSON；操作错误输出到 stderr 并退出 1。参数错误保留 clap 的退出行为。截图路径必须尚不存在；诊断输出目录的父目录必须已存在。

info 的 device 包含 name/model/firmware/build_id/boot_id/uptime_ms，session 包含 session/protocol/max_payload/capabilities/name/build_id。capabilities 数组使用 logs、metrics、ui.tree、input.tap、display.capture、crash。

metrics 的 heap_free/heap_min_free/heap_largest_free/psram_free 是可选字节值，frame_ms 和 queue_depth 可选。heap_min_free 的统计来源需要从平台确认。UI node 含 id/parent/kind/name/x/y/width/height/flags/revision，flags 含义由 provider 定义。

日志 level 1=Error、2=Warn、3=Info、4=Debug、5=Trace。`--level 3` 包含 1–3，`--target input` 是 target 子串过滤。`--count` 为匹配记录数；有限等待按 `--timeout-ms` 超时，follow 可由 Ctrl-C 结束。

## 输入和 Inspector

```sh
escope --device ENDPOINT input tap 100 140 --hold-ms 60
```

打开 Inspector，在顶部填写同一 ENDPOINT 并连接。用户已请求操作复现时，tap 经平台统一输入路由执行。Inspector 默认点击画面只选择节点；Send taps to device 开启后才发送输入。当前没有多进程串口 broker；同一真实串口避免 CLI 和 GUI 同时打开。

## 软件参考设备

```sh
escope --json simulate --listen 127.0.0.1:9337
```

默认参考画面 320×240，按钮区域 x=32..287、y=120..175；tap 100 140 会增加计数，修改 UI revision 和截图像素。计数保存在服务进程中，CLI 重新连接后继续存在。该设备的崩溃摘要明确说明没有物理 crash 记录。

诊断包 report.json 的 complete=false 或 issues 非空表示部分采集失败。schema=1；frame_id、设备时间、主机 captured_at_unix_ms 分别保留，不混为同一个时钟。

CaptureStart 超时/取消且尚不知道 capture ID 时，SDK 会关闭 Session 触发平台 reset；重新连接再采集。已知 ID 的中断会尝试 CaptureEnd，清理失败则断开。不要在相同旧会话内无限重试 Busy。

## 团子 OS

包含 `embedscope` feature 的团子固件枚举两路 CDC：原接口用于文本控制台和 OTA，第二路承载 EmbedScope。ESP-Mosaico S31 的 VID:PID 为 303a:4001，macOS 串口接口号通常为 1/3；选择同一 USB 序列号下的第二路，使用 `--dtr on`。端口名称会因机器改变。官方 v0.4.0 固件没有诊断 Agent，单 CDC 不能作为 EmbedScope 端口打开。

实机 info 的 model 为 `ESP-Mosaico S31 / physical`。其树适配器提供真实控件类型、label 文本与页面名；Home/Lua 画布内容不会展开成逐元素 LVGL 节点。截图来自最终显示平面，包含 Home、原生/Lua Screen 和 Slide 转场。输入经过系统统一队列，页面切换完成后重新采集树与截图；二者是独立采样。

日志中的 `rs_lvgl_ui.stats` 可与页面、revision 和像素共同定位问题。内存最低水位当前缺测，应为 null。RTC 摘要关联 build marker，未主动复现崩溃时不能宣称完整 crash 保留验证。固件构建与刷写使用团子工程工具，不是 escope 命令。

## 编译开关

目标应用使用默认关闭的 `embedscope` feature。开启后，debug/release profile 均可保留 Agent。团子 OS 的 `cargo fw --features embedscope` 构建 release 诊断固件；`cargo fw` 构建不含诊断的正常固件。`rs_lvgl_ui/diagnostics` 只提供事件/sink，不启动 Agent 或端口。

结束会话只清理资源。移除诊断功能需要关闭应用 feature、重新构建，并在设备上安装和启动新镜像。检查 normal/build 依赖图、最终产物和实际枚举接口；普通文本控制台不能作为诊断端口。

## rs_lvgl 原生进程

在包含示例的 rs_lvgl checkout 运行 `cargo run --manifest-path examples/embedscope/Cargo.toml --features embedscope`。连接 `tcp://127.0.0.1:9338`，model 应为 `LVGL 9.6 / memory display`。画面来自真实 LVGL RGB888 flush，不是 simulate 的 synthetic 像素，也不代表物理设备。

通用树提供 lv_obj、几何和 state，名称为空；按钮为224×56，默认布局可 tap 100 140。一个连接同时服务多个 SDK 请求，CLI/Inspector轮流使用。短连接 tap 断线时配对 release，显式 Cancel 才抑制 click；树revision会因输入状态和应用更新变化。CRASH不支持，heap/PSRAM缺测null。需要日志时在同一Session订阅并触发计数更新，不能对空闲且没有日志源的进程要求固定条数的诊断包完整。

默认构建和 `cargo build --manifest-path examples/embedscope/Cargo.toml --release` 不启用诊断；`--release --features embedscope` 仍保留诊断。启用、关闭和移植合同见 [rs_lvgl 指南](https://umbrella22.github.io/rs_lvgl_website/guide/embedscope.html)。
