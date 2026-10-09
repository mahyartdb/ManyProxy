# Research: gpui-18-sweep (Phase 0 — all NEEDS CLARIFICATION resolved)

Owning skills: gpui-kit + gpui-kit-design-guides (0.6.6 verified). No DB/network/schema unknowns — UI-only sweep, presentation + wording, no engine/store/tray logic.

## Decision log

### R1 — Status primitives: `Marker` for lamp, `Badge`/`Tag` for chips (adopt)
Rationale: `marker.rs` `Marker::new()` + `with_variant(MarkerVariant)` + `loading()` fits occupancy lamp with accessible name/tooltip; `badge.rs` `Badge::new().dot()/.count()` fits numeral check chip; `tag.rs` `Tag::new()` + variants fits protocol/group (already used). Alternatives: hand-rolled `div.rounded_full` (current — rejected: no a11y, 12px hit floor manual, duplicates kit). Version: gpui-component 0.6.6.

### R2 — Detail containers: `GroupBox` + `DescriptionList` (adopt)
Rationale: `description_list.rs` `DescriptionList::new()/vertical()/horizontal()` + `DescriptionItem::new(label).value()` + `label_width/bordered/columns` fits Endpoints/Usage/Health/History/Preset label-value rows; `GroupBox::new().title().gap_1/2` already used for 3/5 modules — extend to all 5 for one rhythm. Alternatives: bare `div.text_sm` (current Usage/Preset — rejected: rhythm break). Version: 0.6.6.

### R3 — Forms: `v_form`/`h_form` + `field()` + `footer()` (adopt)
Rationale: `form/{mod,form,field}.rs` `v_form()->Form`, `field()->Field`, `Form::child(Field).footer()` + `Field` `ParentElement::child(input)` (no `Field::child` named method — use trait) fits Add/Edit/Import/Subscriptions/Groups. Alternatives: `dialog_field()` div wrapper (current — rejected: gap/padding drift). Version: 0.6.6.

### R4 — Exclusive choices: `RadioGroup` (adopt)
Rationale: `radio.rs` `RadioGroup::new(id)/vertical/horizontal` + `selected_index(Option<usize>)` + `on_change(&usize)` + `child(Radio)` fits interval (always/1h/6h/12h/24h/7d) and log-level (All/Debug/Info/Warn/Error) as single grouped choices. Alternatives: standalone `Radio` row + duplicate cycle button (current logs — rejected: not a group, keyboard nav broken). Version: 0.6.6.

### R5 — Copies: `Clipboard` button (adopt)
Rationale: `clipboard.rs` `Clipboard::new(id).value().tooltip().on_copied()` + `Sizable` fits per-row + Copy-URLs copies (bind:port only, toast confirm). Alternatives: `Button.small(Copy)` (current — works but reinvents copy affordance). Keep honest-toast + redaction. Version: 0.6.6.

### R6 — Explainer: `HoverCard` for lamp/endpoint notes (adopt sparingly)
Rationale: `hover_card.rs` `HoverCard::new(id).trigger().content().anchor().open_delay/close_delay` fits 2-line `health: never tested — press Test` explainer where tooltip is too thin. Use sparingly (lamp + 1 endpoint note max); tooltips elsewhere. Version: 0.6.6.

### R7 — Overlays: `Sheet` via `WindowExt::open_sheet` for reference flows (adopt)
Rationale: `sheet.rs` `Sheet::new(&mut Window,&mut App).title().footer().size().resizable().overlay_closable()` + `window_ext.rs` `open_sheet/open_sheet_at/has_active_sheet/close_sheet` fits Logs/Subscriptions/Groups (reference, library stays visible behind). **No `Sheet::open` exists — must use `WindowExt`.** True commits stay `Dialog`. Menus stay `DropdownMenu` + `PopupMenuItem::element(div role=MenuItem)` (no id field on enum — verified `menu/popup_menu.rs:33-65`). Version: 0.6.6.

