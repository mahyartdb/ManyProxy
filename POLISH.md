# ManyProxy polish pass — working file

> Living document. This is the single source of truth for the current UI polish pass.
> A later session or another agent should be able to resume from this file alone.
> Update the **Status** and **Work log** sections after every meaningful step.

- Started: 2026-10-23
- Owner: polish pass driven from an OpenCode session
- Design authority: `DESIGN.md` (The Signal Box), `PRODUCT.md`, surface brief `.impeccable/surfaces/src-main-rs.md`
- Mode: **Operate** (tool UI; precision and restraint outrank expression)
- Scope: whole running app surface — library, inspector, toolbar, facets, bulk bar, status bar, dialogs and settings. Refinement, not redesign: preserve the incumbent world, copy, behavior, ids, and tests.

## Status

| Phase | State |
|---|---|
| Evidence capture | done — dark 880/1000/1400 + Add 880/1000 + all six panels at 880/1400 |
| Findings catalogue | done — root causes confirmed in source |
| Fix batch 1 — inspector overflow / tabs / endpoint rows / operate row | done |
| Fix batch 2 — library rows / check grammar / copy label / headers | done |
| Fix batch 3 — toolbar / facets / bulk bar / status bar / dialogs / empty state | done |
| 880x620 + dialog verification | done (dark). Light theme is token-based but was not re-captured this round. |
| `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | done — 1290 passed, 0 failed (2026-10-23) |
| DESIGN.md update + final report | done in this commit |

## Hard constraints (do not break)

From `AGENTS.md` and `DESIGN.md`:

- Local-only, zero system mutation. No system proxy, TUN, DNS rewrite, routing editor, elevation, services, telemetry, silent downloads.
- Every visible string/id listed in `DESIGN.md` is load-bearing. Restyle freely; do not rename pinned ids, drop states, or weaken honest-toast copy.
- No hardcoded colors in app code. Resolve through `cx.theme()` / kit tokens.
- Keys never ride inside labels or accessible names. Kbd chips only on menu rows, tab suffixes, and the operate row.
- Selection = wash + rim on **one** row; checked set = 2px accent rim + numeral chip; bulk bar reads `N checked (of M visible)`.
- Endpoints stay above the operate row. Failure evidence inside the inspector's minimum height.
- No third text size, no second radius, no shadows, no glow, no gradients, no emoji.
- Tests are hermetic. Do not touch the running app's real `library.db`; use the isolated setup below. Do not copy user secrets into the repo or into screenshots that get committed.

## How to run the app for this pass

Current (user's) instance:

```pwsh
# running release build, do not kill unless asked
C:\Users\qmahyar\Downloads\manyproxy-v0.0.1-beta.1-windows-amd64\manyproxy.exe
# pid 17676, window "ManyProxy · 1 running", 1920x1032 at (0,0) when first inspected
```

Dev loop (safe, isolated, no user data):

```pwsh
cargo build --locked
$env:MANYPROXY_DB = 'C:\Users\qmahyar\AppData\Local\Temp\opencode\manyproxy-polish\library.db'
$env:MANYPROXY_EXPORT = 'C:\Users\qmahyar\AppData\Local\Temp\opencode\manyproxy-polish\export.json'
$env:MANYPROXY_LOGS = 'C:\Users\qmahyar\AppData\Local\Temp\opencode\manyproxy-polish\logs'
$env:MANYPROXY_RULE_SETS_DIR = 'C:\Users\qmahyar\AppData\Local\Temp\opencode\manyproxy-polish\rulesets'
.\target\debug\manyproxy.exe
```

Notes:
- Debug binary is `target\debug\manyproxy.exe` (71 MB, much slower than release but fine for screenshots).
- A second instance does **not** need admin. Loopback defaults only.
- Sidecars are not needed to add/select/edit rows; only to start. Checks must not require real egress.
- To verify at the minimum size, resize to `880x620` (or use `set_window_frame` through the cua-driver) and confirm the inspector becomes a drawer.

Verification commands (same as CI):

```pwsh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Single test: `cargo test <substring> --locked`.

