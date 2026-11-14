//! inspector: one binary of the split UI suite. Each binary runs as its own
//! process, which resets the per-process mount ceiling that aborted the
//! single-binary suite (slice 8's harness fix).
//
// The shared helpers live in the support module; each area also keeps the
// suite's original import block so the test bodies compile unchanged.
#![allow(unused_imports)]
mod support;
use support::*;
// The whole test suite: pure-logic tests and gpui test-app paints.
// Belongs: every `#[test]`/`#[gpui_kit::test]` and its local fixtures.
// Not here: production helpers — a test that needs one promotes it to a module.
use std::cell::RefCell;
use std::rc::Rc;

use manyproxy_ui::engine::AutoRestartConfig;
use manyproxy_ui::import::OpenWithKind;
use manyproxy_ui::overlay_open;
use manyproxy_ui::store::{
    AutoClearGroupConfig, HealthSnapshot, NewConfig, Store, StoreError, WindowGeometry,
};
use manyproxy_ui::AppView;
use manyproxy_ui::ColumnId;
use manyproxy_ui::ColumnLayout;
use manyproxy_ui::ConfigForm;
use manyproxy_ui::ExitTraffic;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    point, px, size, AppContext, Bounds, Context, Entity, Hsla, TestAppContext, Window,
};
use manyproxy_core::Secret;

fn seed_input(name: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: Secret::default(),
        group: String::new(),
        bind_ip: String::new(),
        port: String::new(),
    }
}

fn test_facts(delegate: &manyproxy_ui::LibraryDelegate) -> manyproxy_ui::QueryFacts<'_> {
    manyproxy_ui::QueryFacts {
        running: &delegate.running,
        health: &delegate.health,
        crashed: &delegate.crashed,
        latency: &delegate.latency,
    }
}

/// The pure query over a delegate's mirrors, for tests that exercise matching
/// without mounting a window.
fn test_visible(delegate: &manyproxy_ui::LibraryDelegate, query: &manyproxy_ui::FilterQuery) -> Vec<usize> {
    manyproxy_ui::filter(
        &delegate.rows,
        &delegate.search_index,
        &test_facts(delegate),
        &manyproxy_ui::LibraryScope::All,
        query,
    )
}

/// The app's filter-then-sort order, reproduced without an `AppView`.
fn test_sorted(delegate: &manyproxy_ui::LibraryDelegate, query: &manyproxy_ui::FilterQuery) -> Vec<usize> {
    let mut visible = test_visible(delegate, query);
    if !delegate.sort.is_none() {
        if delegate.sort.key == manyproxy_ui::store::LibrarySortKey::TypeSecurity {
            visible.sort_by(|&a, &b| {
                manyproxy_ui::compare_type_security(&delegate.rows[a], &delegate.rows[b])
            });
        } else {
            let context = manyproxy_ui::SortContext {
                latency: &delegate.latency,
                data_traffic: &delegate.data_traffic,
                totals: &delegate.traffic,
                out_ips: &delegate.out_ips,
                upstreams: &delegate.upstream_cache,
            };
            visible.sort_by(|&a, &b| {
                manyproxy_ui::compare_library_rows(
                    &context,
                    delegate.sort,
                    &delegate.rows[a],
                    &delegate.rows[b],
                )
            });
        }
    }
    visible
}

fn test_names(delegate: &manyproxy_ui::LibraryDelegate, query: &manyproxy_ui::FilterQuery) -> Vec<String> {
    test_sorted(delegate, query)
        .into_iter()
        .map(|index| delegate.rows[index].name.clone())
        .collect()
}

/// The five-tab strip is gone. One scroll carries Identity, Endpoint, Health,
/// Actions and the console link.
#[gpui_kit::test]
fn inspector_stacked_sections_replace_the_five_tabs(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    select_library_row(&view, cx, &work.id);
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);

    for id in [
        "inspector-identity",
        "config-endpoints",
        "inspector-health-section",
        "inspector-actions",
        "inspector-console-link",
    ] {
        assert_painted(cx, id);
    }
    for id in ["inspector-tabs", "inspector-tab-bar"] {
        assert_absent(cx, id.to_string());
    }
}

