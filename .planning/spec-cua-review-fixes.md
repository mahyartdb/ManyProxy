# CUA live-review fixes — Spec (source of truth)

## Why

A live cua-driver drive of the fresh throne-adoption build (isolated `MANYPROXY_DB`,
4 `*.example.com` rows, dark default) found the shell functionally complete but
turned up 7 defects/gaps: 2 paint-level, 1 redundancy, 4 verification/accessibility
gaps. Users meet the first two on their very first Add and first Settings open —
truncated format hints and a footer-clipped line read as broken even though the
logic is right. This spec turns all 7 into buildable, measurable work without
reopening any settled design.

## Capabilities

- **CAP-1 — Guided placeholder legibility**: every Add/Edit placeholder reads in
  full at the 880px minimum window width. Intent: format hints (`e.g. …`) must be
  readable where the user types. Success: zero truncated placeholder pixels in the
  guided Server/Port half-columns at 880×620 and 1000×700, dark and light.
- **CAP-2 — Dialog fold integrity**: informational tails never straddle a sticky
  footer. Intent: first-paint Settings shows the whole Timers line above the
  footer with no scroll. Success: Timers line fully clears the footer at 1000×700
  and 880×620; same rule holds for any section tail added later.
- **CAP-3 — Single-close informational footers**: pure-close dialogs offer exactly
  one close affordance. Intent: Settings and Runtime stats stop asking users to
  choose between two identical exits. Success: one `Close` button each; no
  behavior change (nothing ever committed through these footers).
- **CAP-4 — Hotkey path proven with real input**: all advertised bindings fire
  from a real keyboard event. Intent: `Ctrl+N/O/V/T/K/E/F`, `F5/Shift+F5`,
  `Ctrl+T/U`, `Ctrl+1..6`, `Delete`, `Esc` do what their chips promise. Success:
  each binding opens/acts/closes the documented surface on a live drive; any
  binding that does not is fixed, not documented around.
- **CAP-5 — Menu items in the accessibility tree**: top-bar dropdown items and the
  card context-menu items expose role + name to UI Automation. Intent:
  screen-reader users reach every action mouse users reach. Success: UIA walk
  names each item; items invoke through the accessibility channel; nothing
  activates without user action.
- **CAP-6 — Back-to-guided verdict**: the toggle is proven working or fixed.
  Intent: close the one inconclusive probe from the review. Success: a recorded
  foreground click toggles guided⇄JSON both directions with values preserved,
  or a defect with a fix lands instead.
- **CAP-7 — Bulk-bar minimum-width review**: the wrapped bulk bar stays usable at
  880px. Intent: confirm the 2-line wrap degrades gracefully, tighten only if
  cheap. Success: screenshot at 880×620 shows every bulk control reachable with
  no overlap and no horizontal scroll; wrap beyond 2 lines is fixed, not accepted.

### US-1 — Readable format hints (covers CAP-1, Priority: required)

As a first-time adder, I want the full `Server address` / `Server port` example
placeholders visible, so that I know the expected format before typing.

Independent test: open Add at 880×620, screenshot the guided row, assert no
ellipsized placeholder pixels (model grader) plus existing headless paint tests.

Acceptance scenarios:

1. **Given** the Add dialog at 880×620 dark, **When** the guided Server/Port row
   paints, **Then** each placeholder shows its complete text with no clipping.
2. **Given** the same dialog at 1000×700 light, **When** it paints, **Then** the
   same holds (no theme-specific overflow).
3. **Given** Edit with prefilled values, **When** values clear on focus-out empty,
   **Then** the same full placeholders return (single copy owns the strings).

### US-2 — Timers reachable, not necessarily painted (covers CAP-2, Priority: important)

As a settings reader, I want the Timers line reachable without confusion, so that
a half-visible line never reads as a rendering bug.

Amendment 2026-10-12 (measured): body padding cannot lift a mid-body line above
the fold (needs −86px at 1000×700, −158px at 880×620; padding extends the scroll
range downward only; everything above Timers is pinned painted). Full-paint is
relaxed to presence + reachability; Startup/Probe/save-button keep their paint
pins; the landed body-bottom-padding rule governs scroll-bottom tail clearance.

