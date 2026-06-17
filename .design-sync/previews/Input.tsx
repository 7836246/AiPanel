import { Input } from "@aipanel/ui";

export function Default() {
  return (
    <div className="max-w-sm">
      <Input defaultValue="web-prod-1" />
    </div>
  );
}

export function Labeled() {
  return (
    <div className="max-w-sm space-y-1.5">
      <label className="text-xs font-medium text-fg-muted">Host</label>
      <Input placeholder="root@10.0.0.4:22" />
    </div>
  );
}

export function Password() {
  return (
    <div className="max-w-sm space-y-1.5">
      <label className="text-xs font-medium text-fg-muted">sudo password</label>
      <Input type="password" defaultValue="correcthorsebattery" />
    </div>
  );
}

export function Disabled() {
  return (
    <div className="max-w-sm">
      <Input disabled defaultValue="edge-cache (offline)" />
    </div>
  );
}
