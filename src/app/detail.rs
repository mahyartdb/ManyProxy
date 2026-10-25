use crate::*;
use gpui_kit::component::scroll::ScrollableElement as _;

/// T-02: a key never rides inside a label or an accessible name. The operate
/// row announces the action alone and keeps the key in its tooltip, so the
/// key-bearing tooltip text is named here and pinned by the test suite.
pub(crate) const OPERATE_START_TOOLTIP: &str = "Start the selected endpoint (F5)";
pub(crate) const OPERATE_STOP_TOOLTIP: &str = "Stop the selected endpoint (Shift-F5)";
pub(crate) const OPERATE_TEST_TOOLTIP: &str = "Test the selected endpoint (Ctrl-T)";

/// K20/D1 metrics for the inspector tab strip: labels at the kit's xsmall text
/// step with segmented chrome measured in fixed pixels, so the fit below and
/// the layout above can never drift apart.
const TAB_LABEL_REM: f32 = 0.75;
const TAB_LABEL_PAD_X: f32 = 3.;
const TAB_LABEL_GAP: f32 = 2.;
/// Everything the row adds around the labels themselves: 5 × (2 × padding)
/// + 4 gaps + the bar's own 2×2 inset.
const TAB_LABEL_CHROME: f32 = 2. * 5. * TAB_LABEL_PAD_X + 4. * TAB_LABEL_GAP + 4.;

/// The measured fit of the inspector tab strip across frames.
///
/// Labels report their painted width and the strip its capacity through
/// prepaint (the channel the kit's own tab indicator uses); the *next* render
/// normalizes those widths by the scale that painted them and re-solves the
/// scale that keeps all five labels inside the drawer. `settle` reports false
/// once the solved scale matches the applied one — the row is at rest.
#[derive(Clone, Copy)]
struct InspectorTabFit {
    scale: f32,
}

impl InspectorTabFit {
    const ONE: Self = Self { scale: 1.0 };

    fn settle(&mut self, measured: (f32, f32)) -> bool {
        let (labels, capacity) = measured;
        if labels <= 0. || capacity <= TAB_LABEL_CHROME {
            return false;
        }
        // 2% of capacity is air for the sub-pixel rounding between measure
        // and paint; without it the solver would tick 0.5px round-trips.
        let want = (capacity - TAB_LABEL_CHROME) / (labels / self.scale) * 0.98;
        // One 1/128 step at a time, so the settled scale stops moving once
        // the sub-pixel rounding can no longer meaningfully change it.
        let want = (want.clamp(0.5, 1.0) * 128.).round() / 128.;
        if (want - self.scale).abs() < 0.008 {
            return false;
        }
        self.scale = want;
        true
    }
}

#[derive(Default)]
struct InspectorTabFitMeasure {
    /// Per-tab label widths in px from the last completed frame.
    labels: [f32; 5],
    /// Strip capacity in px from the last completed frame.
    container: f32,
}

impl InspectorTabFitMeasure {
    fn report(&self) -> (f32, f32) {
        (self.labels.iter().sum(), self.container)
    }
}

