# Spec: ManyProxy UI polish sweep (library-first, all surfaces)

Status: spec (planning only — no implementation authorized by this document).
Scope source: impeccable UI-improvement sweep, user-confirmed direction:
Library first, all issues in scope, screenshots retaken now (done 2026-10-07).

## Kernel

### 1. Why

From the user's perspective: ManyProxy is a dense Operate tool — a personal
proxy library where every row is a live endpoint. Today the library reads as
a wall of packed text lines, the menus are silent to assistive tech, search
has no visible field, the detail pane's action row wraps at the minimum
window size, and several dialogs say one thing on their buttons while doing
another. Nothing moves, nothing answers hover or focus visibly. The user
asked for every modal, button, page, movement, and field to be better
quality so the whole app feels trustworthy and operable — starting with the
library, covering everything.

### 2. Capabilities

Numbered requirements. Each is user-observable and testable without
implementation knowledge. (IDs stable; never reuse.)

**CAP-1 — Scannable library rows.**
Intent: a user scanning 50 rows can tell name, state, and route at a glance.
1. Each row shows the config name at full emphasis and a serving indicator
   (running / stopped / failed) perceivable without color alone.
2. The meta line carries at most 3 facts; the full route
   (protocol, upstream, group, bind:port, enabled) remains available via
   tooltip and in the detail pane.
3. The serving lamp has a minimum 12px visual target; selection (wash) and
   bulk-checked (rim + numeral chip) remain visually distinct when combined.

**CAP-2 — Operable toolbar and menus for everyone.**
Intent: every toolbar and menu action is reachable, named, and announced.
1. All five lever actions (Add, Start, Stop, Test, Import) and all four menus
   (Library, Manage, Engine, Help) expose an accessible name containing the
   visible label plus shortcut where one exists.
2. Every menu item (Import, From clipboard, Export, Subscriptions, Presets,
   Groups, Start all, Stop all, Test all, About, Settings, Command palette)
   is announced by name to assistive tech.
3. The toolbar fits the 880px minimum width on one row without wrapping;
   below that it wraps to two labeled rows rather than clipping.

**CAP-3 — Discoverable search and filters.**
Intent: the user can find the search field and clear exactly what they mean.
1. The library search is a persistently visible text field with placeholder
   `Search configurations` and a live `N of M` count beside it.
2. The three clears are unambiguous: clearing the query, clearing the facet
   filters, and clearing the bulk-checked set each name their target and
   affect only that target.
3. Facet pickers (Group, Protocol, Status) show the active choice and the
   result count for the current choice.

**CAP-4 — Detail pane that fits the minimum window.**
Intent: endpoints and the primary action stay visible without scrolling at
the minimum size.
1. At 880x620 with a row selected, the endpoint values, their Copy buttons,
   and the Start control are visible without scrolling the inspector.
2. The operate row never exceeds two wrapped lines and never scrolls
   horizontally; management actions (Edit, Delete, Duplicate, resets) live
   behind exactly one disclosed menu.
3. Every pane (Details, Health, Presets) names the row it acts on
   (`Acting on: <name>`).

**CAP-5 — Honest dialog footers.**
Intent: a button's label always describes what pressing it does.
1. Closing a dialog without saving is labeled `Close` or `Cancel`, never a
   verb that implies an action (`Attach`, `Add`, `Done`) when no action runs.
2. Saving/confirming is labeled with its verb (`Add`, `Save`, `Import`,
   `Export`, `Delete`, `Overwrite`, `Reset`, `Clear`, `Restore`) and danger
   styling marks destructive confirms.
3. The Logs viewer distinguishes committing a filter change from discarding
   it in the footer labels.

**CAP-6 — Guided, forgiving forms.**
Intent: first-timers succeed without learning raw JSON; experts keep the
fast path.
1. The Add dialog opens guided (Name, Protocol, Server, Server-port,
   Identity with secret masking, Group, Bind, Port); raw JSON is a disclosed
   `Advanced` section, never the default.
