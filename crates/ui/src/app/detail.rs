//! The inspector: selected-row detail, operate row, connections and traffic tabs.
//! Belongs: everything rendered in the right-hand inspector.
//! Not here: library rows or dialog bodies.
use crate::*;
use gpui_kit::base::{v_virtual_list, Scrollbar, ScrollbarAxis};
use gpui_kit::component::scroll::ScrollableElement as _;

use super::console::console_height;

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

/// B3: the panes that list rows build a virtual window instead of a full
/// subtree. Uniform row heights keep the size cache exact: a declared height
/// that is too small clips a row, so each row is pinned to one line (log and
/// traffic text truncates, the link card is a fixed two-line block).
pub(crate) const PANE_LOG_ROW_H: f32 = 20.;
pub(crate) const PANE_LINK_ROW_H: f32 = 48.;
/// A row list never eats the whole pane: the console body caps the list to a
/// share of what it can show and scrolls itself past it. A short list keeps
/// its natural height.
const PANE_LIST_SHARE: f32 = 0.6;
/// The panel's own height is only known after the first layout; until then the
/// list falls back to this so the pane is never blank on its first frame.
const PANE_LIST_FALLBACK_H: f32 = 320.;

/// B3 acceptance probe: counts the rows a console pane actually builds.
/// Debug-only — release builds compile the counter away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleRows {
    Logs = 0,
    Links = 1,
}

// Per-thread, because a frame runs on the thread that draws it and the suite
// mounts views in parallel.
#[cfg(any(debug_assertions, test))]
thread_local! {
    static PANE_ROWS_BUILT: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0; 2]) };
}

