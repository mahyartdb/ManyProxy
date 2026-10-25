use crate::*;

impl AppView {
    pub(crate) fn render_empty_ctas(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div().id("empty-ctas").test_support()
    }

    /// T-03: the window width decides the inspector's shape. At or above the
    /// breakpoint the inspector docks to the right and takes its share; below
    /// it the inspector becomes a slide-over drawer and the table keeps the
    /// whole window.
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

    /// T-03: the width the library table may spend. Below the drawer's
    /// window width the inspector stops taking space from the list, so the
    /// table keeps the whole window; above it, the docked inspector takes
    /// its share and the table keeps at least `RowMetrics::TABLE_MIN`.
    pub(crate) fn library_table_width(&self, window: &Window, cx: &App) -> f32 {
        let window_width = window.viewport_size().width.as_f32();
        if !self.inspector_docked(window) {
            return window_width;
        }
        (window_width - self.inspector_width(cx)).max(RowMetrics::TABLE_MIN)
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = cx.entity();
        // T-03: one geometry spec, decided once per frame and handed to both
        // the header strip and the row.
        let table_width = self.library_table_width(window, cx);
        self.libraries.update(cx, |state, cx| {
            let list_focus = state.focus_handle(cx);
            let delegate = state.delegate_mut();
            if delegate.app_view.is_none() {
                delegate.app_view = Some(app_entity);
                cx.notify();
            }
            let metrics = RowMetrics::for_table_width(table_width, delegate.row_columns);
            delegate.set_row_metrics(metrics);
            // T-06: the row cards paint the kit focus ring while the list holds
            // the keyboard; the handle lives on the ListState, the app owns the
            // cards, so the frame hands it across once.
            delegate.list_focus = Some(list_focus);
        });

        let show_detail = self.config_count(cx) != 0;
        // T-26: the titlebar label carries the same `· N running` text the OS
        // title gets, both composed from the one shared helper.
        let current_title = window_title_text(self.engine.running_count());
        let facet_bar = self.render_facet_bar(window, cx);
        let bulk_bar = self.render_bulk_bar(cx);
        let column_headers = self.render_library_column_headers(cx);
        let empty_ctas = self.render_empty_ctas(cx);
        let mut list_pane = div()
            .id("config-list")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(facet_bar)
            .child(bulk_bar)
            .child(column_headers)
            .child(List::new(&self.libraries).flex_1());
        list_pane = list_pane.child(empty_ctas);

        let docked = self.inspector_docked(window);
        let inspector_width = self.inspector_width(cx);
        let split = if docked {
            h_resizable("main-split")
                .with_state(&self.split_state)
                .child(
                    resizable_panel()
                        .size_range(px(RowMetrics::TABLE_MIN)..px(10000.))
                        .child(list_pane),
                )
                .child(
                    resizable_panel()
                        .visible(show_detail)
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
                .children(if show_detail && self.inspector_drawer_open {
                    vec![self.render_inspector_drawer(inspector_width, window, cx)]
                } else {
                    Vec::new()
                })
                .into_any_element()
        };

        self.render_shell(cx)
            .child(self.render_titlebar(current_title, window, cx))
            .child(self.render_toolbar(cx))
            .children(self.render_writable_banner(cx))
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
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
