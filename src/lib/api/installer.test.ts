import { beforeEach, describe, expect, it, vi } from "vitest";

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

import {
  installerApi,
  type InstallRequest,
  type InstallTaskEvent,
} from "./installer";

describe("installerApi", () => {
  beforeEach(() => {
    tauri.invoke.mockReset();
    tauri.listen.mockReset();
  });

  it("uses the installer command names and snake-case request envelopes", async () => {
    const request: InstallRequest = {
      task_id: "install-1",
      tools: ["codex"],
      action: "install" as const,
    };

    await installerApi.prepare(request);
    await installerApi.start({
      request,
      confirmed_action_ids: ["install-node"],
    });
    await installerApi.getTask("install-1");
    await installerApi.cancel("install-1");
    await installerApi.replayStartupRecovery();

    expect(tauri.invoke).toHaveBeenNthCalledWith(1, "prepare_tool_install", {
      request,
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(2, "start_tool_install", {
      request: { request, confirmed_action_ids: ["install-node"] },
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(3, "get_install_task", {
      taskId: "install-1",
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(4, "cancel_install_task", {
      taskId: "install-1",
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(
      5,
      "replay_startup_install_recovery",
    );
  });

  it("delivers only events belonging to the subscribed task and returns Tauri cleanup", async () => {
    const unlisten = vi.fn();
    const handler = vi.fn();
    tauri.listen.mockResolvedValue(unlisten);

    const returnedUnlisten = await installerApi.listen("install-1", handler);
    const listener = tauri.listen.mock.calls[0][1] as (event: {
      payload: InstallTaskEvent;
    }) => void;
    const matchingEvent: InstallTaskEvent = {
      type: "stage_changed",
      task_id: "install-1",
      stage: "installing_tools",
    };

    listener({ payload: { ...matchingEvent, task_id: "install-2" } });
    listener({ payload: matchingEvent });

    expect(tauri.listen).toHaveBeenCalledWith(
      "agent-manager://install-task",
      expect.any(Function),
    );
    expect(handler).toHaveBeenCalledOnce();
    expect(handler).toHaveBeenCalledWith(matchingEvent);
    expect(returnedUnlisten).toBe(unlisten);
  });
});
