import { useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { Loader2, XCircle } from "lucide-react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  terminalOpen,
  terminalWrite,
  terminalResize,
  terminalClose,
} from "../lib/api";

// 连接 banner 的阶段：connecting 显示「正在连接…」、error 显示红色失败原因、done 隐藏。
type BannerPhase =
  | { phase: "connecting" }
  | { phase: "error"; reason: string }
  | { phase: "done" };

/**
 * 交互式终端会话组件。
 *
 * 在指定服务器上打开一个 PTY 会话，把用户输入流式发送到后端，
 * 并把后端输出实时写回 xterm。容器尺寸变化时自动 fit 并同步远端窗口大小。
 *
 * 父级需要给本组件一个撑满高度的容器（例如 flex-1），
 * 因为内部使用 h-full 填满父容器。
 */
export default function TerminalSession({
  serverId,
  serverName,
  connLabel,
}: {
  serverId: string;
  serverName: string;
  // 可选 user@host 标签：有则 banner 显示「正在连接 user@host…」，无则回退到 serverName。
  connLabel?: string;
}): JSX.Element {
  // 终端 DOM 挂载点
  const containerRef = useRef<HTMLDivElement>(null);
  // 重连计数：自增即触发 effect 重跑，从而卸载并重建终端
  const [reconnectKey, setReconnectKey] = useState(0);
  // 连接 banner 状态：初始为 connecting；首批输出/打开成功后置 done；失败置 error。
  const [banner, setBanner] = useState<BannerPhase>({ phase: "connecting" });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // 每次重建（含重连）回到连接中状态，重新展示 banner。
    setBanner({ phase: "connecting" });

    // 标记组件是否仍然挂载，防止异步回调在卸载后继续操作
    let disposed = false;
    // 隐藏 banner 的延时句柄：成功打开后短暂延时再隐藏，避免一闪而过。
    let hideTimer: ReturnType<typeof setTimeout> | null = null;
    // 仅在首次收到输出时隐藏一次 banner 的标记。
    let bannerSettled = false;
    // 收到首批输出或打开成功后调用：隐藏「正在连接」banner。
    const settleBanner = (): void => {
      if (disposed || bannerSettled) return;
      bannerSettled = true;
      setBanner({ phase: "done" });
    };
    // 会话 id：terminalOpen 成功后写入；用本地 let 闭包变量防止竞态
    let sessionId: string | null = null;

    // 依据当前主题（读取 CSS 变量）生成 xterm 配色，读不到则回退到中性深色
    const rootStyle = getComputedStyle(document.documentElement);
    const bg = rootStyle.getPropertyValue("--color-bg").trim();
    const fg = rootStyle.getPropertyValue("--color-fg").trim();
    const surface1 = rootStyle.getPropertyValue("--color-surface-1").trim();
    const selected = rootStyle.getPropertyValue("--color-selected").trim();
    const theme = {
      background: bg || surface1 || "#141414",
      foreground: fg || "#ececec",
      cursor: fg || "#ececec",
      // 选区底色读自 token,保证浅/深主题下选中文本都清晰。
      ...(selected ? { selectionBackground: selected } : {}),
    };

    // xterm 通过 canvas 测量字形,无法解析 CSS var();把 --font-mono 解析成具体字体栈。
    const monoVar = rootStyle.getPropertyValue("--font-mono").trim();
    const fontFamily = `${monoVar ? `${monoVar}, ` : ""}ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`;

    // 创建终端实例
    const term = new Terminal({
      cursorBlink: true,
      fontFamily,
      fontSize: 13,
      theme,
    });

    // 加载 fit 插件，挂载到容器并做一次初始 fit
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    try {
      fit.fit();
    } catch {
      /* 容器尚未布局完成时 fit 可能抛错，忽略 */
    }

    // 在服务器上打开 PTY 会话，输出回调直接写回终端
    void terminalOpen(serverId, term.cols, term.rows, (d) => {
      // 卸载后不再写入
      if (disposed) return;
      // 第一次收到数据即认为会话已活，隐藏连接 banner。
      settleBanner();
      term.write(d);
    })
      .then((id) => {
        if (disposed) {
          // 打开成功时组件已卸载：立即关闭会话，避免泄漏
          void terminalClose(id);
          return;
        }
        sessionId = id;
        // 打开成功但可能尚无输出：短暂延时后兜底隐藏 banner。
        hideTimer = setTimeout(settleBanner, 400);
      })
      .catch((err) => {
        if (disposed) return;
        // 打开失败：banner 转红显示原因，同时在终端内打印错误。
        setBanner({ phase: "error", reason: String(err) });
        term.write(`\r\n\x1b[31m无法打开终端会话: ${String(err)}\x1b[0m\r\n`);
      });

    // 用户输入 → 发送到远端
    const onDataDisposable = term.onData((d) => {
      if (sessionId) void terminalWrite(sessionId, d);
    });

    // 终端尺寸变化 → 同步远端窗口大小
    const onResizeDisposable = term.onResize(({ cols, rows }) => {
      if (sessionId) void terminalResize(sessionId, cols, rows);
    });

    // 容器尺寸变化时重新 fit（fit 会触发 onResize，从而同步远端）
    const resizeObserver = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        /* 忽略瞬态布局错误 */
      }
    });
    resizeObserver.observe(container);

    // 卸载清理
    return () => {
      disposed = true;
      if (hideTimer) clearTimeout(hideTimer);
      onDataDisposable.dispose();
      onResizeDisposable.dispose();
      resizeObserver.disconnect();
      if (sessionId) void terminalClose(sessionId);
      term.dispose();
    };
    // reconnectKey 变化时重建终端实现「重连」
  }, [serverId, reconnectKey]);

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      {/* 细标题栏：服务器名 + 重连按钮 */}
      <div className="flex items-center justify-between border-b border-border px-3 py-1.5 text-xs text-fg-muted">
        <span className="truncate">{serverName}</span>
        <button
          type="button"
          onClick={() => setReconnectKey((k) => k + 1)}
          className="rounded px-2 py-0.5 text-xs text-fg hover:bg-surface-2"
        >
          重连
        </button>
      </div>

      {/* 连接 banner：连接中(spinner) / 失败(红色原因)；done 时不渲染 */}
      {banner.phase !== "done" && (
        <div
          className={`flex items-center gap-1.5 border-b border-border px-3 py-1.5 text-[12px] ${
            banner.phase === "error"
              ? "bg-risk-blocked-soft text-risk-blocked"
              : "bg-hover text-fg-muted"
          }`}
        >
          {banner.phase === "connecting" ? (
            <>
              <Loader2 size={13} className="flex-none animate-spin" />
              正在连接 {connLabel ?? serverName}…
            </>
          ) : (
            <>
              <XCircle size={13} className="flex-none" />
              连接失败：{banner.reason}
            </>
          )}
        </div>
      )}

      {/* 终端挂载容器：填满剩余空间 */}
      <div ref={containerRef} className="min-h-0 flex-1 p-1.5" />
    </div>
  );
}
