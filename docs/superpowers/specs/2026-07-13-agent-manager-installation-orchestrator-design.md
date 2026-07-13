# Agent Manager 统一安装编排器设计

## Background

Agent Manager 当前已经支持多种 Agent CLI 的检测、安装、升级、批量操作、安装位置诊断，以及 Windows、macOS 和 WSL 环境识别。现有安装链路仍以拼接并执行工具安装命令为主，默认 Node.js、npm、Shell、PATH、网络和目录权限等前置条件已经可用。

真实客户环境中可能出现 Node.js/npm 缺失、版本过旧、PATH 未刷新、多个安装位置冲突、权限不足、代理或 TLS 故障等问题。当前链路容易把这些根因统一表现为原始命令失败，普通用户难以理解，应用也无法自动完成修复、重试与最终验证。

## Goal

在现有 Agent Manager 内实现统一安装编排器，为 Windows 原生和 macOS 提供从环境检测、修复计划、用户授权、依赖安装、目标工具安装到最终可运行验证的完整闭环。

成功标准不是安装命令退出码为零，而是目标 CLI 能在 Agent Manager 实际使用的环境中被正确解析、启动并返回有效版本。

## Non-Goals

- 本次不改造 WSL 的自动修复与依赖安装链路，现有 WSL 行为保持不变。
- 不重做 Agent Manager 的视觉体系或另建独立安装器应用。
- 不静默卸载未知的 Node.js、npm 或 Agent CLI 安装。
- 不迁移或删除用户现有的 npm 全局包。
- 不绕过 Windows Defender、macOS Gatekeeper、TLS 校验、代码签名或系统授权机制。
- 不通过降低 PowerShell 执行策略或扩大目录权限来掩盖安装问题。
- 不在本次工作中重构与工具生命周期无关的版本查询、设置或代理功能。

## Scope

### Supported Platforms

- Windows 原生，覆盖受 Agent Manager 支持的 x64 和 ARM64 目标。
- macOS，覆盖 Apple Silicon 和仍受产品支持的 Intel 目标。
- WSL 继续使用现有路径，但不进入新的依赖自动修复流程。

### Supported Lifecycle Actions

- 单个 Agent CLI 安装。
- 批量 Agent CLI 安装。
- 安装前共享环境检测与依赖修复。
- 安装后的路径、版本、架构和可执行性验证。
- 与现有升级及多安装位置诊断能力兼容。

### Shared Dependencies

第一阶段的共享依赖重点是 Node.js 和 npm。架构必须允许后续以同一模型增加 Git、Python 或其他依赖，但本次不预先实现没有现有工具需求的依赖管理器。

## Constraints

- 所有系统级变更必须先展示具体计划并获得用户确认。
- UAC、macOS 密码授权和系统安装器必须由操作系统正常呈现。
- 下载的 Node.js 安装包必须来自受信来源，并验证签名、架构和完整性。
- 检测、计划、执行和验证必须是独立阶段，执行前必须存在确定的修复计划。
- UI 优先复用现有工具卡片、生命周期页面、确认弹窗、进度展示、批量结果和诊断入口。
- 仅在现有组件无法表达环境检查、修复计划或步骤状态时增加最小的新 UI。
- 批量任务中的单个工具失败不得阻止其他独立工具继续执行。
- 日志必须保留诊断价值，同时脱敏令牌、API Key、代理凭据和敏感环境变量。

## Proposed Approach

### 1. Unified Installation Orchestrator

新增统一的 `InstallationOrchestrator`，以显式状态机管理安装生命周期：

```text
request
  -> preflight probe
  -> repair plan
  -> privilege confirmation
  -> dependency repair/install
  -> environment refresh and re-probe
  -> tool install
  -> postflight verification
  -> structured result
```

每一步产生结构化状态和日志，不依赖前端从 npm、Shell 或系统安装器的自然语言输出中反推当前状态。

### 2. Module Boundaries

后端安装逻辑从持续扩大的 `misc.rs` 中按职责迁移到独立模块：

```text
src-tauri/src/installer/
  mod.rs
  orchestrator.rs
  model.rs
  probe.rs
  repair.rs
  verifier.rs
  failure.rs
  platform/
    windows.rs
    macos.rs
  tools/
    claude.rs
    codex.rs
    gemini.rs
    opencode.rs
    openclaw.rs
    hermes.rs
```

