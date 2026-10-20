# T-02 — Storage relocation to exe-relative `<appdir>\data\`

- **id**: T-02
- **title**: Storage relocation to exe-relative `<appdir>\data\` (library DB, export, logs, backups, quarantine; cascade tray "Open config" and Settings paths)
- **epic**: E1 — Release pipeline & portable storage
- **size**: M
- **depends-on**: T-01
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E1 (portability rule 1 — "engines and data live beside the exe"); Q28/Q30; gap §2 persistence row ("%APPDATA%\ManyProxy\library.db default" as-built) + P2; AGENTS.md portability rule (landed by T-01).

## Why

The portable-folder model (Q28) requires the data to live **beside the exe**, so a user can move, back up, or delete the folder as a unit and move a drive between machines. Today the default store sits in `%APPDATA%\ManyProxy\library.db` — outside the folder, invisible on a moved install, and a migration trap. Because this is a beta with zero released users, the window to relocate defaults with **no migration path** is before the first release (Q30). The relocation must stay a *default-path change only* — `MANYPROXY_DB` / `MANYPROXY_EXPORT` / `MANYPROXY_LOGS` env overrides remain exactly as‑is.

## What

1. **Shared app-data helper** — add one `pub(crate) fn app_data_dir() -> PathBuf` (name TBD in review; e.g. `data_dir`) that returns `<current_app_dir?>\data\`:
   - engine.rs already has `fn current_app_dir() -> Option<PathBuf>` (engine.rs:4340, private) — either make it `pub(crate)` or move the logic into store.rs; do not duplicate resolution logic in two modules. Its contract is fixed: app-dir only, never `PATH`, never defaults that depend on the current working directory.
   - Fallback when the app dir cannot be resolved (edge cases in dev builds): fall back to the *existing* `MANYPROXY_DB`-less behavior is **forbidden** — fall back to a relative `data\` beside the cwd exe in dev only, and keep the loud `dpapi.rs`-style posture: never `%APPDATA%`, never `~/.manyproxy`.
2. **`Store::default_path()` rewrite** (store.rs:3497): `MANYPROXY_DB` override first (unchanged), then `<app_data_dir()>\library.db`. Delete the `%APPDATA%\ManyProxy\library.db` branch and the dangling `~/.manyproxy/library.db` non-Windows branch entirely (no-migration rule 2: never read, never copy).
3. **Create-on-default**: when opening with defaults, create the parent `data\` directory (recursive) before SQLite open. Env-override paths keep today's behavior (caller-owned).
4. **Derived defaults cascade automatically** — verified in code, these already derive from `default_path().parent()`, so they land inside `data\` with no extra change; assert it in tests:
   - `Store::default_export_path()` (store.rs:5764) → `data\manyproxy-export.json`.
   - `Store::default_logs_path()` (store.rs:5779) → inside `data\` (visible-logs save target; `MANYPROXY_LOGS` override intact).
5. **Backups + quarantine beside DB** (verified: `quarantine_corrupt_file` at store.rs:3540 names `.corrupt-<ts>`, restore backup `.bak-<ts>` lives next to the DB by construction) — no change needed beyond (2); pin by tests that they land in the same *new* parent.
6. **Tray "Open config folder" cascade** — verified: `tray.rs:58 config_folder()` returns `Store::default_path().parent()`; `main.rs:13243 copy_config_folder` copies that string to the clipboard. Nothing to change; add a test asserting the copied path is `<...\data>` under defaults. (Tray menu wording stays "Open config folder"; it copies the path — do not add shell-open in this ticket.)
7. **Settings "Paths" section** — verified rendering shows "data folder + export default with Copy buttons" from the same derived paths (main.rs open_settings_dialog / paths section); nothing to change; covered by the render tests below.
8. **Docs line** — AGENTS.md (from T-01) already claims this model; no doc edit needed unless wording drifts.

## Files to touch

- `src/store.rs` — `default_path()` (3497), new `app_data_dir()`-style helper, create-parent-on-open, tests
- `src/engine.rs` — expose `current_app_dir()` (`pub(crate)`) if the helper lives in store.rs
- `src/main.rs` — no behavior change expected; test coverage for tray copy + Settings paths section render
- `src/tray.rs` — none (derives); test coverage only

## Acceptance criteria

- [ ] Fresh run with no env overrides and an empty folder puts `library.db`, `manyproxy-export.json`, saved-logs target, `.bak-<ts>` backups, and `.corrupt-<ts>` quarantine all inside `<appdir>\data\`.
- [ ] No code path reads or copies `%APPDATA%\ManyProxy` or `~/.manyproxy` — a **guard test** greps the crate source *strings* for those defaults and fails if any default-path constructor references `%APPDATA%` or `.manyproxy` (name it `t52_no_default_paths_reference_appdata`; the grep target is the string literals in `src/`, so it runs hermetically).
- [ ] `MANYPROXY_DB` / `MANYPROXY_EXPORT` / `MANYPROXY_LOGS` overrides behave byte-identically to today (existing tests must pass unchanged).
- [ ] Tray-open copies and Settings Paths section show `<appdir>\data`.
- [ ] Existing DBs (dev machines) are NOT migrated: opening with defaults simply starts a new library in the new location (worth one explicit test: `t52_fresh_default_store_at_exe_relative_data` — open twice, row survives in-memory round trip via export/restore, no `MANYPROXY_DB` involved).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`); all tests hermetic — TEMP dirs only, isolated `MANYPROXY_DB`, no sidecar spawn.
- [ ] New tests (repo style, non-colliding prefix `t52_` — verified unused): `t52_default_path_is_exe_relative_data`, `t52_export_and_logs_defaults_inside_data`, `t52_backup_and_quarantine_beside_db`, `t52_tray_config_folder_cascades`, `t52_no_default_paths_reference_appdata`, `t52_fresh_default_store_at_exe_relative_data`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: this is the implementing ticket — defaults land in `<appdir>\data\`; guard test keeps it true.
- [x] No-migration: legacy branches **deleted**, never read/copied; no schema migration added (this is a path change, not a schema change).
- [x] Plaintext-always-sealed: `dpapi.rs` untouched; DB file ACL hardening (icacls current-user) applies to the new location unchanged.
- [x] Hijack non-concept: N/A (no preset surfaces here).
- [x] No system mutation: none added — border on create-directory, not registry/services/TUN; assert_no_system_mutation family untouched.
- [x] Clipboard `bind:port` discipline: the tray copy now copies a *folder path* (not a credential); unchanged discipline elsewhere.
- [x] Redaction: untouched.
- [x] Sidecars app-dir only: resolution logic shared from the same `current_app_dir` contract; no new search paths.

## Out of scope

- Migrating any existing store (banned forever — Q30).
- Changing the export payload format (T-22) or log ring behavior (T-31).
- Any UI for picking the data folder (env overrides already exist; a picker would be new surface — not asked).
- Arm64/release packaging (T-03/T-04).
