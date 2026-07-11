# Agent Manager Windows 内部构建设计

## Background

Agent Manager 的功能裁剪和 Linux 无 Bundle 构建已经完成。当前产物是 Linux ELF，
不能在 Windows 直接运行。下一阶段需要建立可重复的 Windows 构建与一次性沙箱验证
流程，生成内部测试用的 Windows 安装包和便携包。

## Goal

通过私有 GitHub 仓库和 GitHub Actions Windows Runner 构建 Agent Manager 0.1.0
的未签名 MSI 与便携版 ZIP，并在 Windows Sandbox 中验证核心 Agent 生命周期功能。

## Non-Goals

- 不配置 Windows 代码签名证书。
- 不创建 GitHub Release，也不公开分发产物。
- 不恢复应用自动更新。
- 不在真实用户环境执行 Agent 全局安装、升级或 PATH 修改。
- 不在 WSL/Linux 中把交叉编译结果当作 Windows 实机验证。
- 本阶段不包含 macOS 构建。

## Scope

### Repository and Remotes

- 创建名为 `agent-manager` 的私有 GitHub 仓库。
- 将私有仓库配置为 `origin`。
- 将 `https://github.com/farion1231/cc-switch.git` 配置为 `upstream`。
- 保留现有 CCSwitch Git 历史、MIT License 和上游归属。
- 推送前必须先整理现有未提交文档删除，确保每个提交边界明确。

### Windows CI

工作流运行于 GitHub 托管的 Windows Runner，支持手动触发，并在目标分支更新时按明确
规则触发。工作流必须：

- 安装固定主版本的 Node、Corepack/pnpm 和 Rust MSVC 工具链；
- 使用锁文件安装依赖；
- 运行格式、类型、前端测试、Renderer 构建、Cargo 格式、检查和测试门禁；
- 构建 Tauri Windows MSI；
- 生成便携版 `agent-manager.exe` ZIP；
- 计算 SHA-256 校验值；
- 只上传 GitHub Actions Artifact，不创建 Release。

### Artifacts

产物名称统一为：

- `Agent-Manager-0.1.0-Windows.msi`
- `Agent-Manager-0.1.0-Windows-Portable.zip`
- `SHA256SUMS.txt`

产物未签名，内部测试人员可能看到 Windows“未知发布者”提示。

### Windows Sandbox Verification

测试人员从 GitHub Actions 下载 Artifact，并在 Windows Sandbox 或一次性 Windows
虚拟机中验证：

- MSI 安装、启动和卸载；
- 便携版启动；
- 六种 Agent 的未安装、已安装、损坏和可升级状态；
- 单项安装/升级、批量操作与单项失败后继续执行；
- 多安装位置和 PATH 冲突诊断；
- Windows 原生 Shell 与 WSL Shell 参数转发；
- 应用不访问 CCSwitch 更新源、不注册 Deep Link、不启动已排除服务。

真实安装命令只允许在一次性沙箱/VM 中执行。CI 中必须使用单元测试、命令规划测试或
模拟可执行文件，不能改变 Runner 或用户的全局 Agent 环境作为必需验收步骤。

## Constraints

- GitHub 仓库必须保持私有。
- 第一版产物不签名。
- 构建产物只存放于 GitHub Actions Artifact。
- 使用 Agent Manager `0.1.0`、Bundle ID `com.agentmanager.desktop`。
- Windows 构建不得重新引入 updater、Deep Link 或已排除的 CCSwitch 功能。
- 保留现有六 Agent 生命周期命令与错误语义，不开发替代安装引擎。
- Secrets、令牌和用户配置不得写入仓库、日志或 Artifact。
- 工作流必须可重复运行，并明确 pin 关键工具的主版本。

## Proposed Approach

### 1. Close the Local Verification Gap

先修复现有 Prettier 失败，重新运行 Linux 可执行的所有门禁。将当前继承的文档删除作为
单独提交处理，使工作区在创建远端前保持干净。

### 2. Establish the Private Remote

通过 GitHub 私有仓库保存当前历史。原 `origin` 改名为 `upstream`，新私有仓库设为
`origin`。首次推送前检查远端 URL 和分支，避免向 CCSwitch 官方仓库推送。

### 3. Build on a Native Windows Runner

Windows 工作流从零安装工具链并运行完整质量门禁，随后调用 Tauri 构建。MSI 使用
Tauri/WiX 现有配置生成；便携 ZIP 从 release 可执行文件和最低运行所需资源组装。

### 4. Verify in a Disposable Windows Environment

将 Artifact 下载到 Windows Sandbox。测试报告记录 Windows 版本、构建 Run ID、产物
校验值、每个场景的结果和截图/日志位置。失败场景进入修复—CI—沙箱复测循环。

## Error Handling

- 任一质量门禁失败时不得上传标记为候选的产物。
- 构建或打包步骤失败时保留必要诊断日志，但不得打印密钥和用户配置。
- MSI 与便携包任一缺失时整次构建失败。
- SHA-256 生成或 Artifact 上传失败时整次构建失败。
- Sandbox 测试发现安装器修改宿主环境时立即停止并废弃该候选包。
- 未签名导致的 SmartScreen 提示必须记录为已知限制，不能误报为构建失败。

## Risks

- Windows 未签名安装包会出现未知发布者或 SmartScreen 提示。
- Windows Runner、Tauri、WiX 或 Rust MSVC 版本变化可能破坏可重复构建。
- 便携版可能依赖 WebView2 Runtime；测试报告需记录目标环境是否已安装。
- Windows Sandbox 默认不持久化状态，测试素材和结果需在关闭前导出。
- GitHub 私有仓库和 Actions 用量受账户权限及配额限制。
- 真实 WSL 行为仍需要 Windows 主机已启用 WSL；Sandbox 本身可能不具备完整 WSL
  条件，因此可使用独立一次性 Windows VM 补充验证。

## Acceptance Criteria

- 本地全部格式、类型、前端和 Rust 门禁通过。
- 当前有意文档删除形成独立提交，工作区除明确忽略文件外干净。
- 私有 GitHub 仓库存在，`origin` 指向私有仓库，`upstream` 指向 CCSwitch 官方仓库。
- Windows workflow 可手动触发并在干净 Runner 上成功完成。
- Actions Artifact 同时包含正确命名的 MSI、Portable ZIP 和 SHA256SUMS。
- MSI 和便携版均能在一次性 Windows 环境启动。
- 六 Agent 检测、安装/升级、批量失败隔离和冲突诊断通过沙箱验证。
- updater、Deep Link、旧功能后台副作用和 CCSwitch 官方更新请求均未出现。
- Windows 验证报告包含 Run ID、平台、校验值、测试结果、限制和失败证据。

## Task Breakdown

1. 修复现有格式门禁并复跑本地测试。
2. 审核并提交继承的上游文档删除。
3. 创建并连接私有 GitHub 仓库，配置 `origin`/`upstream`。
4. 添加 Windows CI 质量门禁与 Tauri MSI 构建。
5. 添加便携 ZIP、SHA-256 和 Artifact 上传。
6. 推送并触发首个 Windows 构建，修复 CI 特有问题。
7. 在 Windows Sandbox/VM 中执行安装与生命周期冒烟测试。
8. 编写 Windows 验证报告并产出 0.1.0 内部候选包。

## Verification

本地和 CI 的强制门禁：

```bash
corepack pnpm format:check
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Windows 工作流额外验证 MSI、便携 ZIP 和 SHA-256 文件存在且非空，并上传 Artifact。
Windows Sandbox/VM 的人工验证结果写入
`docs/superpowers/verification/YYYY-MM-DD-agent-manager-windows.md`。
