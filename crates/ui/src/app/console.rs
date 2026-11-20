//! The console dock: logs, connections and traffic in one child of main,
//! above the engine bar. Slice 6 moves the inspector's three evidence panes
//! here; the inspector keeps only its stacked sections.
//!
//! Belongs: the dock's chrome (header, tabs, scope, filter, pause/clear) and
//! its three tab bodies. Not here: the log sheet (the `Filters…` button opens
//! it), the engine bar, or any commit path.

use crate::*;

use super::detail::{note_console_row, pane_rows, ConsoleRows, PANE_LINK_ROW_H, PANE_LOG_ROW_H};

/// R4/fork 2: the dock is fixed-height. Comfortable is the default; compact
/// is what the density row asks for. It never resizes and never persists.
pub fn console_height(density: Density) -> f32 {
    match density {
        Density::Compact => 220.,
        Density::Comfortable => 240.,
    }
}

/// How wide the quick filter may get before it stops shrinking and the bar
/// just truncates. At 1100px the filter shrinks first; tabs and buttons wrap
/// never (one flex row, `flex_nowrap`, each control `flex_shrink_0` except
/// the filter).
pub const CONSOLE_FILTER_MIN_PX: f32 = 80.;
pub const CONSOLE_FILTER_MAX_PX: f32 = 180.;

