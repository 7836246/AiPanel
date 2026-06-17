import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// 前端入口：把 App 挂载到 #root，开发期用 StrictMode 暴露潜在问题。
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
