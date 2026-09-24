---
name: tailwind
description: Tailwind theme authoring from DESIGN.md tokens (v3 theme.extend, v4 @theme). Use when writing or migrating a Tailwind theme, mapping design tokens to utilities, setting up dark mode, or enforcing responsive discipline. Do NOT use for UnoCSS engine config (use unocss), vanilla cascade fundamentals (use css), or token authoring itself (use design-md).
---

# Tailwind

Themes are generated from DESIGN.md tokens, not hand-invented. Token names follow the spec recommended list (`primary, secondary, tertiary, neutral, surface, on-surface, error`; `headline/body/label`; `sm, md, lg, xl, full`). Reference implementation: `../design-md/references/totality-festival/` (DESIGN.md + tailwind.config.js side by side).

## Write the theme

Prefer generation over hand-mapping:

```
npx -p @google/design.md designmd export --format css-tailwind DESIGN.md > theme.css
npx -p @google/design.md designmd export --format json-tailwind DESIGN.md > tailwind.theme.json
```

- `css-tailwind`: Tailwind v4 `@theme` block (`--color-*`, `--font-*`, `--text-*`, `--leading-*`, `--tracking-*`, `--font-weight-*`, `--radius-*`, `--spacing-*`).
- `json-tailwind` (`tailwind` alias): Tailwind v3 `theme.extend` object for `tailwind.config.js`.
- `dtcg`: W3C Design Tokens JSON for cross-tool use.

Hand-map only what export cannot express, following the totality pattern: colors flat under `extend.colors` (v3), type levels as `fontFamily` + `fontSize` tuples (size, lineHeight, letterSpacing, fontWeight), radii under `borderRadius`, layout constants under `spacing`. Never invent a token the DESIGN.md does not define; missing tokens go back to `design-md`, not into the config.

## v3 vs v4 (never mix)

- v3: `tailwind.config.js` with `theme.extend`; dark mode via `darkMode: "class"`.
- v4: CSS-first `@theme` block; no config file for tokens.
- Migrating v3 to v4 means moving every `extend` entry to its `@theme` namespace and deleting the JS token config. Migrate whole, never half: a split theme (some tokens in JS, some in CSS) is a defect.

## Discipline

- Dark mode: strategy per project (class or media); every component verified in both themes before delivery.
- Responsive: mobile-first utilities; no horizontal overflow at 360px; tap targets at least 44px.
- Motion: `transform`/`opacity` utilities only; `prefers-reduced-motion` collapses to instant state (see `css`).
- Cascade fundamentals (layers, specificity budget, container vs media queries): see `css`. Engine questions for UnoCSS projects: see `unocss`.

## Done when

- [ ] Every utility token traces to a DESIGN.md token (generated, or hand-mapped with a reason)
- [ ] v3 xor v4 (never mixed), dark mode verified both themes, 360/768/1440 checked

## Related

- Skills: `css` (cascade fundamentals), `unocss` (alternative engine), `accessible-frontend` (contrast, focus, reduced-motion), `animate` (motion values)
