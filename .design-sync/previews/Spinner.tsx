import { Spinner } from "@aipanel/ui";

export function Sizes() {
  return (
    <div className="flex items-center gap-4">
      <Spinner size="sm" />
      <Spinner size="md" />
    </div>
  );
}

export function WithLabel() {
  return (
    <div className="flex items-center gap-2 text-sm text-fg-muted">
      <Spinner size="sm" />
      <span>Running systemctl restart nginx…</span>
    </div>
  );
}
