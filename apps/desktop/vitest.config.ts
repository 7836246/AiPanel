import { defineConfig } from "vitest/config";

// 前端单测:仅覆盖纯逻辑 / 非 Tauri mock 路径。jsdom 提供 localStorage、window 等(window
// 存在但无 __TAURI_INTERNALS__,故 isTauri() 为 false,api.ts 走 mock 分支)。
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["./src/test-setup.ts"],
  },
});
