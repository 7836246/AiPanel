import { useEffect, useState, type ReactNode } from "react";
import {
  Button,
  IconButton,
  Spinner,
  Terminal,
  type TerminalLine,
} from "@aipanel/ui";
import "./codex-console.css";

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
const Search = ({ size = 16 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke}>
    <circle cx="7" cy="7" r="4.3" />
    <line x1="10.2" y1="10.2" x2="13.5" y2="13.5" />
  </svg>
);
const ListIcon = ({ size = 16 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke}>
    <line x1="3" y1="5" x2="13" y2="5" />
    <line x1="3" y1="8.5" x2="13" y2="8.5" />
    <line x1="3" y1="12" x2="9" y2="12" />
  </svg>
);
const Clock = ({ size = 16 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" {...stroke}>
    <circle cx="8" cy="8" r="5.5" />
    <path d="M8 5v3l2 1.4" />
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
const Check = ({ size = 13, color = "var(--color-risk-low)" }: IconProps & { color?: string }) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke={color} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M3.5 8.3l2.6 2.6L12.5 4.8" />
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

/* ---------------- data ---------------- */
type CheckState = "done" | "running" | "pending";
type StepState = "done" | "running" | "await" | "pending";
interface Step {
  n: number;
  title: string;
  dur: string;
  cmd: string;
  state: StepState;
  showResult: boolean;
  checks: { t: string; s: CheckState }[];
}

function buildSteps(running: boolean): Step[] {
  return [
    {
      n: 1,
      title: "只读检查",
      dur: "00:18",
      cmd: "systemctl status ai-server --no-pager",
      state: "done",
      showResult: true,
      checks: [
        { t: "系统信息检查", s: "done" },
        { t: "磁盘与内存检查", s: "done" },
        { t: "关键配置只读校验", s: "done" },
      ],
    },
    {
      n: 2,
      title: "端口与服务",
      dur: running ? "00:12" : "待开始",
      cmd: "ss -ltnp | grep -E ':22|:80|:443|:8000'",
      state: running ? "running" : "await",
      showResult: false,
      checks: [
        { t: "检查端口 22 / 80 / 443 / 8000", s: running ? "running" : "pending" },
        { t: "服务状态检查", s: "pending" },
        { t: "依赖与连通性检查", s: "pending" },
      ],
    },
    {
      n: 3,
      title: "日志诊断",
      dur: "待执行",
      cmd: 'journalctl -u ai-server --since "1 hour ago" --no-pager',
      state: "pending",
      showResult: false,
      checks: [
        { t: "关键日志扫描", s: "pending" },
        { t: "错误与告警分析", s: "pending" },
        { t: "生成诊断报告", s: "pending" },
      ],
    },
  ];
}

const TERM_IDLE: TerminalLine[] = [
  { text: "root@prod-ai-01:~# systemctl status ai-server --no-pager", tone: "prompt" },
  { text: "● ai-server.service - AI Server" },
  { text: "   Loaded: loaded (/etc/systemd/system/ai-server.service; enabled)", tone: "muted" },
  { text: "   Active: active (running) since Tue 2024-05-21 10:15:42 CST; 2h 13min ago", tone: "success" },
  { text: " Main PID: 23456 (ai-server)" },
  { text: "   Memory: 512.3M" },
  { text: "root@prod-ai-01:~# " },
];
const TERM_RUNNING: TerminalLine[] = [
  { text: "root@prod-ai-01:~# ss -ltnp | grep -E ':22|:80|:443|:8000'", tone: "prompt" },
  { text: 'LISTEN 0  128   0.0.0.0:22     users:(("sshd",pid=812))' },
  { text: 'LISTEN 0  511   0.0.0.0:80     users:(("nginx",pid=1042))' },
  { text: 'LISTEN 0  511   0.0.0.0:443    users:(("nginx",pid=1042))' },
  { text: 'LISTEN 0  128   0.0.0.0:8000   users:(("ai-server",pid=23456))' },
  { text: "▸ 端口检查:4/4 监听正常,无异常占用", tone: "success" },
  { text: "检查服务状态 …", tone: "muted" },
];

/* ---------------- sub-components ---------------- */
function NavItem({ icon, label, kbd, active }: { icon: ReactNode; label: string; kbd?: string; active?: boolean }) {
  return (
    <div
      className={`flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] transition-colors hover:bg-hover ${
        active ? "text-fg" : "text-fg-muted"
      }`}
    >
      {icon}
      <span className="flex-1">{label}</span>
      {kbd ? <span className="text-[11.5px] text-fg-subtle">{kbd}</span> : null}
    </div>
  );
}

function StepRow({ step }: { step: Step }) {
  return (
    <div className="overflow-hidden rounded-md border border-border bg-surface-1">
      <div className="flex items-center gap-2.5 px-3.5 py-3">
        {step.state === "done" && <Check />}
        {step.state === "running" && <Spinner size="sm" />}
        {step.state === "await" && (
          <span className="flex h-4 w-4 items-center justify-center rounded-full border-[1.5px] border-risk-medium text-[10px] font-bold text-risk-medium">
            {step.n}
          </span>
        )}
        {step.state === "pending" && (
          <span className="flex h-4 w-4 items-center justify-center rounded-full border-[1.5px] border-border-strong text-[10px] font-semibold text-fg-subtle">
            {step.n}
          </span>
        )}
        <span className="min-w-0 flex-1 text-[13.5px] font-semibold">{step.title}</span>
        <span className="inline-flex items-center gap-1.5 text-[11.5px] text-fg-muted">
          <span className="h-1.5 w-1.5 rounded-full bg-risk-low" />
          低风险
        </span>
        <span className="font-mono text-[11.5px] text-fg-subtle">{step.dur}</span>
      </div>
      <div className="flex flex-col gap-2.5 px-3.5 pb-3.5">
        <div className="flex items-center gap-2.5 rounded-md bg-hover px-3 py-2 font-mono text-xs">
          <span className="text-fg-subtle">$</span>
          <span className="min-w-0 flex-1 truncate">{step.cmd}</span>
          <IconButton aria-label="复制命令" size="sm">
            <Copy />
          </IconButton>
        </div>
        <div className="flex flex-col gap-1.5">
          {step.checks.map((c, i) => (
            <div key={i} className="flex items-center gap-2.5 text-[12.5px]">
              {c.s === "done" && <Check size={13} />}
              {c.s === "running" && <Spinner size="sm" />}
              {c.s === "pending" && (
                <span className="h-[11px] w-[11px] shrink-0 rounded-full border-[1.5px] border-border-strong" />
              )}
              <span className={c.s === "pending" ? "text-fg-subtle" : undefined}>{c.t}</span>
            </div>
          ))}
        </div>
        {step.showResult && (
          <div className="flex items-center gap-4 pt-0.5 font-mono text-xs text-fg-muted">
            <span className="text-risk-low">exit 0</span>
            <span>0.42s</span>
            <span>影响范围 无</span>
            <span>无需处理</span>
          </div>
        )}
        {step.state === "await" && (
          <div className="text-xs text-fg-muted">需你确认后才会执行 —— 点击右上角「确认执行」开始。</div>
        )}
      </div>
    </div>
  );
}

/* ---------------- screen ---------------- */
export default function CodexConsole() {
  const [, toggleTheme] = useTheme();
  const [running, setRunning] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(true);
  const steps = buildSteps(running);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-bg text-fg" style={{ fontFamily: "var(--font-sans)" }}>
      {/* sidebar */}
      <aside className="flex w-64 flex-none flex-col border-r border-border bg-surface-2">
        <div className="flex items-center gap-2.5 px-3.5 py-3">
          <span className="flex h-7 w-7 items-center justify-center rounded-md bg-brand font-mono text-sm text-brand-fg">›_</span>
          <span className="text-[13.5px] font-semibold">AiPanel</span>
        </div>
        <div className="flex flex-col gap-px px-2 pb-1">
          <NavItem icon={<Pencil />} label="提问" kbd="⌘N" active />
          <NavItem icon={<Search />} label="搜索" />
          <NavItem icon={<ListIcon />} label="审计" />
          <NavItem icon={<Clock />} label="自动化" />
        </div>

        <div className="cx-scroll min-h-0 flex-1 overflow-y-auto px-2 py-1.5">
          <div className="px-2.5 pb-1 pt-2 text-[11.5px] text-fg-subtle">服务器</div>
          <div className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] transition-colors hover:bg-hover">
            <ServerIcon />
            <span className="flex-1">prod-ai-01</span>
            <span className="h-1.5 w-1.5 rounded-full bg-risk-low" />
          </div>
          <div className="flex flex-col gap-px pl-3.5">
            <div className="flex cursor-pointer items-center gap-2 rounded-md bg-selected px-2.5 py-1.5 text-[13px]">
              <span className="min-w-0 flex-1 truncate">排查服务异常</span>
              <span className="flex-none text-[11.5px] text-fg-subtle">1 小时</span>
            </div>
            <div className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-[13px] text-fg-muted transition-colors hover:bg-hover">
              <span className="min-w-0 flex-1 truncate">磁盘空间体检</span>
              <span className="flex-none text-[11.5px] text-fg-subtle">昨天</span>
            </div>
            <div className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-[13px] text-fg-muted transition-colors hover:bg-hover">
              <span className="min-w-0 flex-1 truncate">Nginx 配置检查</span>
              <span className="flex-none text-[11.5px] text-fg-subtle">3 天</span>
            </div>
          </div>
          {["dev-ai-02", "edge-node-03", "backup-04"].map((s, i) => (
            <div key={s} className="mt-0.5 flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] text-fg-muted transition-colors hover:bg-hover">
              <ServerIcon />
              <span className="flex-1">{s}</span>
              <span className={`h-1.5 w-1.5 rounded-full ${i === 2 ? "bg-fg-subtle" : "bg-risk-low"}`} />
            </div>
          ))}
        </div>

        <div className="border-t border-border px-2 py-1.5">
          <NavItem icon={<Gear />} label="设置" />
        </div>
      </aside>

      {/* main */}
      <main className="flex min-w-0 flex-1 flex-col bg-bg">
        {/* top bar */}
        <div className="flex h-10 flex-none items-center justify-between border-b border-border px-3.5">
          <div className="flex min-w-0 items-center gap-2">
            <span className="text-[13.5px] font-semibold">排查 prod-ai-01 服务异常</span>
            <span className="text-fg-subtle">···</span>
          </div>
          <div className="flex items-center gap-0.5">
            <IconButton aria-label="切换主题" onClick={toggleTheme} size="lg">
              <ThemeIcon />
            </IconButton>
            <IconButton aria-label="切换终端" onClick={() => setTerminalOpen((o) => !o)} size="lg">
              <TerminalIcon />
            </IconButton>
          </div>
        </div>

        {/* run scroll */}
        <section className="cx-scroll min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[680px] px-6 pb-3 pt-5">
            {/* run summary card */}
            <div className="flex items-start gap-3 rounded-md border border-border bg-surface-1 px-4 py-3.5">
              <div className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-md bg-hover text-fg-muted">
                <ListIcon size={16} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold">执行计划 · 3 个步骤</div>
                <div className="mt-1 flex flex-wrap items-center gap-3 text-[12.5px] text-fg-muted">
                  <span className="inline-flex items-center gap-1.5">
                    <span className="h-[7px] w-[7px] rounded-full bg-risk-low" />1 完成
                  </span>
                  <span className="inline-flex items-center gap-1.5">
                    <span className="h-[7px] w-[7px] rounded-full bg-risk-medium" />
                    {running ? "1 进行中" : "1 待确认"}
                  </span>
                  <span className="inline-flex items-center gap-1.5">
                    <span className="h-[7px] w-[7px] rounded-full bg-fg-subtle" />1 待执行
                  </span>
                  <span className="text-fg-subtle">· 只读诊断,不会修改服务器</span>
                </div>
              </div>
              <div className="flex flex-none items-center gap-1.5">
                <Button variant="ghost" size="sm">查看输出</Button>
                {running ? (
                  <Button variant="secondary" size="sm" onClick={() => setRunning(false)}>
                    停止
                  </Button>
                ) : (
                  <Button variant="primary" size="sm" onClick={() => { setRunning(true); setTerminalOpen(true); }}>
                    <Play /> 确认执行
                  </Button>
                )}
              </div>
            </div>

            {/* steps */}
            <div className="mt-3.5 flex flex-col gap-2.5">
              {steps.map((s) => (
                <StepRow key={s.n} step={s} />
              ))}
            </div>
          </div>
        </section>

        {/* composer */}
        <div className="flex-none bg-bg px-6 pb-3.5 pt-1.5">
          <div className="mx-auto max-w-[680px] rounded-lg border border-border-strong bg-surface-1 px-3 pb-2.5 pl-4 pt-3 shadow-sm">
            <input
              placeholder="向当前运行追加指令,例如「顺便看看 80 端口的响应头」"
              className="w-full border-none bg-transparent pb-2.5 pt-0.5 text-sm outline-none placeholder:text-fg-subtle"
            />
            <div className="flex items-center justify-between gap-2.5">
              <div className="flex items-center gap-2">
                <IconButton aria-label="添加" variant="bordered" size="lg">
                  <Plus />
                </IconButton>
                <button className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] text-risk-medium transition-colors hover:bg-hover">
                  ⚠ 只读优先
                </button>
              </div>
              <div className="flex items-center gap-2.5">
                <button className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[12.5px] text-fg-muted transition-colors hover:bg-hover">
                  自动选模型 ▾
                </button>
                <button
                  aria-label="发送"
                  className="flex h-[30px] w-[30px] flex-none items-center justify-center rounded-full bg-brand text-brand-fg"
                >
                  <SendArrow />
                </button>
              </div>
            </div>
          </div>
        </div>

        {/* terminal dock */}
        {terminalOpen && (
          <Terminal
            host="prod-ai-01"
            live={running}
            lines={running ? TERM_RUNNING : TERM_IDLE}
            cursor
          />
        )}
      </main>
    </div>
  );
}
