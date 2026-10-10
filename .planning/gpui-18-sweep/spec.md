# Spec: ManyProxy gpui-kit 18-idea sweep (composition-first UI)

Status: spec (planning only — no implementation authorized by this document).
Scope source: 2026-10-09 UI audit (2 explorer passes) + gpui-kit 0.6.6 API verification against registry source. User direction: Full 18-idea sweep.

## Kernel

### 1. Why

From the user's perspective: ManyProxy's signal-box UI works but hand-rolls what gpui-kit already owns — status lamp, check chips, form layouts, radio groups, copy buttons, empty states, log lists, secondary dialogs. Result: inconsistent density (`.small()` in some places, full-size in others), silent truncation, bare Usage/Preset sections next to boxed Endpoints/Health/History, an 8-control operate row duplicating the toolbar, and a Logs viewer that can't scale. The user asked for every idea taken through spec → tickets → implement → review → merge, kit-wise, without breaking the locked visual world (flat, hairlines, one 6px radius, two text sizes, signal-only color).

### 2. Capabilities

Stable IDs, never reused. Each is user-observable and testable without implementation knowledge.

**CAP-1 — Status grammar on kit primitives.**
Intent: serving state reads the same everywhere and announces itself.
1. Row lamp uses kit status primitive (`Marker` or equivalent) with accessible name + tooltip (`Serving` / `Stopped` / `Failed`), flat fill, 12px minimum visual target.
2. Checked-set numeral uses kit chip (`Badge`/`Tag`) with position text, distinct from selection wash when a row is both selected and checked.
3. Lamp, row badge line, and detail `Badge.dot` agree (green serving, red failed, gray stopped); state perceivable without color alone (text/icon, not hue only).

**CAP-2 — Detail sections share one container grammar.**
Intent: inspector scans the same on every tab.
1. Every inspector module (Endpoints, Usage, Health, History, Preset) renders in the same container (`GroupBox`, label/value rows via `DescriptionList` where they read better) with consistent gap rhythm.
2. Endpoint values truncate with full-text tooltip; each copy control copies bind+port only and confirms with an honest toast; upstream stays display-only.
3. Preset pane names rule count and offers inline detach; usage line is not a naked gray string under a boxed neighbor.

**CAP-3 — Forms on `Form`/`Field`, exclusive choices grouped.**
Intent: dialogs read as one form language; experts keep the fast path.
1. Add/Edit/Import/Subscriptions/Groups bodies use kit form layout (`v_form`/`h_form` + `field()` + footer), label-over-field, 4px field stacks, shared commit footer with honest verbs (never bare OK).
2. Add stays guided-first (Server / Server-port / Identity with mask+reveal, live `vless needs uuid`-style hint); raw JSON is a disclosed `Collapsible` Advanced section, never the default; guided↔JSON swap never stacks.
3. Interval picker (always/1h/6h/12h/24h/7d) and Logs level picker (All/Debug/Info/Warn/Error) are each one grouped exclusive choice (`RadioGroup` with `selected_index` + `on_change`), no bracketed/prefixed text conventions, no duplicate cycle button.

**CAP-4 — Density, truncation, and title discipline.**
Intent: controls fit the minimum frame; nothing truncates silently.
1. `.small()` is consistent across toolbar, facet, bulk, detail operate row, endpoint copies, search/bulk clears, Advanced toggle, log actions, and empty-state buttons; footers stay `.small()`.
2. Every truncating surface (row name/meta, endpoint values, status segments, preset/usage lines, log notes) carries a full-text tooltip; shortcut hints ride `Kbd` chips only where a binding exists, with full `Name (Key)` on accessible names.
3. Title bar content uses kit text (`Label` or equivalent), not a raw string child.

**CAP-5 — Empty states teach inside `Empty`.**
Intent: empty never strands the user.
1. Library empty, filtered-no-matches, detail no-selection, and log-no-lines each render kit `Empty` with title + description + working actions in content (Add/Import/Clear-search/Clear-facets/Apply-filter equivalents).
2. Truly-empty CTAs are not stranded in a separate `div` below the list; screen-reader order hears empty then its actions together.
3. Counts stay honest (0-totals read as empty, never a lying zero; `N of M`, `N checked (of M visible)`).