Independent test: open Settings at 1000×700 and 880×620, assert Timers present
in the completed frame and reachable by scroll; assert the scroll-bottom tail
clears the footer per the landed regression test.

Acceptance scenarios:

1. **Given** Settings freshly opened at 1000×700, **When** the frame completes,
   **Then** the Timers line is present and reachable by scroll (paint not
   required).
2. **Given** the same at 880×620, **When** the frame completes, **Then** the
   same holds.
3. **Given** the dialog scrolled to bottom, **When** the tail paints, **Then**
   it clears the footer (landed rule, font-independent).

### US-3 — One close button (covers CAP-3, Priority: important)

As a dialog user, I want a single `Close` on informational dialogs, so that I
stop wondering which exit is the safe one.

Independent test: open Settings and Runtime stats, assert exactly one footer
button each; assert pinned label tests updated (`footer_commit_label`
expectations for pure-close dialogs).

Acceptance scenarios:

1. **Given** Settings open, **When** I read the footer, **Then** exactly one
   `Close` button exists and `Esc` still closes.
2. **Given** Runtime stats open, **When** I read the footer, **Then** the same
   holds and every stats line still paints.
3. **Given** the shared footer helper, **When** a dialog has no commit, **Then**
   it renders Close-only (rule lives in one place, not per dialog).

### US-4 — Hotkeys fire for real (covers CAP-4, Priority: required)

As a keyboard user, I want every advertised shortcut to work, so that the Kbd
chips and Hotkey reference tell the truth.

Independent test: cua-driver live drive with foreground (real) input on an
isolated DB — each binding executed, postcondition screenshotted (human grader
for the drive log, code grader for end-state assertions where headless seams
exist).

Acceptance scenarios:

1. **Given** the library focused, **When** I press `Ctrl+N`, **Then** the Add
   dialog opens guided-first.
2. **Given** any surface, **When** I press `Ctrl+K`, **Then** the command
   palette opens; typing `Settings` + `Enter` opens Settings.
3. **Given** a row selected, **When** I press `F5`, `Shift+F5`, `Ctrl+T`,
   `Ctrl+E`, `Delete`, **Then** start / stop / test / edit / delete-confirm each
   fire for the selected row.
4. **Given** a dialog open, **When** I press `Esc`, **Then** it closes with
   nothing committed.

### US-5 — Menus exposed to assistive tech (covers CAP-5, Priority: important)

As a screen-reader user, I want every menu item announced with role + name, so
that no action is mouse-only.

Independent test: UIA walk snapshots with each top-bar menu open and with the
card context menu open; assert every item appears with role + accessible name
(rule grader on the walk JSON); invoke one item per menu through the
accessibility channel and assert the postcondition.

Acceptance scenarios:

1. **Given** the `PROGRAM` menu open, **When** the UIA tree is walked, **Then**
   `Add configuration`, `Import`, `From clipboard`, `From file(s)`, `Export`
   each appear with role + name.
2. **Given** the card context menu open, **When** the tree is walked, **Then**
   all 15 items appear (and no sixteenth).
3. **Given** any exposed item invoked via UIA, **When** it fires, **Then** the
   same postcondition holds as a mouse click (spot-check Edit + Test).

### US-6 — Guided toggle verdict (covers CAP-6, Priority: important)

As a reviewer, I want a recorded verdict on `Back to guided`, so that the one
inconclusive probe doesn't linger.

Independent test: foreground click log + screenshots both directions on an
isolated DB (human grader).

Acceptance scenarios:

1. **Given** Edit with JSON expanded, **When** I click `Back to guided`, **Then**
   the mini-form shows with server/port echoed and no input clobbered.
2. **Given** guided shown, **When** I click `Advanced (raw JSON)`, **Then** the
   JSON box returns with guided values merged.
3. **Given** either direction fails to reproduce, **When** the drive completes,
   **Then** the verdict is recorded in the ledger and no code changes ship for
   this story.

### US-7 — Bulk bar at minimum width (covers CAP-7, Priority: optional)

As a small-window user, I want the bulk bar reachable at 880px, so that minimum
size never means broken controls.

Independent test: screenshot at 880×620 with 4 rows checked (human grader).

