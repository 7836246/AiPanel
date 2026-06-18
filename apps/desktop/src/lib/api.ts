/**
 * 与 Rust 后端通信的类型化桥接层。
 *
 * 所有后端能力都从这里走。在 Tauri 环境下调用真实的 `#[tauri::command]`；
 * 在纯浏览器（`pnpm dev`，无 Tauri 壳）下回退到轻量 mock，保证 UI 仍能渲染。
 * 类型镜像 apps/desktop/src-tauri 的核心类型（camelCase）。凭据只通过专用命令传递，
 * 绝不存放在这些结构体中。
 */
import { invoke, Channel } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

/** 判断当前是否运行在 Tauri 壳内（否则为浏览器开发模式）。 */
export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type ServerStatus = "online" | "offline" | "unknown";
export type AuthKind = "password" | "key" | "agent";

/** 已保存的服务器档案（凭据不在此处，仅以 credentialRef 引用）。 */
export interface ServerProfile {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authKind: AuthKind;
  credentialRef?: string;
  status: ServerStatus;
  facts: Record<string, string>;
  /** 是否收藏（置顶到概览/侧栏靠前）。 */
  favorite: boolean;
  createdAt: string;
  updatedAt: string;
}

/** 创建/更新服务器的输入（不含凭据）。 */
export interface ServerInput {
  name: string;
  host: string;
  port: number;
  username: string;
  authKind: AuthKind;
}

/** 后端返回的结构化错误。 */
export interface AppError {
  code: string;
  message: string;
}

export type RiskLevel = "low" | "medium" | "high" | "blocked";

/** 计划中的单个步骤：摘要、命令、风险等级、是否只读。 */
export interface PlanStep {
  summary: string;
  command: string;
  risk: RiskLevel;
  readOnly: boolean;
  tool?: string;
}

/** Agent 计划转换出的结构化执行计划。 */
export interface Plan {
  id: string;
  serverId?: string;
  goal: string;
  steps: PlanStep[];
  createdAt: string;
}

/** 风险审查中针对某一步骤的单条发现。 */
export interface RiskFinding {
  stepIndex: number;
  category: string;
  level: RiskLevel;
  message: string;
}

/** 整个计划的风险审查结果：总体等级、是否需确认/二次确认/被阻止。 */
export interface RiskReview {
  overall: RiskLevel;
  requiresConfirmation: boolean;
  requiresDoubleConfirmation: boolean;
  blocked: boolean;
  findings: RiskFinding[];
  stepLevels: RiskLevel[];
}

/** 风险等级的展示元信息（中文标签 + 由设计 token 驱动的颜色类）。 */
export const RISK_META: Record<RiskLevel, { label: string; dot: string; text: string }> = {
  low: { label: "低风险", dot: "bg-risk-low", text: "text-risk-low" },
  medium: { label: "中风险", dot: "bg-risk-medium", text: "text-risk-medium" },
  high: { label: "高风险", dot: "bg-risk-high", text: "text-risk-high" },
  blocked: { label: "已阻止", dot: "bg-risk-blocked", text: "text-risk-blocked" },
};

// ---- 浏览器开发模式的 mock 数据（仅在非 Tauri 环境使用） -------------------

let MOCK_SERVERS: ServerProfile[] = [
  mockServer("prod-ai-01", "root@10.0.0.4:22", "online"),
  mockServer("dev-ai-02", "root@10.0.0.5:22", "online"),
  mockServer("edge-node-03", "root@10.0.0.9:22", "online"),
  mockServer("backup-04", "root@10.0.0.12:22", "unknown"),
];