**CAP-6 — Operate row + bulk bar regrouped, no duplication drift.**
Intent: one command reads once; crowded sets wrap, never sideways-scroll.
1. Detail operate row groups primary commands (`ButtonGroup` for Start/Stop/Test or equivalent) with management actions (Edit/Delete/Duplicate/resets) behind exactly one disclosed `Row actions…` menu (`DropdownButton` or `DropdownMenu`); trigger names its contents.
2. Bulk bar groups with separators (selection verbs | state verbs | destructive trailing `Delete.danger`); opposite pairs (Enable/Disable, Check-visible/Clear-checked) are unambiguous; bar wraps to ≤3 lines and never pushes layout sideways.
3. Toolbar levers and operate-row buttons dispatch through the same handlers (same toasts, same in-flight guards); labels differ only where scope differs (`Start selected` vs `Start "<name>"`).

**CAP-7 — Search, facets, and inspector tabs behave as one filter+view system.**
Intent: find fast, stay oriented.
1. Library search is one persistently visible field (`List.searchable` + `search_placeholder`) with live `N of M` count and a `Clear-search` reset affecting only the query; facet clears affect only facets; bulk clear affects only the checked set.
2. Facet pickers (`Select`) keep active choice + result count visible; tooltip uses outer wrapper (kit `Select` has no tooltip slot — verified 0.6.6 `select.rs`), not an invented builder.
3. Inspector tabs (`TabBar.segmented.small`) carry `Kbd` hints for Ctrl-1/2/3, keep stable anchors, reset scroll on switch, and keep pane order frozen (Header → Endpoints → tab-specific) so Endpoints stays above the fold; per-tab id painting is not invented (kit `Tab::ix` is `pub(crate)` — keep wrapper+marker pattern).

**CAP-8 — Logs scale and stay redacted.**
Intent: tail 10k lines without jank, copy/save without leaking secrets.
1. Log lines render in a virtualized/searchable list (`List` or `VirtualList`), filter applies without reopen jank, level is the single `RadioGroup` from CAP-3.
2. Loading/tail-reopen shows `Spinner`; no-lines shows `Empty` (not a bare string); Pause/Resume toggle keeps stable width (no row reflow).
3. Copy/Save route through secret redaction with a visible redact note; Save offers overwrite gate or picker + discloses `MANYPROXY_LOGS` override; footer verbs distinguish committing a filter change from discarding it.

**CAP-9 — Secondary flows use the right overlay.**
Intent: modals for commits, sheets for reference, palette for everything.
1. Logs / Subscriptions / Groups move from `Dialog` to `Sheet` via `WindowExt::open_sheet` (verified: no `Sheet::open` exists in 0.6.6 — use `WindowExt`), keeping dialog chrome for true commits (Add/Edit/Import/Export/confirms).
2. Overlays dismiss topmost on Escape with focus returning to trigger; menus use `DropdownMenu` + `PopupMenuItem::element` with role/name/shortcut rows (kit `PopupMenuItem` has no id field — verified — keep `div role=MenuItem` inside `element()`).
3. Command palette (`Ctrl-K`, Help entry) covers every key-bound action plus every menu-only destination through the same handlers; Library/Manage groups stay dispatched, not duplicated.

**CAP-10 — Loading, progress, and status read honestly.**
Intent: in-flight work is visible; status strip never shifts layout.
1. Start/Test/Ping show label + inline `Spinner` (`Button.loading()`), ignore double-activation until done; long work offers `Progress`/`ProgressCircle` where bounded, `Skeleton`/`ShimmerText` where streaming (never invented `Skeleton::shimmer` — compose per 0.6.6 API).
2. Status bar uses kit `StatusBar` properly: `left()`/`right()` repeatable, center via `child()`/`children()` (verified: no `.center()` method exists — use `ParentElement`), fixed `min_w` floors + `max_w` truncate + tooltips so segments never shift layout; `StatusBar` pins only what kit pins.
3. Health honesty preserved: `health: never tested — press Test` until first probe; dead rows render error `Alert` titled `Endpoint failed` with the same operate row inline; no per-row start spinners, no fabricated latency for stopped rows, no disabled-without-explanation controls (honest-toast rule stays).

