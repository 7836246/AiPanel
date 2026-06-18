import { describe, it, expect } from "vitest";
import { formatBytes, formatRate, formatUptime } from "./format";

describe("formatBytes", () => {
  it("0 / 负数 / 非有限数 → 0 B", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(NaN)).toBe("0 B");
    expect(formatBytes(Infinity)).toBe("0 B");
  });
  it("字节不带小数,其余保留 1 位", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatBytes(3.5 * 1024 * 1024 * 1024)).toBe("3.5 GB");
  });
});

describe("formatRate", () => {
  it("从 KB/s 起步,负数/非有限 → 0 KB/s", () => {
    expect(formatRate(-1)).toBe("0 KB/s");
    expect(formatRate(NaN)).toBe("0 KB/s");
    expect(formatRate(0)).toBe("0.0 KB/s");
    expect(formatRate(2048)).toBe("2.0 KB/s");
  });
  it(">=100 时不带小数,会进位到 MB/s", () => {
    expect(formatRate(1024 * 1024)).toBe("1.0 MB/s");
    expect(formatRate(1024 * 200)).toBe("200 KB/s"); // >=100 → 0 位小数
  });
});

describe("formatUptime", () => {
  it("0 / 非有限 → —", () => {
    expect(formatUptime(0)).toBe("—");
    expect(formatUptime(NaN)).toBe("—");
  });
  it("分 / 小时 / 天 分级", () => {
    expect(formatUptime(120)).toBe("2分");
    expect(formatUptime(3600 + 600)).toBe("1小时 10分");
    expect(formatUptime(86400 * 2 + 3600 * 3)).toBe("2天 3小时");
  });
});
