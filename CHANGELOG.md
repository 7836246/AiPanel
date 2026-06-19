# Changelog

All notable changes to AiPanel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project plans to follow semantic versioning after the first public release.

## [Unreleased]

## [0.1.1] - 2026-06-19

首个公开发布(桌面 MVP)。包含应用内**在线更新**(GitHub Releases + minisign 签名、`vX.Y.Z`
tag 管理版本)、全站 **Codex 风格 UI**、**可定制外观**(主题模式 + 浅/深主题各自的强调色/背景/
前景/字体/对比度/半透明侧栏、独立整页设置),以及下方记录的全部能力。

### Added

- 应用内在线更新:「设置 · 在线更新」可检查 / 下载 / 安装 / 重启;Tauri updater + minisign
  签名,GitHub Releases 分发 `latest.json`,启动静默检查;`vX.Y.Z` tag 触发多平台构建签名发布。
- 可定制外观:Codex 式独立整页设置(左侧分类导航);浅/深主题各自的强调色、背景、前景、UI/代码
  字体、对比度、半透明侧栏;默认等于原始观感,仅覆写用户改动项。
- `@aipanel/ui` 新增 Codex 风格 Switch / Select 等组件,全站统一圆角与焦点环。

### Fixed (0.1.1)

- 发布流水线:Linux sidecar 改取 codex 的 `*-linux-musl` 资产(此前用 `-gnu` 命名导致下载
  404、Linux 打包失败);musl 为静态二进制,gnu 系统可直接运行。
- 外观:默认不再用 color-mix 覆写全站 token(仅覆写用户真正改动项),修复「默认就把配色改坏」;
  修复设置全屏整页高度塌陷导致内容显示不全。

### Fixed

- Run cancellation hardened: stopping / switching server / opening history /
  deleting an active run now truly interrupts the remote command and finalizes the
  task (no zombie "running" history entries; cancelled plans keep executed steps);
  the composer/buttons disable while a run is in flight (no concurrent ops); the
  doctor cancel handle is armed before any await.
- Add-server dialog clears its form (incl. secrets) on close; port field is editable;
  copy-to-clipboard only confirms on success; doctor cancel keeps prior facts;
  diagnosis with no result surfaces an error instead of a blank panel.

### Security

- Risk Reviewer firewall fix: `iptables -s <ip> -j DROP` (a write rule) was mis-classified
  as read-only Low because lowercasing conflated `-S` (list-rules) with `-s` (source match),
  letting a firewall write bypass confirmation / read-only mode. Now classified case-sensitively
  on the original command and rejected when any write/action flag is present. Regression-tested.
- Risk Reviewer `dd` fix: `dd of=/etc/passwd` (overwriting a critical file) was mis-classified as
  Low; `writes_to` now detects `dd of=<path>` so passwd/shadow → Blocked and `/etc` → High.
- Docker deploy `.env` files (containing generated DB passwords) are written under `umask 077`
  (owner-only, no longer world-readable 0644); container-name precheck now fails if the docker
  query itself errors instead of silently passing.

### Added

- Real-time server monitoring: a Codex-style dropdown "layer" with system facts and live
  CPU / memory / disk / load ring gauges (center percentages), container/service/port/process
  counts, and a network-traffic chart — sampled every 3s over read-only SSH with **no resident
  agent**; hover for details; explicit error state with retry on collection failure.
- In-app online updates: signed releases via GitHub Releases (Tauri updater plugin + minisign);
  versions managed by `vX.Y.Z` tags with `scripts/bump-version.sh`; check / download (progress) /
  install / relaunch from Settings, plus an optional silent check on startup. See
  `docs/RELEASE.zh-Hans.md`.
- Docker app deployment workflows: detect/install Docker, Compose deploy, Caddy/Nginx reverse
  proxy + HTTPS, post-deploy health checks; Uptime Kuma / n8n / WordPress / PostgreSQL / Redis
  templates — each a risk-reviewed Plan through confirm + execute.
- Codex app-server turn/tool loop: `thread/start` → `turn/start` → event stream → tool dispatch
  and relay (turn-error responses surfaced instead of hanging; tool results sanitized before
  reaching the AI), atop the JSON-RPC/stdio transport.
- Test & CI baseline: Rust unit/integration suite + a frontend **vitest** layer (formatting,
  risk display, api mocks, settings keys, audit output) wired into a one-shot `pnpm ci:check`
  gate (typecheck + tests + Codex sidecar integration + Clippy `-D warnings` + build).
- UX polish: command palette shortcuts help + recently-used servers; relative timestamps with
  exact-time hover; one-click copy of audited commands; a terminal "clear" button; a guided
  three-step first-run checklist.
- Simplified provider setup (Codex-style): new providers default to OpenAI-compatible
  — you only configure a base URL + API key. Models are auto-discovered via
  `GET {base}/models` instead of typed by hand: pick from a dropdown in settings, and
  switch the active model right from the home composer (persisted per provider). The
  Codex app-server / custom kinds remain available but are no longer the default.
- Foreground health monitoring: while the app is visible it polls server
  connectivity every 60s; the 概览 nav shows a red badge counting servers needing
  attention (offline, or last doctor's disk/memory >90%), and cards flag "资源紧张".
- File manager ("文件"): browse the selected server's filesystem (SFTP over SSH —
  directory listing, view, edit/save, and upload/download via native file dialogs),
  Codex-style file tree with typed icons, breadcrumb path, and search.
  User-operated, not exposed to the AI agent.
