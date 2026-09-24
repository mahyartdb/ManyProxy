---
name: css
description: Modern vanilla CSS guidance (cascade/layers, flex/grid, container queries, variables, responsive, reduced-motion). Use when writing or reviewing stylesheets, debugging specificity/layout, Tailwind v4 theme work needs cascade fundamentals. Do not use for UnoCSS engine config (use unocss) or component framework styling.
---

# CSS

Cascade-first modern CSS. Specificity is budgeted; layout is chosen, not stacked.

## When to use

- New stylesheets, specificity/layout bugs, responsive or theme work.
- Reviewing CSS for cascade, performance (`transform`/`opacity`-only motion), accessibility (contrast, focus, reduced-motion).

## Rules

1. **Layers first**: `@layer` order (reset, base, components, utilities) decides ties before specificity does. No `!important` without a comment naming what it beats and why.
2. **Layout choice**: grid for two-dimensional placement, flex for one-dimensional flow. Container queries (`@container`) for component-local response; media queries for page-level breakpoints.
3. **Variables**: design tokens as custom properties on `:root`/theme scopes; dark mode via `prefers-color-scheme` + explicit toggle class, both verified.
4. **Responsive**: mobile-first, no horizontal overflow at 360px; tap targets >= 44px; text never escapes its container.
5. **Motion**: animate `transform`/`opacity` only; honor `prefers-reduced-motion` (collapse to instant state changes).
6. **States**: every interactive element has `:hover`, `:focus-visible`, `:active`, `:disabled` accounted for; focus never removed without a replacement indicator.

## Tooling

- Lint: Stylelint for declaration order, duplicate properties, unknown units.
- Verify: resize 360/768/1440, keyboard-only pass, both themes, reduced-motion on.

## Done when

- [ ] No specificity wars, no `!important` without reason.
- [ ] Responsive + themes + reduced-motion + focus verified.

## Related

- Skills: `tailwind` / `unocss` (utility engines), `accessible-frontend` (contrast, focus, reduced-motion), `animate` (motion values)
