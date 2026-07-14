# Agent Manager 安装编排项目交接说明

日期：2026-07-14
用途：交给新的负责人继续完成 Windows Sandbox 验收工具、Windows 交互测试和 macOS 实机验收。

## 一、结论摘要

安装编排功能本身已经实现，Windows 与 macOS 的编译、自动化测试、安装包构建和产物校验均已在 GitHub Actions 通过；但项目还不能视为完整发布验收通过，因为 28 个一次性平台交互场景仍然是 `0/28`。

当前最直接的阻塞点不是产品代码，而是 Windows Sandbox 测试工具：旧工具把整个宿主机 `test` 目录以可写方式映射进 Sandbox，存在覆盖安装包、校验文件和测试脚本的风险。加固设计已经批准，但实施计划在最终审查中仍有两个不可执行的设计错误，因此该计划目前明确禁止执行。

可用一句话概括当前状态：

> 功能实现完成，双平台 CI 编译/测试/打包完成，安全问题已修复；真实安装交互验收尚未开始，Windows Harness 计划需要先修正，macOS 需要真实的一次性 Intel 与 Apple Silicon 环境。

## 二、权威工作区与 Git 状态

### 权威工作区

- 唯一允许继续工作的 checkout：`D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator`
- 分支：`feature/installation-orchestrator`
- 当前本地 HEAD：`2d14b2eca2bf5ddafc68420946f0ac3901d19dfe`
- 远端 `origin/feature/installation-orchestrator`：`ef856ca9cef41ea70f6ba46788e303e56015f1ff`
- 当前状态：工作树干净，本地领先远端 6 个提交，落后 0 个提交。

这 6 个未推送提交只包含 Windows Sandbox 加固设计和实施计划文档，没有产品代码变化：

1. `a4f2446e`：初版 Sandbox 加固设计。
2. `2ca74b85`：修正证据控制边界。
3. `cf9a95ed`：初版实施计划。
4. `41a19e46`：计划修正。
5. `1e2d6670`：计划协议加固。
6. `2d14b2ec`：计划审查门修正，但最终仍被判定为 BLOCKED。

### 禁止使用的主 checkout

`D:\codex\ai-deploy-toolkit\apps\agent-manager` 不是权威实现来源，当前仍有历史污染：

- `src-tauri/src/lib.rs` 已修改。
- `src-tauri/src/installer/` 为未跟踪目录。
- `main` 比 `origin/main` 领先 2 个提交。

不要清理、覆盖、合并或继续在这个主 checkout 开发，除非所有者另行决定如何处理历史污染。

## 三、完成情况

| 工作面 | 当前状态 | 证据 |
| --- | --- | --- |
| 安装编排实施计划 | 完成 | 9/9 个实施任务已接受。 |
| 前端格式、类型、单元测试和构建 | 完成 | 最终 Windows/macOS CI 均通过；Vitest 41/41。 |
| Rust 格式与 `cargo check` | 完成 | 最终 Windows/macOS CI 均通过。 |
| Rust 测试 | 完成 | Windows 155/155；macOS 160/160。 |
| Windows 打包 | 完成 | MSI 与 Portable ZIP 构建、清单和 SHA-256 校验通过。 |
| macOS 打包 | 完成 | Universal DMG 构建，并验证 Mach-O 同时包含 arm64 与 x86_64。 |
| 发布工作流安全 | 完成 | `release.yml` 的签名材料前缀输出已在 `3ec3a499` 删除并独立复核。 |
| 下载产物静态校验 | 完成 | 本地产物哈希与 CI 完全一致，ZIP 无路径穿越、绝对路径、重复条目或符号链接风险。 |
| Windows 一次性环境交互验收 | 未开始 | 0/10，Harness 安全阻塞。 |
| macOS Intel 交互验收 | 未开始 | 0/9，没有真实的一次性 Intel 环境。 |
| macOS Apple Silicon 交互验收 | 未开始 | 0/9，没有真实的一次性 Apple Silicon 环境。 |

不要给出单一的“项目 100% 完成”结论。准确表述应当是：实施与 CI 打包已经完成，交互式平台验收为 `0/28`。

## 四、最终通过的产品代码与 CI