### R8 — Commands: `ButtonGroup` + `DropdownButton` for operate/bulk (adopt)
Rationale: `button_group.rs` `ButtonGroup::new(id).child(Button).layout().compact().outline().on_click(&Vec<usize>)` fits Start/Stop/Test grouping; `dropdown_button.rs` `DropdownButton::new(id).button(Button).dropdown_menu(fn)` fits `Row actions…` trigger. Bulk bar adds `Separator::vertical` grouping, destructive trailing. Alternatives: 8 loose `.small()` buttons (current — rejected: wraps to 2-3 lines, no grouping). Version: 0.6.6.

### R9 — Loading: `Button.loading()` + `Spinner` + `Progress`/`Skeleton` compose (adopt)
Rationale: `Button.loading(bool)` + `spinner.rs` `Spinner::new()` + `Sizable` already used for start/test/ping (keep + extend to all in-flight); `progress/` `Progress::new(id).value().loading()` fits bounded work; `skeleton.rs` `Skeleton::new()` + `shimmer.rs` `ShimmerText::new(text)` compose for streaming (no `Skeleton::shimmer` — compose per API). Alternatives: label-only `Starting…` (rejected: no distinct loading treatment). Version: 0.6.6.

### R10 — Lists: `List` stays library, `VirtualList` for logs (adopt)
Rationale: `list/` `ListState::new(delegate).searchable/selectable/set_query/focus` stays library (ids stable, no churn); `virtual_list.rs` (base) `v_virtual_list/h_virtual_list` + `track_scroll/with_sizing_behavior` fits 10k-line log tails. Library stays `List` (dense-table spike stays gone). Version: gpui-base 0.6.6.

### R11 — Status bar: `StatusBar` center via `children()` (adopt pattern)
Rationale: `status_bar.rs` `StatusBar::new().left().right()` repeatable + `impl ParentElement` (`child/children` → center region). **No `.center()` method — use `children()` for center.** Keeps `min_w` floors + `max_w` truncate + tooltips, no layout shift. Version: 0.6.6.

### R12 — Tabs/search/facets: keep legal workarounds, add hints (adopt)
Rationale: `select.rs` has zero `tooltip` hits — keep outer `div#facet-*-tip.tooltip()` wrapper (legal, documented). `tab.rs` `Tab::ix` is `pub(crate)` — keep wrapper `div#inspector-tabs` + zero-size markers (legal, documented). Add `Kbd` hints for Ctrl-1/2/3 on tabs, freeze pane order Header→Endpoints→tab-specific, keep scroll-reset. `kbd.rs` `Kbd::new(Keystroke)` + `Tooltip::key_binding` for hints. Version: 0.6.6.

### R13 — Settings panel: `Settings`/`SettingPage`/`SettingGroup` (adopt)
Rationale: `setting/` `Settings::new(id).page()` + `SettingPage::new(title).group()` + `SettingGroup::new().title().item()` + `SettingItem::new(title,field)` + `SettingField::switch/input/dropdown/...` fits Settings dialog sections (Startup/Probe/Paths/Maintenance). Alternatives: grouped divs + `Separator::horizontal` (current — rejected: inconsistent). Version: 0.6.6.

### R14 — Empty/feedback: `Empty` + `Alert` (already partly used — extend)
Rationale: `empty.rs` `Empty::new().header(EmptyHeader).content(EmptyContent)` + `alert.rs` `Alert::error(id,msg).title()` fits all empties + dead-health + export warning (already used for 2/5 — extend to all). Version: 0.6.6.

## Alternatives considered (rejected)
- `Sidebar` for library/detail: rejected — current `v_resizable` top/bottom split fits Operate mental model; sidebar would reframe product without mandate.
- `DataTable`/`Table` for library: rejected — card list is the locked view (PRODUCT.md dense-table spike gone); table would reintroduce density + id churn.
- `Popover` replacing all facet `Select`s: deferred — `Select` already shipped (T03); popover only where `Select` genuinely misfits (none found in sweep).
- Custom colors/radius/type: rejected — DESIGN.md locks tokens; zero hardcoded colors must hold.

## NEEDS CLARIFICATION — none remain
All component unknowns resolved by registry reads above. No schema/API/deploy/auth/perf unknowns (UI-only, no new deps, no new contracts).
