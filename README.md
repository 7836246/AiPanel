<p align="center"><a href="#"><img src="./assets/logo-white-bg.png" alt="AiPanel" width="420" /></a></p>
<p align="center"><b>Local AI Server Operations Client</b></p>
<p align="center"><b>本地运行、通过 SSH 管理服务器的 AI 运维客户端</b></p>

<p align="center">
  <a href="#"><img src="https://img.shields.io/badge/status-desktop%20mvp-0F766E" alt="Project Status"></a>
  <a href="#"><img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-0284C7" alt="Platform"></a>
  <a href="#"><img src="https://img.shields.io/badge/agent-SSH%20first-16A34A" alt="SSH First"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-64748B" alt="License: AGPL-3.0"></a>
</p>

<p align="center">
  <a href="/README.md"><img alt="English" src="https://img.shields.io/badge/English-d9d9d9"></a>
  <a href="/docs/README.zh-Hans.md"><img alt="中文(简体)" src="https://img.shields.io/badge/中文(简体)-d9d9d9"></a>
</p>

![AiPanel Preview](./assets/preview.png)
<p align="center"><sub>Local-first desktop client — add a server, run a read-only health check, turn a request into a reviewable plan, then execute over SSH.</sub></p>

------------------------------

## What is AiPanel?

AiPanel is a **local-first AI operations client for Linux servers**. It runs on your own machine and manages remote servers over SSH — deliberately leaving **no resident panel process on the server** and opening **no new public admin surface**. Natural-language requests become reviewable plans; approved actions run over SSH; everything is audited locally.

The core principle: **Codex runs the agent and conversation; AiPanel owns the server-operations security boundary.** The AI proposes plans — it never holds SSH credentials and never runs a raw shell.

## Highlights

- 🔌 **Zero resident agent** — manage servers over plain SSH; nothing to install or keep running on the VPS.
- 🧠 **AI plans, you approve** — natural language → structured plan (goal, steps, commands, risk level) → review → execute.
- 🛡️ **Read-only by default** — safe diagnosis of CPU / memory / disk / ports / services / Docker / Nginx / logs / firewall; writes need explicit confirmation, high-risk needs a second.
- 📊 **Live monitoring & dashboard** — ring gauges + traffic chart sampled every 3s over read-only SSH; multi-server overview with health badges.
- 🖥️ **Interactive workspace** — xterm.js SSH terminal + SFTP file manager, user-operated and never exposed to the AI.
- 🐳 **Deployment workflows** — install Docker, deploy Compose apps, Caddy/Nginx reverse proxy + HTTPS, post-deploy health checks.
- 🔑 **Credentials stay local** — SSH keys, passwords, and API keys live only in the system Keychain; outputs are redacted before they reach the AI or the audit log.
- 📝 **Local audit trail** — intent, plan, risk verdict, confirmation, commands, exit codes, redacted output, and summary — kept on your machine.

## How it works

```
AiPanel Desktop   (Tauri v2 + React + TypeScript)
      │ JSON-RPC / stdio
Codex App Server  (agent runtime: multi-turn, context, model selection, streaming)
      │ tool calls
AiPanel Tools     (server.list · server.doctor.readonly · ssh.run_readonly ·
                   task.plan / review / execute_confirmed · audit.write)
      │
AiPanel Core      (Risk Reviewer · SSH Executor · Audit Log · SQLite · Keychain)
      │
Remote Server
```

SSH execution, risk review, redaction, and audit are implemented by AiPanel itself — never delegated to the AI. Codex reaches servers **only** through AiPanel's reviewed tools; no unrestricted shell is ever exposed.

## Security boundaries (non-negotiable)

