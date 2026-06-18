// 测试初始化:某些 node 版本下 jsdom 未启用 localStorage;提供一个内存实现,
// 保证 settingsKeys 等依赖 localStorage 的纯逻辑测试确定可跑。
if (typeof globalThis.localStorage === "undefined") {
  const store = new Map<string, string>();
  const mem: Storage = {
    getItem: (k) => (store.has(k) ? store.get(k)! : null),
    setItem: (k, v) => {
      store.set(k, String(v));
    },
    removeItem: (k) => {
      store.delete(k);
    },
    clear: () => {
      store.clear();
    },
    key: (i) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", { value: mem, configurable: true });
}
