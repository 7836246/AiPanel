import { cn } from "../../lib/cn";
import { Badge } from "../primitives/Badge";
import { Card, CardContent, CardHeader, CardTitle } from "../primitives/Card";
import { CodeBlock } from "../primitives/CodeBlock";
import { RiskBadge } from "./RiskBadge";
import type { RiskLevel } from "./risk";

export interface PlanStep {
  /** What this step does, in plain language. */
  summary: string;
  /** The exact command or tool call to run. */
  command: string;
  risk: RiskLevel;
  /** Read-only steps never change server state. */
  readOnly?: boolean;
}

export interface CommandPlanProps extends React.HTMLAttributes<HTMLDivElement> {
  /** The task goal, restated by the agent. */
  goal: string;
  steps: PlanStep[];
}

/**
 * A reviewable, structured execution plan produced by the agent. Renders each
 * step with its command, read-only flag, and risk level so the user can audit
 * before approving. The agent's natural-language plan must become this shape —
 * and pass risk review — before anything runs.
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
