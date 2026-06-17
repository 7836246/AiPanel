import { ServerCard } from "@aipanel/ui";

export function Online() {
  return (
    <div className="max-w-sm">
      <ServerCard
        name="web-prod-1"
        host="root@10.0.0.4:22"
        status="online"
        facts={{ OS: "Ubuntu 22.04", CPU: "12%", Mem: "3.1/8 GB", Disk: "44%" }}
      />
    </div>
  );
}

export function Offline() {
  return (
    <div className="max-w-sm">
      <ServerCard
        name="edge-cache"
        host="root@10.0.0.9:22"
        status="offline"
        facts={{ OS: "Alpine 3.19", CPU: "—", Mem: "—", Disk: "—" }}
      />
    </div>
  );
}

export function Grid() {
  return (
    <div className="grid max-w-3xl grid-cols-1 gap-3 sm:grid-cols-2">
      <ServerCard
        name="web-prod-1"
        host="root@10.0.0.4:22"
        status="online"
        facts={{ OS: "Ubuntu 22.04", CPU: "12%", Disk: "44%" }}
      />
      <ServerCard
        name="db-prod-1"
        host="ops@10.0.0.7:22"
        status="online"
        facts={{ OS: "Debian 12", CPU: "31%", Disk: "71%" }}
      />
    </div>
  );
}
