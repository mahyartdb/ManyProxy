use crate::*;

impl AppView {
    /// K12/A5-1: three segments, one grammar.
    ///
    /// `configs · enabled` | `engine <state> · health X live · Y dead` |
    /// `out <bytes> · in <bytes>`
    ///
    /// Ports and connection counters left the strip: ports belong to the
    /// engine's own surface, and the connections count lives in the
    /// Connections inspector tab (its group title already carries `(N)`).
    pub(crate) fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let libraries = self.libraries.read(cx);
        let delegate = libraries.delegate();
        let (total, enabled) = delegate.counts();
        let count_text = status_count_text(total, enabled);
        let filter_text = delegate.active_filter_text();
        let engine_text = self.engine.status_text();
        let health_text = self.health_status_text();
        let usage_text = summed_traffic_text(&self.store);

        let mut left = div()
            .id("status-left")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            // Wide enough for the longest count ("2 configurations · 1 set
            // for launch") so a plural never shifts the neighbouring slots.
            .min_w(px(260.))
            .child({
                let tip = count_text.clone();
                div()
                    .id("status-count")
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .aria_label(count_text.clone())
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(count_text)
            });

        if !filter_text.is_empty() {
            let tip = filter_text.clone();
            left = left
                .child(div().text_color(cx.theme().muted_foreground).child("·"))
                .child(
                    div()
                        .id("status-filter")
                        .test_support()
                        .min_w_0()
                        .max_w(px(280.))
                        .truncate()
                        .aria_label(filter_text.clone())
                        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                        .child(filter_text),
                );
        }

        // K12/A5-1: the two dividers ride with the unit they introduce, so
        // neither floats away from its text. The `·` sits between the two
        // words with the same 8px rhythm the bar uses everywhere else.
        let center = div()
            .id("status-center")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .min_w(px(430.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Separator::vertical())
                    .child({
                        let tip = engine_text.clone();
                        div()
                            .id("engine-status")
                            .test_support()
                            .truncate()
                            .aria_label(engine_text.clone())
                            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                            .child(engine_text)
                    })
                    .child(div().child("·"))
                    .child({
                        let tip = health_text.clone();
                        div()
                            .id("health-status")
                            .test_support()
                            .truncate()
                            .aria_label(health_text.clone())
                            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                            .child(health_text)
                    }),
            );

        let mut right = div()
            .id("status-right")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_2()
            .min_w(px(150.));
        if let Some(usage) = usage_text {
            // K12/A5-2: the separator rides with the segment it splits off, so
            // an absent usage segment can never leave a lone dash on the bar.
            right = right.child(Separator::vertical()).child({
                let tip = usage.clone();
                div()
                    .id("traffic-status")
                    .test_support()
                    .min_w_0()
                    .max_w(px(200.))
                    .truncate()
                    .aria_label(usage.clone())
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(usage)
            });
        }
        // K17/A6-6: the palette's one at-a-glance hint, parked at the bar's
        // right end. A Kbd chip (the same element the menu rows use), not
        // menu text and not a second affordance: the `⌘K` glyph is out —
        // this is a Ctrl-K binding on Windows, and the small-size glyph
        // fallback (`↓` → serif "T", f_0014's family) is what the bar never
        // risks again.
        right = right.child(
            div()
                .id("palette-hint")
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .aria_label("Command palette (Ctrl+K)")
                .child(shortcut_kbd("ctrl-k", "Ctrl+K")),
        );

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
