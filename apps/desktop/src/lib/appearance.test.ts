import { describe, it, expect } from "vitest";
import {
  parsePrefs,
  normalizeHex,
  resolveDark,
  resolveReduceMotion,
  readableOn,
  contrastMixes,
  applyAppearance,
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

// 回归:之前 applyAppearance 无条件用 color-mix 覆写全站 token,默认就把配色改坏。
// 现在只覆写用户真正改过的部分,等于默认时清除内联,让 tokens.css 规则。
describe("applyAppearance — 默认不覆写 token,仅改动项才覆写", () => {
  const root = () => document.documentElement;
  const v = (k: string) => root().style.getPropertyValue(k);

  it("默认偏好:清除全部内联覆写(含先前脏值)", () => {
    root().style.setProperty("--color-brand", "#123456");
    root().style.setProperty("--color-surface-1", "#abcabc");
    applyAppearance(DEFAULT_APPEARANCE);
    expect(v("--color-brand")).toBe("");
    expect(v("--color-surface-1")).toBe("");
    expect(v("--color-bg")).toBe("");
    expect(v("--font-sans")).toBe("");
  });

  it("仅改强调色:覆写 brand,但不派生/覆写表层色", () => {
    root().style.cssText = "";
    applyAppearance({ ...DEFAULT_APPEARANCE, mode: "light", light: { ...DEFAULT_LIGHT, accent: "#10b981" } });
    expect(v("--color-brand")).toBe("#10b981");
    expect(v("--color-surface-1")).toBe(""); // 未改 bg/fg → 不派生
    expect(v("--color-bg")).toBe("");
  });

  it("改背景:覆写 bg 并派生 surface;未改的强调色不覆写", () => {
    root().style.cssText = "";
    applyAppearance({ ...DEFAULT_APPEARANCE, mode: "light", light: { ...DEFAULT_LIGHT, bg: "#101014" } });
    expect(v("--color-bg")).toBe("#101014");
    expect(v("--color-surface-1")).not.toBe(""); // 已派生
    expect(v("--color-brand")).toBe(""); // accent 未改 → 不覆写
  });
});