function mockServer(name: string, target: string, status: ServerStatus): ServerProfile {
  const [username, hostport] = target.split("@");
  const [host, port] = hostport.split(":");
  return {
    id: `mock-${name}`,
    name,
    host,
    port: Number(port),
    username,
    authKind: "password",
    status,
    facts: {},
    favorite: false,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

// ---- 后端命令封装 -------------------------------------------------------------

export async function listServers(): Promise<ServerProfile[]> {
  if (!isTauri()) return [...MOCK_SERVERS]; // 返回新引用,保证 React 能感知更新
  return invoke<ServerProfile[]>("list_servers");
}

export async function getServer(id: string): Promise<ServerProfile> {
  if (!isTauri()) return MOCK_SERVERS.find((s) => s.id === id)!;
  return invoke<ServerProfile>("get_server", { id });
}

export async function createServer(input: ServerInput): Promise<ServerProfile> {
  if (!isTauri()) return mockServer(input.name, `${input.username}@${input.host}:${input.port}`, "unknown");
  return invoke<ServerProfile>("create_server", { input });
}

export async function updateServer(id: string, input: ServerInput): Promise<ServerProfile> {
  if (!isTauri()) return mockServer(input.name, `${input.username}@${input.host}:${input.port}`, "unknown");
  return invoke<ServerProfile>("update_server", { id, input });
}

export async function deleteServer(id: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("delete_server", { id });
}

/** 保存 SSH 凭据（密码/私钥），直接写入凭据存储（Keychain），不经数据库。 */
export async function setServerSecret(id: string, secret: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("set_server_secret", { id, secret });
}

/** 设置/取消服务器收藏（置顶到概览/侧栏靠前）。 */
export async function setServerFavorite(id: string, favorite: boolean): Promise<ServerProfile> {
  if (!isTauri()) {
    // 重建数组(不就地改原对象),并按收藏置顶,贴合后端排序与 React 重渲染。
    MOCK_SERVERS = MOCK_SERVERS
      .map((s) => (s.id === id ? { ...s, favorite } : s))
      .sort((a, b) => Number(b.favorite) - Number(a.favorite));
    return MOCK_SERVERS.find((s) => s.id === id)!;
  }
  return invoke<ServerProfile>("set_server_favorite", { id, favorite });
}

/** 并发刷新所有服务器的 SSH 连通状态，返回更新后的列表。 */
export async function refreshAllServers(): Promise<ServerProfile[]> {
  if (!isTauri()) return [...MOCK_SERVERS]; // 新引用,保证重渲染
  return invoke<ServerProfile[]>("refresh_all_servers");
}

/** 一条命令的执行结果（含退出码与脱敏后的输出）。 */
export interface CommandExecution {
  command: string;
  exitCode: number;
  stdout: string;
  stderr: string;
  durationMs: number;
  startedAt: string;
}

/** SSH 连通性检查结果:ok 是否连上,message 为失败时的可读原因。 */
export interface ConnCheck {
  ok: boolean;
  message: string;
}

export async function checkSshConnection(id: string): Promise<ConnCheck> {
  if (!isTauri()) return { ok: Math.random() > 0.3, message: "(浏览器 mock)" }; // 仅用于演示
  return invoke<ConnCheck>("check_ssh_connection", { id });
}

/** 服务器监控指标快照(SSH 只读采集,服务器零 agent)。 */
export interface ServerMetrics {
  cpuPercent: number;
  cpuCores: number;
  load1: number;
  load5: number;
  load15: number;
  memUsedBytes: number;
  memTotalBytes: number;
  swapUsedBytes: number;
  swapTotalBytes: number;
  diskUsedBytes: number;
  diskTotalBytes: number;
  diskPath: string;
  netRxBytes: number;
  netTxBytes: number;
  uptimeSecs: number;
  containers: number;
  services: number;
  listeningPorts: number;
  procs: number;
  sampledAt: string;
}

/** 采集一份服务器监控指标(前端定时轮询;网络/磁盘速率由前端跨样本求差)。 */
export async function serverMetrics(serverId: string): Promise<ServerMetrics> {
  if (!isTauri()) {
    // 浏览器演示:给一组可变的占位值。
    const r = Math.random();
    return {
      cpuPercent: +(r * 30).toFixed(1), cpuCores: 4, load1: +(r * 2).toFixed(2), load5: 0.3, load15: 0.2,
      memUsedBytes: 1.27e9, memTotalBytes: 7.38e9, swapUsedBytes: 0, swapTotalBytes: 0,
      diskUsedBytes: 34.1e9, diskTotalBytes: 196e9, diskPath: "/",
      netRxBytes: Math.floor(1.4e11 + r * 1e7), netTxBytes: Math.floor(1.07e12 + r * 1e7),
      uptimeSecs: 864000, containers: 3, services: 42, listeningPorts: 12, procs: 180,
      sampledAt: new Date().toISOString(),
    };
  }
  return invoke<ServerMetrics>("server_metrics", { id: serverId });
}

export async function runReadonlyCommand(id: string, command: string): Promise<CommandExecution> {
  if (!isTauri())
    return {
      command,
      exitCode: 0,
      stdout: "(browser mock — no SSH)", // 浏览器 mock：不实际执行 SSH
      stderr: "",
      durationMs: 0,
      startedAt: new Date().toISOString(),
    };
  return invoke<CommandExecution>("run_readonly_command", { id, command });
}

/** 只读体检（doctor）报告：系统概况、端口、服务、告警与各命令执行明细。 */
export interface DoctorReport {
  serverId: string;
  os?: string;
  kernel?: string;
  arch?: string;
  uptime?: string;
  load?: string;
  memory?: string;
  disk?: string;
  ports: string[];
  services: string[];
  docker?: string;
  warnings: string[];
  executions: CommandExecution[];
  createdAt: string;
  // Doctor v2：从原始探测输出解析出的结构化指标（旧记录可能没有，故均为可选）。
  cpuPercent?: number;
  memUsedMb?: number;
  memTotalMb?: number;
  diskUsedPercent?: number;
  serviceCount?: number;
  containerCount?: number;
  portCount?: number;
}

export type TaskStatus =
  | "pending"
  | "planning"
  | "awaiting_confirmation"
  | "running"
  | "completed"
  | "failed"
  | "blocked";

/** 一条审计记录：意图、计划、风险判定、确认、执行明细与总结。 */
export interface AuditRecord {
  id: string;
  serverId?: string;
  intent: string;
  plan?: Plan;
  riskReview?: RiskReview;
  confirmedAt?: string;
  executions: CommandExecution[];
  summary?: string;
  status: TaskStatus;
  createdAt: string;
  updatedAt: string;
}

export async function listAuditRecords(limit = 100): Promise<AuditRecord[]> {
  if (!isTauri()) return [];
  return invoke<AuditRecord[]>("list_audit_records", { limit });
}

export async function getAuditRecord(id: string): Promise<AuditRecord> {
  return invoke<AuditRecord>("get_audit_record", { id });
}

export async function createPlan(intent: string, serverId?: string): Promise<Plan> {
  if (!isTauri())
    return {
      id: "mock-plan",
      serverId,
      goal: `诊断：${intent.slice(0, 40)}`,
      steps: [
        { summary: "检查 nginx 服务状态", command: "systemctl status nginx --no-pager", risk: "low", readOnly: true },
        { summary: "检查监听端口", command: "ss -ltn", risk: "low", readOnly: true },
        { summary: "查看 nginx 最近错误日志", command: "journalctl -u nginx -n 50 --no-pager", risk: "low", readOnly: true },
      ],
      createdAt: new Date().toISOString(),
    };
  return invoke<Plan>("create_plan", { intent, serverId });
}

export async function executeConfirmedPlan(
  plan: Plan,
  opts: { confirmed?: boolean; doubleConfirmed?: boolean; readOnlyMode?: boolean } = {}
): Promise<AuditRecord> {
  if (!isTauri())
    return {
      id: "mock-audit",
      serverId: plan.serverId,
      intent: plan.goal,
      plan,
      executions: plan.steps.map((s) => ({
        command: s.command,
        exitCode: 0,
        stdout: "(browser mock)",
        stderr: "",
        durationMs: 30,
        startedAt: new Date().toISOString(),
      })),
      summary: `${plan.steps.length}/${plan.steps.length} 步成功`,
      status: "completed",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
  return invoke<AuditRecord>("execute_confirmed_plan", {
    plan,
    confirmed: opts.confirmed ?? true,
    doubleConfirmed: opts.doubleConfirmed ?? false,
    readOnlyMode: opts.readOnlyMode ?? false,
  });
}

// ---- 任务 / 运行记录（面向用户的历史） -----------------------------------

export type TaskKind = "plan" | "diagnose" | "doctor";

/** 一次任务/运行记录，对应侧栏历史中的一项。 */
export interface TaskRecord {
  id: string;
  serverId?: string;
  title: string;
  intent: string;
  kind: TaskKind;
  plan?: Plan;
  riskReview?: RiskReview;
  executions: CommandExecution[];
  /** AI 诊断的工具调用轨迹（仅 diagnose 类任务）。 */
  toolCalls?: ToolTrace[];
  summary?: string;
  status: TaskStatus;
  createdAt: string;
  updatedAt: string;
}

let mockTasks: TaskRecord[] = [];

export async function listTasks(serverId?: string, limit = 100): Promise<TaskRecord[]> {
  if (!isTauri())
    return mockTasks.filter((t) => !serverId || t.serverId === serverId).slice(0, limit);
  return invoke<TaskRecord[]>("list_tasks", { serverId, limit });
}

export async function getTask(id: string): Promise<TaskRecord> {
  if (!isTauri()) return mockTasks.find((t) => t.id === id)!;
  return invoke<TaskRecord>("get_task", { id });
}

export async function saveTask(task: TaskRecord): Promise<void> {
  if (!isTauri()) {
    mockTasks = [task, ...mockTasks.filter((t) => t.id !== task.id)];
    return;
  }
  return invoke<void>("save_task", { task });
}

export async function deleteTask(id: string): Promise<void> {
  if (!isTauri()) {
    mockTasks = mockTasks.filter((t) => t.id !== id);
    return;
  }
  return invoke<void>("delete_task", { id });
}

// ---- 流式计划执行 ---------------------------------------------

/** 计划执行过程中的流式事件：步骤状态、输出行、整体完成。 */
export type PlanExecEvent =
  | { type: "step"; index: number; total: number; summary: string; status: "running" | "done" | "failed" }
  | { type: "line"; text: string; stderr: boolean }
  | { type: "done"; status: "done" | "failed"; exitCode: number };

/** 流式执行已确认的计划；逐事件回调 onEvent，完成后返回审计记录。 */
export async function runConfirmedPlanStream(
  plan: Plan,
  opts: { confirmed: boolean; doubleConfirmed: boolean; readOnlyMode?: boolean; runId?: string },
  onEvent: (ev: PlanExecEvent) => void
): Promise<AuditRecord> {
  if (!isTauri()) {
    // 浏览器 mock：先一次性拿到结果，再把每条命令/输出补发成流式行。
    const rec = await executeConfirmedPlan(plan, opts);
    for (const ex of rec.executions) {
      onEvent({ type: "line", text: `$ ${ex.command}`, stderr: false });
      if (ex.stdout) for (const l of ex.stdout.split("\n")) onEvent({ type: "line", text: l, stderr: false });
    }
    return rec;
  }
  const ch = new Channel<PlanExecEvent>();
  ch.onmessage = onEvent;
  return invoke<AuditRecord>("run_confirmed_plan_stream", {
    plan,
    confirmed: opts.confirmed,
    doubleConfirmed: opts.doubleConfirmed,
    readOnlyMode: opts.readOnlyMode ?? false,
    runId: opts.runId ?? "",
    onEvent: ch,
  });
}

/** 请求中断某次正在运行的流式任务（doctor/计划执行）。 */
export async function cancelRun(runId: string): Promise<void> {
  if (!isTauri() || !runId) return;
  return invoke<void>("cancel_run", { runId });
}

// ---- 交互式终端(用户自己操作的 SSH 终端;不暴露给 AI)------------------------

/** 打开所选服务器的交互式终端,返回会话 id;终端输出通过 onOutput 流式回调。 */
export async function terminalOpen(
  serverId: string,
  cols: number,
  rows: number,
  onOutput: (data: string) => void
): Promise<string> {
  if (!isTauri()) {
    onOutput("\r\n[浏览器预览模式不支持真实终端,请在桌面端使用]\r\n");
    return "mock";
  }
  const ch = new Channel<string>();
  ch.onmessage = onOutput;
  return invoke<string>("terminal_open", { id: serverId, cols, rows, onOutput: ch });
}

/** 向终端会话写入(用户键入的数据)。 */
export async function terminalWrite(sessionId: string, data: string): Promise<void> {
  if (!isTauri() || sessionId === "mock") return;
  return invoke<void>("terminal_write", { sessionId, data });
}

/** 终端尺寸变化时同步到远端 PTY。 */
export async function terminalResize(sessionId: string, cols: number, rows: number): Promise<void> {
  if (!isTauri() || sessionId === "mock") return;
  return invoke<void>("terminal_resize", { sessionId, cols, rows });
}

/** 关闭终端会话(杀掉本地 ssh 子进程)。 */
export async function terminalClose(sessionId: string): Promise<void> {
  if (!isTauri() || sessionId === "mock") return;
  return invoke<void>("terminal_close", { sessionId });
}

// ---- 文件管理(SFTP over SSH;用户操作,不暴露给 AI)------------------------

export type FileKind = "dir" | "file" | "link";
export interface FileEntry {
  name: string;
  kind: FileKind;
  size: number;
  /** 修改时间(ISO 字符串或原始 ls 时间戳)。 */
  mtime: string;
}
export interface DirListing {
  path: string;
  entries: FileEntry[];
}
export interface FileContent {
  path: string;
  content: string;
  /** 文件过大被截断时为 true。 */
  truncated: boolean;
}

/** 列出远端目录(默认家目录可传 "."或"~")。 */
export async function fsList(serverId: string, path: string): Promise<DirListing> {
  if (!isTauri()) return { path, entries: [] };
  return invoke<DirListing>("fs_list", { id: serverId, path });
}

/** 读取远端文本文件内容(过大将截断)。 */
export async function fsRead(serverId: string, path: string): Promise<FileContent> {
  if (!isTauri()) return { path, content: "", truncated: false };
  return invoke<FileContent>("fs_read", { id: serverId, path });
}

/** 写回远端文本文件(编辑保存)。 */
export async function fsWrite(serverId: string, path: string, content: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("fs_write", { id: serverId, path, content });
}

/** 选择本地文件并上传到远端目录(scp);返回上传的文件名,取消返回 null。 */
export async function fsUpload(serverId: string, remoteDir: string): Promise<string | null> {
  if (!isTauri()) return null;
  const picked = await openDialog({ multiple: false, directory: false, title: "选择要上传的本地文件" });
  if (!picked || typeof picked !== "string") return null;
  await invoke<void>("fs_upload", { id: serverId, localPath: picked, remoteDir });
  return picked.split("/").pop() ?? picked;
}

/** 把远端文件下载到本地(弹保存对话框,scp);取消返回 false。 */
export async function fsDownload(serverId: string, remotePath: string): Promise<boolean> {
  if (!isTauri()) return false;
  const name = remotePath.split("/").pop() || "download";
  const dest = await saveDialog({ defaultPath: name, title: "保存到本地" });
  if (!dest) return false;
  await invoke<void>("fs_download", { id: serverId, remotePath, localPath: dest });
  return true;
}

// ---- 审计/任务 搜索与导出 -------------------------------------

/** 关键字搜索审计记录（意图/总结/命令的子串匹配）。空查询退化为列表。 */
export async function searchAuditRecords(query: string, limit = 100): Promise<AuditRecord[]> {
  if (!isTauri()) return [];
  return invoke<AuditRecord[]>("search_audit_records", { query, limit });
}

/** 关键字搜索运行历史（标题/意图）。空查询退化为列表。 */
export async function searchTasks(serverId: string | undefined, query: string, limit = 100): Promise<TaskRecord[]> {
  if (!isTauri()) return mockTasks.filter((t) => (!serverId || t.serverId === serverId) && (!query || t.title.includes(query)));
  return invoke<TaskRecord[]>("search_tasks", { serverId, query, limit });
}

/** 导出全部审计记录为格式化 JSON 文件（已脱敏，可安全写盘/分享）。取消返回 false。 */
export async function exportAuditJson(): Promise<boolean> {
  const defaultPath = `aipanel-audit-${new Date().toISOString().slice(0, 10)}.json`;
  if (!isTauri()) {
    const blob = new Blob([JSON.stringify([], null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = defaultPath;
    a.click();
    URL.revokeObjectURL(url);
    return true;
  }
  const dest = await saveDialog({ defaultPath, title: "导出审计 JSON" });
  if (!dest) return false;
  await invoke<void>("export_audit_json_to_path", { path: dest });
  return true;
}

export async function serverDoctorPlan(id: string): Promise<Plan> {
  if (!isTauri())
    return {
      id: "mock-plan",
      serverId: id,
      goal: "只读服务器体检",
      steps: [],
      createdAt: new Date().toISOString(),
    };
  return invoke<Plan>("server_doctor_plan", { id });
}

// ---- Docker 部署工作流(只生成结构化 Plan,执行仍走 审查→确认→执行)--------------

/** 内置应用部署模板。 */
export type AppTemplate = "uptimeKuma" | "n8n" | "wordPress" | "postgres" | "redis";
/** 反向代理选择。 */
export type ReverseProxy = "none" | "caddy" | "nginx";

const mockDockerPlan = (serverId: string, goal: string): Plan => ({
  id: "mock-docker-plan",
  serverId,
  goal,
  steps: [],
  createdAt: new Date().toISOString(),
});

/** 只读探测目标服务器的 Docker 环境(是否已装/在 docker 组等)。 */
export async function dockerDetectPlan(serverId: string): Promise<Plan> {
  if (!isTauri()) return mockDockerPlan(serverId, "检测 Docker 环境");
  return invoke<Plan>("docker_detect_plan", { serverId });
}

/** 生成安装 Docker 的计划(写操作,需确认)。 */
export async function dockerInstallPlan(serverId: string): Promise<Plan> {
  if (!isTauri()) return mockDockerPlan(serverId, "安装 Docker");
  return invoke<Plan>("docker_install_plan", { serverId });
}

/** 生成部署某应用模板的计划(compose + 可选反代/HTTPS + 健康检查)。 */
export async function dockerDeployPlan(
  serverId: string,
  app: AppTemplate,
  domain?: string,
  reverseProxy: ReverseProxy = "none",
): Promise<Plan> {
  if (!isTauri()) return mockDockerPlan(serverId, `部署 ${app}`);
  return invoke<Plan>("docker_deploy_plan", { serverId, app, domain: domain ?? null, reverseProxy });
}

/** 只读体检运行期间流式推送的事件。 */
export type DoctorStreamEvent =
  | { type: "step"; index: number; total: number; summary: string; status: "running" | "done" | "failed" }
  | { type: "line"; text: string; stderr: boolean }
  | { type: "warning"; message: string };

/** 以流式方式运行体检；完成后返回最终报告。 */
export async function runServerDoctorStream(
  id: string,
  onEvent: (ev: DoctorStreamEvent) => void,
  runId = ""
): Promise<DoctorReport> {
  if (!isTauri()) {
    const report = await runServerDoctor(id);
    for (const ex of report.executions) {
      onEvent({ type: "line", text: `$ ${ex.command}`, stderr: false });
      if (ex.stdout) for (const l of ex.stdout.split("\n")) onEvent({ type: "line", text: l, stderr: false });
      if (ex.stderr) onEvent({ type: "line", text: ex.stderr, stderr: true });
    }
    for (const w of report.warnings) onEvent({ type: "warning", message: w });
    return report;
  }
  const ch = new Channel<DoctorStreamEvent>();
  ch.onmessage = onEvent;
  return invoke<DoctorReport>("run_server_doctor_stream", { id, runId, onEvent: ch });
}

export async function runServerDoctor(id: string): Promise<DoctorReport> {
  if (!isTauri())
    return {
      serverId: id,
      os: "Ubuntu 22.04.3 LTS (mock)",
      kernel: "Linux 5.15",
      arch: "x86_64",
      uptime: "up 2 hours",
      ports: ["LISTEN 0 128 0.0.0.0:22", "LISTEN 0 511 0.0.0.0:443"],
      services: ["nginx.service", "ssh.service"],
      warnings: [],
      executions: [
        {
          command: "cat /etc/os-release",
          exitCode: 0,
          stdout: 'PRETTY_NAME="Ubuntu 22.04.3 LTS"',
          stderr: "",
          durationMs: 120,
          startedAt: new Date().toISOString(),
        },
      ],
      createdAt: new Date().toISOString(),
    };
  return invoke<DoctorReport>("run_server_doctor", { id });
}

export interface ToolTrace {
  name: string;
  ok: boolean;
  /** 本次工具调用入参的简短摘要。 */
  argsSummary?: string;
  /** 失败时的错误信息（已脱敏）。 */
  error?: string;
  /** 成功结果的前若干字符预览（已脱敏）。 */
  resultPreview?: string;
}
/** 一次 Agent 轮次的结果：总结文本 + 调用过的工具轨迹。 */
export interface AgentTurnResult {
  summary: string;
  toolCalls: ToolTrace[];
}

/** 自主只读诊断：模型通过只读工具调查后给出总结。 */
export async function runAgentTurn(intent: string, serverId?: string): Promise<AgentTurnResult> {
  if (!isTauri())
    return {
      summary: "(浏览器 mock)请在桌面端配置 OpenAI 兼容供应商后使用 AI 诊断。",
      toolCalls: [],
    };
  return invoke<AgentTurnResult>("run_agent_turn", { intent, serverId });
}

export async function reviewPlan(plan: Plan, readOnlyMode = false): Promise<RiskReview> {
  if (!isTauri()) return mockReview(plan, readOnlyMode);
  return invoke<RiskReview>("review_plan", { plan, readOnlyMode });
}

// 风险等级从低到高的顺序，用于在 mock 中取「最高」总体风险。
const RISK_ORDER: RiskLevel[] = ["low", "medium", "high", "blocked"];
// 浏览器 mock 的风险审查：只读模式下把非 low 的步骤标记为 blocked，并据此推导总体等级。
function mockReview(plan: Plan, readOnlyMode: boolean): RiskReview {
  const levels = plan.steps.map((s) =>
    readOnlyMode && s.risk !== "low" ? ("blocked" as RiskLevel) : s.risk
  );
  const overall =
    levels.reduce<RiskLevel>((acc, l) => (RISK_ORDER.indexOf(l) > RISK_ORDER.indexOf(acc) ? l : acc), "low");
  return {
    overall,
    requiresConfirmation: RISK_ORDER.indexOf(overall) >= 1,
    requiresDoubleConfirmation: levels.includes("high"),
    blocked: levels.includes("blocked"),
    findings: [],
    stepLevels: levels,
  };
}

// ---- 供应商 / 模型选择 ------------------------------------------

export type ProviderKind = "codex_app_server" | "openai_compatible" | "custom";

/** 已保存的模型供应商配置（API Key 不在此处，仅以 credentialRef 引用）。 */
export interface ProviderConfig {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl?: string;
  model?: string;
  codexPath?: string;
  credentialRef?: string;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

/** 新增/编辑供应商的输入（API Key 单独通过参数传递）。 */
export interface ProviderInput {
  id?: string;
  name: string;
  kind: ProviderKind;
  baseUrl?: string;
  model?: string;
  codexPath?: string;
  enabled: boolean;
}

/** 模型选择策略：是否按任务自动选择，否则使用指定的默认供应商。 */
export interface ModelSelectionPolicy {
  auto: boolean;
  defaultProviderId?: string;
}

/** 供应商连通性测试结果。 */
export interface ProviderTestResult {
  ok: boolean;
  message: string;
  detail?: string;
}

let mockProviders: ProviderConfig[] = [];

export async function listProviders(): Promise<ProviderConfig[]> {
  if (!isTauri()) return mockProviders;
  return invoke<ProviderConfig[]>("list_providers");
}

export async function saveProvider(
  input: ProviderInput,
  apiKey?: string,
  clearApiKey = false,
): Promise<ProviderConfig> {
  if (!isTauri()) {
    const id = input.id ?? `mock-${Date.now()}`;
    const cfg: ProviderConfig = {
      id,
      name: input.name,
      kind: input.kind,
      baseUrl: input.baseUrl,
      model: input.model,
      codexPath: input.codexPath,
      credentialRef: clearApiKey ? undefined : apiKey ? `provider:${id}` : undefined,
      enabled: input.enabled,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    mockProviders = [...mockProviders.filter((p) => p.id !== cfg.id), cfg];
    return cfg;
  }
  return invoke<ProviderConfig>("save_provider", { input, apiKey: apiKey ?? null, clearApiKey });
}

export async function deleteProvider(id: string): Promise<void> {
  if (!isTauri()) {
    mockProviders = mockProviders.filter((p) => p.id !== id);
    return;
  }
  return invoke<void>("delete_provider", { id });
}

export async function getModelSelectionPolicy(): Promise<ModelSelectionPolicy> {
  if (!isTauri()) return { auto: true };
  return invoke<ModelSelectionPolicy>("get_model_selection_policy");
}

export async function saveModelSelectionPolicy(policy: ModelSelectionPolicy): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("save_model_selection_policy", { policy });
}

export async function testProvider(input: ProviderInput | ProviderConfig, apiKey?: string): Promise<ProviderTestResult> {
  const anyIn = input as ProviderConfig;
  const config: ProviderConfig = {
    id: anyIn.id ?? "test",
    name: input.name,
    kind: input.kind,
    baseUrl: input.baseUrl,
    model: input.model,
    codexPath: input.codexPath,
    credentialRef: anyIn.credentialRef,
    enabled: input.enabled,
    createdAt: anyIn.createdAt ?? new Date().toISOString(),
    updatedAt: anyIn.updatedAt ?? new Date().toISOString(),
  };
  if (!isTauri())
    // 浏览器 mock：仅做最基本的配置完整性检查（OpenAI 兼容须填 baseUrl）。
    return { ok: input.kind !== "openai_compatible" || !!input.baseUrl, message: "(browser mock) 配置检查" };
  return invoke<ProviderTestResult>("test_provider", { config, apiKey: apiKey ?? null });
}

/**
 * 探测供应商可用模型(OpenAI 格式 GET {base}/models)。
 * 传 ProviderInput(设置页未保存表单)或 ProviderConfig(已保存供应商,首页)均可:
 * 已保存供应商带 credentialRef → 后端读 Keychain 里的 key;设置页临时探测则传 apiKey。
 */
export async function listModels(input: ProviderInput | ProviderConfig, apiKey?: string): Promise<string[]> {
  if (!isTauri()) return [];
  const anyIn = input as ProviderConfig;
  const config: ProviderConfig = {
    id: anyIn.id ?? "probe",
    name: input.name,
    kind: input.kind,
    baseUrl: input.baseUrl,
    model: input.model,
    codexPath: input.codexPath,
    credentialRef: anyIn.credentialRef,
    enabled: input.enabled,
    createdAt: anyIn.createdAt ?? new Date().toISOString(),
    updatedAt: anyIn.updatedAt ?? new Date().toISOString(),
  };
  return invoke<string[]>("list_models", { config, apiKey: apiKey ?? null });
}

/** 设置某供应商当前激活的模型(首页切换即用);model 为 null 清空。 */
export async function setProviderModel(id: string, model: string | null): Promise<ProviderConfig | null> {
  if (!isTauri()) {
    mockProviders = mockProviders.map((p) => (p.id === id ? { ...p, model: model ?? undefined } : p));
    return mockProviders.find((p) => p.id === id) ?? null;
  }
  return invoke<ProviderConfig>("set_provider_model", { id, model });
}

/** 返回当前凭据存储后端标识（如 "mock" 表示开发期内存存储）。 */
export async function credentialBackend(): Promise<string> {
  if (!isTauri()) return "mock";
  return invoke<string>("credential_backend");
}

/** 返回应用版本号（浏览器开发模式下为占位值）。 */
export async function appVersion(): Promise<string> {
  if (!isTauri()) return "0.1.0-dev";
  return invoke<string>("app_version");
}
