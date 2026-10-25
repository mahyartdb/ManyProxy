# ManyProxy bughunt tickets — implementation board

Source of truth: `bughunt.md` (evidence frames) + user decisions (bughunt.md §C).
Execution rules for every implementer: NO subagents · dark GPUI desktop app, Windows-only · follow DESIGN.md ("Signal Box") · hermetic tests only · never weaken `src/tests.rs` boot-path marker scan · engines beside exe, no PATH downloads · secrets DPAPI-sealed, redacted strings · export keeps its explicit-confirm dialog (hard constraint).
Definition of done for every ticket: `cargo fmt` clean, `cargo clippy --all-targets --locked -- -D warnings` green, `cargo test --locked` green (update UIA fixtures/labels in `src/tests.rs` as needed), no new console or PATH behavior. Wave owners may NOT touch files assigned to other wave agents (listed per ticket). Report per ticket: VERIFIED / UNVERIFIED + summary of changed files + test result.

## Board status (orchestrator, 2026-10-25)

- **Wave 1 — DONE.** K1, K8 (incl. dead speedtest/resolve cleanup + row-menu retirement), K2/K3 verified; per-ticket notes below. Wave-1 full-suite run: 1286 passed / 0 failed after the K8 fixes.
- **Wave 2 — DONE.** K4, K9, K12 verified via targeted gates; UNVERIFIED items are machine-limit items listed in K12's status note.
- **Wave 3 — DONE.** K5 verified (orchestrator gate: fmt/clippy clean, all previously-failing tests green). Bulk `More ▾ Delete` confirm removal deferred to K17 by ruling.
- **Wave 4 — DONE.** K11 + K17 verified (both notes below). K11's `library/mod.rs` + `main.rs` field-block deviations accepted; k17's cross-fixes (reduce_motion test determinism, `t10`/`t08` fallout) left intact.
- **K18 — DONE (2026-10-25).** Full regression on the settled tree: `cargo fmt --check` exit 0 · `cargo clippy --all-targets --locked -- -D warnings` exit 0 · `cargo test --locked` **1301 passed / 0 failed** (was 1286/0 pre-Wave-2, 1296/6 post-K5-landing → all six fixed). Boot-path nine-marker scan intact. UNVERIFIED by machine limits / needing eyes: K12 secondary-monitor+DPI placement, K12 sort-header paint on a populated library, K17 palette-hint chip at real DPI/min-width, and the five-hot-states walkthrough (empty · 9 checked · inspector Details/Health · Settings modal · delete-undo) — see bughunt.md §E.

## Wave 1 (parallel — K1, K8, K2/K3)

