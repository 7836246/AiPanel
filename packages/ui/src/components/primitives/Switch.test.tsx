import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { Switch } from "./Switch";

afterEach(cleanup);

describe("Switch", () => {
  it("role=switch 且 aria-checked 反映 checked", () => {
    render(<Switch checked onChange={() => {}} aria-label="开关" />);
    expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("true");
  });

  it("点击切换为 !checked", () => {
    const onChange = vi.fn();
    render(<Switch checked={false} onChange={onChange} aria-label="开关" />);
    fireEvent.click(screen.getByRole("switch"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("disabled 时点击不触发 onChange", () => {
    const onChange = vi.fn();
    render(<Switch checked={false} onChange={onChange} disabled aria-label="开关" />);
    fireEvent.click(screen.getByRole("switch"));
    expect(onChange).not.toHaveBeenCalled();
  });
});