/// The one entry point render.rs calls. A collapsed dock is no element at
/// all: the engine bar never moves because nothing above it changed height.
pub fn render_console_slot(
    view: &mut AppView,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Option<AnyElement> {
    if !view.console.open {
        return None;
    }
    Some(render_console(view, window, cx))
}

pub fn render_console(
    view: &mut AppView,
    _window: &mut Window,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let height = console_height(view.density);
    div()
        .id("console")
        .test_support()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h(px(height))
        .min_h_0()
        .border_y_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(render_console_header(view, cx))
        .child(match view.console.tab {
            ConsoleTab::Logs => render_console_logs(view, cx).into_any_element(),
            ConsoleTab::Connections => render_console_connections(view, cx).into_any_element(),
            ConsoleTab::Traffic => render_console_traffic(view, cx).into_any_element(),
        })
        .into_any_element()
}

/// Header, left to right: title, tabs, scope, quick filter, pause/clear
/// (Logs only), close. One non-wrapping row — the filter is the only
/// shrinkable item.
pub fn render_console_header(view: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let view_entity = cx.entity();

    // The kit's TabBar owns its tabs' element ids (`ix`), so it cannot carry
    // the stable `console-tab-*` / `console-scope-*` ids the suite pins. These
    // stay hand-rolled but expose the same tab-list surface the kit's base
    // tabs do: a TabList parent and a position/size on every Tab.
    let mut tabs = div()
        .id("console-tabs")
        .test_support()
        .role(Role::TabList)
        .aria_label(t!("console-tabs-label"))
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .flex_shrink_0();
    for (ix, tab) in [
        ConsoleTab::Logs,
        ConsoleTab::Connections,
        ConsoleTab::Traffic,
    ]
    .into_iter()
    .enumerate()
    {
        let active = view.console.tab == tab;
        let label = match tab {
            ConsoleTab::Logs => t!("console-tab-logs"),
            ConsoleTab::Connections => t!("console-tab-connections"),
            ConsoleTab::Traffic => t!("console-tab-traffic"),
        };
        let slug = match tab {
            ConsoleTab::Logs => "logs",
            ConsoleTab::Connections => "connections",
            ConsoleTab::Traffic => "traffic",
        };
        let theme = cx.theme();
        let (fg, bg) = if active {
            (theme.tab_active_foreground, theme.list_active)
        } else {
            (theme.muted_foreground, gpui::transparent_black())
        };
        tabs = tabs.child(
            div()
                .id(format!("console-tab-{slug}"))
                .test_support()
                .role(Role::Tab)
                .aria_label(label.clone())
                .aria_selected(active)
                .aria_position_in_set(ix + 1)
                .aria_size_of_set(3)
                // `tab_stop` alone registers nothing — the tab-stop map only
                // holds elements with a focus handle, so `focusable` is what
                // puts the chip in the Tab order.
                .focusable()
                .tab_stop(true)
                .on_click({
                    let entity = view_entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |view, cx| {
                            view.open_console_tab(tab, cx);
                        });
                    }
                })
                .px_2()
                .py_0p5()
                .rounded_sm()
                .text_sm()
                .text_color(fg)
                .bg(bg)
                .hover(|style| style.text_color(theme.tab_active_foreground))
                .child(label),
        );
    }

    // The scope switch is two mutually exclusive radio choices, not a tab
    // list: choosing one does not switch a panel (the section tabs above do
    // that). `aria_selected` says which is active; the keyboard reaches both
    // through Tab (focusable + tab_stop) and the two palette verbs that call
    // `set_console_scope`.
    let mut scope = div()
        .id("console-scope")
        .test_support()
        .aria_label(t!("console-scope-label"))
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(t!("console-scope-tip"))).into())
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .flex_shrink_0();
    for (scope_value, key, slug) in [
        (ConsoleScope::All, "console-scope-all", "all"),
        (ConsoleScope::Selected, "console-scope-selected", "selected"),
    ] {
        let active = view.console.scope == scope_value;
        let label = crate::i18n::translate(key, None);
        let theme = cx.theme();
        let (fg, bg) = if active {
            (theme.tab_active_foreground, theme.list_active)
        } else {
            (theme.muted_foreground, gpui::transparent_black())
        };
        scope = scope.child(
            div()
                .id(format!("console-scope-{slug}"))
                .test_support()
                .role(Role::RadioButton)
                .aria_label(label.clone())
                .aria_selected(active)
                // See the section tabs above: `focusable` registers the
                // handle the tab-stop map needs.
                .focusable()
                .tab_stop(true)
                .on_click({
                    let entity = view_entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |view, cx| {
                            view.set_console_scope(scope_value, cx);
                        });
                    }
                })
                .px_2()
                .py_0p5()
                .rounded_sm()
                .text_sm()
                .text_color(fg)
                .bg(bg)
                .hover(|style| style.text_color(theme.tab_active_foreground))
                .child(label),
        );
    }

    // The scope empty state is per-tab body, but the scope switch itself never
    // bounces back to All — the brief pins that the user chooses the way out.
    let filter = div()
        .flex()
        .flex_row()
        .items_center()
        .min_w(px(CONSOLE_FILTER_MIN_PX))
        .max_w(px(CONSOLE_FILTER_MAX_PX))
        .flex_1()
        .min_w_0();

    let paused = view.console.paused.is_some();
    let logs_only = view.console.tab == ConsoleTab::Logs;
    let mut actions = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .flex_shrink_0();
    // Pause/Clear are Logs-only verbs; on the other tabs they stay visible and
    // disabled rather than disappearing, so the header never reflows between
    // tabs (the brief pins that tabs and buttons never wrap).
    if logs_only {
        actions = actions.child(
            Button::new("console-filters")
                .small()
                .ghost()
                .label(t!("console-filters"))
                .tooltip(t!("console-filters-tip"))
                .on_click({
                    let entity = view_entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |view, cx| {
                            view.open_logs_dialog(window, cx);
                        });
                    }
                }),
        );
    }
    let pause_label = if paused {
        t!("console-resume")
    } else {
        t!("console-pause")
    };
    actions = actions.child(
        Button::new("console-pause")
            .small()
            .ghost()
            // Pausing is Logs-only, but a session paused on Logs stays
            // paused across Ctrl+2/3 hops — Resume must stay operable on
            // the other tabs or the freeze has no way out.
            .disabled(!logs_only && !paused)
            .label(pause_label)
            .on_click({
                let entity = view_entity.clone();
                move |_, _, cx| {
                    entity.update(cx, |view, cx| {
                        if view.console.paused.is_some() {
                            view.resume_console(cx);
                        } else {
                            view.pause_console(cx);
                        }
                    });
                }
            }),
    );
    if paused {
        actions = actions.child(
            div()
                .id("console-paused-badge")
                .test_support()
                .flex_shrink_0()
                .child(
                    gpui_kit::component::tag::Tag::secondary()
                        .small()
                        // Only Logs is frozen; Connections and Traffic keep
                        // updating. Say so instead of claiming the whole
                        // console is paused.
                        .child(t!("console-paused-logs")),
                ),
        );
    }
    actions = actions.child(
        Button::new("console-clear")
            .small()
            .ghost()
            .disabled(!logs_only)
            .label(t!("console-clear"))
            .on_click({
                let entity = view_entity.clone();
                move |_, _, cx| {
                    entity.update(cx, |view, cx| {
                        view.clear_console_logs(cx);
                    });
                }
            }),
    );

    let close_view = view_entity;
    div()
        .id("console-header")
        .test_support()
        .flex()
        .flex_row()
        .flex_nowrap()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .id("console-title")
                .test_support()
                .text_sm()
                .flex_shrink_0()
                .child(t!("console-title")),
        )
        .child(tabs)
        .child(div().w(px(1.)).h_4().flex_shrink_0().bg(cx.theme().border))
        .child(scope)
        .child(filter.child(
            // The kit's Input renders its placeholder from the InputState;
            // the state was built with the localized placeholder already.
            Input::new(&view.console_filter_input).small(),
        ))
        .child(actions)
        .child(
            Button::new("console-close")
                .small()
                .ghost()
                .label(t!("console-close"))
                .on_click(move |_, window, cx| {
                    close_view.update(cx, |view, cx| {
                        view.toggle_console(window, cx);
                    });
                }),
        )
        .into_any_element()
}

