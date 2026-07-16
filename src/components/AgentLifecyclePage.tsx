import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Download,
  Copy,
  Loader2,
  RefreshCw,
  Terminal,
  CheckCircle2,
  AlertCircle,
  ArrowUpCircle,
  ChevronDown,
  Stethoscope,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { installerApi, settingsApi } from "@/lib/api";
import type {
  InstallPreparation,
  InstallTaskSnapshot,
} from "@/lib/api/installer";
import type {
  ToolInstallation,
  ToolInstallationReport,
} from "@/lib/api/settings";
import { motion } from "framer-motion";
import { APP_ICON_MAP } from "@/config/appConfig";
import type { AppId } from "@/lib/api/types";
import { buildWslLifecycleOverrides } from "@/lib/installEnvironment";
import { mergeInstallTaskSnapshot } from "@/lib/installTaskState";
import { extractErrorMessage } from "@/utils/errorUtils";
import { isWindows } from "@/lib/platform";
import { isUpdateAvailable } from "@/lib/version";
import { ToolUpgradeConfirmDialog } from "./settings/ToolUpgradeConfirmDialog";
import { ToolInstallRow } from "./settings/ToolInstallRow";
import { ToolInstallDialog } from "./settings/ToolInstallDialog";

interface ToolVersion {
  name: string;
  version: string | null;
  latest_version: string | null;
  error: string | null;
  // 后端已定位到可执行文件但 --version 报错（装了却跑不起来）。直接读此字段，
  // 不要靠匹配 error 文案反推——避免前端与后端字符串硬耦合。
  installed_but_broken: boolean;
  env_type: "windows" | "wsl" | "macos" | "linux" | "unknown";
  wsl_distro: string | null;
}

const TOOL_NAMES = [
  "claude",
  "codex",
  "gemini",
  "opencode",
  "openclaw",
  "hermes",
] as const;
type ToolName = (typeof TOOL_NAMES)[number];
type ToolLifecycleAction = "install" | "update";

type WslShellPreference = {
  wslShell?: string | null;
  wslShellFlag?: string | null;
};

const WSL_SHELL_OPTIONS = ["sh", "bash", "zsh", "fish", "dash"] as const;
// UI-friendly order: login shell first.
const WSL_SHELL_FLAG_OPTIONS = ["-lic", "-lc", "-c"] as const;

const ENV_BADGE_CONFIG: Record<
  string,
  { labelKey: string; className: string }
> = {
  wsl: {
    labelKey: "settings.envBadge.wsl",
    className:
      "bg-orange-500/10 text-orange-600 dark:text-orange-400 border-orange-500/20",
  },
  windows: {
    labelKey: "settings.envBadge.windows",
    className:
      "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
  },
  macos: {
    labelKey: "settings.envBadge.macos",
    className:
      "bg-gray-500/10 text-gray-600 dark:text-gray-400 border-gray-500/20",
  },
  linux: {
    labelKey: "settings.envBadge.linux",
    className:
      "bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/20",
  },
};

const posixScriptInstallCommand = (url: string) =>
  `bash -c 'tmp=$(mktemp) && curl -fsSL ${url} -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'`;

const HERMES_WINDOWS_INSTALL_SCRIPT =
  "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex";

const powershellEncodedCommand = (script: string): string => {
  let binary = "";
  for (let i = 0; i < script.length; i += 1) {
    const code = script.charCodeAt(i);
    binary += String.fromCharCode(code & 0xff, code >> 8);
  }
  return btoa(binary);
};

const HERMES_WINDOWS_INSTALL_COMMAND = `powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand ${powershellEncodedCommand(
  HERMES_WINDOWS_INSTALL_SCRIPT,
)}`;

const POSIX_ONE_CLICK_INSTALL_COMMANDS = `# Claude Code
${posixScriptInstallCommand("https://claude.ai/install.sh")} || npm i -g @anthropic-ai/claude-code@latest
# Codex
npm i -g @openai/codex@latest
# Gemini CLI
npm i -g @google/gemini-cli@latest
# OpenCode
${posixScriptInstallCommand("https://opencode.ai/install")} || npm i -g opencode-ai@latest
# OpenClaw
npm i -g openclaw@latest
# Hermes
${posixScriptInstallCommand("https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh")}`;

const WINDOWS_ONE_CLICK_INSTALL_COMMANDS = `# Claude Code
npm i -g @anthropic-ai/claude-code@latest
# Codex
npm i -g @openai/codex@latest
# Gemini CLI
npm i -g @google/gemini-cli@latest
# OpenCode
npm i -g opencode-ai@latest
# OpenClaw
npm i -g openclaw@latest
# Hermes
${HERMES_WINDOWS_INSTALL_COMMAND}`;

const ONE_CLICK_INSTALL_COMMANDS = isWindows()
  ? WINDOWS_ONE_CLICK_INSTALL_COMMANDS
  : POSIX_ONE_CLICK_INSTALL_COMMANDS;

const TOOL_DISPLAY_NAMES: Record<ToolName, string> = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
};

// 后端返回的 tool 是 string；这里收敛唯一的 ToolName 断言与兜底，供升级确认
// 对话框按工具名展示（避免在 JSX 里内联 cast、且每次渲染都新建闭包）。
function toolDisplayName(tool: string): string {
  return TOOL_DISPLAY_NAMES[tool as ToolName] ?? tool;
}

const TOOL_APP_IDS: Record<ToolName, AppId> = {
  claude: "claude",
  codex: "codex",
  gemini: "gemini",
  opencode: "opencode",
  openclaw: "openclaw",
  hermes: "hermes",
};

