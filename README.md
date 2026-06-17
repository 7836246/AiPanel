<p align="center"><a href="#"><img src="./assets/logo-white-bg.png" alt="AiPanel" width="420" /></a></p>
<p align="center"><b>Local AI Server Operations Client</b></p>
<p align="center"><b>本地运行、通过 SSH 管理服务器的 AI 运维客户端</b></p>

<p align="center">
  <a href="#"><img src="https://img.shields.io/badge/status-planning-0F766E" alt="Project Status"></a>
  <a href="#"><img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-0284C7" alt="Platform"></a>
  <a href="#"><img src="https://img.shields.io/badge/agent-SSH%20first-16A34A" alt="SSH First"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-64748B" alt="License: AGPL-3.0"></a>
</p>

<p align="center">
  <a href="/README.md"><img alt="English" src="https://img.shields.io/badge/English-d9d9d9"></a>
  <a href="/docs/README.zh-Hans.md"><img alt="中文(简体)" src="https://img.shields.io/badge/中文(简体)-d9d9d9"></a>
</p>

![AiPanel Preview](./assets/preview.png)

------------------------------

## What is AiPanel?

AiPanel is a local AI operations client for Linux servers.

Unlike traditional server panels that must be installed and kept running on every VPS, AiPanel runs on your local machine and connects to servers through SSH. It turns natural language requests into reviewable plans, executes approved actions remotely, and summarizes the result.

- **Zero resident panel on servers**: no long-running panel process is required on the VPS;
- **SSH-first operations**: connect through standard SSH without opening a new public web admin entrance;
- **AI planning before execution**: generate clear plans, risk labels, and commands before running anything;
- **Read-only diagnosis mode**: inspect CPU, memory, disk, ports, services, Docker, Nginx, logs, and firewalls safely;
- **Deployment workflows**: install Docker, deploy Docker Compose apps, configure reverse proxy, HTTPS, and health checks;
- **Local audit trail**: keep task plans, command history, outputs, and summaries on the local client.

## Quick Start

AiPanel is at the **desktop MVP** stage — a Tauri v2 + React app you run locally.

```sh
pnpm install          # install workspace deps
pnpm build:ui         # build the @aipanel/ui design system
pnpm tauri:dev        # launch the desktop app (needs the Rust toolchain)
# or `pnpm dev` for the frontend only, in a browser (backend calls fall back to mocks)
```

In the app you can add a server (credentials go to the system Keychain), test SSH
connectivity, run a read-only health check, turn a request into a reviewable plan,
approve and execute it, and review the local audit trail.

## Roadmap

- [x] Project positioning
- [x] README structure
- [x] Initial logo and preview assets
- [x] Desktop client (Tauri v2 + React) with the AiPanel UI design system
- [x] SSH connection manager (add servers, connectivity test)
- [x] Read-only server doctor
- [x] AI task planning (structured plans; mock engine, Codex bridge entry point)
- [x] Command risk review (Low / Medium / High / Blocked) + local audit log
- [x] Model provider manager
- [ ] Codex app-server tool-call loop
- [ ] Docker app deployment workflows

## Documentation

- [中文文档](./docs/README.zh-Hans.md)
- [Roadmap](./docs/ROADMAP.zh-Hans.md)
- [Architecture](./docs/ARCHITECTURE.zh-Hans.md)
- [Tech Stack](./docs/TECH_STACK.zh-Hans.md)
- [Security Model](./docs/SECURITY_MODEL.zh-Hans.md)
- [Security Policy](./SECURITY.md)
- [Contributing](./CONTRIBUTING.md)
- [Image generation prompts](./assets/prompts/gpt-image-2.md)

## License

Copyright (c) 2026 AiPanel.

Licensed under the [GNU Affero General Public License v3.0](./LICENSE).
