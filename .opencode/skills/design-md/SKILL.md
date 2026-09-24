---
name: design-md
description: Author and validate DESIGN.md design-system files (token frontmatter + rationale prose) per the google-labs-code/design.md alpha spec. Use when creating or updating a DESIGN.md, linting one (broken refs, WCAG contrast), diffing versions for regressions, or exporting tokens to Tailwind v4 @theme / DTCG. Do NOT use for UI auditing (use impeccable) or component code (use the framework skill). Emitted by hallmark/impeccable; consumed by plan/implement as locked tokens.
---

# Design-md

DESIGN.md is a self-contained, plain-text representation of a design system: machine-readable design tokens (YAML frontmatter) plus human-readable rationale (markdown prose). Tokens are the normative values; prose provides context for how to apply them. Source: `google-labs-code/design.md` (spec version alpha; spec + philosophy + festival example vendored in `references/`).

## Author

Frontmatter block must begin and end with a line containing exactly `---`. Schema:

```yaml
version: alpha # optional
name: <string>
description: <string> # optional
omitted: <string[]|OmittedSection[]> # optional, silences missing-section warnings
colors:
  <token-name>: <Color> # any CSS color; hex recommended
typography:
  <token-name>: <Typography> # fontFamily, fontSize, fontWeight, lineHeight, letterSpacing (+ fontFeature, fontVariation)
rounded:
  <scale-level>: <Dimension> # px, em, rem
spacing:
  <scale-level>: <Dimension | number>
components:
  <component-name>:
    <token-name>: <string|token reference> # backgroundColor, textColor, typography, rounded, padding, size, height, width
```

Cross-reference with `{path.to.token}` (e.g. `{colors.primary}`, `{rounded.md}`). References must resolve; components may reference composite values (e.g. `{typography.label-md}`).

Body sections use `##` headings, in this order (omit what is irrelevant, keep the order of what remains):

1. Overview (alias: Brand & Style) 2. Colors 3. Typography 4. Layout (alias: Layout & Spacing) 5. Elevation & Depth (alias: Elevation) 6. Shapes 7. Components 8. Do's and Don'ts

Unknown section headings are preserved, unknown token names accepted if valid, duplicate section headings are an error. Recommended names (non-normative): colors `primary, secondary, tertiary, neutral, surface, on-surface, error`; typography `headline-*, body-*, label-*`; rounded `none, sm, md, lg, xl, full`.

Prose doctrine (from PHILOSOPHY.md): a specific reference beats adjectives ("1970s graduate lecture handout" beats "modern, clean, premium"). Name what is left out: negative constraints in Do's and Don'ts define character. Open sections beyond the minimum (motion, iconography) are fine as custom keys with prose.

## Direction discipline

A DESIGN.md written without real direction is a **draft**, not a deliverable: label it _"draft without direction"_ and say what is missing. Never silently fall back to a neutral default and present it as a system. Downstream UI work consumes DESIGN.md as direction; if a UI rule collides with DESIGN.md, that collision is the owner's call, recorded in one line.

## Validate

CLI: `@google/design.md` on npm. Lint, diff, and export output JSON by default.

```
npx -p @google/design.md designmd lint DESIGN.md
npx -p @google/design.md designmd diff DESIGN.md DESIGN-v2.md
npx -p @google/design.md designmd export --format css-tailwind DESIGN.md > theme.css
```

(Use the `designmd` alias: the `design.md` bin name collides with the Windows Markdown file association. All commands accept a file path or `-` for stdin.)

Lint rules: `broken-ref` (error, unresolvable `{path}` refs), `contrast-ratio` (warning, component text/background below WCAG AA 4.5:1), `missing-primary` / `missing-typography` / `orphaned-tokens` (warnings), `section-order` / `unknown-key` / `token-like-ignored` (warnings), `token-summary` / `missing-sections` / `omitted-rules` (info). Exit code 1 on errors. Gate: zero errors before the file is consumed by a build.

Diff exits 1 on regression (more errors/warnings in `after`). Never accept a v2 with `regression: true` without owner sign-off.

## Export

| Format                             | Output                                             |
| ---------------------------------- | -------------------------------------------------- |
| `css-tailwind`                     | Tailwind v4 `@theme` block (CSS custom properties) |
| `json-tailwind` (`tailwind` alias) | Tailwind v3 `theme.extend` object                  |
| `dtcg`                             | W3C Design Tokens JSON                             |

## Done when

- [ ] Frontmatter parses, sections ordered, refs resolve (`lint`: 0 errors)
- [ ] Contrast pairs pass WCAG AA or carry explicit owner overrides
- [ ] Direction labeled (real system, or _"draft without direction"_)
- [ ] v2 diffs show `regression: false`
