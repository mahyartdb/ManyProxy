# ManyProxy — video bughunt & quietness audit (281s screen recording)

Source: `C:\Users\qmahyar\Desktop\manyproxy-video-test.mp4` — 281s, 1920×1080@60fps. 281 frames extracted at 1fps; an activity map (per-second pixel diff) drove a full manual sweep of every active region; dense moments were read at full resolution plus zoom crops.
Working evidence folder: `C:\Users\qmahyar\AppData\Local\Temp\opencode\bughunt\` (`FINDINGS.md` = raw log, `frames\f_0001..f_0281.jpg`, `z*_*.png` zooms, `activity.txt`).
Frame refs below use `f_0NNN` (t ≈ NN seconds). Test build: `Desktop\Manyproxy video test\` (9 wireguard rows in `data\library.db`, group tag `3`).

Severity: **P0** architecture / brakes the promise · **P1** loud noise or contradiction · **P2** polish/legibility · **P3** minor.

Non-bugs observed and excluded: Windows Firewall OS prompt (t=125, external); 1-frame black blink (t=128, recorder/driver artifact); OS toast notifications at desktop level (not ManyProxy).

---

## A. Confirmed bugs (by window zone)

### A1 · Titlebar & toolbar

**A1-1 [P2] Custom titlebar has no min/max/close controls.** Every frame t=0–281; code check: `src/app/window.rs` sets only the title text; no control drawing anywhere (only `window.minimize_window()` API calls). A Windows app that can only be closed via tray or `Alt+F4` reads as unfinished and violates platform expectations. → Draw min/max/close in the custom chrome (flat, hairline style), double-click-to-maximize, drag region intact.

**A1-2 [P1] Toasts park on top of the toolbar's right cluster and inspector tabs.** `f_0201` toast "Test "p-belgium-brussels-p1": live, 814ms" covers **Logs/Connections/Traffic** tab strip; `f_0267` "Update check failed…" covers **Import / +Add configuration / ■Stop all**; `f_0272` "Show security: on" covers tabs again; toast-vs-tab occlusion was also seen t≈90, t≈170. → Toast slot bottom-right above the status bar, ≤2 stacked, auto-dismiss 3–4s, never over chrome.

**A1-3 [P1] Duplicate entry points for the same two actions.** Right cluster (`Import`, `+Add configuration`) duplicates Program-menu items (`Add configuration`, `Import`, `From clipboard`, `From file(s)`) — `f_0207` — plus the empty-state repeats Add+Import in-canvas. → Keep one primary path (`+Add`, `Import`) in the toolbar; Program menu keeps Export/About only.

**A1-4 [P2] Settings tool button is a menu with exactly one item ("Settings")** — `f_0218`. Ceremony, not function. → Button opens the modal directly.

### A2 · Filter row

**A2-1 [P1] Five filter controls in one row, with an 8-category Status dropdown behind a "Status: All" chip** (`Group: All · Protocol: All · Status: All · search · 9 of 9`). Status internally crosses Enabled/Disabled × Running/Stopped × Live/Dead × Untested — overlapping categories nobody can reason about. → Search + Group (the real container) + count; status filtering lives in the row-lamp column (click a lamp = filter by that state), mirroring Throne which has none of this in-pane.

### A3 · List, checked-set, bulk bar

**A3-1 [P0] Checked-set is the loudest object on screen.** At peak (`f_0088`–`f_0141`, all 9 checked) every checked row carries a 2px blue rim **and** a numbered chip `(1)…(9)`, adjacent to the selected row's own wash+rim — the list flashes blue and selection/check become indistinguishable at a glance. DESIGN.md's selection-vs-check "two languages" collide. → Checked = 3px left accent bar + filled checkbox chip (user-approved); numeral chips only inside the bulk-bar count; selection keeps the 2px rim.

**A3-2 [P0] Bulk bar wraps and clips.** 9 verbs with `[Delete]` orphaned alone on line 2 (`f_0141`); at default window width `Disable` is cut mid-word behind the inspector drawer (t≈72–93). → `N checked (of M)` · `Start` · `Stop` · `More ▾` (Enable/Disable/Move to group/Copy proxies/Delete inside — user-approved). Never wraps, never clips.

**A3-3 [P1] Row grammar contradicts itself.** Rows show a red leading lamp while the Health text says `Running · 127…` (e.g. `f_0199`, `f_0267`), while the status bar counts `health: 8 live · 1 dead` and the inspector badge says the same row is fine. Three state systems (process, endpoint live/dead, last probe) co-signal through mixed colors. DESIGN.md makes lamp/badge/detail agreement a Do-rule. → Lamp = endpoint health only (green live / red dead / gray untested); state text appears only on anomalous rows; status-bar counts and badge must derive from the same source.

**A3-4 [P2] Per-row `Copy proxy` button column** repeats 9× at full width (`f_0201` etc.) — permanent copy-noise. → Remove column; copy lives in bulk `More ▾`, row context menu, and `Ctrl+C`.

**A3-5 [P2] Endpoint column renders as two orphan fragments** `127.0.0.1 :10801` (bind host dimmed + port) under one header — reads as unknown dangling tokens (all row frames). → One composed string or dim `host` + emphasized `:port` with a single visual rhythm.

### A4 · Inspector (right panel)

**A4-1 [P0] Two stacked tab rows = two navigation systems.** Header shows `Details · Health · Presets` on row 1 and `Logs · Connections · Traffic` on row 2 (`f_0210`), above a 2-line acting header — the "details view is some trash" core. → One scrollable summary (identity badge, endpoints, actions) + one segmented view switcher for Health/Logs/Connections/Traffic; Presets live in Routing, not here (user-approved).

**A4-2 [P0] Huge dead gap.** Content ends ~45% down; the operate row is pinned at the bottom, leaving an empty black hole in every capture (`z140_full`, `z200_full`). → Single scroll; actions follow content; no pinned row.

**A4-3 [P1] Detail badge line is a three-signal pileup.** `● Running · 127.0.0.1:10801 · 2151ms` with a **white** leading dot (DESIGN says green/red/gray), plus a second dot floating and clipped at the top edge of the line (z170/z200), plus a `dead` word co-signed with `Running` (`Running · 127.0.0.1:10805 · dead`, t≈200), plus the group chip which wraps to line 2 or rides inline depending on name length ([3] at z170 vs z200). → One badge per DESIGN grammar: `<state> · <bind:port> · <latency>`, lamp colored, no extra floating dot, group chip right-aligned on the same line.

**A4-4 [P2] Endpoints block duplicates content + copy spam.** `upstream / SOCKS / HTTP` each with its own `[Copy]` (f_0201) while the editor strip below repeats upstream/bind data; labels lowercase, values truncating with `…` while space remains. → Compact endpoint table + one copy action; merge editor strip into it.

**A4-5 [P2] Red is used for neutral facts in the Health module.** `health: url 2151ms · probe: 1124B ↑127B · 1 probe(s)` lines and the History line are red (f_0201), plus `1 probe recorded — no trend yet (2151)`. Red should mean failure only (DESIGN). → Dim mono for numbers, red reserved for the failing state; drop `(s)` and byte jargon.

**A4-6 [P2] Unlabeled blue sparkline** under History with 2 data points (`f_0253`) reads as a rendering artifact. → Hide until ≥3 samples or label it (min/med/last).

**A4-7 [P2] Editor strip input styling inconsistent** — bind field boxed, port field borderless wedged between `Add…` and value (z140_editor). → Uniform bordered mono inputs.

**A4-8 [P1] Inspector hosts 8 persistent controls + an overflow button.** `Auto-start | Start F5 | Stop Shift+F5` / `Test Ctrl+T | Ping | Logs | Copy URLs` / `Row actions…` — 9 buttons in 3 tiers where 1 tier + kebab would do, and "Row actions…" duplicates the tier above it. → One action row: `Start · Stop · Test` + kebab `…` (Auto-start, Ping, Copy, Logs, RST traffic/usage).

### A5 · Status bar

**A5-1 [P1] Six segments, two dead ones, plus broken glyphs.** `9 configurations · 9 enabled` (redundant pair), `engine running · 9 endpoints`, `health: 5 live · 4 dead`, `ports 10801 · 10802 · 10803 · +6` (a list nobody uses), `0 connections · 0 TCP · 0 UDP` (double redundancy), and `↓0 B · ↑0 B` where the ↓ arrow degrades to a serif "T" at this size (z140_status; t=14 onward every frame). "Health ↑"-sort arrow in the list header degrades the same way to `Health 1` (`f_0088`). → Three segments: `configs` | `engine · health` | `↓↑ traffic`, with arrows replaced (icons or labels `out`/`in`); fix the small-glyph set (user-approved 3 segments).

**A5-2 [P3] Stray dash artifact** left of the first status-bar segment at t≈14 (f_0014) — likely a toast remnant or resize gripper; remove.

### A6 · Menus

**A6-1 [P0] Tools menu: 21 flat items, zero separators, mixed scopes** (`f_0272`, also t≈26): Start/Stop/Test ×selected · Logs · Runtime stats · Traffic stats · Start/Stop/Test all/group · **Speedtest selected/group/mode** · **Resolve Out-IP selected/group · Resolve domain group** · Stop Testing `Ctrl+.` · Check for updates · Show security · Row columns… · Command palette `Ctrl+K`. Peer apps cap ~10 with separators. → ≤8 items in groups: *Session* (Start all / Stop all / Test all / Stop testing) · *Inspect* (Logs / Runtime stats / Traffic stats) · *App* (Check for updates / Row columns… / Command palette). Speedtest×3 and Resolve×3 are cut entirely (user-approved).

**A6-2 [P1] Row context menu: 17 flat items** (t≈170: Start/Stop/Test/Ping/Speedtest/Resolve×2/Copy proxy/Copy URLs/Share/Edit/Duplicate/Delete/Reset traffic/Reset usage/Move to group). → 7 + `More ▾` (Move to group / Copy / Share… / Duplicate / Reset…).

**A6-3 [P2] `Manage groups…` menu mixes four jobs** (`f_0242`): Subscriptions · Groups · Remove duplicates · Remove unavailable · Remove invalid · Clear tests — the button name describes one of six verbs. → Rename to Library…, or fold cleanup verbs into Settings ▸ Maintenance and keep Groups/Subscriptions as panel tabs (per right-panel decision).

**A6-4 [P2] "Show security" toggles by firing a toast "Show security: on"** while the menu is open (`f_0272`) — state changes should render in place (row chrome/security marks) with no toast.

**A6-5 [P2] Keyboard-chip truncation:** Tools shows `Ctrl+…` for Stop Testing (t≈26, "Ctrl+." confirmed in Shortcuts list, `f_0239`), and kbd chips appear on some items but not others (`f_0272`). → Consistent chip column, correct `Ctrl+.` rendering.

**A6-6 [P3] Command palette exists and is undiscoverable** — only reachable as the LAST item of Tools + a Shortcuts-table row (`Ctrl+K`, `f_0272`, `f_0239`). Either surface it (⌘K hint) or accept it as easter egg.

### A7 · Dialogs & drawers

**A7-1 [P0] Add/Edit dialog has three disclosure affordances + jargon repetition** (t≈49–56): `Advanced settings` button · `Advanced (raw JSON)` button · `VLESS settings [Show]` — three expanders, one of them raw ("who uses raw? lol"), plus `Protocol: vless` label-repeats-the-value inside its own field, plus Group/Bind/Port floats on a 4th mini-row. → Throne grammar (user-approved): `Common` (Name / Protocol / Server / Port) → **one** `Advanced Settings` expander → protocol-specific field section → **OK / Cancel** footer. Raw JSON removed from this dialog; protocol sections follow Throne's WireGuard-block pattern (labeled rows, one toggle per line).

**A7-2 [P0] Settings modal: two commit verbs, both malformed.** A floating `Save` at the right-mid edge (`f_0221`) **plus** `Done` bottom-right (`f_0224`) — Save is detached from what it saves, and the pair is unlike every other dialog (Add=Cancel/Add, Subscriptions=Cancel/Add, Logs=Cancel/Apply filter). → Single footer `[Cancel] [Save]`, matching the OK/Cancel standard.

**A7-3 [P1] Settings modal wastes ⅔ of its area.** General/Probing/Updates/Engine/Maintenance/Shortcuts each hold 3–6 controls in the top-left; the rest is empty black (`f_0221`, `f_0224`, `f_0233`). → Same width, auto-height shrink, or 2-col grid; empty space shows intent gaps.

**A7-4 [P1] Maintenance paints 4 identical full-width red slabs** (`f_0224`): Clear traffic counters / Clear health snapshots / Reset running snapshot / Reset library — permanent red oversupply, labels repeated inside buttons, orphan section label `Library`. Destructive confirm = the only legitimate red per DESIGN. → Neutral buttons; red flash + typed confirm only at click (or inline "type DELETE"), ruin less of the retina per glance.

**A7-5 [P2] Engine section: truncated profile-path leaks + timer jargon** (`f_0233`): `C:\Users\qmahyar\Desktop\Manyproxy video test\sin…` per engine (cut mid-useless), plus `Timers: engine poll 5s · subscriptions 300s · reprobe 60s · traffic 5s` prose. Engines live beside the exe by design; paths are noise at rest. → `sing-box · ready`, `sin…`-lean summary + one `Details` tooltip; drop the Timers line.

**A7-6 [P1] Subscriptions drawer takes over the right side with jargon.** `f_0244`: replaces inspector arbitrarily, `Refresh all due (0)`, 6-choice interval radios (always/1h/6h/12h/24h/7d) on an empty add form, no list region; bulk-bar verb clipped behind it (`Copy proxies` → `C…`). → Subscriptions becomes a right-panel **tab** with a real list; add form trims to Label/URL/Group; interval default 24h, editable inside the sub row.

**A7-7 [P1] Logs drawer: triple filter + env-var prose + double empty-state.** `f_0277`: `Filter` + `Exclude` + `Regex` inputs stacked; prose *«Save writes to C:\Users\qmahyar\Desktop\Manyproxy video test\data\manyproxy-logs.txt (set MANYPROXY_LOGS to override the location)»*(env override documented in UI); empty state shows **both** "No log lines to show" and "No log lines recorded yet." at once; row-scoped drawer duplicates the inspector Logs tab. → Single Logs view (right-panel tab): level segmented control + one filter box; footer `[Cancel] [Apply]`; one empty-state sentence; `Save` keeps its redaction guarantee.

**A7-8 [P1] Routing presets dialog is the noise ceiling** (`f_0256`): three creation paths (`Add by URL` / `New Structured` / `New Remote`), `Rules JSON (optional)` textarea, then a **paragraph** defining the rule grammar (`domain:/suffix:/keyword:/regex:/ruleset:/processName:/processPath:…`, staged-set warnings). Prose-as-UX; raw JSON returns through the back door. → One `New preset` path: Name + Direct/Block/Proxy boxes (placeholder examples per box) + `Add remote ruleset…` URL row; rules-JSON textarea removed (consistent with raw-JSON cut); grammar hints as per-box placeholders only.

**A7-9 [P2] Dialogs and drawers drop onto the live list with no scrim** (`f_0256` Routing presets over rows; Settings modal; Subscriptions drawer clipping the bulk bar). → Scrim or offset-dimming for modals; drawers sized to not overlap list columns.

**A7-10 [P3] Import dialog renders its file list twice** (row inline + tooltip repeating the same names) and its `Import` confirm reads non-primary (earlier frames) — single representation; confirm buttons always accent-filled.

### A8 · Toasts & notification model

**A8-1 [P0] Import/delete use blocking screens; the user hates it.** Import opens a blocking preview dialog; delete asks blocking confirmation; per-row toasts line up during bulk ops. User (verbatim): *«also when i import or delete something there is a pop up that blocks the screen i hate that notification pop ups»*. → Paste/Ctrl+V imports **straight into the current group** (Throne standard, user-approved); import preview stays only for ambiguous multi-format drops, as a slim non-blocking panel; delete = non-blocking toast with **Undo** (5–8s window); export keeps its explicit-confirm dialog (hard constraint).

**A8-2 [P1] Test-all fires one toast per row** (t=141–199 — ≈9 near-identical `Test "…": live, 814ms` toasts in ~60s). → One batched toast: `Testing 9… · 3 live · 6 dead` with a count-only pass; failure detail lands in the inspector Health view.

**A8-3 [P2] Toast text leaks implementation strings** — `Update check failed: update feed fetch failed` (`f_0267`) → human copy («Couldn't reach the update feed»), no internals.

**A8-4 [P3] Clipboard refusal toast is 4 lines** (`Clipboard text is not importable — nothing imported (paste a share link, JSON, conf, or preset link)`, `f_0214`) → one-liner («Nothing importable in clipboard»).

---

## B. Quietness spec — the redesign this hunt points to

User's frame: «i want some bloat cut out … wth is this, check other good apps like throne and v2rayn or necobox … i want standards apply» + «everything is a mess almost».

The Throne references (user-supplied) define the target:

- **List-first window, no right inspector.** Throne: `#/Type/Address/Name/Test Result/Traffic` table + group chips top + bottom utility tabs (`Logs · Connections · Traffic Graph · Runtime Stats`) + 3-zone status strip (`Not Running · Mixed: 127.0.0.1:2080 · Proxy: Direct:`).
- **Detail lives in the edit modal** (`Common` → one `Advanced Settings` → protocol rows → OK/Cancel).
- **Health as a colored number column** (ms, green→red scale; `Unavailable` gray) — no per-row prose badges.
- **Paste = instant import into the current group.** No blocking popup.

