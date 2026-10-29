import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pluginUpdater } from "./pluginRuntime";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ Update: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.unstubAllEnvs());

describe("fork release update routing", () => {
  it("requires signed-update configuration rather than reporting a manual-only build as current", async () => {
    vi.stubEnv("VITE_SIGNED_UPDATER", "false");
    await expect(pluginUpdater.check(null, "stable")).rejects.toThrow("手动下载安装");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("allows the signature-verifying plugin only for configured signed builds", async () => {
    vi.stubEnv("VITE_SIGNED_UPDATER", "true");
    vi.mocked(invoke).mockResolvedValue(null);
    await expect(pluginUpdater.check("  http://127.0.0.1:7890  ", "stable")).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("check_channel_update", { channel: "stable", proxy: "http://127.0.0.1:7890", timeout: 15000 });
  });

  it("opens this fork's downloads page", async () => {
    vi.mocked(openUrl).mockResolvedValue(undefined);
    await pluginUpdater.openReleasePage("stable");
    expect(openUrl).toHaveBeenCalledWith("https://github.com/sdogsq/llm-status-board/releases/latest");
  });
});
