import { useState } from "react";
import { Button, Dialog, Input, Textarea } from "@aipanel/ui";
import { createServer, setServerSecret, type AuthKind, type ServerProfile } from "../lib/api";

// 添加服务器对话框：填写连接信息与凭据，凭据仅写入本地 Keychain。
export default function AddServerDialog({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (s: ServerProfile) => void;
}) {
  const [name, setName] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState(22);
  const [username, setUsername] = useState("root");
  const [authKind, setAuthKind] = useState<AuthKind>("password");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 把表单恢复到初始默认值。
  function reset() {
    setName("");
    setHost("");
    setPort(22);
    setUsername("root");
    setAuthKind("password");
    setSecret("");
    setError(null);
  }

  // 校验并提交：先创建服务器，再（非 agent 且填了凭据时）单独保存凭据。
  async function submit() {
    if (!name.trim() || !host.trim() || !username.trim()) {
      setError("名称、主机、用户名必填");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const srv = await createServer({ name, host, port, username, authKind });
      if (authKind !== "agent" && secret) {
        await setServerSecret(srv.id, secret);
      }
      onCreated(srv);
      reset();
      onClose();
    } catch (e) {
      setError(e && typeof e === "object" && "message" in e ? (e as { message: string }).message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const field = "flex flex-col gap-1";
  const labelCls = "text-[12px] font-medium text-fg-muted";

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="添加服务器"
      description="凭据只保存在本地 Keychain，绝不写入数据库或发送给 AI。"
      footer={
        <>
          <Button variant="secondary" size="sm" onClick={onClose}>
            取消
          </Button>
          <Button variant="primary" size="sm" onClick={submit} disabled={busy}>
            保存
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className={field}>
          <label className={labelCls}>名称</label>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="web-prod-1" />
        </div>
        <div className="flex gap-3">
          <div className={`${field} flex-1`}>
            <label className={labelCls}>主机</label>
            <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="10.0.0.4" />
          </div>
          <div className={`${field} w-24`}>
            <label className={labelCls}>端口</label>
            <Input
              type="number"
              value={port}
              onChange={(e) => setPort(Number(e.target.value) || 22)}
            />
          </div>
        </div>
        <div className={field}>
          <label className={labelCls}>用户名</label>
          <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="root" />
        </div>
        <div className={field}>
          <label className={labelCls}>认证方式</label>
          <select
            value={authKind}
            onChange={(e) => setAuthKind(e.target.value as AuthKind)}
            className="h-9 rounded-md border border-border bg-surface-2 px-2 text-sm text-fg outline-none focus-visible:border-brand"
          >
            <option value="password">密码</option>
            <option value="key">私钥</option>
            <option value="agent">ssh-agent</option>
          </select>
        </div>
        {authKind === "password" && (
          <div className={field}>
            <label className={labelCls}>密码</label>
            <Input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} />
          </div>
        )}
        {authKind === "key" && (
          <div className={field}>
            <label className={labelCls}>私钥内容</label>
            <Textarea
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              rows={3}
              placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
            />
          </div>
        )}
        {error ? <div className="text-[12.5px] text-risk-blocked">{error}</div> : null}
      </div>
    </Dialog>
  );
}
