use crate::*;
use gpui_kit::prelude::FluentBuilder as _;
use std::collections::BTreeSet;

impl AppView {
    /// The filter slot above the table. It always occupies its row, so the
    /// table never moves. A selection swaps the row for the batch bar; the
    /// query stays untouched underneath.
    pub(crate) fn render_filter_slot(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let height = self.density.filter_row_height();
        if self.selection_count() > 0 {
            return div()
                .id("filter-slot")
                .test_support()
                .flex()
                .flex_row()
                .flex_nowrap()
                .items_center()
                .gap_2()
                .w_full()
                .h(px(height))
                .flex_shrink_0()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(self.render_batch_bar(cx))
                .into_any_element();
        }
        if self.port_fields_stale {
            self.sync_port_fields(window, cx);
            self.port_fields_stale = false;
        }
        self.render_filter_row(cx)
    }

    fn render_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let active = self.query.is_active();

        // Snapshot every count the popovers show. Reading the app entity from
        // inside its own render would re-enter the lease already held here.
        let (statuses, chosen_statuses, protocols, chosen_protocols, latency_options, bind_options) = {
            let delegate = self.libraries.read(cx).delegate();
            let facts = self.query_facts(delegate);
            (
                status_counts(
                    &delegate.rows,
                    &delegate.search_index,
                    &facts,
                    &self.scope,
                    &self.query,
                ),
                self.query.statuses.clone(),
                protocol_counts(
                    &delegate.rows,
                    &delegate.search_index,
                    &facts,
                    &self.scope,
                    &self.query,
                ),
                self.query.protocols.clone(),
                latency_counts(
                    &delegate.rows,
                    &delegate.search_index,
                    &facts,
                    &self.scope,
                    &self.query,
                ),
                bind_counts(
                    &delegate.rows,
                    &delegate.search_index,
                    &facts,
                    &self.scope,
                    &self.query,
                ),
            )
        };
        let protocol_filter_text = self.protocol_filter.read(cx).value().to_string();

