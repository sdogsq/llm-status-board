import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pluginUpdater } from "./pluginRuntime";
import { check } from "@tauri-apps/plugin-updater";
import { openUrl } from "@tauri-apps/plugin-opener";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.unstubAllEnvs());

describe("fork release update routing", () => {
  it("requires signed-update configuration rather than reporting a manual-only build as current", async () => {
    vi.stubEnv("VITE_SIGNED_UPDATER", "false");
    await expect(pluginUpdater.check(null)).rejects.toThrow("手动下载安装");
    expect(check).not.toHaveBeenCalled();
  });

  it("allows the signature-verifying plugin only for configured signed builds", async () => {
    vi.stubEnv("VITE_SIGNED_UPDATER", "true");
    vi.mocked(check).mockResolvedValue(null);
    await expect(pluginUpdater.check("  http://127.0.0.1:7890  ")).resolves.toBeNull();
    expect(check).toHaveBeenCalledWith({ proxy: "http://127.0.0.1:7890", timeout: 15000 });
  });

  it("opens this fork's downloads page", async () => {
    vi.mocked(openUrl).mockResolvedValue(undefined);
    await pluginUpdater.openReleasePage();
    expect(openUrl).toHaveBeenCalledWith("https://github.com/sdogsq/llm-status-board/releases/latest");
  });
});
