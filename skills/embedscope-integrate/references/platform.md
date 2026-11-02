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

UiBridge::tree(payload,out) 使用 TreeSource::{revision,read_node}；NativeTree<N> 有界遍历并输出通用 lv_obj/几何/state，当前不提供声明式 View 树或文本。root/线程有效性是 unsafe 构造合同。显示/输入由平台独立提供，不由 TreeSource 隐式启用。

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

固件使用同一 LVGL 9.6 provider 与 rs_lvgl/rs_lvgl_ui 0.3.1。源码依赖假设 EmbedScope 与团子工程为同级目录；构建为 `cargo build -p os --bin os-fw --release --target riscv32imafc-unknown-none-elf --features firmware,embedscope`。C 编译器/libclang 路径仍按目标工程配置。

Agent及大缓冲在PSRAM初始化后创建；开启诊断时服务线程栈为32KiB，关闭时仍为16KiB。目标验证应检查链接栈余量、实际RTOS任务栈和内部heap，不能只依据主线程链接成功判断任务栈足够。指标最低水位缺测返回null。物理验证记录与设备恢复状态见目标工程和EmbedScope当前执行台账。
