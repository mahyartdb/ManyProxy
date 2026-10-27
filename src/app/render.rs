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
    /// `drawer_visible` only matters below the dock breakpoint; the drawer
    /// width is clamped so the table never drops under its floor.
    pub(crate) fn compute(
        viewport: f32,
        inspector_w: f32,
        docked: bool,
        drawer_visible: bool,
    ) -> Self {
        if docked {
            return Self {
                drawer_w: 0.,
                table_w: (viewport - inspector_w).max(RowMetrics::TABLE_MIN),
            };
        }
        if drawer_visible {
            let drawer_w = inspector_w.min(viewport - RowMetrics::TABLE_MIN).max(0.);
            return Self {
                drawer_w,
                table_w: viewport - drawer_w,
            };
        }
        Self {
            drawer_w: 0.,
            table_w: viewport,
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
                    .label("+ Add configuration")
                    .accessibility_label("Add configuration")
                    .tooltip("Add a proxy configuration (Ctrl-N)")
                    .on_click(move |_, window, cx| {
                        add_view.update(cx, |view, cx| view.open_add_config(window, cx));
                    }),
            )
            .child(
                Button::new("empty-import")
                    .small()
                    .label("Import")
                    .accessibility_label("Import")
                    .tooltip("Import share links or files (Ctrl-O)")
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
        let app_entity = cx.entity();
        // T-03/F2: one geometry spec, computed once per frame and handed to
        // the header strip, the rows and the list pane's drawer reserve.
        let show_detail = self.config_count(cx) != 0;
        let docked = self.inspector_docked(window);
        let inspector_width = self.inspector_width(cx);
        let drawer_visible = !docked && show_detail && self.inspector_drawer_open;
        let layout = InspectorLayout::compute(
            window.viewport_size().width.as_f32(),
            inspector_width,
            docked,
            drawer_visible,
        );
        self.libraries.update(cx, |state, cx| {
            let list_focus = state.focus_handle(cx);
            let delegate = state.delegate_mut();
            if delegate.app_view.is_none() {
                delegate.app_view = Some(app_entity);
                cx.notify();
            }
            let metrics = RowMetrics::for_table_width(layout.table_w, delegate.row_columns);
            delegate.set_row_metrics(metrics);
            // T-06: the row cards paint the kit focus ring while the list holds
            // the keyboard; the handle lives on the ListState, the app owns the
            // cards, so the frame hands it across once.
            delegate.list_focus = Some(list_focus);
        });

        // T-26: the titlebar label carries the same `· N running` text the OS
        // title gets, both composed from the one shared helper.
        let current_title = window_title_text(self.engine.running_count());
        let facet_bar = self.render_facet_bar(window, cx);
        let bulk_bar = self.render_bulk_bar(cx);
        let column_headers = self.render_library_column_headers(cx);
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
        if drawer_visible {
            list_pane = list_pane.pr(px(layout.drawer_w));
        }

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
                .children(if drawer_visible {
                    vec![self.render_inspector_drawer(layout.drawer_w, window, cx)]
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
