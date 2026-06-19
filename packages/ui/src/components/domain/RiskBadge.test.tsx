import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { RiskBadge } from "./RiskBadge";
import { RISK_META, type RiskLevel } from "./risk";

afterEach(cleanup);

describe("RiskBadge", () => {
  it("四个等级渲染各自文案(对应安全模型)", () => {
    const levels: RiskLevel[] = ["low", "medium", "high", "blocked"];
    for (const lvl of levels) {
      const { unmount } = render(<RiskBadge level={lvl} />);
      expect(screen.getByText(RISK_META[lvl].label)).toBeTruthy();
      unmount();
    }
  });

  it("套用对应等级的语气色 class", () => {
    render(<RiskBadge level="blocked" />);
    expect(screen.getByText("Blocked").className).toContain("text-risk-blocked");
  });

  it("透传额外 className", () => {
    render(<RiskBadge level="low" className="custom-x" />);
    expect(screen.getByText("Low").className).toContain("custom-x");
  });
});
