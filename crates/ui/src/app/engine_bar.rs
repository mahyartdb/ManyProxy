//! The engine bar: the engine's state, the session's traffic and the global
//! transport controls.
//!
//! Belongs: the bar under the content row and the menu behind its `⋯`.
//! Not here: the console — slice 6 adds it.
use crate::*;

impl AppView {
    /// The engine bar. It sits under the content row, outside the resizable
    /// split, so it spans the table and the inspector alike — never the nav.
    ///
    /// The engine's last error rides here: the retired status strip was the
    /// one surface that kept a crashed engine's cause on screen, and this bar
    /// is its replacement.
    pub(crate) fn render_engine_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let engine_text = engine_status_text(&self.engine);
        let running = self.engine.running_count();
        let usage_text = summed_traffic_text(&self.store);
        // A crash names its cause in `status_text` and stores the same words
        // as the last error; showing both would read the failure twice.
        let engine_error = self
            .engine
            .last_error()
            .map(str::to_string)
            .filter(|error| error != &engine_text);
        let view = cx.entity();
        let testing = self.probe_kind_in_flight(false) || self.group_test_in_flight;

        let engine_tip = engine_text.clone();
        let status = div()
            .id("engine-bar-status")
            .test_support()
            .flex_shrink_0()
            .truncate()
            .aria_label(engine_text.clone())
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(engine_tip.clone())).into())
            .child(engine_text);

        let running_text = t!("running-suffix", running = running);
        let running_segment = div()
            .id("engine-bar-running")
            .test_support()
            .flex_shrink_0()
            .truncate()
            .aria_label(running_text.clone())
            .child(running_text);

        let mut bar = div()
            .id("engine-bar")
            .test_support()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_2()
            .w_full()
            .h(px(self.density.engine_bar_height()))
            .flex_shrink_0()
            .min_w_0()
            .px_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .aria_label(t!("engine-bar-aria"))
            .child(status)
            .child(running_segment);

        if let Some(usage) = usage_text {
            let tip = usage.clone();
            bar = bar
                .child(div().id("engine-bar-separator").test_support().child("·"))
                .child(
                    div()
                        .id("engine-bar-traffic")
                        .test_support()
                        .min_w_0()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .aria_label(usage.clone())
                        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                        .child(usage),
                );
        }

        if let Some(error) = engine_error {
            let tip = error.clone();
            bar = bar.child(
                div()
                    .id("engine-bar-error")
                    .test_support()
                    .min_w_0()
                    .max_w(px(280.))
                    .truncate()
                    .text_color(danger_text_color(cx.theme()))
                    .aria_label(error.clone())
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(error),
            );
        }

        bar.child(div().flex_1())
            .child(
                Button::new("engine-bar-start-all")
                    .small()
                    .label(t!("action-start-all"))
                    .accessibility_label(t!("action-start-all"))
                    .tooltip(t!("toolbar-start-all-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.start_all_engines(window, cx);
                    })),
            )
            .child(
                Button::new("engine-bar-stop-all")
                    .small()
                    .label(t!("action-stop-all"))
                    .accessibility_label(t!("action-stop-all"))
                    .tooltip(t!("toolbar-stop-all-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.stop_all_engines(window, cx);
                    })),
            )
            .child(self.render_engine_menu(view, testing))
            .into_any_element()
    }

    /// The engine bar's `⋯`: the verbs that act on the pool rather than on
    /// one row. `Stop testing` is only live while a probe batch runs.
    pub(crate) fn render_engine_menu(&self, view: Entity<AppView>, testing: bool) -> AnyElement {
        Button::new("engine-bar-overflow")
            .small()
            .ghost()
            .label("⋯")
            .accessibility_label(t!("action-overflow"))
            .tooltip(t!("action-overflow"))
            .dropdown_menu(move |menu, _, _| {
                let test_view = view.clone();
                let stop_view = view.clone();
                menu.item(
                    PopupMenuItem::element(|_, _| {
                        topbar_menu_row(
                            "engine-bar-test-all",
                            t!("toolbar-test-all"),
                            t!("toolbar-test-all"),
                            t!("toolbar-test-all-tip"),
                            None,
                        )
                    })
                    .on_click(move |_, window, cx| {
                        test_view.update(cx, |view, cx| {
                            view.test_all_health(window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        topbar_menu_row(
                            "engine-bar-stop-testing",
                            t!("action-label-stop-testing"),
                            t!("action-label-stop-testing"),
                            t!("toolbar-stop-testing-tip"),
                            None,
                        )
                    })
                    .disabled(!testing)
                    .on_click(move |_, window, cx| {
                        stop_view.update(cx, |view, cx| {
                            view.stop_group_tests(window, cx);
                        });
                    }),
                )
            })
            .into_any_element()
    }
}
