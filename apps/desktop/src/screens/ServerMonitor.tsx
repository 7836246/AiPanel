import { useEffect, useRef, useState, type JSX } from "react";
import {
  Activity,
  AlertCircle,
  Boxes,
  Cpu,
  Gauge,
  MemoryStick,
  Network,
  Plug,
  Server as ServerIcon,
} from "lucide-react";
import { Spinner } from "@aipanel/ui";
import { serverMetrics, type ServerMetrics } from "../lib/api";

// 轮询间隔（毫秒）。后端只回累计值，速率由前端跨样本求差，故无需后端 sleep 测速。
const POLL_MS = 3000;
// 状态中保留的最近样本数量（用于画图与算速率）。约 40 个 ≈ 2 分钟窗口。
const MAX_SAMPLES = 40;

// 带本地时间戳的一个样本：用 client receivedAt（ms）算 Δt，避免依赖远端时钟漂移。
interface Sample {
  metrics: ServerMetrics;
  receivedAt: number;
}

// ---- 通用 helper ------------------------------------------------------------

/** 把字节数格式化成人类可读（B/KB/MB/GB/TB，1024 进制，保留 1 位小数）。 */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  // 字节本身不带小数，其余保留 1 位。
  return `${i === 0 ? n : n.toFixed(1)} ${units[i]}`;
}

/** 把「字节/秒」格式化成速率字符串（KB/s、MB/s…）。 */
function formatRate(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec < 0) return "0 KB/s";
  // 速率最小以 KB/s 起步展示，贴近运维面板习惯。
  const units = ["KB/s", "MB/s", "GB/s"];
  let n = bytesPerSec / 1024;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  return `${n.toFixed(n >= 100 ? 0 : 1)} ${units[i]}`;
}

