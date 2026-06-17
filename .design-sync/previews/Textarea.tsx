import { Button, Textarea } from "@aipanel/ui";

export function AskBox() {
  return (
    <div className="max-w-lg space-y-2">
      <label className="text-xs font-medium text-fg-muted">Ask AiPanel</label>
      <Textarea
        rows={3}
        defaultValue="Check why this website is unreachable. Do not delete anything."
      />
      <div className="flex justify-end">
        <Button size="sm">Generate plan</Button>
      </div>
    </div>
  );
}

export function Empty() {
  return (
    <div className="max-w-lg">
      <Textarea rows={2} placeholder="Describe what you want to do on this server…" />
    </div>
  );
}
