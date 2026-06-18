import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { applyAppearance, readAppearance } from "./lib/appearance";

// 在 React 渲染前就应用外观偏好(主题/强调色/动效/光标),避免启动时先闪浅色再切到深色(FOUC)。
applyAppearance(readAppearance());

// 前端入口：把 App 挂载到 #root，开发期用 StrictMode 暴露潜在问题。
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
