/**
 * 审计/执行输出的展示格式化纯函数(无副作用,便于单测)。
 */
import type { CommandExecution } from "./api";

/** 取文本里第一条非空行(已 trim);全空返回 null。 */
export function firstOutputLine(text: string): string | null {
  return (
    text
      .split("\n")
      .map((line) => line.trim())
      .find((line) => line.length > 0) ?? null
  );
}

/**
 * 执行状态短标签:正常返回 `exit <code>`;退出码为 -1(未获得)时,
 * 附带 stderr/stdout 首行原因(截断到 48 字)。
 */
export function executionStatusLabel(ex: CommandExecution): string {
  if (ex.exitCode !== -1) return `exit ${ex.exitCode}`;
  const reason = firstOutputLine(ex.stderr) ?? firstOutputLine(ex.stdout);
  if (!reason) return "未获得退出码";
  return `未获得退出码 · ${reason.slice(0, 48)}${reason.length > 48 ? "…" : ""}`;
}

/** 把一次执行的 stdout/stderr 合并为带来源标记的非空行列表(stdout 在前)。 */
export function executionOutput(ex: CommandExecution): { text: string; stderr: boolean }[] {
  const lines: { text: string; stderr: boolean }[] = [];
  for (const line of ex.stdout.split("\n")) {
    if (line.trim()) lines.push({ text: line, stderr: false });
  }
  for (const line of ex.stderr.split("\n")) {
    if (line.trim()) lines.push({ text: line, stderr: true });
  }
  return lines;
}