/// The inspector's console link is the only inspector control that opens the
/// dock. It lands on Logs/Selected and clears the console filter.
#[gpui_kit::test]
fn inspector_console_link_opens_logs_scoped_to_the_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    select_library_row(&view, cx, &work.id);
    view.update(cx, |view, _| {
        view.console.filter = "stale".to_string();
        view.console.tab = manyproxy_ui::ConsoleTab::Traffic;
        view.console.scope = manyproxy_ui::ConsoleScope::All;
    });
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);

    cx.update(|window, cx| window.click("inspector-open-console", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);

    let (open, tab, scope, filter) = view.read_with(cx, |view, _| {
        (
            view.console.open,
            view.console.tab,
            view.console.scope,
            view.console.filter.clone(),
        )
    });
    assert!(open, "the link opens the console");
    assert_eq!(tab, manyproxy_ui::ConsoleTab::Logs);
    assert_eq!(scope, manyproxy_ui::ConsoleScope::Selected);
    assert!(filter.is_empty(), "the link lands with an empty filter");
    assert_painted(cx, "console");
}

/// R6: one selected row shows its sections; three show the aggregate counts
/// and no action buttons at all.
#[gpui_kit::test]
fn inspector_follows_one_row_and_aggregates_two_or_more(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vless", "team", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.select_visible_row(0, cx));
    });
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "inspector-actions");
    assert_absent(cx, "inspector-aggregate".to_string());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.select_all_visible_rows(cx));
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "inspector-aggregate");
    assert_eq!(label_of("inspector-selected-count", cx), "3 selected");
    assert_eq!(label_of("inspector-running-count", cx), "0 running");
    assert_eq!(label_of("inspector-failed-count", cx), "0 failed");
    assert_eq!(label_of("inspector-stopped-count", cx), "3 stopped");
    assert_eq!(
        label_of("inspector-use-selection-bar", cx),
        "Use the selection bar to act."
    );
    assert_absent(cx, "inspector-actions".to_string());
    assert_absent(cx, "config-endpoints".to_string());
}

/// The aggregate's hidden count clears the facet filters, keeps the selection,
/// and goes away once every selected row is visible again.
#[gpui_kit::test]
fn inspector_multi_selection_hidden_count_clears_filters(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vmess", "team", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_all_visible_rows(cx);
            view.open_inspector();
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_protocol_facet("vless".to_string(), true, cx);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);

    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 2);
    assert_eq!(
        view.read_with(cx, |view, cx| view.selection_hidden_count(cx)),
        1
    );
    assert_painted(cx, "inspector-hidden-count");
    assert_eq!(label_of("inspector-hidden-count", cx), "1 hidden");

    cx.update(|window, cx| window.click("inspector-hidden-count", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        3,
        "the hidden count clears the filters"
    );
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 3);
    assert_absent(cx, "inspector-hidden-count".to_string());
}

/// `Ctrl+I` toggles the docked inspector without taking the keyboard away from
/// the table.
#[gpui_kit::test]
fn ctrl_i_toggles_inspector_without_moving_focus_off_the_table(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    focus_library(&view, cx);

    let table_focus = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    let toggle = || Keystroke::parse("ctrl-i").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert!(view.read_with(cx, |view, _| view.inspector_visible));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(table_focus.clone()),
        "opening the inspector leaves focus on the table"
    );

    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert!(!view.read_with(cx, |view, _| view.inspector_visible));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(table_focus),
        "closing the inspector leaves focus on the table"
    );
}

/// `Ctrl+\`` opens the dock above the engine bar: the table shrinks by exactly
/// the dock height and the engine bar never moves.
#[gpui_kit::test]
fn ctrl_backtick_toggles_console_and_engine_bar_does_not_move(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    focus_library(&view, cx);

    let bounds = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            let bar = window
                .try_find("engine-bar".to_string())
                .expect("engine bar renders")
                .bounds();
            let content = window
                .try_find("main-content".to_string())
                .expect("main content renders")
                .bounds();
            (bar.origin.y, content.size.height)
        })
    };
    let (bar_y, content_h) = bounds(cx);

    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_painted(cx, "console");
    let (open_bar_y, open_content_h) = bounds(cx);
    assert_eq!(
        open_bar_y, bar_y,
        "the engine bar never moves when the console opens"
    );
    assert_eq!(
        content_h - open_content_h,
        px(240.),
        "the comfortable console takes exactly its 240px from the rows"
    );

    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_absent(cx, "console".to_string());
    let (closed_bar_y, closed_content_h) = bounds(cx);
    assert_eq!(closed_bar_y, bar_y);
    assert_eq!(
        closed_content_h, content_h,
        "closing gives the rows their height back"
    );
}

