import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { Dialog } from "./Dialog";

afterEach(cleanup);

describe("Dialog", () => {
  it("open=false 时不渲染", () => {
    render(
      <Dialog open={false} onClose={() => {}} title="标题">
        正文
      </Dialog>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("open 时渲染标题/正文/默认关闭按钮", () => {
    render(
      <Dialog open onClose={() => {}} title="标题" description="说明">
        正文内容
      </Dialog>,
    );
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByText("标题")).toBeTruthy();
    expect(screen.getByText("说明")).toBeTruthy();
    expect(screen.getByText("正文内容")).toBeTruthy();
    expect(screen.getByRole("button", { name: "关闭" })).toBeTruthy();
  });

  it("按 Esc 触发 onClose", () => {
    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} title="标题">
        正文
      </Dialog>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("点遮罩关闭,点面板内部不关闭", () => {
    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} title="标题">
        正文
      </Dialog>,
    );
    fireEvent.click(screen.getByText("正文")); // 面板内 → stopPropagation,不关闭
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("dialog")); // 遮罩 → 关闭
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("默认「关闭」按钮触发 onClose", () => {
    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} title="标题">
        正文
      </Dialog>,
    );
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