/// The scope row every tab shows when `Selected` is empty: one honest line,
/// no bounce back to `All`.
pub fn render_scope_empty(cx: &App) -> AnyElement {
    div()
        .id("console-empty-selected")
        .test_support()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(t!("console-empty-selected"))
        .into_any_element()
}

pub fn render_console_logs(view: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    if view.console.scope == ConsoleScope::Selected && view.selection_count() == 0 {
        return render_scope_empty(cx);
    }
    let lines = view.console_log_lines(cx);
    if lines.is_empty() {
        // One reason per empty state: with any filter set — the quick filter
        // or a sheet level/include/exclude/regex — "No log lines yet."
        // contradicts "No lines match". The quick filter is the only one
        // clearable from here, so it gets the button; a sheet-only filter
        // points at the sheet.
        let quick = view.console.filter.trim().is_empty();
        let sheet_filtered = !view.logs.filter.trim().is_empty()
            || !view.logs.exclude.trim().is_empty()
            || !view.logs.regex.trim().is_empty()
            || view.logs.level.is_some();
        let mut empty = div()
            .id("console-empty-logs")
            .test_support()
            .text_sm()
            .text_color(cx.theme().muted_foreground);
        if view.console.paused.is_some() {
            // The frozen snapshot may be empty while new records stream in;
            // "No log lines yet." would be a lie. Point at Resume.
            empty = empty.child(t!("console-empty-paused"));
        } else if quick && !sheet_filtered {
            empty = empty.child(t!("console-empty-logs"));
        } else if !quick {
            let view_entity = cx.entity();
            empty = empty
                .child(t!("console-no-match", filter = view.console.filter.trim()))
                .child(
                    Button::new("console-clear-filter")
                        .small()
                        .ghost()
                        .label(t!("console-clear-filter"))
                        .on_click(move |_, _, cx| {
                            view_entity.update(cx, |view, cx| {
                                view.clear_console_filter(cx);
                            });
                        }),
                );
        } else {
            empty = empty.child(t!("console-no-match-sheet"));
        }
        return div()
            .id("console-logs")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .child(empty)
            .into_any_element();
    }

    let count = lines.len();
    let height = view.console_list_height(count, PANE_LOG_ROW_H);
    div()
        .id("console-logs")
        .test_support()
        .flex()
        .flex_col()
        .min_h_0()
        .child(pane_rows(
            cx,
            &mut view.console_logs,
            "console-log-list",
            height,
            count,
            PANE_LOG_ROW_H,
            move |_view, ix, cx| {
                let line = lines.get(ix).cloned().unwrap_or_default();
                note_console_row(ConsoleRows::Logs);
                // The console prepends the clock, so the row's `[level]` tag is
                // the first bracket rather than the first byte of the line.
                // Severity tones only the tag: timestamp and message stay
                // foreground so an error burst reads as text with red tags,
                // not a solid red block.
                let level = line
                    .find('[')
                    .map(|at| log_line_level(&line[at..]))
                    .unwrap_or(LogLevel::Info);
                let tip = line.clone();
                let tone = log_level_text_color(level, cx.theme());
                let parts = match (line.find('['), line.find(']')) {
                    (Some(start), Some(end)) if end > start => Some((
                        line[..start].to_string(),
                        line[start..=end].to_string(),
                        line[end + 1..].to_string(),
                    )),
                    _ => None,
                };
                let row = div()
                    .id(format!("console-log-line-{ix}"))
                    .test_support()
                    .w_full()
                    .h(px(PANE_LOG_ROW_H))
                    // Block-flow children stack vertically inside the 20px
                    // row; the row must be a flex line so clock, tag and
                    // message share it. The message span carries the
                    // ellipsis (min_w_0), so the clock and tag never clip.
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_sm()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into());
                match parts {
                    Some((clock, tag, message)) => row
                        .child(div().flex_shrink_0().child(clock))
                        .child(div().flex_shrink_0().text_color(tone).child(tag))
                        .child(div().min_w_0().truncate().child(message))
                        .into_any_element(),
                    None => row.child(line).into_any_element(),
                }
            },
        ))
        .into_any_element()
}

