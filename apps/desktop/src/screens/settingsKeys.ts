export const READONLY_DEFAULT_KEY = "aipanel-readonly-default";

/** 启动时静默检查更新的开关(默认开启;仅显式存为 "false" 时关闭)。 */
export const UPDATE_AUTOCHECK_KEY = "aipanel-update-autocheck";

/** 读取「启动检查更新」当前值(缺省 true)。 */
export function readUpdateAutoCheck(): boolean {
  try {
    return localStorage.getItem(UPDATE_AUTOCHECK_KEY) !== "false";
  } catch {
    return true;
  }
}

/** 持久化「启动检查更新」开关。 */
export function writeUpdateAutoCheck(value: boolean): void {
  try {
    localStorage.setItem(UPDATE_AUTOCHECK_KEY, value ? "true" : "false");
  } catch {
    // 隐私模式等场景 localStorage 不可写,静默忽略。
  }
}

/** 最近访问的服务器 id 列表(最近在前),供命令面板置顶最近用过的服务器。 */
export const RECENT_SERVERS_KEY = "aipanel-recent-servers";
const RECENT_SERVERS_MAX = 5;

/** 读取最近访问的服务器 id 列表(最近在前;非数组/异常时返回空)。 */
export function readRecentServers(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_SERVERS_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/** 记录一次服务器访问:置顶去重,最多保留 RECENT_SERVERS_MAX 个,返回更新后的列表。 */
export function recordRecentServer(id: string): string[] {
  if (!id) return readRecentServers();
  const next = [id, ...readRecentServers().filter((x) => x !== id)].slice(0, RECENT_SERVERS_MAX);
  try {
    localStorage.setItem(RECENT_SERVERS_KEY, JSON.stringify(next));
  } catch {
    // localStorage 不可写时仅退化为不持久化。
  }
  return next;
}
