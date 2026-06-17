import { useEffect, useState } from "react";
import { Badge, Button, Input } from "@aipanel/ui";
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

const KIND_LABEL: Record<ProviderKind, string> = {
  codex_app_server: "Codex app-server",
  openai_compatible: "OpenAI 兼容",
  custom: "自定义",
};

const EMPTY: ProviderInput = { name: "", kind: "codex_app_server", enabled: true };

export default function SettingsPanel() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [policy, setPolicy] = useState<ModelSelectionPolicy>({ auto: true });
  const [backend, setBackend] = useState<string>("");
  const [form, setForm] = useState<ProviderInput | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [testResult, setTestResult] = useState<ProviderTestResult | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setProviders(await listProviders());
    setPolicy(await getModelSelectionPolicy());
    setBackend(await credentialBackend());
  }
  useEffect(() => {
    refresh().catch(() => {});
  }, []);

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

  async function save() {
    if (!form || !form.name.trim()) return;
    setBusy(true);
    try {
      await saveProvider(form, apiKey || undefined);
      setForm(null);
      setApiKey("");
      setTestResult(null);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: string) {
    await deleteProvider(id);
    if (policy.defaultProviderId === id) {
      const next = { ...policy, defaultProviderId: undefined };
      setPolicy(next);
      await saveModelSelectionPolicy(next);
    }
    await refresh();
  }

  async function updatePolicy(next: ModelSelectionPolicy) {
    setPolicy(next);
    await saveModelSelectionPolicy(next);
  }

  const field = "mb-2 flex flex-col gap-1";
  const labelCls = "text-[12px] font-medium text-fg-muted";

  return (
    <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[680px] px-6 pb-8 pt-5">
        <h2 className="mb-3 text-sm font-semibold">设置 · 模型供应商</h2>

        {backend === "mock" && (
          <div className="mb-3 rounded-md border border-risk-medium/40 bg-risk-medium-soft px-3 py-2 text-[12.5px] text-risk-medium">
            凭据当前存放在内存（开发模式），重启后丢失。生产环境会使用系统 Keychain。
          </div>
        )}

        {/* provider list */}
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

        {/* add / edit form */}
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
            {testResult ? (
              <div className={`mb-3 text-[12.5px] ${testResult.ok ? "text-risk-low" : "text-risk-blocked"}`}>
                {testResult.ok ? "✓ " : "✗ "}
                {testResult.message}
                {testResult.detail ? ` (${testResult.detail})` : ""}
              </div>
            ) : null}
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" onClick={save} disabled={busy || !form.name.trim()}>
                保存
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={async () => setTestResult(await testProvider(form))}
              >
                测试连接
              </Button>
              <Button variant="ghost" size="sm" onClick={() => { setForm(null); setTestResult(null); }}>
                取消
              </Button>
            </div>
          </div>
        ) : (
          <Button variant="secondary" size="sm" className="mt-3" onClick={() => { setForm({ ...EMPTY }); setApiKey(""); }}>
            添加供应商
          </Button>
        )}

        {/* model selection policy */}
        <h2 className="mb-3 mt-8 text-sm font-semibold">模型选择</h2>
        <div className="rounded-md border border-border bg-surface-1 p-4">
          <label className="flex items-center gap-2 text-[13px] text-fg">
            <input
              type="checkbox"
              checked={policy.auto}
              onChange={(e) => updatePolicy({ ...policy, auto: e.target.checked })}
            />
            自动按任务选择模型
          </label>
          {!policy.auto && (
            <div className="mt-3 flex flex-col gap-1">
              <label className={labelCls}>默认供应商</label>
              <select
                value={policy.defaultProviderId ?? ""}
                onChange={(e) => updatePolicy({ ...policy, defaultProviderId: e.target.value || undefined })}
                className="h-9 rounded-md border border-border bg-surface-2 px-2 text-sm text-fg outline-none focus-visible:border-brand"
              >
                <option value="">（未选择）</option>
                {providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
