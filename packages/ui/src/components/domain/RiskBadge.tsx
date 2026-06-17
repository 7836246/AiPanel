import { cn } from "../../lib/cn";
import { RISK_META, type RiskLevel } from "./risk";

export interface RiskBadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  level: RiskLevel;
}

/**
 * 计划操作的风险指示器。四个等级对应安全模型：
 * Low（只读）、Medium（可恢复的状态变更）、High（数据丢失/服务中断/安全边界变化）、
 * Blocked（默认禁止）。
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
