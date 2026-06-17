import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  Button,
  IconButton,
  Spinner,
  Terminal,
  ToastViewport,
  useToasts,
  type TerminalLine,
} from "@aipanel/ui";
import AddServerDialog from "./AddServerDialog";
import ConfirmExecuteDialog from "./ConfirmExecuteDialog";
import EditServerDialog from "./EditServerDialog";
import SettingsPanel, { READONLY_DEFAULT_KEY } from "./SettingsPanel";
import AuditView from "./AuditView";
import ServerOverview from "./ServerOverview";
import CommandPalette, { type PaletteCommand } from "./CommandPalette";
import {
  isTauri,
  cancelRun,
  checkSshConnection,
  createPlan,
  getModelSelectionPolicy,
  reviewPlan,
  runAgentTurn,
  runConfirmedPlanStream,
  runServerDoctorStream,
  serverDoctorPlan,
  listProviders,
  listServers,
  listTasks,
  saveTask,
  deleteTask,
  RISK_META,
  type AppError,
  type CommandExecution,
  type ModelSelectionPolicy,
  type ProviderConfig,
  type RiskLevel,
  type RiskReview,
  type ServerProfile,
  type ServerStatus,
  type TaskRecord,
} from "../lib/api";
import "./codex-console.css";

// 从后端错误或任意异常中提取可展示的错误文本。
const errMsg = (e: unknown): string =>
  e && typeof e === "object" && "message" in e ? (e as AppError).message : String(e);

const nowIso = () => new Date().toISOString();
// 生成本地任务 ID，优先用 crypto.randomUUID，缺失时回退到时间戳+随机串。
const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `t-${Date.now()}-${Math.random().toString(16).slice(2)}`;

// 服务器状态对应的小圆点颜色类。
const statusDot = (s: ServerStatus): string =>
  s === "online" ? "bg-risk-low" : s === "offline" ? "bg-risk-blocked" : "bg-fg-subtle";

// 单个计划步骤在 UI 中的执行状态。
type StepStatus = "pending" | "running" | "done" | "failed";