impl AppView {
    /// T-03: below the breakpoint the inspector slides OVER the table as a
    /// drawer. It is an overlay: the table keeps its full width underneath,
    /// so the list is never squeezed under its floor.
    pub(crate) fn render_inspector_drawer(
        &self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let close_view = cx.entity();
        div()
            .id("inspector-drawer")
            .test_support()
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(width))
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .flex()
            .flex_col()
            .child(self.render_detail(window, cx))
            .child(
                div()
                    .id("inspector-drawer-bar")
                    .test_support()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .px_2()
                    .py_1()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("inspector-drawer-close")
                            .small()
                            .label("Close inspector")
                            .tooltip("Close the inspector drawer (Esc)")
                            .on_click(move |_, window, cx| {
                                close_view.update(cx, |view, cx| {
                                    view.close_inspector_drawer(window, cx);
                                });
                            }),
                    ),
            )
            .into_any_element()
    }

    /// T-03: picking a row opens the drawer again.
    pub(crate) fn open_inspector_drawer(&mut self) {
        self.inspector_drawer_open = true;
    }

    /// T-03: Esc closes the drawer and hands focus back to the list. It
    /// reports whether it closed anything, so the caller can leave the
    /// checked set alone — one overlay layer per Esc.
    pub(crate) fn close_inspector_drawer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if window.viewport_size().width.as_f32() >= INSPECTOR_DRAWER_BREAKPOINT {
            return false;
        }
        if !self.inspector_drawer_open {
            return false;
        }
        self.inspector_drawer_open = false;
        window.focus(&self.focus, cx);
        cx.notify();
        true
    }

    pub(crate) fn render_detail(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(config) = self.selected_config(cx) else {
            if self.config_count(cx) == 0 {
                return div().id("config-detail").test_support().into_any_element();
            }
            return self.render_detail_empty(cx).into_any_element();
        };
        self.render_detail_selected(window, cx, &config)
            .into_any_element()
    }

    /// T-03: rows but no selection. The Add and Import calls to action belong
    /// to an EMPTY library only — here the inspector just asks for a row.
    pub(crate) fn render_detail_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("config-detail")
            .test_support()
            .border_l_1()
            .border_color(cx.theme().border)
            .p_2()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .w_full()
            .h_full()
            .min_w_0()
            .child(
                Empty::new().header(
                    EmptyHeader::new()
                        .title(EmptyTitle::new().child(
                            "Select a configuration to inspect its endpoint, health, and usage",
                        ))
                        .description(EmptyDescription::new().child(
                            "Pick a row in the library — its endpoints and controls appear here.",
                        )),
                ),
            )
            .into_any_element()
    }

    pub(crate) fn render_detail_selected(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        div()
            .id("config-detail")
            .test_support()
            .border_l_1()
            .border_color(cx.theme().border)
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .h_full()
            .w_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(self.render_inspector_tabs(window, cx))
            // K1/A4-2: ONE scroll for the whole pane — header, endpoints (with
            // their editors), the pane module, then the operate row. Nothing
            // is pinned to the panel bottom: actions follow content, so the
            // dead gap and the stranded editor strip cannot come back.
            .child(
                div()
                    .id("config-detail-body")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // A scroll container must own a BOUNDED height: `flex_1`
                    // takes the space left by the tabs so the body scrolls
                    // instead of growing past the panel (which clipped the
                    // operate row and reported a 0px scroll range).
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .track_scroll(&self.detail_scroll)
                    // Appears on hover/wheel: a clipped line alone is a poor
                    // scroll cue once the body carries the whole pane.
                    .vertical_scrollbar(&self.detail_scroll)
                    .child(self.render_inspector_overview(window, cx, config))
                    .child(Self::motion_enter(
                        match self.inspector_tab {
                            InspectorTab::Connections => {
                                self.render_inspector_connections(cx).into_any_element()
                            }
                            InspectorTab::Traffic => {
                                self.render_inspector_traffic(cx).into_any_element()
                            }
                            InspectorTab::Health => {
                                self.render_inspector_health(cx, config).into_any_element()
                            }
                            InspectorTab::Logs => self.render_inspector_logs(cx).into_any_element(),
                            // Details and the legacy Ctrl-3 Presets target share
                            // the Details pane — presets live in Routing now.
                            _ => self.render_inspector_details(cx, config).into_any_element(),
                        },
                        format!("inspector-pane-enter-{}", self.inspector_tab.index()),
                        cx,
                    ))
                    .child(self.render_detail_actions(cx, config)),
            )
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
                            .flex_wrap()
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

    /// K1/A4-3: ONE non-wrapping header line — acting echo, protocol tag, and
    /// the dirty dot on the left; group tag and the single serving badge flush
    /// right (a spacer, not a wrap). The badge is a lamp div plus
    /// `<state> · <bind:port> · <latency>` text: no `●`/`○` glyph, no kit
    /// overlay dot, and `dead` is never co-signed with `Running`.
    pub(crate) fn render_detail_header(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let is_running = self.engine.is_running(&config.id);
        let crashed = self.engine.ids_on_crashed_shards().contains(&config.id);
        let health = self.health_store.status(&config.id);
        // The state word follows the row's endpoint truth: a stopped row is
        // never `dead` (nothing is serving that could fail); a serving row
        // with a dead verdict or a crashed shard is `Failed`.
        let dead_evidence = matches!(health.state, crate::health::RowState::Dead) || crashed;
        let state = if !is_running {
            BadgeState::Stopped
        } else if dead_evidence {
            BadgeState::Failed
        } else {
            BadgeState::Running
        };
        // The latency segment exists only while the endpoint is serving and a
        // probe has run — a stale latency must never dress a failed state.
        let latency_ms = if state == BadgeState::Running {
            health.latency_ms
        } else {
            None
        };
        let badge = inspector_badge_text(state, &config.bind_ip, config.port, latency_ms);
        let badge_color = match state {
            BadgeState::Running => cx.theme().success,
            BadgeState::Failed => cx.theme().danger,
            BadgeState::Stopped => cx.theme().secondary,
        };
        div()
            .id("detail-header-line")
            .test_support()
            .flex()
            .flex_row()
            // K1/A4-3 keeps the group chip and badge on the same line via a
            // spacer, but a narrow pane (240px default) cannot fit the name +
            // tag + a ~220px badge side by side. Wrapping is the fallback that
            // keeps the badge INSIDE the panel instead of overflowing past the
            // window edge (invisible). With room to spare the spacer still
            // pushes the badge flush right on one line.
            .flex_wrap()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
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
            .child({
                // T-03: the dirty marker rides the inspector's row header —
                // the row is one line and has no room for it.
                let dirty = self
                    .libraries
                    .read(cx)
                    .delegate()
                    .endpoint_is_dirty(&config.id, cx);
                if !dirty {
                    div().id("row-dirty-idle").test_support().into_any_element()
                } else {
                    let dirty_tip = format!(
                        "\"{}\" has unapplied endpoint edits — Enter applies, Esc reverts",
                        config.name
                    );
                    div()
                        .id(format!("row-dirty-{}", config.id))
                        .test_support()
                        .size(px(6.))
                        .rounded_full()
                        .bg(cx.theme().list_active_border)
                        .tooltip({
                            let tip = dirty_tip.clone();
                            move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                        })
                        .aria_label(dirty_tip)
                        .into_any_element()
                }
            })
            // K1/A4-3: the group chip and the badge stay on the SAME line —
            // the spacer pushes them flush right instead of letting a long
            // name wrap them onto a second line.
            .child(div().flex_1().min_w_0())
            .child(Tag::new().child(config.group.clone()))
            .child(
                div()
                    .id("detail-serving")
                    .test_support()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .child(div().size(px(6.)).rounded_full().bg(badge_color))
                    .child(div().text_sm().text_color(badge_color).child(badge)),
            )
    }

    /// K1/A4-4: ONE endpoints block — the read-only contact rows and the
    /// selected row's editable bind/port strip share one frame, one draft
    /// warning, and ONE copy action (was three per-row copies plus a strip
    /// pinned below the pane body).
    pub(crate) fn render_detail_endpoints(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let (socks_url, http_url) = proxy_urls(&config.bind_ip, config.port);
        // The row label already says SOCKS / HTTP, so the scheme is redundant
        // noise in a 260px pane. Show host:port — it fits whole — while the
        // block's single Copy action still puts the full URLs on the clipboard.
        let socks_display = socks_url
            .strip_prefix("socks5://")
            .unwrap_or(&socks_url)
            .to_string();
        let http_display = http_url
            .strip_prefix("http://")
            .unwrap_or(&http_url)
            .to_string();

        let upstream = self.cached_upstream(cx, config);
        let strip = self.render_row_editors(window, cx, config);

        div().id("config-endpoints").test_support().child(
            GroupBox::new().title("Endpoints").gap_1().child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_full()
                    .min_w_0()
                    .max_w(px(620.))
                    .child(Self::endpoint_row(
                        cx,
                        "Upstream",
                        upstream,
                        "endpoint-value-upstream",
                    ))
                    .child(Self::endpoint_row(
                        cx,
                        "SOCKS",
                        socks_display,
                        "endpoint-value-socks",
                    ))
                    .child(Self::endpoint_row(
                        cx,
                        "HTTP",
                        http_display,
                        "endpoint-value-http",
                    ))
                    // K1/A4-4: one copy action for the whole block — the
                    // per-row `[Copy]` buttons are gone.
                    .child({
                        let copy_id = config.id.clone();
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .w_full()
                            .flex_shrink_0()
                            .child(div().flex_1().min_w_0())
                            .child(
                                Button::new("copy-endpoints")
                                    .small()
                                    .label("Copy URLs")
                                    .accessibility_label("Copy this endpoint's URLs")
                                    .tooltip("Copy bind endpoints (no secrets)")
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.copy_row_urls(&copy_id, window, cx);
                                    })),
                            )
                    })
                    // K1/A4-7: the editor strip lives INSIDE the block it edits.
                    .child(strip)
                    // The draft's live warning rides the strip that caused it.
                    .children(self.render_row_endpoint_warning(cx, config)),
            ),
        )
    }

    /// One read-only contact row. T-03: the value takes the leftover width and
    /// truncates at the 260px default; the full text stays in its tooltip.
    /// K1/A4-4: the per-row Copy left — the block carries the single action.
    pub(crate) fn endpoint_row(
        cx: &mut Context<Self>,
        label: &'static str,
        value: String,
        value_id: &'static str,
    ) -> impl IntoElement {
        let tip = value.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .w(px(64.))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().description_list_label_foreground)
                    .child(label),
            )
            .child(
                div()
                    .id(value_id)
                    .test_support()
                    .flex_1()
                    .min_w(px(72.))
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(value),
            )
    }

    /// K1/A4-5: red ONLY for a failing state — live numbers ride dim mono and
    /// the probe wording drops `(s)` plus the `probe:`/byte jargon. The full
    /// detail line keeps its job inside the failed ALERT, where red is earned.
    pub(crate) fn render_detail_health(&self, cx: &App, config: &ProxyConfig) -> impl IntoElement {
        let status = self.health_store.status(&config.id);
        let mono_number = |text: String| {
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child(text)
        };
        let state_line = match status.state {
            crate::health::RowState::Unknown => div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Never tested — press Test to check")
                .into_any_element(),
            crate::health::RowState::Dead => div()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(format!(
                    "Failed — {}",
                    status.cause.as_deref().unwrap_or("endpoint dead")
                ))
                .into_any_element(),
            crate::health::RowState::Live => {
                let mut samples: Vec<(&str, String)> = Vec::new();
                if let Some(ms) = self
                    .health_store
                    .latest_of(&config.id, HealthKind::UrlTest)
                    .and_then(|sample| sample.latency_ms)
                {
                    samples.push(("URL test", format!("{ms}ms")));
                }
                if let Some(ms) = self
                    .health_store
                    .latest_of(&config.id, HealthKind::TcpPing)
                    .and_then(|sample| sample.latency_ms)
                {
                    samples.push(("TCP ping", format!("{ms}ms")));
                }
                if samples.is_empty() {
                    samples.push(("State", "live".to_string()));
                }
                let mut line = div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .text_sm();
                for (index, (label, value)) in samples.iter().enumerate() {
                    if index > 0 {
                        line = line.child(mono_number("·".to_string()));
                    }
                    line = line.child(*label).child(mono_number(value.clone()));
                }
                line.into_any_element()
            }
        };
        // Probe accounting, plainly worded: bytes with their direction and a
        // correctly pluralized probe count (never `1 probe(s)`).
        let traffic = self.health_store.traffic(&config.id);
        let probe_line = (traffic.probes > 0).then(|| {
            let probes = if traffic.probes == 1 {
                "1 probe".to_string()
            } else {
                format!("{} probes", traffic.probes)
            };
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Probe traffic")
                .child(mono_number(format!("{} ↓", format_bytes(traffic.rx_bytes))))
                .child(mono_number("·".to_string()))
                .child(mono_number(format!("{} ↑", format_bytes(traffic.tx_bytes))))
                .child(mono_number("·".to_string()))
                .child(mono_number(probes))
                .into_any_element()
        });
        div().id("config-health").test_support().child(
            GroupBox::new()
                .title("Health")
                .gap_1()
                .child(state_line)
                .children(probe_line),
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

    /// K1/A4-6: the sparkline earns its 160x32 only with 3+ samples — a
    /// one-or-two-point "trend" is noise, so the block says how many probes
    /// it has instead. The real trend (caption + chart) still comes from
    /// `history_line` once the row has enough history to draw honestly.
    pub(crate) fn render_detail_history(&self, cx: &App, config: &ProxyConfig) -> impl IntoElement {
        // Same 90-day window the sparkline reads, so the count matches the
        // samples `history_line` would chart.
        let since = now_unix().saturating_sub(90 * 86_400);
        let samples = self
            .store
            .load_health_history(&config.id, Some(since))
            .map(|entries| url_test_sparkline(&entries, 120).len())
            .unwrap_or(0);
        let body = if samples >= 3 {
            self.history_line(&config.id, cx)
        } else {
            div()
                .id("config-history-few")
                .test_support()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(if samples == 0 {
                    "No health history yet — press Test to record a probe".to_string()
                } else {
                    format!("History needs 3 samples — {samples} recorded")
                })
                .into_any_element()
        };
        div()
            .id("config-history")
            .test_support()
            .child(GroupBox::new().title("History").gap_1().child(body))
    }

    /// The row's endpoint editors — K1/A4-4 now mounts this strip INSIDE the
    /// endpoints block, with the ids kept stable (`row-bind-<id>`,
    /// `row-port-input-<id>`) so the row and the inspector speak the same
    /// names. Start/Stop live in the operate row; Edit lives in the row's
    /// context menu — neither rides the strip anymore (K1/A4-8).
    pub(crate) fn render_row_editors(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> AnyElement {
        let row_id = config.id.clone();
        let port_stored = config.port.to_string();
        // The drafts live per row in the delegate; the inspector makes sure
        // the selected row's editors exist before it draws them.
        let editors = self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().ensure_row_inputs(
                &row_id,
                &config.bind_ip,
                &port_stored,
                window,
                list_cx,
            );
            state.delegate().endpoint_inputs.get(&row_id).map(|inputs| {
                (
                    inputs.bind.clone(),
                    inputs.port.clone(),
                    inputs.picker.clone(),
                )
            })
        });
        let Some((bind_state, port_state, bind_picker)) = editors else {
            return div().id("row-editors").test_support().into_any_element();
        };
        // T-33: custom-config rows stay JSON-governed — their listen comes
        // from the document, so they keep the free-text bind field.
        let custom_config_row = is_custom_config_protocol(&config.protocol);
        let name = config.name.clone();
        let bind_cell = if custom_config_row {
            div()
                .id(format!("row-bind-{}", config.id))
                .test_support()
                .w(px(118.))
                .tooltip({
                    let tip = format!(
                        "Serve bind address for \"{name}\" — Enter commits, Esc reverts \
                         (custom config: listens come from the JSON)"
                    );
                    move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                })
                .child(
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(
                            Input::new(&bind_state)
                                .id(format!("row-bind-input-{}", config.id))
                                .small()
                                // K1/A4-7: every strip input is bordered + mono.
                                .bordered(true)
                                .aria_label(format!("Serve bind address for {name}")),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .id(format!("row-bind-{}", config.id))
                .test_support()
                .w(px(118.))
                .tooltip({
                    let tip = format!(
                        "Serve bind address for \"{name}\" — pick one from the address list \
                         (Settings → Addresses); port stays free-text"
                    );
                    move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                })
                .child(
                    // K1/A4-7: the picker matches the port field's mono frame.
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(
                            Select::new(&bind_picker)
                                .id(format!("row-bind-picker-{}", config.id))
                                .small()
                                .placeholder("pick address")
                                .accessibility_label(format!("Serve bind address for {name}")),
                        ),
                )
                .into_any_element()
        };
        let add_button = (!custom_config_row).then(|| {
            let add_view = cx.entity();
            Button::new(format!("row-bind-add-{}", config.id))
                .small()
                .label("Add…")
                .accessibility_label("Add address…")
                .tooltip(format!(
                    "Add address… — opens Settings → Addresses; the list is the only source \
                     for row binds, \"{name}\""
                ))
                .on_click(move |_, window, cx| {
                    add_view.update(cx, |view, cx| {
                        view.open_settings_dialog(window, cx);
                    });
                })
                .into_any_element()
        });
        div()
            .id("row-editors")
            .test_support()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            // K1/A4-4: inside the Endpoints frame the strip needs no hairline
            // or own padding — the GroupBox already separates it.
            .key_context("row-endpoint")
            .on_key_down(cx.listener({
                let revert_id = row_id.clone();
                move |view: &mut AppView,
                      event: &KeyDownEvent,
                      window: &mut Window,
                      cx: &mut Context<AppView>| {
                    if event.keystroke.key != "escape" {
                        return;
                    }
                    let reverted = {
                        let focused = view
                            .libraries
                            .read(cx)
                            .delegate()
                            .row_inputs_focused(&revert_id, window, cx);
                        if !focused {
                            false
                        } else {
                            let mut reverted = false;
                            view.libraries.update(cx, |state, list_cx| {
                                reverted = state
                                    .delegate_mut()
                                    .revert_row_inputs(&revert_id, window, list_cx);
                            });
                            reverted
                        }
                    };
                    if reverted {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }
            }))
            .child(bind_cell)
            .children(add_button)
            .child(
                div()
                    .id(format!("row-port-editor-{}", config.id))
                    .test_support()
                    .w(px(76.))
                    .tooltip({
                        let tip = format!(
                            "Local endpoint {}:{} — Enter commits, Esc reverts",
                            config.bind_ip, config.port
                        );
                        move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                    })
                    .child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(
                                Input::new(&port_state)
                                    .id(format!("row-port-input-{}", config.id))
                                    .small()
                                    // K1/A4-7: the port field stopped being the
                                    // borderless one wedged between Add… and value.
                                    .bordered(true)
                                    .aria_label(format!("Local port for {name}")),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// T-03: the draft's live warning — a colliding endpoint or a bind that
    /// leaves loopback — rides with the editors that caused it.
    pub(crate) fn render_row_endpoint_warning(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> Option<AnyElement> {
        let delegate = self.libraries.read(cx).delegate();
        let (bind_text, port_text) = delegate.endpoint_draft_texts(&config.id, cx)?;
        if let Some(warning) =
            bind_port_collision_warning(&delegate.rows, Some(&config.id), &bind_text, &port_text)
        {
            return Some(
                div()
                    .id(format!("row-endpoint-warn-{}", config.id))
                    .test_support()
                    .w_full()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .truncate()
                    .child(warning)
                    .into_any_element(),
            );
        }
        let hint = lan_hint_for_bind(bind_text.trim())?;
        Some(
            div()
                .id(format!("row-endpoint-warn-{}", config.id))
                .test_support()
                .w_full()
                .text_sm()
                .truncate()
                .child(hint)
                .into_any_element(),
        )
    }

    pub(crate) fn render_detail_actions(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let is_running = self.engine.is_running(&config.id);
        let failed = self.detail_failed(config);
        let start_label = RowPhase::of(is_running, self.start_in_flight, failed).start_label();

        // T-03: these three are direct children of the wrapping operate row.
        // A ButtonGroup is a single flex item and cannot break, so inside the
        // 260px drawer it clipped its own last button. Kept as siblings, each
        // button wraps on its own line and the pinned shortcut chips survive.
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

        // The hairline is a sibling wrapper, not part of the row: the pinned
        // minimum-width test budgets four 24px lines plus gaps inside
        // `detail-operate`, and a 1px top border would push the row to 121px.
        // K1/A4-8: ONE tier now — Start · Stop · Test plus the `…` kebab
        // holding Auto-start, Ping, Logs, Copy URLs, Copy proxy. The draft
        // warning travels with the editors into the endpoints block (A4-4).
        div()
            .w_full()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .id("detail-operate")
                    .test_support()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .child(start)
                    .child(stop)
                    .child(test)
                    .child(self.render_row_menu(cx, config)),
            )
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
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        // K8/A6-2: 7 core verbs at top level (Reset… is its own submenu) and
        // the demoted row chores behind `More ▾`. Speedtest / Resolve Out-IP /
        // Resolve domain are retired (K8): they appear in no menu at all.
        let reset_id = row_id.to_string();
        let reset_view = view.clone();
        let more_id = row_id.to_string();
        let more_view = view.clone();
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
            "copy-proxy",
            row_id,
            "Copy proxy",
            "Copy this row's proxy URL",
            view,
            AppView::copy_row_socks,
        ))
        .item(Self::row_menu_item(
            "edit",
            row_id,
            "Edit",
            "Edit this row",
            view,
            AppView::open_edit_for_row,
        ))
        .submenu("Reset…", window, cx, move |menu, _, _| {
            menu.item(Self::row_menu_item(
                "reset-traffic",
                &reset_id,
                "Reset traffic",
                "Reset this row's traffic counters",
                &reset_view,
                AppView::open_traffic_reset_for_row,
            ))
            .item(Self::row_menu_item(
                "reset-usage",
                &reset_id,
                "Reset usage",
                "Reset this row's usage counters",
                &reset_view,
                AppView::open_usage_reset_for_row,
            ))
        })
        .item(Self::row_menu_item(
            "delete",
            row_id,
            "Delete",
            "Delete this row",
            view,
            AppView::open_remove_for_row,
        ))
        .submenu("More ▾", window, cx, move |menu, _, _| {
            menu.item(Self::row_menu_item(
                "move",
                &more_id,
                "Move to group",
                "Move this row to a group",
                &more_view,
                AppView::open_row_move_dialog,
            ))
            .item(Self::row_menu_item(
                "copy",
                &more_id,
                "Copy URLs",
                "Copy bind endpoints (no secrets)",
                &more_view,
                AppView::copy_row_urls,
            ))
            .item(Self::row_menu_item(
                "share",
                &more_id,
                "Share…",
                "Copy this row's share link (carries credentials)",
                &more_view,
                AppView::open_share_for_row,
            ))
            .item(Self::row_menu_item(
                "duplicate",
                &more_id,
                "Duplicate",
                "Duplicate this row (disabled copy)",
                &more_view,
                AppView::commit_duplicate,
            ))
            .item(Self::row_menu_item(
                "ping",
                &more_id,
                "Ping",
                "TCP-ping this row's endpoint",
                &more_view,
                AppView::ping_row,
            ))
        })
    }

    /// K1/A4-8: the operate row's overflow — ONE `…` trigger holding the five
    /// actions that no longer earn a button: Auto-start (writes the stored
    /// flag only), Ping, Logs, Copy URLs, Copy proxy. Row management (Edit /
    /// Delete / Duplicate / Reset) lives in the row's context menu — this
    /// button used to duplicate half of it above itself.
    pub(crate) fn render_row_menu(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let row_view = cx.entity();
        let row_id = config.id.clone();
        let auto_start = config.enabled;

        Button::new("row-menu")
            .small()
            .label("…")
            .accessibility_label("More actions")
            .tooltip("More actions for the selected endpoint")
            .dropdown_menu(move |menu, _, _| {
                let view = row_view.clone();
                let toggle_id = row_id.clone();
                let copy_id = row_id.clone();
                menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .id("kebab-auto-start")
                            .test_support()
                            .role(Role::MenuItem)
                            .aria_label("Auto-start")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| {
                                    Tooltip::new(
                                        "Auto-start with Start-all — Start/Stop control serving now",
                                    )
                                })
                                .into()
                            })
                            .child(format!(
                                "Auto-start · {}",
                                if auto_start { "On" } else { "Off" }
                            ))
                    })
                    .on_click(move |_, window, cx| {
                        let enabled = !auto_start;
                        view.update(cx, |view, cx| {
                            view.commit_enabled_flag(&toggle_id, enabled, window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("kebab-ping")
                            .test_support()
                            .role(Role::MenuItem)
                            .aria_label("Ping")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("TCP-ping the selected endpoint")).into()
                            })
                            .child("Ping")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                view.ping_selected(window, cx);
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("kebab-logs")
                            .test_support()
                            .role(Role::MenuItem)
                            .aria_label("Logs")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Open logs for the selection")).into()
                            })
                            .child("Logs")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                view.open_logs_dialog(window, cx);
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("kebab-copy-urls")
                            .test_support()
                            .role(Role::MenuItem)
                            .aria_label("Copy URLs")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Copy bind endpoints (no secrets)")).into()
                            })
                            .child("Copy URLs")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                view.copy_selected_urls(window, cx);
                            });
                        }
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("kebab-copy-proxy")
                            .test_support()
                            .role(Role::MenuItem)
                            .aria_label("Copy proxy")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .tooltip(|_, cx| {
                                cx.new(|_| Tooltip::new("Copy this row's proxy URL")).into()
                            })
                            .child("Copy proxy")
                    })
                    .on_click({
                        let view = row_view.clone();
                        move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                // SOCKS bind:port only, no secrets — the row
                                // id captured above keeps it row-scoped.
                                view.copy_row_socks(&copy_id, window, cx);
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

impl AppView {
    /// K1/A4-1: ONE segmented switcher — Details · Health · Logs ·
    /// Connections · Traffic, one row. Presets has no tab of its home here
    /// (Routing is its home); the legacy Ctrl-3 target lands on Details. The
    /// Ctrl-1…5 keys live in Settings → Shortcuts, not on the tab labels.
    pub(crate) fn render_inspector_tabs(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use std::cell::RefCell;
        use std::rc::Rc;

        use gpui_kit::base::ElementExt as _;
        use gpui_kit::prelude::FluentBuilder as _;

        let view = cx.entity();
        let (tab_fg_active, tab_fg_muted, tab_bar_bg, pill_bg, radius) = {
            let theme = cx.theme();
            (
                theme.tab_active_foreground,
                theme.muted_foreground,
                theme.tokens.tab_bar_segmented,
                theme.tokens.background,
                theme.radius,
            )
        };
        let pill_radius = (radius - px(2.)).max(px(0.));

        // K20/D1: the five labels size the row instead of splitting 242px into
        // five equal `flex_1` slots — under the drawer the upstream bar clipped
        // Connections and hid Traffic whole. Labels are content-fit; when the
        // measured content still overruns the capacity, ONE uniform scale-down
        // keeps every label painted: the row never wraps, never shortens.
        let fit = window.use_keyed_state("inspector-tab-fit", cx, |_, _| InspectorTabFit::ONE);
        let measure = window
            .use_keyed_state("inspector-tab-fit-measure", cx, |_, _| {
                Rc::new(RefCell::new(InspectorTabFitMeasure::default()))
            })
            .read(cx)
            .clone();
        let mut current_fit = *fit.read(cx);
        if current_fit.settle(measure.borrow().report()) {
            fit.update(cx, |state, _| *state = current_fit);
            // Paint the settled scale on the next frame; this frame is already
            // committed to its own (possibly just-measured) size.
            window.refresh();
        }
        let scale = current_fit.scale;

        let tabs = [
            InspectorTab::Details,
            InspectorTab::Health,
            InspectorTab::Logs,
            InspectorTab::Connections,
            InspectorTab::Traffic,
        ];

        let mut buttons = Vec::with_capacity(tabs.len());
        for (index, tab) in tabs.into_iter().enumerate() {
            let label = tab.a11y_label();
            let slug = label.to_lowercase();
            let selected = self.inspector_tab == tab;
            buttons.push(
                div()
                    .id(format!("inspector-tab-button-{slug}"))
                    .test_support()
                    .role(Role::Tab)
                    .aria_label(label)
                    .aria_selected(selected)
                    // The drawer overlays the table, so a press must not leak
                    // through to the rows underneath it. Mirrors the kit Tab.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |view, _| {
                                view.set_inspector_tab(InspectorTab::from_index(index));
                            });
                        }
                    })
                    .flex()
                    .h_full()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .px(px(TAB_LABEL_PAD_X))
                    .rounded(pill_radius)
                    .text_color(if selected {
                        tab_fg_active
                    } else {
                        tab_fg_muted
                    })
                    .when(selected, |this| this.bg(pill_bg))
                    .when(!selected, |this| {
                        this.hover(|style| style.text_color(tab_fg_active))
                    })
                    .child(
                        div()
                            .id(format!("inspector-tab-{slug}"))
                            .test_support()
                            .text_size(rems(TAB_LABEL_REM * scale))
                            .line_height(relative(1.0))
                            .whitespace_nowrap()
                            .on_prepaint({
                                let measure = measure.clone();
                                move |bounds, _, _| {
                                    measure.borrow_mut().labels[index] = bounds.size.width.into();
                                }
                            })
                            .child(label),
                    ),
            );
        }

        div()
            .id("inspector-tabs")
            .test_support()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .on_prepaint({
                let measure = measure.clone();
                move |bounds, _, _| {
                    measure.borrow_mut().container = bounds.size.width.into();
                }
            })
            .child(
                div()
                    .id("inspector-tab-bar")
                    .role(Role::TabList)
                    .w_full()
                    .min_w_0()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(TAB_LABEL_GAP))
                    .overflow_hidden()
                    .p(px(2.))
                    .rounded(radius)
                    .bg(tab_bar_bg)
                    .children(buttons),
            )
    }

    /// K1/A4-4: the single-scroll head — header, failed alert (only when the
    /// endpoint is the failure source), then the merged endpoints+editors
    /// block. It needs `window` because the folded editor strip materializes
    /// the row's draft inputs.
    pub(crate) fn render_inspector_overview(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let mut overview = div()
            .id("inspector-overview")
            .test_support()
            .flex()
            .flex_col()
            .gap_0()
            .child(self.render_detail_header(cx, config));
        if self.detail_failed(config) {
            overview = overview.child(
                div().id("config-health-alert").test_support().child(
                    Alert::error(
                        "config-health-alert-box",
                        self.health_store.detail_line(&config.id),
                    )
                    .title("Endpoint failed"),
                ),
            );
        }
        overview.child(self.render_detail_endpoints(window, cx, config))
    }

    pub(crate) fn render_inspector_details(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.render_detail_usage(cx, config))
    }

    pub(crate) fn render_inspector_health(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.render_detail_health(cx, config))
            .child(self.render_detail_history(cx, config))
    }
}