ManyProxy keeps its own identity (bind:port, multi-engine, export/restore, health probes) but adopts the grammar. The concrete targets, all user-confirmed:

1. **Right panel = one surface, three tabs** (Details / Health / Logs · Connections · Traffic in one strip); Details is a scroll summary (badge → endpoints → actions), no second tab row, no pinned operate row, no dead gap.
2. **Add/Edit modal = Throne grammar** (A7-1); raw JSON gone from dialog and routing presets.
3. **Bulk = Start/Stop/More ▾**; **checked rows = accent bar + chip fill**.
4. **Menus cut:** Tools 21→≤8 grouped, row 17→7+More; Speedtest/Resolve ops cut; Settings button opens modal (no 1-item menu); Manage groups menu split into panel tabs + Settings▸Maintenance.
5. **Status bar 3 segments**; glyphs fixed at small size (↓ arrow, sort arrows).
6. **Toasts batched, parked bottom-right, never on chrome; delete = undo toast; paste import instant.**
7. **Copy affordances:** row Copy-column removed; copy via selection/kebab/`Ctrl+C` (3 total surfaces, not 5).

Constraints still honored: local-only (no system proxy/TUN/DNS/telemetry), engines beside exe (no PATH/downloads), DPAPI-sealed secrets & redacted logs/clipboard carry `bind:port` only, export explicit-confirm stays blocking.

