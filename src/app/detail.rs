use crate::*;

/// T-02: a key never rides inside a label or an accessible name. The operate
/// row announces the action alone and keeps the key in its tooltip, so the
/// key-bearing tooltip text is named here and pinned by the test suite.
pub(crate) const OPERATE_START_TOOLTIP: &str = "Start the selected endpoint (F5)";
pub(crate) const OPERATE_STOP_TOOLTIP: &str = "Stop the selected endpoint (Shift-F5)";
pub(crate) const OPERATE_TEST_TOOLTIP: &str = "Test the selected endpoint (Ctrl-T)";

impl AppView {
    pub(crate) fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(config) = self.selected_config(cx) else {
            if self.config_count(cx) == 0 {
                return div().id("config-detail").test_support().into_any_element();
            }
            return self.render_detail_empty(cx).into_any_element();
        };
        self.render_detail_selected(cx, &config).into_any_element()
    }

    pub(crate) fn render_detail_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("config-detail")
            .test_support()

            .border_t_1()
            .border_color(cx.theme().border)
            .p_2()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                Empty::new()
                    .header(
                        EmptyHeader::new()
                            .title(EmptyTitle::new().child(
                                "Select a configuration to inspect its endpoint, health, and usage",
                            ))

                            .description(EmptyDescription::new().child(
                                "Pick a row in the library — its endpoints and controls appear here.",
                            )),
                    )
                    .content(
                        EmptyContent::new().child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(
                                    Button::new("detail-add")

                                        .small()
                                        .label("Add configuration")
                                        .tooltip("Add a proxy configuration")
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.open_add_dialog(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("detail-import")

                                        .small()
                                        .label("Import configurations")
                                        .tooltip("Import share links or files")
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.open_import_dialog(
                                                "",
                                                DEFAULT_GROUP,
                                                None,
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_detail_selected(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        div()
            .id("config-detail")
            .test_support()
            .border_t_1()
            .border_color(cx.theme().border)
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .h_full()
            .min_h_0()
            .child(self.render_inspector_tabs(cx))
            .child(
                div()
                    .id("config-detail-body")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .track_scroll(&self.detail_scroll)
                    .child(self.render_inspector_overview(cx, config))
                    .child(Self::motion_enter(
                        match self.inspector_tab {
                            InspectorTab::Details => {
                                self.render_inspector_details(cx, config).into_any_element()
                            }
                            InspectorTab::Health => {
                                self.render_inspector_health(cx, config).into_any_element()
                            }
                            InspectorTab::Presets => {
                                self.render_inspector_presets(cx, config).into_any_element()
                            }
                            InspectorTab::Logs => self.render_inspector_logs(cx).into_any_element(),
                            InspectorTab::Connections => {
                                self.render_inspector_connections(cx).into_any_element()
                            }
                            InspectorTab::Traffic => {
                                self.render_inspector_traffic(cx).into_any_element()
                            }
                        },
                        format!("inspector-pane-enter-{}", self.inspector_tab.index()),
                        cx,
                    )),
            )
            .child(self.render_detail_actions(cx, config))
            .into_any_element()
    }

    pub(crate) fn render_inspector_logs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.visible_log_lines(cx);

        const LOG_LEVEL_LABELS: [&str; 5] = ["All", "Debug", "Info", "Warn", "Error"];
        let current_level = self.log_level;
        let selected_level = LOG_LEVEL_LABELS
            .iter()
            .position(|label| LogLevel::from_label(label) == current_level);
        let mut level_group = RadioGroup::horizontal("log-pane-level-options")
            .selected_index(selected_level)
            .on_change({
                let group_view = cx.entity();
                move |ix, window, cx| {
                    let level = LOG_LEVEL_LABELS
                        .get(*ix)
                        .map(|label| LogLevel::from_label(label))
                        .unwrap_or(current_level);
                    let level_view = group_view.clone();
                    level_view.update(cx, |view, cx| {
                        view.set_log_level(level, window, cx);
                    });
                }
            });
        for label in LOG_LEVEL_LABELS {
            level_group = level_group.child(
                Radio::new(label)
                    .label(label)
                    .tooltip(log_level_tooltip(label)),
            );
        }
        let mut parts = Vec::new();
        if !self.log_filter.trim().is_empty() {
            parts.push(format!("include {:?}", self.log_filter.trim()));
        }
        if !self.log_exclude.trim().is_empty() {
            parts.push(format!("exclude {:?}", self.log_exclude.trim()));
        }
        if !self.log_regex.trim().is_empty() {
            parts.push(format!("regex {:?}", self.log_regex.trim()));
        }
        if let Some(level) = self.log_level {
            parts.push(format!("level {}", level.as_str()));
        }
        let summary = if parts.is_empty() {
            "No filters — showing everything.".to_string()
        } else {
            format!("Filters: {}", parts.join(" · "))
        };
        let filter_active = !self.log_filter.trim().is_empty()
            || !self.log_exclude.trim().is_empty()
            || !self.log_regex.trim().is_empty()
            || self.log_level.is_some();
        let mut pane = div()
            .id("config-logs")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                GroupBox::new()
                    .title(format!("Logs ({})", lines.len()))
                    .gap_1()
                    .child(
                        div()
                            .id("log-pane-level-row")
                            .test_support()
                            .child(level_group),
                    )
                    .child(
                        div()
                            .id("log-pane-filters")
                            .test_support()
                            .text_sm()
                            .child(summary),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("log-pane-edit-filters")
                                    .small()
                                    .label("Edit filters")
                                    .tooltip(
                                        "Open the logs sheet to edit include, exclude, and regex",
                                    )
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.open_logs_dialog(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("log-pane-copy")
                                    .small()
                                    .label("Copy visible")
                                    .tooltip("Copy the visible log lines (redacted)")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.copy_visible_logs(window, cx);
                                    })),
                            ),
                    ),
            );
        if lines.is_empty() {
            let mut empty = div()
                .id("log-pane-empty")
                .test_support()
                .text_sm()
                .child("No log lines to show.");
            if filter_active {
                empty = empty.child(
                    Button::new("log-pane-clear")
                        .small()
                        .label("Clear filter")
                        .tooltip("Clear the log text filters and level")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.clear_log_filter(window, cx);
                        })),
                );
            }
            pane = pane.child(empty);
        } else {
            let mut list = div().id("log-pane-lines").test_support().w_full();
            for line in &lines {
                list = list.child(line.clone());
            }
            pane = pane.child(list);
        }
        pane
    }

    pub(crate) fn render_inspector_connections(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected_config(cx).map(|config| config.id);
        let rows = build_conn_rows(
            &self.conn_snapshot,
            &self.conn_delta,
            selected.as_deref(),
            self.conn_selected_only,
            self.conn_sort_desc,
        );
        let library: Vec<ProxyConfig> = self.libraries.read(cx).delegate().rows.clone();

        let dest_width_px = self.store.conn_column_widths()[0] as f32;
        let mut pane = div()
            .id("config-connections")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                GroupBox::new()
                    .title(format!("Connections ({})", rows.len()))
                    .gap_1()
                    .child(div().text_sm().child(
                        "Destination · Exit · Proto · Traffic ↓↑ · Session Δ — click a destination to copy it.",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("conn-toggle-scope")
                                    .small()
                                    .label(if self.conn_selected_only {
                                        "Selected row only: on"
                                    } else {
                                        "Selected row only: off"
                                    })
                                    .tooltip("Show only the selected row's flows (all when nothing is selected)")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.conn_selected_only = !view.conn_selected_only;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("conn-toggle-sort")
                                    .small()
                                    .label(if self.conn_sort_desc {
                                        "Sort: traffic ↓"
                                    } else {
                                        "Sort: traffic ↑"
                                    })
                                    .tooltip("Sort connections by cumulative traffic")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.conn_sort_desc = !view.conn_sort_desc;
                                        cx.notify();
                                    })),
                            )

                            .child(
                                Button::new("conn-columns-narrow")
                                    .small()
                                    .label("Columns −")
                                    .tooltip("Narrow every connections column by 20px (persisted)")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.adjust_conn_column_widths(-20, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("conn-columns-widen")
                                    .small()
                                    .label("Columns +")
                                    .tooltip("Widen every connections column by 20px (persisted)")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.adjust_conn_column_widths(20, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("conn-columns-refresh")
                                    .small()
                                    .label("Refresh Column Widths")
                                    .tooltip("Restore the default connections column widths")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.refresh_conn_column_widths(window, cx);
                                    })),
                            )
                    ),
            );
        if rows.is_empty() {
            pane = pane.child(
                div()
                    .id("config-connections-empty")
                    .test_support()
                    .text_sm()
                    .child("No live connections yet — start an exit to see flows here. Health probes never appear."),
            );
        } else {
            for (ix, row) in rows.iter().enumerate() {
                let (name, bind, protocol) = conn_exit_of(&library, row.row_id.as_deref());
                let dest = row.dest.clone();
                let dest_display = if dest.is_empty() {
                    "(no destination)".to_string()
                } else {
                    redact_secrets(&dest)
                };
                // Copy the displayed string; an empty dest has no hostname to copy.
                let dest_value = if dest.is_empty() {
                    String::new()
                } else {
                    dest_display.clone()
                };
                let mut dest_cell = div()
                    .id(format!("conn-col-dest-{ix}"))
                    .test_support()
                    .w(px(dest_width_px))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        Button::new(format!("conn-copy-{ix}"))
                            .small()
                            .label(dest_display)
                            .tooltip("Copy destination")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.copy_conn_cell(
                                    ConnCopyTarget::Dest(ix),
                                    &dest_value,
                                    window,
                                    cx,
                                );
                            })),
                    );
                if self.conn_copied == Some(ConnCopyTarget::Dest(ix)) {
                    dest_cell = dest_cell.child(
                        div()
                            .id(format!("conn-copied-dest-{ix}"))
                            .test_support()
                            .text_sm()
                            .child("Copied!"),
                    );
                }

                // The endpoint cell copies the row's active `bind:port` contact
                // verbatim; unclaimed rows (no exit) stay non-copyable.
                let endpoint_value = bind.clone();
                let endpoint_claimed = row.row_id.is_some();
                let mut endpoint_button = Button::new(format!("conn-endpoint-{ix}"))
                    .small()
                    .label(bind);
                if endpoint_claimed {
                    endpoint_button =
                        endpoint_button
                            .tooltip("Copy endpoint")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.copy_conn_cell(
                                    ConnCopyTarget::Endpoint(ix),
                                    &endpoint_value,
                                    window,
                                    cx,
                                );
                            }));
                }
                let mut meta = div()
                    .id(format!("conn-meta-{ix}"))
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .child(name)
                    .child("·")
                    .child(endpoint_button);
                if self.conn_copied == Some(ConnCopyTarget::Endpoint(ix)) {
                    meta = meta.child(
                        div()
                            .id(format!("conn-copied-endpoint-{ix}"))
                            .test_support()
                            .child("Copied!"),
                    );
                }
                meta = meta.child(format!(
                    "· {protocol} · ↓{} ↑{} · Δ{}",
                    format_bytes(row.rx_bytes),
                    format_bytes(row.tx_bytes),
                    format_bytes(row.delta_rx.saturating_add(row.delta_tx)),
                ));

                pane = pane.child(div().flex().flex_col().child(dest_cell).child(meta));
            }
        }
        pane
    }

    pub(crate) fn render_inspector_traffic(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let library: Vec<ProxyConfig> = self.libraries.read(cx).delegate().rows.clone();
        let totals = self.store.traffic_totals().unwrap_or_default();
        let exits = aggregate_exit_traffic(&library, &totals);
        let max = exits
            .iter()
            .map(ExitTraffic::total)
            .max()
            .unwrap_or(1)
            .max(1);
        let mut pane = div()
            .id("config-traffic")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                GroupBox::new()
                    .title("Traffic (per-exit)")
                    .gap_1()
                    .child(div().text_sm().child(
                        summed_traffic_text(&self.store)
                            .unwrap_or_else(|| "usage unavailable".to_string()),
                    ))
                    .child(div().text_sm().child(
                        "Per-exit aggregates from stored totals — no Direct line (presets emit no per-rule decisions).",
                    )),
            );
        if exits.is_empty() {
            pane = pane.child(
                div()
                    .id("config-traffic-empty")
                    .test_support()
                    .text_sm()
                    .child("No traffic recorded yet — start an exit and pass data."),
            );
        } else {
            for exit in &exits {
                let filled = ((exit.total() as f64 / max as f64) * 12.0).round() as usize;
                let bar = format!(
                    "{}{}",
                    "█".repeat(filled.min(12)),
                    "░".repeat(12usize.saturating_sub(filled.min(12)))
                );
                pane = pane.child(
                    div()
                        .id(format!("traffic-exit-{}", exit.row_id))
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "{} {} · {} ↓{} ↑{}",
                            bar,
                            exit.name,
                            exit.bind,
                            format_bytes(exit.rx_bytes),
                            format_bytes(exit.tx_bytes),
                        )),
                );
            }
        }
        pane
    }

    pub(crate) fn render_detail_header(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let is_running = self.engine.is_running(&config.id);
        let summary_text = self.health_store.row_summary(&config.id);
        let summary_trimmed = summary_text.trim().to_string();
        let summary_opt = if summary_trimmed.is_empty() {
            None
        } else {
            Some(summary_trimmed.as_str())
        };
        let crashed = self.engine.ids_on_crashed_shards().contains(&config.id);
        let badge = if is_running {
            serving_badge(true, &config.bind_ip, config.port, summary_opt)
        } else {
            serving_badge(false, &config.bind_ip, config.port, None)
        };
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .w_full()
            .child(
                div()
                    .id("detail-acting-on")
                    .test_support()
                    .role(Role::Status)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(acting_on_label(config)),
            )
            .child(Tag::new().child(config.protocol.clone()))
            .child(Tag::new().child(config.group.clone()))
            .child(format!("{}:{}", config.bind_ip, config.port))
            .child(
                div().id("detail-serving").test_support().child(
                    Badge::new()
                        .dot()
                        .color(match serving_tone(is_running, summary_opt, crashed) {
                            ServingTone::Failed => cx.theme().danger,
                            ServingTone::Serving => cx.theme().success,
                            ServingTone::Stopped => cx.theme().secondary,
                        })
                        .child(badge),
                ),
            )
    }

    pub(crate) fn render_detail_endpoints(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let (socks_url, http_url) = proxy_urls(&config.bind_ip, config.port);

        let upstream = self.cached_upstream(cx, config);

        div().id("config-endpoints").test_support().child(
            GroupBox::new().title("Endpoints").gap_1().child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_w(px(620.))
                    .child(Self::endpoint_row(
                        cx,
                        "upstream",
                        upstream,
                        "endpoint-value-upstream",
                        Some(EndpointUrl::Upstream),
                    ))
                    .child(Self::endpoint_row(
                        cx,
                        "SOCKS",
                        socks_url,
                        "endpoint-value-socks",
                        Some(EndpointUrl::Socks),
                    ))
                    .child(Self::endpoint_row(
                        cx,
                        "HTTP",
                        http_url,
                        "endpoint-value-http",
                        Some(EndpointUrl::Http),
                    )),
            ),
        )
    }

    pub(crate) fn endpoint_row(
        cx: &mut Context<Self>,
        label: &'static str,
        value: String,
        value_id: &'static str,
        which: Option<EndpointUrl>,
    ) -> impl IntoElement {
        let tip = value.clone();
        let mut value_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .id(value_id)
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(value),
            );
        if let Some(which) = which {
            let (id, button_label, tip, a11y) = match which {
                EndpointUrl::Socks => (
                    "copy-socks",
                    "Copy SOCKS",
                    "Copy the SOCKS endpoint (no secrets)",
                    "Copy SOCKS URL",
                ),
                EndpointUrl::Http => (
                    "copy-http",
                    "Copy HTTP",
                    "Copy the HTTP endpoint (no secrets)",
                    "Copy HTTP URL",
                ),
                EndpointUrl::Upstream => (
                    "copy-upstream",
                    "Copy upstream",
                    "Copy the upstream host:port (no secrets)",
                    "Copy upstream endpoint",
                ),
            };
            value_row = value_row.child(
                div().flex_shrink_0().child(
                    Button::new(id)
                        .small()
                        .label(button_label)
                        .accessibility_label(a11y)
                        .tooltip(tip)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.copy_single_endpoint_url(which, window, cx);
                        })),
                ),
            );
        }
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .w_full()
            .child(
                div()
                    .w(px(72.))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().description_list_label_foreground)
                    .child(label),
            )
            .child(value_row)
    }

    pub(crate) fn render_detail_health(&self, cx: &App, config: &ProxyConfig) -> impl IntoElement {
        let tone = self.health_store.health_tone(&config.id);
        div().id("config-health").test_support().child(
            GroupBox::new().title("Health").gap_1().child(
                div()
                    .text_color(health_tone_color(tone, cx))
                    .child(self.health_store.detail_line(&config.id).into_any_element()),
            ),
        )
    }

    pub(crate) fn render_detail_usage(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        div().id("config-usage").test_support().child(
            GroupBox::new()
                .title("Usage")
                .gap_1()
                .child(match self.usage_parts(&config.id) {
                    Some((down, up)) => DescriptionList::new()
                        .bordered(false)
                        .small()
                        .columns(2)
                        .label_width(px(72.))
                        .item("Down", down, 1)
                        .item("Up", up, 1)
                        .into_any_element(),
                    None => div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.usage_line(&config.id))
                        .into_any_element(),
                }),
        )
    }

    pub(crate) fn render_detail_history(&self, cx: &App, config: &ProxyConfig) -> impl IntoElement {
        div().id("config-history").test_support().child(
            GroupBox::new()
                .title("History")
                .gap_1()
                .child(self.history_line(&config.id, cx)),
        )
    }

    pub(crate) fn render_detail_actions(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let switch_id = format!("enabled-{}", config.id);
        let switch_view = cx.entity();
        let switch_id_for_handler = config.id.clone();

        let is_running = self.engine.is_running(&config.id);
        let failed = self.detail_failed(config);
        let start_label = RowPhase::of(is_running, self.start_in_flight, failed).start_label();

        div()
            .id("detail-operate")
            .test_support()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .w_full()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Switch::new(switch_id)
                    .checked(config.enabled)
                    .label("Auto-start")
                    .accessibility_label("Auto-start")
                    .tooltip("Auto-start with Start-all — Start/Stop control serving now")
                    .on_change(move |enabled: &bool, window, cx| {
                        let enabled = *enabled;
                        let id = switch_id_for_handler.clone();
                        switch_view.update(cx, |view, cx| {
                            view.commit_enabled_flag(&id, enabled, window, cx);
                        });
                    }),
            )
            .child({
                let mut start = Button::new("engine-start")
                    .small()
                    .label(start_label)
                    .accessibility_label("Start the selected endpoint")
                    .child(shortcut_kbd("f5", "(F5)"))
                    .tooltip(OPERATE_START_TOOLTIP)
                    .loading(self.start_in_flight)
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.start_selected_engine(window, cx);
                    }));
                if self.start_in_flight {
                    start = start.child(
                        div()
                            .id("engine-start-spinner")
                            .test_support()
                            .child(Spinner::new().small()),
                    );
                }

                let mut stop = Button::new("engine-stop")
                    .small()
                    .label("Stop")
                    .accessibility_label("Stop")
                    .child(shortcut_kbd("shift-f5", "(Shift-F5)"))
                    .tooltip(OPERATE_STOP_TOOLTIP)
                    .loading(self.start_in_flight)
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.stop_selected_engine(window, cx);
                    }));
                if self.start_in_flight {
                    stop = stop.child(
                        div()
                            .id("engine-stop-spinner")
                            .test_support()
                            .child(Spinner::new().small()),
                    );
                }

                let mut test = Button::new("health-test")
                    .small()
                    .label("Test")
                    .accessibility_label("Test")
                    .child(shortcut_kbd("ctrl-t", "(Ctrl-T)"))
                    .tooltip(OPERATE_TEST_TOOLTIP)
                    .loading(self.probe_kind_in_flight(false))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.test_selected_health(window, cx);
                    }));
                if self.probe_kind_in_flight(false) {
                    test = test.child(
                        div()
                            .id("health-test-spinner")
                            .test_support()
                            .child(Spinner::new().small()),
                    );
                }
                ButtonGroup::new("detail-operate-primaries")
                    .child(start)
                    .child(stop)
                    .child(test)
            })
            .child({
                let mut ping = Button::new("health-ping")
                    .small()
                    .label("Ping")
                    .tooltip("TCP-ping the selected endpoint")
                    .loading(self.probe_kind_in_flight(true))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.ping_selected(window, cx);
                    }));
                if self.probe_kind_in_flight(true) {
                    ping = ping.child(
                        div()
                            .id("health-ping-spinner")
                            .test_support()
                            .child(Spinner::new().small()),
                    );
                }
                ping
            })
            .child(
                Button::new("logs-open")
                    .small()
                    .label("Logs")
                    .tooltip("Open logs for the selection")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_logs_dialog(window, cx);
                    })),
            )
            .child(
                Button::new("copy-urls")
                    .small()
                    .label("Copy URLs")
                    .tooltip("Copy bind endpoints (no secrets)")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.copy_selected_urls(window, cx);
                    })),
            )
            .child(self.render_row_menu(cx))
    }

    pub(crate) fn row_menu_item(
        verb: &str,
        row_id: &str,
        label: &str,
        tip: &str,
        view: &Entity<AppView>,
        act: impl Fn(&mut AppView, &str, &mut Window, &mut Context<AppView>) + 'static,
    ) -> PopupMenuItem {
        let id = format!("rowctx-{verb}-{row_id}");
        let label = label.to_string();
        let tip = tip.to_string();
        let target = row_id.to_string();
        let view = view.clone();
        PopupMenuItem::element(move |_, _| {
            let tip = tip.clone();
            div()
                .id(id.clone())
                .test_support()
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                .child(label.clone())
        })
        .on_click(move |_, window, cx| {
            let target = target.clone();
            view.update(cx, |view, cx| {
                act(view, &target, window, cx);
            });
        })
    }

    pub(crate) fn row_context_menu(
        menu: PopupMenu,
        row_id: &str,
        view: &Entity<AppView>,
    ) -> PopupMenu {
        menu.item(Self::row_menu_item(
            "start",
            row_id,
            "Start",
            "Start this row's endpoint",
            view,
            AppView::start_row,
        ))
        .item(Self::row_menu_item(
            "stop",
            row_id,
            "Stop",
            "Stop this row's endpoint",
            view,
            AppView::stop_row,
        ))
        .item(Self::row_menu_item(
            "test",
            row_id,
            "Test",
            "URL-test this row's endpoint",
            view,
            AppView::test_row,
        ))
        .item(Self::row_menu_item(
            "ping",
            row_id,
            "Ping",
            "TCP-ping this row's endpoint",
            view,
            AppView::ping_row,
        ))
        .item(Self::row_menu_item(
            "speedtest",
            row_id,
            "Speedtest",
            "Measure this row's transfer rate (mode + timeout)",
            view,
            AppView::speedtest_row,
        ))
        .item(Self::row_menu_item(
            "out-ip",
            row_id,
            "Resolve Out-IP",
            "Show this row's egress IP address",
            view,
            AppView::resolve_out_ip_row,
        ))
        .item(Self::row_menu_item(
            "resolve-domain",
            row_id,
            "Resolve domain",
            "Resolve this row's server domain to an IP and save it",
            view,
            AppView::resolve_domain_row,
        ))
        .item(Self::row_menu_item(
            "copy",
            row_id,
            "Copy URLs",
            "Copy bind endpoints (no secrets)",
            view,
            AppView::copy_row_urls,
        ))
        .item(Self::row_menu_item(
            "share",
            row_id,
            "Share",
            "Copy this row's share link (carries credentials)",
            view,
            AppView::open_share_for_row,
        ))
        .item(Self::row_menu_item(
            "edit",
            row_id,
            "Edit",
            "Edit this row",
            view,
            AppView::open_edit_for_row,
        ))
        .item(Self::row_menu_item(
            "duplicate",
            row_id,
            "Duplicate",
            "Duplicate this row (disabled copy)",
            view,
            AppView::commit_duplicate,
        ))
        .item(Self::row_menu_item(
            "delete",
            row_id,
            "Delete",
            "Delete this row",
            view,
            AppView::open_remove_for_row,
        ))
        .item(Self::row_menu_item(
            "reset-traffic",
            row_id,
            "Reset traffic",
            "Reset this row's traffic counters",
            view,
            AppView::open_traffic_reset_for_row,
        ))
        .item(Self::row_menu_item(
            "reset-usage",
            row_id,
            "Reset usage",
            "Reset this row's usage counters",
            view,
            AppView::open_usage_reset_for_row,
        ))
        .item(Self::row_menu_item(
            "move",
            row_id,
            "Move to group",
            "Move this row to a group",
            view,
            AppView::open_row_move_dialog,
        ))
    }

    pub(crate) fn render_row_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row_view = cx.entity();

        Button::new("row-menu")
            .small()
            .label("Row actions…")
            .tooltip("Edit, delete, duplicate, or reset this row")
            .dropdown_menu(move |menu, _, _| {
                menu.item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("edit-config")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Edit the selected configuration"))
                                    .into()
                            })
                            .child("Edit")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.open_edit_dialog(config, window, cx);
                                }
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .id("delete-config")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Delete the selected configuration"))
                                    .into()
                            })
                            .child("Delete")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.open_remove_confirm(config, window, cx);
                                }
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("duplicate-config")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| {
                                    Tooltip::new("Duplicate the selected row (disabled copy)")
                                })
                                .into()
                            })
                            .child("Duplicate")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.commit_duplicate(&config.id, window, cx);
                                } else {
                                    window.push_notification(
                                        Notification::info(no_selection_message()),
                                        cx,
                                    );
                                }
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("reset-traffic")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Reset this row's traffic counters"))
                                    .into()
                            })
                            .child("Reset traffic")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.open_traffic_reset_confirm(config, window, cx);
                                } else {
                                    window.push_notification(
                                        Notification::info(no_selection_message()),
                                        cx,
                                    );
                                }
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("usage-reset")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Reset this row's usage counters"))
                                    .into()
                            })
                            .child("Reset usage")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.open_usage_reset_confirm(config, window, cx);
                                }
                            });
                        }
                    }),
                )
            })
    }

    pub(crate) fn latency_sparkline(latencies: &[u64]) -> impl IntoElement {
        struct SparkPoint {
            x: String,
            y: f64,
        }
        let points: Vec<SparkPoint> = latencies
            .iter()
            .enumerate()
            .map(|(index, ms)| SparkPoint {
                x: index.to_string(),
                y: *ms as f64,
            })
            .collect();
        div().w(px(160.)).h(px(32.)).child(
            LineChart::new(points)
                .x(|point: &SparkPoint| point.x.clone())
                .y(|point: &SparkPoint| point.y)
                .x_axis(false)
                .grid(false),
        )
    }
}
