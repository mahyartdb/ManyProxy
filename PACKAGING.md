# ManyProxy — Release Packaging (ticket 20)

Portable-first Windows release layout. Extends `PACKAGING-NOTES.md`
(release gate, ticket 09) — nothing here contradicts it. Every binary
name, path, and search-order claim below is verified against `src/`
(file:line refs); no step was invented.

## 1. Version + binary-name source (read-only)

- Package name and version come from `Cargo.toml` (READ ONLY, owned by
  feature work): `name = "manyproxy"`, `version = "0.1.0"`
  (`Cargo.toml:1-4`). Verified via `cargo metadata --no-deps`
  (reports `manyproxy 0.1.0`; no build was run).
- The release exe is therefore `manyproxy.exe` on Windows
  (cargo names the exe after the package; no `[[bin]]` rename exists
  in `Cargo.toml`).
- No in-app version string is derived from the package version:
  `src/` contains zero uses of `CARGO_PKG_VERSION` / `env!("CARGO...")`
  (grep-verified). So the version stamp for a release is the
  `Cargo.toml [package] version` value plus the git commit hash —
  record both in the release notes by hand.

## 2. Portable layout

```
ManyProxy-<version>\
  manyproxy.exe        <- target\release\manyproxy.exe
  sing-box.exe         <- stock sidecar (required for standard rows)
  sing-box-awg.exe     <- fork sidecar (only if shipping exotic AWG/Noise/XHTTP support)
  xray.exe             <- Xray sidecar (only if shipping Xray-only VLESS support)
  <user-exe>           <- NOT shipped: the single extra-core slot is user-supplied
  README / hashes      <- release notes + SHA-256 sums (see section 5)
```

- Sidecars must sit **alongside the exe in the app directory**
  (never system paths, never downloaded). "App directory" is the
  directory containing the running exe (`std::env::current_exe()`
  parent — `src/engine.rs:2412-2414`, same pattern at `:2488-2490`,
  `:2577-2579`, `:2623-2625`).
- Either portable zip or installer shape works as long as sidecars
  ship next to the exe (`PACKAGING-NOTES.md` installer-vs-portable
  rule, unchanged).

## 3. Sidecar matrix (exact names)

Windows file names append `.exe` (`*_binary_file_name` fns);
non-Windows builds use the bare base name.

| Slot       | Base (`src/engine.rs`)                                                                   | Windows file                | Search order                                                                                                          | Needed by                                                                                                         |
| ---------- | ---------------------------------------------------------------------------------------- | --------------------------- | --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| stock      | `ENGINE_BINARY_BASE = "sing-box"` (`:52`) via `engine_binary_file_name()` (`:2400-2406`) | `sing-box.exe`              | app dir only (ticket 23: PATH fallback removed)                                                                                          | standard rows                                                                                                     |
| fork       | `FORK_BINARY_BASE = "sing-box-awg"` (`:57`) via `fork_binary_file_name()` (`:2449-2455`) | `sing-box-awg.exe`          | app dir only (ticket 23: PATH fallback removed); stock paths never consult it                                      | AmneziaWG/Noise/XHTTP rows only (missing binary names rows + binary + searched dirs, `MissingForkBinary`, `:352`) |
| xray       | `XRAY_BINARY_BASE = "xray"` (`:67`) via `xray_binary_file_name()` (`:2538-2544`)         | `xray.exe`                  | app dir only (ticket 23: PATH fallback removed); stock/fork paths never consult it                                 | Xray-only VLESS rows only (`resolve_xray_binary`, `:2576-2588`)                                                   |
| extra-core | none (user-supplied)                                                                     | user-chosen plain file name | **app dir ONLY, never `PATH`** (`resolve_extra_binary_in`, `:2590-2617`); single slot, unprivileged spawn             | one active extra-core row; exe name comes from the row's `exe` param (`:895-923`)                                 |

Extra-core `exe` rules (packaging-relevant because the slot is
user-filled, not shipped): plain file name only — empty, `.`, `..`,
any `/`, `\`, `:`, or absolute path is rejected without touching the
filesystem (`src/engine.rs:2596-2607`); a missing file errors naming
the app directory searched (`:2611-2616`). Tell users: drop their
core exe next to `manyproxy.exe` and reference its file name.

Fork smoke check: `<binary> version`, first stdout line
(`probe_fork_version`, `src/engine.rs:2506-2535`). A wrong/plain
binary fails fast here instead of serving exotic rows badly.

## 4. Data paths (NOT in the portable dir)

- Library DB default: `%APPDATA%\ManyProxy\library.db` on Windows,
  `$HOME/.manyproxy/library.db` elsewhere; `MANYPROXY_DB` overrides
  (`Store::default_path`, `src/store.rs:726-737`).
- Export default: `manyproxy-export.json` sibling of the library file;
  `MANYPROXY_EXPORT` overrides (`Store::default_export_path`,
  `src/store.rs:1348-1361`). Export writes only after explicit
  confirm; cancel writes nothing.
- Consequence for packaging: uninstall is delete-app-folder plus
  optionally the data file above — no registry keys, services,
  drivers, or system proxy/DNS/route writes exist to roll back
  (per `PACKAGING-NOTES.md`, unchanged).

## 5. Release checklist (clean checkout → runnable portable dir)

1. `git clone` fresh; confirm `cargo metadata --no-deps` reports the
   intended `manyproxy <version>` (matches `Cargo.toml`).
2. `cargo build --release` (full release build NOT run for this doc;
   first real release should record wall-time + rustc version).
3. Create `ManyProxy-<version>\`; copy `target\release\manyproxy.exe`
   into it.
4. Place sidecars next to the exe per the section-3 matrix (stock
   always; fork/xray only the ones the release supports). Do NOT
   place anything in system paths; do NOT wire downloads into the app
   (prohibited: ticket 20 + app never fetches executables,
   `MissingBinary` contract text, `src/engine.rs:335-337`).
5. Smoke test on a clean machine/VM: run with zero sidecars first —
   each missing binary must surface a typed error naming the rows +
   expected file + every directory searched (never a silent fallback
   or download). Then add sidecars and start one row per shipped core.
6. Publish SHA-256 hashes for `manyproxy.exe` + each shipped sidecar
   (unsigned-binary AV/SmartScreen expectations per
   `PACKAGING-NOTES.md`: sign when a cert exists, never pack/obfuscate).
7. Record `Cargo.toml` version + git hash in the release notes (see
   section 1: no in-app version string exists to do this automatically).

## 6. Non-goals (reaffirmed, not changed)

No auto-update replacing binaries, no telemetry, no admin-by-default,
no TUN/SUID helpers — see `PACKAGING-NOTES.md` (ticket 09). This doc
adds layout + checklist only: no `src/`, `Cargo.toml`, or `Cargo.lock`
changes.
