import { useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  terminalOpen,
  terminalWrite,
  terminalResize,
  terminalClose,
} from "../lib/api";

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
}: {
  serverId: string;
  serverName: string;
}): JSX.Element {
  // 终端 DOM 挂载点
  const containerRef = useRef<HTMLDivElement>(null);
  // 重连计数：自增即触发 effect 重跑，从而卸载并重建终端
  const [reconnectKey, setReconnectKey] = useState(0);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // 标记组件是否仍然挂载，防止异步回调在卸载后继续操作
    let disposed = false;
    // 会话 id：terminalOpen 成功后写入；用本地 let 闭包变量防止竞态
    let sessionId: string | null = null;

    // 依据当前主题（读取 CSS 变量）生成 xterm 配色，读不到则回退到中性深色
    const rootStyle = getComputedStyle(document.documentElement);
    const bg = rootStyle.getPropertyValue("--color-bg").trim();
    const fg = rootStyle.getPropertyValue("--color-fg").trim();
    const surface1 = rootStyle.getPropertyValue("--color-surface-1").trim();
    const theme = {
      background: bg || surface1 || "#141414",
      foreground: fg || "#ececec",
      cursor: fg || "#ececec",
    };

    // 创建终端实例
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "var(--font-mono), ui-monospace, monospace",
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
      term.write(d);
    })
      .then((id) => {
        if (disposed) {
          // 打开成功时组件已卸载：立即关闭会话，避免泄漏
          void terminalClose(id);
          return;
        }
        sessionId = id;
      })
      .catch((err) => {
        if (disposed) return;
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
      {/* 终端挂载容器：填满剩余空间 */}
      <div ref={containerRef} className="min-h-0 flex-1 p-1.5" />
    </div>
  );
}
