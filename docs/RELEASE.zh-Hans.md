# 发布与在线更新

本文档说明 AiPanel 的**版本管理**、**发布流程**与**应用内在线更新**机制。

## 原理

AiPanel 用 **Tauri v2 官方 updater 插件 + GitHub Releases** 实现在线更新,刻意不自造下载/校验逻辑:

```
打 tag vX.Y.Z
   └─> .github/workflows/release.yml(多平台矩阵构建)
          └─> tauri-action 用私钥(minisign)签名 + 创建 GitHub Release
                 └─> 上传各平台安装包 + latest.json(更新清单,含版本/下载地址/签名)
                        └─> 应用内 updater 读取 latest.json,校验签名后下载安装
```

- **更新清单**:`tauri.conf.json` 的 `plugins.updater.endpoints` 指向
  `https://github.com/7836246/AiPanel/releases/latest/download/latest.json`。每次发布由
  tauri-action 生成并随 Release 上传;updater 始终取「latest」那个 Release 的清单。
- **签名校验**:更新包用 minisign 私钥签名;客户端用内置的**公钥**(`plugins.updater.pubkey`,
  已入库)验证签名,**签名不匹配的更新一律拒绝安装**——这是防篡改的核心,所以私钥绝不能泄露。
- **降级**:检查更新失败 / 无网络时,应用照常使用;启动静默检查的错误会被吞掉,不打扰用户。

## 版本来源(统一)

版本号以 `apps/desktop/src-tauri/tauri.conf.json` 的 `version` 为**权威**,并与下列保持一致:

- `package.json`(workspace 根)
- `apps/desktop/package.json`
- `apps/desktop/src-tauri/Cargo.toml` 的 `[package].version` + `Cargo.lock` 中 `desktop` 条目

用脚本一键同步,不要手改:

```sh
scripts/bump-version.sh 0.1.1
```

`scripts/release-check.sh` 发布前会校验这四处一致;若在 tag 上构建,还会校验与 tag(去掉 `v`)一致。

## 一次性准备:签名密钥

更新签名密钥(minisign)**已生成**:

- **公钥**:已写入 `apps/desktop/src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`(公开,随仓库分发)。
- **私钥**:在仓库**之外**的 `~/.aipanel/updater.key`(`.gitignore` 另加 `*.key` 兜底,**绝不入库**)。

发布前需把私钥配成 GitHub 仓库 Secret(只需做一次):

1. 打开仓库 `Settings → Secrets and variables → Actions → New repository secret`。
2. 名称 `TAURI_SIGNING_PRIVATE_KEY`,值为 `~/.aipanel/updater.key` **文件的完整内容**。
3. 本密钥**无密码**,因此 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 可不设(或留空)。

> ⚠️ 私钥一旦丢失/泄露:丢失则无法再为更新签名(只能换新公钥重发全量);泄露则他人可伪造更新。
> 务必离线备份 `~/.aipanel/updater.key`。

> 想自己重新生成:`pnpm --filter @aipanel/desktop exec -- tauri signer generate -w ~/.aipanel/updater.key`,
> 然后把新公钥替换进 `tauri.conf.json`、新私钥更新到 Secret。

## 发布流程

```sh
# 1) 升版本(同步所有来源)
scripts/bump-version.sh 0.1.1

# 2) 提交
git add -A && git commit -m "chore(release): v0.1.1"
git push origin main

# 3) 打 tag 并推送 —— 触发 release.yml 自动构建、签名、发布
git tag v0.1.1
git push origin v0.1.1
```

`release.yml` 会在 macOS(aarch64 + x86_64)、Linux、Windows 上分别:安装依赖 → 按平台 triple 跑
`scripts/fetch-codex.sh` 取 Codex sidecar → `pnpm build:ui` → `tauri-action` 构建+签名+发布,并把各
平台安装包与 `latest.json` 上传到名为 `AiPanel v0.1.1` 的 Release。

发布完成后,旧版本用户在「设置 · 在线更新」点检查(或下次启动静默检查)即可看到并一键升级。

## 用户侧:如何更新

- **设置 · 在线更新**:显示当前版本;点「检查更新」→ 有新版展示版本号与更新说明 →「下载并安装」
  (带进度条)→ 完成后自动重启。
- **启动静默检查**:默认开启,启动时静默检查一次,有新版给一条非打扰提示;可在该区块关闭。

## 验证边界(重要)

本仓库的本地校验覆盖到:插件可编译、`tauri.conf` 与 workflow YAML 合法、更新 UI 可渲染并以 mock 跑通
交互、版本同步脚本与一致性校验。**「真实下载并安装一个更新」这一端到端环节,只有在完成上面「签名
密钥配 Secret + 推首个 tag 发布」后,由真实的已签名 Release 才能验证**——这一步无法在本地离线模拟。
