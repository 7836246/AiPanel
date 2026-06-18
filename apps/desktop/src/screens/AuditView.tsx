import { useEffect, useState, type JSX } from "react";
import { Button, Input, Spinner } from "@aipanel/ui";
import { ChevronDown, ChevronRight, Download, ScrollText, Search } from "lucide-react";
import {
  listAuditRecords,
  searchAuditRecords,
  exportAuditJson,
  type AuditRecord,
} from "../lib/api";
import { formatRelativeTime } from "../lib/time";

// 任务状态到中文标签的映射（与控制台保持一致）。
const STATUS_LABEL: Record<string, string> = {
  completed: "完成",
  failed: "失败",
  blocked: "已阻止",
  running: "进行中",
  awaiting_confirmation: "待确认",
  planning: "规划中",
  pending: "待处理",
};

// 顶部状态筛选项：全部 / 完成 / 失败（前端按 record.status 过滤）。
type StatusFilter = "all" | "completed" | "failed";
const FILTERS: { key: StatusFilter; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "completed", label: "完成" },
  { key: "failed", label: "失败" },
];

// 从后端错误或任意异常中提取可展示的错误文本。
const errMsg = (e: unknown): string =>
  e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);

function firstOutputLine(text: string): string | null {
  return text
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0) ?? null;
}

function executionStatusLabel(ex: AuditRecord["executions"][number]): string {
  if (ex.exitCode !== -1) return `exit ${ex.exitCode}`;
  const reason = firstOutputLine(ex.stderr) ?? firstOutputLine(ex.stdout);
  if (!reason) return "未获得退出码";
  return `未获得退出码 · ${reason.slice(0, 48)}${reason.length > 48 ? "…" : ""}`;
}

function executionOutput(ex: AuditRecord["executions"][number]): { text: string; stderr: boolean }[] {
  const lines: { text: string; stderr: boolean }[] = [];
  for (const line of ex.stdout.split("\n")) {
    if (line.trim()) lines.push({ text: line, stderr: false });
  }
  for (const line of ex.stderr.split("\n")) {
    if (line.trim()) lines.push({ text: line, stderr: true });
  }
  return lines;
}

