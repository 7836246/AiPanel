/**
 * 外观偏好(主题模式 + 浅/深主题各自的强调色/背景/前景/字体/对比度/半透明侧栏 + 动效 + 光标)。
 *
 * 持久化在 localStorage;应用方式是给 <html> 切 class / 写 CSS 变量,全局即时生效,不依赖后端。
 * surface/border/muted 等派生色由 bg/fg + 对比度经 color-mix 推导,使自定义颜色保持协调。
 * 纯逻辑(parse/normalizeHex/relLuminance/resolve* 等)与 DOM 应用分离,便于单测。
 */

export type ThemeMode = "system" | "light" | "dark";
export type MotionPref = "system" | "on" | "off";

/** 单个主题(浅或深)的可定制项。 */
export interface ThemeColors {
  accent: string; // 强调色 hex
  bg: string; // 背景 hex
  fg: string; // 前景(文字) hex
  uiFont: string; // UI 字体 font-family
  codeFont: string; // 代码字体 font-family
  translucentSidebar: boolean; // 半透明侧边栏
  contrast: number; // 对比度 0–100(影响边框/次要文字与背景的分离度)
}

export interface AppearancePrefs {
  mode: ThemeMode;
  motion: MotionPref;
  pointer: boolean;
  light: ThemeColors;
  dark: ThemeColors;
}

const UI_FONT =
  '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", system-ui, "PingFang SC", "Microsoft YaHei", sans-serif';
const CODE_FONT = '"SF Mono", ui-monospace, "JetBrains Mono", Menlo, Consolas, monospace';

export const DEFAULT_LIGHT: ThemeColors = {
  accent: "#339cff",
  bg: "#ffffff",
  fg: "#1a1c1f",
  uiFont: UI_FONT,
  codeFont: CODE_FONT,
  translucentSidebar: true,
  contrast: 45,
};
export const DEFAULT_DARK: ThemeColors = {
  accent: "#339cff",
  bg: "#181818",
  fg: "#ffffff",
  uiFont: UI_FONT,
  codeFont: CODE_FONT,
  translucentSidebar: true,
  contrast: 60,
};
export const DEFAULT_APPEARANCE: AppearancePrefs = {
  mode: "system",
  motion: "system",
  pointer: false,
  light: DEFAULT_LIGHT,
  dark: DEFAULT_DARK,
};

/** 强调色预设(快速选;也可在「外观」里逐主题改 hex)。 */
export const ACCENT_PRESETS: { id: string; label: string; value: string }[] = [
  { id: "blue", label: "蓝", value: "#339cff" },
  { id: "ink", label: "墨", value: "#1a1a1c" },
  { id: "green", label: "绿", value: "#16a34a" },
  { id: "purple", label: "紫", value: "#7c5cff" },
  { id: "orange", label: "橙", value: "#ea7a3b" },
  { id: "pink", label: "粉", value: "#e25fb0" },
];

const KEY = "aipanel-appearance";
const LEGACY_THEME_KEY = "aipanel-theme";
export const APPEARANCE_EVENT = "aipanel-appearance-changed";

/** 归一化十六进制颜色:接受 #abc / abc / #aabbcc,返回小写 #aabbcc;非法返回 null。 */
export function normalizeHex(v: string): string | null {
  const m = v.trim().match(/^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/);
  if (!m) return null;
  let h = m[1];
  if (h.length === 3)
    h = h
      .split("")
      .map((c) => c + c)
      .join("");
  return `#${h.toLowerCase()}`;
}

/** 相对亮度(0–1),用于判断强调色上应配白字还是黑字。 */
export function relLuminance(hex: string): number {
  const n = normalizeHex(hex);
  if (!n) return 0;
  const ch = (i: number) => {
    const s = parseInt(n.slice(1 + i * 2, 3 + i * 2), 16) / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * ch(0) + 0.7152 * ch(1) + 0.0722 * ch(2);
}

/** 强调色上的可读前景:亮色配近黑、暗色配白。 */
export function readableOn(hex: string): string {
  return relLuminance(hex) > 0.5 ? "#0d0d0f" : "#ffffff";
}

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
}
export function resolveDark(mode: ThemeMode, prefersDark: boolean = systemPrefersDark()): boolean {
  return mode === "dark" || (mode === "system" && prefersDark);
}
export function resolveReduceMotion(
  motion: MotionPref,
  systemReduce: boolean = typeof window !== "undefined" && !!window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
): boolean {
  return motion === "on" || (motion === "system" && systemReduce);
}

/** 由对比度(0–100)推导边框/次要文字的混色百分比(纯函数,便于单测)。 */
export function contrastMixes(contrast: number): { border: number; muted: number } {
  const ct = Math.min(100, Math.max(0, contrast));
  return {
    border: Math.round(6 + (ct / 100) * 16), // 6–22%:fg 混入 bg 的比例(越高边框越明显)
    muted: Math.round(45 - (ct / 100) * 25), // 45–20%:bg 混入 fg 的比例(越高对比越强、次要文字越实)
  };
}

