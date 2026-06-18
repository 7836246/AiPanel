import { describe, it, expect } from "vitest";
import {
  parsePrefs,
  normalizeHex,
  resolveDark,
  resolveReduceMotion,
  DEFAULT_APPEARANCE,
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
    expect(normalizeHex("#gggggg")).toBeNull();
  });
});

describe("resolveDark", () => {
  it("dark 恒深;light 恒浅;system 跟随", () => {
    expect(resolveDark("dark", false)).toBe(true);
    expect(resolveDark("light", true)).toBe(false);
    expect(resolveDark("system", true)).toBe(true);
    expect(resolveDark("system", false)).toBe(false);
  });
});

describe("resolveReduceMotion", () => {
  it("on 恒减;off 恒不减;system 跟随", () => {
    expect(resolveReduceMotion("on", false)).toBe(true);
    expect(resolveReduceMotion("off", true)).toBe(false);
    expect(resolveReduceMotion("system", true)).toBe(true);
    expect(resolveReduceMotion("system", false)).toBe(false);
  });
});

describe("parsePrefs", () => {
  it("空输入 → 默认", () => {
    expect(parsePrefs(null)).toEqual(DEFAULT_APPEARANCE);
    expect(parsePrefs("garbage")).toEqual(DEFAULT_APPEARANCE);
  });
  it("从旧 aipanel-theme 迁移 mode", () => {
    expect(parsePrefs(null, "dark").mode).toBe("dark");
    expect(parsePrefs(null, "light").mode).toBe("light");
    expect(parsePrefs(null, null).mode).toBe("system");
  });
  it("校验并归一字段(非法值回落默认;accent 归一)", () => {
    const p = parsePrefs({ mode: "dark", accent: "#ABC", motion: "on", pointer: true });
    expect(p).toEqual({ mode: "dark", accent: "#aabbcc", motion: "on", pointer: true });
    const bad = parsePrefs({ mode: "x", accent: "nope", motion: "y", pointer: "yes" });
    expect(bad).toEqual({ mode: "system", accent: null, motion: "system", pointer: false });
  });
});