// 独立审计视图：自加载审计记录,支持搜索、状态筛选与导出 JSON 文件。
export default function AuditView({
  onNotify,
}: {
  onNotify?: (tone: "info" | "success" | "danger", message: string) => void;
}): JSX.Element {
  const [records, setRecords] = useState<AuditRecord[]>([]);
  const [query, setQuery] = useState("");
  // 防抖后的查询：实际触发后端请求的值，避免每次按键都打请求。
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [filter, setFilter] = useState<StatusFilter>("all");
  const [loading, setLoading] = useState(true);
  const [openId, setOpenId] = useState<string | null>(null);
  // 导出进行中标志：导出期间禁用按钮,防止重复点击触发多次复制。
  const [exporting, setExporting] = useState(false);

  // 搜索防抖：输入停止 ~250ms 后才把 query 同步给 debouncedQuery。
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), 250);
    return () => clearTimeout(timer);
  }, [query]);

  // 加载审计记录：有查询走 searchAuditRecords,空查询走 listAuditRecords。
  // debouncedQuery 变化即触发(输入即搜索,经防抖);首次挂载时为空,等价于列表加载。
  // 当按状态筛选(非全部)时,过滤在客户端进行,故请求更大的 limit 以免计数不全。
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    const q = debouncedQuery.trim();
    const limit = filter !== "all" ? 500 : 100;
    const load = q ? searchAuditRecords(q, limit) : listAuditRecords(limit);
    load
      .then((rs) => {
        if (!cancelled) setRecords(rs);
      })
      .catch((e) => {
        if (!cancelled) {
          setRecords([]);
          onNotify?.("danger", `加载审计失败: ${errMsg(e)}`);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // onNotify 由父级稳定提供,无需纳入依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debouncedQuery, filter]);

  // 导出全部审计为 JSON 文件；取消保存对话框时不报错。
  async function handleExport() {
    setExporting(true);
    try {
      const exported = await exportAuditJson();
      if (exported) onNotify?.("success", "审计 JSON 已导出");
    } catch (e) {
      onNotify?.("danger", `导出失败: ${errMsg(e)}`);
    } finally {
      setExporting(false);
    }
  }

  // 前端按状态过滤(failed 同时涵盖 blocked,视为未成功)。
  const shown = records.filter((r) => {
    if (filter === "all") return true;
    if (filter === "completed") return r.status === "completed";
    return r.status !== "completed";
  });

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      {/* 顶部工具行：搜索 + 状态筛选 + 导出 */}
      <div className="flex flex-none items-center gap-2.5 border-b border-border px-6 py-2.5">
        {/* 搜索框：左侧 Search 图标 + 留出内边距给文本 */}
        <div className="relative max-w-xs flex-1">
          <Search
            size={14}
            className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-subtle"
          />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索意图 / 总结 / 命令…"
            className="pl-8"
          />
        </div>
        <div className="flex items-center gap-0.5 rounded-md bg-surface-2 p-0.5">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              onClick={() => setFilter(f.key)}
              className={`rounded px-2.5 py-1 text-[12.5px] transition-colors ${
                filter === f.key ? "bg-surface-1 text-fg shadow-sm" : "text-fg-muted hover:text-fg"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
        <Button
          variant="secondary"
          size="sm"
          className="ml-auto gap-1.5"
          onClick={handleExport}
          disabled={exporting}
        >
          <Download size={14} />
          {exporting ? "导出中…" : "导出全部 JSON"}
        </Button>
      </div>

      {/* 列表区 */}
      <div className="cx-scroll min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[680px] px-6 pb-6 pt-5">
          <h2 className="mb-3 text-sm font-semibold">审计记录</h2>
          {/* 状态筛选在客户端进行,且加载上限为 500 条;命中上限时提示结果可能不完整。 */}
          {!loading && filter !== "all" && records.length >= 500 && (
            <div className="mb-3 rounded-md border border-risk-medium/40 bg-risk-medium-soft px-3 py-2 text-[12px] text-risk-medium">
              仅基于最近 500 条审计进行筛选,更早的记录未纳入;请用搜索或「导出全部 JSON」查看完整数据。
            </div>
          )}
          {loading ? (
            <div className="flex items-center justify-center gap-2 rounded-md border border-border bg-surface-1 px-4 py-6 text-[13px] text-fg-subtle">
              <Spinner size="sm" /> 加载中…
            </div>
          ) : shown.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 rounded-md border border-border bg-surface-1 px-4 py-10 text-center text-[13px] text-fg-subtle">
              <ScrollText size={24} strokeWidth={1.75} className="text-fg-subtle" />
              {debouncedQuery.trim() || filter !== "all"
                ? "没有匹配的审计记录。"
                : "还没有审计记录。执行一次任务后会出现在这里。"}
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {shown.map((r) => {
                const open = r.id === openId;
                const ok = r.status === "completed";
                return (
                  <div key={r.id} className="overflow-hidden rounded-md border border-border bg-surface-1">
                    <div
                      onClick={() => setOpenId(open ? null : r.id)}
                      className="flex cursor-pointer items-center gap-3 px-4 py-3 transition-colors hover:bg-hover"
                    >
                      <span className={`h-1.5 w-1.5 rounded-full ${ok ? "bg-risk-low" : "bg-risk-blocked"}`} />
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[13.5px] font-medium">{r.intent}</div>
                        {r.summary ? (
                          <div className="truncate text-[12px] text-fg-muted">{r.summary}</div>
                        ) : null}
                      </div>
                      <span className="flex-none text-[11.5px] text-fg-subtle">
                        {STATUS_LABEL[r.status] ?? r.status}
                      </span>
                      <time
                        className="flex-none font-mono text-[11px] text-fg-subtle"
                        title={new Date(r.createdAt).toLocaleString()}
                      >
                        {formatRelativeTime(r.createdAt)}
                      </time>
                      {/* 展开/收起指示图标 */}
                      {open ? (
                        <ChevronDown size={15} className="flex-none text-fg-subtle" />
                      ) : (
                        <ChevronRight size={15} className="flex-none text-fg-subtle" />
                      )}
                    </div>
                    {open && (
                      <div className="border-t border-border px-4 py-3">
                        {r.executions.length === 0 ? (
                          <div className="text-[12px] text-fg-subtle">无命令执行记录</div>
                        ) : (
                          <div className="flex flex-col gap-2">
                            {r.executions.map((ex, i) => (
                              <div key={i} className="rounded-md bg-bg">
                                <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 font-mono text-[11.5px] text-fg-subtle">
                                  <span>$ {ex.command}</span>
                                  <span
                                    className={`ml-auto ${ex.exitCode === 0 ? "text-risk-low" : "text-risk-blocked"}`}
                                  >
                                    {executionStatusLabel(ex)}
                                  </span>
                                </div>
                                {ex.stdout || ex.stderr ? (
                                  (() => {
                                    const all = executionOutput(ex);
                                    return (
                                      <div className="overflow-x-auto px-3 py-2 font-mono text-[11.5px] leading-relaxed">
                                        {all.slice(0, 12).map((line, idx) => (
                                          <div key={idx} className={line.stderr ? "text-risk-blocked" : "text-fg"}>
                                            {line.stderr ? "stderr: " : ""}
                                            {line.text}
                                          </div>
                                        ))}
                                        {all.length > 12 ? (
                                          <div className="text-fg-subtle">…（已截断，共 {all.length} 行）</div>
                                        ) : null}
                                      </div>
                                    );
                                  })()
                                ) : null}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
