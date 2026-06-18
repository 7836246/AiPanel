/**
 * 时间格式化纯函数(无副作用,便于单测)。
 */

/**
 * 把 ISO 时间格式化成相对「刚刚 / x 分钟前 / x 小时前 / x 天前」,超过一周回退到绝对日期。
 * `nowMs` 可注入,便于测试;默认取当前时间。非法输入返回空串。
 */
export function formatRelativeTime(iso: string, nowMs: number = Date.now()): string {
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return "";
  const diff = nowMs - t;
  if (diff < 60_000) return "刚刚"; // 含轻微的未来/时钟漂移
  const min = Math.floor(diff / 60_000);
  if (min < 60) return `${min} 分钟前`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} 小时前`;
  const day = Math.floor(hr / 24);
  if (day < 7) return `${day} 天前`;
  return new Date(t).toLocaleDateString();
}
