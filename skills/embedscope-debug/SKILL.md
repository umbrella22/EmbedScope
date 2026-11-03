---
name: embedscope-debug
description: 使用 EmbedScope 的 escope CLI 或主机 SDK 调试已接入 Agent 的嵌入式设备，采集身份、日志、指标、UI 树和屏幕证据；也支持其软件参考设备。不用于尚未接入 EmbedScope 的通用串口或 JTAG 调试。
---

# EmbedScope 调试

先确认正在观察哪个设备、哪个固件和哪一种数据来源，再根据可用能力采集能解释问题的证据。

## 工具与连接

- 优先使用用户提供的 `escope`、设备路径和工程位置。缺少命令时，从 [GitHub Releases](https://github.com/umbrella22/EmbedScope/releases/latest) 下载匹配平台/架构的 CLI ZIP，解压后直接执行；将目录加入 PATH 后可在任意位置使用 `escope`。已有 Rust 环境时可选择 `cargo install escope-cli --version '=0.1.1' --locked`。开发源码时在实际 checkout 执行 `cargo build -p escope-cli`。
- `escope devices` 只枚举串口；多个端口时结合用户提供的设备身份选择。TCP 使用 `--device tcp://host:port`，串口使用 `--device /dev/...`。串口被占用时处理当前占用来源，不自动终止其他应用。
- 连接后先运行 `escope --device ENDPOINT --json info`，保存设备 build_id、boot_id、session 与 capabilities。软件参考设备的 model 为 `software / synthetic`，其指标和像素不能证明真实板端行为。
- 读取 [命令与结果合同](references/commands.md) 选择实际支持的能力。CLI 与固件版本不一致时以实际 `--help`、hello 和源码定义为准。

确认目标应用以 `embedscope` feature 构建；debug/release profile 不代表 Agent 是否启用。正式团子固件和 rs_lvgl 默认库不会自动启动诊断，缺少端口时先核对构建配置，不能把普通控制台当诊断接口。结束会话不移除已编译的功能，是否关闭需结合最终构建和实际安装镜像判断。

## 证据与复现

已有明确问题时，按相关能力采集：日志用有限 `--count` 或受控 follow；卡顿/内存用 metrics；布局/交互用树、截图与用户授权的输入。需要完整现场时使用 `diagnose --output NEW_DIRECTORY --logs 8`，检查 report.json 的 complete、issues 和数据来源。

输入复现前保存当前 build/session 与 UI revision，复现后重新读取树和显示。节点 ID 需要与 session/revision 一起解释，断线后不能沿用旧节点。操作录制、任意 shell、文件传输、OTA、ELF 符号解析目前没有可用命令。

记录 device dropped、truncated 与主机 host_log_gap。日志无输出可能是过滤、缺少日志、能力未启用、传输占用或连接故障；先收集对应证据，不把缺少日志推断为设备正常。缺测指标是 null，不当成 0。

截图说明其来源是平台最终显示、LVGL 子树还是参考画面。LVGL-only 截图不能代表团子的 Home 或转场。按用户已授权的范围执行输入、复现和代码修改；读取诊断不隐含刷机、重启、升级或删除设备数据。

## 交付

报告结论、触发步骤、设备/固件身份、支持结论的日志/指标/像素，以及失败或缺失的证据。区分观察、推断与已复现根因。链接生成的本地诊断文件，说明验证是在软件参考设备、native 进程还是物理设备执行。修改后用相同触发条件验证可观察结果。
