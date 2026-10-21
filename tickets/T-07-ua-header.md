# T-07 — UA header `ManyProxy/<version>` (no HWID, ever)

- **id**: T-07
- **title**: UA header on subscription refreshes, remote-preset fetches, release feed — `User-Agent: ManyProxy/<version>`
- **epic**: E2 — Update-check
- **size**: S
- **depends-on**: T-05 (ordering), land **after T-06** (shares and slightly extends the fetch builder both use)
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E2 (T-07 item + explicit NO `x-hwid`/`x-device-*` — Q-ratified); gap §2 subscription row "no UA sent" [C `refresh_with_body_running` + grep 0 UA]; §4.4 S2 (adopt UA, refuse HWID) + §6 item on device-identification headers.

## Why

Two independent reasons, one ticket: (a) subscription server compatibility — providers throttle or reject UA-less clients, and Throne sends `Throne/<version>` (§D.1); (b) honesty — a precise, static, per-release identifier "ManyProxy/0.0.1" tells an operator what is asking, with zero device identity. The explicit negative decision matters as much: Throne's `x-hwid`/`x-device-*` header family is opt-in device fingerprinting; ManyProxy **shall not ship the capability** (headers, config, settings, help text — nothing).

## What

1. Add a constant near the fetch helpers: `pub(crate) const USER_AGENT: ... -> String` or a `fn user_agent_header() -> String` returning `format!("ManyProxy/{}", APP_VERSION)` (`APP_VERSION` at main.rs:184).
2. Single insertion point (verified): the request builder inside `async fn fetch_subscription_body` (main.rs:9260–9288) gains `.header("user-agent", user_agent_header())`. This covers **all three** call sites because they share this fn:
   - subscription refreshes — `start_subscription_refresh` (main.rs:17895, `cx.http_client()` → `fetch_subscription_body`);
   - remote-preset fetches — `start_preset_refresh` (main.rs:18994, same helper);
   - the release-feed fetch (T-06's `fetch_release_feed`, same pattern) — if T-06 landed, give it the same header via the shared helper; also work out `fetch_release_feed` to call the helper (or, if T-06 has not merged, this ticket touches T-06's branch state after rebase; same fn).
3. Redirects: the manual No-Follow loop issues each redirect hop as a new request — attach the header on **each hop's** builder (the loop at main.rs:9270–9302).
4. **Negative space pinned**: a fake-client capture test asserts the outgoing request headers contain exactly `user-agent: ManyProxy/<version>` and contain **no** `x-hwid`, `x-device-os`, `x-device-id`, or any header key starting `x-device` (any case). Future feature gates cannot reintroduce them silently.

## Files to touch

- `src/main.rs` — constant + header line in `fetch_subscription_body` (+ future-safe use in T-06's fetch fn if present) + capture tests

## Acceptance criteria

- [ ] Every outgoing HTTP request from the app's fetch layer carries `user-agent: ManyProxy/<APP_VERSION>`.
- [ ] No device-identifier header exists anywhere in the request path (capture test named below pins it).
- [ ] Existing subscription/preset fetch tests pass unchanged (header must not alter policy outcomes; timeouts/redirects/8 MiB cap untouched — `SUBSCRIPTION_MAX_BYTES` store.rs:170 etc.).
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New hermetic tests (prefix `t57_`, verified unused): `t57_user_agent_header_on_subscription_fetch`, `t57_user_agent_header_on_remote_preset_fetch` (capture via the same fake client pattern), `t57_no_device_identifier_headers_anywhere` (positive match on ua + negative match on hwid family).

## Constraints checklist (hard rules)

- [x] Portability / no-AppData: N/A.
- [x] No-migration: N/A.
- [x] Plaintext-always-sealed: N/A — and the UA carries no secrets.
- [x] Hijack non-concept: N/A.
- [x] No system mutation: N/A (header-only change; fetch policy untouched). PWB rules do not apply.
- [x] Clipboard `bind:port` discipline: N/A.
- [x] Redaction: the UA is a version constant — no user data inside it; nothing else leaks into headers (negative tests above).
- [x] Sidecars app-dir only: N/A.

## Out of scope

- Any config-form or Settings surface for the UA (it is not a setting; it is a fixed honest constant).
- Any optional device-identification surface (banned capability — would be a new ticket category, not an option).
- Changing fetch policy constants (8 MiB cap, 20 s timeout, redirect count — untouched).
