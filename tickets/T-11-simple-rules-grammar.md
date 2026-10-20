# T-11 — Simple-rules grammar boxes (3 boxes)

- **id**: T-11
- **title**: Simple-rules grammar boxes — 3 boxes (direct/block/proxy) parsing Throne-style tokens into structured rules
- **epic**: E3 — Routing & interop borrow
- **size**: M
- **depends-on**: T-08 (action vocabulary; grammar boxes target the extended set), optional forward compat with T-12's `rule_set` refs
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-11 line & Q7-era roadmap 7 adoption ("simple-rules grammar boxes adopt"); gap §4.1 R5 (Throne §D.1 4 boxes + exact token spellings); §2 grammar matrix "Simple-rules line grammar … Missing".

## Why

Throne users bring a muscle-memory grammar — one rule per line, tokens like `domain:` `suffix:` `keyword:` `regex:` `ruleset:` `ip:` `processName:` `processPath:` — for the four "everything walks X" boxes. Adopting the *parser*, not Throne's four-box layout (warp-bypass is un-servable here and skipped): it lets communities share rule sets as plain text and keeps docs/transfer consistent with what Throne users already quote. The brief fixes the shape at **3 boxes** (direct / block / proxy) inside the existing PresetForm folding system.

## What

1. **Pure parser in preset.rs**: `pub fn parse_simple_rules(box: SimpleRulesBox, action: &str, text: &str) -> SimpleRulesParse` where `SimpleRulesBox ∈ {Direct, Block, Proxy}` (enum in preset.rs). Returns `{rules: Vec<serde_json::Value>, invalid: Vec<String>}`:
   - one rule per line; blank/`#`-prefixed lines skipped; multiple comma-separated tokens per line allowed (Throne-style), all matching the SAME line-action → single rule with merged matchers;
   - token spellings exactly per Throne (§D.1, cite in code comment-free — mirror the strings in test): `domain:`(exact string), `suffix:`(→`domain_suffix`), `keyword:`(→`domain_keyword`), `regex:`(→`domain_regex`), `ruleset:`(→`rule_set` array value — the rule carries `rule_set:["name"]`; emission requirements arrive with T-12: until then, rulesets referenced simply fail loud at configgen like any unknown exit — noted in the box help text as "requires a staged rule set (T-12) only after that ticket"), `ip:`(→`ip_cidr`), `processName:`(→`process_name`), `processPath:`(→`process_path`);
   - per-box action: Direct→`direct`, Block→`block`(normalized at emission to `reject` via `served_action`), Proxy→`proxy`(→`route`) — sharing the T-08 extended vocabulary and validation pipeline unchanged;
   - invalid lines (unknown token, bad shape, empty token) are **retained** as `invalid: Vec<String>` entries displayed in the editor under an "Invalid lines" fold, **never silently dropped** (Throne's `invalid rule:<raw>` display, minus its exact label — ManyProxy's own copy).
2. **Editor surface** (main.rs PresetForm): a new "Simple rules" fold with the three text areas labeled Direct / Block / Proxy. On save, the boxes' parses are appended to the structured rules; the structured rules view remains the single source of truth (boxes are sugar over the same JSON — re-opening re-parses only lines the app itself emitted back into the boxes? No: **boxes are write-once sugar**, not a live projection — structured rules and simple-rules boxes are two authoring layers over the same stored rules; state must round-trip deterministically: define the mapping as one-way append (parse time) and the invalid fold mirrors whatever parse produced it in the *same session*. Aims: minimal state, no syncing machinery).
3. **No new errors** at intake — the parse output flows through `validate_preset_rules` exactly like hand-built rules.

## Files to touch

- `src/preset.rs` — `parse_simple_rules` + enum + parser tests
- `src/main.rs` — PresetForm fold + render/save wiring + tests

## Acceptance criteria

- [ ] Every documented token parses to the documented key; multiple tokens per line merge into one rule; comma/space separators per Throne.
- [ ] Invalid lines survive verbatim in the editor's Invalid fold and are never dropped or auto-corrected; a session-only behavior (no persistence of invalid lines in the rules JSON).
- [ ] `ruleset:` tokens produce `rule_set` refs that validate at intake (T-12 makes the staging path real; before that, configgen fails loud for unstaged rule sets — same class as unknown exit).
- [ ] Parse output passes the T-08 extended action validation unchanged; `block`→`reject`, `proxy`→`route` normalization intact.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t61_`, verified unused): `t61_simple_rules_token_matrix`, `t61_multi_token_line_merges`, `t61_invalid_lines_retained_not_dropped`, `t61_ruleset_token_maps_to_rule_set_ref`, `t61_box_actions_normalize`, `t61_panel_renders_three_boxes`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A.
- [x] Plaintext-always-sealed: N/A (rule texts are policy, not credentials).
- [x] Hijack non-concept: the parser gains **no** DNS/geo/token keys beyond the listed eight; nothing DNS-rewrite-flavored, no mention anywhere.
- [x] No system mutation: pure config authoring.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: pasted text is shown/edited only in the editor; nothing logged (no new logging surface).
- [x] Sidecars app-dir only: N/A.

## Out of scope

- A warp-bypass box (un-servable → not adopted; gap R5).
- Live projection round-trips from structured rules back into boxes (write-once sugar decision above).
- Ruleset staging/download (T-12).
- Country download menus / prebuilt simple-rule packs (PARKED — Q5).