pub fn render_console_connections(view: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    if view.console.scope == ConsoleScope::Selected && view.selection_count() == 0 {
        return render_scope_empty(cx);
    }
    // Membership is the only thing the flow builders read, and the
    // selection set already holds hidden rows — no need to re-derive the
    // visible-order copy (O(rows) id clones) on every repaint.
    let empty_set = HashSet::new();
    let selected: &HashSet<String> = if view.console.scope == ConsoleScope::Selected {
        &view.selection
    } else {
        &empty_set
    };
    let rows = build_conn_rows(
        &view.conn.snapshot,
        &view.conn.delta,
        selected,
        view.console.scope == ConsoleScope::Selected,
        view.conn.sort_desc,
    );
    let dest_width_px = view.console.runtime.conn_dest_width;

    if rows.is_empty() {
        return div()
            .id("console-connections")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .child(
                div()
                    .id("console-empty-connections")
                    .test_support()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("console-empty-connections")),
            )
            .into_any_element();
    }

    // The quick filter narrows by destination, exit and protocol — the same
    // substring contract the logs and traffic tabs honor. Borrowed exits:
    // every scoped flow walks this pass, not just the visible ones.
    let quick = view.console.filter.trim().to_lowercase();
    let keep: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            if quick.is_empty() {
                return true;
            }
            let (name, bind, protocol) = view.conn.exit_ref(row.row_id.as_deref());
            row.dest.to_lowercase().contains(&quick)
                || bind.to_lowercase().contains(&quick)
                || name.to_lowercase().contains(&quick)
                || protocol.to_lowercase().contains(&quick)
        })
        .map(|(ix, _)| ix)
        .collect();

    if keep.is_empty() {
        let view_entity = cx.entity();
        return div()
            .id("console-connections")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .child(
                div()
                    .id("console-no-match-connections")
                    .test_support()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("console-no-match", filter = view.console.filter.trim()))
                    .child(
                        Button::new("console-clear-filter")
                            .small()
                            .ghost()
                            .label(t!("console-clear-filter"))
                            .on_click(move |_, _, cx| {
                                view_entity.update(cx, |view, cx| {
                                    view.clear_console_filter(cx);
                                });
                            }),
                    ),
            )
            .into_any_element();
    }

    let count = keep.len();
    let height = view.console_list_height(count, PANE_LINK_ROW_H);
    let sort_desc = view.conn.sort_desc;
    // The rows sort by cumulative traffic only, so the control speaks the old
    // inspector's traffic wording instead of a timestamp order it never had.
    let sort_label = if sort_desc {
        t!("detail-sort-desc")
    } else {
        t!("detail-sort-asc")
    };
    let sort_view = cx.entity();
    div()
        .id("console-connections")
        .test_support()
        .flex()
        .flex_col()
        .min_h_0()
        .child(
            div()
                .id("console-conn-toolbar")
                .test_support()
                .flex()
                .flex_row()
                .flex_shrink_0()
                .px_2()
                .py_1()
                .child(
                    Button::new("console-conn-sort")
                        .small()
                        .ghost()
                        .label(sort_label)
                        .tooltip(t!("detail-sort-tip"))
                        .on_click(move |_, _, cx| {
                            sort_view.update(cx, |view, cx| {
                                view.conn.sort_desc = !view.conn.sort_desc;
                                cx.notify();
                            });
                        }),
                ),
        )
        .child(pane_rows(
            cx,
            &mut view.console_connections,
            "console-conn-list",
            height,
            count,
            PANE_LINK_ROW_H,
            move |view, painted_ix, cx| {
                let Some(rows_ix) = keep.get(painted_ix).copied() else {
                    return div().into_any_element();
                };
                let Some(row) = rows.get(rows_ix) else {
                    return div().into_any_element();
                };
                let (name, bind, protocol) = view.conn.exit_of(row.row_id.as_deref());
                let dest = row.dest.clone();
                let dest_display = if dest.is_empty() {
                    t!("detail-no-destination")
                } else {
                    redact_secrets(&dest)
                };
                let dest_value = if dest.is_empty() {
                    String::new()
                } else {
                    dest_display.clone()
                };
                let mut dest_cell = div()
                    .id(format!("console-conn-col-dest-{rows_ix}"))
                    .test_support()
                    .w(px(dest_width_px))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        Button::new(format!("console-conn-copy-{rows_ix}"))
                            .small()
                            .label(dest_display)
                            .tooltip(t!("detail-copy-destination"))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.copy_conn_cell(
                                    ConnCopyTarget::Dest(rows_ix),
                                    &dest_value,
                                    window,
                                    cx,
                                );
                            })),
                    );
                if view.conn.copied == Some(ConnCopyTarget::Dest(rows_ix)) {
                    dest_cell = dest_cell.child(
                        div()
                            .id(format!("console-conn-copied-dest-{rows_ix}"))
                            .test_support()
                            .text_sm()
                            .child(t!("detail-copied")),
                    );
                }
                let endpoint_value = bind.clone();
                let endpoint_claimed = row.row_id.is_some();
                let mut endpoint_button = Button::new(format!("console-conn-endpoint-{rows_ix}"))
                    .small()
                    .label(bind);
                if endpoint_claimed {
                    endpoint_button = endpoint_button
                        .tooltip(t!("detail-copy-endpoint"))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.copy_conn_cell(
                                ConnCopyTarget::Endpoint(rows_ix),
                                &endpoint_value,
                                window,
                                cx,
                            );
                        }));
                }
                let mut meta = div()
                    .id(format!("console-conn-meta-{rows_ix}"))
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
                if view.conn.copied == Some(ConnCopyTarget::Endpoint(rows_ix)) {
                    meta = meta.child(
                        div()
                            .id(format!("console-conn-copied-endpoint-{rows_ix}"))
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
                note_console_row(ConsoleRows::Links);
                div()
                    .id(("console-conn-row", painted_ix))
                    .h(px(PANE_LINK_ROW_H))
                    .w_full()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(dest_cell)
                    .child(meta)
                    .into_any_element()
            },
        ))
        .into_any_element()
}

