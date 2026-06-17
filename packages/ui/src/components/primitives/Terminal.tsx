import { cn } from "../../lib/cn";

export type TerminalLineTone = "default" | "muted" | "prompt" | "success" | "danger";

export interface TerminalLine {
  text: string;
  tone?: TerminalLineTone;
}

export interface TerminalProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Host shown in the dock header, e.g. "prod-ai-01". */
  host: string;
  /** Show the pulsing "live" indicator. */
  live?: boolean;
  /** Transcript lines, rendered in monospace. */
  lines: TerminalLine[];
  /** Append a blinking cursor after the last line. */
  cursor?: boolean;
}

const TONE: Record<TerminalLineTone, string> = {
  default: "text-fg",
  muted: "text-fg-subtle",
  prompt: "text-risk-low",
  success: "text-risk-low",
  danger: "text-risk-blocked",
};

/**
 * Light SSH-transcript dock (Codex-style): a host header with an optional live
 * dot, and a scrollable monospace body. Presentational — pass redacted lines.
 */
export function Terminal({ host, live, lines, cursor, className, ...props }: TerminalProps) {
  return (
    <div
      className={cn("border-t border-border bg-surface-1", className)}
      {...props}
    >
      <div className="flex h-[34px] items-center gap-2 px-3 text-xs">
        <span className="inline-flex items-center gap-1.5 rounded-md bg-hover px-2 py-1">
          <span className="font-mono text-fg-muted">{host}</span>
        </span>
        {live ? (
          <span className="inline-flex items-center gap-1.5 text-xs text-risk-low">
            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-risk-low" />
            实时
          </span>
        ) : null}
      </div>
      <div className="max-h-40 overflow-y-auto border-t border-border px-4 pb-3.5 pt-2.5 font-mono text-xs leading-relaxed">
        {lines.map((line, i) => (
          <div key={i} className={TONE[line.tone ?? "default"]}>
            {line.text}
            {cursor && i === lines.length - 1 ? (
              <span className="ml-0.5 inline-block h-3.5 w-1.5 translate-y-0.5 animate-pulse bg-fg align-middle" />
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}