2. Validation keeps the dialog open, shows an inline error naming the field
   and the fix, and moves focus to the first invalid field.
3. The Port field shows the live auto-port budget
   (`N of 499 in use (10801–10999, spill 18081–18380)`); non-loopback binds
   show the loopback-exposure hint before commit.

**CAP-7 — Import with live, specific feedback.**
Intent: the user knows what will happen before pressing Import.
1. The preview counts valid and invalid items live as text is typed or files
   are picked.
2. The first failure names its item number, label, and reason inline.
3. Total failure keeps the dialog open with the full failure list; partial
   success closes and reports `Imported N · M duplicates skipped · K failed`.

**CAP-8 — Understandable subscriptions.**
Intent: refresh scheduling is explicit at both levels.
1. The global auto-refresh switch and each subscription's own auto-refresh
   switch each state what they govern; refreshing explicitly is always
   available regardless of the switches.
2. Interval choices (always, 1h, 6h, 12h, 24h, 7d) are presented as one
   grouped exclusive choice with no redundant summary line.
3. Plain-HTTP subscription URLs carry a visible plaintext-fetch warning next
   to the URL field.

**CAP-9 — Restrained motion and focus.**
Intent: the app feels alive and keyboard-navigable without breaking its
flat signal-box world (no shadows, glow, gradients, glass — per DESIGN.md).
1. Dialog open/close, inspector tab switch, and bulk-bar appear/disappear
   animate at ≤200ms and honor reduced-motion settings.
2. Every interactive element shows a visible hover response and a visible
   keyboard-focus indicator.
3. In-flight work (start, test, ping) shows label + spinner on the acting
   button and ignores double-activation until done.

**CAP-10 — Trustworthy status, toasts, and empty states.**
Intent: the app never claims what it did not do.
1. The status strip shows library counts, engine/health state, and usage
   totals in fixed slots that do not shift layout when values change.
2. Every bulk or destructive action confirms by naming affected rows and
   reports per-item outcomes; empty libraries and empty filters each teach
   the next step with working buttons.
3. Copy actions move bind + port only, never credentials; logs, URLs, and
   labels stay redacted.

### User stories

**US-1 — Skim a 50-row library and start the right endpoint (P1).**
Covers CAP-1, CAP-4. As the pool owner, I want to spot the stopped
`demo-paris-p1` row and start it, so that my app can use its endpoint.
Why this priority: the library is the app's core surface; everything else
serves it.
Independent Test: with 50 seeded rows, locate a named stopped row and start
it using only the library + detail pane.
Acceptance Scenarios:
1. Given 50 rows with mixed states, when scanning the list, then each row's
   name and serving state are distinguishable without opening the detail.
2. Given a stopped row selected at 880x620, when viewing the inspector,
   then endpoints, Copy buttons, and Start are visible without scrolling.
3. Given a row both selected and bulk-checked, when viewing it, then both
   the selection wash and the checked rim + numeral chip are visible.

**US-2 — Import three share links without fear (P1).**
Covers CAP-7, CAP-6. As a new user, I want to paste links and see what will
import, so that I don't commit blindly.
Why this priority: import is the main onboarding path into the library.
Independent Test: paste 2 valid + 1 invalid link; verify preview, fix, import.
Acceptance Scenarios:
1. Given pasted text with 2 valid and 1 invalid items, when the preview
   updates, then it reads `2 valid · 1 invalid` with the failure's item
   number, label, and reason inline.
2. Given a fully invalid paste, when pressing Import, then the dialog stays
   open listing every failure and nothing is added.
3. Given a partial paste, when import completes, then the toast reports
   imported, duplicates-skipped, and failed counts and the new rows arrive
   selected and checked.

**US-3 — Schedule subscription refreshes confidently (P2).**
Covers CAP-8. As a subscription user, I want explicit refresh scheduling,
so that I know when my pool updates.
Why this priority: silent or surprising refreshes violate the explicitness
brand; less critical than library/import.
Independent Test: add a subscription, set 24h interval, trigger manual
refresh with global auto-refresh off.
Acceptance Scenarios:
1. Given the Subscriptions dialog open, when reading the interval control,
   then exactly one grouped exclusive choice of
   always/1h/6h/12h/24h/7d is presented with the current value marked.