## Evidence and capture protocol

- Screenshots go under `.impeccable/captures/polish-2026-10-23/`.
- Naming: `<surface>-<width>x<height>-<theme>-<state>.png`.
- The running release instance is the first evidence source. The dev instance is the fix-verification source.
- Never crop out the window edge when judging clipping: edge clipping is one of the defects under investigation.
- Capture at 100% display scale (this machine is 1920x1080 @ 1.0).

Current raw captures (outside the repo, first evidence run):

- `C:\Users\qmahyar\AppData\Local\Temp\opencode\mp-full.png` — 1918x1030 full window.
- `C:\Users\qmahyar\AppData\Local\Temp\opencode\mp-right.png` — inspector crop.
- `C:\Users\qmahyar\AppData\Local\Temp\opencode\mp-top.png` — header + first rows.
- `C:\Users\qmahyar\AppData\Local\Temp\opencode\mp-status.png` — status bar.
- `C:\Users\qmahyar\AppData\Local\Temp\opencode\mp-rowzoom.png` — 3x row leading cell.

## Findings

Severity: **P0** blocks a task or lies about state; **P1** visible defect at supported sizes; **P2** inconsistency / missed polish; **P3** cleanup.

### Inspector (right pane)

| ID | Sev | Finding | Evidence | Likely cause | Status |
|---|---|---|---|---|---|
| F-01 | P1 | Inspector content is clipped at the window's right edge at 1920x1032. Endpoint `Copy` buttons, `History` sentence, row editors (`Start`/`Edit`) run past the window. | `mp-right.png` | Content inside the 360px pane has a wider min-content than the pane; endpoint rows reserve a fixed 72px label + copy button and do not clamp/shrink enough. | open |
| F-02 | P1 | Inspector tab bar clips: at 1920 only Details/Health/Presets/Logs are visible; Connections and Traffic fall off the right edge. | `mp-right.png`, full | Six tabs with `Ctrl-1..6` chips in a 360px pane cannot fit; the tab bar does not wrap/scroll/shrink. | open |
| F-03 | P1 | Operate row clips: `Start`, `Edit` are cut off; the row editors (`bind`, `Add…`, port, Start, Edit) are clipped at the pane edge. | full, `mp-right.png` | The editor line and the eight-control operate row are over-subscribed at 360px and the children do not shrink; `flex_wrap` exists on the operate row but not on the editors row. | open |
| F-04 | P2 | `History` sentence is cut mid-word (`no health history yet — press Test to record a pro…`) with no truncation affordance. | `mp-right.png` | No `truncate` / wrap on the history line; body has `overflow_x_hidden`, so the text is simply cut. | open |
| F-05 | P2 | Detail header repeats the endpoint twice: raw `127.0.0.1:10827` next to `○ Stopped`. DESIGN says the badge string is `<state> · <bind>:<port> · <latency>`. | full | `serving_badge` drops the endpoint when stopped and the header also paints it separately; when running it is painted twice. | open |

### Round 2 findings — isolated 1000x700 drawer + 8x crops

These were observed on the isolated release-copy instance (`manyproxy-release.exe`, pid 82260, temp DB) so no user data was touched.

