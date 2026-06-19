import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useToasts } from "./Toast";

describe("useToasts", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("push 添加一条、dismiss 手动移除", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.push("info", "你好"));
    expect(result.current.toasts).toHaveLength(1);
    expect(result.current.toasts[0].message).toBe("你好");
    expect(result.current.toasts[0].tone).toBe("info");

    const id = result.current.toasts[0].id;
    act(() => result.current.dismiss(id));
    expect(result.current.toasts).toHaveLength(0);
  });

  it("到点自动消失", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.push("success", "完成"));
    expect(result.current.toasts).toHaveLength(1);
    act(() => vi.advanceTimersByTime(10_000)); // 超过自动消失时长
    expect(result.current.toasts).toHaveLength(0);
  });

  it("多条通知拥有不同 id", () => {
    const { result } = renderHook(() => useToasts());
    act(() => {
      result.current.push("info", "a");
      result.current.push("danger", "b");
    });
    expect(result.current.toasts).toHaveLength(2);
    expect(result.current.toasts[0].id).not.toBe(result.current.toasts[1].id);
  });
});
