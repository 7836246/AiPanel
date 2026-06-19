import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { Select } from "./Select";

afterEach(cleanup);

const OPTS = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Bravo" },
  { value: "c", label: "Charlie" },
];

describe("Select", () => {
  it("按钮显示占位 / 当前项", () => {
    const { rerender } = render(<Select value="" options={OPTS} onChange={() => {}} placeholder="选一个" />);
    expect(screen.getByRole("button").textContent).toContain("选一个");
    rerender(<Select value="b" options={OPTS} onChange={() => {}} placeholder="选一个" />);
    expect(screen.getByRole("button").textContent).toContain("Bravo");
  });

  it("点击选项触发 onChange 并收起", () => {
    const onChange = vi.fn();
    render(<Select value="" options={OPTS} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button")); // 展开
    fireEvent.click(screen.getByText("Charlie"));
    expect(onChange).toHaveBeenCalledWith("c");
    expect(screen.queryByRole("listbox")).toBeNull(); // 已收起
  });

  it("方向键移动高亮 + Enter 选中", () => {
    const onChange = vi.fn();
    render(<Select value="a" options={OPTS} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button")); // 展开,高亮落到当前项 Alpha(0)
    fireEvent.keyDown(window, { key: "ArrowDown" }); // -> Bravo(1)
    fireEvent.keyDown(window, { key: "ArrowDown" }); // -> Charlie(2)
    fireEvent.keyDown(window, { key: "ArrowUp" }); //   -> Bravo(1)
    fireEvent.keyDown(window, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("b");
  });

  it("Escape 收起", () => {
    render(<Select value="" options={OPTS} onChange={() => {}} />);
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByRole("listbox")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});