2. Given global auto-refresh off, when pressing Refresh on one
   subscription, then only that subscription refreshes and reports its
   outcome.
3. Given a plain-HTTP subscription URL, when the URL field is non-empty,
   then a plaintext-fetch warning is visible beside it.

**US-4 — Drive the whole app by keyboard (P2).**
Covers CAP-2, CAP-9. As a keyboard user, I want every action reachable and
announced, so that I never need the mouse.
Why this priority: Operate tools live on shortcuts and focus; the current
menus are silent to assistive tech.
Independent Test: complete add → start → test → copy-URLs using keyboard only.
Acceptance Scenarios:
1. Given the main window focused, when tabbing through toolbar and menus,
   then every control announces its name (and shortcut where bound) and
   shows a visible focus indicator.
2. Given a dialog open, when pressing its confirm key, then the same commit
   runs as clicking the footer button.
3. Given reduced-motion enabled, when opening dialogs and switching tabs,
   then state changes remain perceivable without animation.

**US-5 — Trust what the app says (P2).**
Covers CAP-5, CAP-10. As a careful owner, I want buttons and toasts that say
what they do, so that I never lose data to a misleading label.
Why this priority: honesty is the brand voice; footer lies directly break it.
Independent Test: walk every dialog footer and confirm label matches effect.
Acceptance Scenarios:
1. Given any dialog, when reading its footer, then the commit button names
   its action and destructive commits carry danger styling.
2. Given a bulk delete of checked rows including running ones, when
   confirming, then the confirm names the count, promises stop-first, and
   the toast reports per-row outcomes.
3. Given a copy action anywhere, when pasting the clipboard, then it
   contains bind + port only and no credential material.

### 3. Constraints

1. Windows 10/11 64-bit desktop app, single crate, edition 2021; no
   workspace changes.
2. Local-only always: no system proxy, TUN, DNS rewrite, routing editor,
   elevation, services/drivers, telemetry, or silent binary downloads
   (guarded by `assert_no_system_mutation`-family tests — keep green).
3. Sidecars resolve from the app directory only, never `PATH`; never download.
4. Secrets: sealed at rest, redacted in logs/URLs/labels, clipboard carries
   bind:port only; export keeps its verbatim plaintext warning.
5. Visual world locked by DESIGN.md ("signal box"): flat, hairlines only,
   one 6px radius, two text sizes, signal-only color; no new palette,
   typeface, shadow, glow, or gradient without replacing the system.
6. Every headless-visible control keeps its stable test id; pinned strings
   (toasts, confirms, counts) change only with test updates in the same
   ticket.
7. Verify per ticket: `cargo fmt --check`, `cargo clippy --all-targets
   --locked -- -D warnings`, `cargo test --locked`.

### 4. Non-goals

1. No engine, health-probing, import-grammar, store-schema, or tray-behavior
   changes — presentation and wording only (explicitly not doing per
   `.planning/ui-complete-gpui/tasks.md`).
2. No new features (no new dialogs, menus, shortcuts, or settings).
3. No DESIGN.md replacement or rebrand; refinement preserves the incumbent
   identity per the impeccable brief-wins rule.
4. No web/mobile/responsive work; minimum-frame containment (880x620) only.
5. No screenshot tooling or harness changes beyond replacing the four
   `assets/screenshots/*.png` captures after the build.

### 5. Success signal

1. A first-time user completes add → start → copy-endpoint → test using
   keyboard alone in under 5 minutes on the minimum window without asking
   for help.
2. Zero footer-label lies: an independent walk of all 20+ dialogs finds no
   button whose label misstates its effect.
3. Assistive-tech pass: every toolbar/menu/dialog control announces its name
   (and shortcut where bound) in a clean traversal.
4. Fresh `assets/screenshots/*.png` (library, add, import, subscriptions)
   match the shipped UI on a seeded demo library with no real secrets.

