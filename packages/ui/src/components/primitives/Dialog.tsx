import { useEffect, useRef } from "react";
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
  // 内层容器引用：用于打开时移入初始焦点（键盘可达）。
  const panelRef = useRef<HTMLDivElement>(null);

  // 按 Esc 关闭对话框。Hook 必须放在 early-return 之前以遵守 Hooks 规则。
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  // 打开后把焦点移入对话框，避免 Tab 落在背景内容上（与 CommandPalette 一致）。
  useEffect(() => {
    if (!open) return;
    const id = requestAnimationFrame(() => panelRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, [open]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onClick={onClose}
    >
      <div
        ref={panelRef}
        tabIndex={-1}
        className={cn(
          "w-full max-w-md overflow-hidden rounded-xl border border-border bg-bg shadow-2xl outline-none",
          className
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 pb-3.5 pt-4">
          <h2 className="text-[15px] font-semibold text-fg">{title}</h2>
          {description ? (
            <p className="mt-1 text-[12.5px] leading-relaxed text-fg-muted">{description}</p>
          ) : null}
        </div>
        {children ? <div className="px-5 pb-1 text-sm text-fg">{children}</div> : null}
        <div className="flex items-center justify-end gap-2 px-5 pb-4 pt-3">
          {footer ?? (
            <Button variant="secondary" size="sm" onClick={onClose}>
              关闭
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