/// A launch starts collapsed on Logs / All with no filter and no pause.
#[gpui_kit::test]
fn console_starts_collapsed_and_launch_defaults_are_logs_all(cx: &mut TestAppContext) {
    use manyproxy_ui::{ConsoleScope, ConsoleTab};
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let defaults = view.read_with(cx, |view, _| {
        (
            view.console.open,
            view.console.tab,
            view.console.scope,
            view.console.filter.clone(),
            view.console.paused.is_some(),
        )
    });
    assert_eq!(
        defaults,
        (
            false,
            ConsoleTab::Logs,
            ConsoleScope::All,
            String::new(),
            false
        ),
        "the console starts collapsed on Logs / All, empty and live"
    );
    assert_absent(cx, "console".to_string());

    focus_library(&view, cx);
    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_painted(cx, "console");
    let selected = |cx: &mut VisualTestContext, id: &str| {
        cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .selected()
        })
        .unwrap_or(false)
    };
    assert!(selected(cx, "console-tab-logs"));
    assert!(!selected(cx, "console-tab-connections"));
    assert!(selected(cx, "console-scope-all"));
    assert!(!selected(cx, "console-scope-selected"));
}

/// `Selected` keeps the whole selection's log lines and drops the global lines
/// that belong to no config.
#[gpui_kit::test]
fn console_logs_selected_scope_uses_the_whole_selection(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::inbound_tag;
    use gpui_kit::{Keystroke, Modifiers};
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let bravo = store
        .add(row19_seed("Bravo", "b.example.com", "10802"))
        .expect("seed works");
    let charlie = store
        .add(row19_seed("Charlie", "c.example.com", "10803"))
        .expect("seed works");
    let tags = manyproxy_ui::health::LogTagIndex::build(&[alpha.clone(), bravo.clone(), charlie.clone()]);
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw(
                &format!("info {}: alpha connected", inbound_tag(&alpha.id)),
                &tags,
            );
            view.logs.store.push_raw(
                &format!("warn {}: bravo retried", inbound_tag(&bravo.id)),
                &tags,
            );
            view.logs.store.push_raw("info engine started", &tags);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);

    click_row_with(
        cx,
        &format!("row-name-text-{}", alpha.id),
        Modifiers::default(),
    );
    click_row_with(
        cx,
        &format!("row-name-text-{}", bravo.id),
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 2);

    focus_library(&view, cx);
    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.update(|window, cx| window.click("console-scope-selected", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);

    let rows = view.read_with(cx, |view, cx| view.console_log_rows(cx));
    assert_eq!(rows.len(), 2, "both selected configs keep their lines");
    for id in [&alpha.id, &bravo.id] {
        assert!(
            rows.iter()
                .any(|row| row.config_id.as_deref() == Some(id.as_str())),
            "the selection's {id} line survives Selected scope"
        );
    }
    assert_painted(cx, "console-log-line-0");
    assert_painted(cx, "console-log-line-1");
    assert_absent(cx, "console-log-line-2".to_string());
}

