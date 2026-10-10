# 08: Sheets, palette, status, loading feedback

**What to build:** Reference flows in `Sheet`s via `WindowExt`, full palette coverage, `StatusBar` center via `children()`, honest loading (`Button.loading` + `Spinner` + `Progress`/`Skeleton` compose + `Alert` consistency).

**Blocked by:** 07-logs-scale (reuses Empty/Spinner/RadioGroup + overlay language; sheets host the logs/subs/groups UIs).

**Owns:** gpui-kit composition.

**Status:** done

- [x] Scenario: open Logs/Subscriptions/Groups → Action: observe + Escape → Expected: each renders as `Sheet` via `WindowExt::open_sheet` (no `Sheet::open`), library visible behind, Escape closes topmost returning focus to trigger; true commits stay `Dialog`; menus keep `DropdownMenu` + `PopupMenuItem::element` pattern (no id invention). Verification: automated test (sheet open/Escape/focus-return where harness supports + `has_active_dialog`/`has_active_sheet` guards). Priority: required.
- [x] Scenario: press Ctrl-K → Action: query every toolbar + menu destination → Expected: palette resolves through same handlers as buttons/menus/keys (Library/Manage groups dispatched, not duplicated). Verification: automated test (palette op → same outcome as button/menu op). Priority: important.
- [x] Scenario: start/test/ping in flight + values changing → Action: watch buttons + status strip → Expected: buttons show label+spinner and ignore double-activation; bounded work offers `Progress`, streaming offers composed `Skeleton`/`ShimmerText` (no `Skeleton::shimmer`); status uses `StatusBar` `left()`/`right()` + center via `children()` (no `.center()`), `min_w` floors + `max_w` truncate+tooltip, no layout shift; dead rows render error `Alert` with inline operate row. Verification: automated test (in-flight flag asserts + status-segment ids + Alert presence). Priority: required.
- [x] Prohibited side effect: no `Sheet::open` / `StatusBar::center()` / `Skeleton::shimmer` inventions; no disabled-without-explanation; no fabricated latency.

Retry cap: 2, then escalate.
