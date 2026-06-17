import { useEffect, useState } from "react";
import {
  AuditEntry,
  Badge,
  Button,
  CommandPlan,
  Dialog,
  ServerCard,
  Textarea,
  type PlanStep,
} from "@aipanel/ui";

type Theme = "light" | "dark";

/** Light-first theme, persisted to localStorage; toggles the `dark` class on <html>. */
function useTheme(): [Theme, () => void] {
  const [theme, setTheme] = useState<Theme>(
    () => (localStorage.getItem("aipanel-theme") as Theme) ?? "light"
  );
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("aipanel-theme", theme);
  }, [theme]);
  return [theme, () => setTheme((t) => (t === "light" ? "dark" : "light"))];
}

const SERVERS = [
  {
    name: "web-prod-1",
    host: "root@10.0.0.4:22",
    status: "online" as const,
    facts: { OS: "Ubuntu 22.04", CPU: "12%", Mem: "3.1/8 GB", Disk: "44%" },
  },
  {
    name: "db-prod-1",
    host: "ops@10.0.0.7:22",
    status: "online" as const,
    facts: { OS: "Debian 12", CPU: "31%", Mem: "9.4/16 GB", Disk: "71%" },
  },
  {
    name: "edge-cache",
    host: "root@10.0.0.9:22",
    status: "offline" as const,
    facts: { OS: "Alpine 3.19", CPU: "—", Mem: "—", Disk: "—" },
  },
];

const PLAN: PlanStep[] = [
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

export default function App() {
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [theme, toggleTheme] = useTheme();

  return (
    <div className="mx-auto flex min-h-full max-w-5xl flex-col gap-6 p-6">
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-fg">AiPanel</h1>
          <p className="text-xs text-fg-muted">
            Local AI server operations · SSH-first
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={toggleTheme}>
            {theme === "light" ? "🌙 Dark" : "☀ Light"}
          </Button>
          <Badge tone="brand">read-only mode</Badge>
        </div>
      </header>

      <section className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {SERVERS.map((s) => (
          <ServerCard key={s.name} {...s} />
        ))}
      </section>

      <section className="space-y-2">
        <label className="text-xs font-medium text-fg-muted">Ask AiPanel</label>
        <Textarea
          defaultValue="Check why this website is unreachable. Do not delete anything."
          rows={2}
        />
        <div className="flex justify-end">
          <Button>Generate plan</Button>
        </div>
      </section>

      <CommandPlan
        goal="Recover the unreachable website on web-prod-1"
        steps={PLAN}
      />

      <div className="flex items-center justify-end gap-2">
        <Button variant="ghost">Discard</Button>
        <Button variant="danger" onClick={() => setConfirmOpen(true)}>
          Approve &amp; run
        </Button>
      </div>

      <section className="space-y-1">
        <h2 className="text-xs font-medium text-fg-muted">Audit trail</h2>
        <AuditEntry
          timestamp="2026-06-17 15:02:11"
          command="systemctl status nginx"
          risk="low"
          exitCode={3}
          output={
            "● nginx.service - A high performance web server\n   Active: inactive (dead)"
          }
        />
        <AuditEntry
          timestamp="2026-06-17 15:02:14"
          command="journalctl -u nginx -n 50 --no-pager"
          risk="low"
          exitCode={0}
        />
      </section>

      <Dialog
        open={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        title="Confirm medium-risk action"
        description="This will restart nginx on web-prod-1. The service will be briefly unavailable."
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setConfirmOpen(false)}
            >
              Cancel
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={() => setConfirmOpen(false)}
            >
              Confirm restart
            </Button>
          </>
        }
      >
        Review the plan once more before approving. High-risk steps would require
        a second confirmation.
      </Dialog>
    </div>
  );
}
