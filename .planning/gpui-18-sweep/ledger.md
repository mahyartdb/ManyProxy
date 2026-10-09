# Ledger: gpui-18-sweep

## T01
Task 01: complete (tests: `cargo test --locked` → 807 passed, 0 failed; `cargo fmt --check` → clean; `cargo clippy --all-targets --locked -- -D warnings` → clean).
Ruling: kit `Marker` is a container row (not a dot) and `Badge` carries no id/tooltip/a11y in gpui-component 0.6.6 (read `marker.rs`/`badge.rs`), so the 12px flat-fill lamp geometry stays a `div` — only text equivalents added (`lamp_access_label` + `check_chip_tip` pure fns, tooltip + `aria_label` on both anchors). Pinned ids `row-dot-<id>` / `row-check-chip-<id>` unchanged; existing `assert_painted` asserts still green. Cost if wrong: lamp would lose test anchor + tooltip anchor; reversible by swapping anchor element.
New test: `tests::t01_status_text_equivalents` (tone-distinct labels, chip position-of-total).
