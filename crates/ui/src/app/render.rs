use crate::*;

/// F2: the one owner of the inspector's geometry. The column headers, the
/// rows, the facet bar and the drawer all read these two numbers, so the
/// table can never lay itself out under the drawer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InspectorLayout {
    pub(crate) drawer_w: f32,
    pub(crate) table_w: f32,
}

impl InspectorLayout {
    /// F19: a hidden inspector costs the table nothing in either presentation,
    /// so the list takes the whole viewport. A visible drawer is clamped so the
    /// table never drops under its floor.
    pub(crate) fn compute(viewport: f32, inspector_w: f32, docked: bool, visible: bool) -> Self {
        if !visible {
            return Self {
                drawer_w: 0.,
                table_w: viewport,
            };
        }
        if docked {
            return Self {
                drawer_w: 0.,
                table_w: (viewport - inspector_w).max(RowMetrics::TABLE_MIN),
            };
        }
        let drawer_w = inspector_w.min(viewport - RowMetrics::TABLE_MIN).max(0.);
        Self {
            drawer_w,
            table_w: viewport - drawer_w,
        }
    }
}

impl AppView {
    /// F18: the empty canvas names the two intake actions and calls the same
    /// AppView methods the toolbar buttons use.
    pub(crate) fn render_empty_ctas(app: Entity<Self>) -> impl IntoElement {
        let add_view = app.clone();
        let import_view = app;
        div()
            .id("empty-library-ctas")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Button::new("empty-add")
                    .small()
                    .primary()
                    .label(t!("action-add-config"))
                    .accessibility_label(t!("action-add-config-accessible"))
                    .tooltip(t!("action-add-config-tip"))
                    .on_click(move |_, window, cx| {
                        add_view.update(cx, |view, cx| view.open_add_config(window, cx));
                    }),
            )
            .child(
                Button::new("empty-import")
                    .small()
                    .label(t!("action-import"))
                    .accessibility_label(t!("action-import"))
                    .tooltip(t!("toolbar-import-tip"))
                    .on_click(move |_, window, cx| {
                        import_view.update(cx, |view, cx| view.open_import_config(window, cx));
                    }),
            )
    }

    /// T-03: the window width decides the inspector's shape. At or above the
    /// breakpoint the inspector docks to the right and takes its share; below
    /// it the inspector becomes a slide-over drawer beside the table.
    pub(crate) fn inspector_docked(&self, window: &Window) -> bool {
        window.viewport_size().width.as_f32() >= INSPECTOR_DRAWER_BREAKPOINT
    }

    /// T-03: the inspector's live width. The split's own state carries it, so
    /// the table's column budget follows a drag. The width is session-only.
    pub(crate) fn inspector_width(&self, cx: &App) -> f32 {
        let sizes = self.split_state.read(cx).sizes();
        sizes
            .get(1)
            .map(|size| size.as_f32())
            .filter(|width| *width >= INSPECTOR_WIDTH_MIN && *width <= INSPECTOR_WIDTH_MAX)
            .unwrap_or(INSPECTOR_WIDTH)
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Task 12: restore-from-maximize is the one frame where DWM may have
        // forgotten the corner style. Re-apply only on that transition.
        let maximized = window.is_maximized();
        if self.window_was_maximized && !maximized {
            crate::os::window::apply_corner_preference(window, self.store.window_corners());
        }
        self.window_was_maximized = maximized;

        let app_entity = cx.entity();
        // T-03/F2: one geometry spec, computed once per frame and handed to
        // the header strip, the rows and the list pane's drawer reserve. The
        // table lives beside the nav, so the budget it is measured against is
        // the column's width, not the window's.
        let show_detail = self.config_count(cx) != 0;
        let docked = self.inspector_docked(window);
        let inspector_width = self.inspector_width(cx);
        let inspector_visible = show_detail && self.inspector_visible;
        let nav_width = self.nav_width(window);
        let column_width = (window.viewport_size().width.as_f32() - nav_width).max(0.);
        let layout =
            InspectorLayout::compute(column_width, inspector_width, docked, inspector_visible);
        // R11: one layout resolved once per frame. Every visible column keeps
        // its width; the table scrolls when the sum is wider than the pane.
        let metrics = RowMetrics::for_table_width(layout.table_w, self.column_layout);
        let content_width = self.column_layout.content_width(layout.table_w);
        self.libraries.update(cx, |state, cx| {
            let list_focus = state.focus_handle(cx);
            let delegate = state.delegate_mut();
            if delegate.app_view.is_none() {
                delegate.app_view = Some(app_entity);
                cx.notify();
            }
            delegate.set_row_metrics(metrics);
            // T-06: the row cards paint the kit focus ring while the list holds
            // the keyboard; the handle lives on the ListState, the app owns the
            // cards, so the frame hands it across once.
            delegate.list_focus = Some(list_focus);
        });

        // T-26: the titlebar label carries the same `· N running` text the OS
        // title gets, both composed from the one shared helper.
        let current_title = window_title_text(self.engine.running_count());
        let nav = self.render_nav(window, cx);
        let action_bar = self.render_action_bar(cx);
        let filter_slot = self.render_filter_slot(window, cx);
        let engine_bar = self.render_engine_bar(cx);
        let column_headers = self.render_library_column_headers(window, cx);
        // R11: the header and the rows live in one horizontally scrolled
        // column, so they can never drift apart and a narrow pane scrolls
        // instead of hiding a column.
        let table = div()
            .id("library-table")
            .test_support()
            .w(px(content_width))
            .min_w(px(content_width))
            .h_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(column_headers)
            // The blank area below the rows belongs to the table, not to a
            // row: a plain click there clears the selection. Rows stop their
            // own clicks so selecting one never reaches this.
            .child(
                div()
                    .id("library-scroll")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .on_click(cx.listener(|view, _, window, cx| {
                        if view.selection_count() > 0 {
                            view.clear_selection(window, cx);
                        }
                    }))
                    .child(List::new(&self.libraries).size_full()),
            );
        let mut list_pane = div()
            .id("config-list")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            // The bar above the table carries labels that cannot shrink; the
            // pane must not grow to fit them, or the table would slide out of
            // the window at a large text size.
            .min_w_0()
            .child(action_bar)
            .child(filter_slot)
            .children(self.render_writable_banner(cx))
            .child(
                div()
                    .id("library-hscroll")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .track_scroll(&self.table_scroll)
                    .overflow_x_scroll()
                    .horizontal_scrollbar(&self.table_scroll)
                    .child(table),
            );
        if !docked && inspector_visible {
            list_pane = list_pane.pr(px(layout.drawer_w));
        }

        let split = if docked && !inspector_visible {
            // F19: a closed docked panel takes no space at all — the list gets
            // the whole viewport instead of an empty split half.
            list_pane.into_any_element()
        } else if docked {
            h_resizable("main-split")
                .with_state(&self.split_state)
                .child(
                    resizable_panel()
                        .size_range(px(RowMetrics::TABLE_MIN)..px(10000.))
                        .child(list_pane),
                )
                .child(
                    resizable_panel()
                        .size(px(inspector_width))
                        .size_range(px(INSPECTOR_WIDTH_MIN)..px(INSPECTOR_WIDTH_MAX))
                        .child(self.render_detail(window, cx)),
                )
                .into_any_element()
        } else {
            div()
                .id("main-split-drawer")
                .test_support()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(list_pane)
                .children(if inspector_visible {
                    vec![self.render_inspector_drawer(layout.drawer_w, window, cx)]
                } else {
                    Vec::new()
                })
                .into_any_element()
        };

        self.render_shell(cx)
            .child(self.render_titlebar(current_title, window, cx))
            .child(
                div()
                    .id("body")
                    .test_support()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(nav)
                    .child(
                        div()
                            .id("main-column")
                            .test_support()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .id("main-content")
                                    .test_support()
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .child(split),
                            )
                            // The engine bar is a sibling of the content row,
                            // not a child of the split: it spans the table and
                            // the inspector, and the nav is never under it.
                            .child(engine_bar),
                    ),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
