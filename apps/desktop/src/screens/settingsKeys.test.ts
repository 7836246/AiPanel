import { describe, it, expect, beforeEach } from "vitest";
import {
  READONLY_DEFAULT_KEY,
  UPDATE_AUTOCHECK_KEY,
  readUpdateAutoCheck,
  writeUpdateAutoCheck,
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
});
