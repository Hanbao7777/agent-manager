import { describe, expect, it } from "vitest";

import { mergeInstallTaskSnapshot } from "./installTaskState";

const snapshot = (stage: string, terminal = false) => ({
  task_id: "install-1",
  stage,
  result: terminal ? { status: "succeeded", tools: [], failure: null } : null,
});

describe("mergeInstallTaskSnapshot", () => {
  it("never replaces a terminal snapshot with late progress", () => {
    const completed = snapshot("completed", true);
    expect(mergeInstallTaskSnapshot(completed, snapshot("repairing"))).toBe(
      completed,
    );
  });

  it("never moves an active task to an earlier stage", () => {
    const installing = snapshot("installing_tools");
    expect(mergeInstallTaskSnapshot(installing, snapshot("preflight"))).toBe(
      installing,
    );
  });

  it("accepts forward progress and terminal results", () => {
    const repairing = snapshot("repairing");
    const installing = snapshot("installing_tools");
    const completed = snapshot("completed", true);
    expect(mergeInstallTaskSnapshot(repairing, installing)).toBe(installing);
    expect(mergeInstallTaskSnapshot(installing, completed)).toBe(completed);
  });
});