### User stories

**US-1 — Skim 50 rows and start the right endpoint by keyboard (P1).**
Covers CAP-1, CAP-4, CAP-6, CAP-7. As the pool owner, I want lamp+name+route scannable and Start reachable, so my app gets its endpoint.
Acceptance:
1. Given 50 seeded rows mixed states, when scanning, then each row's name + serving state (lamp with name/tooltip + badge text) distinguishable without opening detail.
2. Given a stopped row selected at 880x620, when viewing inspector, then endpoints + Copy + Start visible without horizontal scroll, operate row ≤2 lines.
3. Given keyboard only, when completing add → start → test → copy-URLs, then every control announces `Name (Key)` and shows focus.

**US-2 — Trust forms, intervals, and empty states (P1).**
Covers CAP-3, CAP-5. As a new user, I want guided Add + grouped intervals + teaching empties, so I succeed without learning JSON.
Acceptance:
1. Given Add opened, when reading, then guided fields show first with hint, Advanced collapsed; expanding merges guided→JSON, collapsing echoes JSON→blank guided only.
2. Given Subscriptions open, when reading interval, then exactly one `RadioGroup` always/1h/6h/12h/24h/7d with current marked, no redundant summary.
3. Given empty library / no-matches / no-selection / no-log-lines, when viewed, then kit `Empty` shows description + working buttons in one traversal.

**US-3 — Operate bulk + logs without jank or leaks (P2).**
Covers CAP-6, CAP-8, CAP-10. As an operator, I want bulk verbs grouped and logs virtualized/redacted, so 10k-line tails and multi-selects stay safe.
Acceptance:
1. Given N checked rows, when reading bulk bar, then verbs grouped with separators, destructive trailing, label `N checked (of M visible)`, wraps without sideways scroll.
2. Given 10k-line tail, when filtering/level-picking, then list stays responsive, no-lines shows `Empty`, copy/save redacted with visible note.
3. Given start/test/ping in flight, when pressing again, then second press ignored until done, button shows label+spinner.

**US-4 — Sheets, palette, and status stay stable (P2).**
Covers CAP-9, CAP-10. As a daily user, I want reference panes in sheets and a stable status strip, so context never jumps.
Acceptance:
1. Given Logs/Subscriptions/Groups opened, when viewed, then they render as `Sheet` (library visible behind), Escape closes topmost returning focus to trigger.
2. Given `Ctrl-K`, when querying, then every toolbar + menu destination resolves through the same handlers as buttons/menus/keys.
3. Given values changing (counts/engine/health/usage), when watching status strip, then slots hold width (`min_w` floors, `max_w` truncate+tooltip), no layout shift.

### 3. Constraints

1. Windows 10/11 64-bit, single crate edition 2021; `gpui-kit 0.6` sole UI dep (`use gpui_kit::*`).
2. Local-only always: no system proxy/TUN/DNS/routing/elevation/services/telemetry/silent downloads (`assert_no_system_mutation`-family stays green).
3. Sidecars app-dir only, never `PATH`, never downloaded.
4. Secrets: DPAPI at rest, redacted logs/URLs/labels, clipboard bind:port only; export keeps verbatim plaintext warning.
5. Visual world locked by DESIGN.md: flat, hairlines, one 6px radius, two text sizes, signal-only color; theme tokens only, zero hardcoded colors.
6. Pinned ids + pinned strings stay green or update tests in the same ticket with ledger rationale.
7. Verify per ticket: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.
8. Never invent kit APIs (verified 0.6.6 limits: no `Select::tooltip`, no `Tab::id`, no `PopupMenuItem::id`, no `StatusBar::center()`, no `Sheet::open`, no `Skeleton::shimmer`). Wrappers + `WindowExt` + `ParentElement` are the legal paths.

### 4. Non-goals