| ID | Sev | Finding | Evidence | Root cause (confirmed in source) | Status |
|---|---|---|---|---|---|
| F-25 | P1 | Detail pane has no definite width: `w_full` children are as wide as their content, so endpoint rows, row editors and the operate row never wrap/shrink and run past the pane. | `mp-full.png`, `mp-right.png` | `render_detail_selected` sets `h_full` but no `w_full/min_w_0`; `render_detail_endpoints`' column sets only `max_w(620)`; the detail body has no `min_w_0`. | open |
| F-26 | P1 | Endpoint values are truncated to `10.0...`, `socks...`, `http://1...` at 1000 and the Copy buttons are clipped at 1920. | `mp-full.png`, `mp-dev-1000.png` | 72px label + three ~95px "Copy SOCKS/HTTP/upstream" buttons; the value column reserves no width. | open |
| F-27 | P1 | Six inspector tabs clip at both 360 and 260: Connections/Traffic are unreachable with the pointer. | `mp-full.png`, `mp-dev-1000.png` | Six labels + six `Ctrl-N` Kbd chips do not fit 244–344px; the TabBar scrolls horizontally without a visible bar. | open |
| F-28 | P1 | Operate row clips: the Start/Stop/Test ButtonGroup is wider than the 260px drawer's 244px content box, so Test is cut and the row never reaches a clean wrap. | `mp-dev-1000.png` | Kbd chips inflate the group to ~314px; a ButtonGroup is a single flex item and cannot wrap internally. | open |
| F-29 | P1 | Checked grammar is invisible: the numeral chip and 2px rim use `theme().accent`, which is `neutral-800` in the dark kit — almost the same as the card. The `1` chip reads as a dark scratch on the blue wash. | `mp-dev-cell3.png` (8x) | `library/mod.rs` uses `cx.theme().accent` for the chip border/text and the checked card rim. | open |
| F-30 | P1 | Row `Copy proxy` truncates to `Copy...` at the row's 72px trailing cell. | `mp-full.png` | `RowMetrics::COPY = 72`; label + small-button padding needs ~88–92px. | open |
| F-31 | P1 | Active sort header (`Health ↓`) is nearly invisible, which reads as the header being clipped (`Healt`). | `crop-healthhdr.png` | `sort_header_cell` paints the active label with `cx.theme().accent` (neutral-800 dark). | open |
| F-32 | P2 | Drawer overlays the facet pickers: Group is visible, Protocol/Status are under the drawer at 1000. The right end of the bulk bar (Disable) is also covered. | `mp-dev-1000.png`, `mp-dev-checked-conv.png` | Facets sit at the far right of the facet bar because the search field is `flex_1` with no max width; the bulk bar lays out to full window width under the overlay. | open |
| F-33 | P2 | Add dialog header reads as three broken tabs: `Common / Advanced Settings / TLS Security` on one line, and the middle item is a bordered button. | Add-dialog capture, 1000x700 | `common_head` puts `advanced_settings_button` beside the section label; TLS Security is the right column's first line, at the same baseline. | open |
| F-34 | P2 | Add dialog placeholders are clipped: `Server (e.g. (` , `Port (e.g. 44` , `Port (blank or auto = auto-as`. | Add-dialog capture | Placeholders duplicate the field label inside ~200px half-columns. | open |
| F-35 | P2 | Add dialog footer's Add button is not primary; Cancel and Add look identical. | Add-dialog capture | `dialog_footer` (and the import/edit custom footers) never set `.primary()` on the commit button. | open |
| F-36 | P2 | Empty library: Add and Import are both secondary, and the empty block sits in the top third with an orphaned `here.` line. | Empty capture, 1000x700 | `empty-add` has no `.primary()`; the Empty block has no constrained/centred width. | open |
| F-37 | P2 | Toolbar renders `Stop all` as a second white filled button beside `+ Add configuration`. | `mp-full.png` | `global-start-stop` uses the kit's default filled variant, not an explicit secondary/ghost. | open |
| F-38 | P2 | Status bar says `engine running · 1 endpoint(s)`. | `mp-full.png` | `Engine::status_text` uses a literal `endpoint(s)`. | open |
| F-39 | P3 | Checked row's chip uses a 20px slot; three-digit positions need ~31px, so the numeral overflows the reserved slot. | `mp-rowzoom.png` | `RowMetrics::CHECK_SLOT = 20`. | open |
| F-40 | P3 | Drawer opens with the "Select a configuration…" empty inspector after the last row is deleted, and a newly added row is not selected. | dev 1000x700 | Behavior: `inspector_drawer_open` is independent of selection; `commit_add_*` reloads but never selects. Out of the visual-only batch — decide before changing. | hold |

