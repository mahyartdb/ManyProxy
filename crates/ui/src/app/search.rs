use crate::app::columns::WidthAction;
use crate::*;

/// The filter slot's height. It always occupies its row, so the table below
/// it never moves.
pub(crate) const FILTER_SLOT_HEIGHT: f32 = 36.;

impl AppView {
    /// T-23: keep the last-filter memory in step with the query. `set_value`
    /// never emits `InputEvent::Change`, so the clear paths call this directly.
    /// A blank query clears the key rather than storing one, and a write hiccup
    /// only costs the memory — the live filter is unaffected.
    pub(crate) fn remember_library_query(&mut self, query: &str) {
        let _ = if query.trim().is_empty() {
            self.store.clear_library_last_query()
        } else {
            self.store.set_library_last_query(query)
        };
    }

    pub(crate) fn sort_menu_item(
        column: &'static str,
        id_suffix: &'static str,
        label: String,
        tip: String,
        checked: bool,
        action: SortMenuAction,
        view: &Entity<AppView>,
    ) -> PopupMenuItem {
        let id = format!("sort-header-{column}-{id_suffix}");
        let view = view.clone();
        PopupMenuItem::element(move |_, _| {
            let text = label.clone();
            let tip = tip.clone();
            // The check mark is the menu's own state, not a glyph in the label:
            // the visual row gets the check slot and screen readers hear the
            // same fact as words.
            let aria = if checked {
                t!("sort-current-aria", label = &text)
            } else {
                text.clone()
            };
            div()
                .id(id.clone())
                .test_support()
                .role(Role::MenuItem)
                .aria_label(aria)
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                .child(text)
        })
        .checked(checked)
        .on_click(move |_, window, cx| {
            view.update(cx, |view, cx| match action {
                SortMenuAction::Key(key) => view.set_library_sort(key, window, cx),
                SortMenuAction::Direction => view.toggle_library_sort_direction(window, cx),
            });
        })
    }

