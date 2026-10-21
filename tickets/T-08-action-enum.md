# T-08 — Action enum extension (NO hijack) + live proof family

- **id**: T-08
- **title**: Action enum extension — `sniff`, `resolve`, `route-options` (+ `override_address`/`override_port`), `reject` (+ Throne `method` drop/reply, `no_drop`) — ref. H3 — and **no hijack-dns**: only T-08 extends `.github/workflows/live.yml`'s gated proof family
- **epic**: E3 — Routing & interop borrow (roadmap rank 1)
- **size**: M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-08 line (verbatim) + rule 4; Q14 (hijack total ban — **supersedes** gap R1's "map to `direct` and annotate" proposal: no mapping, no intake special-casing, no guidance strings, no docs mention); gap §4.1 R1 (action table evidence), §3 "the one unfaithful bit" (narrow action set breaks Throne profile import), §7 rank 1; verified symbols: `preset.rs:6 VALID_PRESET_ACTIONS`, `preset.rs:8 served_action`, `preset.rs:104 validate_preset_rules`, `preset.rs:641 bad_action_names_index_and_field`, `preset.rs:794 t42_warp_action_guidance_error`, `engine.rs:2214 preset_rules_for` (emission gate at 2247–2251 the same `VALID_PRESET_ACTIONS`), `engine.rs:10758 ticket37_generate_without_presets_is_byte_identical`, `engine.rs:10767 ticket37_preset_rules_merge_before_isolation_rule`.

## Why

Throne-authored route profiles (`throne://route` links and the Throne ecosystem's structured-rule JSON) use the full sing-box action vocabulary — `sniff`, `resolve`, `route-options`, `reject` with `method`/`no_drop`. ManyProxy's intake accepts only `route|proxy|direct|block|reject` (`preset.rs:6`), so Throne-shaped presets fail whole-profile import, and even the attributes that *would* survive pass-through (`preset_rules_for` clones unknown keys at engine.rs:2268) can't carry the actions that make domain rules work on IP-only listener traffic. Extending the enum is the single highest-ROI interop borrow (gap §7 rank 1): it unblocks Throne-ecosystem transfer while keeping ManyProxy's stricter posture — unknown exits still fail **loud** at emission (engine.rs:2284 "names an unknown exit" stays byte-for-byte).

The other decision is a deletion: gap R1 proposed mapping Throne's DNS-mapping action to `direct` with an import note. The brief **overrode** that (Q14/rule 4): the action is not a concept here — no special case, no mapping, no note, no docs mention. Presets containing it simply trip the existing generic unknown-action intake error (`validate_preset_rules`'s "must be one of …" string), whichTicket accepts as the correct, minimal behavior and pins by test.

## What

1. **`preset.rs:6 VALID_PRESET_ACTIONS`** → `["route", "proxy", "direct", "block", "reject", "sniff", "resolve", "route-options"]`. The DNS-mapping action is **not added**, not mapped, not special-cased anywhere.
2. **`preset.rs:8 served_action`** unchanged in spirit: `block → reject`, `proxy → route`, others identity (so `sniff`/`resolve`/`route-options` pass through verbatim).
3. **Intake validation** `validate_preset_rules` (preset.rs:104) — mirror per-action field rules at intake (Throne §D.1 table) while keeping it structural (no semantic inference):
   - `sniff`: no `outbound` (general "outbound needs action route" rule already enforces via route_like check — keep); may carry optional structural fields consistent with sing-box route-rule attrs (pass-through policy already tolerates unknown keys — see T-09 for first-class tabs).
   - `resolve`: optional `server` (string), `strategy` (string); no `outbound`.
   - `route-options`: optional `override_address` (string), `override_port` (u16 range), optional `outbound` — allowed here (Throne serves route-options with/outbound target); emission maps `outbound` under the same **known-exit-only** gate as `route` (unknown exit → loud error, engine.rs:2284 rule applies unchanged).
   - `reject|-` (and Throne `block` normalization): optional `method` ∈ {`drop`, `reply`, `default`}, optional `no_drop` (bool); no `outbound`.
   - Warp actions `warp`/`warp-bypass` keep today's dedicated guidance string (preset.rs engine-warp branch at engine.rs:2242–2245 and its test `t42_warp_action_guidance_error`) — behavior unchanged.
4. **Mirror gate at emission** `engine.rs:2214 preset_rules_for`: the second VALID_PRESET_ACTIONS check (2247) automatically accepts the new actions; update the mirror's per-action field checks (route_like branch at 2254: add `route-options` to the outbound-carrying set; `sniff`/`resolve` to the no-outbound set; `reject` keeps `method`/`no_drop` keys flowing through the clone at 2268). Implement intake+emission as **shared fn(s)** in preset.rs called from both sites so the mirror can never drift (the two lists today are duplicated — this ticket must unify them; the duplicate checks are debt from ticket37).
5. **Emission tests**: per action, `build_main_config`-level assertions that the served route rule is the served action + attrs; `ticket37_generate_without_presets_is_byte_identical` (engine.rs:10758) must stay green — no presets → byte-identical configgen, unperturbed by the enum growth; `ticket37_preset_rules_merge_before_isolation_rule` (10767) stays green.
6. **Docs**: none (hijack non-concept; no new guidance strings; no new names beyond `sniff`/`resolve`/`route-options` route-rule spelling the engine already emits).
7. **Live.yml proof family (the only ticket that extends `.github/workflows/live.yml`)** — opt-in gated `MANYPROXY_LIVE_CHECK=1`, pinned sing-box 1.14.2 via the workflow input (workflow_dispatch, already wired: `live.yml:4–13` and the env pass-through at live.yml:41). Extend the workflow/test family with:
   - a new gated engine test following the existing gate pattern (`if std::env::var("MANYPROXY_LIVE_CHECK")... != "1" { return }` — engine.rs:8558/10886/12923): **`t58_live_throne_country_profile_check`** — a hard-coded Throne-country-*style* profile in Throne doc shape — `sniff` opener, private-range direct rules (`source_ip_is_private`/`ip_is_private`), country-suffix `domain_suffix` direct list, catch-all `route` — **minus the leading DNS-mapping rule** Throne's profiles carry (banned: the proof strips it; it is not generated, named, or emulated). The profile flows the real pipeline: parse → import → attach → `engine.rs` `build_main_config` generate → pinned `sing-box check` green.
   - the gated family is the same pass `live.yml` already runs (`cargo test --locked` under the gates, live.yml:44–45) — T-12's `.srs` rule-set proof **joins this family without further workflow edits** (its test simply lives in the same gated set; see T-12).
   - live.yml edit limited to whatever the new proof needs (e.g. note the profile case in a step summary); the fetch-sing-box step and pins stay exactly as-is.
   - **PWB discipline carries over**: `live.yml` fetching the pinned sing-box is the *CI job* acting for the maintainer from the official upstream release URL (`https://github.com/SagerNet/sing-box/releases/download/v1.14.2/...`, live.yml:31) — never the app.

## Files to touch

- `src/preset.rs` — `VALID_PRESET_ACTIONS` (6), `served_action` (8), `validate_preset_rules` (104) + shared per-action helpers + tests
- `src/engine.rs` — `preset_rules_for` (2214) mirrored via the shared helpers + emission tests + the gated `t58_live_throne_country_profile_check`
- `.github/workflows/live.yml` — extension as scoped above
- nothing in main.rs expected (no UI copy changes — the settings-routing-note stays as-is)

## Acceptance criteria

- [ ] Throne-shaped intake: a profile with `sniff` / `resolve` / `route-options`(+overrides) / `reject`+`method`/`no_drop` rules imports cleanly via `parse_preset_json` → store; byte round-trip preserved.
- [ ] The DNS-mapping action fails intake with the **existing generic** unknown-action error string; zero new machinery, strings, mappings, or docs (test `t58_no_hijack_concept_no_mapping_no_guidance` asserts intake error message unchanged AND that sharing no new code paths mention the banned token).
- [ ] Unknown exits stay loud: `route-options`/`route` with an unknown `outbound` → engine error "names an unknown exit" unchanged.
- [ ] `t42_warp_action_guidance_error` and `bad_action_names_index_and_field` behaviors preserved (warp guidance; generic bad-action).
- [ ] No presets → byte-identical configgen (ticket37 byte test green).
- [ ] Guarded live proof green once dispatched (`MANYPROXY_LIVE_CHECK=1`, pinned sing-box 1.14.2): generated Throne-country-style profile (minus banned leading rule) passes `sing-box check`.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t58_`, verified unused): `t58_intake_accepts_new_actions`, `t58_reject_method_and_no_drop_round_trip`, `t58_route_options_override_fields_validate`, `t58_resolve_server_strategy_pass_through`, `t58_emission_serves_each_action_verbatim`, `t58_no_hijack_concept_no_mapping_no_guidance`, `t58_live_throne_country_profile_check`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A.
- [x] Plaintext-always-sealed: N/A.
- [x] **Hijack non-concept**: enforced by test + code review — no enum entry, no mapping branch, no guidance string, no doc sentence beyond the generic unknown-action fact (T-01).
- [x] No system mutation: pure config-JSON rewrite; assert_no_system_mutation family untouched and green.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.
- [x] Sidecars app-dir only: proof uses the pinned binary under the gates (as today); never `PATH` (binary passed via `MANYPROXY_SINGBOX_BIN`).

## Out of scope

- Any hijack mapping/intake note (dead — Q14).
- Rule-attribute tabs (T-09), `rule_set` staging (T-12), names/lock (T-10), grammar boxes (T-11) — sequenced after.
- Download Profiles menu / auto-update (PARKED), Adblock (DEAD).
- Emission-side DNS objects of any kind (dead).
