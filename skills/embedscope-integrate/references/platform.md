# 接口与平台决策

## Rust 边界

```rust
pub trait Backend {
    fn capabilities(&self) -> u32;
    fn info(&self) -> escope_protocol::DeviceInfo<'_>;
    fn handle(&mut self, op: u16, payload: &[u8], reply: &mut [u8])
        -> Result<usize, escope_protocol::Status>;
    fn reset(&mut self);
}
```

Agent::new(seed)、handle(Envelope, backend, tx)、push_log、poll_event、reset。FrameDecoder::push(byte) 返回 encoded frame，不含零定界符；decode_frame 返回借用 scratch 的 Envelope；调用 handle 前保持 scratch 有效。tx 至少 MAX_FRAME=1098 字节。有效 payload 最长1024字节。

Agent 自带 Hello/Info/LogsSubscribe/Unsubscribe 与 LOGS 队列。系统 handle 提供 Metrics/UiTree/InputTap/CaptureStart/Read/End/Crash/Cancel；所有编码使用协议 reexport 的 minicbor 或 encode_payload。能力位 LOGS/METRICS/UI_TREE/INPUT/CAPTURE/CRASH 依次为 bit0..5。

日志 FIFO 16项，target/message 48/192 UTF-8字节，溢出丢新记录并报告，截断有 truncated。平台任务中处理 push_log，不在中断里并发调用同一Agent。

`UiBridge::tree(payload, out)` 使用 `TreeSource::{revision, read_node}`。EmbedScope 0.5.0 的 `NativeTree<N>` 直接依赖 `rs_lvgl_sys` 0.7.0，与应用的 `rs_lvgl` / `rs_lvgl_ui` 0.7.0 共用唯一 provider；默认 `TreeSource` 不依赖 LVGL。原生兼容版本线如下，应用与适配器需要统一升级依赖和锁文件：

| EmbedScope SDK | rs_lvgl provider |
| --- | --- |
| 0.5.x | 0.7.x |
| 0.4.x | 0.6.x |
| 0.3.x | 0.5.x |
| 0.2.x | 0.4.x |
| 0.1.x | 0.3.x |

NativeTree 按实际启用的控件输出 `label`、`button`、`slider` 等类型；普通 Flex/Grid 容器输出 `row`、`column`、`grid`，其余为 `lv_obj`。它支持应用关闭 Flex 或 Grid 的配置，不依赖高层样式 API。树保留原生 host 和控件内部节点，`name` 为空；View key、组件边界和文本由应用 `TreeSource` 提供。

rs_lvgl_ui 的 `typed::...`、`child_if`、`binding` 和组件插槽沿用原生对象树，插槽不额外生成包装节点。先完成事件分发、应用更新、协调和布局，再调用 `changed()` 发布 revision 并响应查询。`NativeTree` 只遍历传入的 root；`App::overlay` 挂载在独立 top layer，检查弹窗时在 UI 线程选择 `lv_layer_top()`，或由应用 `TreeSource` 统一管理多层节点。

rs_lvgl_ui 的 `container()`、`progress_bar(...)`、`message_box(...)`、`animated_image(...)` 分别对应原生 `lv_obj`（有布局时按布局分类）、`bar`、`msgbox`、`animimage`。高层构造器改名不改变原生 `kind`、低层 rs_lvgl/C API 或 `widget-bar`、`widget-msgbox`、`widget-animimage` feature。