1. No engine/health-probe/import-grammar/store-schema/tray-behavior changes — presentation + wording only.
2. No new features (no new dialogs/menus/shortcuts/settings beyond regrouping existing verbs).
3. No DESIGN.md replacement/rebrand; no new palette/typeface/shadow/glow/gradient.
4. No web/mobile/responsive work; minimum-frame containment (880x620) only.
5. No screenshot tooling changes beyond replacing `assets/screenshots/*.png` after build (if surfaces change visibly).

### 5. Success signal

1. Keyboard-only add → start → copy-endpoint → test completes <5min at 880x620 without help.
2. Zero footer-label lies on an independent 20+-dialog walk; destructive commits danger-styled.
3. Assistive-tech traversal names every toolbar/menu/dialog/tab control (+ shortcut where bound); lamp/check states announced without color alone.
4. `cargo fmt --check` + clippy `-D warnings` + `cargo test --locked` green; hardcoded-color grep stays 0; headless id/string asserts green.

## Seams

Preferred existing seams first:
1. Headless UI tests in `src/main.rs` `mod tests`: `assert_painted` id-presence over `VisualTestContext`, pinned-string asserts, `assert_no_system_mutation` scans. New coverage lands here; no new harness.
2. Text-function seams: `serving_badge`, `library_row_lines*`, `search_count_text`, `bulk_label`, status-bar text fns, confirm-description fns.
3. Live proof opt-in post-implementation: debug build + isolated `MANYPROXY_DB` demo seed + capture of affected surfaces at 1000x700 (human layout judgment).
4. New seams only if unavoidable: one `RadioGroup` interaction helper (interval/level); one virtualized-logs helper; nothing per-row start tracking (forbidden).

## Boundaries

**Always**
- Keep pinned ids/strings green or update tests same-ticket with rationale.
- Honest-toast: actions stay enabled + explain; never introduce `.disabled()` styling.
- Secrets discipline: redaction, DPAPI, bind:port-only clipboard.
- Per-ticket verify trio before claiming done.
- No invented kit APIs; cite verified constructor + builders in ledger.

**Ask first**
- Pinned-string rewording beyond footer-lie fixes (README-claimed labels authoritative).
- DESIGN.md token changes (radius/palette/type) — refinement preserves system.
- Tray close-confirm / autostart semantics touches.

**Never**
- System mutation, `PATH` sidecar lookup, plaintext secrets.
- Bare `OK` footers, disabled-without-explanation, per-row start spinners, fabricated stopped-row latency, Kbd on unbound actions.
- Screenshots with real hosts/credentials/tokens.
- Shadows/glow/gradients/glass/emoji-icons/display type.

## Assumptions

ASSUMPTIONS I'M MAKING:
1. Lever-frame toolbar (5 throws + 4 menus) is the approved structure; work refines it (ButtonGroup/DropdownButton inside the plate) rather than restoring the old 13-button strip.
2. `List` remains the library view (dense-table spike stays gone per PRODUCT.md); `VirtualList` is for logs first, library only if headless-proven without id churn.
3. Sheets apply to reference flows (Logs/Subscriptions/Groups) first; true commits stay dialogs (not the reverse).
4. Fresh screenshots reuse synthetic `*.example.com` demo rows on isolated `MANYPROXY_DB` (not live DB).
5. Minimum frame stays 1000x700 default / 880x620 minimum.
→ Correct me now or I'll proceed with these.

## Verification

- Every CAP has ≥1 US scenario with Given/When/Then observable outcomes (counts, labels, visibility, clipboard content) — no "works correctly".
- UI stories verify by headless id/string asserts (code grader) + live capture at 1000x700 (human layout grader, model grader vs CAP checklist for wording).
- No `[NEEDS CLARIFICATION]` markers remain.
- Two-pass self-check: coherence (stories cover CAP-1…CAP-10, non-goals exclude engine/store/tray consistently) + preservation (all 18 ideas landed: lamp/chip→CAP-1, GroupBox/DescriptionList/Clipboard/HoverCard→CAP-2, Form/Collapsible/RadioGroup/Settings→CAP-3, small/tooltip/TitleBar→CAP-4, Empty→CAP-5, ButtonGroup/DropdownButton/bulk→CAP-6, search/facet/tabs→CAP-7, VirtualList logs→CAP-8, Sheet/Popover/palette→CAP-9, Progress/Skeleton/StatusBar/Alert→CAP-10).
- Capability evals: pass@3 ≥ 0.90 on headless suite for touched seams. Regression: `cargo test --locked` fully green (pass^3 = 1.00) + mutation-family green.
- Anti-patterns avoided: seeded demo rows drive layout tests; keyboard + reduced-motion paths measured; in-flight asserts via state flags, never sleeps.