Acceptance scenarios:

1. **Given** 4 rows checked at 880×620, **When** the bulk bar paints, **Then**
   every control is visible and clickable with no overlap and no horizontal
   scroll (wrap ≤ 2 lines accepted).
2. **Given** wrap would exceed 2 lines, **When** measured, **Then** spacing or
   labels tighten instead of accepting a third line.

## Implementation Decisions

- Placeholder fix is copy-first: shorten the two guided placeholders until they
  clear the half-column at 880px (production strings live only in the blank and
  prefill constructors — no test pins them). Column widening is the fallback,
  not the plan.
- Timers fix is a body-bottom-padding rule in the shared dialog body, not a
  per-line nudge; the rule covers future section tails.
- Single-close lives in the shared footer helper's no-commit branch
  (`footer_commit_label` pure-close arms already return `Close`); per-dialog
  `show_cancel(true)` on pure-close dialogs goes away. Pinned tests asserting
  both buttons update to Close-only.
- Hotkey work is verify-first: foreground cua drive before any code. Background
  PostMessage dispatch provably drops modifier state on this stack (observed
  this session), so background failure alone never counts as an app defect.
- Menu exposure is attempted through the gpui-kit/UIA bridge; synthetic nodes
  that misreport focus or activation are forbidden — if the stack cannot expose
  an item, that item falls back to guaranteed keyboard operability
  (arrows/`Enter`/`Esc` with visible focus) plus a ledger note, never a fake node.
- `Back to guided` ships code only on reproduction; a clean bill ships words.
- Bulk bar ships code only if the 880px capture exceeds 2 wrapped lines.

## Testing Decisions

- Good tests assert externally observable behavior (painted pixels, UIA JSON,
  dialog open/close, toast text), never widget internals.
- Modules under test: guided form placeholders + fold positions (existing
  `assert_painted` seams), dialog body/footer geometry (completed-frame bounds
  assertions), shared footer helper label contract, hotkey dispatch table,
  menu item inventories (existing menu-guard tests extended to UIA JSON).
- Prior art: `ticket43_chrome_paints` paint/presence pattern, menu presence
  guards (`command_strip_all_ids_present`), honest-toast assertions, hermetic
  TEMP-file stores; live drives run on isolated `MANYPROXY_DB` exactly as the
  2026-10-12 review did.
- Graders: code grader for bounds/labels/counts; rule grader (regex/schema) for
  UIA walk JSON and placeholder inventories; model grader for truncation/overlap
  pixels; human grader for drive logs, 880px bulk-bar capture, and guided-toggle
  verdict.

## Out of Scope

- Any data-plane change (parsers, diff, engine, traffic, health): paint and
  access only.
- New dialogs, new menus, new shortcuts, new themes, QR bitmaps, CPU/RAM
  sampling, per-period history, Auto-Selector — all previously deferred, still
  deferred.
- Changing settled copy beyond the two placeholders and the Close-only footer
  (toasts, confirms, notes stay verbatim).
- Registry URL-scheme registration, installer work, OS-level keyboard hooks.

## Further Notes

- Fix order by user value per effort: US-1 + US-2 first (first-run paint),
  US-3 with either, US-6 next (verdict gates nothing), US-4 + US-5 together
  (both need the live drive rig), US-7 last (may ship words only).
- The `Ctrl+U` (Copy URLs) vs Update-current (`Ctrl+R`) binding split from
  ticket 08 stays frozen; hotkey verification must not "fix" it by stealing.

## Seams

1. Completed-frame paint assertions (`assert_painted`, bounds geometry) —
   existing, highest for CAP-1/2/7.
2. Footer label contract (`footer_commit_label` pins + dialog open/close tests) —
   existing, highest for CAP-3.
3. cua-driver live drive on isolated `MANYPROXY_DB` (proven this session:
   background clicks/type, foreground scroll/hotkeys, UIA walks, screenshots) —
   the seam for CAP-4/5/6 and the 880px capture.
4. UIA walk JSON snapshots committed as fixtures — new, proposed at the menu
   item-list level for CAP-5.

Two seams do nearly everything (paint + drive); fixtures are the only addition.

## Boundaries

