import { useEffect, useState, type JSX } from "react";
import { Badge, Button, Input, Spinner, ToastViewport, useToasts } from "@aipanel/ui";
import {
  ArrowLeft,
  Check,
  Cpu,
  DownloadCloud,
  Palette,
  Pencil,
  Plus,
  RefreshCw,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  X,
} from "lucide-react";
import {
  appVersion,
  checkUpdate,
  credentialBackend,
  deleteProvider,
  downloadAndInstallUpdate,
  getModelSelectionPolicy,
  listModels,
  listProviders,
  saveModelSelectionPolicy,
  saveProvider,
  testProvider,
  type ModelSelectionPolicy,
  type ProviderConfig,
  type ProviderInput,
  type ProviderKind,
  type ProviderTestResult,
  type UpdateInfo,
} from "../lib/api";
import {
  READONLY_DEFAULT_KEY,
  readUpdateAutoCheck,
  writeUpdateAutoCheck,
} from "./settingsKeys";
import {
  readAppearance,
  setAppearance,
  normalizeHex,
  DEFAULT_LIGHT,
  DEFAULT_DARK,
  type AppearancePrefs,
  type ThemeColors,
  type ThemeMode,
  type MotionPref,
} from "../lib/appearance";

// 主题模式预览小卡:用固定的浅/深底色画一个迷你窗口(侧栏 + 内容条),不随当前主题变化。
function ThemePreview({ mode }: { mode: ThemeMode }): JSX.Element {
  const light = { bg: "#ffffff", side: "#f1f2f4", bar: "#d6d8dc" };
  const dark = { bg: "#1a1a1c", side: "#242427", bar: "#3a3a3e" };
  const Mini = ({ c }: { c: typeof light }) => (
    <div className="flex h-full w-full overflow-hidden" style={{ background: c.bg }}>
      <div className="h-full w-1/3" style={{ background: c.side }} />
      <div className="flex flex-1 flex-col gap-1 p-1.5">
        <div className="h-1 w-3/4 rounded-full" style={{ background: c.bar }} />
        <div className="h-1 w-1/2 rounded-full" style={{ background: c.bar }} />
        <div className="h-1 w-2/3 rounded-full" style={{ background: c.bar }} />
      </div>
    </div>
  );
  if (mode === "light") return <Mini c={light} />;
  if (mode === "dark") return <Mini c={dark} />;
  // 系统:左浅右深,斜分。
  return (
    <div className="flex h-full w-full overflow-hidden">
      <div className="h-full w-1/2">
        <Mini c={light} />
      </div>
      <div className="h-full w-1/2">
        <Mini c={dark} />
      </div>
    </div>
  );
}

// 单个颜色字段:label + 右侧(原生取色器色块 + 可编辑 hex)。
function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (hex: string) => void }): JSX.Element {
  const [hex, setHex] = useState(value);
  useEffect(() => setHex(value), [value]); // 外部值变化(切主题/重置)时同步
  const commit = (v: string) => {
    const n = normalizeHex(v);
    if (n) onChange(n);
    else setHex(value);
  };
  return (
    <div className="flex items-center justify-between py-2">
      <span className="text-[13px] text-fg">{label}</span>
      <label className="flex items-center gap-2 rounded-full border border-border bg-surface-2 px-2 py-1">
        <span className="relative h-4 w-4 overflow-hidden rounded-full border border-border" style={{ background: value }}>
          <input
            type="color"
            value={normalizeHex(value) ?? "#000000"}
            onChange={(e) => onChange(e.target.value)}
            className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
            aria-label={`${label}取色`}
          />
        </span>
        <input
          value={hex}
          onChange={(e) => setHex(e.target.value)}
          onBlur={() => commit(hex)}
          onKeyDown={(e) => { if (e.key === "Enter") commit(hex); }}
          className="w-20 bg-transparent font-mono text-[12px] uppercase outline-none"
          aria-label={label}
        />
      </label>
    </div>
  );
}