/// `Selected` keeps every flow claimed by any selected row, and nothing else.
#[gpui_kit::test]
fn console_connections_selected_scope_uses_the_whole_selection(cx: &mut TestAppContext) {
    use manyproxy_ui::health::ClashConn;
    use gpui_kit::{Keystroke, Modifiers};

    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let bravo = store
        .add(row19_seed("Bravo", "b.example.com", "10802"))
        .expect("seed works");
    store
        .add(row19_seed("Charlie", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.conn.snapshot = vec![
                (
                    "stock".to_string(),
                    ClashConn {
                        id: "conn-a".to_string(),
                        row_id: Some(alpha.id.clone()),
                        rx_bytes: 1000,
                        tx_bytes: 10,
                        dest: "alpha.example.com:443".to_string(),
                        network: "tcp".to_string(),
                    },
                ),
                (
                    "stock".to_string(),
                    ClashConn {
                        id: "conn-b".to_string(),
                        row_id: Some(bravo.id.clone()),
                        rx_bytes: 2000,
                        tx_bytes: 20,
                        dest: "bravo.example.com:443".to_string(),
                        network: "tcp".to_string(),
                    },
                ),
                (
                    "stock".to_string(),
                    ClashConn {
                        id: "conn-c".to_string(),
                        row_id: None,
                        rx_bytes: 9000,
                        tx_bytes: 90,
                        dest: "unclaimed.example.com:443".to_string(),
                        network: "tcp".to_string(),
                    },
                ),
            ];
            view.conn.delta.clear();
            view.conn.sort_desc = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);

    click_row_with(
        cx,
        &format!("row-name-text-{}", alpha.id),
        Modifiers::default(),
    );
    click_row_with(
        cx,
        &format!("row-name-text-{}", bravo.id),
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );

    focus_library(&view, cx);
    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.update(|window, cx| window.click("console-tab-connections", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.update(|window, cx| window.click("console-scope-selected", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);

    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 2);
    assert_painted(cx, "console-connections");
    assert_eq!(
        label_of("console-conn-copy-0", cx),
        "bravo.example.com:443",
        "the heavier selected flow paints first"
    );
    assert_eq!(
        label_of("console-conn-copy-1", cx),
        "alpha.example.com:443",
        "the second selected flow stays"
    );
    assert_absent(cx, "console-conn-copy-2".to_string());
    assert_absent(cx, "console-conn-row-2".to_string());
}

/// The Traffic tab totals the current scope from the session snapshot and
/// carries the old runtime lines in the same scroll.
#[gpui_kit::test]
fn console_traffic_scope_totals_and_runtime_lines(cx: &mut TestAppContext) {
    use manyproxy_ui::health::ClashConn;
    use gpui_kit::Keystroke;

    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let bravo = store
        .add(row19_seed("Bravo", "b.example.com", "10802"))
        .expect("seed works");
    store
        .add(row19_seed("Charlie", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.conn.snapshot = vec![
                (
                    "stock".to_string(),
                    ClashConn {
                        id: "conn-a".to_string(),
                        row_id: Some(alpha.id.clone()),
                        rx_bytes: 1000,
                        tx_bytes: 10,
                        dest: "alpha.example.com:443".to_string(),
                        network: "tcp".to_string(),
                    },
                ),
                (
                    "stock".to_string(),
                    ClashConn {
                        id: "conn-b".to_string(),
                        row_id: Some(bravo.id.clone()),
                        rx_bytes: 2000,
                        tx_bytes: 20,
                        dest: "bravo.example.com:443".to_string(),
                        network: "tcp".to_string(),
                    },
                ),
                (
                    "stock".to_string(),
                    ClashConn {
                        id: "conn-c".to_string(),
                        row_id: None,
                        rx_bytes: 4000,
                        tx_bytes: 40,
                        dest: "unclaimed.example.com:443".to_string(),
                        network: "tcp".to_string(),
                    },
                ),
            ];
            view.conn.delta.clear();
            view.conn.sort_desc = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);

    click_row_with(
        cx,
        &format!("row-name-text-{}", alpha.id),
        gpui_kit::Modifiers::default(),
    );
    click_row_with(
        cx,
        &format!("row-name-text-{}", bravo.id),
        gpui_kit::Modifiers {
            control: true,
            ..gpui_kit::Modifiers::default()
        },
    );

    focus_library(&view, cx);
    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.update(|window, cx| window.click("console-tab-traffic", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);

    assert_eq!(label_of("console-traffic-rx", cx), "Session RX: 6.8 KB");
    assert_eq!(label_of("console-traffic-tx", cx), "Session TX: 70 B");
    assert_eq!(label_of("console-traffic-rows", cx), "Visible rows: 2");

    cx.update(|window, cx| window.click("console-scope-selected", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_eq!(label_of("console-traffic-rx", cx), "Session RX: 2.9 KB");
    assert_eq!(label_of("console-traffic-tx", cx), "Session TX: 30 B");
    assert_eq!(label_of("console-traffic-rows", cx), "Visible rows: 2");

    cx.update(|window, cx| {
        window.scroll(
            "console-traffic",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    for id in [
        "console-runtime",
        "console-runtime-title",
        "console-runtime-stat-0",
        "console-runtime-stat-8",
        "console-runtime-shards",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "{id} paints in the console's Runtime group"
        );
    }
    assert_painted(cx, "console-runtime-stat-8");
    assert_painted(cx, "console-runtime-shards");
}

/// Pause freezes the painted rows through new records; Resume recomputes;
/// Clear empties the ring and shows the empty line.
#[gpui_kit::test]
fn console_pause_freezes_rows_and_clear_empties_the_ring(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let tags = manyproxy_ui::health::LogTagIndex::default();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw("info first line", &tags);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    focus_library(&view, cx);
    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_painted(cx, "console-log-line-0");

    cx.update(|window, cx| window.click("console-pause", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_painted(cx, "console-paused-badge");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw("warn second line", &tags);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_absent(cx, "console-log-line-1".to_string());

    cx.update(|window, cx| window.click("console-pause", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_absent(cx, "console-paused-badge".to_string());
    assert_painted(cx, "console-log-line-1");

    cx.update(|window, cx| window.click("console-clear", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_painted(cx, "console-empty-logs");
    assert!(
        view.read_with(cx, |view, _| view.logs.store.all().is_empty()),
        "Clear empties the log ring"
    );
}

/// Both close controls hand the keyboard back to the table.
#[gpui_kit::test]
fn console_close_and_inspector_close_return_focus_to_the_table(cx: &mut TestAppContext) {
    use gpui_kit::{Focusable as _, Keystroke};
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    focus_library(&view, cx);
    let toggle = || Keystroke::parse("ctrl-`").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    let filter_focus = view.read_with(cx, |view, cx| {
        view.console_filter_input.read(cx).focus_handle(cx)
    });
    cx.update(|window, cx| window.focus(&filter_focus, cx));
    cx.update(|window, cx| window.click("console-close", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(!view.read_with(cx, |view, _| view.console.open));
    let table_focus = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(table_focus.clone()),
        "closing the console lands focus on the table"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            view.open_inspector();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    let search_focus = view.read_with(cx, |view, cx| view.search_input.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&search_focus, cx));
    cx.update(|window, cx| window.click("inspector-drawer-close", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(!view.read_with(cx, |view, _| view.inspector_visible));
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(table_focus),
        "closing the inspector lands focus on the table"
    );
}

/// The engine bar's Console button paints its open state and toggles the dock.
#[gpui_kit::test]
fn engine_bar_console_button_reflects_open_state(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        label_of("engine-bar-console", cx),
        "Show console",
        "the closed console's button offers Show console"
    );

    cx.update(|window, cx| window.click("engine-bar-console", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(view.read_with(cx, |view, _| view.console.open));
    assert_painted(cx, "console");
    assert_eq!(
        label_of("engine-bar-console", cx),
        "Hide console",
        "the open console swaps the button's name"
    );

    cx.update(|window, cx| window.click("engine-bar-console", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(!view.read_with(cx, |view, _| view.console.open));
    assert_eq!(label_of("engine-bar-console", cx), "Show console");
}

/// `Clear test results…` runs the existing confirm, clears the samples, and is
/// inert while there is nothing to clear.
#[gpui_kit::test]
fn engine_bar_clear_test_results_opens_the_existing_confirm_and_clears(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, ProbeSuccess, TestOutcome};
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: alpha.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: Some(120),
                    error: None,
                },
                success: Some(ProbeSuccess {
                    latency_ms: 120,
                    bytes_rx: 0,
                    bytes_tx: 0,
                }),
            });
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert!(view.read_with(cx, |view, _| view.health_store.has_samples()));

    cx.update(|window, cx| window.click("engine-bar-overflow", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert_painted(cx, "engine-bar-clear-tests");
    cx.update(|window, cx| window.click("engine-bar-clear-tests", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the existing clear-health confirm opens"
    );
    cx.update(|window, cx| window.click("ok", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert!(
        !view.read_with(cx, |view, _| view.health_store.has_samples()),
        "confirming clears the health evidence"
    );

    // With no samples the item is disabled: clicking it opens nothing.
    cx.update(|window, cx| {
        let escape = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.update(|window, cx| window.click("engine-bar-overflow", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    cx.update(|window, cx| window.click("engine-bar-clear-tests", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 900.);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "a disabled Clear test results opens no confirm"
    );
}

/// The retired tab verbs, their Ctrl+1..6 bindings and the status strip are
/// gone from every surface.
#[gpui_kit::test]
fn old_tab_actions_shortcuts_and_status_strip_are_gone(cx: &mut TestAppContext) {
    use manyproxy_ui::app::actions::{ACTIONS, PALETTE_GROUPS};

    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let added = bound_key_strings(cx);
    for key in ["ctrl-1", "ctrl-2", "ctrl-3", "ctrl-4", "ctrl-5", "ctrl-6"] {
        assert!(
            !added.contains(&key.to_string()),
            "{key} must not be bound after the tabs retire"
        );
    }
    for (_, keys) in manyproxy_ui::SHORTCUT_LINES.iter() {
        for key in ["1", "2", "3", "4", "5", "6"] {
            assert!(
                !keys.contains(&format!("Ctrl-{key}")),
                "the shortcut reference lists no inspector tab key: {keys}"
            );
        }
    }
    assert!(
        manyproxy_ui::SHORTCUT_LINES
            .iter()
            .any(|(action, keys)| *action == "Toggle console" && *keys == "Ctrl-`"),
        "Ctrl+` replaces the tab shortcuts"
    );

    let action_ids: Vec<&str> = ACTIONS.iter().map(|def| def.id).collect();
    let palette: Vec<&str> = PALETTE_GROUPS
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    for id in [
        "inspector-details",
        "inspector-health",
        "inspector-logs",
        "inspector-links",
        "inspector-traffic",
        "runtime-stats",
        "traffic-stats",
    ] {
        assert!(!action_ids.contains(&id), "{id} is not an action any more");
        assert!(!palette.contains(&id), "{id} left the palette");
    }

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "status-bar",
        "runtime-stats",
        "traffic-stats",
        "inspector-tabs",
    ] {
        assert_absent(cx, id.to_string());
    }
}

/// A dragged width survives a restart, and a bad stored value falls back to
/// the 300px default.
#[gpui_kit::test]
fn inspector_width_persists_across_reopen(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path =
        std::env::temp_dir().join(format!("manyproxy-slice6-width-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    {
        let mut store = Store::open_file(&path).expect("file store opens");
        store
            .add(row19_seed("Work", "a.example.com", "10801"))
            .expect("seed works");
    }

    let store = Store::open_file(&path).expect("reopen works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            view.open_inspector();
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    let (from, to) = cx.update(|window, _| {
        let detail = window
            .try_find("config-detail")
            .expect("the inspector panel renders")
            .bounds();
        let x = detail.origin.x - px(2.);
        let y = detail.origin.y + detail.size.height / 2.;
        (point(x, y), point(x - px(40.), y))
    });
    cx.update(|window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
    let stored = view.read_with(cx, |view, _| view.store.inspector_width());
    assert!(
        (stored - 340.).abs() <= 3.,
        "mouse-up writes the dragged width, got {stored}"
    );
    paint_frame_at(cx, 1440., 900.);
    let live = view.read_with(cx, |view, cx| view.inspector_width(cx));
    assert!(
        (live - stored).abs() <= 1.,
        "the live split reads the dragged width, got {live}"
    );
    drop(view);

    let store = Store::open_file(&path).expect("restart reopen works");
    let (reopened, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    let reopened_width = reopened.read_with(cx, |view, cx| view.inspector_width(cx));
    assert!(
        (reopened_width - stored).abs() <= 1.,
        "a restart restores the persisted width, got {reopened_width}"
    );

    drop(reopened);
    let mut store = Store::open_file(&path).expect("bad-value reopen works");
    store
        .set_meta(Store::META_INSPECTOR_WIDTH, "not-a-number")
        .expect("meta write works");
    drop(store);

    let store = Store::open_file(&path).expect("bad-value reload works");
    let (fallback, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    assert_eq!(
        fallback.read_with(cx, |view, cx| view.inspector_width(cx)),
        Store::INSPECTOR_WIDTH_DEFAULT,
        "an unreadable stored width falls back to 300"
    );
}
