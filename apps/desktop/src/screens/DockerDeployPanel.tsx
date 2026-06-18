import { useState, type JSX } from "react";
import { Boxes, Download, FileSearch, Rocket } from "lucide-react";
import { Button, Input, Spinner } from "@aipanel/ui";
import {
  dockerDetectPlan,
  dockerInstallPlan,
  dockerDeployPlan,
  type AppTemplate,
  type ReverseProxy,
  type Plan,
} from "../lib/api";

const errMsg = (e: unknown): string =>
  e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);

// 可部署的应用模板(与后端 AppTemplate 对齐)。
const TEMPLATES: { id: AppTemplate; name: string; desc: string; web: boolean }[] = [
  { id: "uptimeKuma", name: "Uptime Kuma", desc: "服务状态监控 · 3001", web: true },
  { id: "n8n", name: "n8n", desc: "工作流自动化 · 5678", web: true },
  { id: "wordPress", name: "WordPress", desc: "建站(含 MySQL)· 8080", web: true },
  { id: "postgres", name: "PostgreSQL", desc: "关系型数据库 · 127.0.0.1:5432", web: false },
  { id: "redis", name: "Redis", desc: "缓存 / 键值库 · 127.0.0.1:6379", web: false },
];

const PROXIES: { id: ReverseProxy; name: string }[] = [
  { id: "none", name: "不加反代" },
  { id: "caddy", name: "Caddy(自动 HTTPS)" },
  { id: "nginx", name: "Nginx(+certbot)" },
];

/**
 * Docker 部署入口:选模板/反代/域名,生成结构化 Plan 后交给上层走
 * 现有「可编辑 → 风险审查 → 确认 → 执行」流程(本面板不执行任何东西)。
 */
export default function DockerDeployPanel({
  serverId,
  onPlan,
}: {
  serverId: string;
  onPlan: (plan: Plan, title: string) => void | Promise<void>;
}): JSX.Element {
  const [app, setApp] = useState<AppTemplate>("uptimeKuma");
  const [proxy, setProxy] = useState<ReverseProxy>("none");
  const [domain, setDomain] = useState("");
  const [busy, setBusy] = useState<"detect" | "install" | "deploy" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const current = TEMPLATES.find((t) => t.id === app);
  const selectApp = (nextApp: AppTemplate) => {
    setApp(nextApp);
    const next = TEMPLATES.find((t) => t.id === nextApp);
    if (!next?.web) {
      setProxy("none");
      setDomain("");
    }
  };

  const run = async (kind: "detect" | "install" | "deploy") => {
    // 反代选了 Caddy/Nginx 却没填域名:无法签发 HTTPS / 配置 vhost,提前拦下避免生成不可用计划。
    if (kind === "deploy" && current?.web && proxy !== "none" && !domain.trim()) {
      setError("已选择反向代理,请先填写域名(如 app.example.com)再生成部署计划。");
      return;
    }
    setBusy(kind);
    setError(null);
    try {
      let plan: Plan;
      let title: string;
      if (kind === "detect") {
        plan = await dockerDetectPlan(serverId);
        title = "检测 Docker 环境";
      } else if (kind === "install") {
        plan = await dockerInstallPlan(serverId);
        title = "安装 Docker";
      } else {
        plan = await dockerDeployPlan(serverId, app, domain.trim() || undefined, proxy);
        title = `部署 ${current?.name ?? app}`;
      }
      await onPlan(plan, title);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex max-w-[760px] flex-col gap-5 px-6 pb-10 pt-5">
        <header className="border-b border-border pb-3">
          <h2 className="flex items-center gap-2 text-[15px] font-semibold">
            <Boxes size={18} className="text-fg-muted" /> Docker 应用部署
          </h2>
          <p className="mt-1 text-[12.5px] text-fg-muted">
            选择应用与反代,生成结构化计划;计划会经风险审查与确认后再执行,生成的密码现场随机产生并写入服务器 .env。
          </p>
        </header>

        {/* Docker 环境:检测 / 安装 */}
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" size="sm" onClick={() => void run("detect")} disabled={busy !== null}>
            {busy === "detect" ? <Spinner size="sm" /> : <FileSearch size={14} />} 检测 Docker
          </Button>
          <Button variant="secondary" size="sm" onClick={() => void run("install")} disabled={busy !== null}>
            {busy === "install" ? <Spinner size="sm" /> : <Download size={14} />} 安装 Docker
          </Button>
          <span className="text-[12px] text-fg-subtle">未装 Docker 时先检测/安装(安装为写操作,需确认)。</span>
        </div>

        {/* 模板选择 */}
        <div>
          <div className="mb-2 text-[12px] uppercase tracking-wide text-fg-subtle">应用模板</div>
          <div className="grid grid-cols-2 gap-2.5 md:grid-cols-3">
            {TEMPLATES.map((t) => (
              <button
                key={t.id}
                onClick={() => selectApp(t.id)}
                className={`rounded-md border px-3.5 py-3 text-left transition-colors ${
                  app === t.id
                    ? "border-brand bg-selected"
                    : "border-border bg-surface-1 hover:bg-hover hover:border-border-strong"
                }`}
              >
                <div className="text-[13.5px] font-semibold">{t.name}</div>
                <div className="mt-0.5 text-[11.5px] text-fg-subtle">{t.desc}</div>
              </button>
            ))}
          </div>
        </div>

        {/* 反代 + 域名(仅 Web 应用有意义) */}
        <div className="grid gap-3 md:grid-cols-2">
          <div>
            <div className="mb-1.5 text-[12px] uppercase tracking-wide text-fg-subtle">反向代理</div>
            <div className="flex flex-col gap-1.5">
              {PROXIES.map((p) => (
                <label key={p.id} className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-1.5 text-[13px] ${proxy === p.id ? "border-brand bg-selected" : "border-border hover:bg-hover"} ${!current?.web ? "opacity-50" : ""}`}>
                  <input type="radio" name="proxy" checked={proxy === p.id} onChange={() => setProxy(p.id)} disabled={!current?.web} />
                  {p.name}
                </label>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1.5 text-[12px] uppercase tracking-wide text-fg-subtle">域名(可选)</div>
            <Input
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
              placeholder="例如 app.example.com"
              disabled={!current?.web || proxy === "none"}
            />
            <p className="mt-1.5 text-[11.5px] text-fg-subtle">
              {/* 说明文案随 proxy 联动,与域名框禁用条件保持一致:proxy="none"(含默认态)时先提示选反代,避免让用户去填一个灰掉的框。 */}
              {current?.web
                ? proxy === "none"
                  ? "选择 Caddy/Nginx 后可填写域名以启用 HTTPS。"
                  : "填域名 + Caddy 即自动签发 HTTPS;Nginx 会追加 certbot 步骤。"
                : "数据库类应用不暴露 HTTP,反代/域名不适用。"}
            </p>
          </div>
        </div>

        {error && (
          <div className="rounded-md border border-risk-blocked/40 bg-risk-blocked-soft px-3 py-2 text-[12.5px] text-risk-blocked">
            {error}
          </div>
        )}

        <div className="flex items-center gap-2 border-t border-border pt-4">
          <Button onClick={() => void run("deploy")} disabled={busy !== null}>
            {busy === "deploy" ? <Spinner size="sm" /> : <Rocket size={14} />} 生成部署计划
          </Button>
          <span className="text-[12px] text-fg-subtle">生成后进入控制台,可逐步编辑、审查并确认执行。</span>
        </div>
      </div>
    </section>
  );
}