// 工具版本探测代价高：每个工具一次 `--version` 子进程 + 一次 npm/github/pypi 网络请求。
// 设置页用 Radix Tabs，非激活 Tab 会被卸载——每次切回「关于」都重挂 AboutSection，若都
// 全量重查纯属浪费。用「模块级」缓存（生命周期 = JS 模块 = 应用会话，不随组件卸载销毁）
// 跨重挂存活：重挂时若缓存仍新鲜（距上次全量加载 < TTL）直接复用、跳过探测；超期或用户
// 手动「刷新」才强制重查。at = 最近一次「全量加载」完成时刻；单工具刷新（切 shell / 升级
// 后）只更新数据、不重置 at，避免一次局部刷新把整体 TTL 续命。
const TOOL_VERSIONS_CACHE_TTL_MS = 10 * 60 * 1000; // 10 分钟
let toolVersionsCache: { data: ToolVersion[]; at: number } | null = null;
// 把探测结果按 name 合并进已有列表：替换同名项、追加新项；空列表时直接采用新结果。
// 组件 state 与模块缓存共用同一套合并语义（单工具与全量探测都经此函数）。
function mergeToolVersions(
  prev: ToolVersion[],
  updated: ToolVersion[],
): ToolVersion[] {
  if (prev.length === 0) return updated;
  const byName = new Map(updated.map((t) => [t.name, t]));
  const merged = prev.map((t) => byName.get(t.name) ?? t);
  const existing = new Set(prev.map((t) => t.name));
  for (const u of updated) {
    if (!existing.has(u.name)) merged.push(u);
  }
  return merged;
}

