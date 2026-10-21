use crate::*;

impl AppView {
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
        for menu in FacetMenu::ALL {
            let (choices, current) = menu.choices(self.libraries.read(cx).delegate());
            let picker = match menu {
                FacetMenu::Group => &mut self.facet_group,
                FacetMenu::Protocol => &mut self.facet_protocol,
                FacetMenu::Status => &mut self.facet_status,
            };
            Self::sync_facet_picker(picker, choices, current, window, cx);
        }
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
        let has_facets = self.libraries.read(cx).delegate().facets_active();
        // T-19: the facet area carries the same per-group opt-in switch for
        // the focused group; has no group to protect by design.
        let facet_group_now = self.libraries.read(cx).delegate().facet_group.clone();
        let facet_auto_clear = facet_group_now
            .as_deref()
            .map(|group| self.store.auto_clear_enabled(group))
            .unwrap_or(false);
        let facet_auto_view = cx.entity();

        let (group_tip, protocol_tip, status_tip) = {
            let libraries = self.libraries.read(cx);
            let delegate = libraries.delegate();
            let (visible, total, _) = delegate.delegate_counts_for_search();
            (
                facet_tip_text(
                    "group",
                    &FacetMenu::Group.current_display(delegate),
                    visible,
                    total,
                ),
                facet_tip_text(
                    "protocol",
                    &FacetMenu::Protocol.current_display(delegate),
                    visible,
                    total,
                ),
                facet_tip_text(
                    "status",
                    &FacetMenu::Status.current_display(delegate),
                    visible,
                    total,
                ),
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
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .id("library-search")
                    .test_support()
                    .flex_1()
                    .min_w(px(140.))
                    .tooltip(|_, cx| cx.new(|_| Tooltip::new(SEARCH_HINT)).into())
                    .child(
                        Input::new(&self.search_input)
                            .small()
                            .aria_label("Search configurations (Ctrl-F)"),
                    ),
            )
            .child(self.render_search_count(cx))
            .child(
                div()
                    .id("facet-group-tip")
                    .test_support()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(group_tip.clone())).into())
                    .child(
                        Select::new(&self.facet_group.state)
                            .id("facet-group")
                            .small()
                            .accessibility_label("Filter by group"),
                    ),
            )
            .child(
                div()
                    .id("facet-protocol-tip")
                    .test_support()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(protocol_tip.clone())).into())
                    .child(
                        Select::new(&self.facet_protocol.state)
                            .id("facet-protocol")
                            .small()
                            .accessibility_label("Filter by protocol"),
                    ),
            )
            .child(
                div()
                    .id("facet-status-tip")
                    .test_support()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(status_tip.clone())).into())
                    .child(
                        Select::new(&self.facet_status.state)
                            .id("facet-status")
                            .small()
                            .accessibility_label("Filter by status"),
                    ),
            )
            .child(
                Button::new("groups-dialog")
                    .small()
                    .label("Groups")
                    .tooltip("Rename or merge groups")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_groups_dialog(window, cx);
                    })),
            )
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
                    .tooltip("Clear group, protocol and status filters (keeps search and checked)")
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
    pub(crate) fn render_library_column_headers(&self, cx: &mut Context<Self>) -> AnyElement {
        let columns = self.row_columns;
        let sort = self.libraries.read(cx).delegate().sort;
        let has_rows = !self.libraries.read(cx).delegate().rows.is_empty();
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
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            // The gutter the row cards spend on the serving lamp and checkbox.
            .child(div().w(px(28.)).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Name"),
            );
        if columns.upstream {
            strip = strip.child(column_header_label("Upstream", 180., cx));
        }
        if columns.health {
            strip = strip.child(sort_header_cell(
                "health",
                "Health",
                150.,
                sort,
                view.clone(),
                cx,
            ));
        }
        if columns.group {
            strip = strip.child(column_header_label("Group", 100., cx));
        }
        if columns.traffic {
            strip = strip.child(sort_header_cell("traffic", "Traffic", 120., sort, view, cx));
        }
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
            let label = if current {
                format!("{} ✓", key.label())
            } else {
                key.label().to_string()
            };
            let tip = if current {
                format!("{} — current sort", key.label())
            } else {
                format!("Sort by {}", key.label())
            };
            menu = menu.item(Self::sort_menu_item(
                column,
                key.token(),
                label,
                tip,
                SortMenuAction::Key(*key),
                view,
            ));
        }
        let next = sort.direction.toggled();
        menu.item(Self::sort_menu_item(
            column,
            "direction",
            format!("Direction: {} → {}", sort.direction.label(), next.label()),
            "Reverse the active key (Type | Security keeps its fixed order)".to_string(),
            SortMenuAction::Direction,
            view,
        ))
    }
}
