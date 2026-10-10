# Ledger: config-completeness

Branch: `dev`. Program: guided editors for every protocol (Throne parity), kit-maximal.
Verify trio per ticket: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`.
Constraints: AGENTS.md (local-only, app-dir sidecars, DPAPI+redaction, hermetic TEMP-only tests, no real sidecar spawn unless live gates).

## Ticket status

- [x] 01-foundation-picker-locks
- [x] 02-vless-shared-panels-mux-engine
- [x] 03-vmess-trojan-shadowsocks
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
- Task 03: complete (this commit, parent 5b2a5f9, tests: cargo test --locked → 852 passed, 2 failed — both pre-existing sandbox-DNS failures, see ruling)
- Ruling: vmess ships two grammars, not one — every `vmess://` import writes boolean `tls` + cipher in `security` (legacy), while hand/new-style rows carry tls-ness in `security` + cipher in `encryption` (a cipher in tls-ness `security` fails `apply_tls` with `unknown security`). Folds branch on `vmess_has_legacy_tls`: cipher→`security` (always-written, `auto` explicit) on legacy, cipher→`encryption` (`auto` removes) on new-style; S2 mirrors the toggle into the boolean on legacy via `fold_tls_vmess_into_params` (shared `fold_tls_security`/`fold_tls_details` halves, T02 behavior byte-identical). Kit: `Select::new(&state)` + `.id().small().accessibility_label()`, masked `Input` + `mask_toggle`, `.small()` `Button` generator, `RadioGroup` (`selected_index` + `on_change`) + `Switch` (`checked` + `on_change`) in shared S3/S4, `Field` + `v_form`, outer-wrapper `Tooltip`s, `Label` captions, `Collapsible` disclosures.
- Ruling: reality intent migrates legacy rows (boolean dropped, cipher moved to `encryption`, `security` becomes `reality`) — the legacy arm drops `pbk`/`sid`, so emitting them there would lie; untouched legacy rows never migrate (blank `pbk` keeps the boolean path byte-identical). Companion engine arm: legacy `insecure` now maps into `tls.insecure` (was hardcoded false, dropping the S2 toggle); `aid`→`alter_id` and `uot`→`udp_over_tcp` fold engine-side (t32 precedent: folded, not duplicated, canonical wins).
- Ruling: selects without a valid default use keep-blank folds (ss `method`, ss `plugin`: blank never fabricates nor strips) — `method` blank leaves the pristine template byte-identical (commit gate names the missing method), stored SIP002 `;`-tails survive a blank pick, and garbage normalizes only where T02 already did (`plugin` blank removes like `flow`). `uot` Switch writes the sing-box `udp_over_tcp` bool (socks-arm precedent, `engine.rs` allowlist) reading the `uot` spelling too; `aid` folds as text (import string spellings echo byte-identical, engine coerces to number).
- Ruling: S1–S4 render fns are shared (`s1_section`/`s2_section`/`s3_section`/`s4_section` + `uuid_row`/`panel_select_row`/`page_disclosure` helpers; vless_block shrunk to calls, T02 ids/behavior unchanged); ss page takes S3+S4 only (no S1/S2 — HasMux parity) and S4 dial-only (no TLS section exists yet — HasTLS gating holds vacuously); `vmess.global_padding/authenticated_length` untouched (no UI, never stripped, lenient passthrough). `clear_panels`/`echo_panels` take the protocol and reset/echo exactly what the page merged (ss skips S1/S2, so another page's unmerged typing stands).
- Ruling: full-suite `cargo test --locked` → 852 passed + 2 failed (`t41_dns_failure_typed_before_spawn`, `resolve_probe_ips_unresolvable_is_empty`) — both fail identically on the clean tree (verified via `git stash`): the sandbox DNS resolves `no-such-host.invalid`, so they are environmental, not T03 regressions. 12 new T03 tests (3 store fold/reader, 1 engine emission-shape, 8 UI incl. dropdown counts 5/3/18/3, SS HasTLS-gating, legacy-vmess + trojan/ss round-trips, plugin footer, per-ticket ENTER-guard).
