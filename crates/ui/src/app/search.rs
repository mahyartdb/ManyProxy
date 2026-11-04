use crate::app::columns::WidthAction;
use crate::*;

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
                format!("{text} — current sort")
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

    pub(crate) fn render_facet_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sync_facet_pickers(window, cx);
        let right_pad = 8.;
        let has_facets = self.libraries.read(cx).delegate().facets_active();
        // T-19: the facet area carries the same per-group opt-in switch for
        // the focused group; has no group to protect by design.
        let facet_group_now = self.libraries.read(cx).delegate().facet_group.clone();
        let facet_auto_clear = facet_group_now
            .as_deref()
            .map(|group| self.store.auto_clear_enabled(group))
            .unwrap_or(false);
        let facet_auto_view = cx.entity();

        let group_tip = {
            let libraries = self.libraries.read(cx);
            let delegate = libraries.delegate();
            let (visible, total, _) = delegate.delegate_counts_for_search();
            facet_tip_text(
                "group or status",
                &FacetMenu::Group.current_display(delegate),
                visible,
                total,
            )
        };
        div()
            .id("facet-bar")
            .test_support()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .pl_2()
            .pr(px(right_pad))
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            // T-23 / F-32: filters lead the row. A `flex_1` search used to
            // claim the whole width and shove Protocol/Status under the
            // inspector drawer at 880–1000px. The search keeps the remaining
            // stretch but stops at 320px, so the drawer can only ever cover
            // its right edge — never the filter values.
            .child(
                div()
                    .id("facet-group-tip")
                    .test_support()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(group_tip.clone())).into())
                    .child(
                        Select::new(&self.facet_group.state)
                            .id("facet-group")
                            .small()
                            .accessibility_label("Filter by group or status"),
                    ),
            )
            .child(
                div()
                    .id("library-search")
                    .test_support()
                    .flex_1()
                    .min_w(px(140.))
                    .max_w(px(320.))
                    .tooltip(|_, cx| cx.new(|_| Tooltip::new(SEARCH_HINT)).into())
                    .child(
                        Input::new(&self.search_input)
                            .small()
                            .aria_label("Search configurations"),
                    ),
            )
            .child(self.render_search_count(cx))
            .child(match facet_group_now {
                Some(group) => {
                    let slug = slugify_group_id(&group);
                    div()
                        .id(format!("facet-auto-clear-{slug}"))
                        .test_support()
                        .child(
                            Switch::new(format!("facet-auto-clear-switch-{slug}"))
                                .checked(facet_auto_clear)
                                .label("Auto-clear unavailable")
                                .tooltip(
                                    "Opt-in per group: after this group's URL test ends, its \
                                     unavailable rows are removed automatically. Running rows \
                                     and the row being edited are never removed; enabling asks \
                                     its own confirmation.",
                                )
                                .accessibility_label(format!(
                                    "Auto-clear unavailable rows in group \"{group}\" after group tests"
                                ))
                                .on_change({
                                    let facet_auto_view = facet_auto_view.clone();
                                    move |next: &bool, window, cx| {
                                        let view = facet_auto_view.clone();
                                        if *next {
                                            view.update(cx, |view, cx| {
                                                view.open_auto_clear_opt_in_confirm(
                                                    &group, false, window, cx,
                                                )
                                            });
                                        } else {
                                            view.update(cx, |view, cx| {
                                                view.commit_auto_clear_off(&group, window, cx)
                                            });
                                        }
                                    }
                                }),
                        )
                        .into_any_element()
                }
                None => div()
                    .id("facet-auto-clear-idle")
                    .test_support()
                    .into_any_element(),
            })
            .child(if has_facets {
                Button::new("clear-facets")
                    .small()
                    .label("Clear facet filters")
                    .tooltip("Clear the group and status filters (keeps search and checked)")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.clear_facets(window, cx);
                    }))
                    .into_any_element()
            } else {
                div().id("facets-idle").test_support().into_any_element()
            })
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
                    .child("Name"),
            );
        if metrics.show_group {
            strip = strip.child(column_header_label(
                LibraryColumn::Group,
                metrics.group,
                view.clone(),
                cx,
            ));
        }
        if metrics.show_protocol {
            strip = strip.child(column_header_label(
                LibraryColumn::Protocol,
                metrics.protocol,
                view.clone(),
                cx,
            ));
        }
        if metrics.show_upstream {
            strip = strip.child(column_header_label(
                LibraryColumn::Upstream,
                metrics.upstream,
                view.clone(),
                cx,
            ));
        }
        strip = strip.child(column_header_label(
            LibraryColumn::Endpoint,
            metrics.endpoint,
            view.clone(),
            cx,
        ));
        if metrics.show_health {
            strip = strip.child(sort_header_cell(
                LibraryColumn::Health,
                sort,
                metrics.health,
                view.clone(),
                window,
                cx,
            ));
        }
        if metrics.show_traffic {
            strip = strip.child(sort_header_cell(
                LibraryColumn::Traffic,
                sort,
                metrics.traffic,
                view.clone(),
                window,
                cx,
            ));
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
            menu = menu.item(Self::sort_menu_item(
                column,
                key.token(),
                key.label().to_string(),
                format!("Sort by {}", key.label()),
                current,
                SortMenuAction::Key(*key),
                view,
            ));
        }
        let next = sort.direction.toggled();
        menu = menu.item(Self::sort_menu_item(
            column,
            "direction",
            format!("Direction: {} → {}", sort.direction.label(), next.label()),
            "Reverse the active key (Type | Security keeps its fixed order)".to_string(),
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
        let aria = format!("{} {} column", label, column.label());
        let view = view.clone();
        PopupMenuItem::element(move |_, _| {
            div()
                .id(id.clone())
                .test_support()
                .role(Role::MenuItem)
                .aria_label(aria.clone())
                .child(label)
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
                    if event.button == MouseButton::Left && view.read(cx).column_resize.is_some() {
                        view.update(cx, |view, cx| view.finish_column_resize(window, cx));
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
