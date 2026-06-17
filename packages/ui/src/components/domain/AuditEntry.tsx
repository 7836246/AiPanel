import { cn } from "../../lib/cn";
import { CodeBlock } from "../primitives/CodeBlock";
import { RiskBadge } from "./RiskBadge";
import type { RiskLevel } from "./risk";

export interface AuditEntryProps extends React.HTMLAttributes<HTMLDivElement> {
  /** When the command ran (already-formatted, local time). */
  timestamp: string;
  command: string;
  risk: RiskLevel;
  /** Process exit code; 0 is success. */
  exitCode: number;
  /** Redacted command output. Never pass unredacted secrets here. */
  output?: string;
}

/** One immutable line in the local audit trail: what ran, when, and the result. */
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
