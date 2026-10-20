# T-12 — `.srs` rule-set local staging

- **id**: T-12
- **title**: Rule-sets section — explicit confirm-per-download `.srs` staging under `<appdir>\data\rule-sets\`, engine emits `route.rule_set` with `local_path`
- **epic**: E3 — Routing & interop borrow
- **size**: M
- **depends-on**: T-08 (emission surface), T-09 (`rule_set` joins the known attribute set); live proof joins T-08's gated family with no further workflow edits
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E3 T-12 line + Q4a (**Option A only** — local staging; **Option B sidecar-remote forbidden** — a sidecar fetching at spawn is silent-by-design and violates the no-silent-downloads posture); gap §4.1 R3 (both options evaluated; A recommended = settled), §2 "rule_set/remote `.srs` sets … Missing".

## Why

Throne's rule-sets power (binary `.srs` catalogs referenced by presets) is also the base for any future country preset (PARKED) — the staging rail is what makes them possible later. The settled design keeps every fetched byte **user-initiated and visible**: explicit confirm per download through the same hardened fetch policy subscriptions use (`subscription_fetch_policy`, store.rs:149 — https-only, internal hosts blocked, resolved-IP recheck), cached under the app's own data dir, and emitted into the config as **`local_path`** so the sidecar never performs network fetches at spawn (Q4a: Option B is dead).

## What

1. **Rule-sets section** in the presets manager (`open_presets_dialog`, main.rs:19103): a sub-list "Rule sets" with entries `{name, url, bytes, downloaded_at, used_by presets}` and actions: **Add by URL** (requires the explicit confirm callout "Download this rule set? <url>" — one confirmation per download; a batch add is several single confirms, no silent batch), **Remove** (removes the cache file + row only if unused; used sets must be detached from presets first — loud inline error, no silent orphan files).
2. **Download flow**: reuse the subscription fetch pipeline (T-07's shared client/header path where landed): `subscription_fetch_policy` gates before any network; size cap 8 MiB (`SUBSCRIPTION_MAX_BYTES`, store.rs:170); timeout/redirect treatment as `fetch_subscription_body` (main.rs:9260). On success the file lands at `<app_data_dir>\rule-sets\<name>.srs` (name sanitized like row ids; collision → suffix, never overwrite silently — "overwrite" is an explicit re-Download confirm on the same entry).
3. **Store**: new table `rule_sets (id, name, url, file_name, bytes, downloaded_at)` created in `create_schema` (additive; no user-version bump), CRUD accessors + `rule_sets_in_use(preset_id)` helper to protect Remove. The binary file itself is cache state, not a secret: no DPAPI treatment (it is a compiled rule catalog, equivalent to a subscription body).
4. **Preset authoring**: rules may reference staged sets — `{"action": ..., "rule_set": ["<name>"]}`; T-09's `GROUPS` gains `("rule_set","Rule set")` so the editor carries a Rule set tab; validation: referenced names must be a staged set **at attachment time** (loud PresetError naming the missing set) — same loudness taste as unknown exits.
5. **Engine emission** (`preset_rules_for` engine.rs:2214 → `build_main_config` at engine.rs:4095): when any preset rule carries `rule_set`, emit a top-level `route.rule_set[]` array (entry `{type:"local", tag:"rs-<sanitized-name>", format:"binary", path:"<absolute staged file path>"}`) — deduped per config build; rules keep their `rule_set:[tag]` reference (name → tag mapping折叠 into the merge). **Missing file at configgen → loud `BadPreset` error naming the set and the file path** (never skip, never substitute).
   - the config generator must never copy `.srs` bytes anywhere else; the path points at the staging location (sidecar-asset posture: file lives beside the app under `data\`).
   - **Option B is dead**: no emission of `type:"remote"` rule sets anywhere; a guard test asserts no generated config ever contains `rule_set` entries with `remote`.
6. **Cache dir creation**: staging dir created on demand like T-02's `data\` (same helper family); path resolvable via the T-02 app-data helper.
7. **Live proof (joins T-08's gated family)**: `t62_live_srs_rule_set_preset_check` — under `MANYPROXY_LIVE_CHECK=1` + `MANYPROXY_SINGBOX_BIN` (pinned sing-box 1.14.2 from `live.yml`, **no further live.yml edits**): compile a small source JSON rule set to `.srs` **at proof time** by invoking the pinned binary's `rule-set compile` (the proof-driven compile is outside the app: it models the maintainer-hosted artifact), stage it via the app's own staging path, generate the preset→config through `build_main_config`, and pass pinned `sing-box check`. Asserts the full generate→check contract including `route.rule_set` configgen.
   - **PWB note carried**: the pinned binary fetch in `live.yml` is the CI job acting for the maintainer (official upstream URL, live.yml:31); the *app* fetches `.srs` files only when the user confirms each download in-session.

## Files to touch

- `src/store.rs` — `rule_sets` table, CRUD, in-use guard, tests
- `src/engine.rs` — emission into `build_main_config`, name→tag mapping, missing-file loud check + tests
- `src/preset.rs` — `rule_set` known-attribute validation (with T-09 helpers) + tests
- `src/main.rs` — Rule sets section UI (confirm-per-download, remove-guard), fetch call via the shared client family + tests

## Acceptance criteria

- [ ] Add by URL: policy gate → confirm → download → staged file appears listed; two identical downloads require two explicit confirms; no silent downloads from any layer (sidecar or app — app-side fetch only through the confirmed flow).
- [ ] Internal-host and plaintext-http URLs behave exactly like subscription policy (block / warn).
- [ ] Preset referencing a staged set: attachment validates; configgen emits `route.rule_set` with `local_path`; check-level correctness proven by the gated live test.
- [ ] Missing staged file at configgen → loud `BadPreset` naming the set; generate never silently proceeds.
- [ ] Remove: blocked while referenced; detaching frees removal; file delete leaves no orphans.
- [ ] No generated config ever contains a `remote` type rule_set entry (guard test).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t62_`, verified unused): `t62_rule_set_download_requires_explicit_confirm`, `t62_rule_set_fetch_policy_parity_internal_blocked`, `t62_rule_set_staged_under_data_rule_sets`, `t62_rule_set_used_guard_blocks_remove`, `t62_emission_local_rule_set_entries`, `t62_missing_rule_set_fails_loud`, `t62_no_remote_rule_set_emission_ever`, `t62_live_srs_rule_set_preset_check`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: cache sits beside the exe (`<appdir>\data\rule-sets\`, T-02 helper).
- [x] No-migration: new table additive; cache is derived state.
- [x] Plaintext-always-sealed: N/A — `.srs` is a binary rule catalog, not a credential container; DPAPI policy untouched.
- [x] Hijack non-concept: rule-set machinery carries **no** DNS-mapping/DNS-server rule keys; nothing named/mentioned/built for it.
- [x] No system mutation: user-confirmed https fetches only; sidecar spawn never fetches (Option B dead, tested).
- [x] Clipboard discipline / Redaction: the rule-set URL redacts via the same `redact_link`-family strings if ever surfaced in error toasts (keep failure text without full query strings).
- [x] Sidecars app-dir only: staged files live in the app dir; engine binary resolution untouched.

## Out of scope

- Download-Profiles country menu, remote-preset auto-update, Adblock preset (PARKED/dead — INDEX is the source).
- Rule-set *editing* (a .srs is binary; the source-JSON path is proof-only).
- Sharing rule sets via `throne://` deeplinks (not asked).
- Any remote-type emission (<Option B> permanently dead).