---

## C. Decisions log (answered by user, 2026-10-24)

| Topic | Decision |
|---|---|
| Raw JSON | Hidden entirely from Add/Edit; routing presets lose the `Rules JSON` textarea too |
| Standard reference | Throne (2 screenshots supplied: edit-modal grammar + list/status grammar) |
| Paste import | Straight into current group, no blocking dialog |
| Blocking popups | Hated — import/delete become non-blocking (delete: undo toast); export stays explicit-confirm |
| Bulk bar | `Start/Stop + More ▾` |
| Checked rows | Thin left accent bar + filled chip (no blue rims, no row-level numeral chips) |
| Inspector | Single scroll + segmented view switcher; kill the 2×3 tab strip |
| Right-side views | One panel + tabs (Subscriptions/Logs as tabs, not popovers) |
| Speedtest/Resolve | Cut from Tools + row menus |
| Status bar | 3 segments |

Open (no user block): whether Subscriptions/Groups/cleanup move under a renamed `Library…` menu vs Settings split (A6-3); whether Command palette gets a visible hint (A6-6).

Unverified-from-code (needs a manual pass when building fixes):
- Stray chip `Updates settings` inside the Settings nav at t≈227 (`f_0228`) — the literal string doesn't exist in `src/`; likely a transient hover/tooltip artifact. Watch for it while touching `src/app/settings.rs`.
- `Settings` nav item labeled `Update` (f_0221) vs `Updates` (f_0224) — almost certainly the same item; confirm label while editing.

