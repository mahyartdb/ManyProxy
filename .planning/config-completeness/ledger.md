# Ledger: config-completeness

Branch: `dev`. Program: guided editors for every protocol (Throne parity), kit-maximal.
Verify trio per ticket: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.
Constraints: AGENTS.md (local-only, app-dir sidecars, DPAPI+redaction, hermetic TEMP-only tests, no real sidecar spawn unless live gates).

## Ticket status

- [x] 01-foundation-picker-locks
- [ ] 02-vless-shared-panels-mux-engine
- [ ] 03-vmess-trojan-shadowsocks
- [ ] 04-wireguard-amneziawg
- [ ] 05-hysteria-tuic
- [ ] 06-small-matrices-ssh
- [ ] 07-edge-xray-chain-extracore
- [ ] 08-intake-direct-fates

## Completions

- Task 01: complete (commits ef5117d..82e5f30, tests: cargo test --locked → 830 passed, 0 failed)
- Ruling: popup-list virtualization — only visible option rows paint, so full-23 asserts use top-row paint + delegate membership (`set_selected_value` resolves all 23); virtual-type switches drive arrows+Enter instead of option clicks. Kit: `Select::new(&state)` + `.id().small().accessibility_label()`, `Field` + `v_form`, outer-wrapper `Tooltip`, `Label` for the edit lock.
- Ruling: Template button stays explicit-overwrite (click = consent, toast names protocol); the pristine-only rule covers the automatic type-switch path (`sync_protocol_template` in dialog builders) only.
- Ruling: ENTER-guard test is validation-sensitive by construction — a broken guard would run the commit on the untouched (invalid) template and fire exactly one error toast; zero toasts + open dialog + zero rows + closed popup pins the guard rather than the validator. Guard mechanism: wrapper `.on_action(Confirm → stop_propagation)` (kit `BaseSelect` propagates Confirm even with popup open, verified gpui-base 0.6.6 `select.rs`, while Cancel stops).
