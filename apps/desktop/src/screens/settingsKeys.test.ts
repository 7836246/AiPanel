import { describe, it, expect, beforeEach } from "vitest";
import {
  READONLY_DEFAULT_KEY,
  UPDATE_AUTOCHECK_KEY,
  readUpdateAutoCheck,
  writeUpdateAutoCheck,
  readRecentServers,
  recordRecentServer,
} from "./settingsKeys";

describe("settingsKeys", () => {
  beforeEach(() => localStorage.clear());

  it("键名稳定(被 CodexConsole/SettingsPanel 依赖)", () => {
    expect(READONLY_DEFAULT_KEY).toBe("aipanel-readonly-default");
    expect(UPDATE_AUTOCHECK_KEY).toBe("aipanel-update-autocheck");
  });

  it("启动检查更新默认开启;仅显式 false 关闭", () => {
    expect(readUpdateAutoCheck()).toBe(true); // 未设置 → 默认开
    writeUpdateAutoCheck(false);
    expect(localStorage.getItem(UPDATE_AUTOCHECK_KEY)).toBe("false");
    expect(readUpdateAutoCheck()).toBe(false);
    writeUpdateAutoCheck(true);
    expect(readUpdateAutoCheck()).toBe(true);
  });

  it("最近服务器:置顶去重、最多 5 个、空初始", () => {
    expect(readRecentServers()).toEqual([]);
    recordRecentServer("a");
    recordRecentServer("b");
    expect(recordRecentServer("a")).toEqual(["a", "b"]); // 再访问 a → 置顶去重
    for (const id of ["c", "d", "e", "f"]) recordRecentServer(id);
    const list = readRecentServers();
    expect(list.length).toBe(5); // 上限 5
    expect(list[0]).toBe("f"); // 最近在前
    expect(list).not.toContain("b"); // 最旧的被挤出
  });
});
