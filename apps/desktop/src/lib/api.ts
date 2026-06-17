/**
 * Typed bridge to the Rust backend.
 *
 * Every backend capability goes through here. Under Tauri it calls the real
 * `#[tauri::command]`s; in a plain browser (`pnpm dev`, no Tauri shell) it falls
 * back to small mocks so the UI still renders. Mirrors apps/desktop/src-tauri
 * core types (camelCase). Secrets are passed to dedicated commands, never kept
 * in these structs.
 */
import { invoke } from "@tauri-apps/api/core";

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

export async function appVersion(): Promise<string> {
  if (!isTauri()) return "0.1.0-dev";
  return invoke<string>("app_version");
}