// 字体字段:label + font-family 文本输入。
function FontField({ label, value, placeholder, onChange }: { label: string; value: string; placeholder?: string; onChange: (v: string) => void }): JSX.Element {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  const commit = () => onChange(v.trim() || value);
  return (
    <div className="flex items-center justify-between gap-3 py-2">
      <span className="flex-none text-[13px] text-fg">{label}</span>
      <input
        value={v}
        placeholder={placeholder}
        onChange={(e) => setV(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") commit(); }}
        className="min-w-0 flex-1 truncate rounded-md border border-border bg-surface-2 px-2 py-1 font-mono text-[12px] text-fg-muted outline-none focus:border-brand"
      />
    </div>
  );
}

// 单个主题(浅/深)的编辑器:强调色/背景/前景/UI 字体/代码字体/半透明侧栏/对比度。
function ThemeEditor({ title, value, defaults, onChange, onReset, cardCls }: {
  title: string;
  value: ThemeColors;
  defaults: ThemeColors;
  onChange: (patch: Partial<ThemeColors>) => void;
  onReset: () => void;
  cardCls: string;
}): JSX.Element {
  const isDefault = JSON.stringify(value) === JSON.stringify(defaults);
  return (
    <div className={cardCls}>
      <div className="mb-1 flex items-center justify-between">
        <span className="text-[13px] font-semibold">{title}</span>
        <button
          type="button"
          onClick={onReset}
          disabled={isDefault}
          className="text-[12px] text-fg-subtle transition-colors hover:text-fg disabled:opacity-40"
        >
          重置为默认
        </button>
      </div>
      <ColorField label="强调色" value={value.accent} onChange={(v) => onChange({ accent: v })} />
      <ColorField label="背景" value={value.bg} onChange={(v) => onChange({ bg: v })} />
      <ColorField label="前景" value={value.fg} onChange={(v) => onChange({ fg: v })} />
      <FontField label="UI 字体" value={value.uiFont} placeholder="-apple-system, …" onChange={(v) => onChange({ uiFont: v })} />
      <FontField label="代码字体" value={value.codeFont} placeholder="ui-monospace, …" onChange={(v) => onChange({ codeFont: v })} />
      <label className="flex items-center justify-between py-2 text-[13px] text-fg">
        半透明侧边栏
        <input
          type="checkbox"
          checked={value.translucentSidebar}
          onChange={(e) => onChange({ translucentSidebar: e.target.checked })}
        />
      </label>
      <div className="flex items-center gap-3 py-2">
        <span className="text-[13px] text-fg">对比度</span>
        <input
          type="range"
          min={0}
          max={100}
          value={value.contrast}
          onChange={(e) => onChange({ contrast: Number(e.target.value) })}
          className="ml-auto w-40 accent-[var(--color-brand)]"
          aria-label={`${title}对比度`}
        />
        <span className="w-7 text-right font-mono text-[12px] text-fg-muted">{value.contrast}</span>
      </div>
    </div>
  );
}

// 外观设置:主题模式 + 浅/深主题各自配置 + 减少动态效果 + 指针光标。即时应用 + 持久化。
function AppearanceSection({ cardCls }: { cardCls: string }): JSX.Element {
  const [prefs, setPrefs] = useState<AppearancePrefs>(() => readAppearance());
  const apply = (next: AppearancePrefs) => {
    setPrefs(next);
    setAppearance(next); // 应用到 <html> + 持久化 + 派发同步事件
  };
  const update = (patch: Partial<AppearancePrefs>) => apply({ ...prefs, ...patch });
  const updateTheme = (which: "light" | "dark", patch: Partial<ThemeColors>) =>
    apply({ ...prefs, [which]: { ...prefs[which], ...patch } });
  const MODES: { id: ThemeMode; label: string }[] = [
    { id: "system", label: "系统" },
    { id: "light", label: "浅色" },
    { id: "dark", label: "深色" },
  ];
  const MOTIONS: { id: MotionPref; label: string }[] = [
    { id: "system", label: "系统" },
    { id: "on", label: "开启" },
    { id: "off", label: "关闭" },
  ];
  return (
    <>
      <h2 className="mb-3 mt-8 flex items-center gap-1.5 text-sm font-semibold">
        <Palette size={15} strokeWidth={1.75} className="text-fg-muted" />
        外观
      </h2>

      {/* 主题模式:三张预览卡 */}
      <div className="mb-3 grid grid-cols-3 gap-2.5">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            onClick={() => update({ mode: m.id })}
            aria-pressed={prefs.mode === m.id}
            className={`overflow-hidden rounded-lg border text-left transition-colors ${
              prefs.mode === m.id ? "border-brand ring-1 ring-brand" : "border-border hover:border-border-strong"
            }`}
          >
            <div className="h-16 w-full border-b border-border">
              <ThemePreview mode={m.id} />
            </div>
            <div className="flex items-center justify-between px-2.5 py-1.5 text-[12.5px]">
              <span>{m.label}</span>
              {prefs.mode === m.id && <Check size={13} className="text-brand" />}
            </div>
          </button>
        ))}
      </div>

      {/* 浅色 / 深色主题各自的颜色、字体、侧栏、对比度 */}
      <ThemeEditor
        title="浅色主题"
        value={prefs.light}
        defaults={DEFAULT_LIGHT}
        cardCls={cardCls}
        onChange={(p) => updateTheme("light", p)}
        onReset={() => updateTheme("light", DEFAULT_LIGHT)}
      />
      <div className="h-2.5" />
      <ThemeEditor
        title="深色主题"
        value={prefs.dark}
        defaults={DEFAULT_DARK}
        cardCls={cardCls}
        onChange={(p) => updateTheme("dark", p)}
        onReset={() => updateTheme("dark", DEFAULT_DARK)}
      />

      {/* 减少动态效果 + 指针光标 */}
      <div className={`mt-2.5 ${cardCls}`}>
        <div className="flex items-center gap-2">
          <span className="text-[13px] text-fg">减少动态效果</span>
          <div className="ml-auto inline-flex rounded-md border border-border p-0.5">
            {MOTIONS.map((m) => (
              <button
                key={m.id}
                type="button"
                onClick={() => update({ motion: m.id })}
                className={`rounded px-2.5 py-0.5 text-[12px] transition-colors ${
                  prefs.motion === m.id ? "bg-surface-2 text-fg shadow-sm" : "text-fg-muted hover:text-fg"
                }`}
              >
                {m.label}
              </button>
            ))}
          </div>
        </div>
        <label className="mt-3 flex items-center gap-2 text-[13px] text-fg">
          <input type="checkbox" checked={prefs.pointer} onChange={(e) => update({ pointer: e.target.checked })} />
          悬停可交互元素时使用指针光标
        </label>
      </div>
    </>
  );
}