pub fn render_console_traffic(view: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    if view.console.scope == ConsoleScope::Selected && view.selection_count() == 0 {
        return render_scope_empty(cx);
    }
    // Membership only — see render_console_connections.
    let empty_set = HashSet::new();
    let selected: &HashSet<String> = if view.console.scope == ConsoleScope::Selected {
        &view.selection
    } else {
        &empty_set
    };
    let (session_rx, session_tx) =
        session_traffic(&view.conn.snapshot, selected, view.console.scope);

    // Per-exit session flows for the current scope. The stored totals stay on
    // the engine bar; the console reads the live snapshot, so an idle session
    // shows honest zeros instead of last week's history.
    let mut per_exit: HashMap<String, (u64, u64)> = HashMap::new();
    for (_, entry) in conn_rows_in_scope(&view.conn.snapshot, selected, view.console.scope) {
        if let Some(row_id) = entry.row_id.clone() {
            let slot = per_exit.entry(row_id).or_insert((0, 0));
            slot.0 = slot.0.saturating_add(entry.rx_bytes);
            slot.1 = slot.1.saturating_add(entry.tx_bytes);
        }
    }
    let exits: Vec<ExitTraffic> = per_exit
        .into_iter()
        .filter_map(|(row_id, (rx, tx))| {
            let row = view.conn.exit_index.get(&row_id)?;
            Some(ExitTraffic {
                row_id,
                name: row.name.clone(),
                bind: row.bind.clone(),
                rx_bytes: rx,
                tx_bytes: tx,
            })
        })
        .collect();

    let quick = view.console.filter.trim().to_lowercase();
    let bars = traffic_top_split(&exits, 8);
    let bars: Vec<ExitTraffic> = if quick.is_empty() {
        bars
    } else {
        bars.into_iter()
            .filter(|exit| {
                exit.name.to_lowercase().contains(&quick)
                    || exit.bind.to_lowercase().contains(&quick)
            })
            .collect()
    };
    let max = console_traffic_bar_max(&bars);

    let mut body = div()
        .id("console-traffic")
        .test_support()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .overflow_y_scroll()
        .track_scroll(&view.console_traffic.scroll)
        .min_h_0()
        .flex_1()
        .child(
            div()
                .id("console-traffic-summary")
                .test_support()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .id("console-traffic-rx")
                        .test_support()
                        .aria_label(t!("console-traffic-rx", bytes = format_bytes(session_rx)))
                        .text_sm()
                        .child(t!("console-traffic-rx", bytes = format_bytes(session_rx))),
                )
                .child(
                    div()
                        .id("console-traffic-tx")
                        .test_support()
                        .aria_label(t!("console-traffic-tx", bytes = format_bytes(session_tx)))
                        .text_sm()
                        .child(t!("console-traffic-tx", bytes = format_bytes(session_tx))),
                )
                .child(
                    div()
                        .id("console-traffic-rows")
                        .test_support()
                        .aria_label(t!("console-traffic-rows", count = bars.len()))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("console-traffic-rows", count = bars.len())),
                ),
        );

    if bars.is_empty() && !quick.is_empty() {
        let view_entity = cx.entity();
        body = body
            .child(t!("console-no-match", filter = view.console.filter.trim()))
            .child(
                Button::new("console-clear-filter")
                    .small()
                    .ghost()
                    .label(t!("console-clear-filter"))
                    .on_click(move |_, _, cx| {
                        view_entity.update(cx, |view, cx| {
                            view.clear_console_filter(cx);
                        });
                    }),
            );
        return body
            .child(render_console_runtime(view, cx))
            .into_any_element();
    }
    if bars.is_empty() {
        return body
            .child(
                div()
                    .id("console-empty-traffic")
                    .test_support()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("console-empty-traffic")),
            )
            .child(render_console_runtime(view, cx))
            .into_any_element();
    }

    for exit in &bars {
        let percent = if max == 0 {
            0.0
        } else {
            (exit.total() as f32 / max as f32 * 100.0).clamp(0.0, 100.0)
        };
        let share = percent.round() as usize;
        // No `.loading()`: the kit's loading mode loops an idle animation
        // forever, which the "nothing loops" motion rule forbids. A value
        // change still runs the kit's one-shot width transition, bounded and
        // reduce-motion aware — that is intended feedback for a fresh sample.
        body = body.child(
            div()
                .id(format!("console-traffic-exit-{}", exit.row_id))
                .test_support()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_sm()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(format!("{} · {}", exit.name, exit.bind)),
                )
                .child(
                    Progress::new(format!("console-traffic-bar-{}", exit.row_id))
                        .value(percent)
                        .small()
                        .flex_1()
                        .min_w_0()
                        .accessibility_label(t!(
                            "console-traffic-bar-aria",
                            name = exit.name.as_str(),
                            percent = share
                        )),
                )
                .child(div().flex_shrink_0().child(t!(
                    "console-traffic-exit-bytes",
                    down = format_bytes(exit.rx_bytes),
                    up = format_bytes(exit.tx_bytes)
                ))),
        );
    }

    // The Runtime group: the old runtime-stats dialog's lines, in the same
    // order, now scoped inside the Traffic tab.
    body.child(render_console_runtime(view, cx))
        .into_any_element()
}

