import { describe, it, expect } from "vitest";
import { firstOutputLine, executionStatusLabel, executionOutput } from "./auditFormat";
import type { CommandExecution } from "./api";

function ex(p: Partial<CommandExecution>): CommandExecution {
  return {
    command: "uname -a",
    exitCode: 0,
    stdout: "",
    stderr: "",
    durationMs: 1,
    startedAt: "2026-06-19T00:00:00Z",
    ...p,
  };
}

describe("firstOutputLine", () => {
  it("取首条非空行(trim);全空 → null", () => {
    expect(firstOutputLine("\n  \n  hello \nworld")).toBe("hello");
    expect(firstOutputLine("   \n\n")).toBeNull();
    expect(firstOutputLine("")).toBeNull();
  });
});

describe("executionStatusLabel", () => {
  it("正常退出码 → exit N", () => {
    expect(executionStatusLabel(ex({ exitCode: 0 }))).toBe("exit 0");
    expect(executionStatusLabel(ex({ exitCode: 137 }))).toBe("exit 137");
  });
  it("未获得退出码(-1):附 stderr/stdout 首行原因,无原因则纯文案", () => {
    expect(executionStatusLabel(ex({ exitCode: -1 }))).toBe("未获得退出码");
    expect(executionStatusLabel(ex({ exitCode: -1, stderr: "连接超时" }))).toBe("未获得退出码 · 连接超时");
    // 优先 stderr;超过 48 字截断加省略号。
    const long = "x".repeat(60);
    const label = executionStatusLabel(ex({ exitCode: -1, stderr: long }));
    expect(label.startsWith("未获得退出码 · ")).toBe(true);
    expect(label.endsWith("…")).toBe(true);
  });
});

describe("executionOutput", () => {
  it("合并 stdout/stderr 非空行,stdout 在前并标注来源", () => {
    const out = executionOutput(ex({ stdout: "a\n\nb", stderr: "err1\n" }));
    expect(out).toEqual([
      { text: "a", stderr: false },
      { text: "b", stderr: false },
      { text: "err1", stderr: true },
    ]);
  });
  it("全空 → 空数组", () => {
    expect(executionOutput(ex({ stdout: "", stderr: "" }))).toEqual([]);
  });
});
