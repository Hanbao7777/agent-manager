# Agent Manager macOS Universal 内部构建设计

## Background

Agent Manager 已完成产品裁剪、Windows 私有 CI 构建和 Windows 安装包基础验证。目前
Windows Artifact 不能在 macOS 运行，因此需要建立独立、可重复的原生 macOS 构建流程。
首版面向内部测试，不配置 Apple Developer 证书，但要求一个安装包同时支持 Intel Mac
与 Apple Silicon Mac。

## Goal

通过私有 GitHub 仓库的 GitHub Actions macOS Runner，构建 Agent Manager `0.1.0`
的未签名 Universal macOS 应用与 DMG，并生成严格校验的私有 Actions Artifact。

## Non-Goals

- 不配置 Apple Developer ID 证书、签名或 Apple 公证。
- 不创建 GitHub Release，不公开发布或自动上传到第三方存储。
- 不恢复应用自动更新、Updater Artifact 或 Deep Link。
- 不构建 Mac App Store 包或 `.pkg` 安装器。
- 不分别向用户交付 Intel 与 Apple Silicon 两个安装包。
- 不在 GitHub Runner 上执行真实 Agent 全局安装或升级。
- 不把 CI 构建成功等同于真实 Mac 启动验证。

## Scope

### macOS CI

新增独立的 macOS 内部构建工作流。工作流必须支持手动触发，并在 `main` 更新时触发；
使用 GitHub 托管的 `macos-14` Runner，固定 Node、Corepack/pnpm 和 Rust 版本，并安装
`aarch64-apple-darwin`、`x86_64-apple-darwin` 两个 Rust target。

工作流必须运行：

- 冻结锁文件的依赖安装；
- 前端格式检查、TypeScript 类型检查、前端单元测试与 Renderer 构建；
- Rust 格式检查、检查与测试；
- Tauri `universal-apple-darwin` 原生构建；
- Universal 架构、产物名称、文件数量与 SHA-256 校验；
- 单一私有 Actions Artifact 上传。

### Artifacts

Actions Artifact 名称为 `Agent-Manager-0.1.0-macOS-Universal-internal`，保留 7 天，
根目录严格包含：

- `Agent-Manager-0.1.0-macOS-Universal.dmg`
- `SHA256SUMS.txt`

`SHA256SUMS.txt` 必须只包含一行小写 SHA-256，文件名必须与 DMG 完全一致。工作流还要
验证 DMG 非空，并验证构建出的 `.app` 主可执行文件同时包含 `arm64` 和 `x86_64`
架构；缺少任一架构即失败。

### macOS Runtime Verification

构建完成后，在真实 Mac 或一次性 macOS VM 中验证：

- SHA-256 与 Artifact 清单匹配；
- DMG 可挂载并展示 Agent Manager；
- `.app` 可复制到 `/Applications` 或一次性测试目录；
- 未签名应用通过右键“打开”或明确的 Gatekeeper 测试步骤启动；
- 应用标题和六个 Agent 条目正确显示；
- 检测、单项操作、批量失败隔离和多安装位置诊断不改变真实用户环境；
- 没有 CCSwitch 更新源、Updater、Deep Link 或排除的后台服务副作用。

真实 Agent 安装命令只允许在一次性 macOS 用户、VM 或可恢复快照中执行。没有真实 Mac
证据时，报告必须将运行验证标为 `BLOCKED`，不能宣称候选包已被接受。

## Constraints

- 仓库保持私有，工作流权限保持只读。
- `origin` 继续指向私有 `Hanbao7777/agent-manager`；不得推送 `upstream`。
- 产品版本保持 `0.1.0`，Bundle ID 保持 `com.agentmanager.desktop`。
- 保留现有六 Agent 生命周期实现和错误语义，不开发替代安装引擎。
- 构建必须生成真正的 Universal Binary，不接受只改文件名的单架构 DMG。
- 不引入签名 Secret、Apple 凭据、用户配置或真实 Agent Token。
- 不修改现有 Windows 工作流的产物契约。
- 关键版本不得使用不受控的 `latest` 或 `stable`。

## Proposed Approach

### 1. Separate macOS Workflow

创建 `.github/workflows/macos-internal.yml`，与 Windows 工作流隔离，避免平台命令、产物
和排障相互耦合。使用现有私有仓库和 `main`，但采用独立的并发组和 Artifact 名称。

