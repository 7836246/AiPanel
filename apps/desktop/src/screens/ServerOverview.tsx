import { useEffect, useState, type JSX } from "react";
import { CheckCircle2, PlugZap, Server, Stethoscope, XCircle } from "lucide-react";
import { Button, Spinner } from "@aipanel/ui";
import { checkSshConnection, type ServerProfile, type ServerStatus } from "../lib/api";

// 连接探测的本地 UI 阶段：idle 无内联提示、checking 显示「连接中…」、
// online/offline 显示结果徽标（绿/红），仅用于即时视觉反馈，不影响 server.status 来源。
type ConnPhase = "idle" | "checking" | "online" | "offline";

// 状态点颜色：在线=安全绿、离线=阻断红、未知=次要灰。
const statusDot = (s: ServerStatus): string =>
  s === "online" ? "bg-risk-low" : s === "offline" ? "bg-risk-blocked" : "bg-fg-subtle";

// 从形如 "73%"、"使用率 85 %"、"42.5%" 的字符串中解析出百分比数值；
// 解析不到合法百分比时返回 null（该项不渲染进度条）。
function parsePercent(value: string): number | null {
  const m = value.match(/(\d+(?:\.\d+)?)\s*%/);
  if (!m) return null;
  const n = Number(m[1]);
  if (!Number.isFinite(n)) return null;
  // 钳制到 0–100，避免异常输出撑坏进度条。
  return Math.min(100, Math.max(0, n));
}

// 按阈值映射风险颜色 token：<70% 低风险、70–90% 中风险、>90% 阻断。
function percentRisk(pct: number): { bar: string; text: string } {
  if (pct > 90) return { bar: "bg-risk-blocked", text: "text-risk-blocked" };
  if (pct >= 70) return { bar: "bg-risk-medium", text: "text-risk-medium" };
  return { bar: "bg-risk-low", text: "text-risk-low" };
}

