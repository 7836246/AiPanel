import { useEffect, useState } from "react";
import { Button, Dialog } from "@aipanel/ui";
import { RISK_META, type Plan, type RiskReview } from "../lib/api";

/**
 * 执行前确认对话框。中/高风险计划在调用 executeConfirmedPlan 之前必须经过这里：
 * - blocked：仅展示，禁止执行；
 * - high / requiresDoubleConfirmation：勾选确认后才能执行（二次确认）；
 * - medium / requiresConfirmation：单次确认即可执行。
 */
export default function ConfirmExecuteDialog({
  open,
  plan,
  review,
  onClose,
  onConfirm,
}: {
  open: boolean;
  plan: Plan | null;
  review: RiskReview | null;
  onClose: () => void;
  onConfirm: (confirmed: boolean, doubleConfirmed: boolean) => void;
}) {
  const [acknowledged, setAcknowledged] = useState(false);

  // 绝不把上一次的「已知晓」勾选带入新的确认。
  useEffect(() => {
    if (!open) setAcknowledged(false);
  }, [open, plan]);

  if (!plan || !review) {
    return <Dialog open={open} onClose={onClose} title="确认执行计划" />;
  }

  const blocked = review.blocked; // 含被阻止步骤：仅展示，禁止执行
  const needsDouble = review.overall === "high" || review.requiresDoubleConfirmation; // 高风险需勾选二次确认
  const overallMeta = RISK_META[review.overall];

  const footer = blocked ? (
    <Button variant="secondary" size="sm" onClick={onClose}>
      关闭
    </Button>
  ) : (
    <>
      <Button variant="secondary" size="sm" onClick={onClose}>
        取消
      </Button>
      <Button
        variant="danger"
        size="sm"
        disabled={needsDouble && !acknowledged}
        onClick={() => onConfirm(true, needsDouble)}
      >
        确认执行
      </Button>
    </>
  );

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="确认执行计划"
      className="max-w-lg"
      footer={footer}
    >
      <div className="flex flex-col gap-3">
        {/* 总体风险 */}
        <div className="flex items-center justify-between rounded-md border border-border bg-surface-2 px-3 py-2">
          <span className="text-[12.5px] text-fg-muted">总体风险</span>
          <span className={`inline-flex items-center gap-1.5 text-sm font-semibold ${overallMeta.text}`}>
            <span className={`h-2 w-2 rounded-full ${overallMeta.dot}`} />
            {overallMeta.label}
          </span>
        </div>

        {/* 目标 */}
        <div className="flex flex-col gap-1">
          <span className="text-[12px] font-medium text-fg-muted">目标</span>
          <p className="text-[13.5px] text-fg">{plan.goal}</p>
        </div>

        {/* 步骤 */}
        <div className="flex flex-col gap-2">
          <span className="text-[12px] font-medium text-fg-muted">步骤 · 共 {plan.steps.length} 个</span>
          <div className="cx-scroll flex max-h-[280px] flex-col gap-2 overflow-y-auto">
            {plan.steps.map((step, i) => {
              const meta = RISK_META[step.risk];
              return (
                <div key={i} className="rounded-md border border-border bg-surface-1 px-3 py-2.5">
                  <div className="flex items-start gap-2.5">
                    <span className="mt-0.5 flex h-4 w-4 flex-none items-center justify-center rounded-full border-[1.5px] border-border-strong text-[10px] font-semibold text-fg-subtle">
                      {i + 1}
                    </span>
                    <span className="min-w-0 flex-1 text-[13px] font-medium text-fg">{step.summary}</span>
                    <span className={`inline-flex flex-none items-center gap-1.5 text-[11.5px] ${meta.text}`}>
                      <span className={`h-1.5 w-1.5 rounded-full ${meta.dot}`} />
                      {meta.label}
                    </span>
                  </div>
                  <div className="mt-2 flex items-center gap-2.5 rounded-md bg-surface-2 px-3 py-2 font-mono text-xs">
                    <span className="text-fg-subtle">$</span>
                    <span className="min-w-0 flex-1 break-all">{step.command}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* 阻止 / 确认提示 */}
        {blocked ? (
          <div className="rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2.5 text-[13px] text-risk-blocked">
            该计划包含被风险策略阻止的步骤，无法执行。
          </div>
        ) : needsDouble ? (
          <label className="flex cursor-pointer items-start gap-2.5 rounded-md border border-risk-high/40 bg-risk-high/10 px-3 py-2.5">
            <input
              type="checkbox"
              checked={acknowledged}
              onChange={(e) => setAcknowledged(e.target.checked)}
              className="mt-0.5 h-3.5 w-3.5 flex-none accent-[var(--color-risk-high)]"
            />
            <span className="text-[13px] text-fg">我已了解高风险操作并确认执行</span>
          </label>
        ) : (
          <div className="rounded-md border border-risk-medium/40 bg-risk-medium/10 px-3 py-2.5 text-[12.5px] text-fg-muted">
            该操作可能变更服务器状态，请确认无误后执行。
          </div>
        )}
      </div>
    </Dialog>
  );
}
