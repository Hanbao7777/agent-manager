# Agent Manager 裁剪设计

## Background

Agent Manager 基于 CC Switch 3.16.5（提交
`50270d5e42c7f7c0e816ad8f33103bd4e12b72dd`）进行裁剪和品牌包装。CC
Switch 已经具备目标产品需要的 Agent 检测、安装、升级与诊断能力，因此本项目不重写
Agent 生命周期引擎，只保留现有能力并移除无关功能。

## Goal

将 CC Switch 裁剪为名为 **Agent Manager** 的单页桌面应用，让用户直接使用 CC
Switch 现有界面管理 Claude Code、Codex、Gemini CLI、OpenCode、OpenClaw 和
Hermes。

## Non-Goals

- 不重新设计或重写 Agent 检测、安装、批量安装、升级及诊断流程。
- 不开发新的 CLI、生命周期核心、任务注册表或 Sandbox。
- 不保留 Provider、代理、路由、MCP、Skills、Prompts、Sessions、Workspace 或用量
  统计功能。
- 第一阶段不建设 Agent Manager 自身的在线更新服务。
- 不自动修改用户 PATH，也不自动删除重复安装。

## Scope

### Retained

- CC Switch 现有 Agent 生命周期界面及其弹窗和状态反馈。
- 六种 Agent 的本地版本检测及最新版本查询。
- 单项安装、批量安装缺失项和升级。
- Windows、macOS、WSL 与 Shell 处理。
- 多安装位置、PATH 冲突及已安装但无法运行的诊断。
- 生命周期链路依赖的 Tauri API、基础设置、日志、通用 UI、主题和国际化能力。
- 上游 MIT License 和必要的来源声明。

### Removed or Disconnected

- Provider 切换、Provider 账户及配置管理。
- 代理、路由、故障转移和用量统计。
- MCP、Skills、Prompts、Sessions 和 Workspace。
- OpenClaw 和 Hermes 的非安装管理功能。
- Deep Link 导入、CC Switch 专属协议和自动更新入口。
- 不再被保留链路引用的数据库表、Tauri commands、Rust 模块、前端代码、依赖、
  翻译及品牌资源。

### Shared-Boundary Decisions

- `AboutSection` 同时包含 Agent 生命周期 UI 和应用自身更新 UI。只复用其生命周期
  区域；`checkUpdate`、`install_update_and_restart` 及对应状态和控件必须移除，不能
  将整个组件不加区分地作为主页面。
- OpenClaw 和 Hermes 在 `misc.rs` 中的检测和安装定义属于保留链路；其配置面板、
  hooks 和非安装管理命令属于排除范围。删除前必须形成共享类型和调用方清单。
- `DatabaseUpgrade.tsx` 对 `install_update_and_restart` 的调用属于应用自更新链路。第一
  阶段不提供替代更新器，因此应连同依赖它的升级阻断界面一起断开或删除，而不是保留
  一个失效按钮。

## Constraints

- 以删除或断开功能为主，不以重写替代现有实现。
- 现有生命周期行为没有已确认缺陷时，不改变其命令、数据流和错误语义。
- 目标平台为 Windows 和 macOS；保留生命周期代码现有的 WSL 支持。
- 产品名称统一为 `Agent Manager`。
- Agent Manager 不得继续访问 CC Switch 官方自动更新地址。
- 保留 MIT License 与上游归属信息，不把归属声明误删为品牌残留。
- 现有未提交删除必须保持原状，不得在裁剪中意外恢复或混入无关提交。

## Proposed Approach

采用入口优先、分层删除的低风险方案。

### 1. Disable the Upstream Update Path

在任何可运行的 Agent Manager 构建产生前，先移除 CC Switch 官方更新端点、前端更新
入口、`install_update_and_restart` 等更新命令注册、updater 插件初始化以及依赖该命令
的数据库升级阻断界面。清理 updater 签名和产物配置，确保应用无法下载或安装 CC
Switch 官方发行物。

### 2. Establish the Single Product Surface

将当前位于设置/关于区域的 Agent 生命周期界面提升为应用唯一主页面。首批变更直接
复用现有生命周期实现，只做必要的组件边界拆分、容器适配和文案替换，不重新设计
Agent 卡片、批量操作、Shell 选择或诊断交互。拆分只负责把 Agent 生命周期区域与应用
自更新区域分开，不改变生命周期数据流。

主页面保留 CC Switch 已有的：

- Agent 安装状态和版本展示；
- 单项安装与升级；
- 批量安装缺失项；
- 安装冲突诊断；
- Windows/WSL/Shell 选择；
- 安装和升级错误反馈。

### 3. Disconnect Excluded Features

从主入口、导航、启动副作用和前端状态中断开排除功能。后端启动流程需要逐项处理数据库、
`AppState`、代理状态、用量 worker、Deep Link、托盘和迁移，确认保留链路的真实依赖后
再移除初始化。此阶段允许不可达代码暂时存在，以便先验证生命周期主链路没有受损。

### 4. Delete Unreachable Frontend and Backend Code

在唯一主页面通过验证后，按功能域删除不可达的 React 组件、hooks、API 封装、Tauri
command 注册、Rust 模块和数据库能力。删除边界以实际引用关系和编译结果为依据，不能
仅凭目录名批量删除。

