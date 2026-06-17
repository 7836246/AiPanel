import { cn } from "../../lib/cn";

export interface SpinnerProps extends React.HTMLAttributes<HTMLSpanElement> {
  size?: "sm" | "md";
}

/** 不确定进度的加载指示器——例如命令流式输出时。 */
export function Spinner({ size = "md", className, ...props }: SpinnerProps) {
  return (
    <span
      role="status"
      aria-label="Loading"
      className={cn(
        "inline-block animate-spin rounded-full border-2 border-border border-t-brand",
        size === "sm" ? "h-3.5 w-3.5" : "h-5 w-5",
        className
      )}
      {...props}
    />
  );
}