/* ---------------- 图标 ---------------- */
type IconProps = { size?: number };
const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};
const Pencil = ({ size = 16 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke}>
    <path d="M11.5 2.5l2 2L6 12l-2.5.5L4 10z" />
  </svg>
);
const ListIcon = ({ size = 16 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke}>
    <line x1="3" y1="5" x2="13" y2="5" />
    <line x1="3" y1="8.5" x2="13" y2="8.5" />
    <line x1="3" y1="12" x2="9" y2="12" />
  </svg>
);
const ServerIcon = ({ size = 15 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke} strokeWidth={1.4}>
    <path d="M2.5 5.5l1-2h9l1 2v6.5a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z" />
  </svg>
);
const Gear = ({ size = 16 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke} strokeWidth={1.4}>
    <circle cx="8" cy="8" r="2.3" />
    <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
  </svg>
);
const Plus = ({ size = 14 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke}>
    <line x1="8" y1="3.5" x2="8" y2="12.5" />
    <line x1="3.5" y1="8" x2="12.5" y2="8" />
  </svg>
);
const Copy = ({ size = 13 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke} strokeWidth={1.4}>
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
    <path d="M3.5 10.5H3a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1h6.5a1 1 0 0 1 1 1v.5" />
  </svg>
);
const SendArrow = ({ size = 15 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke} strokeWidth={1.8}>
    <line x1="8" y1="12.5" x2="8" y2="4" />
    <path d="M4.5 7.5L8 4l3.5 3.5" />
  </svg>
);
const TerminalIcon = ({ size = 15 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke} strokeWidth={1.4}>
    <rect x="2" y="3" width="12" height="10" rx="1.6" />
    <path d="M4.5 6.5l2 1.5-2 1.5M8 10h3.2" />
  </svg>
);
const ThemeIcon = ({ size = 15 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth={1.4} />
    <path d="M8 1.8A6.2 6.2 0 0 1 8 14.2Z" fill="currentColor" />
  </svg>
);
const Play = ({ size = 12 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 14 14" fill="currentColor">
    <path d="M4 3l7 4-7 4z" />
  </svg>
);
const Check = ({ size = 14 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="var(--color-risk-low)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M3.5 8.3l2.6 2.6L12.5 4.8" />
  </svg>
);
const Cross = ({ size = 14 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="var(--color-risk-blocked)" strokeWidth={2} strokeLinecap="round">
    <path d="M4 4l8 8M12 4l-8 8" />
  </svg>
);

/* ---------------- 主题 ---------------- */
// 主题钩子：持久化到 localStorage，并切换 <html> 的 dark 类；返回 [当前主题, 切换函数]。
function useTheme(): [("light" | "dark"), () => void] {
  const [theme, setTheme] = useState<"light" | "dark">(
    () => (localStorage.getItem("aipanel-theme") as "light" | "dark") ?? "light"
  );
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("aipanel-theme", theme);
  }, [theme]);
  return [theme, () => setTheme((t) => (t === "light" ? "dark" : "light"))];
}

/* ---------------- 子组件 ---------------- */
// 侧栏导航项：图标 + 文案，可选快捷键提示与选中态。
function NavItem({ icon, label, kbd, active, onClick }: {
  icon: ReactNode; label: string; kbd?: string; active?: boolean; onClick?: () => void;
}) {
  return (
    <div
      onClick={onClick}
      className={`flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] transition-colors ${
        active ? "bg-selected text-fg" : "text-fg-muted hover:bg-hover"
      }`}
    >
      {icon}
      <span className="flex-1">{label}</span>
      {kbd ? <span className="text-[11.5px] text-fg-subtle">{kbd}</span> : null}
    </div>
  );
}

// 任务类型对应的侧栏小字形,便于一眼区分计划/诊断/体检。
const KIND_GLYPH: Record<string, string> = { plan: "≣", diagnose: "✦", doctor: "✚" };

// 任务状态到中文标签的映射。
const STATUS_LABEL: Record<string, string> = {
  completed: "完成", failed: "失败", blocked: "已阻止", running: "进行中",
  awaiting_confirmation: "待确认", planning: "规划中", pending: "待处理",
};

// 计划中的一行步骤：状态图标、摘要、风险标签，以及可复制的命令。
function StepRow({ summary, command, risk, status }: {
  summary: string; command: string; risk: RiskLevel; status: StepStatus;
}) {
  const meta = RISK_META[risk];
  const [copied, setCopied] = useState(false);
  return (
    <div className="overflow-hidden rounded-md border border-border bg-surface-1">
      <div className="flex items-center gap-2.5 px-3.5 py-3">
        {status === "done" && <Check />}
        {status === "failed" && <Cross />}
        {status === "running" && <Spinner size="sm" />}
        {status === "pending" && (
          <span className="h-3.5 w-3.5 rounded-full border-[1.5px] border-border-strong" />
        )}
        <span className="min-w-0 flex-1 text-[13.5px] font-semibold">{summary}</span>
        <span className="inline-flex items-center gap-1.5 text-[11.5px] text-fg-muted">
          <span className={`h-1.5 w-1.5 rounded-full ${meta.dot}`} />
          {meta.label}
        </span>
      </div>
      <div className="px-3.5 pb-3.5">
        <div className="flex items-center gap-2.5 rounded-md bg-hover px-3 py-2 font-mono text-xs">
          <span className="text-fg-subtle">$</span>
          <span className="min-w-0 flex-1 truncate">{command}</span>
          <IconButton
            aria-label="复制命令"
            size="sm"
            onClick={async () => {
              try {
                if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
                await navigator.clipboard.writeText(command);
                setCopied(true);
                setTimeout(() => setCopied(false), 1200);
              } catch {
                /* 复制失败：不显示「已复制」对勾 */
              }
            }}
          >
            {copied ? <Check size={12} /> : <Copy />}
          </IconButton>
        </div>
      </div>
    </div>
  );
}

/* ---------------- 主屏 ---------------- */
// 主控制台：左侧服务器/历史导航，右侧计划生成、执行、体检、诊断与终端输出。
export default function CodexConsole() {
  const [theme, toggleTheme] = useTheme();
  const [view, setView] = useState<"console" | "audit" | "settings">("console");
  const [servers, setServers] = useState<ServerProfile[]>([]);
  const [selectedServerId, setSelectedServerId] = useState<string | null>(null);
  const [serverQuery, setServerQuery] = useState("");
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [current, setCurrent] = useState<TaskRecord | null>(null);
  const [stepStatus, setStepStatus] = useState<StepStatus[]>([]);
  const [termLines, setTermLines] = useState<TerminalLine[]>([]);
  const [terminalOpen, setTerminalOpen] = useState(true);
  const [running, setRunning] = useState(false);
  // 默认只读优先：从设置写入的 localStorage 读取初始值（缺省安全地为开）。
  const [readOnlyMode, setReadOnlyMode] = useState(
    () => localStorage.getItem(READONLY_DEFAULT_KEY) !== "false"
  );
  // 在控制台里切换只读优先：写入状态的同时回写 localStorage，与设置页保持一致。
  // 支持传入布尔值或更新函数（与 setState 用法对齐）。
  const setReadOnly = (next: boolean | ((v: boolean) => boolean)) => {
    setReadOnlyMode((prev) => {
      const v = typeof next === "function" ? next(prev) : next;
      try {
        localStorage.setItem(READONLY_DEFAULT_KEY, v ? "true" : "false");
      } catch {
        // 隐私模式等场景下 localStorage 可能不可写，静默忽略。
      }
      return v;
    });
  };
  // 模型选择策略：用于决定输入区展示的供应商与后端实际选用保持一致。
  const [policy, setPolicy] = useState<ModelSelectionPolicy>({ auto: true });
  // 命令面板（⌘K）开关。
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [intentValue, setIntentValue] = useState("");
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [editing, setEditing] = useState<ServerProfile | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmReview, setConfirmReview] = useState<RiskReview | null>(null);
  // 单调递增的运行标识：每次发起新操作就自增，用于丢弃切换上下文后过期的异步回调。
  const runIdRef = useRef(0);
  // 当前流式任务的后端取消句柄（doctor/计划执行），供「停止」真正中断远端命令。
  const cancelIdRef = useRef("");
  // 任务输入框引用，供命令面板「新建提问」聚焦。
  const inputRef = useRef<HTMLInputElement>(null);
  // 镜像当前任务到 ref，便于在切换/停止时读取最新值而不受闭包陈旧影响。
  const currentRef = useRef<TaskRecord | null>(null);
  // 轻量通知（错误/成功提示），替代仅在终端里报错。
  const { toasts, push, dismiss } = useToasts();

  // 真正中断进行中的后端流式运行：作废前端回调 + 唤醒后端取消句柄。
  function cancelBackend() {
    runIdRef.current += 1;
    if (cancelIdRef.current) {
      cancelRun(cancelIdRef.current);
      cancelIdRef.current = "";
    }
  }

  const selected = servers.find((s) => s.id === selectedServerId) ?? null;
  // 当前可用于 AI 诊断的供应商：优先采用模型选择策略里的默认供应商
  // （命中且已启用时），与后端实际选用保持一致；否则回退到首个「启用且非 custom」。
  const policyDefault = policy.defaultProviderId
    ? providers.find((p) => p.id === policy.defaultProviderId && p.enabled)
    : undefined;
  const aiProvider =
    policyDefault ?? providers.find((p) => p.enabled && p.kind !== "custom") ?? null;
  const filteredServers = servers.filter(
    (s) => !serverQuery || s.name.toLowerCase().includes(serverQuery.toLowerCase()) || s.host.includes(serverQuery)
  );

  useEffect(() => {
    listServers().then((s) => { setServers(s); setSelectedServerId((cur) => cur ?? s[0]?.id ?? null); }).catch(() => {});
    listProviders().then(setProviders).catch(() => {});
    getModelSelectionPolicy().then(setPolicy).catch(() => {});
  }, []);

  // 保持 currentRef 与 current 同步。
  useEffect(() => { currentRef.current = current; }, [current]);

  // 选中服务器变化时：真正取消进行中的运行（中断远端命令），再重置当前打开的运行并加载历史。
  useEffect(() => {
    cancelBackend();
    setRunning(false);
    setStepStatus([]);
    setCurrent(null);
    setTermLines([]);
    if (!selectedServerId) { setTasks([]); return; }
    listTasks(selectedServerId).then(setTasks).catch(() => setTasks([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedServerId]);

  // 从设置返回时重新拉取供应商与模型选择策略，刷新顶部横幅与模型选择器。
  useEffect(() => {
    if (view !== "settings") {
      listProviders().then(setProviders).catch(() => {});
      getModelSelectionPolicy().then(setPolicy).catch(() => {});
    }
  }, [view]);

  // 选中服务器后静默做一次 SSH 连通性检查，更新在线状态点。
  // 仅在 Tauri 下为真实探测；浏览器 mock 会返回随机值，故跳过以免误导状态点。
  useEffect(() => {
    if (!selectedServerId || !isTauri()) return;
    const id = selectedServerId;
    checkSshConnection(id)
      .then((ok) =>
        setServers((prev) => prev.map((s) => (s.id === id ? { ...s, status: ok ? "online" : "offline" } : s)))
      )
      .catch(() => {});
  }, [selectedServerId]);

  async function refreshTasks() {
    if (selectedServerId) setTasks(await listTasks(selectedServerId).catch(() => []));
  }

  // 打开一条历史任务：先取消任何进行中的运行（避免其回调冲掉刚打开的内容），
  // 再回填步骤状态与终端输出，并切回控制台视图。
  function openTask(t: TaskRecord) {
    cancelBackend();
    setRunning(false);
    setCurrent(t);
    setStepStatus((t.plan?.steps ?? []).map((_, i) => (t.executions[i] ? (t.executions[i].exitCode === 0 ? "done" : "failed") : "pending")));
    setTermLines(execLines(t.executions, t.summary));
    setView("console");
  }

  // 把命令执行结果转成终端行：命令以 prompt 着色，输出按退出码区分正常/错误，末尾追加总结。
  function execLines(executions: CommandExecution[], summary?: string): TerminalLine[] {
    const lines: TerminalLine[] = [];
    for (const ex of executions) {
      lines.push({ text: `$ ${ex.command}`, tone: "prompt" });
      for (const l of (ex.stdout || ex.stderr).split("\n")) if (l.trim()) lines.push({ text: l, tone: ex.exitCode === 0 ? "default" : "danger" });
    }
    if (summary) { lines.push({ text: "", tone: "muted" }); lines.push({ text: summary, tone: "muted" }); }
    return lines;
  }

  // ----- 操作 -----
  // 根据输入意图生成计划，保存为待确认任务（不会自动执行）。
  async function generatePlan() {
    const intent = intentValue.trim();
    if (!intent || !selectedServerId) return;
    const myId = ++runIdRef.current;
    const serverId = selectedServerId;
    setRunning(true);
    setTermLines([{ text: aiProvider ? "AI 规划中…" : "生成计划中(本地规则)…", tone: "muted" }]);
    try {
      const plan = await createPlan(intent, serverId);
      if (runIdRef.current !== myId) return; // 期间已切换上下文则丢弃结果
      const task: TaskRecord = {
        id: newId(), serverId, title: plan.goal, intent, kind: "plan",
        plan, executions: [], status: "awaiting_confirmation", createdAt: nowIso(), updatedAt: nowIso(),
      };
      await saveTask(task);
      if (runIdRef.current !== myId) return;
      setCurrent(task);
      setStepStatus(plan.steps.map(() => "pending"));
      setTermLines([]);
      setIntentValue("");
      await refreshTasks();
    } catch (e) {
      if (runIdRef.current !== myId) return;
      setTermLines([{ text: `生成计划失败: ${errMsg(e)}`, tone: "danger" }]);
      setTerminalOpen(true);
      push("danger", `生成计划失败: ${errMsg(e)}`);
    } finally {
      if (runIdRef.current === myId) setRunning(false);
    }
  }

  // AI 诊断：让模型通过只读工具自主调查并给出总结（需先配置 AI 供应商）。
  async function diagnose() {
    const intent = intentValue.trim();
    if (!intent || !selectedServerId) return;
    // 未配置供应商：给出明确反馈再跳转设置（不要静默跳走）。
    if (!aiProvider) { push("info", "AI 诊断需要先配置模型供应商"); setView("settings"); return; }
    const myId = ++runIdRef.current;
    const serverId = selectedServerId;
    setRunning(true);
    setTerminalOpen(true);
    setCurrent(null);
    setTermLines([{ text: "AI 诊断中…", tone: "muted" }]);
    try {
      const r = await runAgentTurn(intent, serverId);
      if (runIdRef.current !== myId) return;
      const lines: TerminalLine[] = r.toolCalls.map((t) => ({ text: `▸ ${t.name} ${t.ok ? "✓" : "✗"}`, tone: t.ok ? "success" : "danger" }));
      if (lines.length) lines.push({ text: "", tone: "muted" });
      for (const l of r.summary.split("\n")) lines.push({ text: l });
      setTermLines(lines);
      const task: TaskRecord = {
        id: newId(), serverId, title: intent, intent, kind: "diagnose",
        executions: [], toolCalls: r.toolCalls, summary: r.summary, status: "completed", createdAt: nowIso(), updatedAt: nowIso(),
      };
      await saveTask(task);
      if (runIdRef.current !== myId) return;
      setCurrent(task);
      setIntentValue("");
      await refreshTasks();
    } catch (e) {
      if (runIdRef.current !== myId) return;
      setTermLines([{ text: `诊断失败: ${errMsg(e)}`, tone: "danger" }]);
      push("danger", `诊断失败: ${errMsg(e)}`);
    } finally {
      if (runIdRef.current === myId) setRunning(false);
    }
  }

  // 只读体检：流式运行 doctor 计划，全程为只读检查命令。
  async function runDoctor() {
    if (!selectedServerId || running) return;
    const myId = ++runIdRef.current;
    // 取消句柄要在任何 await 之前就绪，保证「刚点体检立刻点停止」也能真正中断。
    const cancelId = newId();
    cancelIdRef.current = cancelId;
    setRunning(true);
    setTerminalOpen(true);
    const plan = await serverDoctorPlan(selectedServerId).catch(() => null);
    const task: TaskRecord = {
      id: newId(), serverId: selectedServerId, title: "只读服务器体检", intent: "只读服务器体检",
      kind: "doctor", plan: plan ?? undefined, executions: [], status: "running", createdAt: nowIso(), updatedAt: nowIso(),
    };
    setCurrent(task);
    setStepStatus((plan?.steps ?? []).map(() => "pending"));
    await saveTask(task); // 先把「运行中」状态持久化，即使中断也能在历史里看到
    await refreshTasks();
    const lines: TerminalLine[] = [{ text: `正在体检 ${selected?.name ?? ""} …`, tone: "muted" }];
    setTermLines([...lines]);
    try {
      const report = await runServerDoctorStream(selectedServerId, (ev) => {
        if (runIdRef.current !== myId) return;
        if (ev.type === "step") {
          setStepStatus((prev) => { const n = [...prev]; n[ev.index] = ev.status === "running" ? "running" : ev.status; return n; });
          if (ev.status !== "running") lines.push({ text: `▸ [${ev.index + 1}/${ev.total}] ${ev.summary} · ${ev.status === "done" ? "完成" : "失败"}`, tone: ev.status === "failed" ? "danger" : "muted" });
        } else if (ev.type === "line") {
          lines.push({ text: ev.text, tone: ev.stderr ? "danger" : "default" });
        } else {
          lines.push({ text: `⚠ ${ev.message}`, tone: "danger" });
        }
        setTermLines([...lines]);
      }, cancelId);
      if (runIdRef.current !== myId) return;
      const failed = report.executions.filter((e) => e.exitCode !== 0).length;
      const ok = failed === 0;
      const parts: string[] = [];
      if (failed) parts.push(`${failed} 项检查失败`);
      if (report.warnings.length) parts.push(`${report.warnings.length} 条告警`);
      const summary = parts.length ? parts.join(" · ") : "体检完成,无告警";
      const done: TaskRecord = { ...task, executions: report.executions, summary, status: ok ? "completed" : "failed", updatedAt: nowIso() };
      await saveTask(done);
      setCurrent(done);
      setServers(await listServers());
      await refreshTasks();
      push(ok ? "success" : "danger", `体检${ok ? "完成" : "未全部通过"}：${summary}`);
    } catch (e) {
      if (runIdRef.current !== myId) return;
      lines.push({ text: `体检失败: ${errMsg(e)}`, tone: "danger" });
      setTermLines([...lines]);
      await saveTask({ ...task, status: "failed", summary: errMsg(e), updatedAt: nowIso() });
      await refreshTasks();
      push("danger", `体检失败: ${errMsg(e)}`);
    } finally {
      if (runIdRef.current === myId) setRunning(false);
    }
  }

  // 先对生成的计划做风险审查，再决定确认（或纯只读时直接执行）。
  async function startExecute() {
    if (!current?.plan) return;
    try {
      const review = await reviewPlan(current.plan, readOnlyMode);
      // 既未被阻止又无需确认（纯低风险）则直接执行，否则弹出确认对话框。
      if (!review.blocked && !review.requiresConfirmation) { await execute(true, false, review); return; }
      setConfirmReview(review);
      setConfirmOpen(true);
    } catch (e) {
      setTermLines([{ text: `风险审查失败: ${errMsg(e)}`, tone: "danger" }]);
      setTerminalOpen(true);
      push("danger", `风险审查失败: ${errMsg(e)}`);
    }
  }

  // 流式执行已确认的计划，逐步更新步骤状态与终端输出，并把结果落库。
  async function execute(confirmed: boolean, doubleConfirmed: boolean, review?: RiskReview) {
    setConfirmOpen(false);
    if (!current?.plan || !confirmed) return;
    const plan = current.plan;
    const myId = ++runIdRef.current;
    setRunning(true);
    setTerminalOpen(true);
    setStepStatus(plan.steps.map(() => "pending"));
    const lines: TerminalLine[] = [{ text: "执行中…", tone: "muted" }];
    setTermLines([...lines]);
    const cancelId = newId();
    cancelIdRef.current = cancelId;
    try {
      const rec = await runConfirmedPlanStream(plan, { confirmed, doubleConfirmed, readOnlyMode, runId: cancelId }, (ev) => {
        if (runIdRef.current !== myId) return;
        if (ev.type === "step") {
          setStepStatus((prev) => { const n = [...prev]; n[ev.index] = ev.status === "running" ? "running" : ev.status; return n; });
        } else if (ev.type === "line") {
          lines.push({ text: ev.text, tone: ev.stderr ? "danger" : "default" });
          setTermLines([...lines]);
        }
      });
      // 即使期间被取消/切走（runId 不再匹配），也把后端返回的最终结果落库，
      // 避免历史里的计划任务停留在「待确认」、已执行步骤丢失。
      const done: TaskRecord = {
        ...current, plan, riskReview: review ?? current.riskReview, executions: rec.executions,
        summary: rec.summary, status: rec.status, updatedAt: nowIso(),
      };
      await saveTask(done);
      await refreshTasks();
      if (runIdRef.current !== myId) return; // 界面已切走则不再更新当前视图
      setCurrent(done);
      setTermLines(execLines(rec.executions, rec.summary));
      setServers(await listServers());
      push(rec.status === "completed" ? "success" : "danger", rec.status === "completed" ? "计划执行完成" : "计划执行未全部成功");
    } catch (e) {
      if (runIdRef.current !== myId) return;
      lines.push({ text: `执行失败: ${errMsg(e)}`, tone: "danger" });
      setTermLines([...lines]);
      push("danger", `执行失败: ${errMsg(e)}`);
    } finally {
      if (runIdRef.current === myId) setRunning(false);
    }
  }

  // 停止当前运行：真正中断远端命令，回退步骤状态，给出「已取消」提示，
  // 并把仍处于「运行中」的当前任务落库为终态，避免历史里留下僵尸记录。
  async function stop() {
    cancelBackend();
    setRunning(false);
    setStepStatus((prev) => prev.map((s) => (s === "running" ? "pending" : s)));
    setTermLines((prev) => [...prev, { text: "⏹ 已取消", tone: "muted" }]);
    const cur = currentRef.current;
    if (cur && cur.status === "running") {
      const cancelled: TaskRecord = { ...cur, status: "failed", summary: cur.summary ?? "已取消", updatedAt: nowIso() };
      await saveTask(cancelled).catch(() => {});
      if (currentRef.current?.id === cancelled.id) setCurrent(cancelled);
      await refreshTasks();
    }
  }

  // 切到审计视图（AuditView 自行加载/搜索审计记录）。
  function openAudit() {
    setView("audit");
  }

  // 命令面板的动作集合：把主界面的关键操作集中为可搜索/键盘可达的快捷入口。
  const paletteCommands: PaletteCommand[] = [
    { id: "ask", label: "新建提问", hint: "聚焦输入框", group: "操作", run: () => { setView("console"); inputRef.current?.focus(); } },
    { id: "doctor", label: "只读体检", hint: selected ? selected.name : "需选择服务器", group: "操作", run: () => { if (selectedServerId && !running) runDoctor(); } },
    { id: "audit", label: "打开审计", group: "导航", run: () => setView("audit") },
    { id: "settings", label: "打开设置", group: "导航", run: () => setView("settings") },
    { id: "theme", label: "切换浅色/深色", group: "界面", run: () => toggleTheme() },
    { id: "terminal", label: "切换终端", group: "界面", run: () => setTerminalOpen((o) => !o) },
    { id: "readonly", label: readOnlyMode ? "关闭只读优先" : "开启只读优先", group: "界面", run: () => setReadOnly((v) => !v) },
    ...servers.map((s) => ({ id: `srv-${s.id}`, label: `切换到 ${s.name}`, hint: s.host, group: "服务器", run: () => setSelectedServerId(s.id) })),
  ];

  // 全局快捷键：⌘K / Ctrl-K 打开命令面板。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const planExecuted = !!current && current.kind === "plan" && current.executions.length > 0;
  const topTitle = current ? current.title : selected ? selected.name : "AiPanel";

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-bg text-fg" style={{ fontFamily: "var(--font-sans)" }}>
      {/* 侧栏 */}
      <aside className="flex w-64 flex-none flex-col border-r border-border bg-surface-2">
        <div className="flex items-center gap-2.5 px-3.5 py-3">
          <span className="flex h-7 w-7 items-center justify-center rounded-md bg-brand font-mono text-sm text-brand-fg">›_</span>
          <span className="text-[13.5px] font-semibold">AiPanel</span>
        </div>
        <div className="flex flex-col gap-px px-2 pb-1">
          <NavItem icon={<Pencil />} label="提问" active={view === "console"} onClick={() => setView("console")} />
          <NavItem icon={<ListIcon />} label="审计" active={view === "audit"} onClick={openAudit} />
        </div>

        <div className="cx-scroll min-h-0 flex-1 overflow-y-auto px-2 py-1.5">
          <div className="flex items-center justify-between px-2.5 pb-1 pt-2">
            <span className="text-[11.5px] text-fg-subtle">服务器</span>
            <IconButton aria-label="添加服务器" size="sm" onClick={() => setAddOpen(true)}>
              <Plus size={13} />
            </IconButton>
          </div>

          {servers.length > 3 && (
            <input
              value={serverQuery}
              onChange={(e) => setServerQuery(e.target.value)}
              placeholder="搜索服务器…"
              className="mb-1 w-full rounded-md border border-border bg-surface-1 px-2.5 py-1 text-[12.5px] outline-none placeholder:text-fg-subtle focus-visible:border-brand"
            />
          )}

          {servers.length === 0 ? (
            <div className="px-2.5 py-2 text-[12.5px] text-fg-subtle">还没有服务器,点 ＋ 添加</div>
          ) : (
            filteredServers.map((srv) => {
              const isSel = srv.id === selectedServerId;
              return (
                <div key={srv.id} className="mt-0.5">
                  <div
                    onClick={() => setSelectedServerId(srv.id)}
                    className={`group flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] transition-colors hover:bg-hover ${isSel ? "" : "text-fg-muted"}`}
                  >
                    <ServerIcon />
                    <span className="flex-1 truncate">{srv.name}</span>
                    <IconButton aria-label="编辑服务器" size="sm" className="opacity-0 transition-opacity group-hover:opacity-100" onClick={(e) => { e.stopPropagation(); setEditing(srv); }}>
                      <Pencil size={12} />
                    </IconButton>
                    <span className={`h-1.5 w-1.5 rounded-full ${statusDot(srv.status)}`} />
                  </div>
                  {isSel && (
                    <div className="flex flex-col gap-px pl-3.5">
                      {tasks.length === 0 ? (
                        <div className="px-2.5 py-1 text-[11.5px] text-fg-subtle">暂无运行记录</div>
                      ) : (
                        tasks.map((t) => (
                          <div
                            key={t.id}
                            onClick={() => openTask(t)}
                            className={`group flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-[13px] transition-colors ${current?.id === t.id ? "bg-selected" : "text-fg-muted hover:bg-hover"}`}
                          >
                            <span className="flex-none text-[11px] text-fg-subtle" title={t.kind}>{KIND_GLYPH[t.kind] ?? "❯"}</span>
                            <span className="min-w-0 flex-1 truncate">{t.title}</span>
                            <IconButton aria-label="删除记录" size="sm" className="opacity-0 transition-opacity group-hover:opacity-100" onClick={async (e) => { e.stopPropagation(); if (currentRef.current?.id === t.id) { cancelBackend(); setRunning(false); } await deleteTask(t.id); if (current?.id === t.id) setCurrent(null); await refreshTasks(); }}>
                              <Cross size={11} />
                            </IconButton>
                          </div>
                        ))
                      )}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>

        <div className="border-t border-border px-2 py-1.5">
          <NavItem icon={<Gear />} label="设置" active={view === "settings"} onClick={() => setView("settings")} />
        </div>
      </aside>

      {/* 主区 */}
      <main className="flex min-w-0 flex-1 flex-col bg-bg">
        <div className="flex h-10 flex-none items-center justify-between border-b border-border px-3.5">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-[13.5px] font-semibold">{topTitle}</span>
          </div>
          <div className="flex items-center gap-0.5">
            <IconButton aria-label="切换主题" onClick={toggleTheme} size="lg" title={theme === "light" ? "切到深色" : "切到浅色"}>
              <ThemeIcon />
            </IconButton>
            <IconButton aria-label="切换终端" onClick={() => setTerminalOpen((o) => !o)} size="lg">
              <TerminalIcon />
            </IconButton>
          </div>
        </div>

        {view === "audit" ? (
          <AuditView onNotify={push} />
        ) : view === "settings" ? (
          <SettingsPanel />
        ) : servers.length === 0 ? (
          <FirstRun onAdd={() => setAddOpen(true)} />
        ) : (
          <>
            <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
              <div className="mx-auto max-w-[680px] px-6 pb-3 pt-5">
                {!aiProvider && (
                  <div className="mb-3 flex items-center gap-2 rounded-md border border-risk-medium/40 bg-risk-medium-soft px-3 py-2 text-[12.5px] text-risk-medium">
                    <span className="flex-1">未配置 AI 供应商 — 生成计划将使用本地规则引擎,AI 诊断不可用。</span>
                    <button className="font-medium underline" onClick={() => setView("settings")}>去配置</button>
                  </div>
                )}

                {current?.plan ? (
                  <>
                    <div className="flex items-start gap-3 rounded-md border border-border bg-surface-1 px-4 py-3.5">
                      <div className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-md bg-hover text-fg-muted"><ListIcon size={16} /></div>
                      <div className="min-w-0 flex-1">
                        <div className="text-sm font-semibold">执行计划 · {current.plan.steps.length} 个步骤</div>
                        <div className="mt-1 text-[12.5px] text-fg-muted">{current.plan.goal}</div>
                      </div>
                      <div className="flex flex-none items-center gap-1.5">
                        {current.executions.length > 0 && <Button variant="ghost" size="sm" onClick={() => setTerminalOpen(true)}>查看输出</Button>}
                        {running ? (
                          <Button variant="secondary" size="sm" onClick={stop}>停止</Button>
                        ) : planExecuted ? (
                          <Button variant="secondary" size="sm" onClick={startExecute}>重新执行</Button>
                        ) : (
                          <Button variant="primary" size="sm" onClick={startExecute}><Play /> 确认执行</Button>
                        )}
                      </div>
                    </div>
                    <div className="mt-3.5 flex flex-col gap-2.5">
                      {current.plan.steps.map((s, i) => (
                        <StepRow key={i} summary={s.summary} command={s.command} risk={s.risk} status={stepStatus[i] ?? "pending"} />
                      ))}
                    </div>
                  </>
                ) : current ? (
                  <div className="rounded-md border border-border bg-surface-1 px-4 py-4">
                    <div className="mb-2 flex items-center gap-2">
                      <span className={`h-1.5 w-1.5 rounded-full ${current.status === "completed" ? "bg-risk-low" : current.status === "failed" ? "bg-risk-blocked" : "bg-fg-subtle"}`} />
                      <span className="text-sm font-semibold">{current.kind === "diagnose" ? "✦ " : ""}{current.title}</span>
                      <span className="ml-auto text-[11.5px] text-fg-subtle">{STATUS_LABEL[current.status] ?? current.status}</span>
                    </div>
                    {/* AI 诊断：展示结构化的调查过程（工具轨迹）+ 结论 */}
                    {current.kind === "diagnose" && current.toolCalls && current.toolCalls.length > 0 && (
                      <div className="mb-3 flex flex-col gap-1.5">
                        <span className="text-[11.5px] text-fg-subtle">调查过程 · {current.toolCalls.length} 次工具调用</span>
                        {current.toolCalls.map((t, i) => (
                          <div key={i} className="rounded-md border border-border bg-bg px-2.5 py-1.5">
                            <div className="flex items-center gap-2 text-[12.5px]">
                              <span className={t.ok ? "text-risk-low" : "text-risk-blocked"}>{t.ok ? "✓" : "✗"}</span>
                              <span className="font-mono">{t.name}</span>
                              {t.argsSummary ? <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-fg-subtle">{t.argsSummary}</span> : null}
                            </div>
                            {(t.error || t.resultPreview) ? (
                              <pre className={`mt-1 overflow-x-auto whitespace-pre-wrap font-mono text-[11px] leading-relaxed ${t.error ? "text-risk-blocked" : "text-fg-muted"}`}>
                                {(t.error ?? t.resultPreview ?? "").split("\n").slice(0, 8).join("\n")}
                              </pre>
                            ) : null}
                          </div>
                        ))}
                      </div>
                    )}
                    {current.summary ? (
                      <p className="whitespace-pre-wrap text-[13px] leading-relaxed text-fg">{current.summary}</p>
                    ) : <p className="text-[13px] text-fg-subtle">无总结</p>}
                  </div>
                ) : (
                  <ServerOverview server={selected} running={running} onDoctor={runDoctor} />
                )}
              </div>
            </section>

            {/* 输入区 */}
            <div className="flex-none bg-bg px-6 pb-3.5 pt-1.5">
              <div className="mx-auto max-w-[680px] rounded-lg border border-border-strong bg-surface-1 px-3 pb-2.5 pl-4 pt-3 shadow-sm">
                <input
                  ref={inputRef}
                  value={intentValue}
                  onChange={(e) => setIntentValue(e.target.value)}
                  onKeyDown={(e) => { if (running) return; if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); generatePlan(); } }}
                  placeholder={running ? "运行中…" : selectedServerId ? "描述运维任务,例如「检查网站为什么打不开」" : "先选择左侧服务器"}
                  disabled={!selectedServerId || running}
                  className="w-full border-none bg-transparent pb-2.5 pt-0.5 text-sm outline-none placeholder:text-fg-subtle disabled:opacity-50"
                />
                <div className="flex items-center justify-between gap-2.5">
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setReadOnly((v) => !v)}
                      title="开启后,生成的写操作步骤会被阻止"
                      className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] transition-colors hover:bg-hover ${readOnlyMode ? "text-risk-medium" : "text-fg-subtle"}`}
                    >
                      {readOnlyMode ? "🔒 只读优先 · 开" : "只读优先 · 关"}
                    </button>
                    <button onClick={diagnose} disabled={!intentValue.trim() || !selectedServerId || running} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg disabled:opacity-40">
                      ✦ AI 诊断
                    </button>
                  </div>
                  <div className="flex items-center gap-2.5">
                    <button onClick={() => setView("settings")} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[12.5px] text-fg-muted transition-colors hover:bg-hover" title="模型供应商设置">
                      {aiProvider ? `${aiProvider.name}${aiProvider.model ? " · " + aiProvider.model : ""}` : "未配置模型 · 去设置"}
                    </button>
                    <button aria-label="发送" onClick={generatePlan} className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-full bg-brand text-brand-fg transition-opacity hover:opacity-90 disabled:opacity-40" disabled={!intentValue.trim() || !selectedServerId || running}>
                      <SendArrow />
                    </button>
                  </div>
                </div>
              </div>
            </div>

            {terminalOpen && (
              <Terminal
                host={selected?.name ?? "—"}
                live={running}
                lines={termLines.length ? termLines : [{ text: "终端输出会显示在这里。", tone: "muted" }]}
                cursor={running}
              />
            )}
          </>
        )}
      </main>

      <AddServerDialog open={addOpen} onClose={() => setAddOpen(false)} onCreated={(s) => { setServers((prev) => [...prev, s]); setSelectedServerId(s.id); push("success", `服务器「${s.name}」已添加`); }} />
      <EditServerDialog
        open={editing !== null}
        server={editing}
        onClose={() => setEditing(null)}
        onSaved={(u) => { setServers((prev) => prev.map((s) => (s.id === u.id ? u : s))); push("success", "服务器已保存"); }}
        onDeleted={(id) => { setServers((prev) => prev.filter((s) => s.id !== id)); setSelectedServerId((cur) => (cur === id ? null : cur)); push("success", "服务器已删除"); }}
      />
      <ConfirmExecuteDialog open={confirmOpen} plan={current?.plan ?? null} review={confirmReview} onClose={() => setConfirmOpen(false)} onConfirm={(c, d) => execute(c, d, confirmReview ?? undefined)} />

      {/* 命令面板（⌘K）：可搜索/键盘可达的快捷操作 */}
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} commands={paletteCommands} />

      {/* 全局通知层（错误/成功提示），固定在右下角 */}
      <ToastViewport toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

/* ---------------- 空态 / 首页 ---------------- */
// 首次使用引导：尚未添加任何服务器时的空状态。
function FirstRun({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center p-8">
      <div className="max-w-md text-center">
        <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-surface-2 text-fg-muted"><ServerIcon size={22} /></div>
        <h2 className="text-base font-semibold">还没有连接任何服务器</h2>
        <p className="mt-2 text-[13px] leading-relaxed text-fg-muted">
          AiPanel 在本地运行、通过 SSH 管理服务器,不在服务器上常驻。添加一台服务器即可开始只读体检与 AI 运维。凭据只存本地 Keychain。
        </p>
        <div className="mt-5"><Button variant="primary" size="md" onClick={onAdd}><Plus /> 添加第一台服务器</Button></div>
      </div>
    </div>
  );
}
