//! The inspector: selected-row detail, operate row, connections and traffic tabs.
//! Belongs: everything rendered in the right-hand inspector.
//! Not here: library rows or dialog bodies.
use crate::*;
use gpui_kit::base::{v_virtual_list, Scrollbar, ScrollbarAxis};
use gpui_kit::component::scroll::ScrollableElement as _;

/// T-02: a key never rides inside a label or an accessible name. The operate
/// row announces the action alone and keeps the key in its tooltip, so the
/// key-bearing tooltip text is named here and pinned by the test suite.
pub(crate) fn operate_start_tooltip() -> String {
    t!("detail-start-tip")
}
pub(crate) fn operate_stop_tooltip() -> String {
    t!("detail-stop-tip")
}
pub(crate) fn operate_test_tooltip() -> String {
    t!("detail-test-tip")
}

/// K20/D1 metrics for the inspector tab strip: one fixed text size for all
/// five, pinned so `Details · Health · Logs · Links · Traffic` fit whole at the
/// 240px drawer floor. Nothing scales them.
const TAB_LABEL_REM: f32 = 0.6875;
const TAB_LABEL_PAD_X: f32 = 2.;
const TAB_LABEL_GAP: f32 = 1.;

/// B3: the panes that list rows build a virtual window instead of a full
/// subtree. Uniform row heights keep the size cache exact: a declared height
/// that is too small clips a row, so each row is pinned to one line (log and
/// traffic text truncates, the link card is a fixed two-line block).
const PANE_LOG_ROW_H: f32 = 20.;
const PANE_LINK_ROW_H: f32 = 48.;
const PANE_TRAFFIC_ROW_H: f32 = 20.;
/// A row list never eats the whole pane: header, endpoints and the operate row
/// share the same scroll, so the list is capped to a share of what the panel
/// can show and scrolls itself past it. A short list keeps its natural height.
const PANE_LIST_SHARE: f32 = 0.6;
/// The panel's own height is only known after the first layout; until then the
/// list falls back to this so the pane is never blank on its first frame.
const PANE_LIST_FALLBACK_H: f32 = 320.;

/// B3 acceptance probe: counts the rows a pane actually builds. Debug-only —
/// release builds compile the counter away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaneRows {
    Logs = 0,
    Links = 1,
    Traffic = 2,
}

// Per-thread, because a frame runs on the thread that draws it and the suite
// mounts views in parallel.
#[cfg(any(debug_assertions, test))]
thread_local! {
    static PANE_ROWS_BUILT: std::cell::Cell<[usize; 3]> = const { std::cell::Cell::new([0; 3]) };
}

fn note_pane_row(pane: PaneRows) {
    #[cfg(any(debug_assertions, test))]
    PANE_ROWS_BUILT.with(|built| {
        let mut counts = built.get();
        counts[pane as usize] += 1;
        built.set(counts);
    });
    #[cfg(not(any(debug_assertions, test)))]
    let _ = pane;
}

#[cfg(test)]
pub(crate) fn pane_rows_built(pane: PaneRows) -> usize {
    PANE_ROWS_BUILT.with(|built| built.get()[pane as usize])
}

#[cfg(test)]
pub(crate) fn reset_pane_rows_built() {
    PANE_ROWS_BUILT.with(|built| built.set([0; 3]));
}

/// B3: the pane's row area. The list is handed the visible range only, so rows
/// outside the window are never built; the paired handle remembers the offset
/// and the scrollbar makes the extra scroll range visible in a narrow panel.
fn pane_rows(
    cx: &mut Context<AppView>,
    pane: &mut PaneWindow,
    id: &'static str,
    height: f32,
    rows: usize,
    row_h: f32,
    build: impl 'static + Fn(&mut AppView, usize, &mut Context<AppView>) -> AnyElement,
) -> AnyElement {
    let sizes = pane.sizes_for(rows, row_h);
    let scroll = pane.scroll.clone();
    let bar = scroll.clone();
    let entity = cx.entity();
    div()
        .relative()
        .w_full()
        .h(px(height))
        .min_h_0()
        .child(
            v_virtual_list(
                entity,
                id,
                sizes,
                move |view: &mut AppView, range, _window, cx| {
                    range.map(|ix| build(view, ix, cx)).collect::<Vec<_>>()
                },
            )
            .track_scroll(&scroll),
        )
        .child(
            div()
                .id(format!("{id}-scrollbar-layer"))
                .absolute()
                .inset_0()
                .child(
                    Scrollbar::new(&bar)
                        .id(format!("{id}-scrollbar"))
                        .axis(ScrollbarAxis::Vertical)
                        .viewport_from_layout(),
                ),
        )
        .into_any_element()
}

