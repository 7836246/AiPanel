import { cn } from "../../lib/cn";
import { Button } from "./Button";

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children?: React.ReactNode;
  /** 底部操作区（右对齐）。默认为一个「关闭」按钮。 */
  footer?: React.ReactNode;
  className?: string;
}

/** 受控模态对话框。用于确认操作——按安全模型，高风险操作需要二次明确确认。 */
export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  className,
}: DialogProps) {
  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onClick={onClose}
    >
      <div
        className={cn(
          "w-full max-w-md rounded-lg border border-border bg-surface-1 shadow-xl",
          className
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">{title}</h2>
          {description ? (
            <p className="mt-1 text-xs text-fg-muted">{description}</p>
          ) : null}
        </div>
        {children ? <div className="px-4 py-3 text-sm text-fg">{children}</div> : null}
        <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
          {footer ?? (
            <Button variant="secondary" size="sm" onClick={onClose}>
              Close
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
