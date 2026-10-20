# EPIC-linux-macos-port.md — Future epic outline (requires its own grilling session)

> **Status**: outline only — NOT ticketed. Every open question below needs the owner's grilling before any spec/ticket exists. Nothing here is committed as scope.
> **Provenance**: brief "FUTURE EPICS" second bullet — linux/macOS port epic (Q29/32a, Q33a). The constraint note (owner-ratified): "ManyProxy stays Windows 10/11-only for the beta; linux/macOS rows are earned per-OS through a dedicated port epic; the AGENTS.md 'Windows-only' line gets rewritten to state supported-now (windows-amd64, then arm64) vs tracked-aspiration (linux/macos via port epic)" — T-01 lands that line.

## One-line intent

Earn linux and macOS per-OS release rows the same honesty way windows-arm64 was earned (gated rows only): keyring/unix frontier (DPAPI→Keychain/Secret-Service), dependency gating, gpui Linux (X11/Wayland) + macOS backends proven, sidecar/core chain per arch, **full AGENTS gate green per OS before any asset row**.

## Shape of the idea (settled in the brief)

1. **Per-OS earned rows** — an asset appears only with that OS's full gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`), mirroring T-04's arm64 discipline.
2. **Secrets backend decided** (Q33a): the sealed-container semantics (`dpapi1:` container shape) get per-OS keyring backends — **Windows = DPAPI**; linux = Secret Service; macOS = Keychain — via the `keyring` crate family or sharper per-OS crates after grilling. **Plaintext is banned on every OS**; the off-Windows loud-fail in `dpapi.rs` is replaced by real seal/unseal per backend, containtaining the same `dpapi1:`-like prefix discipline and zeroize.
3. **Windows-only dep gating in Cargo.toml**: current Windows-only surface (`tray-icon`, `winreg`, `raw-window-handle` usage patterns, `embed-resource`, `png` build script) needs `#[cfg(windows)]`-style target gating and Linux/macOS `tray-icon`/`libayatana` story per crate — audit happens **here**, feature-flag layout decided by grilling.
4. **gpui linux backends**: prove X11 and Wayland run the shell (`AppView` windowing, `test_support` harness, keybindings/menu wiring) — known eww: clipboard semantics, minimize-to-tray availability varies by DE, multi-monitor topbar assumptions.
5. **macOS backend**: gpui/macOS + tray behavior + `.app` bundling questions (translate the ZIP-only hard rule to a mac-appropriate artifact: `.app` bundle? still "a folder you can move" — grilling decides).
6. **Sidecar chain per arch**: pinned sing-box/awg/xray per linux amd64/arm64 + darwin arm64 (+ darwin amd64 if the fork publishes), with the release-chain layout as per T-03/T-04's bundles.
7. **Autostart story per OS**: today HKCU Run (opt-in) — linux XDG autostart file / macOS LaunchAgent (opt-in, same posture) decided in grilling.

## Open questions for the grilling session (never assume)

- **Keyring crate selection** + failure semantics (unlock prompts, backend absence on headless linux — fallback: refuse loud, never plaintext-echo choices).
- **gpui target support reality** — which gpui-version artifacts exist for linux (X11 and Wayland 分别成熟度), macOS; whether the current gpui-kit 0.6 surface needs an upgrade pass to build per-OS.
- **Tray story**: windows `tray-icon` fine; linux needs system tray under libayatana (system-lib dependency — document as bundle requirement or false-fallback); mac tray is NSStatusItem via a crate — where does that dependency land and how is it gated?
- **Setting surface parity**: registry appearance read (`AppsUseLightTheme`) obviously Windows-only — appearance/source-of-truth per OS (XDG portal? mac defaults?) — grilling decides; do not assume.
- **Autostart**: HKCU Run only exists on Windows — opt-in XDG autostart / LaunchAgent equivalents, and whether the feature ships per-OS at all.
- **DPAPI-equivalent tests** (`dpapi.rs` suite) — what hermetic per-OS keyring tests look like (CI runners with reachable keyrings? test-support backends?).
- CI: which runners (`ubuntu-latest`, `macos-latest`/`macos-14` arm) give **true** full-gate green and whether live proofs gate per-OS (probably amd64-only like today).
- Packaging shape per OS: linux → tarball (ZIP hard rule analogies — a folder you move), macOS → `.app`/dmg? — the "self-contained folder" rule mapped honestly rather than forced.
- FFIdSurface shares the Win32 FFI surface in the app (`ShowWindow`/user32 uses, T-30's planned `GetProcessTimes`/psapi) — per-OS process-metric implementations (procfs/sysctl) enumerated, or feature-flag the charts per OS?
- Sequencing vs fork-core: the port epic's sidecar chain depends on whether `ManyProxyCore` 1-exe retires some per-OS sidecar packing — order these two epics' adjacency consciously.

## Non-goals recorded now

- The Windows posture does not change (single binary crate, no workspace, single file UI monolith).
- Neither OS gets system proxy/TUN/DNS-rewrite/elevation surfaces (permanently out of posture).
- No ".Write once, run anywhere" pretense: per-OS earned rows only.

## Sequencing

Ordered **AFTER** the borrow tickets; runs in parallel-adjacency with the fork-core epic (`EPIC-fork-core.md` — the core's cross-arch artifacts are this epic's engine input for per-arch sidecar rows).
