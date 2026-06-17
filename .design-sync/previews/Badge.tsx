import { Badge } from "@aipanel/ui";

export function Tones() {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Badge tone="neutral">unknown</Badge>
      <Badge tone="brand">read-only mode</Badge>
      <Badge tone="success">online</Badge>
      <Badge tone="warning">degraded</Badge>
      <Badge tone="danger">offline</Badge>
      <Badge tone="info">read-only</Badge>
    </div>
  );
}

export function InContext() {
  return (
    <div className="flex items-center gap-2 text-sm text-fg">
      <span>db-prod-1</span>
      <Badge tone="success">online</Badge>
      <Badge tone="neutral">3 steps</Badge>
    </div>
  );
}