- **Always** — local-only (no system mutation; mutation scans stay green);
  honest copy (toasts/confirms/notes verbatim); pinned control ids unchanged;
  hermetic tests (TEMP files, isolated DB); live drives on isolated DB only.
- **Ask first** — weakening any pinned assertion (paint→presence, dropping a
  footer-button pin); changing copy beyond the two placeholders; adding a
  dependency (e.g. image/QR crates are still out).
- **Never** — synthetic accessibility nodes that lie about focus/activation;
  silent behavior changes to intake, subscriptions, engine, or tray; new
  persistent settings keys (no schema migration in this work).

## Assumptions

ASSUMPTIONS I'M MAKING:

1. Shorter placeholder copy (not wider columns) fixes CAP-1.
2. Body bottom padding (not moving Timers) fixes CAP-2.
3. Single `Close` (not `OK`) is the surviving button for CAP-3.
4. Real-input hotkeys work and CAP-4 ships verification only (not a rewrite).
5. The gpui-kit/UIA bridge can expose menu items; keyboard fallback covers the
   remainder (not fake nodes).
6. The guided-toggle miss was my coordinate error (not an app defect).
7. Bulk-bar 2-line wrap at 880px is acceptable (not a redesign).
8. No test pins the exact placeholder strings (grep shows production-only use).
→ Correct me now or I'll proceed with these.

## Verification

- No implementation detail leaks into capabilities or criteria above.
- Every criterion is measurable without implementation knowledge (pixels,
  bounds, counts, named postconditions).
- Zero `[NEEDS CLARIFICATION]` markers (assumptions documented instead).
- Self-check pass 1 coherence: US-6's may-ship-words outcome is reflected in
  Testing Decisions and Boundaries (no schema migration); US-4's verify-first
  rule matches Implementation Decisions; CAP-7's accept-wrap matches
  Assumption 7.
- Self-check pass 2 preservation: all 7 review findings land — truncation→US-1,
  Timers clip→US-2, Cancel+Close→US-3, hotkeys→US-4, UIA menus→US-5,
  guided-toggle→US-6, bulk wrap→US-7. Throne-adoption scope, local-only
  constraints, and the Ctrl+U/Ctrl+R freeze carry over in Further Notes and
  Boundaries.
- Capability evals: pass@3 ≥ 0.90 on the drive-based stories (US-4/5/6 rerun
  the rig 3×). Regression evals: pass³ = 1.00 for `ticket43_chrome_paints`
  and the menu-guard tests on every slice.

## Sources

- Live cua-driver review, 2026-10-12 (this conversation: 15+ screenshots,
  empty state → import → cards → context menu → Edit/Add → Settings scroll →
  Runtime stats → Logs/Connections/Traffic → failed global Start).
- `.planning/spec-throne-adoption.md` + `.scratch/throne-adoption/issues/01–10`
  + `.scratch/throne-adoption/ledger.md` (settled scope, pinned ids, frozen
  bindings, deferred items).
- `AGENTS.md` + `.planning/spec.md` (local-only constitution, CAP-1..7
  baseline, non-goals).
- Code facts grounded pre-write: guided placeholder strings (`main.rs:2379–
  2380, 2636–2653`, production-only); shared footer helper
  (`footer_commit_label`, `main.rs:1736–1745`; Settings/Runtime footers
  `main.rs:15471–15474, 15686–15695`; pins `main.rs:39013–39015, 39060`).

## Maintain

| Role | Owner (reuse, link in place) |
| ---- | ---------------------------- |
| Constitution | `AGENTS.md` (local-only, hermeticity, secrets) — this spec adds nothing. |
| Map | `.planning/spec.md` + `.planning/spec-throne-adoption.md` — this spec links both; dialog/footer/menu ownership stays described there. |
| Status | This spec's Success signals per CAP (measurable thresholds above); the CUA findings table in the review message is superseded by the CAPs. |
| History | `.scratch/throne-adoption/ledger.md` — append per-story verdict lines (esp. US-6's verdict and US-4's drive log pointer); no new ledger file. |

One canonical owner per fact: placeholder strings live in code, footer rules in
the helper's docs, menu inventories in the menu-guard tests, drive evidence in
the ledger. This spec cites them, never copies them.