---

## D. Fix backlog (proposed order)

**P0 — Architecture & contradictions**
1. Unify the right panel: single scroll Details + Health/Logs/Connections/Traffic view switcher; kill Subscriptions/Logs drawers as separate surfaces (A4-1, A4-2, A7-6, A7-7).
2. Rebuild Add/Edit + Routing-presets dialogs on the Throne grammar; remove raw JSON (A7-1, A7-8).
3. Checked-set + bulk bar redesign (A3-1, A3-2).
4. Non-blocking import/delete (paste-instant, undo toast, batched test toasts) (A8-1, A8-2).
5. Row/port/badge single state system — lamp=endpoint, badge/status agree (A3-3, A4-3).

**P1 — Cut the noise**
6. Menus (A6-1–A6-3), Settings modal footer+space+red slabs (A7-2–A7-4), toolbar dedupe+Settings button (A1-3, A1-4), Status→filter simplification (A2-1), inspector action-row merge (A4-8), status bar 3 segments (A5-1), toggle-in-place for Show security (A6-4).

**P2 — Polish**
7. Small-glyph set (↓ sort arrow, kbd chips) (A5-1, A6-5, A3-5), Health module typography (A4-5, A4-6, A4-7), engine section lean-down (A7-5), scrim/drawer sizing (A7-9), toast copy (A8-3, A8-4), titlebar window controls (A1-1).