// 单条指标的进度条：按百分比着色，深色安全风格。
function MetricBar({ pct }: { pct: number }): JSX.Element {
  const risk = percentRisk(pct);
  return (
    <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-surface-2">
      <div
        className={`h-full rounded-full transition-[width] ${risk.bar}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

/**
 * 服务器概览：展示已选服务器的结构化体检指标。
 *
 * - server 为 null：渲染「从左侧选择服务器」占位。
 * - 否则：头部显示名称 / 状态点 / user@host:port / 「只读体检」按钮
 *   （running 时 Spinner + 禁用）；下方把 server.facts 渲染成指标卡网格，
 *   对含百分比的 facts（如 Disk / Memory）额外渲染按风险着色的进度条。
 * - facts 为空：提示先做一次体检。
 */
export function ServerOverview({
  server,
  running,
  onDoctor,
  onStatus,
}: {
  server: ServerProfile | null;
  running: boolean;
  onDoctor: () => void;
  // 可选回调：手动连接/重连得到结果后回传在线与否，供上层（CodexConsole）更新 servers 状态。
  onStatus?: (online: boolean) => void;
}): JSX.Element {
  // 手动连接探测的本地阶段与内联文案；切换服务器时由 key/重渲染重置（见 hooks 写法）。
  const [conn, setConn] = useState<ConnPhase>("idle");
  const [connMsg, setConnMsg] = useState<string | null>(null);

  // 切换服务器时复位本地连接探测状态，避免上一台的提示串到新选中的服务器上。
  const serverId = server?.id ?? null;
  useEffect(() => {
    setConn("idle");
    setConnMsg(null);
  }, [serverId]);

  // 点击「连接/重连」：调用 checkSshConnection，期间 checking，结果落到 online/offline。
  const handleConnect = async (id: string): Promise<void> => {
    setConn("checking");
    setConnMsg(null);
    try {
      const ok = await checkSshConnection(id);
      setConn(ok ? "online" : "offline");
      setConnMsg(ok ? "连接成功" : "连接失败：无法建立 SSH 连接");
      onStatus?.(ok);
    } catch (err) {
      // 调用本身抛错（如后端异常）：按离线处理并展示原因。
      setConn("offline");
      setConnMsg(`连接失败：${String(err)}`);
      onStatus?.(false);
    }
  };

  if (!server) {
    // 空态：图标 + 提示，垂直居中匀称留白。
    return (
      <div className="flex flex-col items-center justify-center gap-2 rounded-md border border-border bg-surface-1 px-4 py-10 text-center">
        <Server size={24} strokeWidth={1.75} className="text-fg-subtle" />
        <div className="text-[13px] text-fg-muted">从左侧选择一台服务器开始。</div>
      </div>
    );
  }

  const facts = Object.entries(server.facts ?? {});

  return (
    <div className="flex flex-col gap-3">
      {/* 头部：标识 + 连接/重连 + 只读体检入口 */}
      <div className="rounded-md border border-border bg-surface-1 px-4 py-3.5">
        <div className="flex items-center gap-2">
          {/* 状态点：连接探测有结果时优先反映本地 online/offline，否则回退 server.status */}
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              conn === "online"
                ? "bg-risk-low"
                : conn === "offline"
                  ? "bg-risk-blocked"
                  : statusDot(server.status)
            }`}
          />
          <span className="text-sm font-semibold">{server.name}</span>
          <span
            className="min-w-0 truncate font-mono text-[12px] text-fg-subtle"
            title={`${server.username}@${server.host}:${server.port}`}
          >
            {server.username}@{server.host}:{server.port}
          </span>
          {/* 连接/重连：与「只读体检」并排，二者各自独立的 loading 状态 */}
          <Button
            variant="secondary"
            size="sm"
            className="ml-auto"
            onClick={() => void handleConnect(server.id)}
            disabled={conn === "checking"}
          >
            {conn === "checking" ? <Spinner size="sm" /> : <PlugZap size={13} />}{" "}
            {conn === "checking" ? "连接中…" : "连接/重连"}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={onDoctor}
            disabled={running}
          >
            {running ? <Spinner size="sm" /> : <Stethoscope size={13} />} 只读体检
          </Button>
        </div>

        {/* 内联连接状态提示：toast 风格的细条，仅在非 idle 时渲染 */}
        {conn !== "idle" && (
          <div
            className={`mt-2.5 flex items-center gap-1.5 rounded px-2.5 py-1.5 text-[12px] ${
              conn === "checking"
                ? "bg-hover text-fg-muted"
                : conn === "online"
                  ? "bg-risk-low-soft text-risk-low"
                  : "bg-risk-blocked-soft text-risk-blocked"
            }`}
          >
            {conn === "checking" ? (
              <>
                <Spinner size="sm" />
                正在连接 {server.username}@{server.host}…
              </>
            ) : conn === "online" ? (
              <>
                <CheckCircle2 size={13} className="flex-none" />
                {connMsg ?? "已连接"}
              </>
            ) : (
              <>
                <XCircle size={13} className="flex-none" />
                {connMsg ?? "连接失败"}
              </>
            )}
          </div>
        )}
      </div>

      {/* 指标网格：每个 fact 一张卡，含百分比者额外渲染进度条 */}
      {facts.length > 0 ? (
        <div className="grid grid-cols-2 gap-2.5 md:grid-cols-3">
          {facts.map(([key, value]) => {
            const pct = parsePercent(value);
            const risk = pct !== null ? percentRisk(pct) : null;
            return (
              <div
                key={key}
                className="rounded-md border border-border bg-surface-1 px-3.5 py-3"
              >
                <div className="truncate text-[11px] uppercase tracking-wide text-fg-subtle">
                  {key}
                </div>
                <div
                  className={`mt-1 truncate font-mono text-[15px] font-semibold ${
                    risk ? risk.text : "text-fg"
                  }`}
                  title={value}
                >
                  {value}
                </div>
                {pct !== null && <MetricBar pct={pct} />}
              </div>
            );
          })}
        </div>
      ) : (
        // 空态：图标 + 提示，垂直居中匀称留白。
        <div className="flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-surface-1 px-4 py-8 text-center">
          <Stethoscope size={24} strokeWidth={1.75} className="text-fg-subtle" />
          <div className="text-[13px] text-fg-muted">暂无体检指标</div>
          <div className="text-[12px] text-fg-subtle">
            点上方「只读体检」做一次安全检查以采集结构化指标。
          </div>
        </div>
      )}
    </div>
  );
}

export default ServerOverview;
