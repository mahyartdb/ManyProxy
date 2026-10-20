# T-33 — Bind picker + Settings address list

- **id**: T-33
- **title**: Per-row bind-IP picker fed by a persisted host list (seeded 127.0.0.1 + 0.0.0.0 non-removable; IPv4/IPv6 literals only; Settings editor; port stays free-text)
- **epic**: E6 — UI affordance pack
- **size**: M
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-33 line (Q17/Q22 bind picker + list; "list seeds `127.0.0.1` + `0.0.0.0` (non-removable, always present); extras addable/removable, IPv4/IPv6 literals only (no hostnames); Settings hosts the list editor; existing rows' current binds auto-seed at migration of the list; port stays free-text; LAN confirm unchanged; custom-config rows stay JSON-governed)"; Q13 listener-auth creds stay dead ("Listener auth creds remain dead (Q13)").
  verified surfaces: inline endpoint inputs per row (`LibraryDelegate::pending_endpoint_commits`/endpoint_inputs — inventory §6; test `r3_row_endpoint_controls_paint`), save gate `save_listen_gate` (main.rs:8663) → `engine::validate_listen_for_save` (engine.rs:4825, wraps `validate_listen(name, bind_ip, port, true)`), Settings dialog (`open_settings_dialog` main.rs:13406), LAN gate `requires_lan_confirm`/`enforce_lan_confirm`/`warn_for_lan_binds` rows, meta family.

## Why

Bind entry today is free-text per row — typos surface only at save-gate time, and every row re-types the same three addresses. A picker bound to a hostname- **banned** list (`127.0.0.1`, `0.0.0.0`, plus user-added machine IPs) turns binds into one-click truth, keeps the port free-text it already is, and leaves the LAN-confirmation discipline byte-undisturbed (the confirm text and triggers come from the same `requires_lan_confirm` machinery).

## What

1. **List storage**: meta `bind_hosts` — JSON array of host strings + prohibited-removal markers carried implicitly: first entries are **constants**the app always unions in (`127.0.0.1`, `0.0.0.0` — never removable, always present at top); extras are user-added and order-preserved.
2. **Auto-seed** (one-time, in the list accessor's default construction): union stored extras with the two constants and with **every current row's `bind_ip`** value at first enable (this "migration of the list" is in-app state shaping — not a data migration; brief wording "existing rows' current binds auto-seed at migration of the list").
3. **Picker**: per-row inline endpoint bind input (row_editor's bind field, invocated from the endpoint_inputs pattern) becomes a picker (Select-style or combo) over the list; free-typed non-list entries are **still allowed where the list allows addition flows?** — decision: picker-only + "Add address…" affordance that opens the Settings list editor (keeps the list the single authority; no ad-hoc binding of unlisted hosts). Ports: **free-text unchanged** (save gate keeps parsing raw text; no picker for ports).
4. **Settings editor**: in Settings (section Basic or Paths-adjacent — implementer places near the "Paths" family or a new small "Addresses" group): value list editor — add (literal validate: IPv4 or IPv6 parse — `std::net::IpAddr::from_str`; hostnames rejected with inline field error "addresses only — no hostnames"), remove (blocked while any row currently uses the bind — inline error naming the blocking row(s)), duplicate rejected.
5. **LAN confirm unchanged**: choosing `0.0.0.0`/any non-loopback value flows through the existing `requires_lan_confirm`/`warn_for_lan_binds` path byte-identically (picker only changes entry mechanics; warning machinery and `enforce_lan_confirm` untouched). "Bind stays editable through custom-config rows JSON-governed" respected — custom-config rows do not consume the picker.
6. **Auth dead**: no listener username/password fields added anywhere (Q13).

## Files to touch

- `src/store.rs` — `META_BIND_HOSTS` const + typed accessor (seed union + used-hosts query) + tests
- `src/main.rs` — endpoint-input picker, Settings list editor, save-route wiring through the existing `save_listen_gate` path + tests
- `src/engine.rs` — none expected (`validate_listen_for_save` already validates literal binds)

## Acceptance criteria

- [ ] Picker lists: `127.0.0.1`, `0.0.0.0` (non-removable), then user extras; deleting/adding extras reflected live in all pickers.
- [ ] Add accepts only IP literals; hostname entry → inline field error (no hostname ever binds).
- [ ] Remove is blocked while used by any row (error names them); constants never removable.
- [ ] Existing rows' binds appear in the list after first pick use (seeded).
- [ ] Port field remains free-text; `save_listen_gate` flow unchanged; LAN confirm text/trigger byte-identical for non-loopback picks.
- [ ] Custom-config row editing unaffected (JSON-governed, no picker injection).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t83_`, verified unused): `t83_bind_hosts_meta_round_trip_and_seed`, `t83_constants_never_removable`, `t83_hostname_rejected_in_list_editor`, `t83_remove_blocked_while_used_by_rows`, `t83_picker_drives_existing_save_gate`, `t83_lan_confirm_unchanged_for_nonloopback_pick`, `t83_custom_config_rows_unaffected`.

## Constraints checklist (hard rules)

- [x] **Portability / no-AppData / No-migration**: additive meta; constants live in code, not migration files.
- [x] **Plaintext-always-sealed**: N/A — binds are addresses; no listener credentials added (Q13 dead stays dead).
- [x] Hijack non-concept: N/A.
- [x] **No system mutation**: bind *values* remain local bind decisions — no firewall/system-proxy interaction added; values editable only through existing save-gate validation (`validate_listen_for_save` unchanged).
- [x] **Clipboard discipline / Redaction**: N/A.
- [x] Sidecars app-dir only: N/A — binds flow to generated config exactly as before (`custom_config_listens`/WILDCARD_LISTEN behavior for custom rows untouched).

## Out of scope

- Listener auth (user/pass fields for the mixed inbound) — **dead** (Q13).
- Bind-per-group or global default bind (attachment is per row; no cross-cutting default asked).
- Hostname resolution at save (banned in-list; `validate_listen` remains literal-only).
- Upnp/port forwarding (never; no system surface).
