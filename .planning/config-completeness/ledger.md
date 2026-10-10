# Ledger: config-completeness

Branch: `dev`. Program: guided editors for every protocol (Throne parity), kit-maximal.
Verify trio per ticket: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.
Constraints: AGENTS.md (local-only, app-dir sidecars, DPAPI+redaction, hermetic TEMP-only tests, no real sidecar spawn unless live gates).

## Ticket status

- [x] 01-foundation-picker-locks
- [x] 02-vless-shared-panels-mux-engine
- [ ] 03-vmess-trojan-shadowsocks
- [ ] 04-wireguard-amneziawg
- [ ] 05-hysteria-tuic
- [ ] 06-small-matrices-ssh
- [ ] 07-edge-xray-chain-extracore
- [ ] 08-intake-direct-fates

## Completions

- Task 01: complete (commits ef5117d..463a670, tests: cargo test --locked → 830 passed, 0 failed)
- Ruling: popup-list virtualization — only visible option rows paint, so full-23 asserts use top-row paint + delegate membership (`set_selected_value` resolves all 23); virtual-type switches drive arrows+Enter instead of option clicks. Kit: `Select::new(&state)` + `.id().small().accessibility_label()`, `Field` + `v_form`, outer-wrapper `Tooltip`, `Label` for the edit lock.
- Ruling: Template button stays explicit-overwrite (click = consent, toast names protocol); the pristine-only rule covers the automatic type-switch path (`sync_protocol_template` in dialog builders) only.
- Ruling: ENTER-guard test is validation-sensitive by construction — a broken guard would run the commit on the untouched (invalid) template and fire exactly one error toast; zero toasts + open dialog + zero rows + closed popup pins the guard rather than the validator. Guard mechanism: wrapper `.on_action(Confirm → stop_propagation)` (kit `BaseSelect` propagates Confirm even with popup open, verified gpui-base 0.6.6 `select.rs`, while Cancel stops).
- Task 02: complete (commits 463a670..9532d96, tests: cargo test --locked → 842 passed, 0 failed)
- Ruling: S4 ships dial half only (4 toggles + timeout/bind/inet4/inet6); TLS-extras deferred — they need engine `tls{}` mapping, else `check` fails on unknown flats.
- Ruling: S2 emits only `apply_tls`-mapped keys (no fragment/tricks/rec-frag UI) — unwritten panel state would ride lenient passthrough into a `check` failure.
- Ruling: S1 conditionals only engine-mapped keys (ws: path/host; httpupgrade/http: host/path; http:+method; grpc: service_name); `headers`/early-data stay JSON-only, never dropped by the engine.
- Ruling: brutal speeds preserved-not-emitted (no sing-box 1.14.2 mapping; engine consumes them off the cloned generation map); mux-force still holds — the panel folds brutal alongside `mux:"on"`, `t02_mux_emission_shape_on_four_protocols` pins `multiplex:{"enabled":true}` on ss/trojan/vless/vmess.
- Ruling: `check`-green proof is hermetic via `generate()` shape (no sidecar binary in dev env; live-gate env can run real `check`); per-key table: absent/`""`→omit, `"on"`→enabled:true, `"off"`→enabled:false, garbage→typed `bad_option` naming `mux`.
- Ruling: ss arm ordering is load-bearing — `apply_mux` runs before `drop_foreign_flats` (the drop list now eats mux keys); the emission test caught the wrong order live. Mux flats also added to the drop list so non-mux protocols can't break `check` on provider junk.
