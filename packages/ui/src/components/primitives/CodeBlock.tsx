import { cn } from "../../lib/cn";

export interface CodeBlockProps extends React.HTMLAttributes<HTMLPreElement> {
  /** The command or output text to render in monospace. */
  children: React.ReactNode;
  /** Optional label shown above the block, e.g. "stdout" or "$ command". */
  label?: string;
}

/** Monospace block for commands and (redacted) command output. */
export function CodeBlock({ children, label, className, ...props }: CodeBlockProps) {
  return (
    <div className="overflow-hidden rounded-md border border-border bg-bg">
      {label ? (
        <div className="border-b border-border bg-surface-2 px-3 py-1.5 font-mono text-xs text-fg-subtle">
          {label}
        </div>
      ) : null}
      <pre
        className={cn(
          "overflow-x-auto px-3 py-2 font-mono text-xs leading-relaxed text-fg",
          className
        )}
        {...props}
      >
        {children}
      </pre>
    </div>
  );
}
