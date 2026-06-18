import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../../lib/cn";

/** 轻量通知：用于把错误/成功等瞬时反馈浮层展示，而不是只写进终端。 */
export type Toast = {
  id: string;
  tone: "info" | "success" | "danger";
  message: string;
};

/** 自动消失时间（毫秒）——约 4 秒。 */
const AUTO_DISMISS_MS = 4000;

/** 生成稳定且无外部依赖的 id：优先用 crypto.randomUUID，回退到 时间戳 + 自增计数。 */
function makeId(counter: number): string {
  try {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
      return crypto.randomUUID();
    }
  } catch {
    // 某些环境下访问 crypto 可能抛错，忽略并走回退方案。
  }
  return `toast-${Date.now()}-${counter}`;
}

/**
 * 自包含的 toast 状态钩子。
 * - push(tone, message)：加入一条通知，约 4 秒后自动消失。
 * - dismiss(id)：手动移除某条通知。
 */
export function useToasts(): {
  toasts: Toast[];
  push: (tone: Toast["tone"], message: string) => void;
  dismiss: (id: string) => void;
} {
  const [toasts, setToasts] = useState<Toast[]>([]);
  // 用 ref 保存自增计数与定时器句柄，避免触发额外渲染。
  const counterRef = useRef(0);
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const push = useCallback(
    (tone: Toast["tone"], message: string) => {
      const id = makeId(counterRef.current++);
      setToasts((prev) => [...prev, { id, tone, message }]);
      // 到点自动消失。
      const timer = setTimeout(() => dismiss(id), AUTO_DISMISS_MS);
      timersRef.current.set(id, timer);
    },
    [dismiss]
  );

  // 组件卸载时清理所有未触发的定时器，避免泄漏与对已卸载组件的 setState。
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      timers.forEach((t) => clearTimeout(t));
      timers.clear();
    };
  }, []);

  return { toasts, push, dismiss };
}

/** 不同语气对应的左侧强调色（走 token，深色模式自动适配）。 */
const TONE_ACCENT: Record<Toast["tone"], string> = {
  success: "border-l-risk-low",
  danger: "border-l-risk-blocked",
  info: "border-l-border-strong",
};

/**
 * 通知视口：固定在右下角的堆叠容器。
 * 在应用根部渲染一次，配合 useToasts 使用。
 */
export function ToastViewport({
  toasts,
  onDismiss,
}: {
  toasts: Toast[];
  onDismiss: (id: string) => void;
}) {
  if (toasts.length === 0) return null;

  return (
    <div
      className="pointer-events-none fixed bottom-4 right-4 z-50 flex flex-col gap-2"
      role="region"
      aria-label="Notifications"
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          role="status"
          className={cn(
            // 卡片基底：表层背景 + 细边框 + 圆角 + 阴影；左侧用强调色作语气标识。
            "pointer-events-auto flex items-start gap-3 rounded-xl border border-l-4 border-border bg-bg px-3.5 py-2.5 text-[13px] text-fg shadow-xl",
            "w-72 max-w-[calc(100vw-2rem)]",
            TONE_ACCENT[t.tone]
          )}
        >
          <span className="flex-1 break-words leading-snug">{t.message}</span>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => onDismiss(t.id)}
            className="shrink-0 rounded-sm text-fg-subtle transition-colors hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}