export function AgentLifecyclePage() {
  const { t } = useTranslation();
  const [toolVersions, setToolVersions] = useState<ToolVersion[]>(
    () => toolVersionsCache?.data ?? [],
  );
  // 有缓存（哪怕已超期）就先展示旧值、初始不 loading；超期时由挂载副作用触发后台
  // 重查（stale-while-revalidate）。无缓存（首次）才从 loading 起步。
  const [isLoadingTools, setIsLoadingTools] = useState(
    () => toolVersionsCache === null,
  );
  const [toolActions, setToolActions] = useState<
    Partial<Record<ToolName, ToolLifecycleAction>>
  >({});
  const [batchAction, setBatchAction] = useState<ToolLifecycleAction | null>(
    null,
  );
  const [showInstallCommands, setShowInstallCommands] = useState(false);

  const [wslShellByTool, setWslShellByTool] = useState<
    Record<string, WslShellPreference>
  >({});
  const [loadingTools, setLoadingTools] = useState<Record<string, boolean>>({});
  // 多处安装冲突诊断结果：按工具存储，有冲突的工具会在其卡片下方展示。
  // 来源两路：顶部「诊断安装冲突」按钮一次性扫全部，或升级后版本未变时自动补诊。
  const [toolDiagnostics, setToolDiagnostics] = useState<
    Partial<Record<ToolName, ToolInstallation[]>>
  >({});
  const [isDiagnosingAll, setIsDiagnosingAll] = useState(false);
  // 升级前探测到「多处安装需确认」时暂存：toolNames=本次要升级的全部工具，
  // plans=其中需要确认的（≥2 处）那些。用户确认后对 toolNames 整体执行升级。
  const [pendingUpgrade, setPendingUpgrade] = useState<{
    toolNames: ToolName[];
    plans: ToolInstallationReport[];
  } | null>(null);
  // 升级 preflight(probe 阶段)的 in-flight 工具集合。
  // probeToolInstallations 是个 1-3 秒级别的跨进程探测(对每个工具跑 --version + canonicalize),
  // 在它返回之前 toolActions / batchAction 都还没被置位 → 按钮不会 disabled → 用户快速双击
  // 会并发开两轮 probe,各自再触发 executeRun(并发的 `npm i -g` / 官方 installer,写冲突)。
  // 把 probe 期间的工具登记在这里、纳入 isAnyBusy 派生,关掉这个并发窗口。
  // 用 Set 而非 boolean:单卡片升级 & 批量升级可能在不同工具上独立 preflight,
  // 精确反映到各自卡片按钮的 disabled。
  const [preflightTools, setPreflightTools] = useState<Set<ToolName>>(
    () => new Set(),
  );
  const [installFlow, setInstallFlow] = useState<{
    visible: boolean;
    preparation: InstallPreparation | null;
    task: InstallTaskSnapshot | null;
    tools: ToolName[];
  }>({ visible: false, preparation: null, task: null, tools: [] });
  const unlistenInstallEvents = useRef<(() => void) | null>(null);
  const unlistenExitBlocked = useRef<(() => void) | null>(null);
  const activeInstallTaskId = useRef<string | null>(null);
  const mounted = useRef(true);

  const toolVersionByName = useMemo(() => {
    return new Map(toolVersions.map((tool) => [tool.name, tool]));
  }, [toolVersions]);

  const updatableToolNames = useMemo(
    () =>
      TOOL_NAMES.filter((toolName) => {
        const tool = toolVersionByName.get(toolName);
        return isUpdateAvailable(tool?.version, tool?.latest_version);
      }),
    [toolVersionByName],
  );

  const refreshToolVersions = useCallback(
    async (
      toolNames: ToolName[],
      wslOverrides?: Record<string, WslShellPreference>,
    ): Promise<ToolVersion[]> => {
      if (toolNames.length === 0) return [];

      // 单工具刷新使用统一后端入口（get_tool_versions）并带工具过滤。
      if (!mounted.current) return [];
      setLoadingTools((prev) => {
        const next = { ...prev };
        for (const name of toolNames) next[name] = true;
        return next;
      });

      try {
        const updated = await settingsApi.getToolVersions(
          toolNames,
          wslOverrides,
        );

        if (!mounted.current) return [];
        setToolVersions((prev) => mergeToolVersions(prev, updated));
        // 同步进模块缓存，供切 Tab 重挂时复用。时间戳沿用上次「全量加载」的（单工具
        // 刷新不算全量、不重置 TTL）；缓存为空时以 at=0 起步——0 是「尚未完成全量加载」
        // 的过期哨兵，确保探测中途切走/切回时，残缺缓存被判过期而触发重查，而非把半套
        // 数据当成完整结果复用。真实时间戳只由 loadAllToolVersions 的 finally 盖上。
        toolVersionsCache = {
          data: mergeToolVersions(toolVersionsCache?.data ?? [], updated),
          at: toolVersionsCache?.at ?? 0,
        };

        // 返回刷新结果，调用方可据此判断版本是否真的探到（避免读 state 撞 stale closure）。
        return updated;
      } catch (error) {
        console.error("[AboutSection] Failed to refresh tools", error);
        return [];
      } finally {
        if (mounted.current) {
          setLoadingTools((prev) => {
            const next = { ...prev };
            for (const name of toolNames) next[name] = false;
            return next;
          });
        }
      }
    },
    [],
  );

  const loadAllToolVersions = useCallback(
    async (options?: { force?: boolean }) => {
      const force = options?.force ?? false;
      // 命中新鲜缓存：切回「关于」Tab 触发的重挂直接复用上次结果，跳过 6 个 `--version`
      // 子进程 + 6 个 latest 版本网络请求。手动「刷新」传 force 绕过缓存强制重查。
      if (
        !force &&
        toolVersionsCache &&
        Date.now() - toolVersionsCache.at < TOOL_VERSIONS_CACHE_TTL_MS
      ) {
        setToolVersions(toolVersionsCache.data);
        if (mounted.current) setIsLoadingTools(false);
        return;
      }
      setIsLoadingTools(true);
      try {
        // 逐工具并发探测：每个工具一完成就合并进 toolVersions（并写模块缓存）、清掉自己
        // 的 loadingTools 标志，对应卡片随即独立刷新——而非等全部探测完才一次性显示（后端
        // 原本对 6 个工具串行 await，总耗时累加；并发后压成「最慢的那一个」）。refreshTool-
        // Versions 已内建按 name 合并 + per-tool loading + try/catch 兜底（单工具失败返回 []
        // 不拖累其余），故 Promise.all 永不 reject。Respect current shell/flag overrides.
        await Promise.all(
          TOOL_NAMES.map((toolName) =>
            refreshToolVersions([toolName], wslShellByTool),
          ),
        );
      } finally {
        // 全量探测结束：把缓存时间戳刷新为现在，标记「刚完成一次全量加载」、重置 TTL。
        if (toolVersionsCache) {
          toolVersionsCache = { ...toolVersionsCache, at: Date.now() };
        }
        if (mounted.current) setIsLoadingTools(false);
      }
    },
    [wslShellByTool, refreshToolVersions],
  );

  const handleToolShellChange = async (toolName: ToolName, value: string) => {
    const wslShell = value === "auto" ? null : value;
    const nextPref: WslShellPreference = {
      ...(wslShellByTool[toolName] ?? {}),
      wslShell,
    };
    setWslShellByTool((prev) => ({ ...prev, [toolName]: nextPref }));
    await refreshToolVersions([toolName], { [toolName]: nextPref });
  };

  const handleToolShellFlagChange = async (
    toolName: ToolName,
    value: string,
  ) => {
    const wslShellFlag = value === "auto" ? null : value;
    const nextPref: WslShellPreference = {
      ...(wslShellByTool[toolName] ?? {}),
      wslShellFlag,
    };
    setWslShellByTool((prev) => ({ ...prev, [toolName]: nextPref }));
    await refreshToolVersions([toolName], { [toolName]: nextPref });
  };

  useEffect(() => {
    void loadAllToolVersions();
    // Mount-only: loadAllToolVersions is intentionally excluded to avoid
    // re-fetching all tools whenever wslShellByTool changes. Single-tool
    // refreshes are handled by refreshToolVersions in the shell/flag handlers.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleCopyInstallCommands = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(ONE_CLICK_INSTALL_COMMANDS);
      toast.success(t("settings.installCommandsCopied"), { closeButton: true });
    } catch (error) {
      console.error("[AboutSection] Failed to copy install commands", error);
      toast.error(t("settings.installCommandsCopyFailed"));
    }
  }, [t]);

  // 升级后自动补诊单个工具：静默后台执行。有冲突写入结果；无冲突则清掉该工具可能残留
  // 的过期冲突展示（外部卸载/修复后冲突可能已消失，不清会一直显示旧列表）。不弹 toast、
  // 不报错打扰——与用户主动点的全量诊断区别对待。
  const diagnoseToolSilently = useCallback(async (toolName: ToolName) => {
    try {
      const [report] = await settingsApi.probeToolInstallations([toolName]);
      if (!mounted.current) return;
      setToolDiagnostics((prev) => {
        if (report?.is_conflict) {
          return { ...prev, [toolName]: report.installs };
        }
        // 无冲突：清掉残留;无旧结果则返回同引用，避免无谓 re-render。
        if (!(toolName in prev)) return prev;
        const next = { ...prev };
        delete next[toolName];
        return next;
      });
    } catch (error) {
      if (!mounted.current) return;
      console.error(
        `[AboutSection] Auto-diagnose failed for ${toolName}`,
        error,
      );
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    void (async () => {
      try {
        const unlistenInstall = await installerApi.listenAll(async (event) => {
          try {
            if (
              activeInstallTaskId.current &&
              activeInstallTaskId.current !== event.task_id
            ) {
              return;
            }
            const snapshot = await installerApi.getTask(event.task_id);
            if (disposed) return;
            activeInstallTaskId.current = snapshot.task_id;
            setInstallFlow((flow) => {
              if (flow.preparation?.requires_confirmation && !snapshot.result) {
                return flow;
              }
              const task = mergeInstallTaskSnapshot(flow.task, snapshot);
              return {
                visible: true,
                preparation: null,
                task,
                tools: task.request.tools as ToolName[],
              };
            });
            if (event.type === "finished") {
              const reportedTools = event.result.tools.map(
                (tool) => tool.tool as ToolName,
              );
              await refreshToolVersions(reportedTools);
              if (disposed) return;
              const toastOptions = { closeButton: true };
              if (event.result.status === "succeeded") {
                toast.success(
                  t("settings.installer.resultStatus.succeeded"),
                  toastOptions,
                );
              } else if (
                event.result.status === "succeeded_with_conflicts" ||
                event.result.status === "installed_not_runnable" ||
                event.result.status === "needs_user_action"
              ) {
                toast.warning(
                  t(`settings.installer.resultStatus.${event.result.status}`),
                  toastOptions,
                );
              } else {
                toast.error(
                  t(`settings.installer.resultStatus.${event.result.status}`),
                  toastOptions,
                );
              }
              if (
                event.result.status === "succeeded_with_conflicts" ||
                event.result.status === "needs_user_action" ||
                event.result.tools.some(
                  (tool) => tool.status === "installed_not_runnable",
                )
              ) {
                for (const tool of reportedTools) {
                  void diagnoseToolSilently(tool);
                }
              }
            }
          } catch (error) {
            console.error(
              "[AboutSection] Failed to apply installer event",
              error,
            );
            if (!disposed) {
              toast.error(t("settings.installer.failed"), {
                description: extractErrorMessage(error) || undefined,
                closeButton: true,
              });
            }
          }
        });
        if (disposed) unlistenInstall();
        else unlistenInstallEvents.current = unlistenInstall;
        if (disposed) return;
        const unlistenExit = await installerApi.listenExitBlocked(() => {
          if (disposed) return;
          setInstallFlow((flow) => ({ ...flow, visible: true }));
          toast.warning(t("settings.installer.exitBlocked"), {
            closeButton: true,
          });
        });
        if (disposed) unlistenExit();
        else unlistenExitBlocked.current = unlistenExit;
        if (disposed) return;
        const active = await installerApi.getActiveTask();
        if (disposed) return;
        if (active && !disposed) {
          activeInstallTaskId.current = active.task_id;
          setInstallFlow({
            visible: true,
            preparation: null,
            task: active,
            tools: active.request.tools as ToolName[],
          });
        }
        await installerApi.replayStartupRecovery();
      } catch (error) {
        console.error("[AboutSection] Failed to restore installer task", error);
        if (!disposed) {
          toast.error(t("settings.installer.failed"), {
            description: extractErrorMessage(error) || undefined,
            closeButton: true,
          });
        }
      }
    })();
    return () => {
      disposed = true;
      mounted.current = false;
      unlistenInstallEvents.current?.();
      unlistenInstallEvents.current = null;
      unlistenExitBlocked.current?.();
      unlistenExitBlocked.current = null;
      activeInstallTaskId.current = null;
    };
  }, [diagnoseToolSilently, refreshToolVersions, t]);

  // 顶部按钮：一次性诊断全部 6 个工具，有冲突的写入各自卡片，
  // 全部无冲突时给一条 info toast。后端逐工具枚举所有安装并判定分歧。
  const handleDiagnoseAll = useCallback(async () => {
    setIsDiagnosingAll(true);
    try {
      const reports = await settingsApi.probeToolInstallations([...TOOL_NAMES]);
      if (!mounted.current) return;
      const next: Partial<Record<ToolName, ToolInstallation[]>> = {};
      let conflicts = 0;
      for (const report of reports) {
        if (report.is_conflict) {
          next[report.tool as ToolName] = report.installs;
          conflicts += 1;
        }
      }
      setToolDiagnostics(next);
      if (conflicts === 0) {
        toast.info(t("settings.toolDiagnoseNoConflict"), { closeButton: true });
      }
    } catch (error) {
      if (!mounted.current) return;
      console.error("[AboutSection] Diagnose all failed", error);
      toast.error(t("settings.toolDiagnoseFailed"), {
        description: extractErrorMessage(error) || undefined,
        closeButton: true,
      });
    } finally {
      if (mounted.current) setIsDiagnosingAll(false);
    }
  }, [t]);

  // 实际执行安装/升级的串行循环（已通过任何必要的确认后才调用）。
  const executeRun = useCallback(
    async (toolNames: ToolName[], action: ToolLifecycleAction) => {
      const isBatch = toolNames.length > 1;
      if (isBatch) {
        setBatchAction(action);
      }

      // 逐工具串行执行：每个工具独立成败、独立刷新版本，一个失败不会连坐
      // 后续工具（后端把整批拼成单脚本 + set -e，会在首个失败处中止整批）。
      // soft=true 表示"命令成功执行但结果仍需用户介入"（版本没变/装上却跑不起来），
      // 与命令本身报错（soft=false）区别对待：前者不算硬失败，toast 降级为 warning。
      const failures: {
        toolName: ToolName;
        detail: string;
        soft: boolean;
        kind?: "notRunnable" | "versionUnchanged";
      }[] = [];
      let succeeded = 0;

      for (const toolName of toolNames) {
        if (!mounted.current) return;
        setToolActions((prev) => ({ ...prev, [toolName]: action }));
        try {
          const previousTool = toolVersionByName.get(toolName);
          const previousVersion = previousTool?.version ?? null;
          const previousLatestVersion = previousTool?.latest_version ?? null;
          const lifecycleOverrides = buildWslLifecycleOverrides(
            [toolName],
            { [toolName]: previousTool?.env_type },
            wslShellByTool,
          );

          await settingsApi.runToolLifecycleAction(
            [toolName],
            action,
            lifecycleOverrides,
          );
          if (!mounted.current) return;
          // 静默执行真正结束后刷新该工具版本，卡片立即反映结果。
          const refreshed = await refreshToolVersions(
            [toolName],
            lifecycleOverrides,
          );
          if (!mounted.current) return;
          const tool = refreshed.find((t) => t.name === toolName);
          if (tool?.version) {
            const latestVersion = tool.latest_version ?? previousLatestVersion;
            const versionUnchangedAfterUpdate =
              action === "update" &&
              Boolean(previousVersion) &&
              tool.version === previousVersion &&
              isUpdateAvailable(tool.version, latestVersion);

            if (versionUnchangedAfterUpdate) {
              // 有些上游 updater 会在未实际改动版本时仍返回 0。这里用刷新后的
              // 当前版本 + latest_version 再确认一次，避免给用户误报升级成功。
              failures.push({
                toolName,
                detail: t("settings.toolActionVersionUnchanged", {
                  version: tool.version,
                  latest: latestVersion ?? t("common.unknown"),
                }),
                soft: true,
                kind: "versionUnchanged",
              });
              if (mounted.current) void diagnoseToolSilently(toolName);
            } else {
              succeeded += 1;
              // 升级成功后无条件补诊：版本没变多半被另一处遮蔽，版本变了另一处也可能仍在，
              // 两种都要刷新冲突展示（diagnoseToolSilently 无冲突时会自动清旧）。
              if (action === "update") {
                if (mounted.current) void diagnoseToolSilently(toolName);
              }
            }
          } else {
            // 命令退出码为 0、但刷新后仍探不到版本：多半是"装上了却跑不起来"
            // （如 openclaw 要求更高的 Node 版本）。refreshToolVersions 的 merge 已把
            // version 置空并写入后端 error，这里只需归类为软失败并展示原因。
            const detail = tool?.error?.trim() || t("settings.toolNotRunnable");
            failures.push({
              toolName,
              detail,
              soft: true,
              kind: "notRunnable",
            });
            // 装了却跑不起来同样可能源于多处安装，自动诊断帮用户定位。
            if (mounted.current) void diagnoseToolSilently(toolName);
          }
        } catch (error) {
          if (!mounted.current) return;
          console.error(
            `[AboutSection] Failed to run tool action for ${toolName}`,
            error,
          );
          const detail = extractErrorMessage(error) || String(error);
          failures.push({ toolName, detail, soft: false });
        } finally {
          if (!mounted.current) return;
          setToolActions((prev) => {
            const next = { ...prev };
            delete next[toolName];
            return next;
          });
        }
      }

      if (!mounted.current) return;
      if (isBatch) {
        setBatchAction(null);
      }

      const actionLabel =
        action === "install"
          ? t("settings.toolInstall")
          : t("settings.toolUpdate");

      if (failures.length === 0) {
        toast.success(
          t("settings.toolActionDone", {
            count: succeeded,
            action: actionLabel,
          }),
          { closeButton: true },
        );
        return;
      }

      // 批量场景每个失败只摘取错误末行（最相关），单工具场景给出完整详情。
      const lastLine = (text: string) => {
        const lines = text.trim().split("\n").filter(Boolean);
        return lines[lines.length - 1] ?? text;
      };
      const failureDescription = isBatch
        ? failures
            .map(
              (f) => `${TOOL_DISPLAY_NAMES[f.toolName]}: ${lastLine(f.detail)}`,
            )
            .join("\n")
        : failures[0]?.detail;

      const hardFailures = failures.filter((f) => !f.soft);
      const allSoftVersionUnchanged =
        failures.length > 0 &&
        failures.every((f) => f.soft && f.kind === "versionUnchanged");

      if (succeeded === 0 && hardFailures.length === 0) {
        // 命令均成功执行、但结果需要用户介入（版本没变 / 装上却跑不起来）
        // → 降级为 warning 并解释原因。
        toast.warning(
          allSoftVersionUnchanged
            ? t("settings.toolActionVersionUnchangedTitle")
            : t("settings.toolActionInstalledNotRunnable"),
          {
            description: failureDescription || undefined,
            closeButton: true,
          },
        );
      } else if (succeeded === 0) {
        toast.error(t("settings.toolActionFailed"), {
          description: failureDescription || undefined,
          closeButton: true,
        });
      } else {
        // 部分成功：用 warning 汇总成败数量，详情列出失败的工具。
        toast.warning(
          t("settings.toolActionPartial", {
            succeeded,
            failed: failures.length,
            action: actionLabel,
          }),
          { description: failureDescription || undefined, closeButton: true },
        );
      }
    },
    [
      t,
      wslShellByTool,
      toolVersionByName,
      refreshToolVersions,
      diagnoseToolSilently,
    ],
  );

  // 升级/安装的统一入口锁。所有动作在入口处先登记 preflight、出口处解锁;
  // 这层锁覆盖三个否则会漏掉的窗口:
  //   ① update 的 probeToolInstallations 阶段(1-3 秒跨进程,executeRun 之前);
  //   ② executeRun 内部 setToolActions 落到 React commit 前的几个 microtask;
  //   ③ install 直接进 executeRun 的同一段 microtask 窗口。
  // 早退检查避免同一工具在 toolActions / preflight 已登记时被重复入栈——批量场景里
  // 只要有一个工具被锁,整批不开新一轮,因为后端 set -e 串行的语义假设是「一次性
  // 单脚本」,跨两次 IPC 并发会破坏它。
  const handleRunToolAction = useCallback(
    async (toolNames: ToolName[], action: ToolLifecycleAction) => {
      if (toolNames.length === 0) return;
      if (
        toolNames.some(
          (name) => preflightTools.has(name) || toolActions[name] !== undefined,
        )
      ) {
        return;
      }
      // 入栈 preflight,按钮立刻 disabled。new Set(prev) 是不可变更新(直接 mutate
      // 原 Set 会让 React 复用引用、跳过 re-render);finally 块负责异常路径解锁。
      setPreflightTools((prev) => {
        const next = new Set(prev);
        toolNames.forEach((name) => next.add(name));
        return next;
      });
      try {
        if (action === "install") {
          const isNative = toolNames.every(
            (toolName) => toolVersionByName.get(toolName)?.env_type !== "wsl",
          );
          if (!isNative) {
            await executeRun(toolNames, action);
            return;
          }
          try {
            const preparation = await installerApi.prepare({
              task_id: null,
              tools: toolNames,
              action,
            });
            if (!mounted.current) return;
            activeInstallTaskId.current = preparation.task_id;
            if (preparation.requires_confirmation) {
              setInstallFlow({
                visible: true,
                preparation,
                task: null,
                tools: toolNames,
              });
            } else {
              setInstallFlow({
                visible: true,
                preparation: null,
                task: {
                  task_id: preparation.task_id,
                  request: { task_id: preparation.task_id, tools: toolNames, action },
                  stage: "installing_tools",
                  plan: preparation.plan,
                  result: null,
                  cancellation_requested: false,
                  interrupted: false,
                },
                tools: toolNames,
              });
              if (!mounted.current) return;
              const outcome = await installerApi.start({
                request: {
                  task_id: preparation.task_id,
                  tools: toolNames,
                  action,
                },
                confirmed_action_ids: [],
              });
              if (outcome.type !== "started") {
                activeInstallTaskId.current = null;
                setInstallFlow({ visible: false, preparation: null, task: null, tools: [] });
                toast.error(t("settings.installer.stateChanged"), { closeButton: true });
              }
            }
          } catch (error) {
            if (!mounted.current) return;
            console.error(
              "[AboutSection] Failed to start native install",
              error,
            );
            activeInstallTaskId.current = null;
            setInstallFlow({ visible: false, preparation: null, task: null, tools: [] });
            toast.error(t("settings.installer.failed"), {
              closeButton: true,
            });
          }
          return;
        }
        let reports: ToolInstallationReport[];
        try {
          reports = await settingsApi.probeToolInstallations(toolNames);
        } catch (error) {
          if (!mounted.current) return;
          // 探测失败不应阻断升级：退回直接执行（等同旧行为）。
          console.error("[AboutSection] probeToolInstallations failed", error);
          await executeRun(toolNames, action);
          return;
        }
        if (!mounted.current) return;
        const needConfirm = reports.filter((r) => r.needs_confirmation);
        if (needConfirm.length === 0) {
          await executeRun(toolNames, action);
          return;
        }
        if (mounted.current)
          setPendingUpgrade({ toolNames, plans: needConfirm });
      } finally {
        if (!mounted.current) return;
        setPreflightTools((prev) => {
          const next = new Set(prev);
          toolNames.forEach((name) => next.delete(name));
          return next;
        });
      }
    },
    [executeRun, preflightTools, toolActions, toolVersionByName],
  );

  const handleConfirmUpgrade = useCallback(() => {
    if (pendingUpgrade) {
      void executeRun(pendingUpgrade.toolNames, "update");
    }
    setPendingUpgrade(null);
  }, [pendingUpgrade, executeRun]);

  const handleCancelUpgrade = useCallback(() => setPendingUpgrade(null), []);

  const handleConfirmInstall = useCallback(
    async (actionIds: string[]) => {
      const preparation = installFlow.preparation;
      if (!preparation) return;
      const tools = installFlow.tools;
      try {
        const outcome = await installerApi.start({
          request: {
            task_id: preparation.task_id,
            tools,
            action: "install",
          },
          confirmed_action_ids: actionIds,
        });
        if (!mounted.current) return;
        if (outcome.type === "state_changed") {
          activeInstallTaskId.current = null;
          setInstallFlow({ visible: false, preparation: null, task: null, tools: [] });
          toast.error(t("settings.installer.stateChanged"), { closeButton: true });
          return;
        }
        if (outcome.type === "refreshed") {
          const refreshed = outcome.preparation;
          activeInstallTaskId.current = refreshed.task_id;
          if (refreshed.requires_confirmation) {
            setInstallFlow({ visible: true, preparation: refreshed, task: null, tools });
            return;
          }
          const resumed = await installerApi.start({
            request: { task_id: refreshed.task_id, tools, action: "install" },
            confirmed_action_ids: [],
          });
          if (!mounted.current) return;
          if (resumed.type !== "started") {
            activeInstallTaskId.current = null;
            setInstallFlow({ visible: false, preparation: null, task: null, tools: [] });
            toast.error(t("settings.installer.stateChanged"), { closeButton: true });
            return;
          }
          setInstallFlow({
            visible: true,
            preparation: null,
            task: {
              task_id: refreshed.task_id,
              request: { task_id: refreshed.task_id, tools, action: "install" },
              stage: "installing_tools",
              plan: refreshed.plan,
              result: null,
              cancellation_requested: false,
              interrupted: false,
            },
            tools,
          });
          return;
        }
        setInstallFlow((flow) => {
          const task = mergeInstallTaskSnapshot(flow.task, {
            task_id: preparation.task_id,
            request: {
              task_id: preparation.task_id,
              tools,
              action: "install",
            },
            stage: "repairing",
            plan: preparation.plan,
            result: null,
            cancellation_requested: false,
            interrupted: false,
          });
          return {
            visible: true,
            preparation: null,
            task,
            tools,
          };
        });
      } catch (error) {
        if (!mounted.current) return;
        console.error("[AboutSection] Failed to confirm native install", error);
        activeInstallTaskId.current = null;
        setInstallFlow({ visible: false, preparation: null, task: null, tools: [] });
        toast.error(t("settings.installer.failed"), { closeButton: true });
      }
    },
    [installFlow, t],
  );

  const handleCancelInstall = useCallback(async () => {
    const taskId =
      installFlow.task?.task_id ?? installFlow.preparation?.task_id;
    if (!taskId) return;
    try {
      await installerApi.cancel(taskId);
      if (installFlow.preparation) {
        activeInstallTaskId.current = null;
        setInstallFlow({ visible: false, preparation: null, task: null, tools: [] });
      }
    } catch (error) {
      if (!mounted.current) return;
      console.error("[AboutSection] Failed to cancel native install", error);
      toast.error(t("settings.installer.failed"), { closeButton: true });
    }
  }, [installFlow, t]);

  const handleRetryInstall = useCallback(() => {
    const tools = installFlow.tools;
    setInstallFlow({
      visible: false,
      preparation: null,
      task: null,
      tools: [],
    });
    activeInstallTaskId.current = null;
    void handleRunToolAction(tools, "install");
  }, [handleRunToolAction, installFlow.tools]);

  // 任一安装/升级进行中（批量或单工具）即视为忙碌：用于禁用所有操作按钮，
  // 避免并发触发多个 npm/pip 全局写入造成冲突。
  // preflightTools 覆盖升级前的 probe 阶段——那段在 executeRun 之前、toolActions
  // 还没置位,如果不算进 busy 会留出 1-3 秒的并发触发窗口。
  const isAnyBusy =
    Boolean(batchAction) ||
    Object.keys(toolActions).length > 0 ||
    preflightTools.size > 0 ||
    Boolean(
      installFlow.preparation || (installFlow.task && !installFlow.task.result),
    );

  return (
    <motion.section
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3 }}
      className="space-y-6"
    >
      <div className="space-y-1 px-1">
        <h2 className="text-xl font-semibold">{t("app.title")}</h2>
        <p className="text-sm text-muted-foreground">{t("app.description")}</p>
      </div>
      <div className="space-y-3">
        <div className="flex flex-col gap-2 px-1 sm:flex-row sm:items-center sm:justify-between">
          <h3 className="text-sm font-medium">{t("settings.localEnvCheck")}</h3>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1.5 text-xs"
              onClick={() => handleDiagnoseAll()}
              disabled={isLoadingTools || isAnyBusy || isDiagnosingAll}
            >
              {isDiagnosingAll ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Stethoscope className="h-3.5 w-3.5" />
              )}
              {isDiagnosingAll
                ? t("settings.toolDiagnosing")
                : t("settings.toolDiagnose")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1.5 text-xs"
              onClick={() => loadAllToolVersions({ force: true })}
              disabled={isLoadingTools || isAnyBusy}
            >
              <RefreshCw
                className={
                  isLoadingTools ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"
                }
              />
              {isLoadingTools ? t("common.refreshing") : t("common.refresh")}
            </Button>
            <Button
              size="sm"
              className="h-7 gap-1.5 text-xs"
              onClick={() => handleRunToolAction(updatableToolNames, "update")}
              disabled={
                isLoadingTools || isAnyBusy || updatableToolNames.length === 0
              }
            >
              {batchAction === "update" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ArrowUpCircle className="h-3.5 w-3.5" />
              )}
              {t("settings.updateAllTools", {
                count: updatableToolNames.length,
              })}
            </Button>
          </div>
        </div>

        <div className="grid gap-3 px-1 sm:grid-cols-2 xl:grid-cols-3">
          {TOOL_NAMES.map((toolName, index) => {
            const tool = toolVersionByName.get(toolName);
            const appConfig = APP_ICON_MAP[TOOL_APP_IDS[toolName]];
            const displayName = TOOL_DISPLAY_NAMES[toolName];
            // 单卡片 loading 用「结果是否已到」而非「整批是否结束」驱动，实现渐进式刷新：
            //   - loadingTools[t]：本工具探测在途（首次加载或单工具刷新）；
            //   - isLoadingTools && !has(t)：整批进行中且该工具尚未返回——覆盖首帧/刷新时
            //     未完成卡片的 loading 外观。某工具结果一落进 toolVersions，has(t) 即为 true，
            //     该卡片立刻脱离 loading（哪怕全局 isLoadingTools 还为 true），其它卡片不受影响。
            const isToolVersionLoading =
              Boolean(loadingTools[toolName]) ||
              (isLoadingTools && !toolVersionByName.has(toolName));
            const isOutdated = isUpdateAvailable(
              tool?.version,
              tool?.latest_version,
            );
            // 已安装却跑不起来（如 Node 版本不达标）：用它区分卡片文案与按钮，避免把
            // "装了跑不起来"误判成"未安装"而给出无用的安装按钮（重装同一版本解决不了）。
            const installedButBroken = Boolean(tool?.installed_but_broken);
            // loading 和 broken 都没有可执行动作；其余按是否已装/是否过期选择。
            const action: ToolLifecycleAction | null =
              isToolVersionLoading || installedButBroken
                ? null
                : !tool?.version
                  ? "install"
                  : isOutdated
                    ? "update"
                    : null;
            const runningAction = toolActions[toolName];
            const title = tool?.version || tool?.error || t("common.unknown");
            const conflicts = toolDiagnostics[toolName];

            return (
              <motion.div
                key={toolName}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.3, delay: 0.15 + index * 0.04 }}
                className="flex min-h-[150px] flex-col gap-3 rounded-xl border border-border bg-gradient-to-br from-card/80 to-card/40 p-4 shadow-sm transition-colors hover:border-primary/30"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-background/80 text-muted-foreground">
                      {appConfig?.icon ?? <Terminal className="h-4 w-4" />}
                    </span>
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">
                        {displayName}
                      </div>
                      {tool?.env_type && ENV_BADGE_CONFIG[tool.env_type] && (
                        <span
                          className={`mt-1 inline-flex w-fit text-[9px] px-1.5 py-0.5 rounded-full border ${ENV_BADGE_CONFIG[tool.env_type].className}`}
                        >
                          {t(ENV_BADGE_CONFIG[tool.env_type].labelKey)}
                          {tool.wsl_distro ? ` · ${tool.wsl_distro}` : ""}
                        </span>
                      )}
                    </div>
                  </div>
                  {isToolVersionLoading ? (
                    <Loader2 className="mt-1 h-4 w-4 animate-spin text-muted-foreground" />
                  ) : tool?.version ? (
                    isOutdated ? (
                      <span className="mt-1 shrink-0 rounded-full border border-yellow-500/20 bg-yellow-500/10 px-1.5 py-0.5 text-[10px] text-yellow-600 dark:text-yellow-400">
                        {t("settings.updateAvailableShort")}
                      </span>
                    ) : (
                      <CheckCircle2 className="mt-1 h-4 w-4 shrink-0 text-green-500" />
                    )
                  ) : (
                    <AlertCircle className="mt-1 h-4 w-4 shrink-0 text-yellow-500" />
                  )}
                </div>

                <div className="space-y-1.5 text-xs">
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">
                      {t("settings.currentVersion")}
                    </span>
                    <span
                      className="min-w-0 truncate font-mono text-foreground"
                      title={title}
                    >
                      {isToolVersionLoading
                        ? t("common.loading")
                        : tool?.version
                          ? tool.version
                          : installedButBroken
                            ? t("settings.installedNotRunnable")
                            : t("common.notInstalled")}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">
                      {t("settings.latestVersion")}
                    </span>
                    <span className="min-w-0 truncate font-mono text-foreground">
                      {isToolVersionLoading
                        ? t("common.loading")
                        : tool?.latest_version || t("common.unknown")}
                    </span>
                  </div>
                  {!isToolVersionLoading && !tool?.version && tool?.error && (
                    <div className="truncate text-[11px] text-muted-foreground">
                      {tool.error}
                    </div>
                  )}
                </div>

                {tool?.env_type === "wsl" && (
                  <div className="flex flex-wrap gap-2">
                    <Select
                      value={wslShellByTool[toolName]?.wslShell || "auto"}
                      onValueChange={(v) => handleToolShellChange(toolName, v)}
                      disabled={isToolVersionLoading || isAnyBusy}
                    >
                      <SelectTrigger className="h-7 w-[82px] text-xs">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">{t("common.auto")}</SelectItem>
                        {WSL_SHELL_OPTIONS.map((shell) => (
                          <SelectItem key={shell} value={shell}>
                            {shell}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Select
                      value={wslShellByTool[toolName]?.wslShellFlag || "auto"}
                      onValueChange={(v) =>
                        handleToolShellFlagChange(toolName, v)
                      }
                      disabled={isToolVersionLoading || isAnyBusy}
                    >
                      <SelectTrigger className="h-7 w-[82px] text-xs">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="auto">{t("common.auto")}</SelectItem>
                        {WSL_SHELL_FLAG_OPTIONS.map((flag) => (
                          <SelectItem key={flag} value={flag}>
                            {flag}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                )}

                {/* 多处安装冲突诊断结果：仅在懒触发后有数据时渲染。 */}
                {conflicts && conflicts.length > 0 && (
                  <div className="space-y-1.5 rounded-lg border border-yellow-500/20 bg-yellow-500/5 p-2.5">
                    <div className="text-[11px] font-medium text-yellow-600 dark:text-yellow-400">
                      {t("settings.toolConflictTitle")}
                    </div>
                    <p className="text-[10px] leading-snug text-muted-foreground">
                      {t("settings.toolConflictHint")}
                    </p>
                    <ul className="space-y-1.5">
                      {conflicts.map((inst) => (
                        <li key={inst.path}>
                          <ToolInstallRow inst={inst} />
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                <div className="mt-auto flex items-center justify-end">
                  {isToolVersionLoading ? (
                    <span className="text-xs text-muted-foreground">
                      {t("common.loading")}
                    </span>
                  ) : installedButBroken ? (
                    // 已安装但跑不起来：重装无济于事，不给按钮，给一句指向环境的提示。
                    <span className="text-xs text-yellow-600 dark:text-yellow-400">
                      {t("settings.toolCheckEnv")}
                    </span>
                  ) : action ? (
                    <Button
                      size="sm"
                      variant={action === "install" ? "outline" : "default"}
                      className="h-7 gap-1.5 text-xs"
                      onClick={() => handleRunToolAction([toolName], action)}
                      disabled={isToolVersionLoading || isAnyBusy}
                    >
                      {runningAction ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : action === "install" ? (
                        <Download className="h-3.5 w-3.5" />
                      ) : (
                        <ArrowUpCircle className="h-3.5 w-3.5" />
                      )}
                      {/* loading 时文案保持不变、仅图标切换为 spinner，
                          按钮宽度恒定，避免"升级"→"升级中…"导致的抖动。 */}
                      {action === "install"
                        ? t("settings.toolInstall")
                        : t("settings.toolUpdate")}
                    </Button>
                  ) : (
                    <span className="text-xs text-muted-foreground">
                      {t("settings.toolReady")}
                    </span>
                  )}
                </div>
              </motion.div>
            );
          })}
        </div>
      </div>

      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3, delay: 0.3 }}
        className="space-y-3"
      >
        <button
          type="button"
          onClick={() => setShowInstallCommands((v) => !v)}
          aria-expanded={showInstallCommands}
          className="flex w-full items-center gap-1.5 px-1 text-sm font-medium text-foreground transition-colors hover:text-primary"
        >
          <ChevronDown
            className={`h-3.5 w-3.5 transition-transform ${
              showInstallCommands ? "" : "-rotate-90"
            }`}
          />
          {t("settings.manualInstallCommands")}
        </button>
        {showInstallCommands && (
          <div className="rounded-xl border border-border bg-gradient-to-br from-card/80 to-card/40 p-4 space-y-3 shadow-sm">
            <div className="flex items-center justify-between gap-2">
              <p className="text-xs text-muted-foreground">
                {t("settings.oneClickInstallHint")}
              </p>
              <Button
                size="sm"
                variant="outline"
                onClick={handleCopyInstallCommands}
                className="h-7 gap-1.5 text-xs"
              >
                <Copy className="h-3.5 w-3.5" />
                {t("common.copy")}
              </Button>
            </div>
            <pre className="text-xs font-mono bg-background/80 px-3 py-2.5 rounded-lg border border-border/60 overflow-x-auto">
              {ONE_CLICK_INSTALL_COMMANDS}
            </pre>
          </div>
        )}
      </motion.div>

      <ToolUpgradeConfirmDialog
        isOpen={pendingUpgrade !== null}
        plans={pendingUpgrade?.plans ?? []}
        displayName={toolDisplayName}
        onConfirm={handleConfirmUpgrade}
        onCancel={handleCancelUpgrade}
      />
      <ToolInstallDialog
        open={installFlow.visible}
        preparation={installFlow.preparation}
        task={installFlow.task}
        toolName={toolDisplayName}
        onConfirm={(actionIds) => void handleConfirmInstall(actionIds)}
        onCancel={() => void handleCancelInstall()}
        onClose={() => setInstallFlow((flow) => ({ ...flow, visible: false }))}
        onRetry={handleRetryInstall}
      />
    </motion.section>
  );
}
