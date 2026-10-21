# Production polish — console, labels, density, settings

Goal: the portable ZIP behaves like a shipped product on a clean Windows machine, and the
main screen works for the real job — 20–100 local SOCKS proxies, one per antidetect profile.

Status: planning. Evidence sources: live UIA screenshots of build `66e33ff`, code map of
`src/main.rs`, production-readiness audit, bulk-workflow audit.

## Confirmed defects (root-caused)

| # | Defect | Evidence | Severity |
|---|--------|----------|----------|
| 1 | Release exe links console subsystem (PE 3) → black console window; its X sends `CTRL_CLOSE_EVENT` and kills app + sidecars | no `#![windows_subsystem]` in `src/main.rs`; `.cargo/config.toml` absent; `Cargo.toml` no release profile; shipped exe PE `Subsystem=3` | blocker |
| 2 | Sidecars spawned with no `CREATE_NO_WINDOW` | `src/engine.rs:5745-5751`, `4626-4631`, `5722`, `8019-8026`, `14000-14042` | blocker |
| 3 | Rows render ~104–120px tall for one line of text | `List` measures one item at MinContent (`list.rs:434`), `flex_wrap` at `src/main.rs:1979-1984` stacks cells; uniform height applied by `cache.rs:216`; `List::new(...).flex_1()` at `28893` has no `max_height` | blocker |
| 4 | Keyboard shortcuts baked into visible labels and accessible names | `src/main.rs:2136-2146`, `25672-25711`, `24763-24798`, `26419`, `26692`, `26715`, `28044-28825` | high |
| 5 | Column headers do not align with their values; Health value clipped/occluded | headers `26530-26582` vs row cells `1919-2119` | high |
| 6 | Two different controls both labelled "Groups"; no active state on nav | `28033-28425`, toolbar button at `26758` region | high |
| 7 | Empty detail pane: 384px-capped `Empty` block inside a ~1000px band, 240px tall | `28908-28919`, kit `empty.rs:142/365` | high |
| 8 | Settings page unstructured / reads as a mess | pending surface inspection | high |
| 9 | No panic hook, no crash file | zero `set_hook` in `src/` | high |
| 10 | Unwritable folder silently runs in memory after one fading toast | `src/main.rs:13640-13653` | high |
| 11 | `APP_VERSION` stays `0.0.1` for every release; no VERSIONINFO in `manyproxy.rc` | `src/main.rs:229`, `Cargo.toml:3`, `manyproxy.rc:1` | medium |
| 12 | Auto-port never probes the socket, only DB uniqueness | `src/store.rs:8301-8317` | medium |

## Decisions taken

- Close button: configurable in Settings — `Close to tray` vs `Quit the app`.
- Shortcuts: never in visible labels. Tooltips and Help → Shortcuts only. Update DESIGN.md
  (its current "Kbd chips beside labels" rule is superseded).
- Detail layout: `TBD — awaiting prototype choice`.

## Steps

1. [ ] Console subsystem + sidecar `CREATE_NO_WINDOW`; verify PE subsystem = 2 on a fresh
       release build and that no console appears on launch.
2. [ ] Shortcut purge across every label/aria/tooltip source; keep keys working; add or
       reuse a single Shortcuts reference surface. Update the tests that pin `(F5)` strings.
3. [ ] Row height fix: single-line, fixed-height, non-wrapping row grid; header/cell share
       one column spec; verify ~20 rows visible at 1032px.
4. [ ] Settings page restructure (per surface report).
5. [ ] Detail pane rebuild per chosen prototype; copy-proxy affordance one click per row.
6. [ ] Production hardening: panic hook + crash file, persistent not-writable banner, version
       stamping, VERSIONINFO, LICENSE/README in the bundle.
7. [ ] Verifier pass on the whole diff; then `cargo fmt --check`, `clippy -D warnings`,
       `cargo test --locked`; commit in logical slices.

## Acceptance checks

- Launching `manyproxy.exe` shows exactly one window and zero consoles.
- Killing the app from Task Manager leaves no orphan sidecars.
- Every visible button label is a verb or noun without a key hint; hover shows the key.
- 20+ rows visible at 1920×1032 with headers aligned to their values.
- Settings: one screen, named sections, no orphan controls.
- No `unwrap()`/`expect()` on the startup path; a forced panic writes a crash file.
- Release ZIP contains VERSIONINFO-stamped exe, README, LICENSE, SHA256SUMS.
