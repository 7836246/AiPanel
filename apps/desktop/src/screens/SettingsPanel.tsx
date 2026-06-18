import { useEffect, useState } from "react";
import { Badge, Button, Input, Spinner, ToastViewport, useToasts } from "@aipanel/ui";
import {
  Check,
  Cpu,
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
  credentialBackend,
  deleteProvider,
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
} from "../lib/api";

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

/**
 * 「默认只读优先」开关的 localStorage 键名。
 * 主界面（CodexConsole）初始化 readOnlyMode 时读取此键作为默认值：
 *   localStorage.getItem(READONLY_DEFAULT_KEY) !== "false"  →  默认只读
 * 即未设置或为 "true" 时默认开启只读，仅显式存为 "false" 时默认关闭。
 */
export const READONLY_DEFAULT_KEY = "aipanel-readonly-default";

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

// 设置面板：管理模型供应商（增删改、连接测试）、模型选择策略、凭据后端提示与通用偏好。
export default function SettingsPanel() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [policy, setPolicy] = useState<ModelSelectionPolicy>({ auto: true });
  const [backend, setBackend] = useState<string>("");
  const [form, setForm] = useState<ProviderInput | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [testResult, setTestResult] = useState<ProviderTestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [busy, setBusy] = useState(false);
  // 模型探测：从 OpenAI 兼容供应商拉取可用模型列表（仅本地 state，供下拉选择）。
  const [models, setModels] = useState<string[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [detectError, setDetectError] = useState<string | null>(null);
  // 通用区块：默认只读优先（持久化到 localStorage，供主界面读取）。
  const [readonlyDefault, setReadonlyDefault] = useState<boolean>(true);
  const { toasts, push, dismiss } = useToasts();

  // 已启用的供应商——默认供应商下拉只在其中选择。
  const enabledProviders = providers.filter((p) => p.enabled);

  // 重新拉取供应商列表、模型选择策略与凭据后端标识。
  async function refresh() {
    setProviders(await listProviders());
    setPolicy(await getModelSelectionPolicy());
    setBackend(await credentialBackend());
  }
  useEffect(() => {
    refresh().catch(() => push("danger", "加载设置失败"));
    setReadonlyDefault(readReadonlyDefault());
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
      // 编辑已保存供应商且未重新输入 Key 时,补回确定性的 credentialRef,让后端能从
      // Keychain 取回密钥探测(否则会因无 Key 而被供应商 401 拒绝)。
      const probe =
        apiKey || !form.id
          ? form
          : ({ ...form, credentialRef: `provider:${form.id}` } as ProviderConfig);
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

  // 保存表单（API Key 留空则不修改），成功后关闭表单并刷新列表。
  async function save() {
    if (!form || !form.name.trim()) return;
    setBusy(true);
    try {
      await saveProvider(form, apiKey || undefined);
      setForm(null);
      setApiKey("");
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
      if (policy.defaultProviderId === id) {
        const next = { ...policy, defaultProviderId: undefined };
        setPolicy(next);
        await saveModelSelectionPolicy(next);
      }
      await refresh();
      push("success", "供应商已删除");
    } catch {
      push("danger", "删除失败");
    }
  }

  // 更新模型选择策略并立即持久化。
  async function updatePolicy(next: ModelSelectionPolicy) {
    setPolicy(next);
    try {
      await saveModelSelectionPolicy(next);
    } catch {
      push("danger", "保存模型选择策略失败");
    }
  }

  // 运行连接测试：测试期间显示 Spinner，结果以颜色区分成功/失败。
  async function runTest() {
    if (!form) return;
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult(await testProvider(form, apiKey || undefined));
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

  const field = "mb-2 flex flex-col gap-1";
  const labelCls = "text-[12px] font-medium text-fg-muted";
  const cardCls = "rounded-md border border-border bg-surface-1 p-4";

  return (
    <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[680px] px-6 pb-8 pt-5">
        <h2 className="mb-3 flex items-center gap-1.5 text-sm font-semibold">
          <Server size={15} strokeWidth={1.75} className="text-fg-muted" />
          设置 · 模型供应商
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
                <Button variant="ghost" size="sm" className="gap-1.5" onClick={() => remove(p.id)}>
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
              <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-…" />
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
                  disabled={detecting}
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
                  <option value="">{models.length === 0 ? "（请先探测模型）" : "（选择模型）"}</option>
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
              {/* 手填兜底：探测不可用时仍可直接输入模型名 */}
              <Input
                className="mt-1"
                value={form.model ?? ""}
                onChange={(e) => setForm({ ...form, model: e.target.value || undefined })}
                placeholder="或手填模型名，如 gpt-4o-mini"
              />
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
              <Button variant="secondary" size="sm" onClick={runTest} disabled={testing}>
                {testing ? "测试中…" : "测试连接"}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setForm(null);
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
          <Button variant="secondary" size="sm" className="mt-3 gap-1.5" onClick={() => { setForm({ ...EMPTY }); setApiKey(""); setTestResult(null); setModels([]); setDetecting(false); setDetectError(null); }}>
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
                {/* 默认指向一个已禁用/不存在的供应商时，额外渲染一个禁用 option
                    保留该陈旧选择的可见性，避免下拉静默空白。 */}
                {policy.defaultProviderId &&
                !enabledProviders.some((p) => p.id === policy.defaultProviderId) ? (
                  <option value={policy.defaultProviderId} disabled>
                    {(providers.find((p) => p.id === policy.defaultProviderId)?.name ??
                      policy.defaultProviderId) + "（已停用/不存在）"}
                  </option>
                ) : null}
                {enabledProviders.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
              {enabledProviders.length === 0 ? (
                <span className="text-[12px] text-fg-subtle">暂无已启用的供应商，请先在上方添加并启用。</span>
              ) : null}
            </div>
          )}
        </div>

        {/* 通用偏好 */}
        <h2 className="mb-3 mt-8 flex items-center gap-1.5 text-sm font-semibold">
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
      </div>

      {/* 瞬时反馈浮层 */}
      <ToastViewport toasts={toasts} onDismiss={dismiss} />
    </section>
  );
}