        let mut row = div()
            .id("filter-slot")
            .test_support()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_2()
            .w_full()
            .h(px(self.density.filter_row_height()))
            .flex_shrink_0()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .id("library-search")
                    .test_support()
                    .flex_1()
                    .min_w(px(140.))
                    .max_w(px(260.))
                    .tooltip(|_, cx| cx.new(|_| Tooltip::new(search_hint())).into())
                    .child(
                        Input::new(&self.search_input)
                            .small()
                            .aria_label(t!("filter-search-placeholder")),
                    ),
            )
            .child(self.render_status_menu(&view, statuses, chosen_statuses))
            .child(self.render_protocol_menu(
                &view,
                protocols,
                chosen_protocols,
                protocol_filter_text,
            ))
            .child(self.render_filter_popover(&view, latency_options, bind_options))
            .child(div().flex_1());

        row = row.child(self.render_filter_count(cx));
        if active {
            let clear = view.clone();
            row = row.child(
                Button::new("filter-clear")
                    .small()
                    .label(t!("filter-clear"))
                    .tooltip(t!("filter-clear-all-tip"))
                    .on_click(move |_, window, cx| {
                        clear.update(cx, |view, cx| view.clear_query(window, cx));
                    }),
            );
        }
        row.into_any_element()
    }

    fn render_filter_count(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = self.filter_count_text(cx);
        div()
            .id("filter-count")
            .test_support()
            .aria_label(text.clone())
            .flex_shrink_0()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(text)
            .into_any_element()
    }

    /// R24: the count is always present. A first load with no rows names the
    /// wait instead of a lying `0 of 0`; otherwise it reads N of M.
    pub(crate) fn filter_count_text(&self, cx: &App) -> String {
        let delegate = self.libraries.read(cx).delegate();
        if delegate.rows.is_empty() && matches!(delegate.load, LibraryLoad::Loading(_)) {
            return t!("library-loading");
        }
        search_count_text(self.visible.len(), scope_total(&delegate.rows, &self.scope))
    }

    fn render_status_menu(
        &self,
        view: &Entity<AppView>,
        counts: Vec<(StatusFacet, usize)>,
        chosen: BTreeSet<StatusFacet>,
    ) -> AnyElement {
        let label = match chosen.len() {
            0 => t!("filter-status"),
            1 => match chosen.iter().next() {
                Some(status) => t!("filter-status-value", value = status_label(*status)),
                None => t!("filter-status"),
            },
            count => format!("{} · {count}", t!("filter-status")),
        };
        let has_chosen = !chosen.is_empty();
        let mut cell = div().flex().flex_row().items_center().flex_shrink_0();
        let popover_view = view.clone();
        cell = cell.child(
            Popover::new("filter-status-popover")
                .trigger(
                    Button::new("filter-status")
                        .small()
                        .label(label)
                        .selected(!chosen.is_empty()),
                )
                .content(move |_, _, _| {
                    let mut body = div().flex().flex_col().gap_1().min_w(px(190.));
                    for &(status, count) in &counts {
                        let checked = chosen.contains(&status);
                        let option_view = popover_view.clone();
                        body = body.child(
                            Checkbox::new(format!("filter-status-option-{}", status_token(status)))
                                .checked(checked)
                                .label(format!("{} · {count}", status_label(status)))
                                .on_change(move |on, _, cx| {
                                    option_view.update(cx, |view, cx| {
                                        view.set_status_facet(status, *on, cx);
                                    });
                                }),
                        );
                    }
                    body
                }),
        );
        if has_chosen {
            let clear = view.clone();
            cell = cell.child(
                Button::new("filter-status-clear")
                    .small()
                    .label("×")
                    .tooltip(t!("filter-status-clear-tip"))
                    .on_click(move |_, _, cx| {
                        clear.update(cx, |view, cx| {
                            for status in StatusFacet::ALL {
                                view.set_status_facet(status, false, cx);
                            }
                        });
                    }),
            );
        }
        cell.into_any_element()
    }

    fn render_protocol_menu(
        &self,
        view: &Entity<AppView>,
        options: Vec<(String, usize)>,
        chosen: BTreeSet<String>,
        filter_text: String,
    ) -> AnyElement {
        let label = match chosen.len() {
            0 => t!("filter-protocol"),
            1 => match chosen.iter().next() {
                Some(protocol) => t!("filter-protocol-value", value = protocol.to_uppercase()),
                None => t!("filter-protocol"),
            },
            count => format!("{} · {count}", t!("filter-protocol")),
        };
        let show_filter = options.len() > 8;
        let needle = filter_text.to_lowercase();
        let protocol_filter = self.protocol_filter.clone();
        let has_chosen = !chosen.is_empty();
        let mut cell = div().flex().flex_row().items_center().flex_shrink_0();
        let popover_view = view.clone();
        cell = cell.child(
            Popover::new("filter-protocol-popover")
                .trigger(
                    Button::new("filter-protocol")
                        .small()
                        .label(label)
                        .selected(!chosen.is_empty()),
                )
                .content(move |_, _, _| {
                    let mut body = div().flex().flex_col().gap_1().min_w(px(200.));
                    if show_filter {
                        body = body.child(
                            Input::new(&protocol_filter)
                                .small()
                                .aria_label(t!("filter-protocol-filter-placeholder")),
                        );
                    }
                    for (protocol, count) in options
                        .iter()
                        .filter(|(protocol, _)| protocol.to_lowercase().contains(&needle))
                    {
                        let checked = chosen.contains(protocol);
                        let option_view = popover_view.clone();
                        let protocol = protocol.clone();
                        let label = format!("{} · {count}", protocol.to_uppercase());
                        body = body.child(
                            Checkbox::new(format!("filter-protocol-option-{protocol}"))
                                .checked(checked)
                                .label(label)
                                .on_change(move |on, _, cx| {
                                    option_view.update(cx, |view, cx| {
                                        view.set_protocol_facet(protocol.clone(), *on, cx);
                                    });
                                }),
                        );
                    }
                    body
                }),
        );
        if has_chosen {
            let clear = view.clone();
            cell = cell.child(
                Button::new("filter-protocol-clear")
                    .small()
                    .label("×")
                    .tooltip(t!("filter-protocol-clear-tip"))
                    .on_click(move |_, _, cx| {
                        clear.update(cx, |view, cx| {
                            let protocols: Vec<String> =
                                view.query.protocols.iter().cloned().collect();
                            for protocol in protocols {
                                view.set_protocol_facet(protocol, false, cx);
                            }
                        });
                    }),
            );
        }
        cell.into_any_element()
    }

    fn render_filter_popover(
        &self,
        view: &Entity<AppView>,
        latency_options: Vec<(LatencyBucket, usize)>,
        bind_options: Vec<(String, usize)>,
    ) -> AnyElement {
        let count = self.query.long_tail_count();
        let label = if count == 0 {
            t!("filter-more")
        } else {
            t!("filter-count", count = count)
        };
        let latency = self.query.latency;
        let binds = self.query.binds.clone();
        let port = self.query.port;
        let custom = self.query.custom_endpoint;
        let port_error = self.port_error.clone();
        let port_from = self.port_from.clone();
        let port_to = self.port_to.clone();
        let popover_view = view.clone();
        Popover::new("filter-more-popover")
            .trigger(
                Button::new("filter-more")
                    .small()
                    .label(label)
                    .selected(count > 0),
            )
            .content(move |_, _, cx| {
                let mut body = div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .min_w(px(260.))
                    .max_w(px(320.));
                let mut latency_section = div().flex().flex_col().gap_1().child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(Label::new(t!("filter-latency")))
                        .when(latency != LatencyFacet::default(), |this| {
                            let clear = popover_view.clone();
                            this.child(
                                Button::new("filter-latency-clear")
                                    .small()
                                    .label("×")
                                    .tooltip(t!("filter-latency-clear-tip"))
                                    .on_click(move |_, _, cx| {
                                        clear.update(cx, |view, cx| {
                                            view.set_latency_facet(LatencyFacet::default(), cx);
                                        });
                                    }),
                            )
                        }),
                );
                for &(bucket, count) in &latency_options {
                    let option_view = popover_view.clone();
                    let include_unprobed = latency.include_unprobed;
                    latency_section = latency_section.child(
                        Checkbox::new(format!("filter-latency-option-{}", bucket.token()))
                            .checked(latency.bucket == bucket)
                            .label(format!("{} · {count}", latency_bucket_label(bucket)))
                            .on_change(move |on, _, cx| {
                                option_view.update(cx, |view, cx| {
                                    view.set_latency_facet(
                                        LatencyFacet {
                                            bucket: if *on { bucket } else { LatencyBucket::Any },
                                            include_unprobed,
                                        },
                                        cx,
                                    );
                                });
                            }),
                    );
                }
                let unprobed_view = popover_view.clone();
                latency_section = latency_section.child(
                    Checkbox::new("filter-include-unprobed")
                        .checked(latency.include_unprobed)
                        .label(t!("filter-include-unprobed"))
                        .on_change(move |on, _, cx| {
                            let include_unprobed = *on;
                            unprobed_view.update(cx, |view, cx| {
                                view.set_latency_facet(
                                    LatencyFacet {
                                        bucket: view.query.latency.bucket,
                                        include_unprobed,
                                    },
                                    cx,
                                );
                            });
                        }),
                );
                body = body.child(latency_section);

                let mut bind_section = div().flex().flex_col().gap_1().child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(Label::new(t!("filter-bind")))
                        .when(!binds.is_empty(), |this| {
                            let clear = popover_view.clone();
                            this.child(
                                Button::new("filter-bind-clear")
                                    .small()
                                    .label("×")
                                    .tooltip(t!("filter-bind-clear-tip"))
                                    .on_click(move |_, _, cx| {
                                        clear.update(cx, |view, cx| {
                                            let binds: Vec<String> =
                                                view.query.binds.iter().cloned().collect();
                                            for bind in binds {
                                                view.set_bind_facet(bind, false, cx);
                                            }
                                        });
                                    }),
                            )
                        }),
                );
                for (bind, count) in &bind_options {
                    let checked = binds.contains(bind);
                    let option_view = popover_view.clone();
                    let bind = bind.clone();
                    let label = format!("{bind} · {count}");
                    bind_section = bind_section.child(
                        Checkbox::new(format!("filter-bind-option-{bind}"))
                            .checked(checked)
                            .label(label)
                            .on_change(move |on, _, cx| {
                                option_view.update(cx, |view, cx| {
                                    view.set_bind_facet(bind.clone(), *on, cx);
                                });
                            }),
                    );
                }
                body = body.child(bind_section);

                let mut port_section = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(Label::new(t!("filter-port")))
                            .when(port.is_some(), |this| {
                                let clear = popover_view.clone();
                                this.child(
                                    Button::new("filter-port-clear")
                                        .small()
                                        .label("×")
                                        .tooltip(t!("filter-port-clear-tip"))
                                        .on_click(move |_, _, cx| {
                                            clear.update(cx, |view, cx| {
                                                view.set_port_range("", "", cx);
                                            });
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .flex_1()
                                    .child(Label::new(t!("filter-port-from")))
                                    .child(Input::new(&port_from).small()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .flex_1()
                                    .child(Label::new(t!("filter-port-to")))
                                    .child(Input::new(&port_to).small()),
                            ),
                    );
                if let Some(error) = &port_error {
                    port_section = port_section.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(error.clone()),
                    );
                }
                body = body.child(port_section);

                let custom_view = popover_view.clone();
                body.child(
                    div()
                        .id("filter-custom-endpoint-row")
                        .tooltip(|_, cx| {
                            cx.new(|_| Tooltip::new(t!("filter-custom-endpoint-tip")))
                                .into()
                        })
                        .child(
                            Checkbox::new("filter-custom-endpoint")
                                .checked(custom)
                                .label(t!("filter-custom-endpoint"))
                                .on_change(move |on, _, cx| {
                                    custom_view.update(cx, |view, cx| {
                                        view.set_custom_endpoint(*on, cx);
                                    });
                                }),
                        ),
                )
            })
            .into_any_element()
    }
}

fn status_label(status: StatusFacet) -> String {
    match status {
        StatusFacet::Enabled => t!("facet-status-enabled"),
        StatusFacet::Disabled => t!("facet-status-disabled"),
        StatusFacet::Running => t!("facet-status-running"),
        StatusFacet::Dead => t!("facet-status-dead"),
        StatusFacet::Stopped => t!("facet-status-stopped"),
    }
}

fn status_token(status: StatusFacet) -> &'static str {
    match status {
        StatusFacet::Enabled => "enabled",
        StatusFacet::Disabled => "disabled",
        StatusFacet::Running => "running",
        StatusFacet::Dead => "dead",
        StatusFacet::Stopped => "stopped",
    }
}
