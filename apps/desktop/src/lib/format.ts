/**
 * 通用格式化纯函数(无 I/O、无副作用,便于单测)。监控等界面共用。
 */

/** 把字节数格式化成人类可读（B/KB/MB/GB/TB，1024 进制，保留 1 位小数）。 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  // 字节本身不带小数，其余保留 1 位。
  return `${i === 0 ? n : n.toFixed(1)} ${units[i]}`;
}

/** 把「字节/秒」格式化成速率字符串（KB/s、MB/s…）。 */
export function formatRate(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec < 0) return "0 KB/s";
  // 速率最小以 KB/s 起步展示，贴近运维面板习惯。
  const units = ["KB/s", "MB/s", "GB/s"];
  let n = bytesPerSec / 1024;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  return `${n.toFixed(n >= 100 ? 0 : 1)} ${units[i]}`;
}

/** 把秒数格式化成「Xd Yh Zm」运行时长。 */
export function formatUptime(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "—";
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}天 ${h}小时`;
  if (h > 0) return `${h}小时 ${m}分`;
  return `${m}分`;
}