// 从后端错误或任意异常中提取可展示的错误文本。
const errMsg = (e: unknown): string =>
  e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);

// 供应商类型到中文标签的映射。
const KIND_LABEL: Record<ProviderKind, string> = {
  openai_compatible: "OpenAI 兼容",
  codex_app_server: "Codex app-server",
  custom: "自定义",
};

// 新增供应商表单的初始空值。默认类型为 OpenAI 兼容（贴近 Codex：只需配 Base URL + Key，模型自动探测）。
const EMPTY: ProviderInput = { name: "", kind: "openai_compatible", enabled: true };

/** 读取「默认只读优先」的当前值（缺省为 true，最安全的默认）。 */
function readReadonlyDefault(): boolean {
  try {
    return localStorage.getItem(READONLY_DEFAULT_KEY) !== "false";
  } catch {
    return true;
  }
}

/** 持久化「默认只读优先」开关。 */
function writeReadonlyDefault(value: boolean) {
  try {
    localStorage.setItem(READONLY_DEFAULT_KEY, value ? "true" : "false");
  } catch {
    // 隐私模式等场景下 localStorage 可能不可写，静默忽略。
  }
}

// 设置分类(Codex 式:左侧独立分类导航,右侧只显示选中分类)。
type SettingsNav = "providers" | "appearance" | "general" | "update";

