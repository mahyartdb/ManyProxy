---
name: typescript
description: Strict TypeScript guidance for typed JavaScript work. Use when writing or reviewing TS (tsconfig strictness, narrowing/guards, type vs interface, ESM, tsc --noEmit, ESLint, Vitest). Do not use for framework specifics (use vue, svelte, react-patterns); do not use for plain CSS or runtime config.
---

# TypeScript

Strict-first TypeScript. No `any` without a written reason; narrow at boundaries; prove types with `tsc --noEmit`.

## When to use

- New TS code, strictness upgrades, type refactors.
- Reviewing TS for soundness (guards, null handling, async types).

## Rules

1. **Strict on**: `strict`, `noUncheckedIndexedAccess`, `noImplicitOverride` where the codebase allows. New code must pass `tsc --noEmit` clean.
2. **Narrow at boundaries**: validate external data (API, storage, props from JS) with explicit guards or schema checks; never assert-and-pray.
3. **`type` vs `interface`**: `interface` for object shapes meant for extension/implementation; `type` for unions, tuples, mapped/conditional shapes.
4. **Null discipline**: `null` vs `undefined` is a deliberate choice per codebase; optional chaining + `??` over non-null assertions. `!` needs a comment with its invariant.
5. **Async types**: `Promise` return types explicit on exported functions; no floating promises (await or `void` with a reason).
6. **Modules**: ESM (`import`/`export`), `verbatimModuleSyntax`-safe (use `import type` for types). No `require` in TS sources.

## Tooling

- Typecheck: `tsc --noEmit` (CI gate).
- Lint: ESLint with typescript-eslint, no-explicit-any as error unless justified inline.
- Test: Vitest (or the repo's runner). Types are not tests: one runnable check per non-trivial branch.

## Done when

- [ ] `tsc --noEmit` clean, lint clean.
- [ ] Boundary inputs narrowed, no unjustified `any` or `!`.