0.7 的 `range_slider` 显示为 `slider`；`chart(ChartSpec)` 显示为一个 `chart`；`custom(widget, props)` 显示实际创建的原生类，例如 `arc`。双端值、chart series ID 和 Props 不生成虚拟节点。Flex wrap/reverse 改变最终几何，不改变原生子对象的顺序。应用迁移时使用统一 ChartSpec、CustomWidget::Props/create/apply_props 签名；手写 Textarea 描述包含 input 字段。细节以 [0.7 迁移指南](https://umbrella22.github.io/rs_lvgl_website/guide/ui-api-migration.html)为准。适配器依据 sys 的有效配置工作，高层 UI 的描述和布局限制由上游处理。

root/线程有效性是 unsafe 构造合同。显示/输入由平台独立提供，不由 `TreeSource` 隐式启用。设备协议保持 v1，与 Rust native provider 版本独立。

主机串口默认保留 DTR。CDC 等待 DTR 时使用 CLI `--dtr on`、SDK ConnectOptions.dtr=Some(true) 或 Inspector 的 DTR 选项；按板验证控制线与复位行为。

## C 边界

使用当前 checkout 的 crates/escope-c-api/include/escope.h。查询 context size/alignment 后初始化，context 不移动；callbacks/user data 存活期间串行调用。feed 在完整帧后停止并返回 consumed；TX满返回 Busy且consumed=0，take_tx排空后继续未消费后缀。容量不足保留TX并返回所需written。

ES_OK/ES_BUSY 等为本地 ABI 返回码，ES_STATUS_* 为远程操作状态，不混用。Callback不得保留借用指针、越界写或unwind。Info UTF-8容量name64/model64/firmware32/build_id64，按header合同返回。poll只在TX空闲时转入日志。

真实C示例：examples/c，验证脚本 bash scripts/check-c-api.sh；它用std主机wrapper提供运行时。固件要提供目标panic handler和链接，不从该示例推断任意MCU工具链已经验收。

## 团子 OS 的现有适配

先检查目标 checkout 是否包含 `crates/os/src/embedscope/` 与 `embedscope` feature。已接入版本复用这些模块，不重复引入 Agent、USB 接口或 LVGL 实例。官方 v0.4.0 发行固件没有开启诊断；不能把发行版控制台当作 EmbedScope 端口。

- `boot/mod.rs::usb_task` 和 `embedscope/transport.rs`：保留原日志/应用/OTA CDC，第二路 CDC 传输二进制帧。主机需要 DTR，断线 epoch 丢弃旧请求、响应并重置会话。
- `console/transport/logging.rs` 和 `embedscope/logs.rs`：格式化前 fan-out，有界上游队列的丢失量通过 Agent::record_dropped_logs 合并。避免只统计 Agent 内部 FIFO。
- `shell/runtime` 和 `embedscope/device.rs`：在既有 UI loop 处理 Agent，LVGL 调用不跨线程。树快照拥有文本/几何数据，后续 cursor 请求不解引用已删除对象。
- `services/display/plane.rs`、`embedscope/frame.rs`、`capture.rs`：完整上传后更新最终 RGB565 影子帧，独立复制固定捕获。Home、Screen、Slide 均覆盖，不长期占用渲染 lease。设备调度器在异步复制就绪后答复 CaptureStart，SDK不会自动重放Busy。
- `services/input/touch.rs::INPUT`：合成按压/定时释放进入统一系统输入队列，reset 释放未完成输入。
- `platform/runtime/crashlog.rs` 和 `panic_record.rs`：RTC 摘要只有在完整 build marker 匹配时才接受构建/启动元数据，历史 panic 可保留未知身份。

固件使用同一 LVGL 9.6 provider 与 rs_lvgl/rs_lvgl_ui 0.3.1。可选 SDK 依赖来自 crates.io，`Cargo.lock` 记录版本/checksum；构建为 `cargo build -p os --bin os-fw --release --target riscv32imafc-unknown-none-elf --features firmware,embedscope`。C 编译器/libclang 路径仍按目标工程配置。

Agent及大缓冲在PSRAM初始化后创建；开启诊断时服务线程栈为32KiB，关闭时仍为16KiB。目标验证应检查链接栈余量、实际RTOS任务栈和内部heap，不能只依据主线程链接成功判断任务栈足够。指标最低水位缺测返回null。物理验证记录与设备恢复状态见目标工程和EmbedScope当前执行台账。

## rs_lvgl 的原生参考接入

rs_lvgl 仓库的 `examples/embedscope/` 是独立应用；先检查其 Cargo.toml 与锁文件选择的版本。该示例当前固定 registry rs_lvgl/rs_lvgl_ui 0.3.1 与 EmbedScope 0.1.0，属于上表的 0.1.x 接入。升级到 EmbedScope 0.5.x 时同时升级应用到 rs_lvgl/rs_lvgl_ui 0.7.x，保留单一 native provider。它运行真实 LVGL 9.6 的 320×240 内存显示，没有 SDL 窗口；不能当作物理显示验收。

在该仓库根目录：

```sh
cargo run --manifest-path examples/embedscope/Cargo.toml --features embedscope
cargo run --manifest-path examples/embedscope/Cargo.toml --release --features embedscope
cargo build --manifest-path examples/embedscope/Cargo.toml --release
cargo tree --manifest-path examples/embedscope/Cargo.toml --edges normal,build
cargo test --manifest-path examples/embedscope/Cargo.toml --features embedscope -- --test-threads=1
```

前两条启用诊断，默认端口 `tcp://127.0.0.1:9338`，只允许 loopback、一个连接。第三条关闭诊断，第四条应没有 escope-* 正常/构建依赖；dev-dependencies 不属于出货产物。不能把应用的 embedscope feature 写成 rs_lvgl_ui 的库 feature。

实现由 UI-loop 轮询非阻塞 TCP/Agent、NativeTree<128>、真实 RGB888 flush 镜像和原生 Pointer。捕获拥有固定像素，End/reset 释放；输入配对，断线提前 release 已接受的 tap（适配短连接 CLI），显式 Cancel 抑制该 click。该示例固定的 SDK 0.1.0 输出 `kind=lv_obj`、`name` 为空；SDK 0.5.0 的类型分类见上文，语义文本仍由自定义 `TreeSource` 提供。内存缺测 null，CRASH 未声明。缓冲、listener、sink、input 均受同一个 feature 控制。

复制到板端时复用现有 executor、最终显示与输入仲裁，不把主机 std/TCP 服务强塞到 no_std 库。混用本地 path 与 registry rs_lvgl 可能产生重复 links=lvgl；统一整个 provider 来源，或去掉 escope-lvgl/native 并实现应用 TreeSource。参考 [rs_lvgl 调试指南](https://umbrella22.github.io/rs_lvgl_website/guide/embedscope.html)。
