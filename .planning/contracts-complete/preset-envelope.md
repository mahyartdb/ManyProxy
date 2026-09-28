# Contract: preset envelope

- Encodings: `throne://route/` accepts standard-b64, urlsafe-b64, and raw JSON; `kind = throne-route-profile`, `v = 1` checked when present; legacy bare array opens the editor (unchanged).
- Shape: `{"name": str, "rules": [sing-box route-rule objects]}`; matcher validation stays shape-only (delegated to `sing-box check`); action intake accepts `route | direct | block | reject | proxy` where `proxy` ≡ `route` alias and `block` serves as `reject`.
- `warp`/`warp-bypass`: explicit guidance error (no endpoint to point at — needs design, not silent drop).
- `remoteroute`: decode stays SSRF-guarded; fetch is explicit-only with fetch-time re-check (decode-time check alone is insufficient); no auto-update interval; BOM-prefixed JSON parses (strip BOM like import).
- Storage: `route_presets` additive columns only; attach re-validates current rules in-tx; orphan sweep banner for dangling `preset_id`.
