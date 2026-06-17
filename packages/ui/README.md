# @aipanel/ui

AiPanel's design system: token-driven primitives plus ops-domain components, built with React + Tailwind v4.

## Install / consume

It's a workspace package — depend on it with `"@aipanel/ui": "workspace:*"`.

```tsx
import { Button, RiskBadge, CommandPlan } from "@aipanel/ui";
```

Pull in styles one of two ways:

- **Inside an app that already runs Tailwind v4** (like `@aipanel/desktop`): import the shared tokens and point Tailwind at this package's source so it keeps the component classes:
  ```css
  @import "tailwindcss";
  @import "@aipanel/ui/tokens.css";
  @source "../../../packages/ui/src/**/*.{ts,tsx}";
  ```
- **Standalone**: import the precompiled stylesheet `@aipanel/ui/styles.css` (produced by `pnpm build`).

## Design tokens

Tokens live in `src/styles/tokens.css` as a Tailwind v4 `@theme` block, so every value is both a CSS variable and a generated utility:

| Group | Examples | Utilities |
| --- | --- | --- |
| Surfaces | `--color-bg`, `--color-surface-1..3`, `--color-border` | `bg-surface-1`, `border-border` |
| Foreground | `--color-fg`, `--color-fg-muted`, `--color-fg-subtle` | `text-fg-muted` |
| Brand / accent | `--color-brand`, `--color-accent` | `bg-brand`, `text-accent` |
| Risk scale | `--color-risk-{low,medium,high,blocked}` (+ `-soft`) | `text-risk-high`, `bg-risk-low-soft` |
| Radius / fonts | `--radius-md`, `--font-mono` | `rounded-md`, `font-mono` |

The palette is dark-first and the risk scale mirrors `docs/SECURITY_MODEL.zh-Hans.md`.

## Components

- **Primitives**: `Button`, `Badge`, `Card` (+ `CardHeader/Title/Description/Content/Footer`), `Input`, `Textarea`, `Spinner`, `CodeBlock`, `Dialog`.
- **Domain**: `RiskBadge` (low/medium/high/blocked), `ServerCard`, `CommandPlan` (+ `PlanStep`), `AuditEntry`.

Variants use `class-variance-authority`; conflicting classes resolve through `cn` (`clsx` + `tailwind-merge`). Keep components on Tailwind utility classes only — no ad-hoc CSS — so the bundle stays portable and `/design-sync`-able.

## Build

```sh
pnpm build      # tsup → dist/index.js + .d.ts, tailwind → dist/styles.css
pnpm typecheck
```

The `dist/` output (compiled components + `styles.css`) is also what a future `/design-sync` import consumes.