// 校验/补齐单个主题。
function parseTheme(raw: unknown, fallback: ThemeColors, legacyAccent?: string | null): ThemeColors {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const hex = (v: unknown, d: string) => (typeof v === "string" ? (normalizeHex(v) ?? d) : d);
  const font = (v: unknown, d: string) => (typeof v === "string" && v.trim() ? v : d);
  return {
    accent: hex(o.accent ?? legacyAccent ?? undefined, fallback.accent),
    bg: hex(o.bg, fallback.bg),
    fg: hex(o.fg, fallback.fg),
    uiFont: font(o.uiFont, fallback.uiFont),
    codeFont: font(o.codeFont, fallback.codeFont),
    translucentSidebar: typeof o.translucentSidebar === "boolean" ? o.translucentSidebar : fallback.translucentSidebar,
    contrast: typeof o.contrast === "number" && Number.isFinite(o.contrast) ? Math.min(100, Math.max(0, o.contrast)) : fallback.contrast,
  };
}

/** 校验任意输入为合法 AppearancePrefs(含从旧扁平结构 {mode,accent,...} 迁移)。 */
export function parsePrefs(raw: unknown, legacyTheme?: string | null): AppearancePrefs {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const mode: ThemeMode =
    o.mode === "light" || o.mode === "dark" || o.mode === "system"
      ? o.mode
      : legacyTheme === "dark"
        ? "dark"
        : legacyTheme === "light"
          ? "light"
          : "system";
  const motion: MotionPref = o.motion === "on" || o.motion === "off" || o.motion === "system" ? o.motion : "system";
  const legacyAccent = typeof o.accent === "string" ? o.accent : null; // 旧扁平结构里的单一强调色
  return {
    mode,
    motion,
    pointer: o.pointer === true,
    light: parseTheme(o.light, DEFAULT_LIGHT, legacyAccent),
    dark: parseTheme(o.dark, DEFAULT_DARK, legacyAccent),
  };
}

export function readAppearance(): AppearancePrefs {
  try {
    const raw = localStorage.getItem(KEY);
    const legacy = localStorage.getItem(LEGACY_THEME_KEY);
    return parsePrefs(raw ? JSON.parse(raw) : null, legacy);
  } catch {
    return structuredCloneSafe(DEFAULT_APPEARANCE);
  }
}

function structuredCloneSafe(p: AppearancePrefs): AppearancePrefs {
  return { ...p, light: { ...p.light }, dark: { ...p.dark } };
}

export function writeAppearance(p: AppearancePrefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // 隐私模式等不可写,静默忽略(仍即时应用)。
  }
}

const mix = (top: string, pct: number, base: string) => `color-mix(in srgb, ${top} ${pct}%, ${base})`;

// 把单个主题的颜色/字体/派生色写到 <html> 内联样式(内联优先级最高,覆盖 @theme 与 .dark)。
function applyThemeColors(root: HTMLElement, c: ThemeColors): void {
  const set = (k: string, v: string) => root.style.setProperty(k, v);
  set("--color-bg", c.bg);
  set("--color-fg", c.fg);
  set("--color-brand", c.accent);
  set("--color-brand-strong", c.accent);
  set("--color-brand-fg", readableOn(c.accent));
  set("--color-accent", c.accent);
  set("--color-accent-strong", c.accent);
  set("--font-sans", c.uiFont);
  set("--font-mono", c.codeFont);
  const { border, muted } = contrastMixes(c.contrast);
  set("--color-surface-1", mix(c.fg, 2.5, c.bg));
  set("--color-surface-2", mix(c.fg, 5, c.bg));
  set("--color-surface-3", mix(c.fg, 8, c.bg));
  set("--color-hover", mix(c.fg, 6, c.bg));
  set("--color-selected", mix(c.fg, 10, c.bg));
  set("--color-border", mix(c.fg, border, c.bg));
  set("--color-border-strong", mix(c.fg, border + 6, c.bg));
  set("--color-fg-muted", mix(c.bg, muted, c.fg));
  set("--color-fg-subtle", mix(c.bg, muted + 12, c.fg));
}

/** 把外观偏好应用到 <html>。 */
export function applyAppearance(p: AppearancePrefs): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const dark = resolveDark(p.mode);
  const c = dark ? p.dark : p.light;
  root.classList.toggle("dark", dark);
  applyThemeColors(root, c);
  root.classList.toggle("reduce-motion", resolveReduceMotion(p.motion));
  root.classList.toggle("pointer-cursor", p.pointer);
  root.classList.toggle("translucent-sidebar", c.translucentSidebar);
}

/** 应用 + 持久化 + 派发同步事件(供顶栏等刷新)。 */
export function setAppearance(p: AppearancePrefs): void {
  applyAppearance(p);
  writeAppearance(p);
  if (typeof window !== "undefined") window.dispatchEvent(new CustomEvent(APPEARANCE_EVENT, { detail: p }));
}