/// The old `Runtime stats` dialog's lines: uptime, endpoints, running binds,
/// session delta, subscriptions, database, selected row, and the shard rows.
pub fn render_console_runtime(view: &mut AppView, cx: &mut Context<AppView>) -> AnyElement {
    let uptime = format_uptime(view.started_at.elapsed().as_secs());
    let running = view.engine.running_count();
    let stats = &view.console.runtime;
    let total = stats.rows;
    let running_list = if stats.running_binds.is_empty() {
        "—".to_string()
    } else {
        stats.running_binds.join(", ")
    };
    let conns = view.conn.snapshot.len();
    let (delta_rx, delta_tx) = view
        .conn
        .delta
        .values()
        .fold((0u64, 0u64), |(rx, tx), (d_rx, d_tx)| {
            (rx.saturating_add(*d_rx), tx.saturating_add(*d_tx))
        });
    let subs_line = if stats.subs_auto {
        t!(
            "stats-subs-due",
            count = stats.subs_total,
            due = stats.subs_due
        )
    } else {
        t!(
            "stats-subs-due-off",
            count = stats.subs_total,
            due = stats.subs_due
        )
    };
    let db_line = stats
        .db_bytes
        .map(format_bytes)
        .unwrap_or_else(|| t!("stats-unknown"));
    // The selected line reads name + health summary only; borrowing the row
    // avoids the full-config clone the cache rules forbid at paint.
    let selected_line = {
        let id = view
            .selection
            .iter()
            .next()
            .filter(|_| view.selection.len() == 1)
            .cloned();
        match (id, view.selected_name(cx)) {
            (Some(id), Some(name)) => {
                let health = view.health_store.row_summary(&id);
                let health = health.trim();
                if health.is_empty() {
                    t!("stats-selected-no-probes", name = name)
                } else {
                    t!("stats-selected-health", name = name, health = health)
                }
            }
            _ => t!("stats-none-selected"),
        }
    };
    let shard_rows = view.shard_rows_for_paint();
    let lines = vec![
        t!("stats-uptime", uptime = uptime),
        t!("stats-endpoints", running = running, total = total),
        t!("stats-running", list = running_list),
        t!("stats-connections", conns = conns),
        t!(
            "stats-session-delta",
            down = format_bytes(delta_rx),
            up = format_bytes(delta_tx)
        ),
        t!("stats-subscriptions", line = subs_line),
        t!("stats-database", line = db_line),
        t!("stats-selected", line = selected_line),
        t!("stats-processes", line = shard_stats_summary(shard_rows)),
    ];

    let mut runtime = div()
        .id("console-runtime")
        .test_support()
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .pt_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .id("console-runtime-title")
                .test_support()
                .text_sm()
                .child(t!("console-runtime-title")),
        );
    // A real list: each line is an item with its text as accessible name,
    // so AT reads the figures and the suite can assert their content.
    let mut stat_list = div().id("console-runtime-stats").role(Role::List);
    for (ix, line) in lines.iter().enumerate() {
        stat_list = stat_list.child(
            div()
                .id(format!("console-runtime-stat-{ix}"))
                .test_support()
                .role(Role::ListItem)
                .aria_label(line.clone())
                .text_sm()
                .child(line.clone()),
        );
    }
    runtime = runtime.child(stat_list);
    let cpu_stroke = cx.theme().chart_1;
    let ram_stroke = cx.theme().chart_2;
    let muted = cx.theme().muted_foreground;
    runtime = runtime.child(
        div()
            .id("console-runtime-shards")
            .test_support()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(t!("stats-sidecar-tip")),
            )
            .children(
                shard_rows
                    .iter()
                    .map(|row| shard_stat_row_view(row, muted, cpu_stroke, ram_stroke)),
            ),
    );
    runtime.into_any_element()
}
