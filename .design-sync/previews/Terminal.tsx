import { Terminal } from "@aipanel/ui";

export function Live() {
  return (
    <div className="max-w-2xl">
      <Terminal
        host="prod-ai-01"
        live
        cursor
        lines={[
          { text: "root@prod-ai-01:~# ss -ltnp | grep -E ':22|:80|:443'", tone: "prompt" },
          { text: 'LISTEN 0 128 0.0.0.0:22   users:(("sshd",pid=812))' },
          { text: 'LISTEN 0 511 0.0.0.0:80   users:(("nginx",pid=1042))' },
          { text: "▸ 端口检查:监听正常,无异常占用", tone: "success" },
          { text: "检查服务状态 …", tone: "muted" },
        ]}
      />
    </div>
  );
}

export function Idle() {
  return (
    <div className="max-w-2xl">
      <Terminal
        host="db-prod-1"
        lines={[
          { text: "root@db-prod-1:~# systemctl status nginx --no-pager", tone: "prompt" },
          { text: "● nginx.service - A high performance web server" },
          { text: "   Active: inactive (dead)", tone: "danger" },
          { text: "root@db-prod-1:~# " },
        ]}
        cursor
      />
    </div>
  );
}
