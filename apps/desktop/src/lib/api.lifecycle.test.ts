import { describe, it, expect } from "vitest";
import {
  listTasks,
  saveTask,
  deleteTask,
  getTask,
  searchTasks,
  listProviders,
  saveProvider,
  deleteProvider,
  type TaskRecord,
} from "./api";

// 非 Tauri(jsdom)下的 mock 增删查生命周期——保证浏览器 dev 模式行为与预期一致。
function mkTask(id: string, serverId: string, title: string): TaskRecord {
  return {
    id,
    serverId,
    title,
    intent: title,
    kind: "plan",
    executions: [],
    status: "completed",
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

describe("task mock 生命周期", () => {
  it("save / list(按服务器过滤) / get / search / delete", async () => {
    await saveTask(mkTask("t-a", "srv-1", "部署 nginx"));
    await saveTask(mkTask("t-b", "srv-1", "排查磁盘"));
    await saveTask(mkTask("t-c", "srv-2", "部署 redis"));

    const srv1 = await listTasks("srv-1");
    expect(srv1.map((t) => t.id).sort()).toEqual(["t-a", "t-b"]);

    expect((await getTask("t-c")).title).toBe("部署 redis");

    const found = await searchTasks("srv-1", "部署");
    expect(found.map((t) => t.id)).toEqual(["t-a"]); // 仅 srv-1 且标题含「部署」

    await deleteTask("t-a");
    expect((await listTasks("srv-1")).map((t) => t.id)).toEqual(["t-b"]);

    // 清理,避免影响其它测试的全局 mock 状态。
    await deleteTask("t-b");
    await deleteTask("t-c");
  });

  it("saveTask 同 id 覆盖且置顶", async () => {
    await saveTask(mkTask("t-x", "srv-9", "旧标题"));
    await saveTask(mkTask("t-y", "srv-9", "另一个"));
    await saveTask({ ...mkTask("t-x", "srv-9", "新标题"), updatedAt: new Date().toISOString() });
    const list = await listTasks("srv-9");
    expect(list[0].id).toBe("t-x"); // 最近保存置顶
    expect(list.find((t) => t.id === "t-x")?.title).toBe("新标题");
    expect(list.filter((t) => t.id === "t-x").length).toBe(1); // 不重复
    await deleteTask("t-x");
    await deleteTask("t-y");
  });
});

describe("provider mock 生命周期", () => {
  it("save 后出现在 list,delete 后消失", async () => {
    await saveProvider(
      { id: "prov-life", name: "测试供应商", kind: "openai_compatible", enabled: true },
      "sk-1",
    );
    expect((await listProviders()).some((p) => p.id === "prov-life")).toBe(true);
    await deleteProvider("prov-life");
    expect((await listProviders()).some((p) => p.id === "prov-life")).toBe(false);
  });
});
