---
name: embedscope-integrate
description: 将 EmbedScope Agent 接入 Rust 或 C/C++ 嵌入式工程，配置字节传输、能力 Backend、LVGL/UI 线程桥、显示与输入，以及验证接入。不用于只使用现成 escope 诊断设备的任务。
---

# EmbedScope 接入

根据目标工程的执行器、传输和显示路径接入最小可验证 Agent，再逐项开放能力。以目标工程指令和用户授权为准，保留已有串口、日志、更新和 LVGL 生命周期。

## 调查与最小连接

确认目标架构、no_std/allocator、执行器、USB/UART 收发点、全局 logger、现有控制协议，以及 LVGL native 供应者。读取实际 EmbedScope 的 Backend、typed payload 和 C header；[平台接入参考](references/platform.md) 提供接口与生命周期决策。

先提供 Hello/Info。capabilities 只报告已实现且可访问的操作；没有屏幕的设备可以只接日志和指标。协议/Agent 默认无分配，不引入 Tokio 或强制 Embassy。设备侧 `escope-protocol`、`escope-agent`、`escope-lvgl` 使用 crates.io 版本；应用锁文件记录实际版本，C ABI 包为 `escope-c-api`。C/C++ 接入可从 [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases/latest) 下载 C SDK ZIP，并按目标 ABI 交叉编译。源码开发使用审阅后的 path/Git revision。主机 CLI 优先使用 Release ZIP；已有 Rust 环境可选 Cargo 安装。

把接收字节交给 FrameDecoder 和 decode_frame，由 Agent::handle 执行请求，再完整发送其返回帧。发送器保存部分写的后缀，不能在 TX 完成前复用缓冲。断线同时清理 parser 并 Agent::reset。seed 非零且跨启动/连接变化。

## 编译开关与交付配置

由应用拥有默认关闭的 `embedscope` feature，将 SDK 依赖设为 optional，并以 `#[cfg(feature = "embedscope")]` 门控模块、初始化、传输、缓冲与日志分流。日常排查和设备验收显式开启；复现优化问题可用 `--release --features embedscope`；正式交付不选择该 feature。

`--release` 不自动移除已启用的调试功能，断开 Inspector 只清理会话。Cargo features 会合并；`--no-default-features` 不能撤销其他依赖显式启用的能力，正式构建避免 `--all-features`。检查 normal/build 依赖图和最终链接产物；设备还要安装关闭诊断的新固件并确认接口。`rs_lvgl_ui/diagnostics` 只提供事件/sink，不会启动 Agent 或端口。

## 能力与线程

在格式化前 fan-out 结构化日志；所有 Agent 调用串行化，跨任务记录先进入有界队列。指标缺测使用 Option/null，记录 internal/PSRAM 和最小值的统计范围。

LVGL 请求在 UI 线程安全点执行；通信任务仅投递请求/接收结果。Backend 同步返回，当前 SDK 不自动重试 Busy。不要依赖通信任务直接访问 LVGL 或等待它自己完成异步工作。

优先复用宿主的 LVGL 实例、配置与 native 供应者。默认 UiBridge 不链接 LVGL；native feature 使用 rs_lvgl_sys 0.5.0 供应者。NativeTree 变化后 changed，root 销毁前停止读取；节点 ID 与 session/revision 配对。自定义 NodeRegistry 在删除钩子 remove，禁止已释放指针继续解析。

显示捕获独立于 LVGL 树，覆盖平台实际输出路径；固定 capture/frame ID、像素格式、stride 和数据生命周期。输入通过应用统一路由成对产生 press/release，reset 释放未完成输入。先完成最小能力，逐项验证再声明 capability。

C/C++ 工程使用准确 escope.h 和 caller-allocated context，按 consumed/TX Busy 合同推进。平台静态库 wrapper 负责 panic/runtime/链接；C ABI 仍需要 Rust 交叉编译产物。

## 验证和交付

运行相关单元/协议测试、目标 cargo check 和可用的 native/C fixture。检查 target runner 后再运行目标程序；构建验证与用户授权的刷机是分开的动作。物理接入验证包含身份、日志、指标、断线重连、坏帧恢复、输入前后状态和截图来源。

交付修改路径、接入入口、开放能力、缓冲和线程预算、实际执行的验证和证据限制。工程缺少真实 provider 或硬件时保留其未验证状态，同时完成独立可执行的库、参考适配和文档工作。