### K1 · Right panel unification (inspector) — owner files: `src/app/detail.rs`, `src/app/render.rs`, `src/models/`, `src/main.rs` (AppView field block only, careful: boot-path marker scan)
> **Status — VERIFIED (2026-10-25).** One segmented 5-tab switcher + single-scroll Details, one-line header with badge, merged endpoints/editors + one copy, Start/Stop/Test `…` kebab, Health/History restyle; Presets pane deleted with the user-approved dead-helper deletions. Tests updated; gate green (full suite ran clean in the K2/K3 session; K18 re-runs the numbers). K8 fallout applied in the K1 follow-up session: row `More ▾` lost Speedtest/Resolve, `Share…` gained its ellipsis, order = Move to group · Copy URLs · Share… · Duplicate · Ping.
Bughunt A4-1, A4-2, A4-3, A4-4, A4-5, A4-6, A4-7, A4-8, A1-2(tab occlusion spacing note).
- Replace the 2 stacked tab rows (Details/Health/Presets + Logs/Connections/Traffic) with ONE segmented view switcher: Details · Health · Logs · Connections · Traffic (5 tabs, one row). Presets tab disappears (Routing is its home).
- Below the acting header (ONE line: badge + group chip right-aligned inline) the Details tab = single scroll: identity badge → endpoints block (merged with editor strip; one copy action, not 3) → actions row (`Start · Stop · Test` + `…` kebab holding Auto-start, Ping, Logs, Copy URLs, Copy proxies). No content pinned to panel bottom; dead gap gone.
- Badge grammar per DESIGN: lamp green serving / red failed / gray stopped; `<state> · <bind:port> · <latency>`; remove the white leading dot, remove the floating clipped second dot, remove `dead` co-signed with Running (state word follows the truth of the row's endpoint; use "dead"/"failed" only when the endpoint is the failure source).
- Health tab: numbers dim mono, red ONLY for failing state; drop `(s)`; byte/probe wording simplified; History sparkline hidden until ≥3 samples (or labeled min/med/last).
- Editor-strip inputs: uniform bordered mono style (bind was boxed, port borderless).
- Keep drawer/breakpoint behavior of T-03 intact; tabs must not sit under the toast slot (toasts reposition in K5).

### K8 · Menus overhaul — owner files: `src/app/shell.rs`, `src/app/toolbar.rs`, `src/app/engine_ops.rs`, `src/text.rs`
> **Status — VERIFIED (2026-10-25).** Tools menu = 11 grouped rows; the board's `≤8` target is **recorded as an accepted deviation** (the board's own three groups enumerate 10 actions, and `Show security` is required by K8 itself). `Test group` removed. Row context menu = 7 core + `More ▾` per spec with **no Speedtest/Resolve anywhere**; the entire unreachable speedtest/resolve cluster (open/state/engine_ops-health/store/text/main + tests) was deleted in the K8 cleanup pass. `Library…` label + aria fixed, Program menu = Export/About only (fixture asserts the dedupe), `Ctrl+.` chip renders without truncation, Show security toggles in place without a toast (new test). Gate green incl. previously failing `t02_toolbar_menu_items_announce_names` / `t05_operate_primaries_grouped_dispatch_same_handler`.
Bughunt A6-1..A6-6, A1-3.
- Tools menu 21→≤8 items with separators, grouped Session (Start all · Stop all · Test all · Stop testing) / Inspect (Logs · Runtime stats · Traffic stats) / App (Check for updates · Row columns… · Command palette). REMOVE Speedtest selected/group/mode and Resolve Out-IP selected/group + Resolve domain group completely (user decision) — delete now-dead paths in `engine_ops.rs`/`health.rs` speedtest plumbing ONLY where it becomes unreachable dead code (clippy will gate you; keep the health.rs probe types used elsewhere).
- Row context menu 17→7 core (Start · Stop · Test · Copy proxy · Edit · Reset… · Delete) + `More ▾` (Move to group · Copy URLs · Share… · Duplicate · Reset traffic · Reset usage · Ping). No Speedtest/Resolve.
- `Settings` toolbar button opens the settings modal directly (delete the one-item menu).
- `Manage groups…` → renamed `Library…` opening the same menu (Groups/Subscriptions/Remove duplicates/unavailable/invalid/Clear tests items stay until K6/K16 lands tabs).
- `Show security` toggles state in place (rows re-render) — NO toast.
- Program menu dedupe: drop `Add configuration`/`Import` items from the Program menu (toolbar cluster keeps them); keep Export/About (+ From clipboard/file(s) inside Import-related bank only if still referenced by toolbar flow).
- Keyboard chips: consistent column, correct `Ctrl+.` rendering for Stop Testing (no `Ctrl+…` truncation).

