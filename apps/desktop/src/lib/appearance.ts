/**
 * 外观偏好(主题模式 / 强调色 / 减少动态效果 / 指针光标)的读写与应用。
 *
 * 持久化在 localStorage;应用方式是给 <html> 切换 class / 写 CSS 变量,因此对全局即时生效,
 * 不依赖任何后端。纯逻辑(resolveDark/normalizeHex/parsePrefs)与 DOM 应用分离,便于单测。
 */

export type ThemeMode = "system" | "light" | "dark";
/** 减少动态效果:system=跟随系统、on=开启(减少)、off=关闭(保留动效)。 */
export type MotionPref = "system" | "on" | "off";

export interface AppearancePrefs {
  mode: ThemeMode;
  /** 强调色十六进制(如 #3399ff);null = 使用主题默认(近黑/近白)。 */
  accent: string | null;
  motion: MotionPref;
  /** 悬停交互元素时切换为指针光标。 */
  pointer: boolean;
}

export const DEFAULT_APPEARANCE: AppearancePrefs = {
  mode: "system",
  accent: null,
  motion: "system",
  pointer: false,
};

/** 强调色预设(null = 跟随主题默认)。 */
export const ACCENT_PRESETS: { id: string; label: string; value: string | null }[] = [
  { id: "default", label: "默认", value: null },
  { id: "blue", label: "蓝", value: "#3399ff" },
  { id: "green", label: "绿", value: "#16a34a" },
  { id: "purple", label: "紫", value: "#7c5cff" },
  { id: "orange", label: "橙", value: "#ea7a3b" },
  { id: "pink", label: "粉", value: "#e25fb0" },
];

const KEY = "aipanel-appearance";
const LEGACY_THEME_KEY = "aipanel-theme"; // 旧的 light/dark 开关,迁移用
/** 外观变更后派发的全局事件名,供其它组件(如顶栏主题图标)同步。 */
export const APPEARANCE_EVENT = "aipanel-appearance-changed";

/** 校验任意输入为合法的 AppearancePrefs,缺字段用默认补齐(纯函数,便于单测)。 */
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
  const motion: MotionPref =
    o.motion === "on" || o.motion === "off" || o.motion === "system" ? o.motion : "system";
  const accent = typeof o.accent === "string" ? normalizeHex(o.accent) : null;
  return { mode, accent, motion, pointer: o.pointer === true };
}

/** 归一化十六进制颜色:接受 #abc / abc / #aabbcc,返回小写 #aabbcc;非法返回 null。 */
export function normalizeHex(v: string): string | null {
  const m = v.trim().match(/^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/);
  if (!m) return null;
  let h = m[1];
  if (h.length === 3) {
    h = h
      .split("")
      .map((c) => c + c)
      .join("");
  }
  return `#${h.toLowerCase()}`;
}

/** 当前系统是否偏好深色。 */
export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** 由主题模式解析出最终是否深色(system 跟随系统)。 */
export function resolveDark(mode: ThemeMode, prefersDark: boolean = systemPrefersDark()): boolean {
  return mode === "dark" || (mode === "system" && prefersDark);
}

/** 由 motion 偏好解析出是否应减少动效(system 跟随 prefers-reduced-motion)。 */
export function resolveReduceMotion(
  motion: MotionPref,
  systemReduce: boolean = typeof window !== "undefined" && !!window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
): boolean {
  return motion === "on" || (motion === "system" && systemReduce);
}

/** 读取外观偏好(含从旧 aipanel-theme 迁移);异常时回退默认。 */
export function readAppearance(): AppearancePrefs {
  try {
    const raw = localStorage.getItem(KEY);
    const legacy = localStorage.getItem(LEGACY_THEME_KEY);
    return parsePrefs(raw ? JSON.parse(raw) : null, legacy);
  } catch {
    return { ...DEFAULT_APPEARANCE };
  }
}

/** 持久化外观偏好。 */
export function writeAppearance(p: AppearancePrefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // 隐私模式等场景不可写,静默忽略(仍会即时应用)。
  }
}

/** 把外观偏好应用到 <html>:深色 class、强调色 CSS 变量、减少动效 class、指针光标 class。 */
export function applyAppearance(p: AppearancePrefs): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.toggle("dark", resolveDark(p.mode));
  // 强调色:内联写 <html> style 覆盖 @theme 与 .dark 的 --color-brand(内联优先级最高,光/暗皆生效)。
  if (p.accent) {
    root.style.setProperty("--color-brand", p.accent);
    root.style.setProperty("--color-brand-strong", p.accent);
    root.style.setProperty("--color-brand-fg", "#ffffff");
  } else {
    root.style.removeProperty("--color-brand");
    root.style.removeProperty("--color-brand-strong");
    root.style.removeProperty("--color-brand-fg");
  }
  root.classList.toggle("reduce-motion", resolveReduceMotion(p.motion));
  root.classList.toggle("pointer-cursor", p.pointer);
}

/** 应用 + 持久化 + 派发同步事件(供顶栏等组件刷新)。 */
export function setAppearance(p: AppearancePrefs): void {
  applyAppearance(p);
  writeAppearance(p);
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(APPEARANCE_EVENT, { detail: p }));
  }
}
