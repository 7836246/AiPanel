import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from "react";
import {
  ArrowUp,
  Check as CheckIcon,
  ChevronDown,
  ChevronUp,
  Boxes,
  ClipboardList,
  Copy as CopyIcon,
  FolderTree,
  LayoutGrid,
  MoreHorizontal,
  PanelBottom,
  PanelLeft,
  PanelRight,
  Lock,
  LockOpen,
  Moon,
  Pencil as PencilIcon,
  Play as PlayIcon,
  Plus as PlusIcon,
  RefreshCw,
  ScrollText,
  Server as ServerIconLucide,
  Settings as SettingsIcon,
  Sparkles,
  Square,
  Star,
  Stethoscope,
  Sun,
  Terminal as TerminalIconLucide,
  X as XIcon,
} from "lucide-react";
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
import Dashboard from "./Dashboard";
import ServerOverview from "./ServerOverview";
import FileBrowser from "./FileBrowser";
import DockerDeployPanel from "./DockerDeployPanel";
import TerminalSession from "./TerminalSession";
import CommandPalette, { type PaletteCommand } from "./CommandPalette";
import {
  isTauri,
  cancelRun,
  checkSshConnection,
  createPlan,
  getModelSelectionPolicy,
  setServerFavorite,
  refreshAllServers,
  reviewPlan,
  runAgentTurn,
  runConfirmedPlanStream,
  runServerDoctorStream,
  serverDoctorPlan,
  listProviders,
  listModels,
  setProviderModel,
  listServers,
  listTasks,
  saveTask,
  deleteTask,
  deleteServer,
  RISK_META,
  type AppError,
  type CommandExecution,
  type ModelSelectionPolicy,
  type Plan,
  type PlanStep,
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
// 右键菜单的单项。
type CtxItem = { label: string; onClick: () => void; danger?: boolean };

/** 轻量右键上下文菜单:在光标处弹出,点空白/Esc 关闭。 */
function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: CtxItem[]; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  // 防止菜单超出视口右/下边界。
  const left = Math.min(x, window.innerWidth - 200);
  const top = Math.min(y, window.innerHeight - (items.length * 34 + 12));
  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }} />
      <div
        className="fixed z-50 min-w-[176px] rounded-md border border-border bg-surface-1 py-1 shadow-lg"
        style={{ left, top }}
      >
        {items.map((it, i) => (
          <button
            key={i}
            onClick={() => { onClose(); it.onClick(); }}
            className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] transition-colors hover:bg-hover ${it.danger ? "text-risk-blocked" : "text-fg-muted hover:text-fg"}`}
          >
            {it.label}
          </button>
        ))}
      </div>
    </>
  );
}

function NavItem({ icon, label, kbd, active, onClick, badge }: {
  icon: ReactNode; label: string; kbd?: string; active?: boolean; onClick?: () => void; badge?: number;
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
      {badge && badge > 0 ? (
        <span className="flex-none rounded-full bg-risk-blocked px-1.5 text-[10px] font-semibold leading-[1.4] text-white" title={`${badge} 台需关注`}>
          {badge}
        </span>
      ) : kbd ? <span className="text-[11.5px] text-fg-subtle">{kbd}</span> : null}
    </div>
  );
}

/**
 * 首页模型选择器:展示当前激活模型,点开后自动探测 {base}/models 列表,选中即生效
 * (set_provider_model 持久化 + onChanged 刷新)。未配置供应商时引导去设置。
 */
function ModelPicker({
  provider,
  onChanged,
  onConfigure,
}: {
  provider: ProviderConfig | null;
  onChanged: () => void;
  onConfigure: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 首次激活某供应商且尚未设过模型时,自动探测并选中首个模型——配置完 base+key 即可用,无需手点。
  useEffect(() => {
    if (!provider || provider.model) return;
    let cancelled = false;
    void (async () => {
      try {
        const list = await listModels(provider);
        if (cancelled) return;
        setModels(list);
        if (list.length) {
          await setProviderModel(provider.id, list[0]);
          if (!cancelled) onChanged();
        }
      } catch {
        /* 自动探测失败:忽略,用户可手动点开探测 */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [provider?.id]);

  if (!provider) {
    return (
      <button onClick={onConfigure} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[12.5px] text-fg-muted transition-colors hover:bg-hover" title="模型供应商设置">
        未配置模型 · 去设置
      </button>
    );
  }

  const probe = async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await listModels(provider);
      setModels(list);
      if (!list.length) setError("未探测到模型");
    } catch (e) {
      setError(e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e));
    } finally {
      setLoading(false);
    }
  };

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next && models.length === 0) void probe();
  };

  const pick = async (m: string) => {
    setOpen(false);
    if (m === provider.model) return;
    try {
      await setProviderModel(provider.id, m);
      onChanged();
    } catch (e) {
      // 持久化失败:重新打开下拉并显示原因,避免「点了没反应」。
      setError(e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e));
      setOpen(true);
    }
  };

  return (
    <div className="relative">
      <button onClick={toggle} className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[12.5px] text-fg-muted transition-colors hover:bg-hover" title="选择模型(点开自动探测)">
        <span className="max-w-[180px] truncate">{provider.model ? `${provider.name} · ${provider.model}` : `${provider.name} · 选择模型`}</span>
        <ChevronDown size={13} className="flex-none" />
      </button>
      {open && (
        <>
          {/* 点击空白关闭 */}
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute bottom-full right-0 z-20 mb-1 max-h-72 w-60 overflow-y-auto rounded-md border border-border bg-surface-1 py-1 shadow-lg">
            <div className="flex items-center justify-between px-2.5 py-1 text-[11px] text-fg-subtle">
              <span>模型</span>
              <button onClick={() => void probe()} disabled={loading} className="inline-flex items-center gap-1 rounded px-1 hover:bg-hover disabled:opacity-50" title="重新探测">
                {loading ? <Spinner size="sm" /> : <RefreshCw size={11} />} 探测
              </button>
            </div>
            {error && <div className="px-2.5 py-1.5 text-[12px] text-risk-blocked">{error}</div>}
            {!loading && !error && models.length === 0 && (
              <div className="px-2.5 py-1.5 text-[12px] text-fg-subtle">点「探测」获取模型列表</div>
            )}
            {models.map((m) => (
              <button
                key={m}
                onClick={() => void pick(m)}
                className={`flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-[12.5px] transition-colors hover:bg-hover ${m === provider.model ? "text-fg" : "text-fg-muted"}`}
              >
                {m === provider.model ? <CheckIcon size={13} className="flex-none text-brand" /> : <span className="w-[13px] flex-none" />}
                <span className="truncate">{m}</span>
              </button>
            ))}
            <div className="mt-1 border-t border-border pt-1">
              <button onClick={onConfigure} className="w-full px-2.5 py-1.5 text-left text-[12px] text-fg-subtle hover:bg-hover">供应商设置…</button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// 首页快捷运维意图:一键填入 composer,降低「不知道输入什么」的门槛(均为只读向描述)。
const QUICK_INTENTS = [
  "检查磁盘和内存使用情况",
  "列出正在运行的服务和容器",
  "看看最近的系统日志有没有报错",
  "检查网站为什么打不开",
];

// 服务器是否处于告警:离线,或上次体检的磁盘/内存使用率 >90%。
function serverAlert(s: ServerProfile): "offline" | "resource" | null {
  if (s.status === "offline") return "offline";
  for (const k of ["Disk", "Memory"]) {
    const v = s.facts?.[k];
    const m = v?.match(/(\d+(?:\.\d+)?)\s*%/);
    if (m && Number(m[1]) > 90) return "resource";
  }
  return null;
}

// 任务类型对应的侧栏小图标,便于一眼区分计划/诊断/体检。
const KIND_GLYPH: Record<string, ReactNode> = {
  plan: <ClipboardList size={13} />,
  diagnose: <Sparkles size={13} />,
  doctor: <Stethoscope size={13} />,
};

// 任务状态到中文标签的映射。
const STATUS_LABEL: Record<string, string> = {
  completed: "完成", failed: "失败", blocked: "已阻止", running: "进行中",
  awaiting_confirmation: "待确认", planning: "规划中", pending: "待处理",
};

// 计划中的一行步骤：状态图标、摘要、风险标签，以及可复制的命令。
// 编辑态回调集合；传入即进入可编辑模式。
type StepEdit = {
  reviewing?: boolean;
  onSummary: (v: string) => void;
  onCommand: (v: string) => void;
  onRemove: () => void;
  onUp?: () => void;
  onDown?: () => void;
};

function StepRow({ summary, command, risk, status, edit }: {
  summary: string; command: string; risk: RiskLevel; status: StepStatus; edit?: StepEdit;
}) {
  const meta = RISK_META[risk];
  const [copied, setCopied] = useState(false);
  return (
    <div className={`overflow-hidden rounded-md border bg-surface-1 ${edit ? "border-brand/50" : "border-border"}`}>
      <div className="flex items-center gap-2.5 px-3.5 py-3">
        {!edit && status === "done" && <CheckIcon size={14} className="text-risk-low" strokeWidth={2} />}
        {!edit && status === "failed" && <XIcon size={14} className="text-risk-blocked" strokeWidth={2} />}
        {!edit && status === "running" && <Spinner size="sm" />}
        {!edit && status === "pending" && (
          <span className="h-3.5 w-3.5 rounded-full border-[1.5px] border-border-strong" />
        )}
        {edit ? (
          <input
            value={summary}
            onChange={(e) => edit.onSummary(e.target.value)}
            placeholder="步骤摘要"
            className="min-w-0 flex-1 border-none bg-transparent text-[13.5px] font-semibold outline-none placeholder:text-fg-subtle"
          />
        ) : (
          <span className="min-w-0 flex-1 text-[13.5px] font-semibold">{summary}</span>
        )}
        <span className="inline-flex items-center gap-1.5 text-[11.5px] text-fg-muted">
          {edit?.reviewing ? <Spinner size="sm" /> : <span className={`h-1.5 w-1.5 rounded-full ${meta.dot}`} />}
          {meta.label}
        </span>
        {edit && (
          <span className="flex flex-none items-center gap-0.5">
            <button aria-label="上移" disabled={!edit.onUp} onClick={edit.onUp} className="rounded px-1 leading-none text-fg-subtle transition-colors hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60 disabled:opacity-30"><ChevronUp size={14} /></button>
            <button aria-label="下移" disabled={!edit.onDown} onClick={edit.onDown} className="rounded px-1 leading-none text-fg-subtle transition-colors hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60 disabled:opacity-30"><ChevronDown size={14} /></button>
            <IconButton aria-label="删除步骤" size="sm" onClick={edit.onRemove}><XIcon size={12} /></IconButton>
          </span>
        )}
      </div>
      <div className="px-3.5 pb-3.5">
        {edit ? (
          <textarea
            value={command}
            onChange={(e) => edit.onCommand(e.target.value)}
            placeholder="输入命令"
            rows={Math.min(5, Math.max(1, command.split("\n").length))}
            spellCheck={false}
            className="w-full resize-y rounded-md bg-hover px-3 py-2 font-mono text-xs outline-none placeholder:text-fg-subtle focus-visible:ring-1 focus-visible:ring-brand"
          />
        ) : (
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
              {copied ? <CheckIcon size={12} className="text-risk-low" strokeWidth={2} /> : <CopyIcon size={13} />}
            </IconButton>
          </div>
        )}
        {edit && risk === "blocked" && (
          <div className="mt-1.5 text-[11.5px] text-risk-blocked">该命令被风险策略阻止,需修改或删除后才能执行。</div>
        )}
      </div>
    </div>
  );
}

/* ---------------- 主屏 ---------------- */
// 主控制台：左侧服务器/历史导航，右侧计划生成、执行、体检、诊断与终端输出。
export default function CodexConsole() {
  const [theme, toggleTheme] = useTheme();
  const [view, setView] = useState<"console" | "audit" | "settings" | "dashboard" | "deploy">("console");
  const [refreshing, setRefreshing] = useState(false);
  // 计划编辑态：draftSteps 非 null 即处于编辑;draftReview 为草稿的服务端重判结果。
  const [draftSteps, setDraftSteps] = useState<PlanStep[] | null>(null);
  const [draftReview, setDraftReview] = useState<RiskReview | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [servers, setServers] = useState<ServerProfile[]>([]);
  const [selectedServerId, setSelectedServerId] = useState<string | null>(null);
  const [serverQuery, setServerQuery] = useState("");
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [current, setCurrent] = useState<TaskRecord | null>(null);
  const [stepStatus, setStepStatus] = useState<StepStatus[]>([]);
  const [termLines, setTermLines] = useState<TerminalLine[]>([]);
  const [terminalOpen, setTerminalOpen] = useState(true);
  // Codex 式可折叠左侧栏(顶栏左侧的开关控制),配合右侧停靠面板形成「一左一右」操作。
  const [sidebarOpen, setSidebarOpen] = useState(true);
  // 顶栏标题旁的「···」菜单开关(Codex 式:新建/重命名/删除当前运行)。
  const [titleMenuOpen, setTitleMenuOpen] = useState(false);
  // 右键上下文菜单(侧栏服务器/运行记录):位置 + 菜单项。
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; items: CtxItem[] } | null>(null);
  const openCtx = (e: ReactMouseEvent, items: CtxItem[]) => {
    e.preventDefault();
    e.stopPropagation();
    setCtxMenu({ x: e.clientX, y: e.clientY, items });
  };
  // Codex 式三栏停靠面板:右侧文件树、底部交互终端(各自可开关、可拖拽改尺寸)。
  const [filesOpen, setFilesOpen] = useState(false);
  const [shellOpen, setShellOpen] = useState(false);
  const [filesW, setFilesW] = useState(380);
  const [shellH, setShellH] = useState(280);
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
  // 服务器列表请求序号:用于丢弃过期的收藏/刷新结果(与 runIdRef 分开,避免误伤运行中的任务)。
  const serversReqRef = useRef(0);
  // 计划草稿重判的请求序号与防抖定时器:与 runIdRef/cancelIdRef 完全分离,绝不参与运行/取消生命周期。
  const reviewReqRef = useRef(0);
  const reviewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
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
  // 与后端 candidate_providers 一致:custom 类型不参与规划,也不应作为首页激活供应商。
  const policyDefault = policy.defaultProviderId
    ? providers.find((p) => p.id === policy.defaultProviderId && p.enabled && p.kind !== "custom")
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

  // 选中服务器变化时：真正取消进行中的运行（中断远端命令），丢弃未保存的计划编辑草稿，再重置并加载历史。
  useEffect(() => {
    cancelBackend();
    if (reviewTimerRef.current) clearTimeout(reviewTimerRef.current);
    reviewReqRef.current += 1;
    setDraftSteps(null);
    setDraftReview(null);
    setReviewing(false);
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

  // 前台健康轮询:应用可见且有服务器时,每 60s 刷新一次连通状态(后台/无服务器时不跑),
  // 让概览/导航的告警计数保持实时。refreshAllServers 内有请求序号守卫,并发调用安全。
  useEffect(() => {
    if (!isTauri()) return;
    const tick = () => {
      if (document.visibilityState === "visible" && servers.length > 0) refreshAll();
    };
    const iv = setInterval(tick, 60000);
    return () => clearInterval(iv);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [servers.length]);

  // 进入「概览」时自动刷新一次所有服务器连通状态，保证在线/离线计数实时。
  useEffect(() => {
    if (view === "dashboard" && servers.length > 0) refreshAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  // 选中服务器后静默做一次 SSH 连通性检查，更新在线状态点。
  // 仅在 Tauri 下为真实探测；浏览器 mock 会返回随机值，故跳过以免误导状态点。
  useEffect(() => {
    if (!selectedServerId || !isTauri()) return;
    const id = selectedServerId;
    checkSshConnection(id)
      .then((r) =>
        setServers((prev) => prev.map((s) => (s.id === id ? { ...s, status: r.ok ? "online" : "offline" } : s)))
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
    cancelEdit(); // 丢弃未保存的计划编辑草稿
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
  // 把一个已生成的计划纳入「待确认任务」(用于 Docker 部署等直接产出 Plan 的入口),
  // 复用与 generatePlan 一致的 可编辑 → 审查 → 确认 → 执行 流程。
  async function adoptPlan(plan: Plan, title: string) {
    const serverId = selectedServerId ?? plan.serverId ?? undefined;
    const task: TaskRecord = {
      id: newId(), serverId, title, intent: title, kind: "plan",
      plan, executions: [], status: "awaiting_confirmation", createdAt: nowIso(), updatedAt: nowIso(),
    };
    await saveTask(task);
    setCurrent(task);
    setStepStatus(plan.steps.map(() => "pending"));
    setTermLines([]);
    setView("console");
    await refreshTasks();
  }

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
  // ----- 计划编辑 -----
  // 防抖触发对草稿计划的服务端重判（风险由后端判定,前端不臆断）。
  function scheduleReview(steps: PlanStep[]) {
    if (!current?.plan) return;
    const plan = current.plan;
    if (reviewTimerRef.current) clearTimeout(reviewTimerRef.current);
    const reqId = ++reviewReqRef.current;
    setReviewing(true);
    reviewTimerRef.current = setTimeout(async () => {
      try {
        const rv = await reviewPlan({ ...plan, steps }, readOnlyMode);
        if (reviewReqRef.current === reqId) setDraftReview(rv);
      } catch {
        /* 重判失败:保留旧徽标,执行时服务端仍会再审查 */
      } finally {
        if (reviewReqRef.current === reqId) setReviewing(false);
      }
    }, 400);
  }

  function startEdit() {
    if (!current?.plan || running) return;
    const steps = current.plan.steps.map((s) => ({ ...s })); // 深拷贝,隔离草稿
    setDraftSteps(steps);
    setDraftReview(current.riskReview ?? null);
    scheduleReview(steps);
  }

  function cancelEdit() {
    if (reviewTimerRef.current) clearTimeout(reviewTimerRef.current);
    reviewReqRef.current += 1; // 作废挂起的重判
    setDraftSteps(null);
    setDraftReview(null);
    setReviewing(false);
  }

  // 完成编辑:对**最终提交的步骤**做一次同步重判(不依赖防抖中的旧结果),
  // 据此回写每步风险/只读标志(以服务端为准),再写回计划并落库——
  // 保证历史里的风险标注与「确认执行」禁用门都反映真正将要执行的内容。
  async function commitEdit() {
    if (!current?.plan || draftSteps === null) return;
    if (reviewTimerRef.current) clearTimeout(reviewTimerRef.current);
    reviewReqRef.current += 1; // 作废任何挂起的防抖重判
    setReviewing(true);
    const steps0 = draftSteps;
    const rv = await reviewPlan({ ...current.plan, steps: steps0 }, readOnlyMode).catch(
      () => draftReview ?? current!.riskReview ?? null
    );
    const levels = rv?.stepLevels ?? [];
    const steps: PlanStep[] = steps0.map((s, i) => {
      const lvl = levels[i];
      return lvl ? { ...s, risk: lvl, readOnly: lvl === "low" } : { ...s };
    });
    const editedPlan: Plan = { ...current.plan, steps };
    const updated: TaskRecord = {
      ...current, plan: editedPlan, riskReview: rv ?? current.riskReview,
      executions: [], status: "awaiting_confirmation", updatedAt: nowIso(),
    };
    setCurrent(updated);
    setStepStatus(steps.map(() => "pending"));
    setDraftSteps(null);
    setDraftReview(null);
    setReviewing(false);
    await saveTask(updated).catch(() => {});
    await refreshTasks();
  }

  // 草稿步骤的增删改/移动:更新草稿并触发重判。
  function applyDraft(next: PlanStep[]) {
    setDraftSteps(next);
    scheduleReview(next);
  }
  function editStepSummary(i: number, v: string) {
    if (draftSteps) applyDraft(draftSteps.map((s, idx) => (idx === i ? { ...s, summary: v } : s)));
  }
  function editStepCommand(i: number, v: string) {
    if (draftSteps) applyDraft(draftSteps.map((s, idx) => (idx === i ? { ...s, command: v } : s)));
  }
  function removeStep(i: number) {
    if (draftSteps) applyDraft(draftSteps.filter((_, idx) => idx !== i));
  }
  function addStep() {
    if (draftSteps) applyDraft([...draftSteps, { summary: "新步骤", command: "", risk: "low", readOnly: true }]);
  }
  function moveStep(i: number, dir: -1 | 1) {
    if (!draftSteps) return;
    const j = i + dir;
    if (j < 0 || j >= draftSteps.length) return;
    const next = [...draftSteps];
    [next[i], next[j]] = [next[j], next[i]];
    applyDraft(next);
  }

  async function startExecute() {
    if (!current?.plan || draftSteps !== null) return; // 编辑中不执行
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

  // 切换服务器收藏（收藏置顶,故改动后重新拉取以应用后端排序）。
  async function toggleFavorite(id: string, favorite: boolean) {
    const reqId = ++serversReqRef.current;
    try {
      await setServerFavorite(id, favorite);
      const next = await listServers();
      if (serversReqRef.current === reqId) setServers(next); // 丢弃过期结果
    } catch (e) {
      push("danger", `收藏失败: ${errMsg(e)}`);
    }
  }

  // 并发刷新所有服务器的连通状态（概览页「刷新全部」）。
  async function refreshAll() {
    const reqId = ++serversReqRef.current;
    setRefreshing(true);
    try {
      const next = await refreshAllServers();
      if (serversReqRef.current === reqId) setServers(next); // 丢弃过期结果
    } catch (e) {
      push("danger", `刷新失败: ${errMsg(e)}`);
    } finally {
      setRefreshing(false);
    }
  }

  // 拖拽改尺寸:按下分隔条后跟随鼠标移动调整面板宽/高;松开移除监听。
  const startDrag = (onDelta: (dx: number, dy: number) => void) => (e: ReactMouseEvent) => {
    e.preventDefault();
    const move = (ev: MouseEvent) => onDelta(ev.movementX, ev.movementY);
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.style.userSelect = "";
    };
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  // 命令面板的动作集合：把主界面的关键操作集中为可搜索/键盘可达的快捷入口。
  const paletteCommands: PaletteCommand[] = [
    { id: "ask", label: "新建提问", hint: "聚焦输入框", group: "操作", run: () => { setView("console"); inputRef.current?.focus(); } },
    // 仅在可执行(已选服务器且未在运行)时出现,避免成为静默死动作。
    ...(selectedServerId && !running
      ? [{ id: "doctor", label: "只读体检", hint: selected?.name, group: "操作", run: () => runDoctor() }]
      : []),
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

  // 需关注(离线/资源紧张)的服务器数,用于概览导航角标。
  const alertCount = servers.filter((s) => serverAlert(s) !== null).length;
  const planExecuted = !!current && current.kind === "plan" && current.executions.length > 0;
  // 顶栏标题:各功能视图显示其名称;控制台显示当前任务标题,无任务时显示「新提问」(贴近 Codex 的对话标题)。
  const topTitle =
    view === "audit" ? "审计"
    : view === "settings" ? "设置"
    : view === "dashboard" ? "概览"
    : view === "deploy" ? "Docker 部署"
    : current ? current.title
    : "新提问";
  // 计划编辑态派生值。
  const planEditing = draftSteps !== null;
  const canEditPlan = !!current?.plan && current.kind === "plan" && current.executions.length === 0 && !running;
  // 展示态下:计划是否为空 / 是否含被阻止步骤(据此禁用「确认执行」)。
  const planEmpty = !!current?.plan && current.plan.steps.length === 0;
  // 含真正 Blocked 的步骤(任何模式下都禁止)。
  const planHardBlocked = !!current?.plan && current.plan.steps.some((s) => s.risk === "blocked");
  // 只读优先开启时,任何非 Low(写/中/高)步骤都会被风险闸门升级为 Blocked——
  // 提前在展示态体现,避免「按钮可点→确认弹窗里才被拦死」的困惑死路。
  const planReadonlyBlocked =
    readOnlyMode && !!current?.plan && current.plan.steps.some((s) => s.risk !== "low" && s.risk !== "blocked");
  const planBlocked = planHardBlocked || planReadonlyBlocked;

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-bg text-fg" style={{ fontFamily: "var(--font-sans)" }}>
      {/* 侧栏(可由顶栏左侧开关折叠) */}
      {sidebarOpen && (
      <aside className="flex w-64 flex-none flex-col border-r border-border bg-surface-2">
        {/* 给 macOS 红绿灯(Overlay 标题栏)让出顶部空间,并作为窗口拖拽区 */}
        <div data-tauri-drag-region className="h-7 flex-none" />
        <div className="flex items-center gap-2.5 px-3.5 py-3">
          <span className="flex h-7 w-7 items-center justify-center rounded-md bg-brand font-mono text-sm text-brand-fg">›_</span>
          <span className="text-[13.5px] font-semibold">AiPanel</span>
        </div>
        <div className="flex flex-col gap-px px-2 pb-1">
          <NavItem icon={<PencilIcon size={16} />} label="提问" active={view === "console"} onClick={() => setView("console")} />
          <NavItem icon={<LayoutGrid size={16} />} label="概览" active={view === "dashboard"} onClick={() => setView("dashboard")} badge={alertCount} />
          <NavItem icon={<PanelBottom size={16} />} label="终端" active={view === "console" && shellOpen} onClick={() => { setShellOpen((o) => (view === "console" ? !o : true)); setView("console"); }} />
          <NavItem icon={<FolderTree size={16} />} label="文件" active={view === "console" && filesOpen} onClick={() => { setFilesOpen((o) => (view === "console" ? !o : true)); setView("console"); }} />
          <NavItem icon={<Boxes size={16} />} label="部署" active={view === "deploy"} onClick={() => setView("deploy")} />
          <NavItem icon={<ScrollText size={16} />} label="审计" active={view === "audit"} onClick={openAudit} />
        </div>

        <div className="cx-scroll min-h-0 flex-1 overflow-y-auto px-2 py-1.5">
          <div className="flex items-center justify-between px-2.5 pb-1 pt-2">
            <span className="text-[11.5px] text-fg-subtle">服务器</span>
            <IconButton aria-label="添加服务器" size="sm" onClick={() => setAddOpen(true)}>
              <PlusIcon size={14} />
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
            <div className="flex flex-col items-center gap-1.5 px-2.5 py-6 text-center">
              <ServerIconLucide size={22} className="text-fg-subtle" strokeWidth={1.75} />
              <div className="text-[12.5px] text-fg-subtle">还没有服务器,点 ＋ 添加</div>
            </div>
          ) : (
            filteredServers.map((srv) => {
              const isSel = srv.id === selectedServerId;
              return (
                <div key={srv.id} className="mt-0.5">
                  <div
                    onClick={() => setSelectedServerId(srv.id)}
                    onContextMenu={(e) =>
                      openCtx(e, [
                        { label: "连接 / 重连", onClick: () => { setSelectedServerId(srv.id); checkSshConnection(srv.id).then((r) => { setServers((prev) => prev.map((s) => (s.id === srv.id ? { ...s, status: r.ok ? "online" : "offline" } : s))); push(r.ok ? "success" : "danger", r.ok ? `${srv.name} 连接成功` : `连接失败:${r.message}`); }).catch(() => {}); } },
                        { label: "打开终端", onClick: () => { setSelectedServerId(srv.id); setView("console"); setShellOpen(true); } },
                        { label: "打开文件", onClick: () => { setSelectedServerId(srv.id); setView("console"); setFilesOpen(true); } },
                        { label: "Docker 部署", onClick: () => { setSelectedServerId(srv.id); setView("deploy"); } },
                        { label: srv.favorite ? "取消收藏" : "收藏置顶", onClick: () => toggleFavorite(srv.id, !srv.favorite) },
                        { label: "编辑服务器", onClick: () => setEditing(srv) },
                        { label: "删除服务器", danger: true, onClick: () => { if (window.confirm(`删除服务器「${srv.name}」?`)) void deleteServer(srv.id).then(() => { setServers((prev) => prev.filter((s) => s.id !== srv.id)); if (selectedServerId === srv.id) setSelectedServerId(null); push("success", "服务器已删除"); }).catch((e) => push("danger", errMsg(e))); } },
                      ])
                    }
                    className={`group flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] transition-colors hover:bg-hover ${isSel ? "" : "text-fg-muted"}`}
                  >
                    <ServerIconLucide size={15} strokeWidth={1.75} className="flex-none text-fg-subtle" />
                    <span className="flex-1 truncate">{srv.name}</span>
                    <button
                      aria-label={srv.favorite ? "取消收藏" : "收藏"}
                      onClick={(e) => { e.stopPropagation(); toggleFavorite(srv.id, !srv.favorite); }}
                      className={`flex-none rounded leading-none transition-opacity focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60 ${srv.favorite ? "text-risk-medium" : "text-fg-subtle opacity-0 group-hover:opacity-100"}`}
                    >
                      <Star size={13} fill={srv.favorite ? "currentColor" : "none"} />
                    </button>
                    <IconButton aria-label="编辑服务器" size="sm" className="opacity-0 transition-opacity group-hover:opacity-100" onClick={(e) => { e.stopPropagation(); setEditing(srv); }}>
                      <PencilIcon size={12} />
                    </IconButton>
                    <span role="img" aria-label={srv.status === "online" ? "在线" : srv.status === "offline" ? "离线" : "未知/未检测"} title={srv.status === "online" ? "在线" : srv.status === "offline" ? "离线" : "未知/未检测"} className={`h-1.5 w-1.5 rounded-full ${statusDot(srv.status)}`} />
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
                            onContextMenu={(e) =>
                              openCtx(e, [
                                { label: "打开", onClick: () => openTask(t) },
                                { label: "重命名", onClick: () => { const name = window.prompt("重命名此运行", t.title)?.trim(); if (!name) return; const u = { ...t, title: name, updatedAt: nowIso() }; if (currentRef.current?.id === t.id) setCurrent(u); void saveTask(u).then(refreshTasks).catch(() => {}); } },
                                { label: "删除", danger: true, onClick: () => { void (async () => { if (currentRef.current?.id === t.id) { cancelBackend(); setRunning(false); } await deleteTask(t.id); if (currentRef.current?.id === t.id) setCurrent(null); await refreshTasks(); })(); } },
                              ])
                            }
                            className={`group flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-[13px] transition-colors ${current?.id === t.id ? "bg-selected" : "text-fg-muted hover:bg-hover"}`}
                          >
                            <span className="flex-none text-fg-subtle" title={t.kind}>{KIND_GLYPH[t.kind] ?? <ClipboardList size={13} />}</span>
                            <span className="min-w-0 flex-1 truncate">{t.title}</span>
                            <IconButton aria-label="删除记录" size="sm" className="opacity-0 transition-opacity group-hover:opacity-100" onClick={async (e) => { e.stopPropagation(); if (currentRef.current?.id === t.id) { cancelBackend(); setRunning(false); } await deleteTask(t.id); if (currentRef.current?.id === t.id) setCurrent(null); await refreshTasks(); }}>
                              <XIcon size={12} />
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
          <NavItem icon={<SettingsIcon size={16} />} label="设置" active={view === "settings"} onClick={() => setView("settings")} />
        </div>
      </aside>
      )}

      {/* 主区 */}
      <main className="flex min-w-0 flex-1 flex-col bg-bg">
        <div
          data-tauri-drag-region
          className={`flex h-10 flex-none items-center justify-between border-b border-border pr-2.5 ${sidebarOpen ? "pl-2.5" : "pl-[78px]"}`}
        >
          {/* 左侧:折叠侧栏开关 + 标题(与右侧操作组形成「一左一右」,贴近 Codex) */}
          <div className="flex min-w-0 items-center gap-1.5">
            <IconButton
              aria-label="折叠/展开侧栏"
              onClick={() => setSidebarOpen((o) => !o)}
              size="lg"
              title="折叠/展开侧栏"
              className={sidebarOpen ? "text-fg-muted" : "text-brand"}
            >
              <PanelLeft size={16} />
            </IconButton>
            <span className="truncate text-[13.5px] font-semibold">{topTitle}</span>
            {view === "console" && (
              <div className="relative flex-none">
                <IconButton aria-label="更多" size="lg" title="更多" className="text-fg-muted" onClick={() => setTitleMenuOpen((o) => !o)}>
                  <MoreHorizontal size={16} />
                </IconButton>
                {titleMenuOpen && (
                  <>
                    <div className="fixed inset-0 z-10" onClick={() => setTitleMenuOpen(false)} />
                    <div className="absolute left-0 top-full z-20 mt-1 w-44 rounded-md border border-border bg-surface-1 py-1 shadow-lg">
                      <button
                        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg"
                        onClick={() => {
                          setTitleMenuOpen(false);
                          setCurrent(null);
                          setStepStatus([]);
                          setTermLines([]);
                          setView("console");
                          setTimeout(() => inputRef.current?.focus(), 0);
                        }}
                      >
                        新建提问
                      </button>
                      {current && (
                        <>
                          <button
                            className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg"
                            onClick={() => {
                              setTitleMenuOpen(false);
                              const name = window.prompt("重命名此运行", current.title)?.trim();
                              if (!name) return;
                              const updated = { ...current, title: name, updatedAt: nowIso() };
                              setCurrent(updated);
                              void saveTask(updated).then(refreshTasks).catch(() => {});
                            }}
                          >
                            重命名
                          </button>
                          <button
                            className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] text-risk-blocked transition-colors hover:bg-hover"
                            onClick={() => {
                              setTitleMenuOpen(false);
                              if (!window.confirm("删除此运行记录?")) return;
                              const id = current.id;
                              setCurrent(null);
                              void deleteTask(id).then(refreshTasks).catch(() => {});
                            }}
                          >
                            删除此运行
                          </button>
                        </>
                      )}
                    </div>
                  </>
                )}
              </div>
            )}
          </div>
          {/* 右侧:主题 + 工作区面板开关(分组,贴近 Codex 右侧操作区) */}
          <div className="flex flex-none items-center gap-0.5">
            <IconButton aria-label="切换主题" onClick={toggleTheme} size="lg" title={theme === "light" ? "切到深色" : "切到浅色"}>
              {theme === "light" ? <Moon size={16} /> : <Sun size={16} />}
            </IconButton>
            {view === "console" && (
              <>
                <span className="mx-1 h-4 w-px bg-border" />
                <IconButton aria-label="文件面板" onClick={() => setFilesOpen((o) => !o)} size="lg" title="文件面板" className={filesOpen ? "text-brand" : "text-fg-muted"}>
                  <PanelRight size={16} />
                </IconButton>
                <IconButton aria-label="终端面板" onClick={() => setShellOpen((o) => !o)} size="lg" title="终端面板" className={shellOpen ? "text-brand" : "text-fg-muted"}>
                  <PanelBottom size={16} />
                </IconButton>
                <IconButton aria-label="切换输出" onClick={() => setTerminalOpen((o) => !o)} size="lg" title="运行输出" className={terminalOpen ? "text-brand" : "text-fg-muted"}>
                  <TerminalIconLucide size={16} />
                </IconButton>
              </>
            )}
          </div>
        </div>

        {view === "audit" ? (
          <AuditView onNotify={push} />
        ) : view === "settings" ? (
          <SettingsPanel />
        ) : view === "deploy" ? (
          selected ? (
            <DockerDeployPanel serverId={selected.id} onPlan={(plan, title) => adoptPlan(plan, title)} />
          ) : (
            <div className="flex min-h-0 flex-1 items-center justify-center text-[13px] text-fg-subtle">
              先在左侧选择一台服务器,再进行 Docker 部署
            </div>
          )
        ) : view === "dashboard" ? (
          servers.length === 0 ? (
            <FirstRun onAdd={() => setAddOpen(true)} />
          ) : (
            <Dashboard
              servers={servers}
              selectedServerId={selectedServerId}
              onSelect={(id) => { setSelectedServerId(id); setView("console"); }}
              onToggleFavorite={toggleFavorite}
              onRefreshAll={refreshAll}
              refreshing={refreshing}
            />
          )
        ) : servers.length === 0 ? (
          <FirstRun onAdd={() => setAddOpen(true)} />
        ) : (
          // 工作区:中部控制台 + 右侧文件树 + 底部终端(三栏同屏,Codex 式)
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex min-h-0 flex-1">
              <div className="flex min-w-0 min-h-0 flex-1 flex-col">
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
                    <div className={`flex items-start gap-3 rounded-md border bg-surface-1 px-4 py-3.5 ${planEditing ? "border-brand/50 ring-1 ring-brand/30" : "border-border"}`}>
                      <div className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-md bg-hover text-fg-muted"><ClipboardList size={16} /></div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 text-sm font-semibold">
                          执行计划 · {(planEditing ? draftSteps! : current.plan.steps).length} 个步骤
                          {planEditing && <span className="rounded bg-brand/15 px-1.5 py-0.5 text-[11px] font-medium text-brand">编辑中</span>}
                        </div>
                        <div className="mt-1 line-clamp-2 break-words text-[12.5px] text-fg-muted" title={current.plan.goal}>{current.plan.goal}</div>
                      </div>
                      <div className="flex flex-none items-center gap-1.5">
                        {planEditing ? (
                          <>
                            <Button variant="ghost" size="sm" onClick={cancelEdit}>取消</Button>
                            <Button variant="primary" size="sm" onClick={commitEdit}>完成编辑</Button>
                          </>
                        ) : running ? (
                          <Button variant="secondary" size="sm" onClick={stop}><Square size={13} /> 停止</Button>
                        ) : (
                          <>
                            {current.executions.length > 0 && <Button variant="ghost" size="sm" onClick={() => setTerminalOpen(true)}>查看输出</Button>}
                            {canEditPlan && <Button variant="ghost" size="sm" onClick={startEdit}><PencilIcon size={13} /> 编辑计划</Button>}
                            {planExecuted ? (
                              <Button variant="secondary" size="sm" onClick={startExecute}>重新执行</Button>
                            ) : (
                              <Button variant="primary" size="sm" onClick={startExecute} disabled={planEmpty || planBlocked} title={planBlocked ? "计划含被阻止步骤,请编辑或删除后再执行" : planEmpty ? "计划没有步骤" : undefined}><PlayIcon size={13} /> 确认执行</Button>
                            )}
                          </>
                        )}
                      </div>
                    </div>

                    {/* 展示态:被阻止提示 banner。区分「真正 Blocked」与「只读优先拦截了写操作」,后者给出关掉只读优先的出路。 */}
                    {!planEditing && planBlocked && (
                      <div className="mt-2.5 flex items-center gap-2 rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2 text-[12.5px] text-risk-blocked">
                        <span className="flex-1">
                          {planHardBlocked
                            ? "该计划含被风险策略阻止的步骤,无法执行。"
                            : "只读优先已开启,该计划的写操作步骤被拦截,无法执行。"}
                        </span>
                        {!planHardBlocked && (
                          <button className="font-medium underline" onClick={() => setReadOnly(false)}>关闭只读优先</button>
                        )}
                        {canEditPlan && <button className="font-medium underline" onClick={startEdit}>编辑计划</button>}
                      </div>
                    )}

                    <div className="mt-3.5 flex flex-col gap-2.5">
                      {(planEditing ? draftSteps! : current.plan.steps).map((s, i) => (
                        <StepRow
                          key={i}
                          summary={s.summary}
                          command={s.command}
                          risk={planEditing ? (draftReview?.stepLevels[i] ?? s.risk) : s.risk}
                          status={stepStatus[i] ?? "pending"}
                          edit={planEditing ? {
                            reviewing,
                            onSummary: (v) => editStepSummary(i, v),
                            onCommand: (v) => editStepCommand(i, v),
                            onRemove: () => removeStep(i),
                            onUp: i > 0 ? () => moveStep(i, -1) : undefined,
                            onDown: i < draftSteps!.length - 1 ? () => moveStep(i, 1) : undefined,
                          } : undefined}
                        />
                      ))}
                      {planEditing && draftSteps!.length === 0 && (
                        <div className="rounded-md border border-dashed border-border px-4 py-4 text-center text-[12.5px] text-fg-subtle">计划暂无步骤,点「+ 添加步骤」开始</div>
                      )}
                      {planEditing && (
                        <button onClick={addStep} className="inline-flex items-center justify-center gap-1.5 rounded-md border border-dashed border-border-strong px-3 py-2 text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60">
                          <PlusIcon size={14} /> 添加步骤
                        </button>
                      )}
                    </div>
                  </>
                ) : current ? (
                  <div className="rounded-md border border-border bg-surface-1 px-4 py-4">
                    <div className="mb-2 flex items-center gap-2">
                      <span className={`h-1.5 w-1.5 rounded-full ${current.status === "completed" ? "bg-risk-low" : current.status === "failed" ? "bg-risk-blocked" : "bg-fg-subtle"}`} />
                      <span className="inline-flex items-center gap-1.5 text-sm font-semibold">{current.kind === "diagnose" ? <Sparkles size={14} className="text-fg-muted" /> : null}{current.title}</span>
                      <span className="ml-auto text-[11.5px] text-fg-subtle">{STATUS_LABEL[current.status] ?? current.status}</span>
                      {/* 运行中(如 doctor 计划为空时落入此分支)仍要有「停止」入口,避免无处可停。 */}
                      {running && <Button variant="secondary" size="sm" onClick={stop}><Square size={13} /> 停止</Button>}
                    </div>
                    {/* AI 诊断：展示结构化的调查过程（工具轨迹）+ 结论 */}
                    {current.kind === "diagnose" && current.toolCalls && current.toolCalls.length > 0 && (
                      <div className="mb-3 flex flex-col gap-1.5">
                        <span className="text-[11.5px] text-fg-subtle">调查过程 · {current.toolCalls.length} 次工具调用</span>
                        {current.toolCalls.map((t, i) => (
                          <div key={i} className="rounded-md border border-border bg-bg px-2.5 py-1.5">
                            <div className="flex items-center gap-2 text-[12.5px]">
                              <span className={`flex-none ${t.ok ? "text-risk-low" : "text-risk-blocked"}`}>{t.ok ? <CheckIcon size={13} strokeWidth={2} /> : <XIcon size={13} strokeWidth={2} />}</span>
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
                  <ServerOverview
                    key={selected?.id}
                    server={selected}
                    running={running}
                    onDoctor={runDoctor}
                    onStatus={(online) => {
                      if (!selected) return;
                      setServers((prev) =>
                        prev.map((s) => (s.id === selected.id ? { ...s, status: online ? "online" : "offline" } : s))
                      );
                    }}
                  />
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
                      className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60 ${readOnlyMode ? "text-risk-medium" : "text-fg-subtle"}`}
                    >
                      {readOnlyMode ? <Lock size={14} /> : <LockOpen size={14} />}
                      {readOnlyMode ? "只读优先 · 开" : "只读优先 · 关"}
                    </button>
                    <button onClick={diagnose} disabled={!intentValue.trim() || !selectedServerId || running} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] text-fg-muted transition-colors hover:bg-hover hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60 disabled:opacity-40">
                      <Sparkles size={14} /> AI 诊断
                    </button>
                  </div>
                  <div className="flex items-center gap-2.5">
                    <ModelPicker
                      key={aiProvider?.id ?? "none"}
                      provider={aiProvider}
                      onChanged={() => listProviders().then(setProviders).catch(() => {})}
                      onConfigure={() => setView("settings")}
                    />
                    <button aria-label="发送" onClick={generatePlan} className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-full bg-brand text-brand-fg transition-opacity hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60 disabled:opacity-40" disabled={!intentValue.trim() || !selectedServerId || running}>
                      <ArrowUp size={16} strokeWidth={2} />
                    </button>
                  </div>
                </div>
              </div>
              {selectedServerId && !running && !intentValue.trim() && (
                <div className="mx-auto mt-2 flex max-w-[680px] flex-wrap gap-1.5">
                  {QUICK_INTENTS.map((q) => (
                    <button
                      key={q}
                      onClick={() => { setIntentValue(q); inputRef.current?.focus(); }}
                      className="rounded-full border border-border bg-surface-1 px-2.5 py-1 text-[12px] text-fg-muted transition-colors hover:border-border-strong hover:bg-hover hover:text-fg"
                    >
                      {q}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {terminalOpen && (
              <Terminal
                host={selected?.name ?? "—"}
                live={running}
                lines={termLines.length ? termLines : [{ text: "终端输出会显示在这里。", tone: "muted" }]}
                cursor={running}
              />
            )}
              </div>
              {/* 右侧文件树面板(可拖拽改宽) */}
              {filesOpen && (
                <>
                  <div
                    onMouseDown={startDrag((dx) => setFilesW((w) => Math.min(760, Math.max(360, w - dx))))}
                    role="separator"
                    aria-orientation="vertical"
                    className="group relative w-1.5 flex-none cursor-col-resize bg-border transition-colors hover:bg-brand/40"
                  >
                    <span className="pointer-events-none absolute left-1/2 top-1/2 h-7 w-0.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-fg-subtle/40 transition-colors group-hover:bg-brand" />
                  </div>
                  <aside className="flex min-h-0 min-w-0 flex-none flex-col overflow-hidden border-l border-border" style={{ width: filesW }}>
                    {selected ? (
                      <FileBrowser key={selected.id} serverId={selected.id} serverName={selected.name} />
                    ) : (
                      <div className="flex flex-1 items-center justify-center px-4 text-center text-[12.5px] text-fg-subtle">
                        选择左侧服务器后查看文件
                      </div>
                    )}
                  </aside>
                </>
              )}
            </div>
            {/* 底部交互终端面板(可拖拽改高) */}
            {shellOpen && (
              <>
                <div
                  onMouseDown={startDrag((_dx, dy) => setShellH((h) => Math.min(640, Math.max(140, h - dy))))}
                  role="separator"
                  aria-orientation="horizontal"
                  className="group relative h-1.5 flex-none cursor-row-resize bg-border transition-colors hover:bg-brand/40"
                >
                  <span className="pointer-events-none absolute left-1/2 top-1/2 h-0.5 w-7 -translate-x-1/2 -translate-y-1/2 rounded-full bg-fg-subtle/40 transition-colors group-hover:bg-brand" />
                </div>
                <div className="flex min-h-0 flex-none flex-col" style={{ height: shellH }}>
                  {selected ? (
                    <TerminalSession key={selected.id} serverId={selected.id} serverName={selected.name} connLabel={`${selected.username}@${selected.host}`} />
                  ) : (
                    <div className="flex flex-1 items-center justify-center text-[12.5px] text-fg-subtle">
                      选择左侧服务器后打开终端
                    </div>
                  )}
                </div>
              </>
            )}
          </div>
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
      {ctxMenu && <ContextMenu x={ctxMenu.x} y={ctxMenu.y} items={ctxMenu.items} onClose={() => setCtxMenu(null)} />}

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
        <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-surface-2 text-fg-muted"><ServerIconLucide size={24} strokeWidth={1.75} /></div>
        <h2 className="text-base font-semibold">还没有连接任何服务器</h2>
        <p className="mt-2 text-[13px] leading-relaxed text-fg-muted">
          AiPanel 在本地运行、通过 SSH 管理服务器,不在服务器上常驻。添加一台服务器即可开始只读体检与 AI 运维。凭据只存本地 Keychain。
        </p>
        <div className="mt-5"><Button variant="primary" size="md" onClick={onAdd}><PlusIcon size={16} /> 添加第一台服务器</Button></div>
      </div>
    </div>
  );
}