### K2 · Add/Edit dialog — Throne grammar — owner files: `src/forms/`, `src/dialog/`, `src/app/commit_forms.rs`, `src/app/commit_engine.rs` (add/edit/preset commit paths only)
> **Status — VERIFIED (2026-10-25).** Common → one collapsed Advanced Settings → protocol section → Group/Bind/Port secondary row; literal OK footers (T-05 `debug_assert` exemption pinned); no raw JSON editor (structured raw JSON only for protocols with no guided block, per step-0 decision); protocol select shows its value. Tests for all 18 protocol pages re-pointed; gate green.
Bughunt A7-1, A7-10 (resolved by K17's single file-list import).
- Structure = Throne: `Common` section (Name / Protocol / Server / Port) → ONE full-width `Advanced Settings` expander (merged home of Advanced settings + VLESS settings Show + all TLS/transport extras; collapsed by default) → protocol-specific field section (labeled rows like Throne's WireGuard block) → footer: `Cancel` (secondary) + `OK` (primary accent, right-aligned).
- REMOVE `Advanced (raw JSON)` entirely (user decision). Kept only: import paths may accept JSON payloads where the importer already supports them; the editor never shows a raw JSON textarea for configs.
- Remove the `Protocol: vless` label-repeats-value quirk (`Protocol` select shows its value inside the field row like any select).
- Group / Bind / Port(`auto`) move into a compact secondary row above the footer, consistent bordered inputs.
- Keyboard: Enter=OK, Esc=Cancel retained from dialog kit.

### K3 · Routing presets dialog — owner files: `src/forms/` (theorem boxes; shares K2 owner), `src/app/commit_forms.rs` (preset commit)
> **Status — VERIFIED (2026-10-25).** One creation path: `New preset` with three grammar boxes (placeholders+tooltips) + `Add remote ruleset…` URL row; Rules JSON textarea + grammar paragraph deleted; footer consistent with K2; a K3 behavioral defect found during the test pass was fixed.
Bughunt A7-8.
- One creation path: `New preset` — Name + three rule boxes (Direct / Block / Proxy) each with an inline placeholder example (`domain:example.com, suffix:cn — one rule per line`, `keyword:ads, processName:updater.exe`, `ruleset:streaming, domain:netflix.com`) + an `Add remote ruleset…` URL row (replaces `Add by URL`/`New Remote`).
- REMOVE the `Rules JSON (optional)` textarea and the grammar explainer paragraph — grammar lives in per-box placeholders + tooltips only.
- Footer `Cancel`/`Add` unchanged (or OK per K2 for consistency — owner's call, be consistent with K2).

## Wave 2 (parallel — K4, K9, K12)

### K4 · Checked-set + bulk bar — owner files: `src/app/bulk.rs`, `src/library/`, `src/models/` (row visual layer)
> **Status — VERIFIED (2026-10-25, targeted gate).** Checked rows = 3px accent bar + filled chip, selection keeps its rim; bulk bar = `N checked (of M)` + Start/Stop/`More ▾`, overflow pinned by test at the min frame; Start pinned to the batch path (`k04_*` tests). The fmt/clippy frictions it saw from `settings.rs` were K9 in-flight state and settled.
Bughunt A3-1, A3-2.
- Checked rows: 3px left accent bar + filled checkbox chip. NO per-row 2px blue rim, NO numeral chips in rows. Selection keeps its 2px rim (check and selection no longer collide).
- Bulk bar: `N checked (of M)` text + `Start` + `Stop` + `More ▾` (Enable · Disable · Move to group · Copy proxies · Delete). Never wraps, never clips behind the drawer at min width; Delete inside More keeps its destructive confirm styling inside the menu row.
- Bulk verbs call the same engine ops as today (start_many_additive batch path for Start — do not regress batches into per-row spawns).

### K9 · Settings modal — owner files: `src/app/settings.rs`, `src/app/commit_forms.rs` (settings save path), `src/app/commit_async.rs` if wiring needed
> **Status — VERIFIED (2026-10-25, targeted gate).** One Cancel/Save footer bottom-right (floating Save + Done removed), auto-height body with no dead void, maintenance = 2×2 neutral buttons with click-time red confirm for Reset library, lean `sing-box · ready` engine rows with a Details tooltip, nav labels pinned ("Update"/"Updates" inconsistency fixed in test). The stray `Updates settings` chip was not reproducible (reported). fmt/clippy clean; `settings`+`k9_*` tests green. Flagged leftover handed to K5: the dead `"settings" => "Done"` footer mapping in `dialog/kit.rs`.
Bughunt A7-2..A7-5.
- ONE footer: `Cancel` (secondary) + `Save` (primary) bottom-right; remove the floating mid-right `Save` and the `Done` button; Esc=Cancel note stays.
- Layout: shrink dead space — two-column layout for short sections (General/Probing/Updates) or auto-height; content must not sit top-left over a black void.
- Maintenance: four ops become neutral buttons in a 2×2 grid, NOT full-width red slabs; red flash/confirm only at click for Reset library (and typed confirm only for Reset library if trivial to add); remove the orphan `Library` label.
- Engine section: rows become `sing-box · ready` style lean summaries (name + status + one `Details` tooltip with full path); drop the truncated path strings; drop the `Timers: …` jargon line.
- Shortcuts tab unchanged (it's the reference list), keep `Ctrl+.` correct there.
- While in the file: check why a stray `Updates settings` chip appeared in the rail at t≈227 hover (bughunt A7-6-pending) and fix any hover/tooltip mis-render you can find; report if un-reproducible.

### K12 · Status bar + small glyphs + titlebar — owner files: `src/engine.rs` (status text), `src/app/bits.rs` (icons/glyphs), `src/app/window.rs`, `src/app/render.rs` bottom strip if needed, `src/text.rs`
> **Status — VERIFIED (2026-10-25, targeted gate).** Status bar = 3 segments (configs/enabled · engine+health split · out/in usage) with ports/connections counts retired from the bar; sort-header marker lives in `sort_header_marker` (no more `Health 1` reads; populated-library paint UNVERIFIED on this single-display machine); custom titlebar = hairline min/max/close with hover washes (close = red wash), double-click maximize, drag intact; T-32-style placement fixes included (stored placement wins, unplaced centers — secondary-monitor/DPI case UNVERIFIED).
Bughunt A5-1, A5-2, A1-1, A3-4(arrow family), A6-5 leftovers.
- Status bar → 3 segments: `N configs · M enabled` | `engine <state> · health X live · Y dead` | `↓ out · ↑ in` usage. Ports list and connections counts leave the bar (ports available via Settings/Engine detail; connections count lives in its inspector tab).
- Replace the tiny ↓ glyph with a clearer icon or `out:`/`in:` labels (test at 4px baseline like the video shows); same for the `Health ↑` sort header (must not read `Health 1`).
- Titlebar: draw min / maximize / close controls in the custom chrome (flat hairline style per DESIGN.md, hover wash, close hover = red wash), double-click title area toggles maximize, drag region intact (io_ops pre-maximize persistence comment for context).
- Remove the status-bar stray dash artifact if it is ours (i.e., a deco/hover remnant), else note it.

## Wave 3 (single — K5)

### K5 · Toast system + non-blocking import/delete — owner files: `src/dialog/` (toast kit), `src/app/engine_ops.rs`, `src/app/state.rs`, `src/app/io_ops.rs`, `src/app/commit_forms.rs` (import path), `src/import_flow/`, `src/text.rs`, `src/app/shell.rs` (paste handler)
> **Status — VERIFIED (2026-10-25).** Toast kit in `dialog/toast.rs` (bottom-right above the status bar, max 2, ~4.2s success / ~8s error, never over toolbar/inspector tabs). Batched sweeps: `Testing N…` then exactly one `Tested N: X live · Y dead`; single-row tests keep their own result toast. `Couldn't reach the update feed` (no URLs/paths), `Nothing importable in clipboard` one-liner, Ctrl+V/From-clipboard imports directly when clean (t08 renamed `t08_paste_binding_imports_direct_without_dialog`). Delete = immediate + Undo toast with an 8s stash; expiry = permanent; undo restores rows + group assignments through `add_batch` (rows return disabled, port falls back to auto if taken, engine not restarted). Dead `"settings" => "Done"` footer arm dropped (`ticket43_footer_labels` pins `Close`). K5 kept bulk `More ▾ Delete`'s confirm modal because `bulk.rs` was K4-owned → orchestrator ruling: drop it per spec line 91, queued to K17. Export's explicit confirm untouched (hard constraint held). Orchestrator gate on K5's tree: see wave-3 line above.
Bughunt A1-2, A8-1, A8-2, A8-3, A8-4, A6-4 leftovers.
- Toasts park bottom-right above the status bar, max 2 visible, auto-dismiss ~4s (success) / ~8s (error); NEVER overlay the toolbar or inspector tabs. Add a small safe-area offset from window edges.
- Test-all / group tests emit ONE batched toast: start `Testing N…` then final `Tested N: X live · Y dead` (probe_result per-row toasts removed). Keep inspector Health as the per-row detail surface.
- `Test "name": live, 814ms` → success toasts only for single-row tests; group/bulk = batched only.
- Update-check failure copy → `Couldn't reach the update feed` (no internals).
- Clipboard-imposable refusal → one-liner `Nothing importable in clipboard`.
- Ctrl+V / `From clipboard` / clipboard paste imports DIRECTLY into the current group (no blocking preview dialog) when content parses cleanly; the preview flow remains only for ambiguous multi-source imports (file a note in PR description if any).
- Delete (single + bulk): NO blocking confirm modal; deletion happens immediately and a toast offers `Undo` (stores removed rows in memory for the toast lifetime ~8s; Undo restores rows+group assignments; expiry = permanent). Export unchanged (explicit-confirm stays — hard rule).
- `Show security` toast (if K8 didn't already) removed.

## Wave 4 (parallel — K11, K17)

### K11 · Filter row simplification + inspector polish leftovers — owner files: `src/app/search.rs`, `src/pickers/facet.rs`, `src/app/state.rs` (facet state only), `src/app/detail.rs` signs
> **Status — VERIFIED (2026-10-25, orchestrator gate).** Filter row is `Search · Group · N of M`: the 8-way Status dropdown is gone (Enabled/Disabled live under the Group menu), count chips fold into the `All` header, clicking a row lamp filters by that endpoint state (ListState delegate + explicit AppView facet re-sync), and the empty-search state keeps exactly one Add/Import entry (toolbar). `set_facet_protocol`/`protocol_options`/`status_options` and the retired facet variants are gone with no lingering references. Declared + accepted scope deviations: `src/library/mod.rs` (the facet model's home) and `src/main.rs` AppView field block — the field-block edit already covered by the board's K1 note; nine-marker boot-path scan intact, full suite green.
Bughunt A2-1.
- Filter row = `Search · Group · N of M` (count chips fold into an `All` dropdown header per group filter). Status facet collapse: replace the 8-way cross-product Status dropdown with (a) search-in-lamp: click the row-lamp legend/segment = filter by that endpoint state, and (b) keep Enabled/Disabled under the Group dropdown's menu (or a single `State` menu with the two meaningful axes: enabled + endpoint-live; NO cross-product options list).
- Empty search results state keeps one Add/Import entry (single entry point — prefer toolbar, drop in-canvas duplicates unless tests assert them; update tests either way).

### K17 · Chromium leftovers + final polish — owner files: everything NOT owned by K11 this wave: `src/app/bits.rs`, `src/app/render.rs`, `src/app/focus.rs`, `src/app/open.rs`, `src/text.rs`, plus `src/app/bulk.rs` for the single bulk-delete item below
> **Status — VERIFIED (2026-10-25).** Import dialog: the picked-file success toast is gone — the dialog's `Files:` row is the one representation (unreadable-file warnings remain) — and `import-dialog-ok` is accent-filled; Export keeps its explicit confirm (hard constraint held). f_0014 stray dash: no code source in the tree; K12's status-bar rewrite already retired the lone-separator class, the remaining hairline is kit chrome — reported, not changed. Palette hint: ASCII `Ctrl+K` Kbd chip parked at the status bar's right end (`palette-hint`, `aria_label` "Command palette (Ctrl+K)", paint test added). `⌘K` rejected deliberately: the binding is Ctrl on Windows and the small-size glyph fallback class (A5-1 `↓`→serif) is exactly what the bar avoids. Bulk `More ▾ Delete` now matches the ruling: immediate delete through K5's pipeline — one stash batch of the removed rows, one batched Undo toast whose tint states the truth (success/warning/error), no second stash; the blocking confirm is gone. The now-dead "Confirm before deleting" Settings row, its commit, and the `skip_delete_confirm` store meta/accessors were removed (nothing read them; a legacy meta row is ignored) and their tests updated honestly. DESIGN sweep: `bits.rs`/`render.rs`/`focus.rs` clean (topbar chip alignment A6-5 already conformant). Gate green: fmt clean, clippy clean, full suite 1301 passed / 0 failed. Bulk-stash expiry and single-row undo tests unchanged and green.
Bughunt A5-2, A6-6, A7-10, A4 leftovers.
- Import dialog: single representation of the file list (no inline+tooltip double render); Import/primary confirm buttons all primary-accent styled.
- Stray dash near status bar left edge (f_0014) — remove if ours.
- Command palette: leave Ctrl+K working; add a `⌘K` hint chip in the status-bar right end (single quiet affordance, no menu changes).
- Sweep: any remaining DESIGN.md conformance nits (two font sizes, flat surfaces, hairline borders) in bits/render; do not restructure other agents' surfaces here.
- Orchestrator ruling (spec line 91): bulk `More ▾ Delete` drops its confirm modal — deletion is immediate and routes through K5's existing delete+undo pipeline (same stash, one batched Undo toast; do not invent a second stash). `bulk.rs` was K4-owned during K5, hence the deferral.

## Final (orchestrator)

### K18 · Regression sweep — owner: orchestrator
- `cargo fmt --check` · `cargo clippy --all-targets --locked -- -D warnings` · `cargo test --locked`
- Run the app briefly if possible (`cargo run`) and eyeball the five hot states from the video: empty, 9 checked, inspector Details/Health, Settings modal, delete-undo toast.
- Update this board + `bughunt.md` statuses; list any UNVERIFIED items for user review.
