# Contract: bulk select + facets

- Set: `checked: HashSet<config-id>`; keyed by id never index; pruned in `set_rows`; hidden members retained with `N selected (M visible)` label.
- Bar: conditional row under toolbar when non-empty: `N selected` + Start / Stop / Test / Delete / Clear. Start reuses `start_all_partial_message` + `is_row_local_start_error` split (row-local per-row, union-level atomic). Delete names all affected rows, reuses detach-then-delete confirm. Clear empties set; Esc clears set only with no dialog open.
- Facets: `group_filter: Option<String>` (None = All) + `protocol_filter: Option<String>` + `status_filter` (All | Running | Stopped, Enabled/Disabled optional) ANDed with text `query` in `apply_filter`; `visible` pipeline unchanged. Controls: `Select` single, `Combobox multiple` multi-value, `Checkbox` booleans, `TabBar` segmented status.
- Keyboard: new app actions Delete + Ctrl-F via `actions!`+`bind_app_keys` + APP_KEY_CONTEXT + dialog guard; Enter never rebound (consume `ListEvent::Confirm`); anchor (`selected`) stays single for detail/Enter.
- Search: one lowercase blob per row built in `set_rows` (name|protocol|group|bind|port|server|sub-label|preset-name|state words); no per-keystroke JSON, no per-row `to_lowercase` at filter time.
