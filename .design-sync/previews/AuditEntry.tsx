import { AuditEntry } from "@aipanel/ui";

export function Trail() {
  return (
    <div className="max-w-2xl">
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
      <AuditEntry
        timestamp="2026-06-17 15:02:31"
        command="systemctl restart nginx"
        risk="medium"
        exitCode={0}
      />
    </div>
  );
}

export function Failure() {
  return (
    <div className="max-w-2xl">
      <AuditEntry
        timestamp="2026-06-17 15:05:02"
        command="systemctl restart nginx"
        risk="medium"
        exitCode={1}
        output={"Job for nginx.service failed because the control process exited."}
      />
    </div>
  );
}
