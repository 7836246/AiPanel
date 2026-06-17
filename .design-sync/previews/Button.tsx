import { Button } from "@aipanel/ui";

export function Variants() {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button variant="primary">Generate plan</Button>
      <Button variant="secondary">Cancel</Button>
      <Button variant="outline">Add server</Button>
      <Button variant="ghost">Discard</Button>
      <Button variant="danger">Approve &amp; run</Button>
    </div>
  );
}

export function Sizes() {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button size="sm">Small</Button>
      <Button size="md">Medium</Button>
      <Button size="lg">Large</Button>
    </div>
  );
}

export function Disabled() {
  return (
    <div className="flex items-center gap-3">
      <Button disabled>Running…</Button>
      <Button variant="danger" disabled>
        Approve &amp; run
      </Button>
    </div>
  );
}