## Seams

Preferred existing seams (highest first, fewest new):

1. Headless UI tests in `src/main.rs` `mod tests` (`:13709`): `assert_painted`
   (`:19774`) id-presence checks over `VisualTestContext`, pinned-string
   asserts (toasts, confirms, counts), `assert_no_system_mutation`-family
   scans. New coverage lands here; no new harness.
2. Text-function seams: `serving_badge`, `library_row_lines*`,
   `search_count_text`, `bulk_label`, status-bar text fns, confirm-description
   fns — assert wording without rendering.
3. Live proof (opt-in, post-implementation only): debug build + isolated
   `MANYPROXY_DB` demo seed + cua-driver capture of the four screenshots at
   1000x700; compares against `assets/screenshots/*.png`.
4. New seams only if unavoidable: one grouped-radio test helper for the
   interval/level pickers; nothing per-row (per-row start tracking stays
   forbidden by DESIGN.md).

## Boundaries

**Always**
- Keep all pinned element ids and pinned strings green or update tests in
  the same ticket with ledger rationale.
- Keep honest-toast behavior: every action stays enabled and explains itself;
  never introduce disabled-control styling.
- Keep secrets discipline: redaction, DPAPI at rest, bind:port-only clipboard.
- Run the per-ticket verify trio (fmt, clippy, locked tests) before claiming
  done.
- Recapture the four screenshots after the build on the seeded demo library.

**Ask first**
- Any pinned-string rewording beyond the footer-lie fixes (README-claimed
  labels are authoritative).
- Any DESIGN.md token change (radius, palette, type scale) — refinement
  preserves the system; replacement needs explicit approval.
- Touching tray close-confirm semantics or autostart behavior.

**Never**
- System mutation paths (proxy/TUN/DNS/routing/elevation/services/telemetry/
  silent downloads), `PATH`-based sidecar lookup, plaintext secret storage.
- Bare `OK` footers, disabled-without-explanation controls, per-row start
  spinners, fabricated latency for stopped rows, Kbd chips on unbound actions.
- Shipping screenshots containing real upstream hosts, credentials, or tokens.

## Assumptions

ASSUMPTIONS I'M MAKING:
1. Order is library surfaces first (rows, toolbar/menus, search/facets,
   detail, status), then dialogs (footers, Add, Import, Subscriptions),
   then motion/focus, then final polish + screenshots (not animation-first).
2. The lever-frame toolbar (5 throws + 4 menus) is the approved structure;
   work refines it rather than restoring the old 13-button strip.
