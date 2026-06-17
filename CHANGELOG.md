# Changelog

All notable changes to AiPanel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project plans to follow semantic versioning after the first public release.

## [Unreleased]

### Added

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
