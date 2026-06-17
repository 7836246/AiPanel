import { cn } from "../../lib/cn";
import { Badge } from "../primitives/Badge";
import { Card, CardContent, CardHeader, CardTitle } from "../primitives/Card";
import { CodeBlock } from "../primitives/CodeBlock";
import { RiskBadge } from "./RiskBadge";
import type { RiskLevel } from "./risk";

export interface PlanStep {
  /** 该步骤做什么，用通俗语言描述。 */
  summary: string;
  /** 要执行的确切命令或工具调用。 */
  command: string;
  risk: RiskLevel;
  /** 只读步骤不会改变服务器状态。 */
  readOnly?: boolean;
}

export interface CommandPlanProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Agent 复述的任务目标。 */
  goal: string;
  steps: PlanStep[];
}

/**
 * Agent 产出的、可审查的结构化执行计划。逐步渲染命令、只读标记与风险等级，
 * 供用户在批准前审计。Agent 的自然语言计划必须先转换成此结构、并通过风险审查，
 * 才能执行任何操作。
 */
export function CommandPlan({ goal, steps, className, ...props }: CommandPlanProps) {
  return (
    <Card className={className} {...props}>
      <CardHeader>
        <div>
          <CardTitle>Execution plan</CardTitle>
          <p className="mt-0.5 text-xs text-fg-muted">{goal}</p>
        </div>
        <Badge tone="neutral">
          {steps.length} step{steps.length === 1 ? "" : "s"}
        </Badge>
      </CardHeader>
      <CardContent className="space-y-3">
        {steps.map((step, i) => (
          <div
            key={i}
            className="rounded-md border border-border bg-surface-2/50 p-3"
          >
            <div className="mb-2 flex items-center justify-between gap-3">
              <span className="flex items-center gap-2 text-sm text-fg">
                <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-surface-3 text-xs text-fg-muted">
                  {i + 1}
                </span>
                {step.summary}
              </span>
              <span className="flex shrink-0 items-center gap-1.5">
                {step.readOnly ? <Badge tone="info">read-only</Badge> : null}
                <RiskBadge level={step.risk} />
              </span>
            </div>
            <CodeBlock className={cn("mt-1")}>{step.command}</CodeBlock>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