    pub(crate) fn focus_library_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.focus_handle(cx).focus(window, cx);
    }

    pub(crate) fn sync_search_box(
        view: &Entity<Self>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        view.update(cx, |view, cx| {
            view.remember_library_query(text);
            view.search_input.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx);
            });
        });
    }

    /// The retired facet band's clear action. Slice 3's filter row owns it.
    #[allow(dead_code)]
    pub(crate) fn clear_library_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_library_query("");
        self.search_input.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });
        self.libraries.update(cx, |state, list_cx| {
            state.set_query("", window, list_cx);
        });
        cx.notify();
    }

    pub(crate) fn sync_facet_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // K11/A2-1: one facet menu; the enabled/disabled axis rides inside it.
        let (choices, current) = FacetMenu::Group.choices(self.libraries.read(cx).delegate());
        Self::sync_facet_picker(&mut self.facet_group, choices, current, window, cx);
        let (choices, current) = FacetMenu::Latency.choices(self.libraries.read(cx).delegate());
        Self::sync_facet_picker(&mut self.facet_latency, choices, current, window, cx);
    }

    /// T-05: one writer for the latency facet, shared by the picker and the
    /// unprobed switch. The live filter is the important part; a failed meta
    /// write only costs the remembered choice, so it does not interrupt the
    /// user (the remembered query takes the same trade).
    pub(crate) fn set_latency_facet(&mut self, facet: LatencyFacet, cx: &mut Context<Self>) {
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_latency_facet(facet);
            list_cx.notify();
        });
        let _ = self.store.set_latency_facet(facet);
        cx.notify();
    }

    pub(crate) fn sync_facet_picker(
        picker: &mut FacetPicker,
        choices: Vec<FacetChoice>,
        current: Option<String>,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) {
        if picker.synced != choices {
            picker.synced = choices.clone();
            picker.state.update(cx, |state, cx| {
                state.set_items(FacetChoiceList(choices.clone()), window, cx);
                state.set_selected_value(&current, window, cx);
            });
            return;
        }
        let selected = picker.state.read(cx).selected_value().cloned().flatten();
        if selected != current {
            picker.state.update(cx, |state, cx| {
                state.set_selected_value(&current, window, cx);
            });
        }
    }

    /// The filter slot above the table. It always occupies its row, so the
    /// table never moves when the filter row grows into it. Slice 1 keeps the
    /// existing search input here; slice 3 replaces the slot with the full
    /// filter row (Search · Status · Protocol · Filter).
    pub(crate) fn render_filter_slot(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("filter-slot")
            .test_support()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_2()
            .w_full()
            .h(px(FILTER_SLOT_HEIGHT))
            .flex_shrink_0()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            // T-23 / F-32: the search keeps the remaining stretch but stops at
            // 320px, so the inspector drawer can only ever cover its right
            // edge — never the field itself.
            .child(
                div()
                    .id("library-search")
                    .test_support()
                    .flex_1()
                    .min_w(px(140.))
                    .max_w(px(320.))
                    .tooltip(|_, cx| cx.new(|_| Tooltip::new(search_hint())).into())
                    .child(
                        Input::new(&self.search_input)
                            .small()
                            .aria_label(t!("search-placeholder")),
                    ),
            )
            .into_any_element()
    }

    /// T-27: the library list's column header strip. The visible columns come
    /// from `library_columns`; the health and traffic cells are sortable and
    /// carry the picker menu, the rest are labels that keep the strip lined up.
    ///
    /// T-03: the strip reads the same `RowMetrics` the row reads, so every
    /// label sits at its column's first character instead of at a second,
    /// disagreeing set of widths.
    pub(crate) fn render_library_column_headers(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let libraries = self.libraries.read(cx);
        let delegate = libraries.delegate();
        let metrics = delegate.row_metrics;
        let sort = delegate.sort;
        let has_rows = !delegate.rows.is_empty();
        let view = cx.entity();
        let column_drop = self.column_drop;
        if !has_rows {
            // An empty library keeps today's empty-state clean: a header strip
            // over nothing is noise.
            return div()
                .id("library-column-headers")
                .test_support()
                .into_any_element();
        }
        let mut strip = div()
            .id("library-column-headers")
            .test_support()
            .relative()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_2()
            // A drag move outside any movable header must clear the indicator;
            // parent listeners register before child listeners, so the headers
            // below can put it back when the pointer is over one of them.
            .on_drag_move(
                cx.listener(move |view, _: &DragMoveEvent<ColumnDrag>, _, cx| {
                    view.clear_column_drop(cx);
                }),
            )
            // The row's content starts after the list item's 4px inset and
            // the card's 8px padding — the strip starts at the same place.
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            // The gutter the row cards spend on the serving lamp and checkbox.
            .child(
                div()
                    .id("header-lamp-check")
                    .test_support()
                    .w(px(metrics.lamp_check))
                    .flex_shrink_0(),
            )
            // The numeral check chip's reserved slot.
            .child(
                div()
                    .id("header-check-slot")
                    .test_support()
                    .w(px(metrics.check_slot))
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .id("header-name")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("column-name")),
            );
        for column in *metrics.layout.order() {
            if !metrics.column_visible(column) {
                continue;
            }
            let drop_side =
                column_drop.and_then(|(target, side)| (target == column).then_some(side));
            let width = metrics.column_width(column);
            strip = strip.child(match column {
                LibraryColumn::Group
                | LibraryColumn::Protocol
                | LibraryColumn::Upstream
                | LibraryColumn::Endpoint => {
                    column_header_label(column, width, drop_side, view.clone(), cx)
                }
                LibraryColumn::Health | LibraryColumn::Traffic => {
                    sort_header_cell(column, sort, width, drop_side, view.clone(), window, cx)
                }
            });
        }
        // The row's trailing cell holds the copy action; the strip keeps the
        // same width so the columns stay lined up.
        strip = strip.child(
            div()
                .id("header-copy")
                .test_support()
                .w(px(metrics.copy))
                .flex_shrink_0(),
        );
        strip = strip.child(column_drag_canvas(view));
        strip.into_any_element()
    }

    /// T-27: the header picker — the key list for this column plus the direction
    /// toggle, with the live choice marked. Item ids are pinned
    /// (`sort-header-{column}-{key|direction}`) for the UI test.
    pub(crate) fn sort_header_menu(
        menu: PopupMenu,
        column: &'static str,
        sort: LibrarySort,
        view: &Entity<AppView>,
    ) -> PopupMenu {
        let mut menu = menu;
        for key in sort_header_keys(column) {
            let current = *key == sort.key;
            let label = library_sort_key_label(*key);
            menu = menu.item(Self::sort_menu_item(
                column,
                key.token(),
                label.clone(),
                t!("sort-by", label = label),
                current,
                SortMenuAction::Key(*key),
                view,
            ));
        }
        let next = sort.direction.toggled();
        menu = menu.item(Self::sort_menu_item(
            column,
            "direction",
            t!(
                "sort-direction",
                from = sort_direction_label(sort.direction),
                to = sort_direction_label(next)
            ),
            t!("sort-direction-tip"),
            false,
            SortMenuAction::Direction,
            view,
        ));
        match LibraryColumn::from_id(column) {
            Some(column) => Self::width_menu_items(menu.separator(), column, view),
            None => menu,
        }
    }

    /// T-03: the same three verbs every header exposes, minus the sort half for
    /// the columns that have no sort key.
    pub(crate) fn width_header_menu(
        menu: PopupMenu,
        column: LibraryColumn,
        view: &Entity<AppView>,
    ) -> PopupMenu {
        Self::width_menu_items(menu, column, view)
    }

    fn width_menu_items(
        mut menu: PopupMenu,
        column: LibraryColumn,
        view: &Entity<AppView>,
    ) -> PopupMenu {
        for action in WidthAction::ALL {
            menu = menu.item(Self::width_menu_item(column, action, view));
        }
        menu
    }

    fn width_menu_item(
        column: LibraryColumn,
        action: WidthAction,
        view: &Entity<AppView>,
    ) -> PopupMenuItem {
        let id = format!("column-menu-{}-{}", column.id(), action.id_suffix());
        let label = action.label();
        let aria = t!(
            "column-width-aria",
            action = label.clone(),
            column = library_column_label(column)
        );
        let view = view.clone();
        PopupMenuItem::element(move |_, _| {
            div()
                .id(id.clone())
                .test_support()
                .role(Role::MenuItem)
                .aria_label(aria.clone())
                .child(label.clone())
        })
        .on_click(move |_, window, cx| {
            action.apply(&view, column, window, cx);
        })
    }
}

