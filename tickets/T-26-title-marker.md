# T-26 — Window title marker `· N running`

- **id**: T-26
- **title**: Window title gains `· N running` (running-endpoint count) — tray tooltip parity for free
- **epic**: E6 — UI affordance pack
- **size**: S
- **depends-on**: T-05
- **branch**: tracking branch `throne-borrow` → per-ticket PR (Q19b)
- **provenance**: brief E6 T-26 line; gap §4.3 U11 (Throne appends `[Admin][Tun][System Proxy]` + running profile §B.2; "ManyProxy has static title; append `· N running` (+ current preset when one is attached) … too noisy — count only"); verified: static `APP_TITLE` at main.rs:168, `app_window_options` sets `titlebar.title = Some(APP_TITLE.into())` (main.rs:170–181), custom TitleBar label at main.rs:24231; tray already does the count tooltip (`tray.rs:348 set_running` — "ManyProxy · N running").
## Why

The count is the one "system state" ManyProxy honestly has; surfacing `· N running` in the title lets an alt-tab glance or an OBS capture read the farm's liveness without opening the window. Throne's markers exist for Admin/Tun/System-Proxy states that ManyProxy deliberately never enters (OOS) — count-only is the settled adopt.

## What

1. Title composition: `APP_TITLE` when zero running; `format!("{APP_TITLE} · {n} running")` with n > 0 — updated on the same cadence the app already learns running counts from (`poll_shards`-driven state in main.rs, and the tray `set_running(n)` call site — single shared computation, one formatting helper used by both surfaces).
2. Both surfaces **stay in sync** (title + tray tooltip `set_running`): one helper `fn running_title_suffix(n: usize) -> String` returning "" for 0, ` · N running` else; consumed by the titlebar update and `tray.rs set_running`'s current formatting.
3. Preset-attached marker was considered and **dropped** by the brief (too noisy); nothing else appended (no `[Admin]`-style tokens ever — the states do not exist here).
4. Titlebar label update path: gpui titlebar is set via `window_options` at open plus the TitleBar label (main.rs:24231 `TitleBar::new().child(Label::new(APP_TITLE))`); implement via the available title update mechanism in gpui-kit 0.6 (implementer verifies the exact API — `window.set_title`/titlebar label update requirement): if gpui-kit lacks live re-title, **fall back to updating only the custom TitleBar Label child** and record that in the PR (the OS window title remains the static APP_TITLE then; tray tooltip still updates). No blocked state: either path satisfies the acceptance minimum; the OS-title sync is a stretch note, not a requirement.

## Files to touch

- `src/main.rs` — helper + titlebar update wiring + tests
- `src/tray.rs` — formatting reuse only (no behavior change expected)

## Acceptance criteria

- [ ] `· N running` appears and follows the running count live across start/stop/all/quit drift; zero-running renders plain `ManyProxy`.
- [ ] Tray tooltip and (titlebar label / OS title where achievable) show the same number from one helper (no drift).
- [ ] No preset markers, no `[Admin]`-family tokens, no extra text.
- [ ] AGENTS gate green (`cargo fmt --check` → `cargo clippy --all-targets --locked -- -D warnings` → `cargo test --locked`).
- [ ] New tests (prefix `t76_`, verified unused): `t76_running_title_suffix_shapes`, `t76_titlebar_label_follows_running_count`, `t76_tray_tooltip_stays_in_sync`.

## Constraints checklist (hard rules)

- [x] Portability / no-AppData / No-migration / Plaintext / Hijack / Sidecars: N/A.
- [x] No system mutation: N/A — window text only.
- [x] Clipboard / Redaction: N/A (count only).

## Out of scope

- Marking libraries by preset/group names (dropped as noisy).
- Battery/eye-candy title animations.
- Anything Admin/TUN/System-Proxy-shaped (the states are permanently absent — OOS by design; never imitate them).
