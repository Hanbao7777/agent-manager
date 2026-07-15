export type WslLifecyclePreference = {
  wslShell?: string | null;
  wslShellFlag?: string | null;
};

export function buildWslLifecycleOverrides(
  toolNames: readonly string[],
  environmentByTool: Readonly<Record<string, string | undefined>>,
  configured: Readonly<Record<string, WslLifecyclePreference>>,
): Record<string, WslLifecyclePreference> {
  return Object.fromEntries(
    toolNames
      .filter((toolName) => environmentByTool[toolName] === "wsl")
      .map((toolName) => [toolName, configured[toolName] ?? {}]),
  );
}
