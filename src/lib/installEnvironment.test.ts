import { describe, expect, it } from "vitest";

import { buildWslLifecycleOverrides } from "./installEnvironment";

describe("buildWslLifecycleOverrides", () => {
  it("marks WSL tools even when the user kept default shell settings", () => {
    expect(
      buildWslLifecycleOverrides(
        ["claude", "codex"],
        { claude: "wsl", codex: "windows" },
        {},
      ),
    ).toEqual({ claude: {} });
  });

  it("preserves configured WSL shell settings", () => {
    expect(
      buildWslLifecycleOverrides(
        ["claude"],
        { claude: "wsl" },
        { claude: { wslShell: "zsh", wslShellFlag: "-lic" } },
      ),
    ).toEqual({ claude: { wslShell: "zsh", wslShellFlag: "-lic" } });
  });
});