/** 把秒数格式化成「Xd Yh Zm」运行时长。 */
function formatUptime(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "—";
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}天 ${h}小时`;
  if (h > 0) return `${h}小时 ${m}分`;
  return `${m}分`;
}

// 按阈值映射风险颜色：<70 低、70–90 中、>90 阻断。返回 SVG stroke 用的 CSS 变量与文字 token。
function pctColor(pct: number): { stroke: string; text: string } {
  if (pct > 90) return { stroke: "var(--color-risk-blocked)", text: "text-risk-blocked" };
  if (pct >= 70) return { stroke: "var(--color-risk-medium)", text: "text-risk-medium" };
  return { stroke: "var(--color-risk-low)", text: "text-risk-low" };
}

// ---- SVG 环形仪表 -----------------------------------------------------------

/**
 * 自绘 SVG 环形仪表（不引图表库）：
 * - value：0–100 的填充百分比；ring 颜色按阈值着色。
 * - label：环心大字（如 "42%" 或负载值）。
 * - title / subtitle：环下标题与副标。
 */
function Ring({
  value,
  centerText,
  title,
  subtitle,
}: {
  value: number;
  centerText: string;
  title: string;
  subtitle: string;
}): JSX.Element {
  const size = 116;
  const stroke = 9;
  const r = (size - stroke) / 2;
  const circ = 2 * Math.PI * r;
  const clamped = Math.min(100, Math.max(0, value));
  const dash = (clamped / 100) * circ;
  const color = pctColor(clamped);

  return (
    <div className="flex flex-col items-center gap-2 rounded-md border border-border bg-surface-1 px-3 py-4">
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
        {/* 底环：浅灰轨道。 */}
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--color-surface-2)"
          strokeWidth={stroke}
        />
        {/* 进度环：按阈值着色，圆头线帽。 */}
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={color.stroke}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={`${dash} ${circ - dash}`}
          style={{ transition: "stroke-dasharray 0.4s ease, stroke 0.4s ease" }}
        />
        {/* 环心大字：反向旋转抵消父级 -90°，保持正向。 */}
        <text
          x="50%"
          y="50%"
          dominantBaseline="central"
          textAnchor="middle"
          className={`rotate-90 origin-center fill-current font-mono text-[17px] font-semibold ${color.text}`}
        >
          {centerText}
        </text>
      </svg>
      <div className="text-center">
        <div className="text-[12px] font-semibold text-fg">{title}</div>
        <div className="mt-0.5 text-[11px] text-fg-subtle">{subtitle}</div>
      </div>
    </div>
  );
}

// ---- SVG 双线面积图 ---------------------------------------------------------

/**
 * 自绘 SVG 折线/面积图（不引图表库）：双线展示上行/下行速率（字节/秒）。
 * - 共用同一 Y 轴（取两序列最大值为上界），各自半透明面积 + 实线。
 */
function RateChart({
  rx,
  tx,
}: {
  rx: number[]; // 接收速率序列（字节/秒）
  tx: number[]; // 发送速率序列（字节/秒）
}): JSX.Element {
  const w = 600;
  const h = 140;
  const pad = 6;
  const n = Math.max(rx.length, tx.length);
  // Y 轴上界：两序列最大值，留 15% 余量；全 0 时给个最小值避免除零。
  const peak = Math.max(1, ...rx, ...tx) * 1.15;

  // 把一条序列映射成 SVG 折线点串；x 均匀分布，y 反向（顶部为大值）。
  const toPoints = (data: number[]): { x: number; y: number }[] =>
    data.map((v, i) => ({
      x: pad + (n <= 1 ? 0 : (i / (n - 1)) * (w - 2 * pad)),
      y: pad + (1 - v / peak) * (h - 2 * pad),
    }));

  const line = (pts: { x: number; y: number }[]): string =>
    pts.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");

  // 面积：折线 + 沿底边闭合。
  const area = (pts: { x: number; y: number }[]): string => {
    if (pts.length === 0) return "";
    const first = pts[0];
    const last = pts[pts.length - 1];
    return `${line(pts)} L${last.x.toFixed(1)},${h - pad} L${first.x.toFixed(1)},${h - pad} Z`;
  };

  const rxPts = toPoints(rx);
  const txPts = toPoints(tx);
  // 下行用 brand 色、上行用 risk-low（绿），与运维面板「收/发」配色直觉一致。
  const rxColor = "var(--color-brand)";
  const txColor = "var(--color-risk-low)";

  return (
    <svg
      width="100%"
      height={h}
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      className="rounded-md border border-border bg-surface-1"
    >
      {/* 三条横向网格线。 */}
      {[0.25, 0.5, 0.75].map((g) => (
        <line
          key={g}
          x1={pad}
          x2={w - pad}
          y1={pad + g * (h - 2 * pad)}
          y2={pad + g * (h - 2 * pad)}
          stroke="var(--color-border)"
          strokeWidth={0.5}
          strokeDasharray="3 4"
        />
      ))}
      {/* 下行面积 + 线。 */}
      <path d={area(rxPts)} fill={rxColor} opacity={0.1} />
      <path d={line(rxPts)} fill="none" stroke={rxColor} strokeWidth={1.6} />
      {/* 上行面积 + 线。 */}
      <path d={area(txPts)} fill={txColor} opacity={0.1} />
      <path d={line(txPts)} fill="none" stroke={txColor} strokeWidth={1.6} />
    </svg>
  );
}

// ---- 小卡：计数 / 标题 ------------------------------------------------------

/** 带左竖条的分段标题（贴近 1Panel/宝塔卡片头风格）。 */
function SectionTitle({ icon, text }: { icon: JSX.Element; text: string }): JSX.Element {
  return (
    <div className="mb-2.5 flex items-center gap-2">
      <span className="h-3.5 w-1 rounded-full bg-brand" />
      {icon}
      <span className="text-[13px] font-semibold text-fg">{text}</span>
    </div>
  );
}

/** 概览计数小卡：图标 + 数值 + 标签。 */
function CountCard({
  icon,
  value,
  label,
}: {
  icon: JSX.Element;
  value: number;
  label: string;
}): JSX.Element {
  return (
    <div className="flex items-center gap-3 rounded-md border border-border bg-surface-1 px-3.5 py-3">
      <span className="flex h-9 w-9 flex-none items-center justify-center rounded-md bg-surface-2 text-fg-muted">
        {icon}
      </span>
      <div className="min-w-0">
        <div className="font-mono text-[18px] font-semibold leading-none text-fg">{value}</div>
        <div className="mt-1 truncate text-[11px] text-fg-subtle">{label}</div>
      </div>
    </div>
  );
}

/** 图例点（用于监控图上行/下行 + 即时值）。 */
function Legend({
  color,
  label,
  value,
}: {
  color: string;
  label: string;
  value: string;
}): JSX.Element {
  return (
    <div className="flex items-center gap-1.5">
      <span className="h-2 w-2 rounded-full" style={{ backgroundColor: color }} />
      <span className="text-[11px] text-fg-subtle">{label}</span>
      <span className="font-mono text-[12px] font-semibold text-fg">{value}</span>
    </div>
  );
}

// ---- 主组件 -----------------------------------------------------------------

/**
 * 服务器监控面板（纯展示 + 轮询，不改服务器状态，不暴露给 AI）。
 *
 * - 每 POLL_MS 轮询 serverMetrics(serverId)；serverId 变化重置；卸载清定时器；
 *   document.hidden 时暂停轮询（回到前台立即补一次）。
 * - 保留最近 MAX_SAMPLES 个样本用于画图与跨样本算速率。
 * - 竞态：用单调递增的请求序号守卫，丢弃过期轮询结果。
 * - 首样本前显示 Spinner；出错显示内联红条并保留上次数据。
 */
export default function ServerMonitor({ serverId }: { serverId: string }): JSX.Element {
  const [samples, setSamples] = useState<Sample[]>([]);
  const [error, setError] = useState<string | null>(null);
  // 请求序号守卫：每次发起轮询自增，回调只接受「最新一次」的结果。
  const seqRef = useRef(0);

  useEffect(() => {
    // serverId 变化：清空旧服务器的样本与错误，避免串图。
    setSamples([]);
    setError(null);
    // 本次 effect 的「代」号:只在 serverId 变化(新 effect)时自增,用于丢弃**旧服务器**的结果。
    // 注意:绝不要在每次 poll 时自增——否则一旦单次采集(经代理)慢于轮询间隔,后发的轮询会把
    // 先发请求的结果判成「过期」而丢弃,导致永远不入样本、一直转圈。
    const gen = (seqRef.current += 1);

    let cancelled = false;
    // 在途守卫:上一轮还没回来就跳过这一轮,避免慢链路下并发 SSH 堆积。
    let inFlight = false;

    const poll = async (): Promise<void> => {
      if (cancelled || inFlight) return;
      // document.hidden 时跳过本轮拉取,省流并避免后台堆积。
      if (typeof document !== "undefined" && document.hidden) return;
      inFlight = true;
      try {
        const m = await serverMetrics(serverId);
        // 仅当切换到了别的服务器(更新的 effect)才丢弃;同一服务器的结果一律接受。
        if (cancelled || gen !== seqRef.current) return;
        setError(null);
        setSamples((prev) => {
          const next = [...prev, { metrics: m, receivedAt: Date.now() }];
          return next.length > MAX_SAMPLES ? next.slice(next.length - MAX_SAMPLES) : next;
        });
      } catch (err) {
        if (cancelled || gen !== seqRef.current) return;
        // 出错保留上次数据,仅显示内联红条。
        setError(String(err));
      } finally {
        inFlight = false;
      }
    };

    void poll(); // 立即拉一次，避免首屏等待整个间隔。
    const timer = setInterval(() => void poll(), POLL_MS);

    // 回到前台立即补一次（暂停期间没有累积样本，先补齐再恢复节奏）。
    const onVisible = (): void => {
      if (typeof document !== "undefined" && !document.hidden) void poll();
    };
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [serverId]);

  // 首样本前：居中 Spinner。
  if (samples.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 rounded-md border border-border bg-surface-1 px-4 py-16 text-center">
        <Spinner size="md" />
        <div className="text-[13px] text-fg-muted">正在采集服务器指标…</div>
        {error && (
          <div className="flex items-center gap-1.5 text-[12px] text-risk-blocked">
            <AlertCircle size={13} /> {error}
          </div>
        )}
      </div>
    );
  }

  const latest = samples[samples.length - 1].metrics;

  // 计算各序列速率：相邻样本 (curr - prev) / Δt（秒）。net 计数器只增，回绕/负值钳为 0。
  const rxSeries: number[] = [];
  const txSeries: number[] = [];
  for (let i = 1; i < samples.length; i += 1) {
    const a = samples[i - 1];
    const b = samples[i];
    const dt = (b.receivedAt - a.receivedAt) / 1000;
    if (dt <= 0) {
      rxSeries.push(0);
      txSeries.push(0);
      continue;
    }
    rxSeries.push(Math.max(0, (b.metrics.netRxBytes - a.metrics.netRxBytes) / dt));
    txSeries.push(Math.max(0, (b.metrics.netTxBytes - a.metrics.netTxBytes) / dt));
  }
  // 即时速率：取最后一个差分；不足两样本时为 0。
  const rxNow = rxSeries.length > 0 ? rxSeries[rxSeries.length - 1] : 0;
  const txNow = txSeries.length > 0 ? txSeries[txSeries.length - 1] : 0;

  // 派生百分比。
  const memPct =
    latest.memTotalBytes > 0 ? (latest.memUsedBytes / latest.memTotalBytes) * 100 : 0;
  const diskPct =
    latest.diskTotalBytes > 0 ? (latest.diskUsedBytes / latest.diskTotalBytes) * 100 : 0;
  // 负载相对核数：load1/cores 作为「负载饱和度」百分比（满核=100%）。
  const loadSat = latest.cpuCores > 0 ? (latest.load1 / latest.cpuCores) * 100 : 0;
  const loadHealthy = loadSat < 100;

  const rxColor = "var(--color-brand)";
  const txColor = "var(--color-risk-low)";

  return (
    <div className="flex flex-col gap-4">
      {/* 头部：标识 + 采样时间 + 运行时长。 */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border bg-surface-1 px-4 py-3">
        <ServerIcon size={15} className="text-fg-muted" />
        <span className="text-[13px] font-semibold text-fg">实时监控</span>
        <span className="flex items-center gap-1 text-[12px] text-fg-subtle">
          <Activity size={12} className="text-risk-low" /> 运行 {formatUptime(latest.uptimeSecs)}
        </span>
        <span className="ml-auto font-mono text-[11px] text-fg-subtle">
          采样于 {new Date(latest.sampledAt).toLocaleTimeString()} · 每 {POLL_MS / 1000}s
        </span>
      </div>

      {/* 出错内联红条（保留上次数据）。 */}
      {error && (
        <div className="flex items-center gap-1.5 rounded-md bg-risk-blocked-soft px-3 py-2 text-[12px] text-risk-blocked">
          <AlertCircle size={13} className="flex-none" /> 指标刷新失败：{error}（展示上次数据）
        </div>
      )}

      {/* 1) 概览：计数卡。 */}
      <section>
        <SectionTitle icon={<Boxes size={14} className="text-fg-muted" />} text="概览" />
        <div className="grid grid-cols-2 gap-2.5 md:grid-cols-4">
          <CountCard icon={<Boxes size={17} />} value={latest.containers} label="容器" />
          <CountCard icon={<Plug size={17} />} value={latest.services} label="运行服务" />
          <CountCard icon={<Network size={17} />} value={latest.listeningPorts} label="监听端口" />
          <CountCard icon={<Activity size={17} />} value={latest.procs} label="进程" />
        </div>
      </section>

      {/* 2) 状态：四个环形仪表。 */}
      <section>
        <SectionTitle icon={<Gauge size={14} className="text-fg-muted" />} text="状态" />
        <div className="grid grid-cols-2 gap-2.5 md:grid-cols-4">
          <Ring
            value={Math.min(100, loadSat)}
            centerText={latest.load1.toFixed(2)}
            title="系统负载"
            subtitle={`${latest.cpuCores} 核 · ${loadHealthy ? "运行流畅" : "负载偏高"}`}
          />
          <Ring
            value={latest.cpuPercent}
            centerText={`${latest.cpuPercent.toFixed(1)}%`}
            title="CPU"
            subtitle={`${(latest.cpuPercent / 100 * latest.cpuCores).toFixed(1)}/${latest.cpuCores} 核`}
          />
          <Ring
            value={memPct}
            centerText={`${memPct.toFixed(0)}%`}
            title="内存"
            subtitle={`${formatBytes(latest.memUsedBytes)} / ${formatBytes(latest.memTotalBytes)}`}
          />
          <Ring
            value={diskPct}
            centerText={`${diskPct.toFixed(0)}%`}
            title="磁盘"
            subtitle={`${formatBytes(latest.diskUsedBytes)} / ${formatBytes(latest.diskTotalBytes)} · ${latest.diskPath}`}
          />
        </div>
        {/* 交换分区（若有）：作为补充细条，避免占满状态网格。 */}
        {latest.swapTotalBytes > 0 && (
          <div className="mt-2.5 flex items-center gap-2 rounded-md border border-border bg-surface-1 px-3.5 py-2 text-[11px] text-fg-subtle">
            <MemoryStick size={13} className="text-fg-muted" />
            <span>交换分区</span>
            <span className="font-mono text-fg-muted">
              {formatBytes(latest.swapUsedBytes)} / {formatBytes(latest.swapTotalBytes)}
            </span>
            <span className="ml-auto inline-flex items-center gap-1">
              <Cpu size={12} /> {latest.cpuCores} 核
            </span>
          </div>
        )}
      </section>

      {/* 3) 监控：网络速率双线图。 */}
      <section>
        <SectionTitle icon={<Network size={14} className="text-fg-muted" />} text="监控" />
        <div className="rounded-md border border-border bg-surface-1 p-3">
          {/* 即时值 + 累计收发图例。 */}
          <div className="mb-2.5 flex flex-wrap items-center gap-x-5 gap-y-1.5">
            <Legend color={rxColor} label="下行" value={formatRate(rxNow)} />
            <Legend color={txColor} label="上行" value={formatRate(txNow)} />
            <span className="ml-auto text-[11px] text-fg-subtle">
              总接收 <span className="font-mono text-fg-muted">{formatBytes(latest.netRxBytes)}</span>
              {"  ·  "}
              总发送 <span className="font-mono text-fg-muted">{formatBytes(latest.netTxBytes)}</span>
            </span>
          </div>
          {/* 至少 2 个样本（≥1 个差分）才有意义。 */}
          {rxSeries.length >= 1 ? (
            <RateChart rx={rxSeries} tx={txSeries} />
          ) : (
            <div className="flex h-[140px] flex-col items-center justify-center gap-1.5 rounded-md border border-dashed border-border text-center">
              <Network size={20} className="text-fg-subtle" />
              <div className="text-[12px] text-fg-muted">采集更多样本后绘制速率曲线…</div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
