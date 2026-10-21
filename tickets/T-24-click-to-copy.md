# T-24 — Click-to-copy cells + `Copied!` transient tooltips (Connections tab)

- **id**: T-24
- **title**: Click-to-copy on Connections tab cells (dest/endpoint) with `Copied!` transient tooltips — clipboard discipline untouched
- **epic**: E6 — UI affordance pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-24 line ("Connections tab cells (dest/endpoint only; clipboard discipline untouched)"); gap §4.3 U4 (Throne §F.1 ~1.5 s tooltip copies; "keep the bind:port-only clipboard discipline — copy `dest`/`endpoint` values, never params"); verified: Connections tab region main.rs:10955–11114 (`ConnRow` core/id/dest/row rx/tx + deltas, `conn_selected_only`, sort desc by bytes), gpui-kit Tooltip usage exists today.
## Why

The values a user wants are already painted; copying them today requires a right-click menu detour. Throne's click-copy turns visible cells into one-click copy targets. scope-fenced to the Connections tab: only `dest` and the per-row endpoint values — **never** the chain/rule/param data (the map layer keeps secrecy; this is a presentation affordance at layer 1).

## What

1. Connections tab (main.rs:10955–11114 region): clicking the **destination hostname/IP cell** or the **row endpoint** cell of a connection row copies exactly the displayed string (dest as displayed; endpoint as the row's already-active `bind:port`-style contact — both are clipboard-discipline-safe categories per the rule; init the check per the row model's values).
2. Feedback: transient inline `Copied!` tooltip at the click site — gpui-kit Tooltip anchored to the clicked cell, ~1.5 s transient then auto-dismiss; no modal, no toast stack pollution (toasts stay for big actions).
3. Reset behavior: no state; the copy is direct — no selection changes, no filters, no re-render side effects beyond the transient tooltip.
4. Keyboard parity: row context-menu copy commands untouched; alt-key access unchanged (the cells keep existing tooltips readable).

## Files to touch

- `src/main.rs` — Connections tab cell wrappers (`on_click` + tooltip transient helper) + tests

## Acceptance criteria

- [ ] Clicking dest cell → clipboard holds exactly the displayed dest string; clicking endpoint cell → exactly the endpoint string; *no other cell or value is copyable this way*.
- [ ] `Copied!` tooltip appears at the cell, clears within ~1.5 s (round trip verified in the gpui_kit test harness).
- [ ] Repeat click copies again (idempotent; no state jam).
- [ ] Test-selected row flows, search, and export flows unaffected.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t74_`, verified unused): `t74_click_dest_copies_exact_string`, `t74_click_endpoint_copies_endpoint_only`, `t74_copied_tooltip_transient_clears`, `t74_other_cells_not_click_copyable`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Hijack / Sidecars: N/A.
- [x] **No system mutation / No plaintext risk**: dest and endpoint values are contact coordinate strings (RFC-allowable display text; no credentials, no params, no URLs). The set of copyable values is closed — enforced by the only-two-targets test above.
- [x] Clipboard `bind:port` discipline: preserved deliberately — the endpoint copy carries the same `bind:port` only; Share-dialog/confirm paths (e.g. `open_share_for_row` main.rs:12506 / `copy_row_urls` 12496) untouched.
- [x] Redaction: N/A (display values only).

## Out of scope

- Library rows (the main list) click-copy — not asked; row menus already cover it.
- Rows' param/链 data cells anywhere.
- Selection-extension copy or multi-cell copy.
