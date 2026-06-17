import { forwardRef } from "react";
import { cn } from "../../lib/cn";

export interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

/** 多行文本框——例如用户输入、供 Agent 生成计划的自然语言请求。 */
export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, rows = 3, ...props }, ref) => (
    <textarea
      ref={ref}
      rows={rows}
      className={cn(
        "w-full resize-y rounded-md border border-border bg-surface-2 px-3 py-2 text-sm text-fg placeholder:text-fg-subtle focus-visible:border-brand focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-50",
        className
      )}
      {...props}
    />
  )
);
Textarea.displayName = "Textarea";
