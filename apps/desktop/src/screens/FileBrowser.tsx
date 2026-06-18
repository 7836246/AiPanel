import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from "react";
import {
  ChevronRight,
  CornerLeftUp,
  Download,
  File as FileIcon,
  FileCode,
  FileText,
  Folder,
  FolderOpen,
  RefreshCw,
  Save,
  Search,
  Upload,
} from "lucide-react";
import { Button, IconButton, Input, Spinner } from "@aipanel/ui";
import {
  fsList,
  fsRead,
  fsWrite,
  fsUpload,
  fsDownload,
  type DirListing,
  type FileEntry,
  type FileContent,
} from "../lib/api";

/* ---------------- 工具函数 ---------------- */

// 从后端错误或任意异常中提取可展示的文本（与 CodexConsole 保持一致风格）。
const errMsg = (e: unknown): string =>
  e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);

// 把字节数格式化为人类可读的大小（目录不显示）。
function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

// 修改时间尽量友好展示：能解析成日期则本地化，否则原样返回（兼容原始 ls 时间戳）。
function formatMtime(mtime: string): string {
  if (!mtime) return "";
  const t = Date.parse(mtime);
  if (Number.isNaN(t)) return mtime;
  const d = new Date(t);
  const now = new Date();
  const sameYear = d.getFullYear() === now.getFullYear();
  const pad = (n: number) => String(n).padStart(2, "0");
  const md = `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return sameYear ? md : `${d.getFullYear()}-${md}`;
}

// 取文件扩展名（小写，无点）。
function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

// 按扩展名给文件挑选 lucide 图标：.md → FileText；代码/配置类 → FileCode；其它 → File。
const CODE_EXTS = new Set(["json", "js", "jsx", "ts", "tsx", "rs", "yaml", "yml", "toml", "sh", "py", "go", "c", "h", "cpp", "java", "rb", "php", "css", "html", "xml", "conf", "ini"]);
function fileGlyph(name: string): JSX.Element {
  const ext = extOf(name);
  if (ext === "md" || ext === "markdown" || ext === "txt" || ext === "log") return <FileText size={15} strokeWidth={1.75} className="flex-none text-fg-subtle" />;
  if (CODE_EXTS.has(ext)) return <FileCode size={15} strokeWidth={1.75} className="flex-none text-fg-subtle" />;
  return <FileIcon size={15} strokeWidth={1.75} className="flex-none text-fg-subtle" />;
}

// 规范化路径拼接：基于当前目录进入子项 / 回到上一级，避免出现 "//" 或末尾斜杠。
function joinPath(dir: string, name: string): string {
  if (dir === "." || dir === "") return name; // 家目录相对路径起点
  return dir.endsWith("/") ? `${dir}${name}` : `${dir}/${name}`;
}
function parentPath(dir: string): string | null {
  if (dir === "." || dir === "" || dir === "~") return null; // 已在起点（家目录），无上一级
  if (dir === "/") return null;
  const trimmed = dir.replace(/\/+$/, "");
  const i = trimmed.lastIndexOf("/");
  if (i < 0) return "."; // 形如 "foo/bar" 相对家目录的单段，退回家目录
  if (i === 0) return "/"; // 形如 "/etc"，退回根
  return trimmed.slice(0, i);
}

// 先文件夹后文件，再按名称排序（链接归入文件，与真实文件面板一致）。
function sortEntries(entries: FileEntry[]): FileEntry[] {
  return [...entries].sort((a, b) => {
    const ad = a.kind === "dir" ? 0 : 1;
    const bd = b.kind === "dir" ? 0 : 1;
    if (ad !== bd) return ad - bd;
    return a.name.localeCompare(b.name);
  });
}

/* ---------------- 主组件 ---------------- */

// 文件管理面板（Codex 冷白风格，紧凑密度）：左侧目录列表 + 右侧文本编辑器。
// 不自己拉服务器，所有数据通过 props（serverId/serverName）+ api（fsList/fsRead/fsWrite）。
export default function FileBrowser({ serverId, serverName }: { serverId: string; serverName: string }): JSX.Element {
  const [path, setPath] = useState("."); // 初始为家目录
  const [listing, setListing] = useState<DirListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 上传/下载等动作的错误走独立通道，不写入会顶替整个目录列表的 error。
  const [actionError, setActionError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  // 右侧编辑器状态。
  const [openFile, setOpenFile] = useState<string | null>(null); // 当前打开文件的完整路径
  const [fileLoading, setFileLoading] = useState(false);
  const [fileError, setFileError] = useState<string | null>(null);
  const [content, setContent] = useState(""); // 编辑器当前内容
  const [savedContent, setSavedContent] = useState(""); // 最近一次磁盘内容（用于判断是否脏）
  const [truncated, setTruncated] = useState(false);
  const [saving, setSaving] = useState(false);
  const [justSaved, setJustSaved] = useState(false); // 保存成功后短暂显示「已保存 ✓」
  const savedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 单调递增请求序号：切目录/切文件后丢弃过期的异步结果，避免竞态覆盖。
  const dirReqRef = useRef(0);
  const fileReqRef = useRef(0);

  // 加载某个目录（列目录）。
  const loadDir = useCallback(
    async (target: string) => {
      const reqId = ++dirReqRef.current;
      setLoading(true);
      setError(null);
      try {
        const res = await fsList(serverId, target);
        if (dirReqRef.current !== reqId) return; // 已切走，丢弃
        setListing(res);
        setPath(res.path || target); // 以后端返回的规范路径为准
      } catch (e) {
        if (dirReqRef.current !== reqId) return;
        setError(errMsg(e));
        setListing({ path: target, entries: [] });
      } finally {
        if (dirReqRef.current === reqId) setLoading(false);
      }
    },
    [serverId]
  );

  // 服务器或首次挂载时加载家目录；切换服务器时一并清空已打开文件。
  useEffect(() => {
    setOpenFile(null);
    setContent("");
    setSavedContent("");
    setQuery("");
    loadDir(".");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverId]);

  // 卸载时清理「已保存」提示的定时器，避免内存泄漏 / 卸载后 setState。
  useEffect(() => {
    return () => {
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
    };
  }, []);

  // 打开（读取）一个文件到右侧编辑器。
  async function openFileAt(fullPath: string) {
    const reqId = ++fileReqRef.current;
    setOpenFile(fullPath);
    setFileLoading(true);
    setFileError(null);
    setTruncated(false);
    setJustSaved(false); // 切换文件时清掉残留的「已保存」提示
    try {
      const res: FileContent = await fsRead(serverId, fullPath);
      if (fileReqRef.current !== reqId) return;
      setContent(res.content);
      setSavedContent(res.content);
      setTruncated(res.truncated);
    } catch (e) {
      if (fileReqRef.current !== reqId) return;
      setFileError(errMsg(e));
      setContent("");
      setSavedContent("");
    } finally {
      if (fileReqRef.current === reqId) setFileLoading(false);
    }
  }

  // 保存编辑器内容回远端。
  async function save() {
    // 无改动也纳入提前返回，避免 ⌘S 触发冗余的远端写。
    if (!openFile || saving || truncated || content === savedContent) return;
    setSaving(true);
    setFileError(null);
    try {
      await fsWrite(serverId, openFile, content);
      setSavedContent(content); // 标记为已保存（不再脏）
      // 轻量正反馈：编辑器头部短暂显示「已保存 ✓」，几秒后自动消失。
      setJustSaved(true);
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
      savedTimerRef.current = setTimeout(() => setJustSaved(false), 2000);
    } catch (e) {
      setFileError(errMsg(e));
    } finally {
      setSaving(false);
    }
  }

  // 点击目录项：文件夹进入，文件打开。
  const parent = parentPath(path);
  const dirty = openFile !== null && content !== savedContent;

  // 有未保存改动时,切文件/切目录前先确认,避免静默丢弃远端文件的编辑。
  function confirmDiscard(): boolean {
    return !dirty || window.confirm("有未保存的修改,确定放弃吗?");
  }

  function onEntryClick(entry: FileEntry) {
    if (!confirmDiscard()) return;
    const full = joinPath(path, entry.name);
    if (entry.kind === "dir") {
      setQuery(""); // 进入新目录时清空过滤词
      loadDir(full);
    } else {
      openFileAt(full);
    }
  }

  // 上传:选本地文件 → scp 到当前目录 → 刷新列表。
  async function handleUpload() {
    setActionError(null);
    try {
      const name = await fsUpload(serverId, path);
      if (name) loadDir(path);
    } catch (e) {
      // 用独立的 actionError 提示，避免顶替目录列表导致文件看起来全没了。
      setActionError(`上传失败: ${errMsg(e)}`);
    }
  }

  // 下载:把某个远端文件 scp 到本地(弹保存对话框)。
  async function handleDownload(entry: FileEntry) {
    setActionError(null);
    try {
      await fsDownload(serverId, joinPath(path, entry.name));
    } catch (e) {
      setActionError(`下载失败: ${errMsg(e)}`);
    }
  }

  // 当前目录按「先文件夹后文件」排序，并按搜索框过滤名称。
  const visibleEntries = useMemo(() => {
    const sorted = sortEntries(listing?.entries ?? []);
    const q = query.trim().toLowerCase();
    return q ? sorted.filter((e) => e.name.toLowerCase().includes(q)) : sorted;
  }, [listing, query]);

  // 顶部面包屑：把路径切成可点击的层级（家目录起点用 "~" 表示）。
  const crumbs = useMemo(() => {
    if (path === "." || path === "" || path === "~") return [{ label: "~", target: "." }];
    if (path === "/") return [{ label: "/", target: "/" }];
    const abs = path.startsWith("/");
    const segs = path.replace(/\/+$/, "").split("/").filter(Boolean);
    const out: { label: string; target: string }[] = [];
    if (abs) {
      out.push({ label: "/", target: "/" });
      let acc = "";
      for (const s of segs) {
        acc += `/${s}`;
        out.push({ label: s, target: acc });
      }
    } else {
      out.push({ label: "~", target: "." });
      let acc = "";
      for (const s of segs) {
        acc = acc ? `${acc}/${s}` : s;
        out.push({ label: s, target: acc });
      }
    }
    return out;
  }, [path]);

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-bg text-fg" style={{ fontFamily: "var(--font-sans)" }}>
      {/* 顶部路径栏：上一级 + 面包屑 + 刷新 */}
      <div className="flex h-10 flex-none items-center gap-2 border-b border-border px-3.5">
        <IconButton
          aria-label="上一级"
          size="sm"
          disabled={!parent || loading}
          onClick={() => { if (parent && confirmDiscard()) loadDir(parent); }}
          title="上一级目录"
        >
          <CornerLeftUp size={15} />
        </IconButton>
        <div className="cx-scroll flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto whitespace-nowrap text-[12.5px]">
          {crumbs.map((c, i) => (
            <span key={`${c.target}-${i}`} className="inline-flex items-center gap-0.5">
              {i > 0 && <ChevronRight size={13} className="flex-none text-fg-subtle" />}
              {i === crumbs.length - 1 ? (
                // 当前层：非交互 span，避免点自身触发对当前目录的冗余 loadDir。
                <span aria-current="page" className="rounded px-1.5 py-0.5 font-semibold text-fg">
                  {c.label}
                </span>
              ) : (
                <button
                  onClick={() => { if (confirmDiscard()) loadDir(c.target); }}
                  className="rounded px-1.5 py-0.5 text-fg-muted transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/60"
                >
                  {c.label}
                </button>
              )}
            </span>
          ))}
        </div>
        <span className="hidden flex-none text-[11.5px] text-fg-subtle sm:inline" title={serverName}>
          {serverName}
        </span>
        <IconButton aria-label="刷新" size="sm" disabled={loading} onClick={() => loadDir(path)} title="刷新当前目录">
          <RefreshCw size={15} className={loading ? "animate-spin" : undefined} />
        </IconButton>
        <Button
          variant="secondary"
          size="sm"
          disabled={loading}
          onClick={handleUpload}
          title={loading ? "目录加载中…" : "上传本地文件到当前目录"}
        >
          <Upload size={13} /> 上传
        </Button>
      </div>

      {/* 主区：左列表 + 右编辑器 */}
      <div className="flex min-h-0 flex-1">
        {/* 左侧：目录列表 */}
        <div className="flex min-h-0 w-[320px] flex-none flex-col border-r border-border bg-surface-2">
          {/* 搜索框 */}
          <div className="flex-none p-2">
            <div className="relative">
              <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-subtle" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="过滤当前目录…"
                className="h-8 bg-surface-1 pl-8 text-[12.5px]"
              />
            </div>
          </div>

          {/* 上传/下载等动作的错误：非破坏性内联条，不顶替下方目录列表 */}
          {actionError && (
            <div className="m-1.5 flex items-center justify-between gap-2 rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2 text-[12.5px] text-risk-blocked">
              <span className="min-w-0 flex-1 break-words">{actionError}</span>
              <button
                type="button"
                aria-label="关闭"
                title="关闭"
                onClick={() => setActionError(null)}
                className="flex-none rounded px-1 leading-none hover:opacity-70"
              >
                ✕
              </button>
            </div>
          )}

          {/* 列表主体 */}
          <div className="cx-scroll min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
            {loading ? (
              <div className="flex items-center justify-center gap-2 py-10 text-[12.5px] text-fg-subtle">
                <Spinner size="sm" /> 加载中…
              </div>
            ) : error ? (
              <div className="m-1.5 rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2 text-[12.5px] text-risk-blocked">
                <div className="break-words">{error}</div>
                {/* 给目录加载失败一个直接重试入口（重试当前目标目录） */}
                <button
                  type="button"
                  onClick={() => loadDir(path)}
                  className="mt-2 rounded border border-risk-blocked/40 px-2 py-0.5 text-[11.5px] transition-colors hover:bg-risk-blocked/15"
                >
                  重试
                </button>
              </div>
            ) : visibleEntries.length === 0 ? (
              <div className="flex flex-col items-center gap-1.5 px-3 py-10 text-center">
                <FolderOpen size={22} className="text-fg-subtle" strokeWidth={1.75} />
                <div className="text-[12.5px] text-fg-subtle">
                  {query.trim() ? "没有匹配的项目" : "空目录"}
                </div>
              </div>
            ) : (
              visibleEntries.map((entry) => {
                const isOpen = entry.kind !== "dir" && openFile === joinPath(path, entry.name);
                return (
                  <div
                    key={entry.name}
                    onClick={() => onEntryClick(entry)}
                    title={entry.name}
                    className={`group flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13px] transition-colors ${
                      isOpen ? "bg-selected text-fg" : "text-fg-muted hover:bg-hover"
                    }`}
                  >
                    {entry.kind === "dir" ? (
                      <Folder size={15} strokeWidth={1.75} className="flex-none text-risk-medium" />
                    ) : (
                      fileGlyph(entry.name)
                    )}
                    <span className="min-w-0 flex-1 truncate">
                      {entry.name}
                      {entry.kind === "link" ? <span className="text-fg-subtle"> →</span> : null}
                    </span>
                    <span className="flex-none text-[11px] text-fg-subtle">
                      {entry.kind === "dir" ? "" : formatSize(entry.size)}
                    </span>
                    <span className="hidden flex-none text-[11px] text-fg-subtle lg:inline">
                      {formatMtime(entry.mtime)}
                    </span>
                    {entry.kind !== "dir" && (
                      <IconButton
                        aria-label="下载"
                        size="sm"
                        className="flex-none opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                        onClick={(e) => { e.stopPropagation(); void handleDownload(entry); }}
                        title="下载到本地"
                      >
                        <Download size={13} />
                      </IconButton>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* 右侧：文本编辑器 */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-bg">
          {!openFile ? (
            <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-8 text-center">
              <FileText size={26} className="text-fg-subtle" strokeWidth={1.5} />
              <div className="text-[13px] text-fg-subtle">从左侧选择一个文件查看 / 编辑</div>
            </div>
          ) : (
            <>
              {/* 编辑器头部：文件名 + 保存 */}
              <div className="flex h-10 flex-none items-center gap-2 border-b border-border px-3.5">
                <span className="flex-none text-fg-subtle">{fileGlyph(openFile)}</span>
                <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]" title={openFile}>
                  {openFile}
                  {dirty ? <span className="ml-1 text-risk-medium">●</span> : null}
                </span>
                {/* 保存成功后短暂出现的正反馈，不抢占空间（不存在时不渲染） */}
                {justSaved && !dirty && (
                  <span className="flex-none text-[11.5px] text-risk-low">已保存 ✓</span>
                )}
                <Button
                  variant="primary"
                  size="sm"
                  onClick={save}
                  disabled={saving || fileLoading || truncated || !dirty}
                  title={saving ? "保存中…" : fileLoading ? "读取中…" : truncated ? "文件过大被截断,禁止保存以免数据丢失" : !dirty ? "无改动" : undefined}
                >
                  {saving ? <Spinner size="sm" /> : <Save size={13} />} 保存
                </Button>
              </div>

              {/* 文件过大截断提示：禁止保存，避免把磁盘上未读到的内容截掉。 */}
              {truncated && (
                <div className="flex-none border-b border-border bg-risk-medium-soft px-3.5 py-1.5 text-[11.5px] text-risk-medium">
                  文件过大,内容已截断,仅供查看,无法保存。
                </div>
              )}

              {/* 文件读取错误内联提示 */}
              {fileError && (
                <div className="flex-none border-b border-border bg-risk-blocked/10 px-3.5 py-1.5 text-[11.5px] text-risk-blocked">
                  {fileError}
                </div>
              )}

              {/* 编辑区 */}
              <div className="relative min-h-0 flex-1">
                {fileLoading ? (
                  <div className="flex h-full items-center justify-center gap-2 text-[12.5px] text-fg-subtle">
                    <Spinner size="sm" /> 读取中…
                  </div>
                ) : (
                  <textarea
                    value={content}
                    onChange={(e) => setContent(e.target.value)}
                    readOnly={truncated}
                    spellCheck={false}
                    placeholder="（空文件）"
                    onKeyDown={(e) => {
                      // ⌘S / Ctrl-S 保存；对不可保存态给出提示而非完全静默。
                      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
                        e.preventDefault();
                        if (truncated) {
                          setFileError("文件已截断,无法保存。");
                        } else if (!dirty) {
                          setJustSaved(true); // 复用「已保存」反馈表达「无改动」
                          if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
                          savedTimerRef.current = setTimeout(() => setJustSaved(false), 1500);
                        } else {
                          save();
                        }
                      }
                    }}
                    className="absolute inset-0 h-full w-full resize-none border-none bg-bg px-4 py-3 font-mono text-[12.5px] leading-relaxed text-fg outline-none placeholder:text-fg-subtle"
                  />
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