## Companions

- Visual: `assets/screenshots/library.png`, `add-dialog.png`, `import-dialog.png`, `subscriptions-dialog.png`.
- System of record: `PRODUCT.md`, `DESIGN.md` (signal-box world, selection grammar, honest-toast, no-disabled).
- Prior art: `.planning/spec-ui-polish.md` (CAP-1…10, US-1…5), `.planning/ui-complete-gpui/tasks.md` (T01–T11), `.planning/fix-max-polish-tickets.md` (lanes A/B).
- API ground truth: gpui-kit 0.6.6 / gpui-component 0.6.6 / gpui-base 0.6.6 registry source (facade `gpui-kit-0.6.6/src/lib.rs`; components `gpui-component-0.6.6/src/*.rs`; resizable/virtual-list `gpui-base-0.6.6/src/...`).

## Sources

1. User 2026-10-09: "what can we improve? … specially ui wise. gpui-kit wise" → 18 ideas; "spec ticket implement review and merge" + "Full 18-idea sweep" scope vote → this spec.
2. Explorer audits 2026-10-09 (main.rs ~16k lines + DESIGN/PRODUCT/screenshots): layout map, component-usage table, 15 concrete gaps (lamp/chip divs, menu-item divs, Label starvation, small() drift, status 3-region, header wrap, Usage/Preset bare, operate duplication, search split, Select tooltip wrappers, bulk 8 verbs, dual footers, logs stack, tab markers, empty/truncation/title gaps).
3. Kit-usage audit: used-kit list (Alert/Badge/Button/LineChart/Checkbox/Command/Dialog/Empty/GroupBox/Input/Kbd/Label/List/Menu/Notification/Radio/Resizable/Select/Separator/Spinner/StatusBar/Switch/Tab/Tag/Tooltip/Theme/TitleBar) + unused (Sidebar/DataTable/VirtualList/HoverCard/Popover/Progress/Skeleton/DescriptionList/Message/Slider/pickers/Avatar/Breadcrumb/Tree/Sheet-open) + zero hardcoded colors + domain-keyed ids + focus/overlay findings.
4. API verification 2026-10-09 vs registry 0.6.6: Form/RadioGroup/Clipboard/DescriptionList exist; Sheet::open does not (use WindowExt::open_sheet); StatusBar center via children(); Select has no tooltip slot; Tab::ix pub(crate); PopupMenuItem has no id — all with file/line citations.
5. Repo authorities: `AGENTS.md` (commands/constraints/hermetic tests), `PRODUCT.md`, `DESIGN.md` Do's/Don'ts, `.planning/*` ledgers.

## Maintain

| Role | Owner (reuse in place, link don't copy) |
| ---- | --------------------------------------- |
| Constitution | `AGENTS.md` + DESIGN.md Do's/Don'ts. This spec adds nothing to governance. |
| Map | `src/main.rs` (monolith UI), `src/store.rs`, `src/engine.rs`, `src/health.rs`, `src/tray.rs`, `assets/screenshots/*.png`, PRODUCT.md/DESIGN.md. |
| Status | Suite green per git log (`c1cd288` + `a5145d3`); screenshots lever-frame baseline 2026-10-08; no new status until tickets land. Delete-zone: old 13-button screenshots (deleted, do not recreate). |
| History | `.planning/ui-complete-gpui/` ledger (T01–T11), fix-max-polish lanes A/B, 2026-10-07/08 retakes. |

Next (not executed here): `/plan` → `/to-tickets` → `/decide` → `/implement`.
