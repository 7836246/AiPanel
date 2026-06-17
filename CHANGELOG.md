# Changelog

All notable changes to AiPanel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project plans to follow semantic versioning after the first public release.

## [Unreleased]

### Fixed

- Run cancellation hardened: stopping / switching server / opening history /
  deleting an active run now truly interrupts the remote command and finalizes the
  task (no zombie "running" history entries; cancelled plans keep executed steps);
  the composer/buttons disable while a run is in flight (no concurrent ops); the
  doctor cancel handle is armed before any await.
- Add-server dialog clears its form (incl. secrets) on close; port field is editable;
  copy-to-clipboard only confirms on success; doctor cancel keeps prior facts;
  diagnosis with no result surfaces an error instead of a blank panel.

### Added

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
