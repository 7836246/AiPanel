# Building with AiPanel UI

AiPanel UI is the design system for **AiPanel**, a local AI server-operations console. It is **dark-first** and ops-flavored: surfaces are near-black, text is light, and a four-level **risk scale** (low / medium / high / blocked) carries the security meaning. Build screens that look like a calm, dense operations console — not a marketing site.

## Setup — no provider, just a dark surface

There is **no provider or theme wrapper**. Every component is self-contained and already styled from the bundle's `styles.css`; import it and render. The only thing you must do is put content on a dark surface, because tokens assume it:

```jsx
// AiPanelUI.* are the library components (from the bundle global)
<div style={{ background: "var(--color-bg)", color: "var(--color-fg)", minHeight: "100%" }}>
  {/* your screen */}
</div>
```

Skip the dark background and components still render, but they'll sit on white and look wrong.

## Styling idiom — compose components; style glue with token variables

Two rules:

1. **Prefer composing library components** over hand-built markup. They already carry the spacing, color, and radius of the system.
2. **For your own layout/glue, use the design tokens as CSS variables** (`var(--token)`) — via inline `style` or a `<style>` block. The shipped stylesheet is static, so invent-your-own utility classes will not resolve; the tokens always will.

Token vocabulary (all are `var(--…)`):

| Group | Tokens |
|---|---|
| Surfaces | `--color-bg`, `--color-surface-1`, `--color-surface-2`, `--color-surface-3` |
| Borders | `--color-border`, `--color-border-strong` |
| Text | `--color-fg`, `--color-fg-muted`, `--color-fg-subtle` |
| Brand / accent | `--color-brand`, `--color-brand-strong`, `--color-accent` |
| Risk | `--color-risk-low`, `--color-risk-medium`, `--color-risk-high`, `--color-risk-blocked` (+ each `-soft` for fills) |
| Status | `--color-success`, `--color-warning`, `--color-danger`, `--color-info` |
| Radius / type | `--radius-sm/md/lg/xl`, `--font-sans`, `--font-mono` (use `--font-mono` for commands & output) |

Use semantic surfaces in order: page = `--color-bg`, cards = `--color-surface-1`, nested fills = `--color-surface-2/3`. Never hardcode hex; reach for the token.

## Use the domain components for domain meaning

Don't reinvent these — they encode AiPanel's model:

- **`RiskBadge`** `level="low|medium|high|blocked"` — the canonical way to show operation risk. Use it, not a generic colored tag, wherever a command/step has risk.
- **`CommandPlan`** `goal` + `steps[]` (`{summary, command, risk, readOnly?}`) — a reviewable execution plan.
- **`ServerCard`** `name` / `host` / `status` / `facts` — a saved server.
- **`AuditEntry`** `timestamp` / `command` / `risk` / `exitCode` / `output?` — one audit-trail line.
- Primitives: `Button` (`variant` primary/secondary/ghost/outline/danger; use `danger` for confirmed high-risk actions), `Badge`, `Card` (+ `CardHeader/CardTitle/CardDescription/CardContent/CardFooter`), `Input`, `Textarea`, `Spinner`, `CodeBlock` (commands/output), `Dialog` (confirmations — high-risk needs a second one).

## Where the truth lives

Before styling, read the bound `styles.css` (and its `@import`ed `_ds_bundle.css`) for the exact token values, and each component's `<Name>.prompt.md` / `<Name>.d.ts` for its real props. The files beat any summary here.

## Idiomatic snippet

```jsx
<div style={{ background: "var(--color-bg)", color: "var(--color-fg)", padding: 24 }}>
  <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
    <AiPanelUI.ServerCard
      name="web-prod-1"
      host="root@10.0.0.4:22"
      status="online"
      facts={{ OS: "Ubuntu 22.04", CPU: "12%", Disk: "44%" }}
    />
  </div>
  <div style={{ marginTop: 24 }}>
    <AiPanelUI.CommandPlan
      goal="Recover the unreachable website on web-prod-1"
      steps={[
        { summary: "Check nginx", command: "systemctl status nginx", risk: "low", readOnly: true },
        { summary: "Restart nginx", command: "systemctl restart nginx", risk: "medium" },
      ]}
    />
  </div>
</div>
```