- `EnvironmentProbe`：检测系统、架构、磁盘、网络、代理、Node.js、npm、PATH、Shell、权限和安装冲突。
- `RepairPlanner`：将检测结果转换为可审查的修复动作列表。
- `PlatformAdapter`：封装 Windows/macOS 的安装器、提权、PATH 和进程环境差异。
- `DependencyManager`：管理 Node.js/npm 的检测、安装、升级和修复。
- `ToolInstallStrategy`：描述各工具的依赖、首选方式、备用方式、命令入口和验证方式。
- `InstallationVerifier`：验证实际命中路径、版本、退出码、架构和冲突状态。
- `FailureClassifier`：把原始失败映射为稳定的结构化错误类型。

各模块通过数据模型通信，避免平台分支、工具分支和 UI 文案互相耦合。

### 3. Core Task Model

每个安装任务至少包含：

- `task_id`
- 目标工具和生命周期动作
- 当前阶段及进度
- 环境检测快照
- 修复计划及每个动作的权限级别
- 用户授权状态
- 已执行动作和结果
- 安装后验证结果
- 结构化错误
- 脱敏日志位置
- 最终状态

最终状态限定为：

- `succeeded`
- `succeeded_with_conflicts`
- `installed_not_runnable`
- `cancelled_by_user`
- `needs_user_action`
- `failed`

批量安装共享一次环境检测和共享依赖修复，随后按工具隔离执行和汇总结果。

### 4. Environment Detection

安装前统一检测：

- 操作系统版本和 CPU 架构是否受支持。
- 系统盘、临时目录和目标目录是否有足够空间。
- npm Registry、工具官方站点和配置代理是否可连接。
- Node.js 是否存在、版本是否受支持、架构及实际命中路径。
- npm 是否存在、能否执行、版本、Registry、prefix 和缓存目录。
- Agent Manager 当前进程、登录 Shell 与新子进程可见的 PATH。
- npm prefix、缓存、临时目录及目标目录是否可写。
- Node.js、npm 和目标 CLI 是否存在多个安装位置。
- 目标文件或 CLI 是否被进程占用。
- Windows Defender、Gatekeeper、执行策略或签名校验是否阻止操作。

网络失败必须区分 DNS、连接超时、连接重置、代理不可达、TLS/证书错误和 HTTP 状态错误，不统一归为下载失败。

### 5. Node.js and npm Repair Policy

按以下优先级处理：

1. Node.js 和 npm 均正常时直接进入目标工具安装。
2. Node.js 正常而 npm 仅因 PATH 不可见时，修复或刷新环境后重新检测。
3. Node.js 存在但 npm 文件缺失或无法执行时，修复同一套 Node.js 安装。
4. Node.js 版本过低或架构错误时，生成升级或替换计划并请求授权。
5. Node.js 完全缺失时，下载安装经过产品验证的受支持 LTS。
6. 检测到多套 Node.js 时，默认保持当前命令行实际命中的安装，不静默切换或卸载。
7. 修复后必须在干净的新进程环境中重新验证 `node --version`、`npm --version`、路径和架构。

应用中的版本策略指定经过验证的 Node.js LTS 范围，不在 UI 文案或平台命令中散落硬编码版本。

### 6. Platform Behavior

#### Windows

- 使用 Node.js 官方签名 MSI，并校验来源、签名、完整性和架构。
- 系统级安装或 PATH 修改前展示变更，通过标准 UAC 授权。
- 合并并刷新用户与系统环境变量，正确处理 `npm.cmd`、`npx.cmd` 和 npm 全局 bin。
- 不修改全局 PowerShell 执行策略。
- 安装器结束后重新探测实际状态，不仅依赖 MSI 返回值。

#### macOS

- 使用 Node.js 官方 PKG 作为无需预装第三方包管理器的基线方案。
- 校验安装包签名、公证状态、完整性和 CPU 架构。
- 通过系统授权流程安装，不使用 `sudo npm install -g` 作为默认修复手段。
- 同时检测 GUI 应用环境与 zsh 登录环境的 PATH。
- 识别 `/usr/local` 和 `/opt/homebrew` 以及 Intel/Apple Silicon 混装冲突。
- 安装器结束后重新探测实际状态。

### 7. Authorization and Safety

无需额外确认的动作：

