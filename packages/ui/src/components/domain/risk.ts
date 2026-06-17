/** Operation risk levels, mirroring docs/SECURITY_MODEL.zh-Hans.md. */
export type RiskLevel = "low" | "medium" | "high" | "blocked";

export const RISK_META: Record<
  RiskLevel,
  { label: string; text: string; bg: string; border: string; dot: string }
> = {
  low: {
    label: "Low",
    text: "text-risk-low",
    bg: "bg-risk-low-soft",
    border: "border-risk-low/40",
    dot: "bg-risk-low",
  },
  medium: {
    label: "Medium",
    text: "text-risk-medium",
    bg: "bg-risk-medium-soft",
    border: "border-risk-medium/40",
    dot: "bg-risk-medium",
  },
  high: {
    label: "High",
    text: "text-risk-high",
    bg: "bg-risk-high-soft",
    border: "border-risk-high/40",
    dot: "bg-risk-high",
  },
  blocked: {
    label: "Blocked",
    text: "text-risk-blocked",
    bg: "bg-risk-blocked-soft",
    border: "border-risk-blocked/40",
    dot: "bg-risk-blocked",
  },
};
