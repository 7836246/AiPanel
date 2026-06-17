import type { JSX } from "react";
import { Button, Spinner } from "@aipanel/ui";
import type { ServerProfile, ServerStatus } from "../lib/api";

// 状态点颜色：在线=安全绿、离线=阻断红、未知=次要灰。
const statusDot = (s: ServerStatus): string =>
  s === "online" ? "bg-risk-low" : s === "offline" ? "bg-risk-blocked" : "bg-fg-subtle";

// 听诊器图标（只读体检入口），与 CodexConsole 中风格保持一致。
const Stethoscope = ({ size = 14 }: { size?: number }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    strokeWidth={1.4}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M4 2v4a3 3 0 0 0 6 0V2" />
    <path d="M7 9v1.5a3.5 3.5 0 0 0 7 0V9" />
    <circle cx="13.5" cy="8" r="1.2" />
  </svg>
);

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
}: {
  server: ServerProfile | null;
  running: boolean;
  onDoctor: () => void;
}): JSX.Element {
  if (!server) {
    return (
      <div className="rounded-md border border-border bg-surface-1 px-4 py-8 text-center text-[13px] text-fg-subtle">
        从左侧选择一台服务器开始。
      </div>
    );
  }

  const facts = Object.entries(server.facts ?? {});

  return (
    <div className="flex flex-col gap-3">
      {/* 头部：标识 + 只读体检入口 */}
      <div className="rounded-md border border-border bg-surface-1 px-4 py-3.5">
        <div className="flex items-center gap-2">
          <span className={`h-1.5 w-1.5 rounded-full ${statusDot(server.status)}`} />
          <span className="text-sm font-semibold">{server.name}</span>
          <span className="font-mono text-[12px] text-fg-subtle">
            {server.username}@{server.host}:{server.port}
          </span>
          <Button
            variant="secondary"
            size="sm"
            className="ml-auto"
            onClick={onDoctor}
            disabled={running}
          >
            {running ? <Spinner size="sm" /> : <Stethoscope />} 只读体检
          </Button>
        </div>
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
        <div className="rounded-md border border-dashed border-border px-4 py-6 text-center text-[13px] text-fg-subtle">
          暂无体检指标，点上方「只读体检」做一次安全检查以采集结构化指标。
        </div>
      )}
    </div>
  );
}

export default ServerOverview;
