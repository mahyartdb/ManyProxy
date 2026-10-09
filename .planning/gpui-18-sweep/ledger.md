# Ledger: gpui-18-sweep

## T01
Task 01: complete (tests: `cargo test --locked` → 807 passed, 0 failed; `cargo fmt --check` → clean; `cargo clippy --all-targets --locked -- -D warnings` → clean).
Ruling: kit `Marker` is a container row (not a dot) and `Badge` carries no id/tooltip/a11y in gpui-component 0.6.6 (read `marker.rs`/`badge.rs`), so the 12px flat-fill lamp geometry stays a `div` — only text equivalents added (`lamp_access_label` + `check_chip_tip` pure fns, tooltip + `aria_label` on both anchors). Pinned ids `row-dot-<id>` / `row-check-chip-<id>` unchanged; existing `assert_painted` asserts still green. Cost if wrong: lamp would lose test anchor + tooltip anchor; reversible by swapping anchor element.
New test: `tests::t01_status_text_equivalents` (tone-distinct labels, chip position-of-total).

## T02
Task 02: complete (tests: `t02_detail_modules_share_container_grammar` + seams `detail`(6) `endpoint`(14) `copy`(11) `preset`(61) `usage`(1) `health`(57) `history`(2) all green; `cargo fmt --check` → clean; `cargo clippy --all-targets --locked -- -D warnings` → clean; hardcoded-color grep → 0).
Ruling: endpoints stay custom rows, not `DescriptionList` — the trailing per-row Copy buttons + pinned `endpoint-value-*` ids need a three-cell row the list grid cannot host at the 72px label width the 880px-minimum containment depends on (verified `description_list.rs:178-190` `.item(label, value, span)` has no trailing-cell slot); labels already ride `description_list_label_foreground` and values truncate with full-text tooltips. Cost if wrong: endpoint rows would lose per-row copy placement; reversible by re-adding the row builder.
Ruling: R5 kit `Clipboard` not adopted for per-row copies — verified `clipboard.rs:77-128` renders an icon-only ghost button with no built-in honest toast; the labeled small `Button`s already satisfy the copy contract (bind:port-only via `proxy_urls`/`upstream_host_port`, honest `Copied … URL` toasts, pinned `copy-socks/http/upstream` ids green under t06/t10). Cost if wrong: visible copy labels lost; reversible via `Clipboard::new(id).value_fn().on_copied()` swap.
Ruling: R6 `HoverCard` not used in these modules — plain tooltips carry the full endpoint text and no 2-line explainer is needed here; keeps the sweep's sparing budget for the lamp note. Cost if wrong: none functional; add `HoverCard` trigger/content later without id churn.
Ruling: no pinned id/string changed — `config-endpoints/health/usage/history`, `endpoint-value-*`, `copy-upstream/socks/http`, `preset-detach-inline`/`preset-detach-idle` all paint as before; no same-commit test updates needed beyond the new test.
New test: `tests::t02_detail_modules_share_container_grammar` (all 5 modules boxed per pane; buckets-vs-line seam; idle-detach marker).
