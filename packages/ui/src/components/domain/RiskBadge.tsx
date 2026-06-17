import { cn } from "../../lib/cn";
import { RISK_META, type RiskLevel } from "./risk";

export interface RiskBadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  level: RiskLevel;
}

/**
 * Risk indicator for a planned operation. The four levels map to the security
 * model: Low (read-only), Medium (recoverable), High (data loss / outage /
 * boundary change), Blocked (forbidden by default).
 */
export function RiskBadge({ level, className, ...props }: RiskBadgeProps) {
  const meta = RISK_META[level];
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-semibold uppercase tracking-wide",
        meta.bg,
        meta.text,
        meta.border,
        className
      )}
      {...props}
    >
      <span className={cn("h-1.5 w-1.5 rounded-full", meta.dot)} />
      {meta.label}
    </span>
  );
}
