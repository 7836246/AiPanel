import { describe, it, expect } from "vitest";
import {
  isTauri,
  RISK_META,
  listServers,
  checkSshConnection,
  serverMetrics,
  saveProvider,
  type RiskLevel,
} from "./api";

// 测试运行在 jsdom 下:window 存在但无 __TAURI_INTERNALS__,故走非 Tauri 的 mock 分支。
describe("isTauri", () => {
  it("测试环境(无 __TAURI_INTERNALS__)判定为非 Tauri", () => {
    expect(isTauri()).toBe(false);
  });
});

describe("RISK_META", () => {
  it("四个风险等级齐全且带中文标签与色彩 token", () => {
    const levels: RiskLevel[] = ["low", "medium", "high", "blocked"];
    for (const l of levels) {
      expect(RISK_META[l]).toBeTruthy();
      expect(RISK_META[l].label.length).toBeGreaterThan(0);
      expect(RISK_META[l].dot).toContain("bg-risk-");
      expect(RISK_META[l].text).toContain("text-risk-");
    }
  });
});

describe("非 Tauri mock 路径", () => {
  it("listServers 返回数组(新引用)", async () => {
    const a = await listServers();
    const b = await listServers();
    expect(Array.isArray(a)).toBe(true);
    expect(a).not.toBe(b); // 每次新引用,保证 React 能感知
  });

  it("checkSshConnection 返回 {ok, message}", async () => {
    const r = await checkSshConnection("mock-x");
    expect(typeof r.ok).toBe("boolean");
    expect(typeof r.message).toBe("string");
  });

  it("serverMetrics 返回完整指标快照", async () => {
    const m = await serverMetrics("mock-x");
    expect(m.cpuCores).toBeGreaterThan(0);
    expect(m.memTotalBytes).toBeGreaterThan(0);
    expect(m.diskPath).toBe("/");
    expect(typeof m.sampledAt).toBe("string");
  });

  it("saveProvider 编辑时保留既有 credentialRef(与后端一致)", async () => {
    // 先创建并带 Key → credentialRef = provider:<id>
    const created = await saveProvider(
      { id: "vitest-p1", name: "p", kind: "openai_compatible", enabled: true },
      "sk-secret",
    );
    expect(created.credentialRef).toBe("provider:vitest-p1");
    // 编辑且不重填 Key、不清除 → 应保留既有引用,而非抹成 undefined
    const edited = await saveProvider({
      id: "vitest-p1",
      name: "p-renamed",
      kind: "openai_compatible",
      enabled: true,
    });
    expect(edited.credentialRef).toBe("provider:vitest-p1");
    // 显式清除 → 置空
    const cleared = await saveProvider(
      { id: "vitest-p1", name: "p", kind: "openai_compatible", enabled: true },
      undefined,
      true,
    );
    expect(cleared.credentialRef).toBeUndefined();
  });
});