// 设置面板:左侧分类导航 + 右侧对应面板(返回应用 / 搜索 / 分组),不再全堆一页。
export default function SettingsPanel({ onBack }: { onBack?: () => void }) {
  const [nav, setNav] = useState<SettingsNav>("providers");
  const [navQuery, setNavQuery] = useState("");
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [policy, setPolicy] = useState<ModelSelectionPolicy>({ auto: true });
  const [backend, setBackend] = useState<string>("");
  const [form, setForm] = useState<ProviderInput | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [clearApiKey, setClearApiKey] = useState(false);
  const [testResult, setTestResult] = useState<ProviderTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [busy, setBusy] = useState(false);
  // 模型探测：从 OpenAI 兼容供应商拉取可用模型列表（仅本地 state，供下拉选择）。
  const [models, setModels] = useState<string[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [detectError, setDetectError] = useState<string | null>(null);
  // 通用区块：默认只读优先（持久化到 localStorage，供主界面读取）。
  const [readonlyDefault, setReadonlyDefault] = useState<boolean>(true);
  // 在线更新区块。
  const [version, setVersion] = useState<string>("");
  const [autoCheck, setAutoCheck] = useState<boolean>(true);
  // 更新状态机:idle 空闲 / checking 检查中 / latest 已最新 / available 有新版 / downloading 下载安装中 / error 出错。
  const [updPhase, setUpdPhase] = useState<"idle" | "checking" | "latest" | "available" | "downloading" | "error">("idle");
  const [updInfo, setUpdInfo] = useState<UpdateInfo | null>(null);
  const [updError, setUpdError] = useState<string | null>(null);
  // 下载进度(0–100;total 未知时为 null,展示忙碌态)。
  const [updPct, setUpdPct] = useState<number | null>(null);
  const { toasts, push, dismiss } = useToasts();

  // 与后端 candidate_providers 一致：默认供应商只允许选择已启用且可用于规划的 provider；
  // custom 类型不参与规划，不能作为固定默认项。
  const selectableProviders = providers.filter((p) => p.enabled && p.kind !== "custom");

  // 编辑已保存供应商且未重新输入 Key 时，补回确定性的 credentialRef，让后端能从
  // Keychain 取回旧密钥进行探测/测试；清除 Key 时必须避免复用旧凭据。
  function providerProbeConfig(): ProviderInput | ProviderConfig | null {
    if (!form) return null;
    if (clearApiKey || apiKey || !form.id) return form;
    return { ...form, credentialRef: `provider:${form.id}` } as ProviderConfig;
  }

  // 重新拉取供应商列表、模型选择策略与凭据后端标识。
  async function refresh() {
    setProviders(await listProviders());
    setPolicy(await getModelSelectionPolicy());
    setBackend(await credentialBackend());
  }
  useEffect(() => {
    refresh().catch(() => push("danger", "加载设置失败"));
    setReadonlyDefault(readReadonlyDefault());
    setAutoCheck(readUpdateAutoCheck());
    appVersion().then(setVersion).catch(() => setVersion("?"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 编辑某供应商：把其配置回填到表单，并清空 API Key 输入与测试结果。
  function edit(p: ProviderConfig) {
    setForm({
      id: p.id,
      name: p.name,
      kind: p.kind,
      baseUrl: p.baseUrl,
      model: p.model,
      codexPath: p.codexPath,
      enabled: p.enabled,
    });
    setApiKey("");
    setClearApiKey(false);
    setTestResult(null);
    // 回填已保存模型，作为下拉初始候选；探测状态清空。
    setModels(p.model ? [p.model] : []);
    setDetecting(false);
    setDetectError(null);
  }

  // 探测模型：调用 OpenAI 兼容接口拉取可用模型列表，成功后写入本地 state 供下拉选择。
  async function detectModels() {
    if (!form) return;
    setDetecting(true);
    setDetectError(null);
    try {
      const probe = providerProbeConfig() ?? form;
      const raw = await listModels(probe, apiKey || undefined);
      // 兼容返回 string[] 或 { id?/name? }[] 两种形态，归一化为字符串数组。
      const ids = (raw as unknown[])
        .map((m) =>
          typeof m === "string"
            ? m
            : m && typeof m === "object"
              ? String((m as { id?: unknown; name?: unknown }).id ?? (m as { name?: unknown }).name ?? "")
              : "",
        )
        .filter((s) => s.length > 0);
      // 去重并保留已选模型（即便未在返回列表中），避免下拉丢失当前值。
      const merged = Array.from(new Set([...(form.model ? [form.model] : []), ...ids]));
      setModels(merged);
      if (ids.length === 0) {
        setDetectError("未探测到任何模型，请检查 Base URL / API Key 后重试。");
      } else if (!form.model) {
        // 未选过模型时，默认选中第一个探测结果。
        setForm({ ...form, model: ids[0] });
      }
    } catch (e) {
      setDetectError(`探测失败: ${errMsg(e)}`);
    } finally {
      setDetecting(false);
    }
  }

  // 保存表单：API Key 留空默认保留旧凭据；显式勾选时清除已保存 Keychain 凭据。
  async function save() {
    if (!form || !form.name.trim()) return;
    setBusy(true);
    try {
      await saveProvider(form, apiKey || undefined, clearApiKey);
      setForm(null);
      setApiKey("");
      setClearApiKey(false);
      setTestResult(null);
      setModels([]);
      setDetecting(false);
      setDetectError(null);
      await refresh();
      push("success", "供应商已保存");
    } catch (e) {
      push("danger", `保存失败: ${errMsg(e)}`);
    } finally {
      setBusy(false);
    }
  }

  // 删除供应商；若它正是默认供应商，则同时清掉默认选择。
  async function remove(id: string) {
    try {
      await deleteProvider(id);
      await refresh();
      push("success", "供应商已删除");
    } catch (e) {
      await refresh().catch(() => undefined);
      push("danger", `删除失败: ${errMsg(e)}`);
    }
  }

  // 更新模型选择策略并立即持久化。
  async function updatePolicy(next: ModelSelectionPolicy) {
    setPolicy(next);
    try {
      await saveModelSelectionPolicy(next);
    } catch (e) {
      await refresh().catch(() => undefined);
      push("danger", `保存模型选择策略失败: ${errMsg(e)}`);
    }
  }

  // 运行连接测试：测试期间显示 Spinner，结果以颜色区分成功/失败。
  async function runTest() {
    if (!form) return;
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult(await testProvider(providerProbeConfig() ?? form, apiKey || undefined));
    } catch (e) {
      setTestResult({ ok: false, message: "测试请求失败", detail: errMsg(e) });
    } finally {
      setTesting(false);
    }
  }

  // 切换「默认只读优先」并持久化到 localStorage。
  function toggleReadonlyDefault(value: boolean) {
    setReadonlyDefault(value);
    writeReadonlyDefault(value);
    push("info", value ? "已设为默认只读优先(下次启动新会话生效)" : "已关闭默认只读优先(下次启动新会话生效)");
  }

  // 切换「启动检查更新」并持久化。
  function toggleAutoCheck(value: boolean) {
    setAutoCheck(value);
    writeUpdateAutoCheck(value);
  }

  // 手动检查更新:成功后区分「已最新」与「有新版」;失败给可读错误,不影响 app 使用。
  async function runCheckUpdate() {
    setUpdPhase("checking");
    setUpdError(null);
    setUpdInfo(null);
    try {
      const info = await checkUpdate();
      if (info) {
        setUpdInfo(info);
        setUpdPhase("available");
      } else {
        setUpdPhase("latest");
      }
    } catch (e) {
      setUpdError(errMsg(e));
      setUpdPhase("error");
    }
  }

  // 下载并安装当前可用更新,完成后自动重启(由 api 内部 relaunch)。
  async function runInstallUpdate() {
    setUpdPhase("downloading");
    setUpdError(null);
    setUpdPct(0);
    try {
      await downloadAndInstallUpdate((downloaded, total) => {
        setUpdPct(total && total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null);
      });
      // 正常情况下会自动重启;若未重启(如平台差异),给出提示。
      push("success", "更新已安装,请重启应用以生效");
    } catch (e) {
      setUpdError(errMsg(e));
      setUpdPhase("error");
    }
  }

  const field = "mb-2 flex flex-col gap-1";
  const labelCls = "text-[12px] font-medium text-fg-muted";
  const cardCls = "rounded-md border border-border bg-surface-1 p-4";

  const NAV_ITEMS: { id: SettingsNav; label: string; icon: JSX.Element }[] = [
    { id: "providers", label: "模型供应商", icon: <Server size={15} strokeWidth={1.75} /> },
    { id: "appearance", label: "外观", icon: <Palette size={15} strokeWidth={1.75} /> },
    { id: "general", label: "通用", icon: <SlidersHorizontal size={15} strokeWidth={1.75} /> },
    { id: "update", label: "在线更新", icon: <DownloadCloud size={15} strokeWidth={1.75} /> },
  ];
  const navShown = NAV_ITEMS.filter((i) => !navQuery.trim() || i.label.includes(navQuery.trim()));
  return (
    <section className="flex min-h-0 flex-1">
      {/* 左侧:返回应用 + 搜索 + 分类导航(Codex 式独立设置页) */}
      <nav className="flex w-56 flex-none flex-col gap-1 overflow-y-auto border-r border-border bg-surface-2 px-2.5 py-3">
        <button
          type="button"
          onClick={() => onBack?.()}
          className="mb-1 flex items-center gap-1.5 rounded-md px-2 py-1.5 text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg"
        >
          <ArrowLeft size={15} /> 返回应用
        </button>
        <input
          value={navQuery}
          onChange={(e) => setNavQuery(e.target.value)}
          placeholder="搜索设置…"
          className="mb-2 w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-[12.5px] outline-none focus:border-brand"
        />
        <div className="px-2 pb-1 text-[11px] uppercase tracking-wide text-fg-subtle">个人</div>
        {navShown.map((it) => (
          <button
            key={it.id}
            type="button"
            onClick={() => setNav(it.id)}
            aria-current={nav === it.id}
            className={`flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] transition-colors ${
              nav === it.id ? "bg-selected font-medium text-fg" : "text-fg-muted hover:bg-hover hover:text-fg"
            }`}
          >
            <span className={nav === it.id ? "text-brand" : "text-fg-subtle"}>{it.icon}</span>
            {it.label}
          </button>
        ))}
        {navShown.length === 0 && <div className="px-2 py-2 text-[12px] text-fg-subtle">无匹配设置</div>}
      </nav>

      {/* 右侧:仅渲染选中分类的面板 */}
      <div className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[680px] px-6 pb-8 pt-5">
        {nav === "providers" && (
        <>
        <h2 className="mb-3 flex items-center gap-1.5 text-sm font-semibold">
          <Server size={15} strokeWidth={1.75} className="text-fg-muted" />
          模型供应商
        </h2>

        {/* 凭据后端提示：mock 给出警告样式，keychain 给出安全提示 */}
        {backend === "mock" ? (
          <div className="mb-3 rounded-md border border-risk-medium/40 bg-risk-medium-soft px-3 py-2 text-[12.5px] text-risk-medium">
            <span className="font-medium">凭据当前存放在内存（开发兜底）。</span>
            重启后将丢失，且仅用于开发调试。生产环境会自动使用系统 Keychain。
          </div>
        ) : backend ? (
          <div className="mb-3 flex items-center gap-2 rounded-md border border-border bg-surface-2 px-3 py-2 text-[12.5px] text-fg-muted">
            <Badge tone="success">
              <ShieldCheck size={12} strokeWidth={1.75} />
              Keychain
            </Badge>
            凭据由系统 Keychain 安全保管，不写入数据库或日志。
          </div>
        ) : null}

        {/* 供应商列表 */}
        <div className="flex flex-col gap-2">
          {providers.length === 0 && !form ? (
            <div className="rounded-md border border-border bg-surface-1 px-4 py-6 text-center text-[13px] text-fg-subtle">
              还没有配置供应商。
            </div>
          ) : (
            providers.map((p) => (
              <div key={p.id} className="flex items-center gap-3 rounded-md border border-border bg-surface-1 px-4 py-3">
                {/* 供应商行图标 */}
                <Server size={16} strokeWidth={1.75} className="flex-none text-fg-subtle" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[13.5px] font-medium">{p.name}</span>
                    <Badge tone={p.enabled ? "success" : "neutral"}>{p.enabled ? "启用" : "停用"}</Badge>
                    {policy.defaultProviderId === p.id ? <Badge tone="brand">默认</Badge> : null}
                  </div>
                  <div className="mt-0.5 text-[12px] text-fg-muted">
                    {KIND_LABEL[p.kind]}
                    {p.model ? ` · ${p.model}` : ""}
                    {p.baseUrl ? ` · ${p.baseUrl}` : ""}
                  </div>
                </div>
                <Button variant="ghost" size="sm" className="gap-1.5" onClick={() => edit(p)}>
                  <Pencil size={14} />
                  编辑
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="gap-1.5"
                  onClick={() => {
                    // 供应商可能含 Keychain 凭据引用，误删代价高，删除前二次确认。
                    if (window.confirm(`确认删除供应商「${p.name}」？`)) remove(p.id);
                  }}
                >
                  <Trash2 size={14} />
                  删除
                </Button>
              </div>
            ))
          )}
        </div>

        {/* 新增 / 编辑表单 */}
        {form ? (
          <div className="mt-3 rounded-md border border-border bg-surface-1 p-4">
            <div className="mb-3 flex items-center gap-1.5 text-[13px] font-semibold">
              {form.id ? <Pencil size={14} className="text-fg-muted" /> : <Plus size={14} className="text-fg-muted" />}
              {form.id ? "编辑供应商" : "新增供应商"}
            </div>
            <div className={field}>
              <label className={labelCls}>名称</label>
              <Input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="例如 Codex" />
            </div>
            {/* 只支持 OpenAI 兼容供应商;codex 是其底层运行时(端点支持 Responses 时自动启用),不作为单独类型暴露。 */}
            <div className={field}>
              <label className={labelCls}>Base URL</label>
              <Input
                value={form.baseUrl ?? ""}
                onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
                placeholder="https://api.example.com/v1"
              />
              <span className="text-[12px] text-fg-subtle">
                只需填写 Base URL 与 API Key,模型可一键探测后下拉选择。OpenAI 官方 / 兼容 Responses 的端点会自动用打包的 codex 引擎,否则走直连。
              </span>
            </div>
            <div className={field}>
              <label className={labelCls}>API Key（仅存本地 Keychain，不进数据库）</label>
              <Input
                type="password"
                value={apiKey}
                onChange={(e) => {
                  setApiKey(e.target.value);
                  if (e.target.value) setClearApiKey(false);
                }}
                placeholder={form.id ? "留空则保留已保存密钥" : "sk-…"}
                disabled={clearApiKey}
              />
              {form.id ? (
                <label className="mt-1 flex items-center gap-2 text-[12px] text-fg-muted">
                  <input
                    type="checkbox"
                    checked={clearApiKey}
                    onChange={(e) => {
                      setClearApiKey(e.target.checked);
                      if (e.target.checked) setApiKey("");
                    }}
                  />
                  清除已保存 API Key
                </label>
              ) : null}
            </div>
            {/* 探测 + 下拉选择模型（贴近 Codex 体验），并保留手填兜底。 */}
            <div className={field}>
              <label className={labelCls}>模型</label>
              <div className="flex items-center gap-2">
                <Button
                  variant="secondary"
                  size="sm"
                  className="gap-1.5"
                  onClick={detectModels}
                  disabled={detecting || !form.baseUrl?.trim()}
                  title={!form.baseUrl?.trim() ? "请先填写 Base URL" : undefined}
                >
                  {detecting ? <Spinner size="sm" /> : <RefreshCw size={14} />}
                  {detecting ? "探测中…" : "探测模型"}
                </Button>
                <select
                  value={form.model ?? ""}
                  onChange={(e) => setForm({ ...form, model: e.target.value || undefined })}
                  disabled={detecting || models.length === 0}
                  className="h-9 min-w-0 flex-1 rounded-md border border-border bg-surface-2 px-2 text-sm text-fg outline-none focus-visible:border-brand disabled:opacity-60"
                >
                  {/* 占位文案三态：探测中 / 探测前或失败 / 已有结果，与按钮状态保持一致 */}
                  <option value="">
                    {detecting ? "（探测中…）" : models.length === 0 ? "（请先探测模型）" : "（选择模型）"}
                  </option>
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </div>
              {/* 探测失败内联红条提示 */}
              {detectError ? (
                <div className="mt-1 rounded-md border border-risk-blocked/40 bg-risk-blocked-soft px-2.5 py-1.5 text-[12px] text-risk-blocked">
                  {detectError}
                </div>
              ) : null}
              {/* 手填兜底：仅在探测无结果或探测失败时显示，避免与下拉并列造成「需再填一次」的误解 */}
              {(!models.length || detectError) && (
                <Input
                  className="mt-1"
                  value={form.model ?? ""}
                  onChange={(e) => setForm({ ...form, model: e.target.value || undefined })}
                  placeholder="探测不可用时手填模型名，如 gpt-4o-mini"
                />
              )}
            </div>
            <label className="mb-3 flex items-center gap-2 text-[13px] text-fg">
              <input type="checkbox" checked={form.enabled} onChange={(e) => setForm({ ...form, enabled: e.target.checked })} />
              启用
            </label>
            {/* 测试连接结果：测试中显示 Spinner，成功/失败用不同语气色 + detail 文案 */}
            {testing ? (
              <div className="mb-3 flex items-center gap-2 text-[12.5px] text-fg-muted">
                <Spinner size="sm" />
                正在测试连接…
              </div>
            ) : testResult ? (
              <div
                className={`mb-3 rounded-md border px-3 py-2 text-[12.5px] ${
                  testResult.ok
                    ? "border-risk-low/40 bg-risk-low-soft text-risk-low"
                    : "border-risk-blocked/40 bg-risk-blocked-soft text-risk-blocked"
                }`}
              >
                <span className="inline-flex items-center gap-1 font-medium align-middle">
                  {testResult.ok ? <Check size={14} strokeWidth={2} /> : <X size={14} strokeWidth={2} />}
                  {testResult.ok ? "连接成功" : "连接失败"}
                </span>
                {testResult.message ? ` — ${testResult.message}` : ""}
                {testResult.detail ? <div className="mt-0.5 opacity-80">{testResult.detail}</div> : null}
              </div>
            ) : null}
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" onClick={save} disabled={busy || !form.name.trim()}>
                {busy ? "保存中…" : "保存"}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={runTest}
                disabled={testing || !form.baseUrl?.trim()}
                title={!form.baseUrl?.trim() ? "请先填写 Base URL" : undefined}
              >
                {testing ? "测试中…" : "测试连接"}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setForm(null);
                  setClearApiKey(false);
                  setTestResult(null);
                  setTesting(false);
                  setModels([]);
                  setDetecting(false);
                  setDetectError(null);
                }}
              >
                取消
              </Button>
            </div>
          </div>
        ) : (
          <Button variant="secondary" size="sm" className="mt-3 gap-1.5" onClick={() => { setForm({ ...EMPTY }); setApiKey(""); setClearApiKey(false); setTestResult(null); setModels([]); setDetecting(false); setDetectError(null); }}>
            <Plus size={14} />
            添加供应商
          </Button>
        )}

        {/* 模型选择策略 */}
        <h2 className="mb-3 mt-8 flex items-center gap-1.5 text-sm font-semibold">
          <Cpu size={15} strokeWidth={1.75} className="text-fg-muted" />
          模型选择
        </h2>
        <div className={cardCls}>
          <label className="flex items-center gap-2 text-[13px] text-fg">
            <input
              type="checkbox"
              checked={policy.auto}
              onChange={(e) => updatePolicy({ ...policy, auto: e.target.checked })}
            />
            自动按任务选择模型
          </label>
          <p className="mt-1 text-[12px] text-fg-subtle">
            开启后由 AiPanel 按任务在已启用供应商间自动选择；关闭后固定使用下方默认供应商。
          </p>
          {!policy.auto && (
            <div className="mt-3 flex flex-col gap-1">
              <label className={labelCls}>默认供应商</label>
              <select
                value={policy.defaultProviderId ?? ""}
                onChange={(e) => updatePolicy({ ...policy, defaultProviderId: e.target.value || undefined })}
                className="h-9 rounded-md border border-border bg-surface-2 px-2 text-sm text-fg outline-none focus-visible:border-brand"
              >
                <option value="">（未选择）</option>
                {/* 默认指向一个已禁用/不存在/不可规划的供应商时，额外渲染一个禁用 option
                    保留该陈旧选择的可见性，避免下拉静默空白。 */}
                {policy.defaultProviderId &&
                !selectableProviders.some((p) => p.id === policy.defaultProviderId) ? (
                  <option value={policy.defaultProviderId} disabled>
                    {(providers.find((p) => p.id === policy.defaultProviderId)?.name ??
                      policy.defaultProviderId) + "（已停用/不存在/不可用于规划）"}
                  </option>
                ) : null}
                {selectableProviders.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
              {selectableProviders.length === 0 ? (
                <span className="text-[12px] text-fg-subtle">暂无可用于规划的已启用供应商，请先添加并启用 OpenAI 兼容供应商。</span>
              ) : null}
            </div>
          )}
        </div>

        </>
        )}

        {/* 外观 */}
        {nav === "appearance" && <AppearanceSection cardCls={cardCls} />}

        {/* 通用偏好 */}
        {nav === "general" && (
        <>
        <h2 className="mb-3 flex items-center gap-1.5 text-sm font-semibold">
          <SlidersHorizontal size={15} strokeWidth={1.75} className="text-fg-muted" />
          通用
        </h2>
        <div className={cardCls}>
          <label className="flex items-center gap-2 text-[13px] text-fg">
            <input
              type="checkbox"
              checked={readonlyDefault}
              onChange={(e) => toggleReadonlyDefault(e.target.checked)}
            />
            默认只读优先
          </label>
          <p className="mt-1 text-[12px] text-fg-subtle">
            开启后主界面默认进入只读模式，仅允许检查类命令；执行写操作前需手动关闭。建议保持开启。
          </p>
        </div>
        </>
        )}

        {/* 在线更新 */}
        {nav === "update" && (
        <>
        <h2 className="mb-3 flex items-center gap-1.5 text-sm font-semibold">
          <DownloadCloud size={15} strokeWidth={1.75} className="text-fg-muted" />
          在线更新
        </h2>
        <div className={cardCls}>
          {/* 当前版本 + 检查按钮 */}
          <div className="flex items-center gap-2">
            <span className="text-[13px] text-fg">当前版本</span>
            <span className="font-mono text-[12.5px] text-fg-muted">v{version || "…"}</span>
            <Button
              variant="secondary"
              size="sm"
              className="ml-auto"
              onClick={() => void runCheckUpdate()}
              disabled={updPhase === "checking" || updPhase === "downloading"}
              title="向 GitHub Releases 检查是否有新版本"
            >
              {updPhase === "checking" ? <Spinner size="sm" /> : <RefreshCw size={13} />}{" "}
              {updPhase === "checking" ? "检查中…" : "检查更新"}
            </Button>
          </div>

          {/* 状态:已最新 */}
          {updPhase === "latest" && (
            <div className="mt-2.5 flex items-center gap-1.5 rounded bg-risk-low-soft px-2.5 py-1.5 text-[12px] text-risk-low">
              <Check size={13} /> 已是最新版本。
            </div>
          )}

          {/* 状态:出错(降级,不影响使用)*/}
          {updPhase === "error" && (
            <div className="mt-2.5 rounded bg-risk-blocked-soft px-2.5 py-1.5 text-[12px] text-risk-blocked">
              检查/更新失败：{updError}（不影响正常使用,可稍后重试）
            </div>
          )}

          {/* 状态:有新版 / 下载安装中 */}
          {(updPhase === "available" || updPhase === "downloading") && updInfo && (
            <div className="mt-2.5 rounded-md border border-border bg-surface-2 p-3">
              <div className="flex items-center gap-2">
                <Badge tone="success">新版本 v{updInfo.version}</Badge>
                <span className="text-[11px] text-fg-subtle">当前 v{updInfo.currentVersion}</span>
              </div>
              {updInfo.notes && (
                <pre className="mt-2 max-h-40 overflow-y-auto whitespace-pre-wrap text-[12px] leading-relaxed text-fg-muted">
                  {updInfo.notes}
                </pre>
              )}
              {updPhase === "downloading" ? (
                <div className="mt-2.5">
                  <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-1">
                    <div
                      className="h-full rounded-full bg-brand transition-[width]"
                      style={{ width: updPct === null ? "100%" : `${updPct}%` }}
                    />
                  </div>
                  <div className="mt-1 text-[11px] text-fg-subtle">
                    {updPct === null ? "正在下载…" : `下载中 ${updPct}%`}（完成后将自动重启）
                  </div>
                </div>
              ) : (
                <Button variant="primary" size="sm" className="mt-2.5" onClick={() => void runInstallUpdate()}>
                  <DownloadCloud size={13} /> 下载并安装
                </Button>
              )}
            </div>
          )}

          {/* 启动检查开关 */}
          <label className="mt-3 flex items-center gap-2 text-[13px] text-fg">
            <input type="checkbox" checked={autoCheck} onChange={(e) => toggleAutoCheck(e.target.checked)} />
            启动时自动检查更新
          </label>
          <p className="mt-1 text-[12px] text-fg-subtle">
            通过 GitHub Releases 分发,更新包经签名校验后才会安装。关闭后仅在此处手动检查。
          </p>
        </div>
        </>
        )}
      </div>
      </div>

      {/* 瞬时反馈浮层 */}
      <ToastViewport toasts={toasts} onDismiss={dismiss} />
    </section>
  );
}
