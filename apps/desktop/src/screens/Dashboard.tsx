import type { JSX } from "react";
import { RefreshCw, Server, Star, Stethoscope } from "lucide-react";
import { Button, IconButton, Spinner } from "@aipanel/ui";
import type { ServerProfile, ServerStatus } from "../lib/api";
import { formatRelativeTime } from "../lib/time";

// 状态点颜色：在线=安全绿、离线=阻断红、未知=次要灰（与 ServerOverview 保持一致）。
const statusText = (s: ServerStatus): string =>
  s === "online" ? "在线" : s === "offline" ? "离线" : "未知";

// 状态小徽标配色(软底)。
const statusChip = (s: ServerStatus): string =>
  s === "online"
    ? "bg-risk-low-soft text-risk-low"
    : s === "offline"
      ? "bg-risk-blocked-soft text-risk-blocked"
      : "bg-hover text-fg-subtle";

// 关注的关键指标键名（按此顺序在卡内展示），仅取 facts 中存在的项。
const METRIC_KEYS = ["Load", "Memory", "Disk", "Services", "Containers", "Ports"] as const;

// 资源告警:上次体检的磁盘/内存使用率 >90% 视为紧张。
function hasResourceAlert(s: ServerProfile): boolean {
  return ["Disk", "Memory"].some((k) => {
    const v = s.facts?.[k];
    const p = v ? parsePercent(v) : null;
    return p !== null && p > 90;
  });
}

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

// 紧凑进度条：按百分比着色，深色安全风格；留白略增以提升可读性。
function MetricBar({ pct }: { pct: number }): JSX.Element {
  const risk = percentRisk(pct);
  return (
    <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-surface-2">
      <div
        className={`h-full rounded-full transition-[width] ${risk.bar}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

/** 单台服务器卡片：状态点 + 名称 + 连接串 + 收藏星 + 关键指标 + 更新时间。 */
function ServerTile({
  server,
  selected,
  refreshing,
  onSelect,
  onToggleFavorite,
}: {
  server: ServerProfile;
  selected: boolean;
  // 全局刷新进行中：徽标改为「连接中…」+ 小 Spinner，让刷新过程可见。
  refreshing: boolean;
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
      // 显式按钮名：避免读屏把卡内名称/连接串/各指标拼成长串，明确卡片用途为「打开控制台」
      aria-label={`打开 ${server.name} 控制台`}
      onClick={() => onSelect(server.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(server.id);
        }
      }}
      className={`flex cursor-pointer flex-col gap-2.5 rounded-md border px-3.5 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand ${
        selected
          ? "border-border-strong bg-selected"
          : "border-border bg-surface-1 hover:bg-hover hover:border-border-strong"
      }`}
    >
      {/* 头部：名称 + 状态徽标 + 收藏星 */}
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="min-w-0 truncate text-[13.5px] font-semibold">{server.name}</span>
            {refreshing ? (
              // 刷新进行中：徽标显示「连接中…」+ 小 Spinner，过程可见。
              <span className="flex-none inline-flex items-center gap-1 rounded bg-hover px-1.5 py-0.5 text-[10px] font-medium text-fg-muted">
                <Spinner size="sm" /> 连接中…
              </span>
            ) : (
              <span className={`flex-none rounded px-1.5 py-0.5 text-[10px] font-medium ${statusChip(server.status)}`}>
                {statusText(server.status)}
              </span>
            )}
            {/* 资源告警徽标:磁盘/内存紧张时提示 */}
            {!refreshing && hasResourceAlert(server) && (
              <span className="flex-none rounded bg-risk-blocked-soft px-1.5 py-0.5 text-[10px] font-medium text-risk-blocked">资源紧张</span>
            )}
          </div>
          <div
            className="mt-0.5 truncate font-mono text-[11px] text-fg-subtle"
            title={`${server.username}@${server.host}:${server.port}`}
          >
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
          {/* 已收藏用实心 Star，未收藏用描边 Star */}
          <Star size={13} fill={server.favorite ? "currentColor" : "none"} />
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
        <div className="flex items-center gap-1.5 text-[11.5px] text-fg-subtle">
          <Stethoscope size={13} className="flex-none" />
          尚无体检数据 · 点开卡片做一次只读体检
        </div>
      )}

      {/* 末行:仅在有体检数据时展示更新时间(避免对未体检卡片显示误导性时间) */}
      {metrics.length > 0 && (
        <div className="text-[10px] text-fg-subtle" title={new Date(server.updatedAt).toLocaleString()}>
          更新于 {formatRelativeTime(server.updatedAt)}
        </div>
      )}
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
    <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex max-w-[920px] flex-col gap-4 px-6 pb-8 pt-5">
        {/* 顶部：标题 + 小统计 + 刷新全部 */}
        <div className="flex flex-wrap items-center gap-3 border-b border-border pb-3">
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
          title="仅刷新在线/离线状态;指标(负载/内存/磁盘)请用「只读体检」更新"
        >
          {/* 刷新进行中：图标与文案同步切换为「刷新中…」，避免只转圈不变文案 */}
          {refreshing ? (
            <>
              <Spinner size="sm" /> 刷新中…
            </>
          ) : (
            <>
              <RefreshCw size={14} /> 刷新状态
            </>
          )}
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
              refreshing={refreshing}
              onSelect={onSelect}
              onToggleFavorite={onToggleFavorite}
            />
          ))}
        </div>
      ) : (
        // 空态：图标 + 标题 + 副说明，垂直居中匀称留白。
        <div className="flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-surface-1 px-4 py-12 text-center">
          <Server size={24} strokeWidth={1.75} className="text-fg-subtle" />
          <div className="text-[13px] font-medium text-fg-muted">还没有服务器</div>
          <div className="text-[12px] text-fg-subtle">先在左侧添加一台以开始概览。</div>
        </div>
      )}
      </div>
    </section>
  );
}

export default Dashboard;
