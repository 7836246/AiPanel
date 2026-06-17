import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  Button,
  IconButton,
  Spinner,
  Terminal,
  type TerminalLine,
} from "@aipanel/ui";
import AddServerDialog from "./AddServerDialog";
import ConfirmExecuteDialog from "./ConfirmExecuteDialog";
import EditServerDialog from "./EditServerDialog";
import SettingsPanel from "./SettingsPanel";
import {
  createPlan,
  reviewPlan,
  runAgentTurn,
  runConfirmedPlanStream,
  runServerDoctorStream,
  serverDoctorPlan,
  listAuditRecords,
  listProviders,
  listServers,
  listTasks,
  saveTask,
  deleteTask,
  RISK_META,
  type AppError,
  type AuditRecord,
  type CommandExecution,
  type ProviderConfig,
  type RiskLevel,
  type RiskReview,
  type ServerProfile,
  type ServerStatus,
  type TaskRecord,
} from "../lib/api";
import "./codex-console.css";

const errMsg = (e: unknown): string =>
  e && typeof e === "object" && "message" in e ? (e as AppError).message : String(e);

const nowIso = () => new Date().toISOString();
const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `t-${Date.now()}-${Math.random().toString(16).slice(2)}`;

const statusDot = (s: ServerStatus): string =>
  s === "online" ? "bg-risk-low" : s === "offline" ? "bg-risk-blocked" : "bg-fg-subtle";

type StepStatus = "pending" | "running" | "done" | "failed";

/* ---------------- icons ---------------- */
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
const Stethoscope = ({ size = 14 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke} strokeWidth={1.4}>
    <path d="M4 2v4a3 3 0 0 0 6 0V2" />
    <path d="M7 9v1.5a3.5 3.5 0 0 0 7 0V9" />
    <circle cx="13.5" cy="8" r="1.2" />
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

/* ---------------- theme ---------------- */
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

/* ---------------- sub-components ---------------- */
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

const STATUS_LABEL: Record<string, string> = {
  completed: "完成", failed: "失败", blocked: "已阻止", running: "进行中",
  awaiting_confirmation: "待确认", planning: "规划中", pending: "待处理",
};

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
            onClick={() => {
              navigator.clipboard?.writeText(command);
              setCopied(true);
              setTimeout(() => setCopied(false), 1200);
            }}
          >
            {copied ? <Check size={12} /> : <Copy />}
          </IconButton>
        </div>
      </div>
    </div>
  );
}

