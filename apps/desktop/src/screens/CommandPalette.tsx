import { useEffect, useMemo, useRef, useState, type JSX } from "react";
import { Badge, Input, cn } from "@aipanel/ui";
import {
  ChevronRight,
  Compass,
  type LucideIcon,
  MousePointerClick,
  Search,
  SearchX,
  Server,
  SlidersHorizontal,
} from "lucide-react";

// 分组名到小图标的映射（与控制台里 group 的中文值一致）。
const GROUP_ICON: Record<string, LucideIcon> = {
  操作: MousePointerClick,
  导航: Compass,
  界面: SlidersHorizontal,
  服务器: Server,
};

/** 单个可执行命令项。纯展示——run 由调用方提供。 */
export interface PaletteCommand {
  id: string;
  label: string;
  /** 右侧灰色提示文字（如快捷键说明）。 */
  hint?: string;
  /** 可选分组名，作为分组小标题展示。 */
  group?: string;
  /** 执行命令。组件在调用后会自动 onClose。 */
  run: () => void;
}

export interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  commands: PaletteCommand[];
}

/**
 * ⌘K 命令面板：固定居中浮层 + 半透明遮罩，顶部搜索框（自动聚焦），
 * 按 label/hint 不区分大小写过滤，↑/↓ 选择、Enter 执行、Esc 关闭，鼠标悬停高亮。
 * 纯展示组件，不自行管理 commands；执行后自动 onClose。
 */
export function CommandPalette({
  open,
  onClose,
  commands,
}: CommandPaletteProps): JSX.Element | null {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // 每次打开时重置查询与选中项，并聚焦搜索框。
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      // 等待浮层渲染后聚焦。
      const id = requestAnimationFrame(() => inputRef.current?.focus());
      return () => cancelAnimationFrame(id);
    }
  }, [open]);

  // 按 label / hint 不区分大小写过滤。
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.label.toLowerCase().includes(q) ||
        (c.hint ? c.hint.toLowerCase().includes(q) : false)
    );
  }, [commands, query]);

  // 过滤结果变化时把选中项夹回有效范围。
  useEffect(() => {
    setActiveIndex((i) => Math.min(Math.max(i, 0), Math.max(filtered.length - 1, 0)));
  }, [filtered.length]);

  // 选中项滚动进可视区域。
  useEffect(() => {
    if (!open) return;
    const node = listRef.current?.querySelector<HTMLElement>(
      `[data-cmd-index="${activeIndex}"]`
    );
    node?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  if (!open) return null;

  const execute = (cmd: PaletteCommand | undefined) => {
    if (!cmd) return;
    cmd.run();
    onClose();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => (filtered.length ? (i + 1) % filtered.length : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) =>
        filtered.length ? (i - 1 + filtered.length) % filtered.length : 0
      );
    } else if (e.key === "Enter") {
      e.preventDefault();
      execute(filtered[activeIndex]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 p-4 pt-[12vh]"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      onClick={onClose}
    >
      <div
        className="flex max-h-[70vh] w-full max-w-lg flex-col overflow-hidden rounded-lg border border-border bg-surface-1 shadow-xl"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        {/* 顶部搜索框：左侧 Search 图标 */}
        <div className="border-b border-border p-2">
          <div className="relative">
            <Search
              size={14}
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-subtle"
            />
            <Input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="输入以搜索命令…"
              aria-label="Search commands"
              className="pl-8"
              // 已在容器上统一处理键盘事件，这里只需放行。
            />
          </div>
        </div>

        {/* 命令列表 */}
        <div ref={listRef} className="flex-1 overflow-y-auto p-1">
          {filtered.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 px-3 py-8 text-center text-sm text-fg-muted">
              <SearchX size={24} strokeWidth={1.75} className="text-fg-subtle" />
              没有匹配的命令
            </div>
          ) : (
            renderItems(filtered, activeIndex, setActiveIndex, execute)
          )}
        </div>
      </div>
    </div>
  );
}

/** 渲染（带可选分组小标题的）命令列表项。 */
function renderItems(
  items: PaletteCommand[],
  activeIndex: number,
  setActiveIndex: (i: number) => void,
  execute: (cmd: PaletteCommand) => void
): JSX.Element[] {
  const out: JSX.Element[] = [];
  let lastGroup: string | undefined;

  items.forEach((cmd, index) => {
    // 分组发生变化时插入小标题（带 group 小图标）。
    if (cmd.group && cmd.group !== lastGroup) {
      lastGroup = cmd.group;
      const GroupIcon = GROUP_ICON[cmd.group];
      out.push(
        <div
          key={`group-${cmd.group}-${index}`}
          className="flex items-center gap-1.5 px-3 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-wide text-fg-subtle"
        >
          {GroupIcon ? <GroupIcon size={12} strokeWidth={2} /> : null}
          {cmd.group}
        </div>
      );
    } else if (!cmd.group) {
      lastGroup = undefined;
    }

    const isActive = index === activeIndex;
    out.push(
      <button
        key={cmd.id}
        type="button"
        data-cmd-index={index}
        // mousedown 优先于输入框 blur，避免点击前失焦造成的闪烁。
        onMouseDown={(e) => e.preventDefault()}
        onMouseEnter={() => setActiveIndex(index)}
        onClick={() => execute(cmd)}
        className={cn(
          "flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm text-fg transition-colors",
          isActive ? "bg-selected" : "hover:bg-hover"
        )}
      >
        {/* 选中态左侧指示箭头：仅高亮项可见，避免行宽抖动用占位 */}
        <ChevronRight
          size={14}
          className={cn("flex-none text-brand", isActive ? "opacity-100" : "opacity-0")}
        />
        <span className="min-w-0 flex-1 truncate">{cmd.label}</span>
        {cmd.hint ? (
          <Badge tone="neutral" className="shrink-0 font-mono">
            {cmd.hint}
          </Badge>
        ) : null}
      </button>
    );
  });

  return out;
}

export default CommandPalette;