- 只读环境检测。
- 刷新 Agent Manager 进程内环境。
- 清理本次任务生成的临时下载。
- 有上限的安全网络重试。
- 修复 Agent Manager 自身保存的非敏感配置。

必须展示计划并由用户授权的动作：

- 安装或升级 Node.js。
- 修改系统级 PATH。
- 调用 MSI/PKG 系统安装器。
- 修改系统目录权限。
- 替换已存在的系统级工具。

禁止自动执行的动作：

- 卸载未知的 Node.js 或 Agent CLI 安装。
- 删除用户全局 npm 包。
- 强制关闭用户进程。
- 绕过系统安全、签名或证书校验。
- 静默降低安全策略。
- 在多套 Node.js 环境间擅自迁移全局包。

### 8. Agent Manager UI Integration

继续使用现有 `AgentLifecyclePage` 作为入口，不新增独立安装器窗口或独立产品流程。

优先复用：

- 工具卡片和单项安装/升级入口。
- 批量操作入口及部分失败汇总。
- 现有确认弹窗的版式和交互。
- 现有进度、状态徽标、错误提示和诊断入口。
- 多安装位置诊断及实际命中路径展示。

需要补充的最小 UI 状态：

- 安装前环境检查结果。
- 系统变更和授权预览。
- 检测、修复、安装、验证的分步骤进度。
- 结构化失败原因、推荐动作和脱敏报告导出。

原始终端输出默认折叠，用户可展开查看。前端关闭安装弹窗不终止后端任务；退出应用时若仍有系统修改或安装任务运行，必须提示用户。

### 9. Cancellation and Recovery

- 检测阶段可立即取消。
- 下载阶段取消时终止下载并清理本次临时文件。
- 系统安装器启动后不强制终止，等待其结束或由用户在系统界面取消，再重新检测实际状态。
- 工具安装子进程取消后，保留已经产生的安装状态并重新验证。
- Agent Manager 异常退出后，下次启动根据任务记录和当前系统状态重新检测，不盲目续跑旧命令。
- PATH 修改失败时恢复执行前保存的值。
- Node.js 安装失败时重新检测，不假设安装完全成功或完全失败。
- npm 安装失败不回滚仍然健康的 Node.js。
- CLI 已落盘但验证失败时进入 `installed_not_runnable`，显示实际路径和启动错误。
- 管理员授权被拒绝归类为用户取消，不显示为系统故障。

### 10. Logging and Diagnostics

每次任务生成稳定任务 ID，记录阶段、时间、动作、目标路径、退出码、分类错误和验证结果。日志必须对以下内容脱敏：

- API Key 和认证令牌。
- URL 中的用户名、密码和敏感查询参数。
- 代理凭据。
- 已知敏感环境变量。
- 原始命令中可能携带的密钥参数。

失败界面展示用户可理解的标题、失败步骤、检测原因、已执行变更、未执行动作和推荐修复入口，同时允许复制脱敏诊断报告。

## Error Handling

稳定错误分类至少包括：

- `unsupported_platform`
- `unsupported_architecture`
- `insufficient_disk_space`
- `dependency_missing`
- `dependency_too_old`
- `dependency_broken`
- `path_not_visible`
- `multiple_installations`
- `permission_denied`
- `privilege_declined`
- `file_in_use`
- `dns_failure`
- `network_timeout`
- `proxy_unreachable`
- `tls_failure`
- `download_integrity_failure`
- `signature_verification_failure`
- `installer_failure`
- `tool_install_failure`
- `verification_failure`

每个错误包含稳定代码、用户文案键、失败阶段、可重试性、是否需要用户操作、推荐动作和脱敏技术详情。未知错误保留原始退出码与日志，但不得错误归类为依赖缺失。

## UI Behavior

安装前展示检测摘要；只有涉及提权或系统变更时才增加确认步骤。确认页明确列出将安装或升级的依赖、目标版本范围、安装位置、PATH 变化、预计下载量和是否需要管理员授权。

安装中展示步骤状态：系统检测、依赖下载与验证、依赖安装、环境刷新、工具安装和最终验证。安装成功页展示工具版本、实际路径和运行环境。成功但存在冲突时保留成功结果，同时指出命令行可能命中其他安装。

不得为了新流程复制已有工具卡片、对话框或诊断页面。优先扩展已有组件的 props 和状态模型；只有职责确实不同且无法清晰复用时才新增组件。

## Risks

