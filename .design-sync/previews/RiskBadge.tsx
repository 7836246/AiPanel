import { RiskBadge } from "@aipanel/ui";

export function AllLevels() {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <RiskBadge level="low" />
      <RiskBadge level="medium" />
      <RiskBadge level="high" />
      <RiskBadge level="blocked" />
    </div>
  );
}

export function InStep() {
  return (
    <div className="flex items-center gap-2 font-mono text-xs text-fg">
      <span>systemctl restart nginx</span>
      <RiskBadge level="medium" />
    </div>
  );
}