**P3 — Sweep**
8. Import dialog dupes (A7-10), stray dash (A5-2), palette hint (A6-6), artifacts noted above.

*Relationship to DESIGN.md: every P0/P1 item above enforces an existing DESIGN rule (signal colors, badge grammar, two font sizes, flat surfaces, selection-vs-check separation, red discipline) — this is a conformance pass plus the user-approved modernization, not a restyle.*

## E. Outcome (2026-10-25)

All backlog items (P0 1–5, P1 6, P2 7, P3 8) landed across ticket waves K1–K17; per-ticket detail and accepted deviations live in `tickets.md` (tools-menu `≤8` row target recorded as an accepted deviation at 11 rows). Final regression by the orchestrator on the settled tree: `cargo fmt --check` clean · `cargo clippy --all-targets --locked -- -D warnings` clean · `cargo test --locked` **1301 passed / 0 failed**. The nine-marker boot-path scan in `src/tests.rs` stayed intact throughout. Residual items still needing a human eye or hardware (UNVERIFIED): secondary-monitor/DPI window placement, sort-header paint on a populated library, palette-hint chip at real DPI / min-width, and the five-hot-states walkthrough (empty · 9 checked · inspector Details/Health · Settings modal · delete-undo toast).

**K19 addendum (2026-10-25, real-eyes pass by the verification agent, evidence in `Temp\opencode\hot5\`):** five hot states + stretch VERIFIED — including second-display rendering (`06-stretch\secondary-monitor.png`; both screens 1920×1080 @100%) and the Ctrl+K chip unclipped at 882/1400px, which closes placement and chip items from above; only DPI>100% remains hardware-unreachable-verified. New findings logged: D1 P2 inspector tab row clips at the default 1002px drawer width (Connections truncated, Traffic invisible; `detail.rs` segmented row), D2 P3 the DESIGN.md empty-library paragraph still prescribes in-canvas Add/Import (stale vs K11's toolbar-only entry and the asserting tests), D3 P3 the dialog-kit modal carries an invisible interactive 24×24 Close hit-target (Settings top-right). K19 ran verification-only (no tree changes), imported its own 9 test rows via the live paste-import path, left the library as it found it (empty → empty). Fixes rolled into K20.

**K20 addendum (2026-10-25, commit `cccf6e5`):** D1 fixed by sizing the segmented tab bar to content (paint test added at the default width; suite 1302/0); D2 doc line synced; D3 closed with upstream `Dialog::close_button(false)` across all dialog routes — root cause was a gpui-component icon Button painting blank because the kit's close-icon assets aren't registered in this app, leaving an interactive but invisible 24×24 target.