### Library list

| ID | Sev | Finding | Evidence | Likely cause | Status |
|---|---|---|---|---|---|
| F-10 | P1 | Row check chip is illegible: only a vertical sliver of `24`, `50`, `41` is visible between the checkbox and the name. | `mp-rowzoom.png` | `check_slot` is 20px, the chip adds padding + border and the slot clips it; with 3-digit positions (171 checked) it is worse. | open |
| F-11 | P1 | Checked rows show no visible 2px accent rim in the screenshot even though all 171 rows are checked (bulk bar + UIA labels). | full | Suspect theme accent equals a low-contrast token on the card, or the checked border is being overridden; verify in dev build before changing. | open |
| F-12 | P1 | Trailing `Copy proxy` button truncates its own label to `Copy...` at 72px. | `mp-top.png` | `RowMetrics::COPY = 72` is narrower than label + small-button padding. | open |
| F-13 | P1 | `Health` column header is clipped (`Healt`). | `mp-top.png` | Header strip reserves the same 110px column but the label is cut; likely header row lacks right padding or the last column ends under the inspector edge. | open |
| F-14 | P2 | At 1920 the name column absorbs ~850px of empty space, leaving name and numeric columns as two distant clusters. | full | `name` is `flex_1` with fixed every other column. | open |
| F-15 | P2 | The selection rim and checked rim both use the accent, but the selected row's rim is visually louder/blue while checked rows read gray. The grammar is not legible at a glance. | full | Needs verification after F-11. | open |

### Toolbar, facets, bulk bar, status bar

| ID | Sev | Finding | Evidence | Likely cause | Status |
|---|---|---|---|---|---|
| F-20 | P2 | Two white primary buttons compete in the toolbar: `Add configuration` and `Stop all`. `Stop all` is the master throw and is destructive when running; it reads as a second primary. | full | `Add` uses primary; master throw likely uses default/primary style. | open |
| F-21 | P2 | Status bar says `1 endpoint(s)` instead of a real plural. | `mp-status.png` | Naive `format!("{} endpoint(s)")`. | open |
| F-22 | P2 | Status bar connection segment reads `0 active · 0 TCP · 0 UDP` with no noun, so `0 active` is ambiguous. | `mp-status.png` | Segment copy omits the object (`connections`). | open |
| F-23 | P2 | Facet row spends most of its width on one search field (≈770px at 1920) and pushes facets to the far right; the count sits detached in the middle. | full | Search field is `flex_1`; no max width. | open |
| F-24 | P3 | Toolbar and status bar labels are 12px system text with low letter-spacing; at 1920 the top strip feels thin compared with the 30px rows. | full | Kit small button density. | open |

### To verify (not yet confirmed)

- Empty library: CTAs, no-selection inspector centering, honest zero totals.
- First-run/empty hero.
- Settings dialog: section rail, field rhythm, 820px width.
- Import dialog: preview valid/invalid counts, long URLs.
- Add/Edit dialog: guided vs Advanced, validation errors, port budget.
- Light theme parity.
- 880x620 drawer mode: tab bar, operate row, endpoint rows.
- Focus rings and keyboard-only path (Tab order, Esc one layer, Ctrl-K).
- Toast stacking and long error strings.
- Loading/busy states (`Starting…` + spinner) and probe failures.

## Final resolution (this pass)

The status cells above are the initial triage. Shipped state:

- **Fixed:** F-01–F-05, F-10–F-13, F-15, F-20–F-23, F-25–F-31, F-33–F-39.
- **Mitigated:** F-32 — the facet band reserves the open drawer's width and wraps, so the count, Clear search, and Clear facets stay clickable; the drawer still overlays list rows on the right, which is accepted for an overlay.
- **Held:** F-14 (name column absorbs slack at 1920), F-24 (thin top/bottom chrome density), F-40 (drawer opens with no selection; Add does not auto-select).
- **Final layout details:** Add and Edit dialogs are 560px; the guided hint row truncates/wraps instead of pushing the Advanced buttons outside; empty state uses a short title with the Add/Import description; row Copy uses compact padding inside the pinned 72px cell; the operate hairline is a sibling wrapper so `detail-operate` stays 120px; the facet band reserves the drawer's width while it is open.
- **Verified dark:** 880x620, 1000x700, 1400x900; all six inspector panels at 880 and 1400; Add dialog at 880 and 1000; Endpoints values now read whole as `host:port` under their SOCKS/HTTP labels while Copy still writes the full scheme.
- **Not re-captured:** the final test-driven changes (facet wrap under the open drawer, Edit at 560, compact Copy) are pinned by geometry/click tests but were not re-captured; light theme at 880 was not re-captured. All colors still resolve through theme tokens; no hardcoded colors were added.

## Work log

- 2026-10-23: started pass. Loaded `impeccable context`, `DESIGN.md`, surface brief. Inspected running release build (`manyproxy.exe`, pid 17676) at 1920x1032 dark, a 171-row imported library with all rows checked. Captured full window, inspector, header, status bar and 3x leading-cell crops. Catalogued F-01…F-05, F-10…F-15, F-20…F-24. Started isolated debug build.
- 2026-10-23 (round 2): debug binary stack-overflows on the Add dialog (`thread 'main' has overflowed its stack`), so all visual verification moved to a release-copy instance (`%TEMP%\manyproxy-polish\manyproxy-release.exe`) with an isolated DB. Confirmed F-25…F-39 at 1000x700 and with 8x crops. Key insight: the inspector's missing `w_full` is the single cause of most clipping; the check chip/rim and active sort header all use `accent` (neutral-800 in the dark kit) instead of a real signal token.
- 2026-10-23 (round 2): the `cua-driver` MCP server disappeared mid-session. Evidence now comes from `System.Drawing` window captures plus Win32 `SetWindowPos`/UIAutomation through PowerShell. The old captures remain in `%TEMP%\opencode`.
- 2026-10-23 (final): moved capture to `PrintWindow(..., 2)` so occlusion, foreground changes, and hover tooltips cannot corrupt evidence. Finished the inspector: two-row six-tab bar; endpoint rows display `host:port` under SOCKS/HTTP (Copy keeps the full URL); endpoint gaps compacted so Usage stays above the fold at 880; detail body shrink-wraps when tall and shrinks/scrolls at 620; row editors sit directly under Usage; a spacer pins the operate row to the bottom; the body has a hover scrollbar; Connections' action row wraps instead of clipping; Add is 560px wide with short placeholders and the commit button primary. Verified all six panels at 880/1400 and the Add dialog at 880/1000.
- 2026-10-23 (CI repair): the first full suite found nine failures, all fixed without weakening pinned assertions: restore the Add/Import empty copy contract and split the short title from the description; keep Copy in its 72px cell with compact padding; let the guided hint shrink/wrap so Advanced settings stays inside the dialog; widen Edit to 560px; move the operate hairline to a wrapper so the row stays 120px; reserve the open drawer's width in the facet band so Clear search is clickable; remove the stale tray test registry key. `cargo fmt --check` and clippy are clean. A rerun hit an external listener on 10801 (31 engine tests); the engine group then passed 184/184 and the full `cargo test --locked` finished 1290 passed, 0 failed. Commit is the last action.

## Handoff notes

- The running instance's database contains real user configs. **Do not delete, export, or alter it.** All interactive testing uses the isolated `%TEMP%` DB above.
- The initial release build is current with `HEAD 83fcb0c` (built 06:11 after the 06:10 commit).
- Screenshots in `%TEMP%` survive compaction; copy any keeper into `.impeccable/captures/polish-2026-10-23/` before final report.
- Fix order: P1 clipping/legibility first, then grammar/copy, then density polish.
- After each batch: `cargo fmt` then a targeted `cargo test`, then a fresh screenshot at the affected widths.