/// T-03: window-level pointer tracking for a width drag. A header cell's own
/// `on_mouse_move` stops at the cell edge, but the user may drag higher or
/// lower than the 30px strip; the window listener follows until mouse-up. It
/// has to be registered during paint, hence the zero-visual canvas.
fn column_drag_canvas(view: Entity<AppView>) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_mouse_event({
                let view = view.clone();
                move |event: &MouseMoveEvent, _phase, _window, cx| {
                    if view.read(cx).column_resize.is_some() {
                        view.update(cx, |view, cx| {
                            view.drag_column_resize(event.position.x.as_f32(), cx)
                        });
                    }
                }
            });
            window.on_mouse_event({
                let view = view.clone();
                move |event: &MouseUpEvent, _phase, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    if view.read(cx).column_resize.is_some() {
                        view.update(cx, |view, cx| view.finish_column_resize(window, cx));
                    }
                    // A drag released outside a header has no on_drop to clear
                    // its indicator; defer so a real drop still sees the
                    // indicator state during its own dispatch.
                    if view.read(cx).column_drop.is_some() {
                        let view = view.clone();
                        window.defer(cx, move |_, cx| {
                            view.update(cx, |view, cx| view.clear_column_drop(cx));
                        });
                    }
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h_full()
    .into_any_element()
}