impl AppView {
    /// T-03: below the breakpoint the inspector slides OVER the table as a
    /// drawer. It is an overlay: the table keeps its own pane beside it, so
    /// the list is never squeezed under its floor. The root occludes input,
    /// or a blank click reaches the rows behind it and retargets the inspector.
    pub(crate) fn render_inspector_drawer(
        &mut self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let close_view = cx.entity();
        let drawer = div()
            .id("inspector-drawer")
            .occlude()
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
                            .label(t!("detail-close"))
                            .tooltip(t!("detail-close-tip"))
                            .on_click(move |_, window, cx| {
                                close_view.update(cx, |view, cx| {
                                    view.close_inspector(window, cx);
                                });
                            }),
                    ),
            );
        // B18: the panel eases in where it stands. The fade rides the drawer's
        // own element, so the overlay keeps `.absolute()` and the list's
        // reserved gutter never re-measures.
        Self::motion_fade(drawer, "inspector-drawer-enter", cx).into_any_element()
    }

    /// F19: picking a row opens the inspector again — it is the only way back
    /// to the row's controls once Esc closed it.
    pub(crate) fn open_inspector(&mut self) {
        self.inspector_visible = true;
    }

    /// F19: Esc and the close button shut the inspector in BOTH presentations
    /// and hand focus back to the list. It reports whether it closed
    /// anything, so the caller can leave the selection alone - one overlay
    /// layer per Esc.
    pub(crate) fn close_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.inspector_visible {
            return false;
        }
        self.inspector_visible = false;
        window.focus(&self.focus, cx);
        cx.notify();
        true
    }

    pub(crate) fn render_detail(
        &mut self,
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
                        .title(EmptyTitle::new().child(t!("detail-empty-title")))
                        .description(EmptyDescription::new().child(t!("detail-empty-description"))),
                ),
            )
            .into_any_element()
    }

    pub(crate) fn render_detail_selected(
        &mut self,
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
            .child(self.render_inspector_tabs(cx))
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
                            InspectorTab::Links => {
                                self.render_inspector_connections(cx).into_any_element()
                            }
                            InspectorTab::Traffic => {
                                self.render_inspector_traffic(cx).into_any_element()
                            }
                            InspectorTab::Health => {
                                self.render_inspector_health(cx, config).into_any_element()
                            }
                            InspectorTab::Logs => self.render_inspector_logs(cx).into_any_element(),
                            InspectorTab::Details => {
                                self.render_inspector_details(cx, config).into_any_element()
                            }
                        },
                        format!("inspector-pane-enter-{}", self.inspector_tab.index()),
                        cx,
                    ))
                    .child(self.render_detail_actions(cx, config)),
            )
            .into_any_element()
    }

    /// B3: how tall the pane's row area is. A short list keeps its natural
    /// height; past its share of the panel the list scrolls itself, because the
    /// panel's own scroll cannot tell which rows are on screen.
    fn pane_list_height(&self, rows: usize, row_h: f32) -> f32 {
        if rows == 0 {
            return 0.;
        }
        let viewport = self.detail_scroll.bounds().size.height.as_f32();
        let viewport = if viewport > 0. {
            viewport
        } else {
            PANE_LIST_FALLBACK_H
        };
        (rows as f32 * row_h).min((viewport * PANE_LIST_SHARE).max(row_h * 3.))
    }

    pub(crate) fn render_inspector_logs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.visible_log_lines(cx);

        const LOG_LEVEL_LABELS: [&str; 5] = ["All", "Debug", "Info", "Warn", "Error"];
        let current_level = self.logs.level;
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
            let level = LogLevel::from_label(label);
            level_group = level_group.child(
                Radio::new(label)
                    .label(log_level_label(level))
                    .tooltip(log_level_tooltip(level)),
            );
        }
        let mut parts = Vec::new();
        if !self.logs.filter.trim().is_empty() {
            parts.push(t!(
                "detail-log-filter-include",
                value = self.logs.filter.trim()
            ));
        }
        if !self.logs.exclude.trim().is_empty() {
            parts.push(t!(
                "detail-log-filter-exclude",
                value = self.logs.exclude.trim()
            ));
        }
        if !self.logs.regex.trim().is_empty() {
            parts.push(t!(
                "detail-log-filter-regex",
                value = self.logs.regex.trim()
            ));
        }
        if let Some(level) = self.logs.level {
            parts.push(t!("detail-log-filter-level", value = level.as_str()));
        }
        let summary = if parts.is_empty() {
            t!("detail-log-filters-none")
        } else {
            t!("detail-log-filters", parts = parts.join(" · "))
        };
        let filter_active = !self.logs.filter.trim().is_empty()
            || !self.logs.exclude.trim().is_empty()
            || !self.logs.regex.trim().is_empty()
            || self.logs.level.is_some();
        let mut pane = div()
            .id("config-logs")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                GroupBox::new()
                    .title(t!("detail-logs-title", count = lines.len()))
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
                                    .label(t!("detail-edit-filters"))
                                    .tooltip(t!("detail-edit-filters-tip"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.open_logs_dialog(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("log-pane-copy")
                                    .small()
                                    .label(t!("detail-copy-visible"))
                                    .tooltip(t!("detail-copy-visible-tip"))
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
                .child(t!("detail-logs-empty"));
            if filter_active {
                empty = empty.child(
                    Button::new("log-pane-clear")
                        .small()
                        .label(t!("detail-clear-filter"))
                        .tooltip(t!("detail-clear-filter-tip"))
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.clear_log_filter(window, cx);
                        })),
                );
            }
            pane = pane.child(empty);
        } else {
            let rows = lines.len();
            let height = self.pane_list_height(rows, PANE_LOG_ROW_H);
            pane = pane.child(pane_rows(
                cx,
                &mut self.pane_logs,
                "log-pane-list",
                height,
                rows,
                PANE_LOG_ROW_H,
                move |_view, ix, _cx| {
                    let line = lines.get(ix).cloned().unwrap_or_default();
                    note_pane_row(PaneRows::Logs);
                    div()
                        .id(("log-line", ix))
                        .w_full()
                        .h(px(PANE_LOG_ROW_H))
                        .text_sm()
                        .truncate()
                        .child(line)
                        .into_any_element()
                },
            ));
        }
        pane
    }

    pub(crate) fn render_inspector_connections(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_config(cx).map(|config| config.id);
        let rows = build_conn_rows(
            &self.conn.snapshot,
            &self.conn.delta,
            selected.as_deref(),
            self.conn.selected_only,
            self.conn.sort_desc,
        );
        let library: Vec<ProxyConfig> = self.libraries.read(cx).delegate().rows.clone();

        let dest_width_px = self.store.conn_column_widths()[0] as f32;
        let mut pane = div()
            .id("config-links")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                GroupBox::new()
                    .title(t!("detail-links-title", count = rows.len()))
                    .gap_1()
                    .child(div().text_sm().child(t!("detail-links-description")))
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
                                    .label(if self.conn.selected_only {
                                        t!("detail-scope-on")
                                    } else {
                                        t!("detail-scope-off")
                                    })
                                    .tooltip(t!("detail-scope-tip"))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.conn.selected_only = !view.conn.selected_only;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("conn-toggle-sort")
                                    .small()
                                    .label(if self.conn.sort_desc {
                                        t!("detail-sort-desc")
                                    } else {
                                        t!("detail-sort-asc")
                                    })
                                    .tooltip(t!("detail-sort-tip"))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.conn.sort_desc = !view.conn.sort_desc;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("conn-columns-narrow")
                                    .small()
                                    .label(t!("detail-columns-narrow"))
                                    .tooltip(t!("detail-columns-narrow-tip"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.adjust_conn_column_widths(-20, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("conn-columns-widen")
                                    .small()
                                    .label(t!("detail-columns-widen"))
                                    .tooltip(t!("detail-columns-widen-tip"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.adjust_conn_column_widths(20, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("conn-columns-refresh")
                                    .small()
                                    .label(t!("detail-columns-reset"))
                                    .tooltip(t!("detail-columns-reset-tip"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.refresh_conn_column_widths(window, cx);
                                    })),
                            ),
                    ),
            );
        if rows.is_empty() {
            pane = pane.child(
                div()
                    .id("config-links-empty")
                    .test_support()
                    .text_sm()
                    .child(t!("detail-links-empty")),
            );
        } else {
            let count = rows.len();
            let height = self.pane_list_height(count, PANE_LINK_ROW_H);
            let library = Rc::new(library);
            pane = pane.child(pane_rows(
                cx,
                &mut self.pane_links,
                "config-link-list",
                height,
                count,
                PANE_LINK_ROW_H,
                move |view, ix, cx| {
                    let Some(row) = rows.get(ix) else {
                        return div().into_any_element();
                    };
                    let (name, bind, protocol) = conn_exit_of(&library, row.row_id.as_deref());
                    let dest = row.dest.clone();
                    let dest_display = if dest.is_empty() {
                        t!("detail-no-destination")
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
                                .tooltip(t!("detail-copy-destination"))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.copy_conn_cell(
                                        ConnCopyTarget::Dest(ix),
                                        &dest_value,
                                        window,
                                        cx,
                                    );
                                })),
                        );
                    if view.conn.copied == Some(ConnCopyTarget::Dest(ix)) {
                        dest_cell = dest_cell.child(
                            div()
                                .id(format!("conn-copied-dest-{ix}"))
                                .test_support()
                                .text_sm()
                                .child(t!("detail-copied")),
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
                        endpoint_button = endpoint_button
                            .tooltip(t!("detail-copy-endpoint"))
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
                        .flex_nowrap()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .overflow_hidden()
                        .child(name)
                        .child("·")
                        .child(endpoint_button);
                    if view.conn.copied == Some(ConnCopyTarget::Endpoint(ix)) {
                        meta = meta.child(
                            div()
                                .id(format!("conn-copied-endpoint-{ix}"))
                                .test_support()
                                .child(t!("detail-copied")),
                        );
                    }
                    meta = meta.child(t!(
                        "detail-conn-meta",
                        protocol = protocol,
                        down = format_bytes(row.rx_bytes),
                        up = format_bytes(row.tx_bytes),
                        delta = format_bytes(row.delta_rx.saturating_add(row.delta_tx))
                    ));

                    note_pane_row(PaneRows::Links);
                    div()
                        .id(("conn-row", ix))
                        .h(px(PANE_LINK_ROW_H))
                        .w_full()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .child(dest_cell)
                        .child(meta)
                        .into_any_element()
                },
            ));
        }
        pane
    }

    pub(crate) fn render_inspector_traffic(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .title(t!("detail-traffic-title"))
                    .gap_1()
                    .child(
                        div().text_sm().child(
                            summed_traffic_text(&self.store)
                                .unwrap_or_else(|| t!("detail-usage-unavailable")),
                        ),
                    )
                    .child(div().text_sm().child(t!("detail-traffic-description"))),
            );
        if exits.is_empty() {
            pane = pane.child(
                div()
                    .id("config-traffic-empty")
                    .test_support()
                    .text_sm()
                    .child(t!("detail-traffic-empty")),
            );
        } else {
            let count = exits.len();
            let height = self.pane_list_height(count, PANE_TRAFFIC_ROW_H);
            pane = pane.child(pane_rows(
                cx,
                &mut self.pane_traffic,
                "config-traffic-list",
                height,
                count,
                PANE_TRAFFIC_ROW_H,
                move |_view, ix, _cx| {
                    let Some(exit) = exits.get(ix) else {
                        return div().into_any_element();
                    };
                    let filled = ((exit.total() as f64 / max as f64) * 12.0).round() as usize;
                    let bar = format!(
                        "{}{}",
                        "█".repeat(filled.min(12)),
                        "░".repeat(12usize.saturating_sub(filled.min(12)))
                    );
                    note_pane_row(PaneRows::Traffic);
                    div()
                        .id(format!("traffic-exit-{}", exit.row_id))
                        .test_support()
                        .w_full()
                        .h(px(PANE_TRAFFIC_ROW_H))
                        .text_sm()
                        .truncate()
                        .child(t!(
                            "detail-traffic-row",
                            bar = bar,
                            name = exit.name.as_str(),
                            bind = exit.bind.as_str(),
                            down = format_bytes(exit.rx_bytes),
                            up = format_bytes(exit.tx_bytes)
                        ))
                        .into_any_element()
                },
            ));
        }
        pane
    }

    /// K1/A4-3: ONE non-wrapping header line — acting echo and the dirty dot on
    /// the left, the single serving badge flush right (a spacer, not a wrap).
    /// The badge is a lamp div plus `<state> · <bind:port> · <latency>` text: no
    /// `●`/`○` glyph, no kit overlay dot, and `dead` is never co-signed with
    /// `Running`.
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
        // DESIGN: the lamp, the badge dot, and the badge text agree. Stopped
        // used to take `secondary`, a surface token, which painted the badge
        // #e5e5e5 on white paper and erased it.
        let badge_color = serving_tone_color(state.into(), cx.theme());
        div()
            .id("detail-header-line")
            .test_support()
            .flex()
            .flex_row()
            // K1/A4-3 keeps the badge on the same line via a spacer, but a
            // narrow pane (240px default) cannot fit the name + a ~220px badge
            // side by side. Wrapping is the fallback that
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
                    let dirty_tip = t!("detail-dirty-tip", name = config.name.as_str());
                    div()
                        .id(format!("row-dirty-{}", config.id))
                        .test_support()
                        .size(px(6.))
                        .rounded_full()
                        .bg(warning_text_color(cx.theme()))
                        .tooltip({
                            let tip = dirty_tip.clone();
                            move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                        })
                        .aria_label(dirty_tip)
                        .into_any_element()
                }
            })
            // K1/A4-3: the badge stays on the SAME line — the spacer pushes it
            // flush right instead of letting a long name wrap it onto a second
            // line.
            .child(div().flex_1().min_w_0())
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
            GroupBox::new()
                .title(t!("detail-endpoints-title"))
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .w_full()
                        .min_w_0()
                        .max_w(px(620.))
                        .child(Self::endpoint_row(
                            cx,
                            t!("detail-endpoint-upstream"),
                            upstream,
                            "endpoint-value-upstream",
                        ))
                        .child(Self::endpoint_row(
                            cx,
                            t!("detail-endpoint-socks"),
                            socks_display,
                            "endpoint-value-socks",
                        ))
                        .child(Self::endpoint_row(
                            cx,
                            t!("detail-endpoint-http"),
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
                                        .label(t!("detail-copy-urls"))
                                        .accessibility_label(t!("detail-copy-urls-aria"))
                                        .tooltip(t!("detail-copy-urls-tip"))
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
        label: String,
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
                .child(t!("detail-never-tested"))
                .into_any_element(),
            crate::health::RowState::Dead => {
                let cause = status
                    .cause
                    .clone()
                    .unwrap_or_else(|| t!("detail-endpoint-dead"));
                div()
                    .text_sm()
                    .text_color(danger_text_color(cx.theme()))
                    .child(t!("detail-failed", cause = cause))
                    .into_any_element()
            }
            crate::health::RowState::Live => {
                let mut samples: Vec<(String, String)> = Vec::new();
                if let Some(ms) = self
                    .health_store
                    .latest_of(&config.id, HealthKind::UrlTest)
                    .and_then(|sample| sample.latency_ms)
                {
                    samples.push((t!("detail-url-test"), format!("{ms}ms")));
                }
                if let Some(ms) = self
                    .health_store
                    .latest_of(&config.id, HealthKind::TcpPing)
                    .and_then(|sample| sample.latency_ms)
                {
                    samples.push((t!("detail-tcp-ping"), format!("{ms}ms")));
                }
                if samples.is_empty() {
                    samples.push((t!("detail-state"), t!("detail-live")));
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
                    line = line.child(label.clone()).child(mono_number(value.clone()));
                }
                line.into_any_element()
            }
        };
        // Probe accounting, plainly worded: bytes with their direction and a
        // correctly pluralized probe count (never `1 probe(s)`).
        let traffic = self.health_store.traffic(&config.id);
        let probe_line = (traffic.probes > 0).then(|| {
            let probes = t!("detail-probe-count", count = traffic.probes);
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(t!("detail-probe-traffic"))
                .child(mono_number(t!(
                    "detail-probe-rx",
                    bytes = format_bytes(traffic.rx_bytes)
                )))
                .child(mono_number("·".to_string()))
                .child(mono_number(t!(
                    "detail-probe-tx",
                    bytes = format_bytes(traffic.tx_bytes)
                )))
                .child(mono_number("·".to_string()))
                .child(mono_number(probes))
                .into_any_element()
        });
        div().id("config-health").test_support().child(
            GroupBox::new()
                .title(t!("detail-health-title"))
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
                .title(t!("detail-usage-title"))
                .gap_1()
                .child(match self.usage_parts(&config.id) {
                    Some((down, up)) => DescriptionList::new()
                        .bordered(false)
                        .small()
                        .columns(2)
                        .label_width(px(72.))
                        .item(t!("detail-usage-down"), down, 1)
                        .item(t!("detail-usage-up"), up, 1)
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
                    t!("detail-history-empty")
                } else {
                    t!("detail-history-few", count = samples)
                })
                .into_any_element()
        };
        div().id("config-history").test_support().child(
            GroupBox::new()
                .title(t!("detail-history-title"))
                .gap_1()
                .child(body),
        )
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
                    let tip = t!("detail-bind-custom-tip", name = name.as_str());
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
                                .aria_label(t!("detail-bind-aria", name = name.as_str())),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .id(format!("row-bind-{}", config.id))
                .test_support()
                .w(px(118.))
                .tooltip({
                    let tip = t!("detail-bind-tip", name = name.as_str());
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
                                .placeholder(t!("detail-bind-placeholder"))
                                .accessibility_label(t!("detail-bind-aria", name = name.as_str())),
                        ),
                )
                .into_any_element()
        };
        let add_button = (!custom_config_row).then(|| {
            let add_view = cx.entity();
            Button::new(format!("row-bind-add-{}", config.id))
                .small()
                .label(t!("detail-add-address"))
                .accessibility_label(t!("detail-add-address-aria"))
                .tooltip(t!("detail-add-address-tip", name = name.as_str()))
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
                        let tip = t!(
                            "detail-port-tip",
                            bind = config.bind_ip.as_str(),
                            port = config.port
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
                                    .aria_label(t!("detail-port-aria", name = name.as_str())),
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
                    .text_color(danger_text_color(cx.theme()))
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
        let phase = RowPhase::of(is_running, self.start_in_flight, failed);
        let start_label = match phase {
            RowPhase::Starting => t!("detail-starting"),
            RowPhase::Stopped | RowPhase::Running | RowPhase::Failed => t!("detail-start"),
        };

        // T-03: these three are direct children of the wrapping operate row.
        // A ButtonGroup is a single flex item and cannot break, so inside the
        // 260px drawer it clipped its own last button. Kept as siblings, each
        // button wraps on its own line and the pinned shortcut chips survive.
        let mut start = Button::new("engine-start")
            .small()
            .label(start_label)
            .accessibility_label(t!("detail-start-aria"))
            .child(shortcut_kbd("f5", "(F5)"))
            .tooltip(operate_start_tooltip())
            .loading(self.start_in_flight)
            .on_click(cx.listener(|view, _, window, cx| {
                view.start_selected_engine(window, cx);
            }));
        // F11: Start is the primary action only while the endpoint is stopped;
        // Stop and Test only mean something while it runs.
        if !is_running {
            start = start.primary();
        }
        if self.start_in_flight {
            start = start.child(
                div()
                    .id("engine-start-spinner")
                    .test_support()
                    .child(Spinner::new().small()),
            );
        }

        // DESIGN: an enabled control answers with an honest toast; it never
        // fakes its own unavailability. Stop and Test stay live on a stopped
        // row and the commit path says there is nothing to do.
        let stop = Button::new("engine-stop")
            .small()
            .label(t!("detail-stop"))
            .accessibility_label(t!("detail-stop-aria"))
            .child(shortcut_kbd("shift-f5", "(Shift-F5)"))
            .tooltip(operate_stop_tooltip())
            .on_click(cx.listener(|view, _, window, cx| {
                view.stop_selected_engine(window, cx);
            }));

        let mut test = Button::new("health-test")
            .small()
            .label(t!("detail-test"))
            .accessibility_label(t!("detail-test-aria"))
            .child(shortcut_kbd("ctrl-t", "(Ctrl-T)"))
            .tooltip(operate_test_tooltip())
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
        // holding Auto-start, Ping, Logs, Copy URLs, Copy SOCKS. The draft
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

    /// A context-menu row for one table action. The target row travels by id
    /// and is resolved when the handler runs, never at render.
    fn context_menu_item(
        id: &'static str,
        row_id: &str,
        view: &Entity<AppView>,
        multi: Option<usize>,
    ) -> PopupMenuItem {
        Self::context_menu_item_with_label(id, row_id, view, multi, None)
    }

    fn context_menu_item_with_label(
        id: &'static str,
        row_id: &str,
        view: &Entity<AppView>,
        multi: Option<usize>,
        label: Option<String>,
    ) -> PopupMenuItem {
        let def = action(id);
        let element_id = format!("rowctx-{}-{row_id}", def.id);
        let label = label.unwrap_or_else(|| def.label());
        let tip = def.tooltip().unwrap_or_else(|| def.label());
        let target = row_id.to_string();
        let view = view.clone();
        PopupMenuItem::element(move |_, _| {
            let tip = tip.clone();
            div()
                .id(element_id.clone())
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
            view.update(cx, |view, cx| {
                // A menu opened on a row inside a multi-selection acts on the
                // whole selection; anywhere else it keeps its single subject.
                let target = match multi {
                    Some(count) if count > 1 => ActionTarget::Selection,
                    _ => ActionTarget::Row(&target),
                };
                run_action(view, id, target, window, cx);
            });
        })
    }

    /// The row verbs that widen to the whole set, and the copy they wear when
    /// they do. Edit / Share / Duplicate stay row-scoped by nature.
    fn multi_row_label(id: &str, count: usize) -> Option<String> {
        if count <= 1 {
            return None;
        }
        match id {
            "start" => Some(t!("rowctx-multi-start", count = count)),
            "stop" => Some(t!("rowctx-multi-stop", count = count)),
            "test" => Some(t!("rowctx-multi-test", count = count)),
            "copy-urls" => Some(t!("rowctx-multi-copy-urls", count = count)),
            "copy-proxy" => Some(t!("rowctx-multi-copy-proxy", count = count)),
            "move" => Some(t!("rowctx-multi-move", count = count)),
            "delete" => Some(t!("rowctx-multi-delete", count = count)),
            _ => None,
        }
    }

    fn row_context_item(
        id: &'static str,
        row_id: &str,
        view: &Entity<AppView>,
        multi_count: usize,
    ) -> PopupMenuItem {
        let multi = Self::multi_row_label(id, multi_count);
        let multi = multi.map(|label| (multi_count, label));
        match multi {
            Some((count, label)) => {
                Self::context_menu_item_with_label(id, row_id, view, Some(count), Some(label))
            }
            None => Self::context_menu_item(id, row_id, view, None),
        }
    }

    /// R31: the row's `More` menu. The order is exhaustive and signed: every
    /// verb the surface map gives a row lives here, and nothing is dropped.
    ///
    /// The menu acts on the row it was opened on, unless that row is part of a
    /// multi-selection — then the verb that widens to the set widens.
    pub(crate) fn row_more_menu(
        menu: PopupMenu,
        row_id: &str,
        view: &Entity<AppView>,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let multi_count = {
            let app = view.read(cx);
            if app.is_selected(row_id) {
                app.selection_count()
            } else {
                1
            }
        };
        let mut menu = ["start", "stop", "test"]
            .into_iter()
            .fold(menu, |menu, id| {
                menu.item(Self::row_context_item(id, row_id, view, multi_count))
            });
        menu = menu
            .item(Self::row_context_item("edit", row_id, view, multi_count))
            .item(Self::row_context_item(
                "copy-urls",
                row_id,
                view,
                multi_count,
            ))
            .item(Self::row_context_item("move", row_id, view, multi_count))
            .item(Self::row_context_item(
                "assign-preset",
                row_id,
                view,
                multi_count,
            ))
            .item(Self::row_context_item("logs", row_id, view, multi_count))
            .item(Self::row_context_item("share", row_id, view, multi_count))
            .item(Self::row_context_item(
                "duplicate",
                row_id,
                view,
                multi_count,
            ))
            .item(Self::row_context_item("ping", row_id, view, multi_count));
        let reset_id = row_id.to_string();
        let reset_view = view.clone();
        menu = menu.submenu(t!("rowctx-reset"), window, cx, move |menu, _, _| {
            ["reset-traffic", "reset-usage"]
                .into_iter()
                .fold(menu, |menu, id| {
                    menu.item(Self::context_menu_item(id, &reset_id, &reset_view, None))
                })
        });
        menu = menu
            .item(Self::row_context_item(
                "auto-start",
                row_id,
                view,
                multi_count,
            ))
            .item(Self::row_context_item("delete", row_id, view, multi_count));
        menu
    }

    /// K1/A4-8: the operate row's overflow — ONE `…` trigger holding the five
    /// actions that no longer earn a button: Auto-start (writes the stored
    /// flag only), Ping, Logs, Copy URLs, Copy SOCKS. Row management (Edit /
    /// Delete / Duplicate / Reset) lives in the row's context menu — this
    /// button used to duplicate half of it above itself.
    pub(crate) fn render_row_menu(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let row_view = cx.entity();
        let auto_note = if config.enabled {
            t!("row-kebab-on")
        } else {
            t!("row-kebab-off")
        };

        Button::new("row-menu")
            .small()
            .label("…")
            .accessibility_label(t!("row-kebab-aria"))
            .tooltip(t!("row-kebab-tip"))
            .dropdown_menu(move |menu, _, _| {
                let view = row_view.clone();
                let note = auto_note.clone();
                ["auto-start", "ping", "logs", "copy-urls", "copy-proxy"]
                    .into_iter()
                    .fold(menu, |menu, id| {
                        menu.item(Self::selection_menu_item(id, Some(note.clone()), &view))
                    })
            })
    }

    /// An inspector-menu row for one table action against the selection. Only
    /// the auto-start row carries a state suffix; the label is the table's.
    fn selection_menu_item(
        id: &'static str,
        note: Option<String>,
        view: &Entity<AppView>,
    ) -> PopupMenuItem {
        let def = action(id);
        let element_id = format!("kebab-{}", def.id);
        let label = def.label();
        let text = match note {
            Some(note) => format!("{label} · {note}"),
            None => label.clone(),
        };
        let tip = def.tooltip().unwrap_or_else(|| def.label());
        let view = view.clone();
        PopupMenuItem::element(move |_, _| {
            let tip = tip.clone();
            div()
                .id(element_id.clone())
                .test_support()
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                .child(text.clone())
        })
        .on_click(move |_, window, cx| {
            view.update(cx, |view, cx| {
                run_action(view, id, ActionTarget::Selection, window, cx);
            });
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
    /// Links · Traffic, one row. Presets has no tab of its own here
    /// (Routing is its home). The Ctrl-1…5 keys live in Settings → Shortcuts,
    /// not on the tab labels.
    pub(crate) fn render_inspector_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::prelude::FluentBuilder as _;

        let view = cx.entity();
        let (tab_fg_active, tab_fg_muted, tab_bar_bg, pill_bg, radius) = {
            let theme = cx.theme();
            (
                theme.tab_active_foreground,
                theme.muted_foreground,
                theme.tokens.tab_bar_segmented,
                // Task 11: the selected tab is the inspector's active pill, so
                // it wears the accent wash instead of a neutral surface.
                theme.list_active,
                theme.radius,
            )
        };
        let pill_radius = (radius - px(2.)).max(px(0.));

        // K20/D1: the five labels size the row instead of splitting it into
        // five equal `flex_1` slots — that clipped the last tab whole.

        let tabs = [
            InspectorTab::Details,
            InspectorTab::Health,
            InspectorTab::Logs,
            InspectorTab::Links,
            InspectorTab::Traffic,
        ];

        let mut buttons = Vec::with_capacity(tabs.len());
        for (index, tab) in tabs.into_iter().enumerate() {
            let label = inspector_tab_label(tab);
            let slug = tab.a11y_label().to_lowercase();
            let selected = self.inspector_tab == tab;
            buttons.push(
                div()
                    .id(format!("inspector-tab-button-{slug}"))
                    .test_support()
                    .role(Role::Tab)
                    .aria_label(label.clone())
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
                            .text_size(rems(TAB_LABEL_REM))
                            .line_height(relative(1.0))
                            .whitespace_nowrap()
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
                    .title(t!("detail-endpoint-failed")),
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
