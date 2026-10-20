# T-22 — Export envelope `{schema, version, created, checksum}` + restore-side negotiation

- **id**: T-22
- **title**: Export envelope hardening — `{schema:"manyproxy-library", version, created, checksum}` + restore-side schema negotiation; verbatim-warning + cancel-writes-nothing preserved
- **epic**: E5 — Subscription & exchange pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E5 T-22 line; gap §4.6 P1 (borrow Throne's archive envelope semantics minus its magic binary — ManyProxy's self-contained explicit-confirm JSON posture stays); as-built: `export_payload_full` (store.rs:390, free fn building `{outbounds, library, subscriptions, tombstones}`), restore path `restore_library_json` (store.rs:6040) + `restore_library_file` (store.rs:3587) + dry-run → backup → swap pipeline, intake discriminator `is_library_export` (import.rs:2400), dialog string `SHARE_WARNING`-analog export warning (test `export_dialog_opens_with_verbatim_warning` main.rs:28707).

## Why

Versioned, self-describing exports are what let a future build know whether a user's export is ingestible (schema negotiation) or garbage (loud refusal) instead of deep-failing inside restore. Throne does this with the `THRN` archive; ManyProxy gets the same durability from four envelope fields + a checksum, staying JSON-*explicit-confirm* (keep the verbatim-plaintext warning + cancel-writes-nothing contract unchanged).

## What

1. **Envelope extension** — `export_payload_full` (store.rs:390) emits, at the top level, **before** the payload keys:
   ```json
   {
     "schema": "manyproxy-library",
     "schema_version": 1,
     "created": <unix-secs>,
     "checksum": "<fnv/serde-stable checksum of the canonical payload>",
     "outbounds": [...],
     "library": [...],
     "subscriptions": [...],
     "tombstones": [...]
   }
   ```
   - checksum: deterministic byte-level hash of the canonical payload (serde_json order-stable — hash of the minified payload serialization computed AFTER the envelope-free payload is materialized); no new crate (use the same FNV-1a approach visible in preset.rs cap-name, or SHA-free std hashing — simplest stable, not security-critical: it is an integrity cue, not a crypto claim).
2. **Restore-side negotiation** — `restore_library_json` (store.rs:6040) pre-step:
   - parse envelope: `schema` must equal `manyproxy-library`; `schema_version` ≤ current (1); else **loud refusal before any write** ("library export schema not supported (got X, supported ≤1)") — refusal leaves the DB untouched (dry-run first as today; restore order: parse → negotiate → dry-run → backup → swap already is the flow; negotiation slots in right after parse).
   - checksum match: mismatch → loud `RestoreError` naming checksum mismatch (still no write; old builds' exports that lack a checksum are **accept with a warning** when `schema_version` is missing — backward-compat envelope tolerance: allow unenveloped payloads exactly as today, rendering the existing dry-run+confirm).
   - `is_library_export` (import.rs:2400) still routes plain-file intake; update it to recognize the new envelope's first key too (idempotent for old payloads).
3. **Dialog string** — the export dialog's verbatim-plaintext warning and the restore-confirm text gain one clause: "Restore requires a matching ManyProxy export (schema manyproxy-library, version 1)." — keep the original warning text byte-identical otherwise (pinned by existing test `export_dialog_opens_with_verbatim_warning`, main.rs:28707 — extend, never rewrite).
4. Cancel-writes-nothing unchanged at all refusal points (negotiation failures are pre-backup).

## Files to touch

- `src/store.rs` — `export_payload_full` envelope + checksum + `restore_library_json` negotiation + tests
- `src/import.rs` — `is_library_export` envelope recognition + test
- `src/main.rs` — one dialog copy line + test extension

## Acceptance criteria

- [ ] New exports carry the four envelope fields; `restore_library_json` accepts them; tamper (1-byte flip) → checksum mismatch, loud refusal, **zero writes** (no `.bak`, no swap).
- [ ] Schema negotiation: mismatched schema/version → loud typed refusal pre-write; missing envelope (legacy export) → restore works exactly as today.
- [ ] `is_library_export` recognizes enveloped exports (paste/file intake parity).
- [ ] Existing export/restore round-trip tests pass; `export_dialog_opens_with_verbatim_warning` extension green; cancel at any step writes nothing (existing cancel-writes-nothing pins keep).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t72_`, verified unused): `t72_export_envelope_fields_present`, `t72_restore_accepts_enveloped_export`, `t72_checksum_mismatch_refuses_without_writes`, `t72_version_mismatch_refuses_without_writes`, `t72_legacy_unenveloped_export_still_restores`, `t72_is_library_export_recognizes_envelope`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A (envelope is format, not path).
- [x] No-migration: envelope versioning is forward-capability; no user-data migrations.
- [x] **Plaintext-always-sealed & warning discipline**: the verbatim-plaintext warning stays byte-identical; envelope adds no secret handling (checksum hashes the payload — it is not a redaction regression; the export already carries plaintext by explicit confirm).
- [x] Hijack non-concept: N/A.
- [x] No system mutation: N/A.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: `created` timestamp only; nothing user-identifying enters the envelope.
- [x] Sidecars: N/A.

## Out of scope

- Zip-archive/binary `THRN`-style format (JSON explicit-confirm stays the format).
- Encrypted exports (not asked).
- Export of presets/outbound share links changes (separate surface, untouched).
- Auto-save restore points or scheduled exports.
