# T-06 — Update-check client (JSON-only, never downloads/executes)

- **id**: T-06
- **title**: Update-check client — release-feed fetch, semver compare, Tools ▸ Check for updates + About button
- **epic**: E2 — Update-check
- **size**: M
- **depends-on**: T-05 (the feed only exists after the first public prerelease)
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E2 decisions verbatim (Q15 supersedes the parked posture — build now; Q12/`Q15 build release-check now`); gap §5 item 6 soft-allowed borrow; throne `no-checksum updater` anti-pattern NOT copied (gap §6 item 5 — ManyProxy executes nothing).

## Why

"Is there a new release?" is the only Throne-power utility ManyProxy is missing on the distribution side — but Throne's binary-updater path (download + execute, no checksum, §J.3) is dead-on-arrival here. The settled shape is a **JSON-only fetch** of the public releases feed, a semver comparison against the running version, and a dialog pointing the user at the releases page in their own browser. ManyProxy never downloads, hashes, or executes a release asset — the user does, on their own judgment.

## What

1. **Feed fetch** — new async fn `fetch_release_feed(client: Arc<dyn HttpClient>, url: &str) -> Result<FetchedRelease, String>` in `main.rs`, next to the verified pattern `fetch_subscription_body` (main.rs:9260): same `cx.http_client()` usage, same `HyperClient`-agnostic `http_client::http::Request::builder()`, 10 s timeout, ≤5 redirects (`RedirectPolicy::NoFollow` + manual follow like the subscription path), ≤1 MiB body cap.
   - **SSRF/feed policy**: run the URL through `store::subscription_fetch_policy(url)` + `store::subscription_fetch_policy_resolved` (store.rs:149/176; resolver `store::subscription_url_host` + `resolve_subscription_addresses` like the subscription path) — https-only Allow, plaintext http warns, internal hosts blocked. Default feed constant: `https://api.github.com/repos/QMahyar/ManyProxy/releases/latest`.
   - Headers: whatever T-07 establishes (if T-07 landed) or none beyond defaults; NO `x-hwid`-family header ever (Q-ratified; capability must not ship).
2. **Parse + compare** — serde_json parse of the latest-release payload (fields `tag_name`, `prerelease`, `html_url`, `name`):
   - tag normalization: strip leading `v`, compare `major.minor.patch` numerically against `APP_VERSION` (0.0.1 after T-05; `main.rs:184 env!("CARGO_PKG_VERSION")`).
   - **Prerelease rule (brief)**: `releases/latest` never returns prereleases, so with a stable payload in flight, compare against it; while `APP_VERSION` itself carries a prerelease suffix (future), the client also surfaces prerelease tags — implement as: if `APP_VERSION` contains `-`, request the **releases list** endpoint and pick the newest tag per the allow-prerelease rule; otherwise use `releases/latest`. Today (0.0.1, no suffix) the compare target set is stable-only.
   - `404` (both the no-releases and the still-private states) renders exactly **"No releases published yet."** — not an error toast class.
   - Malformed JSON/side failures render as a plain error notification with the network-free detail string (no raw body dumps).
3. **Result surfaces** — a small dialog (gpui-kit Dialog):
   - Up to date: "ManyProxy <APP_VERSION> is up to date."
   - Newer: "New version available: <tag> (running <APP_VERSION>)" + source button **"Open releases in browser"**.
   - Browser opening: hand-rolled `ShellExecuteW` FFI helper in main.rs consistent with the crate's minimal-FFI style (the crate already carries user-level Win32 calls; no new crate, no `cmd /c start`, no `explorer.exe` spawn of the *app's* making beyond the standard shell verb). The helper opens the URL the *user* accepted — user-initiated, no silent navigation, no URL prefetch.
4. **Placement** — Tools menu "Check for updates" (via `topbar_menu_row` pattern in `render_toolbar`, main.rs:23318, the menu the "Stop Testing"/"Resolve domain group" items use around main.rs:24062–24098) **and** a "Check for updates" button in the About dialog (About region ~14212). No startup auto-check, no background polling, no persistent banner (explicit click only; "no auto-update" statement in About remains true — checking ≠ updating).
5. **Setting override** — Settings gains one text field `update_feed_url` (meta key via `Store::set_meta` family, store.rs `META_*` pattern at store.rs:6777–6982), default = the constant above; validate with the same fetch policy on save (https-only, internal-host blocked; invalid → field error via the existing save-note pattern).

## Files to touch

- `src/main.rs` — `fetch_release_feed`, compare fn, dialog, Tools menu item (insert near "Resolve domain group"/"Stop Testing" block, ~24060–24098), About button, Settings field, FFI opener helper, tests
- `src/store.rs` — `META_UPDATE_FEED_URL` const + typed getter/setter mirroring `META_RESTORE_RUNNING` (store.rs:6777/6801)
- nothing in engine/import/health/tray

## Acceptance criteria

- [ ] Tools ▸ Check for updates + About button both drive the same single fetch; results as specified; "Open releases in browser" opens the user's browser to the fetched `html_url` only on click.
- [ ] 404 → "No releases published yet."; malformed feed → plain error notification; both hermetic-test assertions.
- [ ] Semver compare matches: same `0.0.1` == up-to-date; `0.0.2` / `v0.0.2` newer; older/equal ignored; prerelease-only feed with stable running version = up-to-date.
- [ ] Feed URL policy: internal host → blocked before any network; http → warn line; override round-trips through Settings.
- [ ] **No binary ever downloaded/executed by the app** — the only network artifact is the JSON body; assert in the header-capturing fake client test that the request targets the feed URL only.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New hermetic tests (prefix `t56_`, verified unused): `t56_feed_fetch_policy_blocks_internal_hosts`, `t56_semver_compare_matrix`, `t56_404_renders_no_releases_message`, `t56_prerelease_visibility_follows_app_channel`, `t56_dialog_reports_new_version_with_browser_link`, `t56_settings_feed_url_round_trip_and_validation`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A (new meta key is additive; old installs simply default).
- [x] Plaintext-always-sealed: N/A (no credentials on the feed path; `fld`s untouched).
- [x] Hijack non-concept: N/A.
- [x] No system mutation: the fetch is explicit, JSON-only, no execution, no download — user-initiated click; browser opening is user-approved navigation of their own app. PWB rules do not apply (no sidecars fetched here).
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: no tokens in the feed; still route failure strings through plain constants (no raw echoes of URL/query).
- [x] Sidecars app-dir only: N/A.

## Out of scope

- Downloading/installing releases in-app (permanently dead — Q15 says JSON only, never download/execute).
- Checksum verification flows (ManyProxy executes nothing; nothing to verify).
- Auto-run at startup, background cadence, or a tray badge.
- Beta-channel toggle (PARKED in INDEX).
