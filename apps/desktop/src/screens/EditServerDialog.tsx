import { useEffect, useState } from "react";
import { Save, Trash2, TriangleAlert, X } from "lucide-react";
import { Button, Dialog, Input, Textarea } from "@aipanel/ui";
import {
  deleteServer,
  setServerSecret,
  updateServer,
  type AuthKind,
  type ServerProfile,
} from "../lib/api";

// 编辑服务器对话框：修改连接信息、可选更新凭据，或二次确认后删除服务器。
export default function EditServerDialog({
  open,
  server,
  onClose,
  onSaved,
  onDeleted,
}: {
  open: boolean;
  server: ServerProfile | null;
  onClose: () => void;
  onSaved: (s: ServerProfile) => void;
  onDeleted: (id: string) => void;
}) {
  const [name, setName] = useState("");
  const [host, setHost] = useState("");
  // 端口用字符串受控，允许中间态（空串、删空、多位），提交时再回退到 22。
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("root");
  const [authKind, setAuthKind] = useState<AuthKind>("password");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 服务器变化时回填表单，并清掉密钥/确认/错误状态。
  useEffect(() => {
    if (!server) return;
    setName(server.name);
    setHost(server.host);
    setPort(String(server.port));
    setUsername(server.username);
    setAuthKind(server.authKind);
    setSecret("");
    setConfirmDelete(false);
    setError(null);
  }, [server]);

  // 保存修改：更新服务器信息，并在填了新凭据时单独写入（留空则不动凭据）。
  async function submit() {
    if (!server) return;
    if (!name.trim() || !host.trim() || !username.trim()) {
      setError("名称、主机、用户名必填");
      return;
    }
    // 端口:留空回退 22;填了就必须是 1-65535 的整数,避免静默改成 22 连错端口。
    if (port.trim() && (!Number.isInteger(Number(port)) || Number(port) < 1 || Number(port) > 65535)) {
      setError("端口必须是 1-65535 之间的整数");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const portNum = Number(port) || 22; // 空串回退到 22
      const updated = await updateServer(server.id, { name, host, port: portNum, username, authKind });
      if (authKind !== "agent" && secret) {
        await setServerSecret(server.id, secret);
      }
      onSaved(updated);
      onClose();
    } catch (e) {
      setError(e && typeof e === "object" && "message" in e ? (e as { message: string }).message : String(e));
    } finally {
      setBusy(false);
    }
  }

  // 删除服务器：首次点击仅切到确认态，再次点击才真正删除。
  async function remove() {
    if (!server) return;
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await deleteServer(server.id);
      onDeleted(server.id);
      onClose();
    } catch (e) {
      setError(e && typeof e === "object" && "message" in e ? (e as { message: string }).message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const field = "flex flex-col gap-1.5";
  const labelCls = "text-[12px] font-medium tracking-wide text-fg-muted";
  // 原生 select 对齐设计 token，补齐过渡与焦点环
  const selectCls =
    "h-9 rounded-md border border-border bg-surface-2 px-2.5 text-sm text-fg outline-none transition-colors focus-visible:border-brand focus-visible:ring-2 focus-visible:ring-brand/60";

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="编辑服务器"
      description="凭据只保存在本地 Keychain，绝不写入数据库或发送给 AI。"
      footer={
        <>
          {confirmDelete ? (
            <Button variant="secondary" size="sm" onClick={() => setConfirmDelete(false)}>
              <X size={15} strokeWidth={1.75} />
              取消删除
            </Button>
          ) : null}
          <Button
            variant={confirmDelete ? "primary" : "secondary"}
            size="sm"
            onClick={remove}
            disabled={busy}
            className={
              confirmDelete
                ? "bg-risk-blocked text-white hover:bg-risk-blocked/90 focus-visible:ring-risk-blocked/60"
                : "text-risk-blocked hover:bg-risk-blocked/10 focus-visible:ring-risk-blocked/60"
            }
          >
            <Trash2 size={15} strokeWidth={1.75} />
            {confirmDelete ? "确认删除？" : "删除"}
          </Button>
          <div className="flex-1" />
          <Button variant="secondary" size="sm" onClick={onClose}>
            <X size={15} strokeWidth={1.75} />
            取消
          </Button>
          <Button variant="primary" size="sm" onClick={submit} disabled={busy}>
            <Save size={15} strokeWidth={1.75} />
            保存
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        {/* 删除二次确认：醒目的危险提示，引导用户再次确认不可恢复操作 */}
        {confirmDelete ? (
          <div className="flex items-start gap-2.5 rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2.5">
            <TriangleAlert size={16} strokeWidth={1.75} className="mt-px flex-none text-risk-blocked" />
            <div className="text-[12.5px] leading-relaxed text-risk-blocked">
              <p className="font-semibold">即将删除该服务器</p>
              <p className="text-risk-blocked/80">此操作不可恢复，本地保存的凭据也会一并移除。再次点击「确认删除？」以继续。</p>
            </div>
          </div>
        ) : null}
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
              min={1}
              max={65535}
              step={1}
              value={port}
              onChange={(e) => setPort(e.target.value)}
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
            onChange={(e) => {
              setAuthKind(e.target.value as AuthKind);
              setSecret(""); // 切换认证方式后不保留已输入的凭据
            }}
            className={selectCls}
          >
            <option value="password">密码</option>
            <option value="key">私钥</option>
            <option value="agent">ssh-agent</option>
          </select>
        </div>
        {authKind === "password" && (
          <div className={field}>
            <label className={labelCls}>更新密钥（留空则不修改）</label>
            <Input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} />
          </div>
        )}
        {authKind === "key" && (
          <div className={field}>
            <label className={labelCls}>更新密钥（留空则不修改）</label>
            <Textarea
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              rows={3}
              placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
            />
          </div>
        )}
        {error ? (
          <div className="flex items-start gap-2 rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2 text-[12.5px] text-risk-blocked">
            <TriangleAlert size={14} strokeWidth={1.75} className="mt-px flex-none" />
            <span>{error}</span>
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}
