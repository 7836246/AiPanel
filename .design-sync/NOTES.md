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
- `[FONT_MISSING]` — **RESOLVED.** Inter (400/500/600) + JetBrains Mono (400/500), latin-subset woff2 from fontsource, live in `packages/ui/fonts/` with `packages/ui/fonts.css` (`@font-face`). Shipped to the DS bundle via `cfg.extraFonts: ["fonts.css"]`, and to the desktop app via `@import "@aipanel/ui/fonts.css"`. If you bump the package version and the fonts change, re-download the same weights. Subset is latin-only — non-latin glyphs fall back.

## Styling model (for the conventions header)

- **Theming is light-first, class-based.** `tokens.css` holds light values in `@theme` (the `:root` default) and overrides `--color-*` in an **unlayered** `.dark` block — unlayered so it beats the layered `@theme :root` regardless of order/specificity. Add the `dark` class to any ancestor to flip the subtree. Components use semantic token utilities, so they switch automatically; no component code is theme-aware. The desktop app's `useTheme` hook toggles `dark` on `<html>` (default light, persisted to localStorage). If you add a token, define it in BOTH the `@theme` light block and the `.dark` block.
- No provider/theme wrapper. Components are pre-styled from `styles.css`.
- The shipped `styles.css` is STATIC (Tailwind compiled at `pnpm build:css`, scanning `packages/ui/src`). It contains only the utility classes the components themselves use — so the design agent's own glue must use the token CSS variables (`var(--color-*)`), not invented utility classes. The conventions header says exactly this.

## Re-sync risks (watch-list)

- **Previews never machine-rendered** (no chromium). If a component's API changed, its authored `.design-sync/previews/<Name>.tsx` could silently break the card — re-verify visually, or install chromium and run the render check.
- **Brand fonts are vendored** (`packages/ui/fonts/*.woff2`, committed) — not fetched at build time, so they're stable; latin subset only.
- **Preview content is illustrative** (server names, commands, exit codes) and tied to the current component props. If a domain component's prop shape changes (e.g. `PlanStep`, `ServerCardProps`), update the matching preview.
- The `style?: react.CSSProperties` lowercase-namespace quirk in emitted `.d.ts` is cosmetic (only the `style` prop) and passes the parse gate; left as-is rather than hand-writing `dtsPropsFor` for all 17.
