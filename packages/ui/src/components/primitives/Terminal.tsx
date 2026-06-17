import { cn } from "../../lib/cn";

export type TerminalLineTone = "default" | "muted" | "prompt" | "success" | "danger";

export interface TerminalLine {
  text: string;
  tone?: TerminalLineTone;
}

export interface TerminalProps extends React.HTMLAttributes<HTMLDivElement> {
  /** 标题栏中展示的主机名，例如 "prod-ai-01"。 */
  host: string;
  /** 是否显示脉冲的「实时」指示点。 */
  live?: boolean;
  /** 终端文本行，以等宽字体渲染。 */
  lines: TerminalLine[];
  /** 是否在最后一行追加闪烁光标。 */
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
 * 轻量的 SSH 输出面板（Codex 风格）：带可选实时指示点的主机标题栏，
 * 加上可滚动的等宽正文。纯展示组件——传入的行须已脱敏。
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
