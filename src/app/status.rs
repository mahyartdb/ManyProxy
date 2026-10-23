use crate::*;

impl AppView {
    pub(crate) fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let libraries = self.libraries.read(cx);
        let delegate = libraries.delegate();
        let (total, enabled) = delegate.counts();
        let engine_text = self.engine.status_text();
        let health_text = self.health_status_text();

        let filter_text = delegate.active_filter_text();
        let ports: Vec<u16> = delegate
            .rows
            .iter()
            .filter(|row| self.engine.is_running(&row.id))
            .map(|row| row.port)
            .collect();
        let ports_text = serving_ports_text(&ports);

        let count_text = format!(
            "{} {} · {} enabled",
            total,
            if total == 1 {
                "configuration"
            } else {
                "configurations"
            },
            enabled
        );
        let mut left = div()
            .id("status-left")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .min_w(px(210.))
            .child({
                let tip = count_text.clone();
                div()
                    .id("status-count")
                    .test_support()
                    .min_w(px(180.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(count_text)
            });

        if !filter_text.is_empty() {
            let tip = filter_text.clone();
            left = left.child(Separator::vertical()).child(
                div()
                    .id("status-filter")
                    .test_support()
                    .min_w_0()
                    .max_w(px(280.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(filter_text),
            );
        }
        let mut center = div()
            .id("status-center")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .min_w(px(430.))
            .child({
                let tip = engine_text.clone();
                div()
                    .id("engine-status")
                    .test_support()
                    .min_w(px(140.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(engine_text)
            })
            .child(Separator::vertical())
            .child({
                let tip = health_text.clone();
                div()
                    .id("health-status")
                    .test_support()
                    .min_w(px(190.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(health_text)
            });

        if let Some(ports) = ports_text {
            center = center.child(Separator::vertical()).child({
                let tip = ports.clone();
                div()
                    .id("status-ports")
                    .test_support()
                    .min_w_0()
                    .max_w(px(200.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(ports)
            });
        }

        if let Some(counters) = self.conn_counter_text() {
            center = center.child(Separator::vertical()).child({
                let tip = counters.clone();
                div()
                    .id("conn-counters")
                    .test_support()
                    .min_w_0()
                    .max_w(px(180.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(counters)
            });
        }
        let mut right = div()
            .id("status-right")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .min_w(px(150.));
        if let Some(totals) = summed_traffic_text(&self.store) {
            right = right.child(Separator::vertical()).child({
                let tip = totals.clone();
                div()
                    .id("traffic-status")
                    .test_support()
                    .min_w_0()
                    .max_w(px(200.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(totals)
            });
        }

        StatusBar::new().left(left).child(center).right(right)
    }

    pub(crate) fn render_search_count(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (visible, total, query) = self
            .libraries
            .read(cx)
            .delegate()
            .delegate_counts_for_search();
        let has_query = !query.trim().is_empty();
        let clear_view = cx.entity();
        div()
            .id("search-count")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .child(search_count_text(visible, total))
            .child(if has_query {
                Button::new("clear-search")
                    .small()
                    .label("Clear search")
                    .tooltip("Clear the search query (keeps facet filters and checked)")
                    .on_click(move |_, window, cx| {
                        clear_view.update(cx, |view, cx| {
                            view.clear_library_search(window, cx);
                        });
                    })
                    .into_any_element()
            } else {
                div()
                    .id("search-count-idle")
                    .test_support()
                    .into_any_element()
            })
            .into_any_element()
    }
}