最终通过双平台 CI 的产品代码提交是：

`ef856ca9cef41ea70f6ba46788e303e56015f1ff`

该提交之前的重要修复包括：

- `3ec3a499`：删除从 Tauri 签名材料派生并输出 10 字符前缀的危险诊断。
- `cb28753b`：修复两个 Rust `E0382` 所有权问题。
- `b6781184`：修复三个 Rust 测试编译问题。
- `320bb01a`：拒绝 Windows 注册表 PATH 中的空条目。
- `ef856ca9`：使 macOS PATH 解析不依赖当前宿主系统，并过滤空段。

### 最终 CI

- Windows：<https://github.com/Hanbao7777/agent-manager/actions/runs/29306055858>
  - 状态：成功。
  - 耗时：11 分 18 秒。
  - Rust 测试：155/155。
  - 产物：`Agent-Manager-0.1.0-Windows-internal`。
- macOS：<https://github.com/Hanbao7777/agent-manager/actions/runs/29306056981>
  - 状态：成功。
  - 耗时：10 分 27 秒。
  - Rust 测试：160/160。
  - 产物：`Agent-Manager-0.1.0-macOS-Universal-internal`。

两次最终运行只使用标准 `windows-2022` 和 `macos-14` GitHub-hosted Runner，没有使用 Larger Runner。仓库在运行后已恢复为私有；两次临时公开窗口合计约 35 分 12 秒，API 检查没有发现 Fork。

### 最终产物哈希

- Windows 外层 Artifact ZIP：`0272d3dee5af66f26a515dda1cee7d51cf0739782d5f7942531368124ecebac8`
- Windows MSI：`90d1b5ca888b69d3bb8d87debeb0cc54538e6e628c8cade081c67d49fcfcdfcd`
- Windows Portable ZIP：`47b88a9f684e7960208b95134898fd1d5e7c54b1acc4b0f8296032ffe3cc3142`
- macOS 外层 Artifact ZIP：`f00bcedabf0fb11cd390c2df4ff1ad230f05b569d176b7e396d8ec5b175912bc`
- macOS DMG：`53e7f81e4af14d0c5a8cc7c715090b8671f8608668f4d75d7f29470a6e0ab1a6`

本地下载目录：`D:\codex\ai-deploy-toolkit\test`

该目录当前包含 MSI、Portable ZIP、独立 EXE、macOS Artifact ZIP、DMG、`SHA256SUMS.txt`、WebView2 安装器、旧 `.wsb`、旧 PowerShell Harness 和历史 `evidence`。这里是受控暂存区，不是版本化源码目录。

## 五、当前主要阻塞点

### 阻塞 1：旧 Windows Sandbox Harness 有高危宿主写入边界

问题编号：`TEST-HARNESS-001`
严重级别：HIGH

旧文件：

- `D:\codex\ai-deploy-toolkit\test\agent-manager-test.wsb`
- `D:\codex\ai-deploy-toolkit\test\run-agent-manager-sandbox.ps1`

`.wsb` 把整个 `D:\codex\ai-deploy-toolkit\test` 映射为 Sandbox 内的 `C:\AgentManagerTest`，并设置 `ReadOnly=false`。随后 LogonCommand 自动从该映射运行脚本。脚本会清理和创建目录、启动 WebView2 安装器、运行 MSI 安装/卸载并启动 Portable EXE。

虽然这些进程本意是在 Sandbox 内执行，但 Sandbox 代码也能改写宿主机上的安装包、哈希清单、Harness 文件和全部历史证据。因此安全预检正确地阻止了运行；截至交接时，没有因为本轮验收启动过 Sandbox、安装器或产品进程。

旧 `test\evidence` 中的日志和截图早于本轮，不能当成本轮新证据。

### 阻塞 2：加固设计已批准，但当前实施计划禁止执行

已批准设计：

`docs/superpowers/specs/2026-07-14-windows-sandbox-harness-hardening-design.md`

设计选择 Alternative B：

