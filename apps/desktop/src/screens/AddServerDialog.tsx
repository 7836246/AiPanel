import { useEffect, useState } from "react";
import { Save, TriangleAlert, X } from "lucide-react";
import { Button, Dialog, Input, Select, Spinner, Textarea } from "@aipanel/ui";
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
  // 端口用字符串受控，允许中间态（空串、删空、多位），提交时再回退到 22。
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("root");
  const [authKind, setAuthKind] = useState<AuthKind>("password");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 把表单恢复到初始默认值（含清空 secret）。
  function reset() {
    setName("");
    setHost("");
    setPort("22");
    setUsername("root");
    setAuthKind("password");
    setSecret("");
    setBusy(false);
    setError(null);
  }

  // 对话框关闭时重置全部表单字段，避免下次打开残留旧输入（含密码/私钥）。
  useEffect(() => {
    if (!open) reset();
  }, [open]);

  // 校验并提交：先创建服务器，再（非 agent 且填了凭据时）单独保存凭据。
  async function submit() {
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
      const srv = await createServer({ name, host, port: portNum, username, authKind });
      if (authKind !== "agent" && secret) {
        await setServerSecret(srv.id, secret);
      }
      onCreated(srv);
      onClose(); // 关闭会触发上面的 useEffect 重置表单

    } catch (e) {
      setError(e && typeof e === "object" && "message" in e ? (e as { message: string }).message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const field = "flex flex-col gap-1.5";
  const labelCls = "text-[12px] font-medium tracking-wide text-fg-muted";

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="添加服务器"
      description="凭据只保存在本地 Keychain，绝不写入数据库或发送给 AI。"
      footer={
        <>
          <Button variant="secondary" size="sm" onClick={onClose}>
            <X size={15} strokeWidth={1.75} />
            取消
          </Button>
          {/* 保存按钮指向字段区的 form，使输入框内回车也能提交；busy 时显示 Spinner + 文案，提示凭据写入 Keychain 可能有延迟 */}
          <Button variant="primary" size="sm" type="submit" form="add-server-form" disabled={busy}>
            {busy ? (
              <>
                <Spinner size="sm" />
                保存中…
              </>
            ) : (
              <>
                <Save size={15} strokeWidth={1.75} />
                保存
              </>
            )}
          </Button>
        </>
      }
    >
      {/* 字段区改为 form：输入框内按回车即可提交，与 footer 槽外的保存按钮通过 form id 关联 */}
      <form
        id="add-server-form"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
        className="flex flex-col gap-3.5"
      >
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
          <Select
            value={authKind}
            onChange={(v) => { setAuthKind(v as AuthKind); setSecret(""); }}
            aria-label="认证方式"
            options={[
              { value: "password", label: "密码" },
              { value: "key", label: "私钥" },
              { value: "agent", label: "ssh-agent" },
            ]}
          />
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
        {error ? (
          <div className="flex items-start gap-2 rounded-md border border-risk-blocked/40 bg-risk-blocked/10 px-3 py-2 text-[12.5px] text-risk-blocked">
            <TriangleAlert size={14} strokeWidth={1.75} className="mt-px flex-none" />
            <span>{error}</span>
          </div>
        ) : null}
      </form>
    </Dialog>
  );
}
