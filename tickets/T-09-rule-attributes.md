# T-09 — Rule-attribute enrichment (Source tab, invert, ip_version, process_path_regex)

- **id**: T-09
- **title**: Rule-attribute enrichment — add Source tab (`source_ip_cidr`, `source_ip_is_private`, `source_port`, `source_port_range`), `invert`, `ip_version`, `process_path_regex`
- **epic**: E3 — Routing & interop borrow
- **size**: S
- **depends-on**: T-08
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-09 line; gap §4.1 R2 (engine pass-through already free: `merged = obj.clone()` at `engine.rs preset_rules_for` ~2268 — the gaps are validation-blindness and editor blindness); §2 grammar matrix rows "source_* missing [C grep 0]", "invert missing", "ip_version absent", "process_path_regex absent"; §4.1 R2 note on why per-listener source rules are unusually valuable.

## Why

For a listener-farm app the *source* side of a rule is the local client app: `source_ip_*`/`source_port` rules express "only clients from 127.0.0.1 ride exit X; LAN clients go direct" with zero OS touch — uniquely valuable here. The engine already passes unknown attribute keys through (`engine.rs` clone), so the missing pieces are (a) intake validation for the few keys Throne validates, and (b) the editor surfacing the keys at all (`rule_attribute_tabs` at preset.rs:539 has no Source tab, and "Other"-tab tolerance hides them from real use).

## What

1. **`preset.rs:539 rule_attribute_tabs` `GROUPS`** — add:
   - `("source_ip_cidr", "Source")`, `("source_ip_is_private", "Source")`, `("source_port", "Source")`, `("source_port_range", "Source")` → a new **Source** tab when any is present;
   - `("process_path_regex", "Process")` → lands in the existing Process tab;
   - `invert` and `ip_version` → flagged on the Domain/IP tabs per Throne (§D.1 carries them per-rule): map `("ip_version", "IP")` and `("invert", "Domain")`? — decision: `invert` is cross-cutting; render it on **every** tab by pushing Tab "Invert" when present (a dedicated tab beats guessing the fold). Keep it simple & mechanical: `("ip_version","IP")`, `("process_path_regex","Process")`, `("invert","Invert")`, Source×4 as above. "Other"-tab fallback (preset.rs:568–572) shrinks accordingly (these become known).
2. **Intake validation** (shared helpers from T-08 in `validate_preset_rules`): light structural checks, Throne-spirit:
   - `source_port`/`port`-like: integer or numeric-string; `source_port_range`/`port_range`: `"a:b"` shape;
   - `source_ip_cidr`: string (list form tolerated as today);
   - `ip_version`: integer 4/6; `invert`: bool; `process_path_regex`: string.
   - Unknown-key tolerance elsewhere unchanged (pass-through stays at intake; emission stays loud only for unknown **exits**, not attributes).
3. **Presets editor fold** — `main.rs PresetForm` (used by `open_presets_dialog` at main.rs:19103): add fold rows for the new keys in the same structured-editor style as Domain/IP/Port/Process rows today (no new UI framework; values edit as JSON-shape-aware fields like the existing matcher rows do).
4. **Emission**: nothing to do beyond T-08's shared pass-through (verify by test: rules with the new keys emit verbatim into generated `route.rules`).

## Files to touch

- `src/preset.rs` — `GROUPS` table (545–558), validation helpers, tests
- `src/main.rs` — PresetForm fold rows + render tests
- `src/engine.rs` — pass-through approval tests only (no behavior change)

## Acceptance criteria

- [ ] `rule_attribute_tabs` returns `Source`/`Invert`/`IP`(ip_version)/`Process`(process_path_regex) tabs correctly and stops leaking those keys to "Other".
- [ ] Structural validation errors on malformed port/ip_version shapes nav to the same `PresetError` family as today's field messages.
- [ ] Editor renders the new folds; saved rules round-trip through share links (`preset_share_link`, preset.rs:497).
- [ ] Emission pass-through pinned: a rule with `source_ip_cidr` + `invert` + `process_path_regex` emits verbatim in `build_main_config` output.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] Existing `t10_rule_attribute_tabs_name_matcher_groups` (preset.rs:967) stays green (extend, don't break).
- [ ] New tests (prefix `t59_`, verified unused): `t59_tabs_surface_source_group`, `t59_tabs_surface_invert_ip_version_process_regex`, `t59_validation_shape_checks`, `t59_editor_folds_render_new_keys`, `t59_emission_passes_new_attributes_verbatim`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A.
- [x] Plaintext-always-sealed: N/A (matchers are routing policy, not credentials — consistent with the earlier ledger ruling that matcher values stay plaintext inside the preset).
- [x] Hijack non-concept: N/A (attribute keys only).
- [x] No system mutation: config-JSON only.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: N/A.
- [x] Sidecars app-dir only: N/A.

## Out of scope

- `rule_set`/`.srs` staging (T-12) even though a `rule_set` key will eventually appear in `GROUPS` (that ticket owns it).
- Simple-rules boxes (T-11).
- Any source-IP-driven mutation of client behavior beyond route-rule semantics (none here — config lines only).
