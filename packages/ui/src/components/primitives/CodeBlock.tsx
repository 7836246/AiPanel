import { cn } from "../../lib/cn";

export interface CodeBlockProps extends React.HTMLAttributes<HTMLPreElement> {
  /** 以等宽字体渲染的命令或输出文本。 */
  children: React.ReactNode;
  /** 块上方的可选标签，例如 "stdout" 或 "$ command"。 */
  label?: string;
}

/** 用于展示命令与（脱敏后）命令输出的等宽代码块。 */
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
