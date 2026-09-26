# 26: UI-state races + error surfacing (main.rs)

**What to build:** Selection, dialogs, batches, and toggles that can't act on the wrong row, lose input, or lie about errors.

**Blocked by:** none (owns main.rs; runs after 25 lands only if 25 changes store APIs — additive-only per ticket 25, else parallel-safe).

**Owns:** gpui-kit

**Status:** done (commit 10a935b; `cargo test` 340 passed, verified)

Findings to fix (all src/main.rs, verify each in code before fixing):
- HIGH stale selection (112-128): apply_filter keeps index without identity check → search narrows, actions hit the wrong row. Fix: re-derive selection by id (set_rows:100-110 already does — reuse the pattern).
- HIGH dialog hijack (2545-2550 via 2500/2367): background refresh completion closes ANY open dialog + opens Subscriptions, destroying input. Fix: only touch the subs dialog if it is open; never close foreign dialogs; preserve subs add-form text across genuine reopens.
- MED edit staleness (1790-1826, 388-397, 1203-1222): prefilled snapshot + blind update. Fix: re-read + version/changed check at commit (commit_start_selected:1342-1344 is the clean pattern).
- MED batch races (2183-2193, 2742-2754, 2289-2308, 2230-2252): begin_refresh_batch resets unconditionally; manual-vs-tick double fetch + dup toasts + partial loss. Fix: generation guard, in-flight dedupe by sub id, atomic batch swap.
- MED switch meaning (2850-2879, 1259-1271 vs 1342-1460): switch writes flag only, engine untouched → divergent states. Fix: toast clarifying enable≠start, or couple them explicitly (one semantic, documented).
- MED subs-form loss (2555-2635): reopen rebuilds blank form. Fix: carry over in-progress text.
- MED auto-toggle snapshot (950-962, 2312-2329): re-read current value at commit.
- LOW L1 headless batch toast dropped (2506-2541); L2 LAN text vs commit set (1392-1417 vs 1422-1423: snapshot ids at open, commit by ids); L3 export preview frozen (1699-1721 vs 1728-1757); L4 logs dialog frozen (1629-1690); L5 logs double-close (1645-1653 + 267-281); L6 add-while-filtered UX; L7 double-commit relies on framework guarantee — add idempotency token. Fix or explicitly rule each.
- RUST unwrap_or_default lies (1393/1423/1449/1466): surface DB errors, never "library is empty".
- RUST set_enabled ignored (1351/1372/1431/1451): handle failure (rollback engine state or loud error).
- RUST stop swallowed (3236/3256 paths via apply): report honestly.
- RUST fetch lifetime (2335-2361 + 2720-2722): Weak entity for refresh tasks; no post-close store writes.
- RUST tick linger (979-1004): cancel promptly.
- RUST drop read failures (1958): name the skipped files.
- UI-MED unpaired key_context (2972-2973): wire app actions (Add/Import/Start/Stop/Test) to keyboard via actions!+bind_keys, or remove the dead context.

- [ ] Scenario: select A, search to only C → Action: toggle → Expected: acts on A or clears selection, never on C. Verification: automated test. Priority: required.
- [ ] Scenario: type in Add dialog during background refresh → Action: wait → Expected: dialog + input intact. Verification: automated test. Priority: required.
- [ ] Scenario: corrupt DB → Action: start all → Expected: error notification, never "library is empty". Verification: automated test. Priority: required.
- [ ] Prohibited side effect: no behavior change to parser/engine/store logic; keyboard additions must not steal dialog keys.