- Visible connection flow: a "连接/重连" action with a live 连接中/已连接/失败 state on
  the server overview, a "正在连接 user@host…" banner in the terminal, and a
  连接中 indicator on dashboard cards while refreshing.
- Codex cool-white palette: white-dominant with cool slate grays (replacing the
  earlier warm-neutral pass), light + dark, AA contrast.
- Interactive SSH terminal ("终端"): a real, user-operated terminal for the selected
  server (xterm.js front-end + a local PTY wrapping `ssh` via portable-pty, streamed
  over a Tauri channel). User-driven and never exposed to the AI agent — the
  "no raw shell for the agent" boundary is unchanged.
- Codex-neutral color palette: warm off-white / near-black neutral grays with a
  restrained monochrome accent (de-saturated risk colors), light + dark.
- Editable plan execution: inspect and edit a generated plan before running —
  edit each step's command/summary, add/remove/reorder steps. Every edit is
  re-reviewed by the risk gate (server-side re-classification); blocked or empty
  plans can't run; the edited plan is persisted to task history; execution routes
  each step by its re-classified risk (not the stale model flag).
- Multi-server dashboard ("概览"): at-a-glance grid of all servers with live
  status, key metrics (load/memory/disk bars from the last doctor run), a
  "refresh all" that concurrently re-checks SSH connectivity, and click-to-open.
- Server favorites: star a server to pin it to the top of the dashboard and
  sidebar (persisted; backend orders favorites first).
- Structured AI-diagnosis result: a card showing the tool-call trace (args /
  result preview / errors, sanitized) plus the conclusion, persisted and
  restorable from history (no longer terminal-only).
- Command palette (⌘K / Ctrl-K): searchable, keyboard-reachable quick actions
  (new ask, doctor, audit, settings, theme/terminal toggle, read-only toggle,
  switch server).
- Doctor v2: structured metrics parsed from probes (memory used/total, root-disk %,
  load, service/container/port counts) surfaced as facts + a server overview with
  threshold-colored progress bars; extra read-only probes.
- Real run cancellation: stopping a doctor / plan execution now actually terminates
  the remote command (cancel registry + cancel_run + forced tty so the remote gets
  SIGHUP), not just stops listening.
- Audit search + JSON export, and task (run history) search.
- Hardened Risk Reviewer: command-name detection is now segment-aware (matches the
  head command of each pipeline segment), so keywords inside grep/echo/pipes/quotes
  no longer cause false positives; SQL-destructive statements only flag in an actual
  SQL execution context; real dangerous commands and read-only carve-outs unchanged.
- Settings: default-provider selector (model policy), credential-backend indicator
  (keychain/mock warning), clearer connection-test feedback, default read-only toggle.
- Toast notifications (success/error) surfaced across the app instead of only the
  terminal; SSH connectivity auto-checked when a server is selected (live status).
- Broader Risk Reviewer coverage (shutdown/reboot, disk wipe, PID 1 kill,
  /etc/passwd|shadow writes, firewall flush, package removal, account changes,
  crontab -r, file truncation, chattr, sysctl -w, authorized_keys removal, …),
  with read-only carve-outs preserved.
- Simplified-Chinese comments across the whole codebase.
- Real task/run model: every plan / AI diagnosis / doctor run is persisted per
  server and listed in the sidebar (restore or delete a run); the main screen is
  driven entirely by real data — no mock plans, terminal transcripts, titles, or
  history. First-run onboarding and a no-provider banner replace the empty shell.
- Streaming plan execution over a Tauri channel (live per-step status + output),
  with the same server-side re-review + confirmation enforcement as the blocking path.
- Provider fallback chain for planning (default → other enabled providers → local
  mock) so planning always works and degrades clearly.
- Real AI: OpenAI-compatible provider over HTTP (chat / structured-output planning
  / summarize), with AiPanel re-classifying every step's risk; create_plan uses the
  configured provider and falls back to the offline mock engine.
- Autonomous read-only diagnosis: the agent investigates via read-only AiPanel
  Tools (OpenAI function calling) and summarizes; write tools are never exposed to
  the autonomous loop.
- Codex app-server bridge: JSON-RPC/stdio transport (spawn + initialize advertising
  only the AiPanel Tools surface).
- Live streaming server doctor over a Tauri channel — the console terminal fills in
  per-step / per-line as it runs.
- Execute-confirmation dialog (medium = confirm, high = double-confirm checkbox,
  blocked = refused); server-side re-review enforces it.
- Server management UI: edit, update secret, and delete servers.
- Desktop MVP backend (Tauri v2, Rust): Core types + error layer, SQLite store
  with migrations, Keychain-backed credential store (mock fallback), Risk
  Reviewer, SSH executor over system OpenSSH with output sanitization, read-only
  Server Doctor, local audit log, Plan Engine (mock), Agent Provider abstraction
  with a Codex app-server entry point, and the AiPanel Tools capability layer.
- Tauri commands for servers, SSH connectivity, doctor, plans, risk review,
  audit, providers, and model-selection policy.
- Frontend wiring: real server list, add-server dialog, plan generation with risk
  labels, doctor execution streaming to the terminal, audit view, and a Provider
  Manager settings panel.
- AiPanel UI design system (`@aipanel/ui`), light/dark Codex-style theme, and the
  CodexConsole desktop UI.
- Initial project README.
- Simplified Chinese README.
- AGPL-3.0 license.
- gpt-image-2 generated logo and README preview assets.
- Open-source community files.
- Roadmap, architecture, and security model documents.
- Technical stack document with Codex app-server as Agent Runtime.
