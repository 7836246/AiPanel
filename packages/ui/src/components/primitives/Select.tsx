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
  const ref = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  const shown = searchable && query.trim() ? options.filter((o) => o.label.toLowerCase().includes(query.trim().toLowerCase())) : options;

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
                shown.map((o) => {
                  const active = o.value === value;
                  return (
                    <button
                      key={o.value}
                      type="button"
                      role="option"
                      aria-selected={active}
                      onClick={() => {
                        onChange(o.value);
                        setOpen(false);
                        setQuery("");
                      }}
                      className={cn(
                        "flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] transition-colors hover:bg-hover",
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