- 固定 `test\input` 作为只读映射。
- 每轮只把新建且为空的 `test\evidence\<run-id>\sandbox-output` 映射为可写。
- `control`、launch manifest、配置哈希和收集记录保留在未映射的宿主目录。
- 离线配置默认禁网；只有明确的下载/网络失败场景启网。
- 剪贴板、打印机、音频输入、视频输入和 vGPU 均禁用。
- `complete.json` 最后写入；缺失、陈旧、部分完成、ID/哈希不匹配或清理失败均不得通过。
- 版本化 Harness 源码应进入 `scripts\windows-sandbox\`，外部 `test` 只是 staging。

当前被阻塞的计划：

`docs/superpowers/plans/2026-07-14-windows-sandbox-harness-hardening.md`

不要按该计划执行。最终审查发现两个阻塞问题：

1. **映射路径偏离批准设计。** 计划把动态目录 `test\input-generations\input-<hash>-<guid>` 直接映射进 Sandbox，而批准设计要求始终只映射固定的 `test\input`。
2. **Driver API 跨进程不可实现。** 计划让场景 Driver 接收包含 ScriptBlock 的 Hashtable，但 Driver 又通过独立的 `powershell.exe -File` 进程启动；ScriptBlock/Hashtable 无法按该方案通过命令行跨进程传递。

前任 Coordinator 已用 Terra 和 Sol 进行多轮计划修正，达到自动替换上限后停止。这不是产品实现失败，而是计划仍不具备可执行性。

### 阻塞 3：macOS 交互测试环境不存在

Windows 主机只能静态核对 DMG、ZIP、哈希和已有 Mach-O 架构证据。以下 18 个场景必须分别在真实的一次性 macOS Intel 与 Apple Silicon 环境执行，不能用 Windows 静态检查或 GitHub Actions 打包结果替代。

### 非阻塞但需注意：本机缺少 MSVC `link.exe`

历史本地验证报告记录 `cargo check`/`cargo test` 因本机缺少 MSVC `link.exe` 而被阻塞。该限制已经由最终 Windows/macOS CI 的完整 Rust check/test 成功证据弥补，因此不是当前产品代码 blocker，但仍会影响接手人在这台电脑上进行本地 Rust 编译。

## 六、尚未执行的 28 个交互场景

### Windows：0/10

1. Clean install
2. UAC accepted
3. UAC declined
4. PATH refresh
5. Multiple Node installations
6. Proxy failure
7. File lock
8. Disk-space guard
9. Batch partial failure
10. Postflight path/version

### macOS Intel：0/9

1. PKG authorization accepted
2. PKG authorization declined
3. Signature rejection
4. GUI/zsh PATH difference
5. `/usr/local` versus `/opt/homebrew`
6. Root-owned npm prefix
7. Proxy/TLS failure
8. Batch partial failure
9. Postflight path/version

### macOS Apple Silicon：0/9

场景与 Intel 相同，但必须在 Apple Silicon 一次性环境重新执行和留证，不能复用 Intel 结果。

## 七、推荐的接手方案

### 第一步：修正计划，不要立即实现

保留已批准设计，针对两个 blocker 修改实施计划：

1. **固定映射 `test\input`。**
   - 在同一卷创建未映射的 staging 目录并完成白名单、哈希、reparse-point 和完整性校验。
   - 确认没有 Windows Sandbox/Harness 运行并持有独占切换锁。
   - 将旧 `test\input` 原子重命名到未映射的历史目录。
   - 将已验证 staging 原子重命名为固定 `test\input`。
   - `.wsb` 始终只引用固定 `test\input`，不引用 generation 目录。
2. **改用固定 JSON 跨进程协议。**
   - 父 Runner 为每个场景写严格验证的 request JSON。
   - 独立 Driver 只接收 request/result 文件路径和固定参数。
   - Driver 输出严格 schema 的 result JSON。
   - 禁止跨进程传递 ScriptBlock、Hashtable 或任意命令文本。
   - 所有产品、MSI、WebView 和子进程操作由版本化 Driver 内部的固定实现完成。

修正后的计划必须重新通过：

- Gate 1：符合批准设计、文件边界和 28 场景范围。
- Gate 2：竞态、PID/进程树、超时、manifest 排序、PowerShell 5.1 兼容、schema 验证、reparse-point、防旧证据复用和清理策略均可实现。

### 第二步：按任务逐个实现

推荐使用全新 Worker 逐任务实施，每个任务后进行 Gate 1/Gate 2，不要对这个安全敏感 Harness 采用一次性内联实现。产品代码不是本轮修改范围；若测试暴露产品缺陷，应另建独立任务。

### 第三步：重新做静态安全预检

在启动 Sandbox 前，至少确认：

- `test\input` 是只读映射。
- 只有当前 run 的空 `sandbox-output` 可写。
- `control` 和 run container 未映射。
- 两个宿主路径不相等、不互为父子、没有 reparse point。
- 离线/在线配置的网络状态正确。
- 全部设备重定向按设计关闭。
- 输入 manifest 和文件哈希一致。
- 旧 Harness 不再作为 fallback。

任一检查失败都必须在启动前停止。

### 第四步：执行 Windows 10 场景

每轮使用新的 run ID 和证据目录。只有 `complete.json`、控制记录、日志、截图、结果、时间窗、manifest/config/source revision 全部互相匹配时才可记为 PASS。部分输出、超时、Sandbox 崩溃、清理失败或陈旧结果必须记为 FAIL/BLOCKED，不能复用旧证据。

### 第五步：准备两个 macOS 一次性环境

分别准备 Intel 和 Apple Silicon 环境执行各自 9 个场景。静态哈希、CI、DMG 架构检查只能作为辅助证据，不能提升交互场景状态。

### 第六步：更新最终验收记录

历史文件 `docs/superpowers/verification/2026-07-13-installation-orchestrator.md` 记录的是较早提交 `b58433de` 的本地验证，当时 Rust 编译和平台验证尚未执行。接手人不能只看该文件判断当前状态；应将最终 CI、Windows Sandbox 和 macOS 实机证据写入新的或更新后的验证报告，并清楚标注证据对应的提交和环境。

## 八、安全与操作禁令

- 不要在宿主机直接运行 MSI、Portable EXE 或 WebView2 安装器。
- 不要运行当前外部 `agent-manager-test.wsb`。
- 不要执行当前 BLOCKED 的实施计划。
- 不要触碰或清理 dirty main checkout。
- 不要把仓库公开、触发 Actions、运行 `release.yml` 或发布 Release；这些都不是当前 Harness 工作所需。
- 不要把静态检查、单元测试或 CI 打包称为 28 个交互场景通过。
- 不要采用“寻找最新 evidence 目录”的策略；必须使用明确返回的 run ID 和未映射控制记录。
- 不要在日志、结果或诊断中输出密钥、密钥子串、长度、哈希或派生值。

## 九、关键文档索引

- 原始实施计划：`docs/superpowers/plans/2026-07-13-agent-manager-installation-orchestrator.md`
- 实施进度账本：`.superpowers/sdd/progress.md`
- 历史本地验证：`docs/superpowers/verification/2026-07-13-installation-orchestrator.md`
- 已批准 Sandbox 加固设计：`docs/superpowers/specs/2026-07-14-windows-sandbox-harness-hardening-design.md`
- 当前禁止执行的 Sandbox 计划：`docs/superpowers/plans/2026-07-14-windows-sandbox-harness-hardening.md`
- 本交接说明：`docs/tasks/2026-07-14-installation-orchestrator-handoff.md`

## 十、接手人开始前检查清单

- [ ] 确认当前目录是权威 feature worktree，而不是 dirty main。
- [ ] 确认产品代码基线 `ef856ca9` 与最终 CI 证据一致。
- [ ] 阅读已批准设计，不执行当前 BLOCKED 计划。
- [ ] 先把计划修正为固定 `test\input` 原子替换。
- [ ] 把 Driver 改为固定 JSON request/result 协议。
- [ ] 让修正计划重新通过规格和安全质量双 Gate。
- [ ] 实现并静态验证新的版本化 Harness。
- [ ] 重新执行宿主写入边界预检。
- [ ] 仅在预检通过后启动 Windows Sandbox。
- [ ] 逐项完成 Windows 10 个场景并保存新证据。
- [ ] 在真实 Intel 与 Apple Silicon macOS 环境完成 18 个场景。
- [ ] 更新最终验收报告后再讨论合并或发布。
