import { defineConfig } from "vitest/config";

// 设计系统组件测试:jsdom 环境 + @testing-library/react 渲染。
export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
  },
});
