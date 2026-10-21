use crate::*;

impl AppView {
    pub(crate) fn render_empty_ctas(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div().id("empty-ctas").test_support()
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = cx.entity();
        self.libraries.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            if delegate.app_view.is_none() {
                delegate.app_view = Some(app_entity);
                cx.notify();
            }
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

        self.render_shell(cx)
            .child(TitleBar::new().child(Label::new(current_title)))
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .id("main-content")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_resizable("main-split")
                            .child(
                                resizable_panel()
                                    .size_range(px(120.)..px(10000.))
                                    .child(list_pane),
                            )
                            .child(
                                resizable_panel()
                                    .visible(show_detail)
                                    .size(px(240.))
                                    .size_range(px(240.)..px(480.))
                                    .child(self.render_detail(cx)),
                            ),
                    ),
            )
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
