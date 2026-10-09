# Quickstart: validate gpui-18-sweep end-to-end

Prereqs: Windows 10/11 64-bit, stable Rust, `cargo` available, repo root `e:\Code\ManyProxy`. No admin needed (loopback defaults). Isolated demo DB for live proofs (never live DB).

## 1. Headless (per ticket, required)

```powershell
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Expect: all green; `assert_painted` ids in `contracts/ui.md` paint; pinned-string asserts match; `assert_no_system_mutation`-family green; hardcoded-color grep stays 0:
`rg -n "hsla!|rgba\(|#[0-9a-fA-F]{6}" src/ --glob '*.rs'` → only non-paint hits (icon bytes, hash constants, test URIs).

## 2. Keyboard walkthrough (human, <5min at 880x620)

1. `Ctrl-N` → guided Add (hint visible, Advanced collapsed) → fill Server/Port/Identity → Port budget reads `N of 499` → commit `Add`.
2. Select row → Endpoints + Copy + Start visible without h-scroll; operate row ≤2 lines; `F5` starts (label+spinner, double-press ignored).
3. `Ctrl-T` tests, `Ctrl-U` copies (paste → bind:port only, no credentials).
4. `Ctrl-1/2/3` switches Details/Health/Presets with `Kbd` hints visible; order Header→Endpoints→tab-specific stable.
5. `Ctrl-K` resolves every toolbar + menu destination through same handlers.
6. Empty library / no-matches / no-selection / no-log-lines each show `Empty` title+description+working buttons in one traversal.
7. Logs/Subscriptions/Groups open as sheets (library behind), Escape closes topmost, focus returns to trigger.

## 3. Live capture (post-implementation, opt-in)

Debug build + isolated `MANYPROXY_DB` demo seed (`*.example.com` rows, no secrets) + capture at 1000x700: library, add, import, subscriptions. Replace `assets/screenshots/*.png` only if surfaces visibly changed. Human judges layout; model judges wording vs CAP checklist.
