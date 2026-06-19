import { useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "../../lib/cn";

export interface SelectOption {
  value: string;
  label: string;
  icon?: ReactNode;
}

export interface SelectProps {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  /** 选项较多时显示搜索框过滤。 */
  searchable?: boolean;
  searchPlaceholder?: string;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}

const Chevron = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d="m6 9 6 6 6-6" />
  </svg>
);
const CheckMark = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d="M20 6 9 17l-5-5" />
  </svg>
);

/**
 * Codex 风格下拉选择:按钮显示当前项(图标 + 文案)+ 下箭头,点开浮层列出选项(选中项打勾),
 * 可选搜索过滤。点遮罩 / Esc 关闭,键盘可达。受控组件(value/onChange)。
 */
export function Select({
  value,
  options,
  onChange,
  placeholder = "请选择",
  searchable = false,
  searchPlaceholder = "搜索…",
  disabled,
  className,
  ...rest
}: SelectProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0); // 键盘高亮项索引(相对 shown)
  const ref = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.value === value);

  const shown = searchable && query.trim() ? options.filter((o) => o.label.toLowerCase().includes(query.trim().toLowerCase())) : options;

  // 选中并收起(点击/回车共用)。
  const choose = (v: string) => {
    onChange(v);
    setOpen(false);
    setQuery("");
  };

  // 打开或过滤结果变化时,把高亮重置到当前选中项(否则首项)。仅依赖 open/query,
  // 避免把 shown 放进依赖导致每次渲染重置、压掉方向键移动。
  useEffect(() => {
    if (!open) return;
    const i = shown.findIndex((o) => o.value === value);
    setActiveIdx(i >= 0 ? i : 0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, query]);

  // 键盘:Esc 关闭;↑↓ 移动高亮;Enter 选中高亮项(不打断既有点击流)。
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIdx((i) => Math.min(i + 1, shown.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIdx((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter") {
        const o = shown[activeIdx];
        if (o) {
          e.preventDefault();
          choose(o.value);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, shown, activeIdx]);

  return (
    <div ref={ref} className={cn("relative", className)}>
      <button
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={rest["aria-label"]}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-2 rounded-lg border border-border bg-surface-2 px-2.5 py-1.5 text-[13px] text-fg transition-colors hover:border-border-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {current?.icon && <span className="flex-none text-fg-muted">{current.icon}</span>}
        <span className={cn("min-w-0 flex-1 truncate text-left", !current && "text-fg-subtle")}>{current?.label ?? placeholder}</span>
        <span className="flex-none text-fg-subtle">
          <Chevron />
        </span>
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            role="listbox"
            className="absolute left-0 right-0 top-full z-50 mt-1 max-h-72 overflow-hidden rounded-lg border border-border bg-bg shadow-xl"
          >
            {searchable && (
              <div className="border-b border-border p-1.5">
                <input
                  autoFocus
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={searchPlaceholder}
                  className="w-full rounded-md bg-surface-2 px-2.5 py-1.5 text-[12.5px] outline-none"
                />
              </div>
            )}
            <div className="max-h-60 overflow-y-auto py-1">
              {shown.length === 0 ? (
                <div className="px-3 py-2 text-[12.5px] text-fg-subtle">无匹配项</div>
              ) : (
                shown.map((o, i) => {
                  const active = o.value === value; // 当前已选中
                  const highlighted = i === activeIdx; // 键盘高亮
                  return (
                    <button
                      key={o.value}
                      type="button"
                      role="option"
                      aria-selected={active}
                      onMouseEnter={() => setActiveIdx(i)}
                      onClick={() => choose(o.value)}
                      className={cn(
                        "flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] transition-colors hover:bg-hover",
                        highlighted && "bg-hover",
                        active ? "text-fg" : "text-fg-muted",
                      )}
                    >
                      {o.icon && <span className="flex-none text-fg-muted">{o.icon}</span>}
                      <span className="min-w-0 flex-1 truncate">{o.label}</span>
                      {active && (
                        <span className="flex-none text-brand">
                          <CheckMark />
                        </span>
                      )}
                    </button>
                  );
                })
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