- Node.js 官方安装包版本、下载地址或签名链变化可能导致依赖安装中断。
- Windows UAC 和 macOS Installer 的外部状态不完全受应用控制，取消和异常退出必须依赖重新检测收敛。
- GUI 应用与登录 Shell 的 PATH 差异可能导致安装后短暂不一致。
- 多套 Node.js、版本管理器和包管理器并存时，自动选择错误环境可能造成工具安装到非预期位置。
- 错误分类过度依赖英文输出会脆弱，因此必须优先使用退出码、文件状态、网络错误类型和复检结果。
- 实机测试矩阵较大，必须使用一次性 VM 或专用测试设备，避免修改开发者的全局环境。
- 将安装逻辑从 `misc.rs` 拆出时存在生命周期回归风险，需要保留兼容入口并分阶段迁移。

## Acceptance Criteria

- 全新受支持的 Windows/macOS 设备无需用户理解 Node.js 或 npm，即可完成支持工具的安装。
- Node.js/npm 缺失、损坏、过旧、架构错误或 PATH 不可见时，应用能检测、形成修复计划并完成授权后的修复。
- 所有系统级变更在执行前明确展示并获得用户授权。
- 安装成功必须同时验证命令路径、有效版本、CPU 架构和可执行性。
- 网络、代理、TLS、权限、磁盘、文件占用、签名和多版本问题以结构化原因展示。
- 批量安装共享环境检测和依赖修复，单项失败保持隔离并形成汇总。
- Node.js/npm 正常的用户不经历不必要的依赖安装或提权步骤。
- 不删除未知安装、不强制关闭用户进程、不降低系统安全策略。
- 日志和诊断报告不泄露令牌、API Key 或代理凭据。
- 现有工具卡片、生命周期页面、批量操作、冲突诊断和 WSL 路径保持兼容。
- UI 复用现有组件，只增加表达新状态所需的最小界面。

## Task Breakdown

1. 定义安装任务、检测结果、修复计划、结构化错误和验证结果模型。
2. 从现有生命周期逻辑中提取工具策略与通用安装编排接口。
3. 实现 Windows/macOS 环境检测及测试替身。
4. 实现 Node.js/npm 版本策略、修复计划和平台安装适配器。
5. 实现系统变更授权、任务状态机、取消和恢复语义。
6. 实现目标工具安装策略与安装后验证。
7. 实现错误分类、日志脱敏和诊断报告。
8. 将新任务状态接入现有 Tauri API 与事件通道，并保留兼容入口。
9. 扩展现有 Agent Manager UI 以展示检测、确认、进度和结果。
10. 完成单元测试、模拟集成测试和 Windows/macOS 实机验证。

## Verification

### Unit Tests

- 环境检测结果到修复计划的确定性映射。
- Node.js/npm 缺失、过旧、损坏、PATH 不可见和多安装场景。
- 平台权限级别、用户取消和禁止动作约束。
- 网络、权限、安装器和验证错误分类。
- 日志与诊断报告脱敏。
- 批量任务共享依赖且单项失败隔离。

### Integration Tests

使用伪造的 `node`、`npm`、系统安装器和工具 CLI 覆盖完整状态机，不修改测试机的真实全局环境。验证事件顺序、取消、重启恢复、PATH 刷新、失败重试和最终状态。

### Windows Matrix

- 无 Node.js/npm。
- Node.js 正常但 npm 不在 PATH。
- Node.js 版本过旧或架构不匹配。
- 多套 Node.js/npm。
- UAC 接受与拒绝。
- npm Registry 超时、代理错误和 TLS 错误。
- npm prefix 或缓存目录无权限。
- CLI 安装后 PATH 尚未刷新。
- CLI 已落盘但无法运行。
- 磁盘不足、文件占用和批量部分失败。

### macOS Matrix

- Apple Silicon 与 Intel。
- 无 Node.js/npm。
- PKG 安装成功、失败与授权拒绝。
- GUI PATH 与 zsh PATH 不一致。
- `/usr/local` 与 `/opt/homebrew` 冲突。
- npm 目录被 root 所有。
- Gatekeeper、签名或公证验证失败。
- 代理、TLS、磁盘不足和批量部分失败。

实机验证必须在干净、可重置的 Windows VM 和 macOS 测试环境中执行，并记录系统版本、架构、安装前后路径、依赖版本、工具版本、授权结果和失败证据。
