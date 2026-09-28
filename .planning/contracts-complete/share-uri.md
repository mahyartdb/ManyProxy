# Contract: share-URI + QR

- Serializer: `config → share-URI` is the exact mirror of `src/import.rs` parsers (per-protocol emit, same key sets incl. WG `reserved/fm`, hy2/TUIC extras validated against 1.14.2; never emit 1.15-only fields).
- Round-trip rule: every emitted URI must re-import via existing intake to an equivalent row (id excluded from equality); round-trip test per protocol.
- Copy: detail `Copy share link` button sits next to `copy-urls` (which stays bind+port only); secret-warning copy reuses `export_warning_text` verbatim.
- QR: `qrcode 0.14.1` encoder → `image` PNG bytes or SVG string → GPUI image element (element name verified at build); QR is pure incremental UI on the serializer (no QR without serializer first).
- Errors: unsupported-for-share types fail loudly with guidance; never emit a URI that import rejects.
