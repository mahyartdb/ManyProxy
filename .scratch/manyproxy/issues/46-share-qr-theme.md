# 46: Share-URI serializer + QR + theme spike

**What to build:** Safe sharing: config→share-URI serializer mirroring the import grammars with round-trip tests and secret warnings, Copy-share-link button, QR render on top of the serializer, and a theme-capability spike with a toggle only if gpui-kit exposes tokens.

**Blocked by:** 42-protocols-serve-all (serializer mirrors its grammars; rebase as emission lands).

**Owns:** backend-patterns

**Status:** ready-for-agent

- [ ] Scenario: row of each served protocol → Action: Copy share link then re-import → Expected: round-trips to an equivalent row; warning copy matches export warning text. Verification: automated test. Priority: required.
- [ ] Scenario: share URI present → Action: open QR → Expected: scannable code renders (PNG via image bridge or SVG string); unsupported-for-share types fail loudly with guidance. Verification: manual UX review (phone scan). Priority: important.
- [ ] Scenario: theme spike → Action: probe gpui-kit 0.6 tokens → Expected: settings toggle ships with badge/health/log contrast audit if supported, else docs record unsupported. Verification: manual UX review. Priority: optional.
- [ ] Prohibited side effect: `copy-urls` stays bind+port only; no QR without the serializer first.
