# Changelog

All notable changes to AiPanel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project plans to follow semantic versioning after the first public release.

## [Unreleased]

### Added

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