pub(crate) fn note_pane_row(pane: ConsoleRows) {
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
pub(crate) fn pane_rows_built(pane: ConsoleRows) -> usize {
    PANE_ROWS_BUILT.with(|built| built.get()[pane as usize])
}

#[cfg(test)]
pub(crate) fn reset_pane_rows_built() {
    PANE_ROWS_BUILT.with(|built| built.set([0; 2]));
}

/// B3: the pane's row area. The list is handed the visible range only, so rows
/// outside the window are never built; the paired handle remembers the offset
/// and the scrollbar makes the extra scroll range visible in a narrow panel.
pub(crate) fn pane_rows(
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
        self.close_console_focus_table(window, cx);
        cx.notify();
        true
    }

    pub(crate) fn render_detail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // R6: the inspector follows the selection. Exactly one selected row
        // shows its sections; two or more show the aggregate; zero (or an
        // empty library) shows the empty state.
        match self.selection_count() {
            0 => {
                if self.config_count(cx) == 0 {
                    return div().id("config-detail").test_support().into_any_element();
                }
                self.render_detail_empty(cx).into_any_element()
            }
            1 => {
                let Some(config) = self.selected_config(cx) else {
                    return self.render_detail_empty(cx).into_any_element();
                };
                self.render_detail_selected(window, cx, &config)
                    .into_any_element()
            }
            _ => self.render_inspector_aggregate(cx).into_any_element(),
        }
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
        // Slice 6: one stacked scroll — Identity, Endpoint, Health, Actions —
        // no tabs. Health and traffic updates repaint inside; only a target
        // CHANGE resets the scroll, and that resets live in the selection
        // handler, never here.
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
            .child(
                div()
                    .id("config-detail-body")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // A scroll container must own a BOUNDED height: `flex_1`
                    // takes the space left over so the body scrolls instead
                    // of growing past the panel (which clipped the operate
                    // row and reported a 0px scroll range).
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .track_scroll(&self.detail_scroll)
                    // Appears on hover/wheel: a clipped line alone is a poor
                    // scroll cue once the body carries the whole pane.
                    .vertical_scrollbar(&self.detail_scroll)
                    .child(self.render_inspector_identity(cx, config))
                    .child(self.render_inspector_endpoint(window, cx, config))
                    .child(self.render_inspector_health_section(cx, config))
                    .child(self.render_inspector_actions(cx, config))
                    .child(self.render_inspector_console_link(cx)),
            )
            .into_any_element()
    }

    /// Slice 6: the console body's row-area height. A short list keeps its
    /// natural height; past its share of the dock the list scrolls itself.
    /// The dock's height is fixed (240/220), so the share of it a list may
    /// paint is a constant, not a measurement — one less layout read per
    /// frame, and the dock never resizes anyway.
    pub(crate) fn console_list_height(&self, rows: usize, row_h: f32) -> f32 {
        if rows == 0 {
            return 0.;
        }
        let viewport = console_height(self.density) - 36.; // header row
        let viewport = if viewport > 0. {
            viewport
        } else {
            PANE_LIST_FALLBACK_H
        };
        (rows as f32 * row_h).min((viewport * PANE_LIST_SHARE).max(row_h * 3.))
    }

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
        _window: &mut Window,
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
        // Custom-config rows read their listens from JSON; the stored bind and
        // port are display only, so there is nothing here to edit.
        let endpoint_editable = !is_custom_config_protocol(&config.protocol);
        let inline_editing = endpoint_editable
            && self.endpoint_draft.as_ref().is_some_and(|draft| {
                draft.row_id == config.id && draft.host == EndpointHost::InspectorInline
            });
        let inline_form = inline_editing
            .then(|| self.endpoint_frame(cx))
            .flatten()
            .map(|frame| render_endpoint_form(cx.entity(), &frame, cx));

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
                            let edit_id = config.id.clone();
                            let edit_view = cx.entity();
                            let mut row = div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .w_full()
                                .flex_shrink_0()
                                .child(div().flex_1().min_w_0());
                            if endpoint_editable && !inline_editing {
                                row = row.child(
                                    Button::new("detail-endpoint-edit")
                                        .small()
                                        .label(t!("endpoint-edit"))
                                        .accessibility_label(t!(
                                            "endpoint-edit-aria",
                                            name = config.name.as_str()
                                        ))
                                        .tooltip(t!("endpoint-edit-tip"))
                                        .on_click(move |_, window, cx| {
                                            edit_view.update(cx, |view, cx| {
                                                view.open_endpoint_draft(
                                                    &edit_id,
                                                    EndpointHost::InspectorInline,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }),
                                );
                            }
                            row.child(
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
                        // The one editor, inline, directly under the values.
                        .children(inline_form),
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
    /// Slice 6, Identity section: the old Details content — name (tooltip
    /// carries the whole name), Protocol, Group, Source (`Manual` when the
    /// row has no subscription), Upstream, and Preset when one is attached.
    pub(crate) fn render_inspector_identity(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let delegate = self.libraries.read(cx).delegate();
        let source = crate::row_source(config, &delegate.sub_labels);
        let preset = config
            .preset_id
            .as_deref()
            .and_then(|id| delegate.preset_names.get(id))
            .cloned();
        let upstream = self.cached_upstream(cx, config);
        let mut list = DescriptionList::new()
            .bordered(false)
            .small()
            .columns(2)
            .label_width(px(72.))
            .item(t!("detail-identity-name"), config.name.to_string(), 1)
            .item(t!("detail-identity-protocol"), config.protocol.clone(), 1)
            .item(t!("detail-identity-group"), config.group.clone(), 1)
            .item(t!("detail-identity-source"), source, 1)
            .item(t!("detail-endpoint-upstream"), upstream, 1);
        if let Some(preset) = preset {
            list = list.item(t!("detail-identity-preset"), preset, 1);
        }
        div()
            .id("inspector-identity")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.render_detail_header(cx, config))
            .child(list)
    }

    /// Slice 6, Endpoint section: the same values the old endpoints block
    /// read, still hosting slice 5's one inline draft. No second editor.
    pub(crate) fn render_inspector_endpoint(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        self.render_detail_endpoints(window, cx, config)
    }

    /// Slice 6, Health section: state, latency probes, the failure reason,
    /// the probe/usage block and the honest history line. `Test` rides the
    /// actions section; this section only reports.
    pub(crate) fn render_inspector_health_section(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        let mut section = div()
            .id("inspector-health-section")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.render_detail_health(cx, config));
        if self.detail_failed(config) {
            section = section.child(
                div().id("config-health-alert").test_support().child(
                    Alert::error(
                        "config-health-alert-box",
                        self.health_store.detail_line(&config.id),
                    )
                    .title(t!("detail-endpoint-failed")),
                ),
            );
        }
        section
            .child(self.render_detail_usage(cx, config))
            .child(self.render_detail_history(cx, config))
    }

    /// Slice 6, Actions section: Start/Stop (Start is primary while stopped),
    /// Test, and the row's `More` kebab — the same menu the row uses, not a
    /// second one. The F5/Shift+F5/Ctrl+T hints stay pinned.
    pub(crate) fn render_inspector_actions(
        &self,
        cx: &mut Context<Self>,
        config: &ProxyConfig,
    ) -> impl IntoElement {
        div()
            .id("inspector-actions")
            .test_support()
            .child(self.render_detail_actions(cx, config))
    }

    /// The inspector's one console entry: `Open in console →` opens the dock
    /// on Logs with scope `Selected` and an empty console filter.
    pub(crate) fn render_inspector_console_link(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        div()
            .id("inspector-console-link")
            .test_support()
            .flex()
            .flex_row()
            .justify_end()
            .child(
                Button::new("inspector-open-console")
                    .small()
                    .ghost()
                    .label(t!("inspector-open-console"))
                    .tooltip(t!("inspector-open-console"))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| {
                            view.open_console_scoped_to_selection(true, cx);
                        });
                    }),
            )
    }

    /// R6: the 2+ selection aggregate. Counts only — no action buttons, no
    /// endpoint editor, no second batch panel. `{N} hidden` clears filters,
    /// exactly as the batch bar's hidden count does.
    pub(crate) fn render_inspector_aggregate(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.selection_count();
        let (running, failed, stopped) = {
            let mut counts = (0usize, 0usize, 0usize);
            let delegate = self.libraries.read(cx).delegate();
            for id in self.selection.iter() {
                let Some(row) = delegate.row_by_id(id) else {
                    continue;
                };
                if self.engine.is_running(id) {
                    counts.0 += 1;
                } else if self.detail_failed(row) {
                    counts.1 += 1;
                } else {
                    counts.2 += 1;
                }
            }
            counts
        };
        let hidden = self.selection_hidden_count(cx);
        let selected = t!("inspector-selected-count", count = count);
        let running = t!("inspector-running-count", count = running);
        let failed = t!("inspector-failed-count", count = failed);
        let stopped = t!("inspector-stopped-count", count = stopped);
        let mut section = div()
            .id("inspector-aggregate")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .child(
                div()
                    .id("inspector-selected-count")
                    .test_support()
                    .aria_label(selected.clone())
                    .text_sm()
                    .child(selected),
            )
            .child(
                div()
                    .id("inspector-running-count")
                    .test_support()
                    .aria_label(running.clone())
                    .text_sm()
                    .child(running),
            )
            .child(
                div()
                    .id("inspector-failed-count")
                    .test_support()
                    .aria_label(failed.clone())
                    .text_sm()
                    .child(failed),
            )
            .child(
                div()
                    .id("inspector-stopped-count")
                    .test_support()
                    .aria_label(stopped.clone())
                    .text_sm()
                    .child(stopped),
            );
        if hidden > 0 {
            let view = cx.entity();
            section = section.child(
                Button::new("inspector-hidden-count")
                    .small()
                    .ghost()
                    .label(t!("selection-hidden", count = hidden))
                    .tooltip(t!("selection-hidden-tip"))
                    .on_click(move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.reveal_hidden_selection(window, cx);
                        });
                    }),
            );
        }
        section.child(
            div()
                .id("inspector-use-selection-bar")
                .test_support()
                .aria_label(t!("inspector-use-selection-bar"))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(t!("inspector-use-selection-bar")),
        )
    }
}
