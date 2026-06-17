import { CommandPlan, type PlanStep } from "@aipanel/ui";

const STEPS: PlanStep[] = [
  {
    summary: "Check whether nginx is running",
    command: "systemctl status nginx",
    risk: "low",
    readOnly: true,
  },
  {
    summary: "Inspect the last 50 error log lines",
    command: "journalctl -u nginx -n 50 --no-pager",
    risk: "low",
    readOnly: true,
  },
  {
    summary: "Restart nginx to recover the service",
    command: "systemctl restart nginx",
    risk: "medium",
  },
];

export function RecoveryPlan() {
  return (
    <div className="max-w-2xl">
      <CommandPlan
        goal="Recover the unreachable website on web-prod-1"
        steps={STEPS}
      />
    </div>
  );
}

export function ReadOnlyDoctor() {
  return (
    <div className="max-w-2xl">
      <CommandPlan
        goal="Read-only health check on db-prod-1"
        steps={[
          { summary: "Disk usage", command: "df -h", risk: "low", readOnly: true },
          { summary: "Memory", command: "free -m", risk: "low", readOnly: true },
          {
            summary: "Listening ports",
            command: "ss -tlnp",
            risk: "low",
            readOnly: true,
          },
        ]}
      />
    </div>
  );
}
