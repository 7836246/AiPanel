import { describe, it, expect } from "vitest";
import { formatRelativeTime } from "./time";

describe("formatRelativeTime", () => {
  const now = new Date("2026-06-19T12:00:00Z").getTime();
  const ago = (ms: number) => new Date(now - ms).toISOString();

  it("非法输入 → 空串", () => {
    expect(formatRelativeTime("not-a-date", now)).toBe("");
  });
  it("一分钟内 / 未来 → 刚刚", () => {
    expect(formatRelativeTime(ago(5_000), now)).toBe("刚刚");
    expect(formatRelativeTime(new Date(now + 10_000).toISOString(), now)).toBe("刚刚");
  });
  it("分钟 / 小时 / 天 分级", () => {
    expect(formatRelativeTime(ago(5 * 60_000), now)).toBe("5 分钟前");
    expect(formatRelativeTime(ago(3 * 3600_000), now)).toBe("3 小时前");
    expect(formatRelativeTime(ago(2 * 86400_000), now)).toBe("2 天前");
  });
  it("超过一周 → 绝对日期(非相对文案)", () => {
    const s = formatRelativeTime(ago(10 * 86400_000), now);
    expect(s).not.toContain("前");
    expect(s.length).toBeGreaterThan(0);
  });
});
