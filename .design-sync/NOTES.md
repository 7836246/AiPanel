# design-sync notes — @aipanel/ui

Repo-specific gotchas for future syncs. Read this first.

## Build / environment

- Monorepo (pnpm workspace). The DS package is `@aipanel/ui` at `packages/ui`.
- Rebuild the DS with `pnpm --filter @aipanel/ui build` (recorded as `cfg.buildCmd`). It runs tsup (JS + `.d.ts`) then tailwind CLI (`dist/styles.css`).
- Converter invocation (from repo root):
  `node .ds-sync/package-build.mjs --config .design-sync/config.json --node-modules ./packages/ui/node_modules --entry ./packages/ui/dist/index.js --out ./ds-bundle`
- `--node-modules` MUST be `./packages/ui/node_modules` — pnpm symlinks `react`/`react-dom` there; the repo-root `node_modules` has no `react`.
- `.ds-sync/` deps: `esbuild ts-morph @types/react typescript` (typescript added so validate runs the `.d.ts` parse check).
- `cfg.componentSrcMap` pins the 5 Card sub-parts (CardHeader/Title/Description/Content/Footer) to `Card.tsx` so they group under `primitives` instead of falling to `general`.
- `cfg.overrides`: Dialog = `single` (open overlay); CommandPlan / AuditEntry / ServerCard = `column` (wide).

## Known render warns (triaged, expected)

- `[RENDER_SKIPPED]` — render check is intentionally OFF. No chromium/playwright is installed in this environment; the user chose to review previews in their own browser (`.review.html`). **Previews are not machine-verified.** A future run with chromium should drop `--no-render-check` and run the full render check + `package-capture` grading.
- `[FONT_MISSING]` "Inter", "JetBrains Mono" — tokens name these families but the repo ships no font files, so the bundle renders in the fallback stack (`ui-sans-serif`/`system-ui`, `ui-monospace`). This is currently **accepted** (the token fallbacks are intentional). To ship the real brand fonts later: add the `.woff2` + `@font-face` and point `cfg.extraFonts` at them. Surface this to the user on each sync until resolved.

## Styling model (for the conventions header)

- No provider/theme wrapper. Components are pre-styled from `styles.css`.
- The shipped `styles.css` is STATIC (Tailwind compiled at `pnpm build:css`, scanning `packages/ui/src`). It contains only the utility classes the components themselves use — so the design agent's own glue must use the token CSS variables (`var(--color-*)`), not invented utility classes. The conventions header says exactly this.

## Re-sync risks (watch-list)

- **Previews never machine-rendered** (no chromium). If a component's API changed, its authored `.design-sync/previews/<Name>.tsx` could silently break the card — re-verify visually, or install chromium and run the render check.
- **FONT_MISSING unresolved** — every design renders in fallback fonts until brand fonts are wired.
- **Preview content is illustrative** (server names, commands, exit codes) and tied to the current component props. If a domain component's prop shape changes (e.g. `PlanStep`, `ServerCardProps`), update the matching preview.
- The `style?: react.CSSProperties` lowercase-namespace quirk in emitted `.d.ts` is cosmetic (only the `style` prop) and passes the parse gate; left as-is rather than hand-writing `dtsPropsFor` for all 17.