Deep Link 必须作为完整功能域移除，包括前端入口、Rust handlers、插件初始化、Cargo
依赖、Tauri scheme 配置和相关 commands，不能只删除界面引用。

### 5. Clean Dependencies and Brand the Product

删除无引用的前端与 Rust 依赖、翻译和资源；将应用名称、包描述、窗口标题、Bundle
标识、协议和图标替换为 Agent Manager 对应配置。品牌清单至少覆盖 `package.json`、
`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、updater/签名元数据、Bundle
identifier、图标、窗口标题、前端文案和协议标识。以后只能接入 Agent Manager 自己
签名的发布产物。

## Architecture and Data Flow

```text
Agent Manager main window
    -> existing lifecycle UI
    -> existing settings API bindings
    -> retained Tauri lifecycle commands
    -> CC Switch detection / install / update / diagnostics implementation
    -> operating-system shell and official Agent distribution channels
```

生命周期界面只通过现有 Tauri API 调用后端。裁剪不得新增第二套命令生成器、Agent
清单或安装状态来源，避免出现双重真相来源。

## Error Handling

- 保留现有安装和升级失败信息，包括 stdout/stderr 尾部摘要。
- 批量操作中单个 Agent 失败不得阻断其他 Agent。
- 多安装位置或 PATH 冲突继续走现有确认与诊断流程。
- 网络、npm、Shell 或运行环境错误继续使用现有错误语义。
- 高级环境信息保持现有行为；默认不新增强制暴露的技术细节。
- 裁剪期间发现生命周期链路仍依赖某项设置或模块时，先保留并记录依赖，不能为满足
  删除数量而重写或伪造替代实现。

## Risks

- 当前前后端功能交叉引用较多，目录级删除可能误伤生命周期命令或启动流程。
- 自动更新地址、Bundle 标识或签名配置残留可能使产品更新为完整 CC Switch 或造成
  更新失败。
- OpenClaw、Hermes 的安装管理与其扩展管理代码可能共享类型或设置，需要按引用拆除。
- `src-tauri/src/lib.rs` 的启动副作用可能在界面入口删除后仍运行代理、用量、迁移、
  数据库、Deep Link 或托盘逻辑，造成排除功能仍有运行时影响。
- 大量现有未提交删除会增加审查噪声，实施时必须保持每批改动边界清晰。
- Windows、macOS 和 WSL 的行为无法只靠单一开发环境完整验证，需要平台构建或人工
  冒烟测试。

## Acceptance Criteria

- 应用启动后只展示 CC Switch 现有 Agent 生命周期管理界面。
- Claude Code、Codex、Gemini CLI、OpenCode、OpenClaw 和 Hermes 均可检测状态。
- 单项安装、批量安装缺失项、升级和安装冲突诊断入口保持可用。
- Windows、macOS 与现有 WSL/Shell 行为没有被主动改变。
- Provider、代理、路由、用量、MCP、Skills、Prompts、Sessions 和 Workspace 不再有
  可达界面或已注册的专属 Tauri commands。
- 产品可见名称和应用元数据统一为 Agent Manager；MIT License 和上游来源声明除外，
  产品界面及发行物不再以 CC Switch 作为产品名。
- 应用不再配置或请求 CC Switch 官方自动更新端点。
- 应用不再初始化 updater 或 Deep Link 插件，不再注册排除功能的启动任务、后台 worker
  或专属 Tauri commands。
- `package.json`、`Cargo.toml`、`tauri.conf.json`、Bundle identifier、窗口、图标、
  协议和产品文案完成品牌残留检查。
- TypeScript 类型检查、前端单元测试、React 生产构建和 Rust `cargo check` 通过。
- Windows 与 macOS 至少分别完成一次构建或可重复的人工冒烟验证。

## Task Breakdown

1. 禁用 CC Switch 更新端点、插件、命令、签名配置和依赖更新命令的界面。
2. 从 `AboutSection` 拆出并复用现有生命周期区域，将其设为唯一主页面。
3. 完成 Agent Manager 名称、应用元数据及最低限度品牌替换。
4. 断开排除功能的前端入口与后端启动副作用。
5. 删除不可达前端代码、对应 Tauri commands、Rust 模块和数据库能力。
6. 完整移除 Deep Link，并按依赖映射拆除 OpenClaw/Hermes 非生命周期功能。
7. 清理无引用依赖、翻译、资源、协议和构建配置。
8. 完成跨平台构建、生命周期冒烟验证和残留品牌、插件及后台任务扫描。

## Verification

每个裁剪批次至少运行：

```bash
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo check --manifest-path src-tauri/Cargo.toml
```

涉及平台命令或打包配置的批次还需在对应平台运行 Tauri 构建或人工冒烟测试，并记录：

- 六种 Agent 的检测结果；
- 至少一个未安装 Agent 的安装流程；
- 至少一个已安装 Agent 的升级或无更新流程；
- 批量安装缺失项的独立失败处理；
- 多位置/PATH 冲突诊断；
- 应用启动和网络请求中不存在 CC Switch 官方更新端点。
- 应用启动日志和进程行为中不存在排除功能的后台 worker 或插件副作用；
- 发行配置中不存在 `ccswitch` Deep Link scheme、官方 updater 公钥或官方更新 URL。
