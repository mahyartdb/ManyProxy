# Data model: gpui-18-sweep (UI-only — no storage changes)

No SQLite schema changes. No new tables/columns. No engine/health/import/tray logic changes. This sweep touches presentation + wording in `src/main.rs` only.

## UI state (existing, regrouped — not new entities)

| State | Owner | Shape | Rule |
| ----- | ----- | ----- | ---- |
| Selection | view | `Option<RowId>` (single) | Wash on one row only; header echoes `Acting on: <name>` |
| Checked set | view | `Vec<RowId>` ordered | Rim + numeral chip; bulk bar `N checked (of M visible)` |
| Facet filter | view + `SelectState` ×3 | group/proto/status + query | Clears affect only their target (query vs facets vs checked) |
| Inspector tab | view | Details/Health/Presets index | Frozen pane order Header→Endpoints→tab-specific; scroll reset on switch |
| In-flight | view | `start_in_flight`, `probe_kind_in_flight` | Label+spinner, ignore double-activation; state flags, never sleeps |
| Log filter/level | dialog/sheet state | query + All/Debug/Info/Warn/Error (`RadioGroup` index) | Single grouped choice; filter commit vs discard distinct verbs |
| Overlay stack | `Window` + `Root` layers | dialogs (commits) + sheets (reference) + menus + notifications | Escape closes topmost, focus returns to trigger |

Validation: existing headless seams (`assert_painted` ids, pinned-string fns) + one `RadioGroup` helper + one virtualized-logs helper if unavoidable. Nothing per-row start tracking (forbidden).
