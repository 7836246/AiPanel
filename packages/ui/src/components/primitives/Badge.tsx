import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/cn";

const badge = cva(
  "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium",
  {
    variants: {
      tone: {
        neutral: "border-border bg-surface-2 text-fg-muted",
        brand: "border-brand/40 bg-brand/10 text-brand",
        success: "border-success/40 bg-success/10 text-success",
        warning: "border-warning/40 bg-warning/10 text-warning",
        danger: "border-danger/40 bg-danger/10 text-danger",
        info: "border-info/40 bg-info/10 text-info",
      },
    },
    defaultVariants: {
      tone: "neutral",
    },
  }
);

/** 徽标属性：原生 span 属性 + cva 色调变体（tone）。 */
export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badge> {}

/** 小状态标签。服务器操作的风险等级请优先用领域组件 `RiskBadge`。 */
export function Badge({ className, tone, ...props }: BadgeProps) {
  return <span className={cn(badge({ tone }), className)} {...props} />;
}
