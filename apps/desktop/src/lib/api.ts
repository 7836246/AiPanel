/**
 * Typed bridge to the Rust backend.
 *
 * Every backend capability goes through here. Under Tauri it calls the real
 * `#[tauri::command]`s; in a plain browser (`pnpm dev`, no Tauri shell) it falls
 * back to small mocks so the UI still renders. Mirrors apps/desktop/src-tauri
 * core types (camelCase). Secrets are passed to dedicated commands, never kept
 * in these structs.
 */
import { invoke, Channel } from "@tauri-apps/api/core";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type ServerStatus = "online" | "offline" | "unknown";
export type AuthKind = "password" | "key" | "agent";

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
  createdAt: string;
  updatedAt: string;
}

export interface ServerInput {
  name: string;
  host: string;
  port: number;
  username: string;
  authKind: AuthKind;
}

export interface AppError {
  code: string;
  message: string;
}

export type RiskLevel = "low" | "medium" | "high" | "blocked";

export interface PlanStep {
  summary: string;
  command: string;
  risk: RiskLevel;
  readOnly: boolean;
  tool?: string;
}

export interface Plan {
  id: string;
  serverId?: string;
  goal: string;
  steps: PlanStep[];
  createdAt: string;
}

export interface RiskFinding {
  stepIndex: number;
  category: string;
  level: RiskLevel;
  message: string;
}

export interface RiskReview {
  overall: RiskLevel;
  requiresConfirmation: boolean;
  requiresDoubleConfirmation: boolean;
  blocked: boolean;
  findings: RiskFinding[];
  stepLevels: RiskLevel[];
}

/** Display metadata for a risk level (Chinese label + token-driven colors). */
export const RISK_META: Record<RiskLevel, { label: string; dot: string; text: string }> = {
  low: { label: "低风险", dot: "bg-risk-low", text: "text-risk-low" },
  medium: { label: "中风险", dot: "bg-risk-medium", text: "text-risk-medium" },
  high: { label: "高风险", dot: "bg-risk-high", text: "text-risk-high" },
  blocked: { label: "已阻止", dot: "bg-risk-blocked", text: "text-risk-blocked" },
};

// ---- browser-dev mocks (only used when not under Tauri) -------------------

const MOCK_SERVERS: ServerProfile[] = [
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
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

// ---- commands -------------------------------------------------------------

export async function listServers(): Promise<ServerProfile[]> {
  if (!isTauri()) return MOCK_SERVERS;
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

/** Store an SSH secret (password/private key). Goes straight to the credential store. */
export async function setServerSecret(id: string, secret: string): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("set_server_secret", { id, secret });
}

export interface CommandExecution {
  command: string;
  exitCode: number;
  stdout: string;
  stderr: string;
  durationMs: number;
  startedAt: string;
}

export async function checkSshConnection(id: string): Promise<boolean> {
  if (!isTauri()) return Math.random() > 0.3; // demo only
  return invoke<boolean>("check_ssh_connection", { id });
}

export async function runReadonlyCommand(id: string, command: string): Promise<CommandExecution> {
  if (!isTauri())
    return {
      command,
      exitCode: 0,
      stdout: "(browser mock — no SSH)",
      stderr: "",
      durationMs: 0,
      startedAt: new Date().toISOString(),
    };
  return invoke<CommandExecution>("run_readonly_command", { id, command });
}

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
}

export type TaskStatus =
  | "pending"
  | "planning"
  | "awaiting_confirmation"
  | "running"
  | "completed"
  | "failed"
  | "blocked";

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

/** Live events streamed while the server doctor runs. */
export type DoctorStreamEvent =
  | { type: "step"; index: number; total: number; summary: string; status: "running" | "done" | "failed" }
  | { type: "line"; text: string; stderr: boolean }
  | { type: "warning"; message: string };

/** Run the doctor with live streaming; resolves with the final report. */
export async function runServerDoctorStream(
  id: string,
  onEvent: (ev: DoctorStreamEvent) => void
): Promise<DoctorReport> {
  if (!isTauri()) {
    onEvent({ type: "line", text: "(浏览器 mock — 无流式)", stderr: false });
    return runServerDoctor(id);
  }
  const ch = new Channel<DoctorStreamEvent>();
  ch.onmessage = onEvent;
  return invoke<DoctorReport>("run_server_doctor_stream", { id, onEvent: ch });
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
}
export interface AgentTurnResult {
  summary: string;
  toolCalls: ToolTrace[];
}

/** Autonomous read-only diagnosis: the model investigates via read-only tools, then summarizes. */
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

const RISK_ORDER: RiskLevel[] = ["low", "medium", "high", "blocked"];
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

// ---- providers / model selection ------------------------------------------

export type ProviderKind = "codex_app_server" | "openai_compatible" | "custom";

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

export interface ProviderInput {
  id?: string;
  name: string;
  kind: ProviderKind;
  baseUrl?: string;
  model?: string;
  codexPath?: string;
  enabled: boolean;
}

export interface ModelSelectionPolicy {
  auto: boolean;
  defaultProviderId?: string;
}

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

export async function saveProvider(input: ProviderInput, apiKey?: string): Promise<ProviderConfig> {
  if (!isTauri()) {
    const cfg: ProviderConfig = {
      id: input.id ?? `mock-${Date.now()}`,
      name: input.name,
      kind: input.kind,
      baseUrl: input.baseUrl,
      model: input.model,
      codexPath: input.codexPath,
      credentialRef: apiKey ? `provider:${input.id ?? "new"}` : undefined,
      enabled: input.enabled,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    mockProviders = [...mockProviders.filter((p) => p.id !== cfg.id), cfg];
    return cfg;
  }
  return invoke<ProviderConfig>("save_provider", { input, apiKey: apiKey ?? null });
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

export async function testProvider(input: ProviderInput, apiKey?: string): Promise<ProviderTestResult> {
  const config: ProviderConfig = {
    id: input.id ?? "test",
    name: input.name,
    kind: input.kind,
    baseUrl: input.baseUrl,
    model: input.model,
    codexPath: input.codexPath,
    enabled: input.enabled,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
  if (!isTauri())
    return { ok: input.kind !== "openai_compatible" || !!input.baseUrl, message: "(browser mock) 配置检查" };
  return invoke<ProviderTestResult>("test_provider", { config, apiKey: apiKey ?? null });
}

export async function credentialBackend(): Promise<string> {
  if (!isTauri()) return "mock";
  return invoke<string>("credential_backend");
}

export async function appVersion(): Promise<string> {
  if (!isTauri()) return "0.1.0-dev";
  return invoke<string>("app_version");
}