3. Fresh screenshots use synthetic `*.example.com` demo rows on an isolated
   `MANYPROXY_DB`, matching the 2026-10-07 retake (not the user's live DB).
4. `audit`/`layout`/`clarify`/`animate`/`polish` are the fix vehicles; no new
   impeccable commands are introduced.
5. Minimum frame stays 1000x700 default / 880x620 minimum.
→ Correct me now or I'll proceed with these.

## Verification

- Every CAP has at least one US scenario exercising it; every scenario names
  Given/When/Then with observable outcomes (counts, labels, visibility,
  clipboard content) — no "works correctly".
- UI stories verify by headless id/string asserts (code grader) plus live
  window capture at 1000x700 comparing the four screenshots (human grader
  for layout judgment, model grader against the CAP checklist for wording).
- No `[NEEDS CLARIFICATION]` markers remain; at most 3 ever existed and all
  were resolved into Assumptions above.
- Two-pass self-check done: coherence (stories cover CAP-1…CAP-10 with no
  orphan CAP; non-goals exclude engine/store/tray logic consistently) and
  preservation (every sweep finding — menus UIA, row density, search,
  operate wrap, footers, guided forms, import preview, intervals, motion,
  status shift, footer minors — landed in a CAP or non-goal).
- Capability evals: pass@3 ≥ 0.90 on headless suite for touched seams.
  Regression evals: `cargo test --locked` fully green (pass^3 = 1.00) plus
  `assert_no_system_mutation`-family green before merge.
- Eval anti-patterns avoided: seeded demo rows (not golden screenshots)
  drive layout tests; keyboard-only and reduced-motion paths measured, not
  just happy-path clicks; no flaky timing graders (in-flight states assert
  via state flags, never sleeps).

## Companions

- Visual evidence: `assets/screenshots/library.png`, `add-dialog.png`,
  `import-dialog.png`, `subscriptions-dialog.png` (retaken 2026-10-07 on
  the lever-frame build; stale pre-menu captures deleted).
- System of record: `PRODUCT.md` (product context), `DESIGN.md` (signal-box
  world: tokens, selection grammar, honest-toast, no-disabled rules).
- Prior art: `.planning/ui-complete-gpui/tasks.md` (T01–T11, pinned seams),
  `.scratch/ui-production-pass.md` exit predicate, README claim → code map.
- No new glossary/topology companions needed; kernel cites these in place.

## Sources

1. User statements (2026-10-07 session): "@impeccable look for ui
   improvement opportunities… every modal/button/page/movement/field";
   "screenshots are old — delete and retake, tell subagents too";
   direction answers: Library first / All issues / Retake now; "@spec all
   of this". → Landed: Why, CAP-1…CAP-10, US-1…US-5, Non-goals 5, Assumptions.
2. Dialog-mapper subagent (live `src/main.rs` + store/engine/tray):
   21-dialog inventory with fields/buttons/validation/anchors and 10 UX gaps
   (footer lies, danger asymmetry, preview staleness, reopen jank, missing
   anchors, toast-only errors, silent-when-dialog-open shortcuts, tray
   bifurcation, ungrouped radios, stacked-hint overflow). → Landed:
   CAP-5, CAP-6, CAP-7, CAP-8, CAP-9-part, US-2, US-3, US-5.
3. Surface-mapper subagent (live `src/main.rs:956-13586` + tray/preset):
   toolbar/menus, rows, inspector, filters, status, empty/toast/spinner,
   shortcuts, motion audit (zero animation hits, single hover rule). →
   Landed: CAP-1, CAP-2, CAP-3, CAP-4, CAP-9, CAP-10, US-1, US-4, Seams.
4. Fresh captures (1000x700, isolated demo DB): lever-frame toolbar,
   guided-first Add with port budget, Import with live preview, Subscriptions
   with grouped radios, selected-row detail with endpoints + Copy buttons. →
   Landed: Success signal 4, Companions, Assumptions 3.
5. Repo authorities: `AGENTS.md` (commands, constraints, hermetic tests),
   `PRODUCT.md:23,35,41` (toolbar/shortcuts/toasts/stale-shots note),
   `DESIGN.md` (signal-box rules, selection grammar, honest-toast,
   no-disabled, minimum frame). → Landed: Constraints, Boundaries, Seams 1–2.
6. `.planning/ui-complete-gpui/tasks.md` pinned seams + T10 procedure. →
   Landed: Constraints 6–7, Seams, Non-goals 1.

## Maintain

| Role | Owner (reuse in place, link don't copy) |
| ---- | --------------------------------------- |
| Constitution | `AGENTS.md` + DESIGN.md Do's/Don'ts (local-only, honest-toast, secrets, no-disabled, theme-token rules). This spec adds nothing to governance. |
| Map | `src/main.rs` (monolith UI), `src/store.rs` (library/DB), `src/engine.rs` (sidecars/LAN), `src/health.rs` (probes), `src/tray.rs` (tray), `assets/screenshots/*.png`, PRODUCT.md/DESIGN.md. |
| Status | Suite green (`cargo test --locked`), screenshots fresh 2026-10-07, PRODUCT.md:41 stale-note now outdated (update that line when this spec lands). Delete-zone: old 13-button screenshots (deleted, do not recreate). |
| History | `.planning/ui-complete-gpui/` ledger (T01–T11), 2026-10-07 retake note (isolated demo DB, lever-frame baseline). |

Next (not executed here): `/plan` → `/to-tickets` → `/decide` → `/implement`.