function AuditPanel({ records }: { records: AuditRecord[] }) {
  const [openId, setOpenId] = useState<string | null>(null);
  return (
    <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[680px] px-6 pb-6 pt-5">
        <h2 className="mb-3 text-sm font-semibold">审计记录</h2>
        {records.length === 0 ? (
          <div className="rounded-md border border-border bg-surface-1 px-4 py-6 text-center text-[13px] text-fg-subtle">
            还没有审计记录。执行一次任务后会出现在这里。
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {records.map((r) => {
              const open = r.id === openId;
              const ok = r.status === "completed";
              return (
                <div key={r.id} className="overflow-hidden rounded-md border border-border bg-surface-1">
                  <div onClick={() => setOpenId(open ? null : r.id)} className="flex cursor-pointer items-center gap-3 px-4 py-3 transition-colors hover:bg-hover">
                    <span className={`h-1.5 w-1.5 rounded-full ${ok ? "bg-risk-low" : "bg-risk-blocked"}`} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13.5px] font-medium">{r.intent}</div>
                      {r.summary ? <div className="truncate text-[12px] text-fg-muted">{r.summary}</div> : null}
                    </div>
                    <span className="flex-none text-[11.5px] text-fg-subtle">{STATUS_LABEL[r.status] ?? r.status}</span>
                    <time className="flex-none font-mono text-[11px] text-fg-subtle">{new Date(r.createdAt).toLocaleString()}</time>
                  </div>
                  {open && (
                    <div className="border-t border-border px-4 py-3">
                      {r.executions.length === 0 ? (
                        <div className="text-[12px] text-fg-subtle">无命令执行记录</div>
                      ) : (
                        <div className="flex flex-col gap-2">
                          {r.executions.map((ex, i) => (
                            <div key={i} className="rounded-md bg-bg">
                              <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 font-mono text-[11.5px] text-fg-subtle">
                                <span>$ {ex.command}</span>
                                <span className={`ml-auto ${ex.exitCode === 0 ? "text-risk-low" : "text-risk-blocked"}`}>exit {ex.exitCode}</span>
                              </div>
                              {ex.stdout || ex.stderr ? (
                                <pre className="overflow-x-auto px-3 py-2 font-mono text-[11.5px] leading-relaxed text-fg">
                                  {(ex.stdout || ex.stderr).split("\n").slice(0, 12).join("\n")}
                                </pre>
                              ) : null}
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}

/* ---------------- screen ---------------- */
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
  const [readOnlyMode, setReadOnlyMode] = useState(false);
  const [intentValue, setIntentValue] = useState("");
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [audits, setAudits] = useState<AuditRecord[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [editing, setEditing] = useState<ServerProfile | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmReview, setConfirmReview] = useState<RiskReview | null>(null);
  const runIdRef = useRef(0);

  const selected = servers.find((s) => s.id === selectedServerId) ?? null;
  const aiProvider = providers.find((p) => p.enabled && p.kind !== "custom") ?? null;
  const filteredServers = servers.filter(
    (s) => !serverQuery || s.name.toLowerCase().includes(serverQuery.toLowerCase()) || s.host.includes(serverQuery)
  );

  useEffect(() => {
    listServers().then((s) => { setServers(s); setSelectedServerId((cur) => cur ?? s[0]?.id ?? null); }).catch(() => {});
    listProviders().then(setProviders).catch(() => {});
  }, []);

  // Load run history for the selected server; reset the open run.
  useEffect(() => {
    setCurrent(null);
    setStepStatus([]);
    setTermLines([]);
    if (!selectedServerId) { setTasks([]); return; }
    listTasks(selectedServerId).then(setTasks).catch(() => setTasks([]));
  }, [selectedServerId]);

  async function refreshTasks() {
    if (selectedServerId) setTasks(await listTasks(selectedServerId).catch(() => []));
  }

  function openTask(t: TaskRecord) {
    setCurrent(t);
    setStepStatus((t.plan?.steps ?? []).map((_, i) => (t.executions[i] ? (t.executions[i].exitCode === 0 ? "done" : "failed") : "pending")));
    setTermLines(execLines(t.executions, t.summary));
    setView("console");
  }

  function execLines(executions: CommandExecution[], summary?: string): TerminalLine[] {
    const lines: TerminalLine[] = [];
    for (const ex of executions) {
      lines.push({ text: `$ ${ex.command}`, tone: "prompt" });
      for (const l of (ex.stdout || ex.stderr).split("\n")) if (l.trim()) lines.push({ text: l, tone: ex.exitCode === 0 ? "default" : "danger" });
    }
    if (summary) { lines.push({ text: "", tone: "muted" }); lines.push({ text: summary, tone: "muted" }); }
    return lines;
  }

  // ----- actions -----
  async function generatePlan() {
    const intent = intentValue.trim();
    if (!intent || !selectedServerId) return;
    setRunning(true);
    setTermLines([{ text: aiProvider ? "AI 规划中…" : "生成计划中(本地规则)…", tone: "muted" }]);
    try {
      const plan = await createPlan(intent, selectedServerId);
      const task: TaskRecord = {
        id: newId(), serverId: selectedServerId, title: plan.goal, intent, kind: "plan",
        plan, executions: [], status: "awaiting_confirmation", createdAt: nowIso(), updatedAt: nowIso(),
      };
      await saveTask(task);
      setCurrent(task);
      setStepStatus(plan.steps.map(() => "pending"));
      setTermLines([]);
      setIntentValue("");
      await refreshTasks();
    } catch (e) {
      setTermLines([{ text: `生成计划失败: ${errMsg(e)}`, tone: "danger" }]);
      setTerminalOpen(true);
    } finally {
      setRunning(false);
    }
  }

  async function diagnose() {
    const intent = intentValue.trim();
    if (!intent || !selectedServerId) return;
    if (!aiProvider) { setView("settings"); return; }
    setRunning(true);
    setTerminalOpen(true);
    setCurrent(null);
    setTermLines([{ text: "AI 诊断中…", tone: "muted" }]);
    try {
      const r = await runAgentTurn(intent, selectedServerId);
      const lines: TerminalLine[] = r.toolCalls.map((t) => ({ text: `▸ ${t.name} ${t.ok ? "✓" : "✗"}`, tone: t.ok ? "success" : "danger" }));
      if (lines.length) lines.push({ text: "", tone: "muted" });
      for (const l of r.summary.split("\n")) lines.push({ text: l });
      setTermLines(lines);
      const task: TaskRecord = {
        id: newId(), serverId: selectedServerId, title: intent, intent, kind: "diagnose",
        executions: [], summary: r.summary, status: "completed", createdAt: nowIso(), updatedAt: nowIso(),
      };
      await saveTask(task);
      setCurrent(task);
      setIntentValue("");
      await refreshTasks();
    } catch (e) {
      setTermLines([{ text: `诊断失败: ${errMsg(e)}`, tone: "danger" }]);
    } finally {
      setRunning(false);
    }
  }

  async function runDoctor() {
    if (!selectedServerId) return;
    const myId = ++runIdRef.current;
    setRunning(true);
    setTerminalOpen(true);
    const plan = await serverDoctorPlan(selectedServerId).catch(() => null);
    const task: TaskRecord = {
      id: newId(), serverId: selectedServerId, title: "只读服务器体检", intent: "只读服务器体检",
      kind: "doctor", plan: plan ?? undefined, executions: [], status: "running", createdAt: nowIso(), updatedAt: nowIso(),
    };
    setCurrent(task);
    setStepStatus((plan?.steps ?? []).map(() => "pending"));
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
      });
      if (runIdRef.current !== myId) return;
      const ok = report.executions.some((e) => e.exitCode === 0);
      const summary = report.warnings.length ? `${report.warnings.length} 条告警` : "体检完成,无告警";
      const done: TaskRecord = { ...task, executions: report.executions, summary, status: ok ? "completed" : "failed", updatedAt: nowIso() };
      await saveTask(done);
      setCurrent(done);
      setServers(await listServers());
      await refreshTasks();
    } catch (e) {
      if (runIdRef.current !== myId) return;
      lines.push({ text: `体检失败: ${errMsg(e)}`, tone: "danger" });
      setTermLines([...lines]);
      await saveTask({ ...task, status: "failed", summary: errMsg(e), updatedAt: nowIso() });
      await refreshTasks();
    } finally {
      if (runIdRef.current === myId) setRunning(false);
    }
  }

  // Review a generated plan, then confirm (or execute directly if purely read-only).
  async function startExecute() {
    if (!current?.plan) return;
    try {
      const review = await reviewPlan(current.plan, readOnlyMode);
      if (!review.blocked && !review.requiresConfirmation) { await execute(true, false, review); return; }
      setConfirmReview(review);
      setConfirmOpen(true);
    } catch (e) {
      setTermLines([{ text: `风险审查失败: ${errMsg(e)}`, tone: "danger" }]);
      setTerminalOpen(true);
    }
  }

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
    try {
      const rec = await runConfirmedPlanStream(plan, { confirmed, doubleConfirmed, readOnlyMode }, (ev) => {
        if (runIdRef.current !== myId) return;
        if (ev.type === "step") {
          setStepStatus((prev) => { const n = [...prev]; n[ev.index] = ev.status === "running" ? "running" : ev.status; return n; });
        } else if (ev.type === "line") {
          lines.push({ text: ev.text, tone: ev.stderr ? "danger" : "default" });
          setTermLines([...lines]);
        }
      });
      if (runIdRef.current !== myId) return;
      const done: TaskRecord = {
        ...current, plan, riskReview: review ?? current.riskReview, executions: rec.executions,
        summary: rec.summary, status: rec.status, updatedAt: nowIso(),
      };
      await saveTask(done);
      setCurrent(done);
      setTermLines(execLines(rec.executions, rec.summary));
      setServers(await listServers());
      await refreshTasks();
    } catch (e) {
      if (runIdRef.current !== myId) return;
      lines.push({ text: `执行失败: ${errMsg(e)}`, tone: "danger" });
      setTermLines([...lines]);
    } finally {
      if (runIdRef.current === myId) setRunning(false);
    }
  }

  function stop() {
    runIdRef.current += 1;
    setRunning(false);
  }

  function openAudit() {
    setView("audit");
    listAuditRecords().then(setAudits).catch(() => setAudits([]));
  }

  const planExecuted = !!current && current.kind === "plan" && current.executions.length > 0;
  const topTitle = current ? current.title : selected ? selected.name : "AiPanel";

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-bg text-fg" style={{ fontFamily: "var(--font-sans)" }}>
      {/* sidebar */}
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
                            <span className="min-w-0 flex-1 truncate">{t.title}</span>
                            <IconButton aria-label="删除记录" size="sm" className="opacity-0 transition-opacity group-hover:opacity-100" onClick={async (e) => { e.stopPropagation(); await deleteTask(t.id); if (current?.id === t.id) setCurrent(null); await refreshTasks(); }}>
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

      {/* main */}
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
          <AuditPanel records={audits} />
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
                      <span className="text-sm font-semibold">{current.title}</span>
                      <span className="ml-auto text-[11.5px] text-fg-subtle">{STATUS_LABEL[current.status] ?? current.status}</span>
                    </div>
                    {current.summary ? (
                      <p className="whitespace-pre-wrap text-[13px] leading-relaxed text-fg">{current.summary}</p>
                    ) : <p className="text-[13px] text-fg-subtle">无总结</p>}
                  </div>
                ) : (
                  <ServerHome server={selected} running={running} onDoctor={runDoctor} />
                )}
              </div>
            </section>

            {/* composer */}
            <div className="flex-none bg-bg px-6 pb-3.5 pt-1.5">
              <div className="mx-auto max-w-[680px] rounded-lg border border-border-strong bg-surface-1 px-3 pb-2.5 pl-4 pt-3 shadow-sm">
                <input
                  value={intentValue}
                  onChange={(e) => setIntentValue(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); generatePlan(); } }}
                  placeholder={selectedServerId ? "描述运维任务,例如「检查网站为什么打不开」" : "先选择左侧服务器"}
                  disabled={!selectedServerId}
                  className="w-full border-none bg-transparent pb-2.5 pt-0.5 text-sm outline-none placeholder:text-fg-subtle disabled:opacity-50"
                />
                <div className="flex items-center justify-between gap-2.5">
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setReadOnlyMode((v) => !v)}
                      title="开启后,生成的写操作步骤会被阻止"
                      className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] transition-colors hover:bg-hover ${readOnlyMode ? "text-risk-medium" : "text-fg-subtle"}`}
                    >
                      {readOnlyMode ? "🔒 只读优先 · 开" : "只读优先 · 关"}
                    </button>
                    <button onClick={diagnose} disabled={!intentValue.trim() || !selectedServerId} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg disabled:opacity-40">
                      ✦ AI 诊断
                    </button>
                  </div>
                  <div className="flex items-center gap-2.5">
                    <button onClick={() => setView("settings")} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[12.5px] text-fg-muted transition-colors hover:bg-hover" title="模型供应商设置">
                      {aiProvider ? `${aiProvider.name}${aiProvider.model ? " · " + aiProvider.model : ""}` : "未配置模型 ▾"}
                    </button>
                    <button aria-label="发送" onClick={generatePlan} className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-full bg-brand text-brand-fg transition-opacity hover:opacity-90 disabled:opacity-40" disabled={!intentValue.trim() || !selectedServerId}>
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

      <AddServerDialog open={addOpen} onClose={() => setAddOpen(false)} onCreated={(s) => { setServers((prev) => [...prev, s]); setSelectedServerId(s.id); }} />
      <EditServerDialog
        open={editing !== null}
        server={editing}
        onClose={() => setEditing(null)}
        onSaved={(u) => setServers((prev) => prev.map((s) => (s.id === u.id ? u : s)))}
        onDeleted={(id) => { setServers((prev) => prev.filter((s) => s.id !== id)); setSelectedServerId((cur) => (cur === id ? null : cur)); }}
      />
      <ConfirmExecuteDialog open={confirmOpen} plan={current?.plan ?? null} review={confirmReview} onClose={() => setConfirmOpen(false)} onConfirm={(c, d) => execute(c, d, confirmReview ?? undefined)} />
    </div>
  );
}

/* ---------------- empty / home states ---------------- */
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

function ServerHome({ server, running, onDoctor }: { server: ServerProfile | null; running: boolean; onDoctor: () => void }) {
  if (!server) {
    return <div className="rounded-md border border-border bg-surface-1 px-4 py-8 text-center text-[13px] text-fg-subtle">从左侧选择一台服务器开始。</div>;
  }
  const facts = Object.entries(server.facts ?? {});
  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-md border border-border bg-surface-1 px-4 py-3.5">
        <div className="flex items-center gap-2">
          <span className={`h-1.5 w-1.5 rounded-full ${statusDot(server.status)}`} />
          <span className="text-sm font-semibold">{server.name}</span>
          <span className="font-mono text-[12px] text-fg-subtle">{server.username}@{server.host}:{server.port}</span>
          <Button variant="secondary" size="sm" className="ml-auto" onClick={onDoctor} disabled={running}>
            {running ? <Spinner size="sm" /> : <Stethoscope />} 只读体检
          </Button>
        </div>
        {facts.length > 0 && (
          <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1.5">
            {facts.map(([k, v]) => (
              <div key={k} className="flex items-baseline justify-between gap-2 text-[12.5px]">
                <dt className="text-fg-subtle">{k}</dt>
                <dd className="truncate font-medium text-fg">{v}</dd>
              </div>
            ))}
          </dl>
        )}
      </div>
      <div className="rounded-md border border-dashed border-border px-4 py-6 text-center text-[13px] text-fg-subtle">
        在下方输入运维任务生成可审查的计划,或点「只读体检」做一次安全检查。
      </div>
    </div>
  );
}
