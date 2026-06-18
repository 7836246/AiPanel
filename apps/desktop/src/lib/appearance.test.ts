import { describe, it, expect } from "vitest";
import {
  parsePrefs,
  normalizeHex,
  resolveDark,
  resolveReduceMotion,
  readableOn,
  contrastMixes,
  DEFAULT_APPEARANCE,
  DEFAULT_LIGHT,
  DEFAULT_DARK,
} from "./appearance";

describe("normalizeHex", () => {
  it("接受 #abc / abc / #aabbcc,归一为小写 #aabbcc", () => {
    expect(normalizeHex("#abc")).toBe("#aabbcc");
    expect(normalizeHex("ABCDEF")).toBe("#abcdef");
    expect(normalizeHex("  #3399FF ")).toBe("#3399ff");
  });
  it("非法 → null", () => {
    expect(normalizeHex("")).toBeNull();
    expect(normalizeHex("#12")).toBeNull();
    expect(normalizeHex("red")).toBeNull();
  });
});

describe("readableOn", () => {
  it("亮色配近黑、暗色配白", () => {
    expect(readableOn("#ffffff")).toBe("#0d0d0f");
    expect(readableOn("#000000")).toBe("#ffffff");
    expect(readableOn("#181818")).toBe("#ffffff");
    expect(readableOn("#339cff")).toBe("#ffffff"); // 中蓝 → 白字
  });
});

describe("resolveDark / resolveReduceMotion", () => {
  it("主题模式解析", () => {
    expect(resolveDark("dark", false)).toBe(true);
    expect(resolveDark("light", true)).toBe(false);
    expect(resolveDark("system", true)).toBe(true);
    expect(resolveDark("system", false)).toBe(false);
  });
  it("动效解析", () => {
    expect(resolveReduceMotion("on", false)).toBe(true);
    expect(resolveReduceMotion("off", true)).toBe(false);
    expect(resolveReduceMotion("system", true)).toBe(true);
  });
});

describe("contrastMixes", () => {
  it("对比度越高:边框混色越多、次要文字混入背景越少(更实);并被钳到 0–100", () => {
    const lo = contrastMixes(0);
    const hi = contrastMixes(100);
    expect(lo.border).toBeLessThan(hi.border);
    expect(lo.muted).toBeGreaterThan(hi.muted);
    expect(contrastMixes(-50)).toEqual(contrastMixes(0));
    expect(contrastMixes(999)).toEqual(contrastMixes(100));
  });
});

describe("parsePrefs", () => {
  it("空输入 → 默认(含浅/深两套主题)", () => {
    expect(parsePrefs(null)).toEqual(DEFAULT_APPEARANCE);
    expect(parsePrefs("garbage")).toEqual(DEFAULT_APPEARANCE);
    expect(parsePrefs(null).light).toEqual(DEFAULT_LIGHT);
    expect(parsePrefs(null).dark).toEqual(DEFAULT_DARK);
  });
  it("从旧 aipanel-theme 迁移 mode", () => {
    expect(parsePrefs(null, "dark").mode).toBe("dark");
    expect(parsePrefs(null, "light").mode).toBe("light");
  });
  it("旧扁平结构里的单一 accent 迁移到浅/深两套", () => {
    const p = parsePrefs({ mode: "dark", accent: "#abc" });
    expect(p.light.accent).toBe("#aabbcc");
    expect(p.dark.accent).toBe("#aabbcc");
  });
  it("逐主题字段校验:非法颜色/字体/对比度回落默认,合法保留", () => {
    const p = parsePrefs({
      light: { accent: "#10b981", bg: "nope", contrast: 999, translucentSidebar: false, uiFont: "" },
    });
    expect(p.light.accent).toBe("#10b981");
    expect(p.light.bg).toBe(DEFAULT_LIGHT.bg); // 非法颜色回落
    expect(p.light.contrast).toBe(100); // 钳制
    expect(p.light.translucentSidebar).toBe(false);
    expect(p.light.uiFont).toBe(DEFAULT_LIGHT.uiFont); // 空字体回落
  });
});
