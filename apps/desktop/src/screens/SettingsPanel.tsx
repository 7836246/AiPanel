import { useEffect, useState } from "react";
import { Badge, Button, Input, Spinner, ToastViewport, useToasts } from "@aipanel/ui";
import {
  credentialBackend,
  deleteProvider,
  getModelSelectionPolicy,
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

// 供应商类型到中文标签的映射。
const KIND_LABEL: Record<ProviderKind, string> = {
  codex_app_server: "Codex app-server",
  openai_compatible: "OpenAI 兼容",
  custom: "自定义",
};

// 新增供应商表单的初始空值。
const EMPTY: ProviderInput = { name: "", kind: "codex_app_server", enabled: true };

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
      await refresh();
      push("success", "供应商已保存");
    } catch {
      push("danger", "保存失败，请检查配置");
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
    } catch {
      setTestResult({ ok: false, message: "测试请求失败", detail: "无法连接到后端" });
    } finally {
      setTesting(false);
    }
  }

  // 切换「默认只读优先」并持久化到 localStorage。
  function toggleReadonlyDefault(value: boolean) {
    setReadonlyDefault(value);
    writeReadonlyDefault(value);
    push("info", value ? "已设为默认只读优先" : "已关闭默认只读优先");
  }

  const field = "mb-2 flex flex-col gap-1";
  const labelCls = "text-[12px] font-medium text-fg-muted";
  const cardCls = "rounded-md border border-border bg-surface-1 p-4";

  return (
    <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[680px] px-6 pb-8 pt-5">
        <h2 className="mb-3 text-sm font-semibold">设置 · 模型供应商</h2>

        {/* 凭据后端提示：mock 给出警告样式，keychain 给出安全提示 */}
        {backend === "mock" ? (
          <div className="mb-3 rounded-md border border-risk-medium/40 bg-risk-medium-soft px-3 py-2 text-[12.5px] text-risk-medium">
            <span className="font-medium">凭据当前存放在内存（开发兜底）。</span>
            重启后将丢失，且仅用于开发调试。生产环境会自动使用系统 Keychain。
          </div>
        ) : backend ? (
          <div className="mb-3 flex items-center gap-2 rounded-md border border-border bg-surface-2 px-3 py-2 text-[12.5px] text-fg-muted">
            <Badge tone="success">Keychain</Badge>
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
                <Button variant="ghost" size="sm" onClick={() => edit(p)}>
                  编辑
                </Button>
                <Button variant="ghost" size="sm" onClick={() => remove(p.id)}>
                  删除
                </Button>
              </div>
            ))
          )}
        </div>

        {/* 新增 / 编辑表单 */}
        {form ? (
          <div className="mt-3 rounded-md border border-border bg-surface-1 p-4">
            <div className="mb-3 text-[13px] font-semibold">{form.id ? "编辑供应商" : "新增供应商"}</div>
            <div className={field}>
              <label className={labelCls}>名称</label>
              <Input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="例如 Codex" />
            </div>
            <div className={field}>
              <label className={labelCls}>类型</label>
              <select
                value={form.kind}
                onChange={(e) => setForm({ ...form, kind: e.target.value as ProviderKind })}
                className="h-9 rounded-md border border-border bg-surface-2 px-2 text-sm text-fg outline-none focus-visible:border-brand"
              >
                <option value="codex_app_server">Codex app-server</option>
                <option value="openai_compatible">OpenAI 兼容</option>
                <option value="custom">自定义</option>
              </select>
            </div>
            {form.kind === "codex_app_server" ? (
              <div className={field}>
                <label className={labelCls}>codex 可执行文件路径</label>
                <Input
                  value={form.codexPath ?? ""}
                  onChange={(e) => setForm({ ...form, codexPath: e.target.value })}
                  placeholder="codex"
                />
              </div>
            ) : (
              <>
                <div className={field}>
                  <label className={labelCls}>Base URL</label>
                  <Input
                    value={form.baseUrl ?? ""}
                    onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
                    placeholder="https://api.example.com/v1"
                  />
                </div>
                <div className={field}>
                  <label className={labelCls}>API Key（仅存本地 Keychain，不进数据库）</label>
                  <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-…" />
                </div>
              </>
            )}
            <div className={field}>
              <label className={labelCls}>模型</label>
              <Input value={form.model ?? ""} onChange={(e) => setForm({ ...form, model: e.target.value })} placeholder="gpt-5-codex" />
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
                <span className="font-medium">{testResult.ok ? "✓ 连接成功" : "✗ 连接失败"}</span>
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
                }}
              >
                取消
              </Button>
            </div>
          </div>
        ) : (
          <Button variant="secondary" size="sm" className="mt-3" onClick={() => { setForm({ ...EMPTY }); setApiKey(""); setTestResult(null); }}>
            添加供应商
          </Button>
        )}

        {/* 模型选择策略 */}
        <h2 className="mb-3 mt-8 text-sm font-semibold">模型选择</h2>
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
        <h2 className="mb-3 mt-8 text-sm font-semibold">通用</h2>
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
