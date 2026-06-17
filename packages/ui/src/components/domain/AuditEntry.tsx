import { cn } from "../../lib/cn";
import { CodeBlock } from "../primitives/CodeBlock";
import { RiskBadge } from "./RiskBadge";
import type { RiskLevel } from "./risk";

export interface AuditEntryProps extends React.HTMLAttributes<HTMLDivElement> {
  /** 命令执行时间（已格式化的本地时间）。 */
  timestamp: string;
  command: string;
  risk: RiskLevel;
  /** 进程退出码；0 表示成功。 */
  exitCode: number;
  /** 脱敏后的命令输出。绝不在此传入未脱敏的敏感信息。 */
  output?: string;
}

/** 本地审计轨迹中的一条不可变记录：执行了什么、何时执行、结果如何。 */
export function AuditEntry({
  timestamp,
  command,
  risk,
  exitCode,
  output,
  className,
  ...props
}: AuditEntryProps) {
  const ok = exitCode === 0;
  return (
    <div
      className={cn("border-l-2 border-border py-2 pl-3", className)}
      {...props}
    >
      <div className="flex items-center gap-2 text-xs text-fg-subtle">
        <time className="font-mono">{timestamp}</time>
        <RiskBadge level={risk} />
        <span
          className={cn(
            "ml-auto font-mono font-medium",
            ok ? "text-success" : "text-danger"
          )}
        >
          exit {exitCode}
        </span>
      </div>
      <p className="mt-1 font-mono text-xs text-fg">{command}</p>
      {output ? (
        <CodeBlock className="mt-1.5 max-h-40 overflow-auto">{output}</CodeBlock>
      ) : null}
    </div>
  );
}
