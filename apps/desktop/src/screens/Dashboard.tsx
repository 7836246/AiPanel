import type { JSX } from "react";
import { Button, IconButton, Spinner } from "@aipanel/ui";
import type { ServerProfile, ServerStatus } from "../lib/api";

// 状态点颜色：在线=安全绿、离线=阻断红、未知=次要灰（与 ServerOverview 保持一致）。
const statusDot = (s: ServerStatus): string =>
  s === "online" ? "bg-risk-low" : s === "offline" ? "bg-risk-blocked" : "bg-fg-subtle";

// 关注的关键指标键名（按此顺序在卡内展示），仅取 facts 中存在的项。
const METRIC_KEYS = ["Load", "Memory", "Disk", "Services", "Containers", "Ports"] as const;

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

// 紧凑进度条：按百分比着色，深色安全风格。
function MetricBar({ pct }: { pct: number }): JSX.Element {
  const risk = percentRisk(pct);
  return (
    <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-surface-2">
      <div
        className={`h-full rounded-full transition-[width] ${risk.bar}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

// 实心收藏星（已收藏）。
const StarFilled = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden>
    <path d="M8 1.5l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.3 4.2 13.3l.7-4.3-3.1-3 4.3-.6z" />
  </svg>
);

// 空心收藏星（未收藏）。
const StarOutline = ({ size = 14 }: { size?: number }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    strokeWidth={1.3}
    strokeLinejoin="round"
    aria-hidden
  >
    <path d="M8 1.8l1.85 3.75 4.15.6-3 2.92.71 4.13L8 11.27 4.29 13.2 5 9.07l-3-2.92 4.15-.6z" />
  </svg>
);

// 刷新图标（与「刷新全部」按钮配合）。
const Refresh = ({ size = 14 }: { size?: number }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    strokeWidth={1.4}
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden
  >
    <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
    <path d="M13.5 2v3h-3" />
  </svg>
);

/** 单台服务器卡片：状态点 + 名称 + 连接串 + 收藏星 + 关键指标 + 更新时间。 */
function ServerTile({
  server,
  selected,
  onSelect,
  onToggleFavorite,
}: {
  server: ServerProfile;
  selected: boolean;
  onSelect: (id: string) => void;
  onToggleFavorite: (id: string, favorite: boolean) => void;
}): JSX.Element {
  const facts = server.facts ?? {};
  // 仅取关注键中存在的指标，保持稳定顺序。
  const metrics = METRIC_KEYS.filter((k) => facts[k] != null && facts[k] !== "");

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onSelect(server.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(server.id);
        }
      }}
      className={`flex cursor-pointer flex-col gap-3 rounded-md border px-4 py-3.5 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand ${
        selected
          ? "border-border-strong bg-selected"
          : "border-border bg-surface-1 hover:bg-hover"
      }`}
    >
      {/* 头部：状态点 + 名称 + 收藏星 */}
      <div className="flex items-start gap-2">
        <span className={`mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full ${statusDot(server.status)}`} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold">{server.name}</div>
          <div className="truncate font-mono text-[11px] text-fg-subtle">
            {server.username}@{server.host}:{server.port}
          </div>
        </div>
        <IconButton
          aria-label={server.favorite ? "取消收藏" : "收藏"}
          size="sm"
          className={server.favorite ? "text-risk-medium hover:text-risk-medium" : ""}
          onClick={(e) => {
            // 阻止冒泡，避免点星触发卡片主体的 onSelect。
            e.stopPropagation();
            onToggleFavorite(server.id, !server.favorite);
          }}
        >
          {server.favorite ? <StarFilled /> : <StarOutline />}
        </IconButton>
      </div>

      {/* 关键指标：含百分比者（Disk/Memory）额外渲染进度条 */}
      {metrics.length > 0 ? (
        <div className="grid grid-cols-2 gap-x-3 gap-y-2">
          {metrics.map((key) => {
            const value = facts[key];
            const pct = parsePercent(value);
            const risk = pct !== null ? percentRisk(pct) : null;
            return (
              <div key={key} className="min-w-0">
                <div className="truncate text-[10px] uppercase tracking-wide text-fg-subtle">
                  {key}
                </div>
                <div
                  className={`truncate font-mono text-[13px] font-semibold ${
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
        <div className="rounded border border-dashed border-border px-3 py-2.5 text-center text-[12px] text-fg-subtle">
          尚无体检数据
        </div>
      )}

      {/* 末行：最近更新时间 */}
      <div className="text-[10px] text-fg-subtle">
        更新于 {new Date(server.updatedAt).toLocaleString()}
      </div>
    </div>
  );
}

/**
 * 多服务器概览（Dashboard）。
 *
 * 以卡片网格呈现所有服务器的在线状态与关键体检指标；点击卡片主体选中并进控制台，
 * 点收藏星切换收藏，顶部「刷新全部」触发并发连通刷新。组件本身不拉数据，全部走 props。
 */
export function Dashboard({
  servers,
  selectedServerId,
  onSelect,
  onToggleFavorite,
  onRefreshAll,
  refreshing,
}: {
  servers: ServerProfile[];
  selectedServerId: string | null;
  onSelect: (id: string) => void; // 点卡片主体 → 选中并进控制台
  onToggleFavorite: (id: string, favorite: boolean) => void;
  onRefreshAll: () => void;
  refreshing: boolean;
}): JSX.Element {
  const online = servers.filter((s) => s.status === "online").length;
  const offline = servers.filter((s) => s.status === "offline").length;
  const total = servers.length;

  return (
    <div className="flex flex-col gap-4">
      {/* 顶部：标题 + 小统计 + 刷新全部 */}
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-base font-semibold">服务器概览</h2>
        <div className="flex items-center gap-3 text-[12px] text-fg-muted">
          <span className="inline-flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-risk-low" />
            在线 {online}
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-risk-blocked" />
            离线 {offline}
          </span>
          <span className="text-fg-subtle">总数 {total}</span>
        </div>
        <Button
          variant="secondary"
          size="sm"
          className="ml-auto"
          onClick={onRefreshAll}
          disabled={refreshing}
        >
          {refreshing ? <Spinner size="sm" /> : <Refresh />} 刷新全部
        </Button>
      </div>

      {/* 卡片网格 / 空态 */}
      {total > 0 ? (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {servers.map((server) => (
            <ServerTile
              key={server.id}
              server={server}
              selected={server.id === selectedServerId}
              onSelect={onSelect}
              onToggleFavorite={onToggleFavorite}
            />
          ))}
        </div>
      ) : (
        <div className="rounded-md border border-dashed border-border px-4 py-10 text-center text-[13px] text-fg-subtle">
          还没有服务器，先在左侧添加一台以开始概览。
        </div>
      )}
    </div>
  );
}

export default Dashboard;