- The AI never holds SSH credentials and never runs a raw shell — only AiPanel-reviewed tools reach a server.
- AI output is a **plan, not a fact** — it passes the Risk Reviewer before anything runs.
- **Read-only by default**; write actions require explicit confirmation, destructive ones a second confirmation; `rm -rf /`-class commands are **Blocked**.
- Credentials (SSH keys/passwords, sudo, API keys, DB passwords) live only in the local Keychain — never committed, logged, or sent to the AI.
- IPs, tokens, secrets, and connection strings are redacted before being sent to the AI or written to the audit log.

See the [Security Model](./docs/SECURITY_MODEL.zh-Hans.md) for the authoritative command-review and risk classification.

## Quick Start

AiPanel is at the **desktop MVP** stage — a Tauri v2 + React app you run locally.

```sh
pnpm install          # install workspace deps
pnpm build:ui         # build the @aipanel/ui design system
pnpm tauri:dev        # launch the desktop app (needs the Rust toolchain)
# or: pnpm dev        # frontend only, in a browser (backend calls fall back to mocks)
```

Credentials use the system Keychain by default, including in `tauri:dev`. For mock-only development, set `AIPANEL_CREDENTIAL_BACKEND=mock`.

In the app you can add a server, test SSH connectivity, run a read-only health check, turn a request into a reviewable plan, approve and execute it, and review the local audit trail.

## Quality Gates

```sh
pnpm ci:check         # typecheck · frontend vitest · Rust tests · Codex sidecar · Clippy (-D warnings) · build
```

Run `scripts/fetch-codex.sh` first if the bundled Codex sidecar hasn't been fetched for your platform. Before shipping a macOS build, run `pnpm release:check` — the full gate that also builds the Tauri app and verifies it is signed with a Developer ID Application identity and has a valid notarization ticket.

## Features

**Connect & diagnose**
- SSH connection manager — add servers, connectivity test, visible connect/reconnect
- Read-only server doctor with structured metrics + live streaming output
- Real-time monitoring — ring gauges + traffic chart over read-only SSH, no resident agent
- Multi-server dashboard with favorites and foreground health polling

**AI operations**
- Natural-language task planning — real LLM via OpenAI-compatible providers (structured output), with an offline mock fallback
- Autonomous read-only diagnosis — the agent investigates using read-only tools only
- Risk review (Low / Medium / High / Blocked) with confirm / second-confirm before writes + local audit log
- Codex app-server runtime — `thread/start` → `turn/start` → event stream → tool dispatch/relay; preferred runtime with OpenAI fallback
- Model provider manager — base URL + API key; models auto-discovered via `/v1/models`

**Operate**
- Interactive SSH terminal + SFTP file manager — Codex-style 3-pane workspace; user-operated, not exposed to the AI
- Docker deployment workflows — detect/install, Compose deploy, Caddy/Nginx reverse proxy + HTTPS, health checks; Uptime Kuma / n8n / WordPress / PostgreSQL / Redis templates, each a risk-reviewed plan
- In-app online updates — signed GitHub releases (Tauri updater + minisign), `vX.Y.Z` tags, check / download / install + relaunch

**Foundation**
- `@aipanel/ui` design system — Tailwind v4 tokens with Codex-style primitives
- Rust unit/integration tests + frontend vitest, Clippy (`-D warnings`), one-shot `pnpm ci:check`

## Documentation

- [中文文档](./docs/README.zh-Hans.md)
- [Roadmap](./docs/ROADMAP.zh-Hans.md)
- [Release & online updates](./docs/RELEASE.zh-Hans.md)
- [Architecture](./docs/ARCHITECTURE.zh-Hans.md)
- [Tech Stack](./docs/TECH_STACK.zh-Hans.md)
- [Security Model](./docs/SECURITY_MODEL.zh-Hans.md)
- [Security Policy](./SECURITY.md)
- [Contributing](./CONTRIBUTING.md)
- [Image generation prompts](./assets/prompts/gpt-image-2.md)

## License

Copyright (c) 2026 AiPanel.

Licensed under the [GNU Affero General Public License v3.0](./LICENSE).