### 2. Build Both Architectures as One Universal App

在原生 Apple Silicon runner 上安装两种 Rust target，调用 Tauri 的
`universal-apple-darwin` 构建目标。构建后使用 macOS 原生架构检查工具验证 `.app` 主
可执行文件确实同时包含 `arm64` 与 `x86_64`，再接受 DMG。

### 3. Strict Internal Artifact Contract

从 Tauri bundle 输出中要求恰好一个 DMG，将其规范化重命名，计算 SHA-256，并在上传前
重新解析清单和计算哈希。只上传 DMG 与清单，不上传 `.app`、构建缓存、日志或用户数据。

### 4. Disposable macOS Verification

Artifact 构建成功后，在真实 Mac 或可恢复 VM 上完成 Gatekeeper、挂载、复制、启动与
六 Agent 冒烟验证。观察结果写入独立验证文档；未签名限制与真实功能失败分开记录。

## Error Handling

- 任一质量门禁失败时不得上传 Artifact。
- Rust target、Universal 合并或架构验证失败时整次构建失败。
- 找不到 DMG、发现多个 DMG、DMG 为空或名称不匹配时整次构建失败。
- SHA-256 清单存在额外行、格式错误、大小写错误或哈希不匹配时整次构建失败。
- Artifact 上传失败时不得将构建视为候选成功。
- CI 特有问题只允许以最小、聚焦提交修复，不得跳过门禁或使用
  `continue-on-error` 掩盖失败。
- Gatekeeper 因未签名阻止首次双击时记录为已知限制；应用仍无法通过明确允许步骤启动则
  记为真实失败。

## Risks

- 未签名、未公证应用会触发 Gatekeeper 警告，不适合直接面向普通客户分发。
- Universal DMG 比单架构 DMG 更大，构建时间也更长。
- Intel 目标的依赖或 Rust crate 可能在 Apple Silicon runner 上暴露交叉编译问题。
- GitHub 私有仓库的 macOS Actions 分钟消耗和计费高于 Linux/Windows。
- GitHub runner 镜像变化可能影响 Xcode、DMG 工具或目标架构构建。
- 没有可用真实 Mac 或 macOS VM 时，只能完成构建验证，不能完成候选接受。

## Acceptance Criteria

- macOS workflow 可手动触发并在 `main` 推送时触发。
- 工作流在私有 GitHub 托管 macOS Runner 上完成全部前端和 Rust 门禁。
- 构建出的 Agent Manager 主可执行文件同时包含 `arm64` 与 `x86_64`。
- Actions Artifact 根目录严格只有 Universal DMG 与 `SHA256SUMS.txt`。
- DMG 名称为 `Agent-Manager-0.1.0-macOS-Universal.dmg`，非空且哈希匹配。
- 工作流不签名、不公证、不创建 Release、不配置 Updater 或 Deep Link。
- Windows workflow 与 Windows Artifact 契约保持不变。
- 真实 Mac/VM 验证报告准确记录平台、Run ID、提交 SHA、哈希、Gatekeeper 结果、六 Agent
  场景和证据；未执行的场景明确标记 `BLOCKED`。

## Task Breakdown

1. 审核现有 macOS/Tauri 配置、工具版本与 Universal 构建前置条件。
2. 添加 macOS 私有 CI 的工具链和全部质量门禁。
3. 添加 Universal `.app`/DMG 构建与双架构验证。
4. 添加规范化 DMG、严格 SHA-256 清单和单一 Artifact 上传。
5. 推送并诊断首个 macOS 构建，只做 CI 证据支持的最小修复。
6. 在真实 Mac 或一次性 macOS VM 中验证挂载、启动和六 Agent 行为。
7. 编写 macOS 验证报告并作出 `ACCEPTED`、`REJECTED` 或 `BLOCKED` 决定。

## Verification

本地和 CI 门禁：

```bash
corepack pnpm format:check
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

macOS workflow 另外验证：

- Rust 两个 Apple target 均已安装；
- `.app` 主可执行文件为 `arm64` + `x86_64` Universal Binary；
- DMG 恰好一个、非空并按契约重命名；
- Artifact 只有 DMG 与一行小写 SHA-256 清单；
- 未出现签名、公证、Release、Updater 或 Deep Link 行为。

