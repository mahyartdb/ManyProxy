//! filters: one binary of the split UI suite. Each binary runs as its own
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

use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    point, px, size, AppContext, Bounds, Context, Entity, Hsla, TestAppContext, Window,
};
use manyproxy_core::Secret;
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

#[gpui_kit::test]
fn filter_row_renders_search_status_protocol_filter_count_clear(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    for width in [1100., 1440.] {
        paint_frame_at(cx, width, 700.);
        assert_painted(cx, "filter-slot");
        assert_painted(cx, "library-search");
        assert_painted(cx, "filter-status");
        assert_painted(cx, "filter-protocol");
        assert_painted(cx, "filter-more");
        assert_painted(cx, "filter-count");
        assert_eq!(label_of("filter-status", cx), "Status");
        assert_eq!(label_of("filter-protocol", cx), "Protocol");
        assert_eq!(label_of("filter-more", cx), "Filter");
        assert_eq!(label_of("filter-count", cx), "2 of 2");
        assert_absent(cx, "filter-clear".to_string());
        assert_absent(cx, "filter-chips".to_string());
        assert_absent(cx, "facet-bar".to_string());
        let height = cx.update(|window, _| {
            window
                .try_find("filter-slot".to_string())
                .expect("filter slot paints")
                .bounds()
                .size
                .height
        });
        assert_eq!(height, px(36.), "the row is exactly 36px at {width}px");
    }

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.set_compact_density(window, cx));
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1100., 700.);
    let height = cx.update(|window, _| {
        window
            .try_find("filter-slot".to_string())
            .expect("filter slot paints")
            .bounds()
            .size
            .height
    });
    assert_eq!(height, px(34.), "compact density keeps the row at 34px");
}

#[gpui_kit::test]
fn search_debounces_80ms(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    use std::time::Duration;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let handle = view.read_with(cx, |view, cx| view.search_input.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));

    cx.simulate_input("zzz");
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        2,
        "a keystroke does not filter before the 80ms window"
    );
    cx.executor().advance_clock(Duration::from_millis(79));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        2,
        "79ms is still inside the window"
    );

    // A second keystroke inside the window retires the first task: the old
    // text never applies even when its own 80ms elapses.
    for _ in 0..3 {
        let backspace = gpui_kit::Keystroke::parse("backspace").expect("test keystroke parses");
        cx.update(|window, cx| window.dispatch_keystroke(backspace, cx));
    }
    cx.simulate_input("Work");
    cx.executor().advance_clock(Duration::from_millis(79));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        2,
        "the replaced zzz task did not apply"
    );
    cx.executor().advance_clock(Duration::from_millis(2));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        1,
        "the latest text applies once, 80ms after the last keystroke"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.query.search.clone()),
        "Work",
        "the field and the query agree"
    );
}

#[gpui_kit::test]
fn enter_and_clear_apply_immediately(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    use gpui_kit::Keystroke;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let handle = view.read_with(cx, |view, cx| view.search_input.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.search_input.update(cx, |input, cx| {
                input.set_value("Work".to_string(), window, cx);
            });
        });
    });
    let enter = Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(enter, cx));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        1,
        "Enter applies without waiting for the timer"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.query.search.clone()),
        "Work"
    );

    // The field's clear path is immediate too.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.clear_search(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        2,
        "clearing the field restores every row at once"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view
            .search_input
            .read(cx)
            .value()
            .to_string()),
        "",
        "the field empties with the query"
    );

    // `Clear` is the same immediate path.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.search_input.update(cx, |input, cx| {
                input.set_value("Work".to_string(), window, cx);
            });
        });
    });
    let enter = Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(enter, cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("filter-clear", cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        2,
        "Clear drops the query without waiting for the timer"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.query.search.clone()),
        "",
        "Clear empties the search"
    );
}

#[gpui_kit::test]
fn status_and_protocol_buttons_show_active_values_and_clear_with_x(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_status_facet(manyproxy_ui::StatusFacet::Running, true, cx)
        });
    });
    paint_frame(cx);
    assert_eq!(
        label_of("filter-status", cx),
        "Status · Running",
        "the button carries the one active value"
    );
    assert_painted(cx, "filter-status-clear");
    cx.update(|window, cx| window.click("filter-status-clear", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(label_of("filter-status", cx), "Status");
    assert!(
        view.read_with(cx, |view, _| view.query.statuses.is_empty()),
        "the x clears only the status facet"
    );
    assert_absent(cx, "filter-status-clear".to_string());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_protocol_facet("vless".to_string(), true, cx)
        });
    });
    paint_frame(cx);
    assert_eq!(
        label_of("filter-protocol", cx),
        "Protocol · VLESS",
        "the button carries the one active protocol"
    );
    assert_painted(cx, "filter-protocol-clear");
    cx.update(|window, cx| window.click("filter-protocol-clear", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(label_of("filter-protocol", cx), "Protocol");
    assert!(
        view.read_with(cx, |view, _| view.query.protocols.is_empty()),
        "the x clears only the protocol facet"
    );
}

#[gpui_kit::test]
fn filter_button_counts_the_long_tail(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(label_of("filter-more", cx), "Filter");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_latency_facet(
                manyproxy_ui::LatencyFacet {
                    bucket: manyproxy_ui::LatencyBucket::Ms100,
                    include_unprobed: false,
                },
                cx,
            );
            view.set_bind_facet("127.0.0.1".to_string(), true, cx);
            view.set_port_range("10800", "10900", cx);
        });
    });
    paint_frame(cx);
    assert_eq!(
        label_of("filter-more", cx),
        "Filter · 3",
        "latency, bind and port each count one"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.set_port_range("", "", cx));
    });
    paint_frame(cx);
    assert_eq!(
        label_of("filter-more", cx),
        "Filter · 2",
        "clearing one control drops its count"
    );
}

#[gpui_kit::test]
fn count_reads_n_of_m(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    let rows: Vec<_> = (0..1_037)
        .map(|index| {
            t77_row(
                &(index + 1).to_string(),
                &format!("Row {}", index + 1),
                "vless",
                "g",
                10_801 + index as u16,
            )
        })
        .collect();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries
                .update(cx, |state, _| state.delegate_mut().set_rows(rows));
            view.apply_query(cx);
            cx.notify();
        });
    });
    paint_frame(cx);
    assert_eq!(
        label_of("filter-count", cx),
        "1037 of 1037",
        "with no query N = M"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("port=10801:10842".to_string(), cx)
        });
    });
    paint_frame(cx);
    assert_eq!(
        label_of("filter-count", cx),
        "42 of 1037",
        "N is the visible set, M the scope before the query"
    );
}

#[gpui_kit::test]
fn no_match_is_not_empty_library(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, "library-empty");
    assert_painted(cx, "empty-add");
    assert_painted(cx, "empty-import");
    assert_absent(cx, "library-clear-filters".to_string());

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.store
                .add(facet_seed("Work", "vless", "travel", "10801"))
                .expect("seed works");
            view.reload(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("zzz-no-hit".to_string(), cx)
        });
    });
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    assert_painted(cx, "library-clear-filters");
    assert_eq!(label_of("filter-count", cx), "0 of 1");
    assert_absent(cx, "empty-add".to_string());
    assert_absent(cx, "empty-import".to_string());
}

#[gpui_kit::test]
fn batch_bar_restores_filter_values(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("Work".to_string(), cx);
            view.set_status_facet(manyproxy_ui::StatusFacet::Disabled, true, cx);
            view.set_bind_facet("127.0.0.1".to_string(), true, cx);
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        1,
        "the selected row arms the batch bar"
    );
    assert_painted(cx, "batch-clear");
    assert_absent(cx, "filter-status".to_string());

    cx.update(|window, cx| window.click("batch-clear", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.query.search.clone()),
        "Work",
        "the search query returns"
    );
    assert_eq!(
        label_of("filter-status", cx),
        "Status · Disabled",
        "the status returns"
    );
    assert_eq!(
        label_of("filter-more", cx),
        "Filter · 1",
        "the bind returns"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        1,
        "the filter still narrows the list"
    );
}

#[gpui_kit::test]
fn empty_library_offers_one_entry_point(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    // F18: the canvas repeats the action bar's two intake actions under the
    // sentence that names them; the action bar stays home when rows exist.
    assert_painted(cx, "empty-library-ctas");
    assert_painted(cx, "empty-add");
    assert_painted(cx, "empty-import");
    assert_painted(cx, "action-bar-add");
    assert_painted(cx, "action-bar-import");
}

#[gpui_kit::test]
fn t06_empty_library_describer_teaches_next_step(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    // F18: the next step is the actions the sentence names — inline now,
    // with the action bar still carrying them when rows exist.
    assert_painted(cx, "empty-add");
    assert_painted(cx, "empty-import");
    assert_painted(cx, "action-bar-add");
    assert_painted(cx, "action-bar-import");
    let (total, enabled) =
        view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().counts());
    assert_eq!((total, enabled), (0, 0), "counts name the empty set");
    assert_eq!(
        view.read_with(cx, |view, _| view.health_counts()),
        (0, 0),
        "empty health never reads as failure"
    );

    let traffic = view.read_with(cx, |view, _| manyproxy_ui::summed_traffic_text(&view.store));
    assert!(
        traffic.is_none() || traffic.as_deref() == Some("out 0 B · in 0 B"),
        "usage on empty is absent or zero — never nonzero: {traffic:?}"
    );
}

#[gpui_kit::test]
fn t04_empty_library_actions_live_in_the_action_bar(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    // F18: the canvas holds the intake CTAs and the action bar keeps its
    // pair, both bound to the same handlers.
    assert_painted(cx, "empty-library-ctas");
    assert_painted(cx, "empty-add");
    assert_painted(cx, "empty-import");
    for id in ["action-bar-add", "action-bar-import"] {
        assert_painted(cx, id);
        assert_within_frame(cx, id);
    }
}

#[gpui_kit::test]
fn t04_density_anchors_paint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("work".to_string(), cx);
            view.set_bind_facet("nope".to_string(), true, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "action-bar-count",
        "action-bar-add",
        "action-bar-import",
        "engine-bar-status",
    ] {
        assert_painted(cx, id);
    }
    // T-03: the Add and Import calls to action belong to an EMPTY library.
    // A library with rows but no selection asks for a row instead.
    assert_absent(cx, "detail-add".to_string());
    assert_absent(cx, "detail-import".to_string());
}

#[gpui_kit::test]
fn t04_log_no_lines_empty_clears_filter(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.logs.filter = "zzz-no-hit".to_string();
            view.open_logs_dialog(window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "logs sheet open"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "logs opens no dialog"
    );
    assert_painted(cx, "log-lines");
    assert_painted(cx, "log-copy-visible");
    assert_painted(cx, "log-tail-toggle");
    assert_painted(cx, "logs-save");
    assert_painted(cx, "log-clear-filter");
    cx.update(|window, cx| {
        window.click("log-clear-filter", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.filter.clone()),
        String::new(),
        "Clear filter restores the full tail"
    );
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "sheet reopens around the restored tail"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "reopen opens no dialog"
    );
    assert_painted(cx, "log-lines");
}

#[test]
fn field_error_targets_name_the_owning_field() {
    use manyproxy_ui::field_error_target;
    use manyproxy_ui::store::NewConfig;
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let error = store
        .add(NewConfig {
            name: "   ".to_string(),
            protocol: "bogus".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect_err("bad add fails");
    assert_eq!(
        field_error_target(&error).map(|(field, _)| field),
        Some("name"),
        "first error wins, got {error}"
    );
    let error = store
        .add(NewConfig {
            name: "Work".to_string(),
            protocol: "bogus".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect_err("bad protocol fails");
    let (field, message) = field_error_target(&error).expect("protocol maps");
    assert_eq!(field, "protocol");
    assert!(message.contains("valid"), "message guides, got {message}");
    let error = store
        .add(NewConfig {
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: String::new(),
            port: "99999".to_string(),
        })
        .expect_err("bad port fails");
    assert_eq!(
        field_error_target(&error).map(|(field, _)| field),
        Some("port"),
        "got {error}"
    );
    assert_eq!(
        field_error_target(&manyproxy_ui::store::StoreError::NotFound("7".to_string())),
        None,
        "non-field errors map nowhere"
    );
}

#[test]
fn bind_port_collision_warning_names_colliding_row() {
    use manyproxy_ui::bind_port_collision_warning;
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(NewConfig {
            name: "One".to_string(),
            protocol: "vless".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: "127.0.0.1".to_string(),
            port: "12001".to_string(),
        })
        .expect("seed works");
    let rows = store.list().expect("list works");
    let warning = bind_port_collision_warning(&rows, None, "127.0.0.1", "12001").expect("collides");
    assert!(warning.contains("One"), "names the row, got {warning}");
    assert!(warning.contains("12001"), "names the port, got {warning}");
    assert!(
        warning.contains("still allowed"),
        "non-blocking, got {warning}"
    );
    assert!(
        bind_port_collision_warning(&rows, None, "127.0.0.1", "12002").is_none(),
        "free pair stays quiet"
    );
    assert!(
        bind_port_collision_warning(&rows, None, "127.0.0.1", "").is_none(),
        "blank port means auto — never collides"
    );
    assert!(
        bind_port_collision_warning(&rows, None, "127.0.0.1", "bogus").is_none(),
        "invalid port is validation's job, not the warning's"
    );
    let self_id = rows[0].id.clone();
    assert!(
        bind_port_collision_warning(&rows, Some(&self_id), "127.0.0.1", "12001").is_none(),
        "edit excludes its own row"
    );
    assert!(
        bind_port_collision_warning(&rows, None, "", "12001").is_some(),
        "blank bind means the loopback default"
    );
}

#[test]
fn http_subscription_warning_only_for_plaintext() {
    use manyproxy_ui::http_subscription_warning;
    let warning = http_subscription_warning("http://example.com/sub.txt").expect("http warns");
    assert!(
        warning.contains("plaintext"),
        "Warn policy surfaced, got {warning}"
    );
    assert!(
        http_subscription_warning("https://example.com/sub.txt").is_none(),
        "https stays quiet"
    );
    assert!(
        http_subscription_warning("").is_none(),
        "blank is a validation error, not a warning"
    );
    assert!(
        http_subscription_warning("not-a-url").is_none(),
        "unparseable is a validation error, not a warning"
    );
}

#[test]
fn already_subscribed_message_names_existing_label() {
    use manyproxy_ui::already_subscribed_message;
    let text = already_subscribed_message("Nightly");
    assert!(
        text.contains("Already subscribed"),
        "friendly path, got {text}"
    );
    assert!(text.contains("Nightly"), "names the row, got {text}");
}

#[gpui_kit::test]
fn add_commit_invalid_stays_open_and_focuses_field(cx: &mut TestAppContext) {
    use gpui_kit::component::input::AnyInputState;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: std::rc::Rc<std::cell::RefCell<Option<(String, String)>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("   ", window, cx);
            });
            let expected = AnyInputState::from(form.name.clone()).focus_handle(cx);
            let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
            assert_eq!(
                window.focused(cx),
                Some(expected),
                "focus moves to the error field"
            );
            closed
        })
    });
    assert!(!closed, "invalid commit keeps the dialog open");
    let (field, _) = form_error.borrow().clone().expect("inline error recorded");
    assert_eq!(field, "name", "inline error names the field");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
}

#[gpui_kit::test]
fn t06_add_blank_name_and_bad_params_names_first_field(cx: &mut TestAppContext) {
    use gpui_kit::component::input::AnyInputState;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("   ", window, cx);
            });
            form.params.update(cx, |state, cx| {
                state.set_value("{{{", window, cx);
            });
            let expected = AnyInputState::from(form.name.clone()).focus_handle(cx);
            let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
            assert_eq!(
                window.focused(cx),
                Some(expected),
                "focus moves to the first invalid field"
            );
            closed
        })
    });
    assert!(!closed, "invalid commit keeps the dialog open");
    let (field, message) = form_error.borrow().clone().expect("inline error recorded");
    assert_eq!(field, "name", "first invalid field wins");
    assert!(!message.trim().is_empty(), "error names the fix");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
}

#[gpui_kit::test]
fn t06_add_dialog_paints_live_port_budget(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    let budget = ConfigForm::port_budget_text(&[]);
    assert!(
        budget.contains("0 of 499 in use (10801–10999, spill 18081–18380)"),
        "live budget copy: {budget}"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_painted(cx, "port-budget");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t06_add_form_lan_bind_shows_loopback_hint_before_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::{lan_hint_for_bind, ConfigForm};
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::blank(window, cx);
            assert!(
                lan_hint_for_bind(form.bind.read(cx).value().as_ref()).is_none(),
                "loopback default shows no hint"
            );
            form.bind.update(cx, |state, cx| {
                state.set_value("0.0.0.0", window, cx);
            });
            let hint = lan_hint_for_bind(form.bind.read(cx).value().as_ref())
                .expect("non-loopback bind hints before commit");
            assert!(hint.contains("0.0.0.0"), "hint names the bind: {hint}");
            assert!(
                hint.contains("Leaving loopback exposes"),
                "loopback-exposure hint: {hint}"
            );
            assert!(hint.contains("LAN confirm"), "names the gate: {hint}");
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        cx.update(|window, _| window.try_find("lan-hint").is_none()),
        "loopback default shows no LAN hint"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn duplicate_subscription_url_warns_without_double_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add_subscription("Nightly", "https://example.com/a.txt", "g", true)
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_subscription_add(
                "Copy".to_string(),
                "  https://example.com/a.txt  ".to_string(),
                "g".to_string(),
                true,
                manyproxy_ui::store::DEFAULT_AUTO_REFRESH_SECS,
                window,
                cx,
            )
        })
    });
    assert!(!closed, "dup keeps the dialog open");
    assert_eq!(notification_count(cx), 1, "one friendly warning");
    let subs = view.read_with(cx, |view, _| {
        view.store.list_subscriptions().expect("list works")
    });
    assert_eq!(subs.len(), 1, "no double row");
    assert_eq!(subs[0].label, "Nightly", "original kept");
}

#[gpui_kit::test]
fn edit_subscription_persists_and_refreshes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/a.txt", "g1", true)
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_subscription_edit(
                &sub.id,
                "Daily".to_string(),
                "https://example.com/b.txt".to_string(),
                "g2".to_string(),
                3_600,
                window,
                cx,
            )
        })
    });
    assert!(closed, "valid edit closes");
    assert_eq!(notification_count(cx), 1, "one success notice");
    let subs = view.read_with(cx, |view, _| {
        view.store.list_subscriptions().expect("list works")
    });
    assert_eq!(subs.len(), 1, "edit refreshes in place");
    assert_eq!(subs[0].label, "Daily");
    assert_eq!(subs[0].url, "https://example.com/b.txt");
    assert_eq!(subs[0].group, "g2");
    assert_eq!(subs[0].interval_secs, 3_600, "edit commits the interval");
    let ghost = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_subscription_edit(
                "9999",
                "x".to_string(),
                "https://example.com/x.txt".to_string(),
                "g".to_string(),
                3_600,
                window,
                cx,
            )
        })
    });
    assert!(ghost, "ghost edit closes with a warning");
    assert_eq!(notification_count(cx), 2, "ghost warns once more");
}

#[test]
fn preset_copy_names_reasons_never_content() {
    use manyproxy_ui::store::RoutePreset;
    use manyproxy_ui::{
        preset_delete_confirm_description, preset_error_message, preset_row_summary,
    };
    let error = manyproxy_ui::preset::decode_route_link("throne://route/!!!").expect_err("fails");
    let text = preset_error_message(&error);
    assert!(text.contains("Invalid routing preset"), "{text}");
    let preset = RoutePreset {
        id: "1".to_string(),
        name: "ads".to_string(),
        source: "link".to_string(),
        source_url: None,
        rules_json: r#"[{"action":"block"}]"#.to_string(),
        updated_at: None,
        description: None,
        is_builtin: false,
        last_used_at: None,
        locked: false,
    };
    let summary = preset_row_summary(&preset, &["\"Row\"".to_string()]);
    assert!(summary.contains("ads"), "{summary}");
    assert!(summary.contains("1 rule"), "{summary}");
    assert!(summary.contains("\"Row\""), "{summary}");
    let lone = preset_row_summary(&preset, &[]);
    assert!(lone.contains("unattached"), "{lone}");
    let confirm = preset_delete_confirm_description("ads", &["\"Row\"".to_string()]);
    assert!(confirm.contains("ads"), "{confirm}");
    assert!(confirm.contains("\"Row\""), "{confirm}");
    assert!(confirm.contains("detaches"), "{confirm}");
}

#[gpui_kit::test]
fn preset_route_link_commit_stores_preset(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let done = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(t37_route_link("ads"), "g".to_string(), None, window, cx)
        })
    });
    assert!(done, "route link commits and closes");
    let presets = view.read_with(cx, |view, _| view.store.list_presets().expect("list works"));
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].name, "ads");
    assert_eq!(
        presets[0].rules().expect("rules decode").len(),
        1,
        "rules stored"
    );
}

#[gpui_kit::test]
fn preset_json_commit_stores_preset_and_rejects_bad_rules(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let done = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(
                r#"{"name":"office","rules":[{"action":"direct"}]}"#.to_string(),
                "g".to_string(),
                None,
                window,
                cx,
            )
        })
    });
    assert!(done, "preset JSON commits and closes");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .store
            .list_presets()
            .expect("list works")
            .len()),
        1
    );
    let kept_open = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(
                r#"{"name":"bad","rules":[{"action":"route"},{"SECRET":true}]}"#.to_string(),
                "g".to_string(),
                None,
                window,
                cx,
            )
        })
    });
    assert!(!kept_open, "malformed rules keep the dialog open");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .store
            .list_presets()
            .expect("list works")
            .len()),
        1,
        "nothing stored on failure"
    );
    assert_eq!(notification_count(cx), 2, "success + error notices");
}

#[gpui_kit::test]
fn preset_attach_and_detach_commit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store.add(seed_input("Work")).expect("seed works");
    let preset = store
        .create_preset(
            "ads",
            "manual",
            None,
            &[serde_json::json!({"action": "block"})],
        )
        .expect("preset seeds");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_attach_ids(
                std::slice::from_ref(&row.id),
                Some(preset.id.clone()),
                window,
                cx,
            );
        });
    });
    let attached = view.read_with(cx, |view, _| {
        view.store
            .get(&row.id)
            .expect("get works")
            .expect("row kept")
    });
    assert_eq!(attached.preset_id.as_deref(), Some(preset.id.as_str()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_attach_ids(std::slice::from_ref(&row.id), None, window, cx);
        });
    });
    let detached = view.read_with(cx, |view, _| {
        view.store
            .get(&row.id)
            .expect("get works")
            .expect("row kept")
    });
    assert_eq!(detached.preset_id, None);
}

#[gpui_kit::test]
fn laneb_preset_attach_dialog_detaches_inline_with_rule_count(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store.add(seed_input("Work")).expect("seed works");
    let preset = store
        .create_preset(
            "ads",
            "manual",
            None,
            &[serde_json::json!({"action": "block"})],
        )
        .expect("preset seeds");
    let (view, cx) = mount_view(store, cx);
    // K1: the inspector's Presets pane is gone — attach/detach lives in the
    // preset-attach dialog. Unattached, the preset offers its Attach button.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let config = view.row_config(&row.id).expect("row reads");
            view.open_preset_attach_dialog(config.clone(), vec![config.id.clone()], window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-attach-{}", preset.id))
            .is_some()),
        "the unattached preset offers an Attach button"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-detach").is_none()),
        "no Detach button before the attach lands"
    );
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click(format!("preset-attach-{}", preset.id), cx);
    });
    cx.run_until_parked();
    let attached = view.read_with(cx, |view, _| {
        view.store
            .get(&row.id)
            .expect("get works")
            .expect("row kept")
    });
    assert_eq!(
        attached.preset_id.as_deref(),
        Some(preset.id.as_str()),
        "dialog attach persists to the store"
    );
    assert_eq!(notification_count(cx), before + 1, "attach toasts");
    // Reopened attached, the same line offers Detach inline.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let config = view.row_config(&row.id).expect("row reads");
            view.open_preset_attach_dialog(config.clone(), vec![config.id.clone()], window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("preset-detach").is_some()),
        "the attached preset offers an inline Detach"
    );
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("preset-detach", cx);
    });
    cx.run_until_parked();
    let detached = view.read_with(cx, |view, _| {
        view.store
            .get(&row.id)
            .expect("get works")
            .expect("row kept")
    });
    assert_eq!(detached.preset_id, None, "inline detach clears");
    assert_eq!(notification_count(cx), before + 1, "detach toasts");
    // Detach never touches the preset itself — its rule count survives.
    let rules = view.read_with(cx, |view, _| {
        view.store
            .get_preset(&preset.id)
            .expect("get works")
            .expect("preset kept")
            .rules()
            .expect("rules decode")
            .len()
    });
    assert_eq!(rules, 1, "the preset keeps its single rule");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn presets_dialog_opens_with_footer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .create_preset(
            "ads",
            "manual",
            None,
            &[serde_json::json!({"action": "block"})],
        )
        .expect("preset seeds");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("presets-dialog-ok").is_some()));
    assert!(cx.update(|window, _| window.try_find("presets-dialog-cancel").is_some()));
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
}

#[test]
fn appearance_mapping_and_summaries() {
    use gpui_kit::component::ThemeMode;
    use manyproxy_ui::{appearance_theme_mode, preset_summary_text};
    assert_eq!(appearance_theme_mode("dark", true), ThemeMode::Dark);
    assert_eq!(appearance_theme_mode("dark", false), ThemeMode::Dark);
    assert_eq!(appearance_theme_mode("light", false), ThemeMode::Light);
    assert_eq!(appearance_theme_mode("system", true), ThemeMode::Light);
    assert_eq!(appearance_theme_mode("system", false), ThemeMode::Dark);
    assert_eq!(appearance_theme_mode("midnight", true), ThemeMode::Dark);
    assert!(
        preset_summary_text(Some(0)).contains("0 routing presets"),
        "honest zero"
    );
    assert!(
        preset_summary_text(None).contains("unavailable"),
        "broken read never prints a lying zero"
    );
}

#[gpui_kit::test]
fn settings_spine_lists_one_column_and_filters_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    assert_eq!(store.appearance(), "dark", "fresh store reads dark");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));

    // B17: six caps section headers in fixed order, in ONE column. A section
    // below the viewport is not painted, so the walk scrolls the column and
    // checks the order at every step.
    let steps = settings_spine_walk(cx);
    for step in &steps {
        let mut sorted = step.clone();
        sorted.sort_by_key(|id| SETTINGS_HEADERS.iter().position(|h| h == id));
        assert_eq!(
            step, &sorted,
            "each painted set keeps the column's order: {step:?}"
        );
    }
    let seen: Vec<&'static str> = steps.iter().flatten().copied().collect();
    for id in SETTINGS_HEADERS {
        assert!(
            seen.contains(&id),
            "{id} is reachable by scrolling the one column; saw {seen:?}"
        );
    }
    // Back to the top for the rest of the test.
    cx.update(|window, cx| {
        window.scroll(
            "settings-body",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_dialog_open(cx);

    // B17: every section renders in that one column — no rail, no hidden panes.
    assert_painted(cx, "settings-body-general");
    for id in [
        "settings-row-appearance",
        "settings-appearance-group",
        "settings-row-accent",
        "settings-accent-swatches",
    ] {
        assert_painted(cx, id);
    }
    // The accent row makes General taller than one viewport; the rest of the
    // section is one small scroll down.
    for id in [
        "settings-row-window-behaviour",
        "settings-row-show-tray",
        "settings-row-autostart",
        "settings-row-restore",
    ] {
        scroll_settings_into_view(cx, id);
        assert_painted(cx, id);
    }
    scroll_settings_into_view(cx, "settings-row-window-corners");
    assert_painted(cx, "settings-row-window-corners");
    // The link-handler row makes General taller than one viewport; Probing is
    // still there, one small scroll down.
    scroll_settings_into_view(cx, "settings-body-probing");
    assert_painted(cx, "settings-body-probing");
    cx.update(|window, cx| {
        window.scroll(
            "settings-body",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    // T-04: the Routing and Preset summaries left Settings; the presets
    // manager is reached from the Routing menu and the inspector pane.
    for id in [
        "settings-routing-summary",
        "settings-open-routing-presets",
        "settings-preset-summary",
        "settings-section-hotkey",
    ] {
        assert_absent(cx, id.to_string());
    }
    // B17: the rail is gone — the column replaced it.
    for id in ["settings-rail", "settings-rail-general"] {
        assert_absent(cx, id.to_string());
    }

    // B17: the filter field sits above the column with its `N of M` badge.
    assert_painted(cx, "settings-filter");
    assert_painted(cx, "settings-filter-count");
    let (filter_bottom, general_top) = cx.update(|window, _| {
        (
            window.find("settings-filter-band").bounds().bottom(),
            window.find("settings-section-general").bounds().origin.y,
        )
    });
    assert!(
        filter_bottom <= general_top,
        "the filter band sits above the column: band {filter_bottom:?} vs column {general_top:?}"
    );

    // B17: the label column is one width and the longest label fits it — a
    // label that wraps reads as two columns of label text beside the controls.
    let longest = "Maximum kept log lines";
    let shaped = cx.update(|window, _| {
        let style = window.text_style();
        let mut font = gpui_kit::font(style.font_family.clone());
        font.weight = style.font_weight;
        font.style = style.font_style;
        let runs = vec![gpui_kit::TextRun {
            len: longest.len(),
            font,
            color: gpui_kit::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        window
            .text_system()
            .shape_line(
                longest.into(),
                style.font_size.to_pixels(window.rem_size()),
                &runs,
                None,
            )
            .width
    });
    assert!(
        shaped <= px(manyproxy_ui::app::settings::SETTINGS_LABEL_WIDTH),
        "the longest label fits the label column: text {shaped:?} vs column {:?}",
        px(manyproxy_ui::app::settings::SETTINGS_LABEL_WIDTH)
    );

    // B17: the filter narrows by label, and a section with nothing left takes
    // its header with it.
    cx.update(|window, cx| {
        window.click("settings-filter", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.input("cores", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.scroll(
            "settings-body",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_eq!(
        settings_headers_painted(cx),
        vec!["settings-section-engine"],
        "only the section holding the match keeps its header"
    );
    assert_painted(cx, "settings-engine-cores-help");
    assert!(
        cx.update(|window, _| window.try_find("settings-row-autostart").is_none()),
        "the filtered-out rows leave the column"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.appearance()),
        "dark",
        "filtering writes nothing"
    );

    // B17: a query that matches nothing says so instead of showing an empty
    // column with no explanation.
    cx.update(|window, cx| {
        window.input("zzzz", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_painted(cx, "settings-filter-empty");
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
}

/// K9/A7-2: the Settings footer is ONE pair — `Cancel` (secondary) beside
/// `Save` (primary) — with the Esc note; `Save` commits dirty text fields and
/// `Cancel`/Esc discard them. No section carries its own Save and `Done` is
/// gone.
#[gpui_kit::test]
fn k9_settings_one_footer_saves_dirty_text_and_cancel_discards(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    let esc = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");

    let open = |view: &Entity<AppView>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
        });
        paint_dialog_open(cx);
    };

    open(&view, cx);
    assert_painted(cx, "settings-dialog-cancel");
    assert_painted(cx, "settings-dialog-save");
    assert_painted(cx, "settings-footer-esc-note");
    assert_absent(cx, "settings-dialog-done".to_string());
    let (cancel, save) = cx.update(|window, _| {
        (
            window.find("settings-dialog-cancel").bounds(),
            window.find("settings-dialog-save").bounds(),
        )
    });
    assert!(
        cancel.right() < save.left(),
        "Cancel sits left of the primary Save: {cancel:?} vs {save:?}"
    );
    assert!(
        save.origin.y > px(0.),
        "the footer Save sits at the dialog bottom, not mid-body"
    );

    // Save commits the one dirty text field, silently, and closes.
    let before = notification_count(cx);
    let cap = view
        .read_with(cx, |view, _| view.logs.cap_input.clone())
        .expect("the log-cap field exists while Settings is open");
    cx.update(|window, cx| {
        cap.update(cx, |state, cx| state.set_value("1234", window, cx));
    });
    cx.update(|window, cx| window.click("settings-dialog-save", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Save closes Settings"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.log_max_lines()),
        1234,
        "the footer Save commits the dirty field"
    );
    assert_eq!(
        notification_count(cx),
        before,
        "a clean save fires no per-field toast"
    );

    // Cancel discards a dirty field.
    open(&view, cx);
    let cap = view
        .read_with(cx, |view, _| view.logs.cap_input.clone())
        .expect("the log-cap field exists while Settings is open");
    cx.update(|window, cx| {
        cap.update(cx, |state, cx| state.set_value("999", window, cx));
    });
    cx.update(|window, cx| window.click("settings-dialog-cancel", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel closes Settings"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.log_max_lines()),
        1234,
        "Cancel stores nothing"
    );

    // Esc is the same discard route.
    open(&view, cx);
    let cap = view
        .read_with(cx, |view, _| view.logs.cap_input.clone())
        .expect("the log-cap field exists while Settings is open");
    cx.update(|window, cx| {
        cap.update(cx, |state, cx| state.set_value("777", window, cx));
    });
    cx.update(|window, cx| window.dispatch_keystroke(esc, cx));
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Esc closes Settings"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.log_max_lines()),
        1234,
        "Esc stores nothing"
    );
}

/// K9/A7-4: the four maintenance ops are one 2×2 grid of neutral, full-cell
/// buttons; the orphan `Library` label and the per-row wrappers are gone.
#[gpui_kit::test]
fn k9_maintenance_grid_is_two_by_two(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-maintenance-grid");

    assert_painted(cx, "settings-maintenance-grid");
    let ids = [
        "settings-clear-traffic",
        "settings-clear-health",
        "settings-clear-snapshot",
        "settings-reset-library",
    ];
    for id in ids {
        assert_painted(cx, id);
        assert_absent(cx, id.replace("settings-", "settings-row-"));
    }
    // The orphan `Library` label row is gone with the red slabs.
    assert_absent(cx, "settings-row-reset-library".to_string());

    let bounds: Vec<_> = ids
        .iter()
        .map(|id| cx.update(|window, _| window.find(*id).bounds()))
        .collect();
    let (traffic, health, snapshot, reset) = (bounds[0], bounds[1], bounds[2], bounds[3]);
    assert_eq!(
        traffic.origin.y, health.origin.y,
        "row 1 shares one baseline"
    );
    assert_eq!(
        snapshot.origin.y, reset.origin.y,
        "row 2 shares one baseline"
    );
    assert!(
        traffic.origin.y < snapshot.origin.y,
        "the grid really has two rows: {traffic:?} vs {snapshot:?}"
    );
    assert!(
        traffic.origin.x < health.origin.x,
        "the grid really has two columns: {traffic:?} vs {health:?}"
    );
    let width = traffic.size.width;
    for (id, bounds) in ids.iter().zip(bounds.iter()) {
        assert!(
            (bounds.size.width - width).abs() <= px(0.5),
            "{id} fills its grid cell: {:?} vs {width:?}",
            bounds.size.width
        );
        assert!(
            width > px(200.),
            "the grid button fills half the body, not its content: {width:?}"
        );
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

/// K9/A7-5: Engine rows are lean summaries (`sing-box · ready`) with one
/// `Details` tooltip carrying the full path; no path line, no timer jargon.
#[gpui_kit::test]
fn k9_engine_rows_are_lean_summaries(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-row-engine-xray");

    for (row, name) in [
        ("settings-row-engine-sing-box", "sing-box"),
        ("settings-row-engine-sing-box-awg", "sing-box-awg"),
        ("settings-row-engine-xray", "xray"),
    ] {
        assert_painted(cx, row);
        assert_painted(cx, &format!("settings-engine-details-{name}"));
        assert_painted(cx, &format!("settings-engine-summary-{name}"));
        let height = cx.update(|window, _| window.find(row).bounds().size.height);
        assert!(
            height <= px(40.),
            "{name}'s row is one lean line, not a path block: {height:?}"
        );
    }
    assert_absent(cx, "settings-ticks".to_string());

    use manyproxy_ui::app::settings::{engine_details_tooltip, engine_summary};
    assert_eq!(engine_summary("sing-box", true), "sing-box · ready");
    assert_eq!(engine_summary("xray", false), "xray · not found");
    let ready = engine_details_tooltip(
        "sing-box",
        Some(std::path::Path::new(r"C:\Apps\ManyProxy\sing-box.exe")),
    );
    assert!(
        ready.contains(r"C:\Apps\ManyProxy\sing-box.exe"),
        "Details carries the full path: {ready}"
    );
    let missing = engine_details_tooltip("xray", None);
    assert!(
        missing.contains("PATH is never searched"),
        "a missing engine states the lookup rule: {missing}"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

/// K9: the rail's six names are stable and `Updates` is plural everywhere —
/// the stray `Updates settings` chip was the rail tooltip restating the label
/// (removed), not a second nav item.
#[test]
fn k9_settings_nav_labels_are_stable() {
    use manyproxy_ui::app::settings::SettingsSection;
    let titles: Vec<&str> = SettingsSection::ORDER.iter().map(|s| s.title()).collect();
    assert_eq!(
        titles,
        [
            "General",
            "Probing",
            "Updates",
            "Engine",
            "Maintenance",
            "Shortcuts"
        ]
    );
    assert_eq!(SettingsSection::Updates.title(), "Updates");
    assert!(
        !titles.contains(&"Update"),
        "no singular Update nav label exists"
    );
}

/// K9/A7-3 + B17: the column caps at the viewport and scrolls past it, and it
/// hugs a short result instead of holding a fixed frame. The body never paints
/// under the footer either way.
#[gpui_kit::test]
fn k9_settings_body_auto_heights(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);

    // The whole spine is longer than the viewport, so the body stops at the
    // cap instead of growing the dialog off screen.
    let capped = cx.update(|window, _| window.find("settings-shell").bounds().size.height);
    assert!(
        capped < px(520.),
        "the column caps at the viewport instead of growing: {capped:?}"
    );
    let (last_header_bottom, body_bottom, footer_top) = cx.update(|window, _| {
        (
            window
                .try_find("settings-section-shortcuts".to_string())
                .map(|snapshot| snapshot.bounds().bottom())
                .unwrap_or(px(0.)),
            window.find("settings-body").bounds().bottom(),
            window.find("settings-dialog-save").bounds().origin.y,
        )
    });
    assert!(
        last_header_bottom > body_bottom,
        "the long column scrolls past the body's edge: shortcuts {last_header_bottom:?} vs body {body_bottom:?}"
    );
    assert!(
        body_bottom <= footer_top,
        "the body never paints under the footer: body {body_bottom:?} vs footer {footer_top:?}"
    );

    // A filter that leaves one row shrinks the dialog instead of holding the
    // capped frame — the body hugs its content.
    cx.update(|window, cx| window.click("settings-filter", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.input("log cap", cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let short = cx.update(|window, _| window.find("settings-shell").bounds().size.height);
    assert!(
        short < capped,
        "a filtered column hugs its content: {short:?} vs capped {capped:?}"
    );
    let (body_bottom, footer_top) = cx.update(|window, _| {
        (
            window.find("settings-body").bounds().bottom(),
            window.find("settings-dialog-save").bounds().origin.y,
        )
    });
    assert!(
        body_bottom <= footer_top,
        "the short body still clears the footer: body {body_bottom:?} vs footer {footer_top:?}"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn nav_links_into_the_presets_manager(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    assert!(
        cx.update(|window, _| window.try_find("nav-routing-presets").is_some()),
        "the nav carries Routing presets"
    );
    // T-04: the routing summary left Settings — the nav links into the
    // presets manager instead, and the link is not dead.
    cx.update(|window, cx| {
        window.click("nav-routing-presets", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the nav opens the presets manager"
    );
    assert!(
        cx.update(|window, _| window.try_find("presets-dialog-ok").is_some()),
        "the presets manager renders its own footer"
    );
    assert!(
        cx.update(|window, _| window.try_find("presets-summary").is_some()),
        "the presets manager carries the inventory the Settings dialog gave up"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    cx.update(|window, cx| {
        window.click("nav-settings", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the nav's Settings opens the Settings dialog"
    );
    assert!(
        cx.update(|window, _| window.try_find("settings-section-general").is_some()),
        "the dialog carries the settings spine"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    let _ = view;
}

#[gpui_kit::test]
fn appearance_choice_persists_and_paints_live(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    use gpui_kit::component::ThemeMode;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        assert_eq!(cx.theme().mode, ThemeMode::Dark, "dark paints by default");
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.commit_appearance("light", window, cx), "light commits");
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.store.appearance()),
        "light",
        "choice persists"
    );
    cx.update(|_, cx| {
        assert_eq!(cx.theme().mode, ThemeMode::Light, "light paints live");
    });
    assert_eq!(notification_count(cx), before + 1, "commit toasts once");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.commit_appearance("dark", window, cx), "dark commits");
        });
    });
    cx.update(|_, cx| {
        assert_eq!(cx.theme().mode, ThemeMode::Dark, "dark paints live");
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                !view.commit_appearance("midnight", window, cx),
                "junk fails loudly"
            );
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.store.appearance()),
        "dark",
        "failed write keeps the old choice"
    );
}

#[gpui_kit::test]
fn accent_choice_persists_paints_live_and_survives_a_theme_swap(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    use manyproxy_ui::store::Accent;

    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "settings-row-accent");
    assert_painted(cx, "settings-accent-violet");

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_accent(Accent::Violet, window, cx),
                "a preset commits"
            );
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.store.accent()),
        Accent::Violet,
        "the pick persists"
    );
    cx.update(|_, cx| {
        let theme = cx.theme();
        assert!(
            (theme.list_active.h - 0.75).abs() < 0.03,
            "selection follows the accent: {:?}",
            theme.list_active
        );
        assert!(
            (theme.primary.h - 0.75).abs() < 0.03,
            "checked controls follow the accent: {:?}",
            theme.primary
        );
        assert!(
            (theme.ring.h - 0.75).abs() < 0.03,
            "the focus ring follows the accent: {:?}",
            theme.ring
        );
    });
    assert_eq!(notification_count(cx), before + 1, "each pick toasts once");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_appearance("light", window, cx),
                "the theme still swaps"
            );
        });
    });
    cx.update(|_, cx| {
        assert!(
            (cx.theme().list_active.h - 0.75).abs() < 0.03,
            "the accent is re-stamped after Theme::change"
        );
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_accent(Accent::Blue, window, cx),
                "the default is reachable again"
            );
        });
    });
    cx.update(|_, cx| {
        let selection = cx.theme().list_active;
        assert!(
            (0.55..0.70).contains(&selection.h) && selection.s > 0.5,
            "blue restores the kit's selection, got {selection:?}"
        );
    });
}

#[gpui_kit::test]
fn window_corner_choice_persists_and_paints_in_settings(cx: &mut TestAppContext) {
    use manyproxy_ui::store::WindowCorners;

    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-row-window-corners");
    assert_painted(cx, "settings-row-window-corners");

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_window_corners(WindowCorners::Square, window, cx),
                "Square commits"
            );
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.store.window_corners()),
        WindowCorners::Square,
        "the pick persists"
    );
    assert_eq!(notification_count(cx), before + 1, "the pick toasts once");

    // The OS call is best-effort under the harness (no HWND); it must not
    // panic, and the choice stays readable for the next boot's apply.
    assert_eq!(
        view.read_with(cx, |view, _| view.store.window_corners()),
        WindowCorners::Square
    );
}

#[gpui_kit::test]
fn attached_start_serves_preset_rules_first(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(NewConfig {
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let preset = store
        .create_preset(
            "ads",
            "manual",
            None,
            &[serde_json::json!({"action":"route","domain":["example.com"]})],
        )
        .expect("preset seeds");
    store
        .set_config_preset(&row.id, Some(&preset.id))
        .expect("attach works");
    let (view, cx) = mount_view(store, cx);
    let (engine, stock, _, _, _) = EngineController::fake_quartet();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = engine;
        });
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.reload_quiet(cx);
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_engine(window, cx));
    });
    let served: serde_json::Value =
        serde_json::from_str(&stock.last_config().expect("sidecar served a config"))
            .expect("served JSON parses");
    let rules = served["route"]["rules"].as_array().expect("rules array");
    assert_eq!(rules.len(), 2, "preset rule + isolation, got {rules:?}");
    assert_eq!(rules[0]["action"], serde_json::json!("route"));
    assert_eq!(rules[0]["domain"], serde_json::json!(["example.com"]));
    assert!(
        rules[1].get("domain").is_none(),
        "isolation closes the table"
    );
}

#[test]
fn t10_format_uptime_units() {
    use manyproxy_ui::format_uptime;
    assert_eq!(format_uptime(0), "0s");
    assert_eq!(format_uptime(45), "45s");
    assert_eq!(format_uptime(125), "2m 5s");
    assert_eq!(format_uptime(5 * 3_600 + 12 * 60), "5h 12m");
    assert_eq!(format_uptime(3 * 86_400 + 4 * 3_600), "3d 4h");
}

#[test]
fn t10_traffic_stacked_bar_splits_rx_tx() {
    use manyproxy_ui::traffic_stacked_bar;
    let full = traffic_stacked_bar(800, 200, 1000);
    assert_eq!(full.chars().count(), 12, "{full}");
    assert!(full.starts_with("██"), "rx leads: {full}");
    assert!(full.contains('▓'), "tx stacks: {full}");
    let empty = traffic_stacked_bar(0, 0, 0);
    assert_eq!(empty, "░".repeat(12), "zero max never divides: {empty}");
    let heaviest = traffic_stacked_bar(500, 500, 1000);
    assert!(!heaviest.contains('░'), "heaviest fills: {heaviest}");
}

#[test]
fn t10_traffic_stats_summary_labels_all_time() {
    use manyproxy_ui::store::TrafficPeriod;
    use manyproxy_ui::traffic_stats_summary_text;
    let text = traffic_stats_summary_text(TrafficPeriod::Week, 2, 4_000, 140);
    assert!(text.contains("7d"), "{text}");
    assert!(text.contains("all-time"), "{text}");
    assert!(text.contains("2 exits"), "{text}");
}

#[gpui_kit::test]
fn t10_share_dialog_copies_nonempty_link(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store.add(tick12_seed("Work", "10871")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_share_for_row(&row.id, window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "share opens a dialog"
    );
    assert!(
        cx.update(|window, _| window.try_find("share-link-text").is_some()),
        "dialog shows the link"
    );
    assert!(
        cx.update(|window, _| window.try_find("share-warning").is_some()),
        "credential warning rides along"
    );

    cx.update(|window, cx| {
        window.click("share-dialog-ok", cx);
    });
    cx.run_until_parked();
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert!(!clip.is_empty(), "share copy never empty");
    assert!(clip.starts_with("manyproxy://add/"), "{clip}");

    let back = manyproxy_ui::import::parse_link(&clip).expect("share re-imports");
    assert_eq!(back.protocol, "vless");
    assert_eq!(back.host, "198.51.100.9");
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_share_for_row("9999", window, cx));
    });
    assert_eq!(notification_count(cx), before + 1, "ghost warns");
}

#[gpui_kit::test]
fn t10_preset_export_link_copies_decodable_link(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let preset = store
        .create_preset(
            "office",
            "manual",
            None,
            &[serde_json::json!({"action":"direct"})],
        )
        .expect("preset seeds");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_export_link(&preset, window, cx)
        });
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert!(!clip.is_empty(), "export copy never empty");
    let parsed = manyproxy_ui::preset::decode_route_link(&clip).expect("export decodes");
    assert_eq!(parsed.name, "office");
    assert_eq!(parsed.rules.len(), 1);
}

#[gpui_kit::test]
fn t10_preset_update_lands_fetched_rules(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let preset = store
        .import_remote_preset_shell("office", "https://example.com/preset.json")
        .expect("shell seeds");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_preset_refresh(
                &preset.id,
                &preset.name,
                Ok(manyproxy_ui::FetchedSubscription {
                    body: r#"{"name":"office","rules":[{"action":"block"}]}"#.to_string(),
                    userinfo: None,
                }),
                window,
                cx,
            );
        });
    });
    let rules = view.read_with(cx, |view, _| {
        view.store
            .get_preset(&preset.id)
            .expect("read works")
            .expect("preset kept")
            .rules()
            .expect("rules decode")
    });
    assert_eq!(rules.len(), 1, "fetched rules land");
    assert_eq!(rules[0]["action"], serde_json::json!("block"));
}

#[gpui_kit::test]
fn t10_advanced_rule_ops_rewrite_rules_box(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let text = cx.update(|window, cx| {
        view.update(cx, |_view, cx| {
            let form = manyproxy_ui::PresetForm::restored(
                window,
                cx,
                "office",
                r#"[{"action":"direct"},{"action":"route","outbound":"tag"}]"#,
            );
            manyproxy_ui::AppView::rewrite_preset_rules_box(
                &form,
                &manyproxy_ui::PresetRulesOp::CycleAction(0),
                window,
                cx,
            );
            form.rules.read(cx).value().to_string()
        })
    });
    let rules: Vec<serde_json::Value> = serde_json::from_str(&text).expect("box stays JSON");
    assert_eq!(
        rules[0]["action"],
        serde_json::json!("proxy"),
        "direct cycles to proxy"
    );
    let text = cx.update(|window, cx| {
        view.update(cx, |_view, cx| {
            let form = manyproxy_ui::PresetForm::restored(window, cx, "office", &text);
            manyproxy_ui::AppView::rewrite_preset_rules_box(
                &form,
                &manyproxy_ui::PresetRulesOp::Remove(1),
                window,
                cx,
            );
            form.rules.read(cx).value().to_string()
        })
    });
    let rules: Vec<serde_json::Value> = serde_json::from_str(&text).expect("box stays JSON");
    assert_eq!(rules.len(), 1, "delete drops the rule");
    let text = cx.update(|window, cx| {
        view.update(cx, |_view, cx| {
            let form = manyproxy_ui::PresetForm::restored(window, cx, "office", &text);
            manyproxy_ui::AppView::rewrite_preset_rules_box(
                &form,
                &manyproxy_ui::PresetRulesOp::Add,
                window,
                cx,
            );
            form.rules.read(cx).value().to_string()
        })
    });
    let rules: Vec<serde_json::Value> = serde_json::from_str(&text).expect("box stays JSON");
    assert_eq!(rules.len(), 2, "add appends");
    assert_eq!(rules[1]["action"], serde_json::json!("direct"));
}

#[gpui_kit::test]
fn t59_editor_folds_render_new_keys(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let preset = store
        .create_preset(
            "t59-folds",
            "paste",
            None,
            &[
                serde_json::json!({
                    "action": "route",
                    "source_ip_cidr": ["10.0.0.0/8"],
                    "source_port": 8080,
                }),
                serde_json::json!({
                    "action": "direct",
                    "source_ip_is_private": true,
                    "source_port_range": "1000:2000",
                    "ip_version": 6,
                    "invert": true,
                    "process_path_regex": "thm\\.exe$",
                }),
                serde_json::json!({"action": "direct"}),
            ],
        )
        .expect("preset seeds");
    let fold_id = |index: usize, key: &str| format!("preset-rule-fold-{}-{index}-{key}", preset.id);
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_edit_dialog(preset.clone(), window, cx);
        });
    });
    paint_dialog_open(cx);
    for (index, key) in [
        (0, "source_ip_cidr"),
        (0, "source_port"),
        (1, "source_ip_is_private"),
        (1, "source_port_range"),
        (1, "ip_version"),
        (1, "invert"),
        (1, "process_path_regex"),
    ] {
        assert!(
            cx.update(|window, _| window.try_find(fold_id(index, key)).is_some()),
            "fold row {index}/{key} renders in the preset edit dialog"
        );
    }
    for key in ["source_ip_cidr", "invert"] {
        assert!(
            cx.update(|window, _| window.try_find(fold_id(2, key)).is_none()),
            "bare rule renders no {key} fold"
        );
    }
}

#[gpui_kit::test]
fn t60_locked_preset_renders_readonly(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let rules = vec![
        serde_json::json!({
            "action": "route", "name": "Office exit",
            "domain": ["example.com"],
            "source_ip_cidr": ["10.0.0.0/8"],
        }),
        serde_json::json!({"action": "direct", "name": "Local"}),
    ];
    let preset = store
        .create_preset("locked-one", "manual", None, &rules)
        .expect("preset seeds");
    let pid = preset.id.clone();
    store.set_preset_locked(&pid, true).expect("lock seeds");
    // Re-read so the snapshot carries the locked flag, the way the
    // store-fed flow always opens the dialog.
    let preset = store
        .get_preset(&pid)
        .expect("read works")
        .expect("preset kept");
    let (view, cx) = mount_view(store, cx);

    // The manager row offers the explicit toggle...
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find(format!("preset-lock-{pid}")).is_some()),
        "manager row offers the lock toggle"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    // ...and the editor renders readonly for a locked preset.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_edit_dialog(preset.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-locked-hint-{pid}"))
            .is_some()),
        "the lock hint renders"
    );
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-rule-name-{pid}-0"))
            .is_some()),
        "named rules still render their label column"
    );
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-rule-outbound-{pid}-0"))
            .is_some()),
        "route rules still show their outbound line"
    );
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-rule-fold-{pid}-0-source_ip_cidr"))
            .is_some()),
        "attribute folds render view-only"
    );
    for absent in [
        format!("preset-rule-action-{pid}-0"),
        format!("preset-rule-delete-{pid}-0"),
        format!("preset-rule-add-{pid}"),
    ] {
        assert!(
            cx.update(|window, _| window.try_find(absent.clone()).is_none()),
            "locked editor renders no {absent}"
        );
    }
    assert!(
        cx.update(|window, _| window.try_find("preset-edit-ok").is_none()),
        "locked editor renders no save path"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    // The same explicit toggle unlocks; the editor gains its editing
    // surface back.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_lock(&pid, false, window, cx)
        });
    });
    let fresh = view.read_with(cx, |view, _| {
        view.store
            .get_preset(&pid)
            .expect("read works")
            .expect("preset kept")
    });
    assert!(!fresh.locked, "unlock persists");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_edit_dialog(fresh, window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("preset-edit-ok").is_some()),
        "unlocked editor restores the save path"
    );
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-rule-action-{pid}-0"))
            .is_some()),
        "unlocked editor cycles actions again"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t60_completion_lists_valid_exits_only(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    // Pure surface: `direct` first, serving tags after, deduped — exactly
    // the values the engine accepts, nothing invented.
    assert_eq!(
        manyproxy_ui::preset::preset_exit_suggestions(&[]),
        vec!["direct".to_string()]
    );
    let tags = [
        "out-2".to_string(),
        "out-1".to_string(),
        "out-2".to_string(),
        String::new(),
        "direct".to_string(),
    ];
    assert_eq!(
        manyproxy_ui::preset::preset_exit_suggestions(&tags),
        vec![
            "direct".to_string(),
            "out-2".to_string(),
            "out-1".to_string(),
        ],
        "dedup keeps direct first; empty and repeated tags drop"
    );
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(tick12_seed("Alpha", "10871"))
        .expect("seed works");
    let preset = store
        .create_preset(
            "exits",
            "manual",
            None,
            &[
                serde_json::json!({"action": "route", "name": "Office exit"}),
                serde_json::json!({"action": "direct"}),
            ],
        )
        .expect("preset seeds");
    let pid = preset.id.clone();
    store
        .set_config_preset(&row.id, Some(&pid))
        .expect("attach works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_edit_dialog(preset.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-rule-exits-{pid}-0"))
            .is_some()),
        "route rules offer the exits suggestions"
    );
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-rule-exits-{pid}-1"))
            .is_none()),
        "non-route rules offer none"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let config = view.row_config(&row.id).expect("row reads");
            view.open_preset_attach_dialog(config.clone(), vec![config.id.clone()], window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-exits").is_some()),
        "the attach dialog compiles the serving exits list"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t64_dialog_monitor_section_renders_top_five(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, TestOutcome};
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let names = [
        "Fast",
        "Slow",
        "FastTwin",
        "Regressed",
        "Idle",
        "NoSample",
        "Dead",
    ];
    for (index, name) in names.iter().enumerate() {
        store
            .add(t64_seed_in_group(
                name,
                "Work",
                &format!("1081{}", index + 1),
            ))
            .expect("seed works");
    }
    let id_of = {
        let rows = store.list().expect("rows read");
        move |name: &str| {
            rows.iter()
                .find(|row| row.name == name)
                .map(|row| row.id.clone())
                .expect("seeded row id")
        }
    };
    let (view, cx) = mount_view(store, cx);
    let url = |id: String, at: i64, latency: Option<u64>, error: Option<&str>| -> TestOutcome {
        TestOutcome {
            success: None,
            sample: HealthSample {
                config_id: id,
                at,
                kind: HealthKind::UrlTest,
                latency_ms: latency,
                error: error.map(str::to_string),
            },
        }
    };
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for outcome in [
                url(id_of("Fast"), 1, Some(45), None),
                url(id_of("Slow"), 1, Some(220), None),
                url(id_of("FastTwin"), 1, Some(45), None),
                url(id_of("Regressed"), 1, Some(45), None),
                url(id_of("Regressed"), 3, Some(999), None),
                url(id_of("Dead"), 1, None, Some("dial refused")),
            ] {
                view.health_store.record_outcome(&outcome);
            }
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(manyproxy_ui::tray::TrayAction::SelectExit, window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "tray Select-exit opens the attach dialog"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor").is_some()),
        "the monitor section renders"
    );
    for index in 0..5 {
        assert!(
            cx.update(|window, _| window
                .try_find(format!("preset-attach-monitor-{index}"))
                .is_some()),
            "entry {index} renders"
        );
    }
    for index in [5, 6] {
        assert!(
            cx.update(|window, _| window
                .try_find(format!("preset-attach-monitor-{index}"))
                .is_none()),
            "cap holds: entry {index} absent"
        );
    }
    let (title, lines) = view.read_with(cx, |view, _| {
        let rows = view
            .store
            .list()
            .expect("rows read")
            .into_iter()
            .filter(|row| row.group == "Work")
            .collect::<Vec<_>>();
        manyproxy_ui::health::best_rows_monitor_lines("Work", &rows, &view.health_store)
            .expect("monitor lines for the group")
    });
    assert_eq!(title, "Best by health in Work:");
    assert_eq!(
            lines,
            vec![
                "1) Fast (45ms)".to_string(),
                "2) FastTwin (45ms)".to_string(),
                "3) Slow (220ms)".to_string(),
                "4) Regressed (999ms)".to_string(),
                "5) Idle (no probes this session)".to_string(),
            ],
            "top five by latest health: 45-tie stable, 220 next, Regressed keeps its LATEST 999, \
             then the first no-sample row; the second no-sample row and the dead row fall past the cap"
        );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t64_dialog_monitor_hidden_for_empty_group(cx: &mut TestAppContext) {
    use manyproxy_ui::ProxyConfig;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let only = store
        .add(t64_seed_in_group("Only", "Work", "10811"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let phantom = ProxyConfig {
        id: "nobody".to_string(),
        name: "Ghost".to_string(),
        protocol: "vless".to_string(),
        params: "{}".to_string().into(),
        group: "Nobody".to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port: 10998,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_attach_dialog(phantom.clone(), vec![phantom.id.clone()], window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "dialog still opens for a group with no rows"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor").is_none()),
        "empty group hides the monitor entirely — no empty-space glitch"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let config = view.row_config(&only.id).expect("row reads");
            view.open_preset_attach_dialog(config.clone(), vec![config.id.clone()], window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor").is_some()),
        "a single-row group still shows the monitor"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor-0").is_some()),
        "the one entry renders"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor-1").is_none()),
        "nothing follows the single row"
    );
    let (title, lines) = view.read_with(cx, |view, _| {
        let rows = view.store.list().expect("rows read");
        let group = rows
            .first()
            .map(|row| row.group.clone())
            .unwrap_or_default();
        manyproxy_ui::health::best_rows_monitor_lines(&group, &rows, &view.health_store)
            .expect("monitor lines")
    });
    assert_eq!(title, "Best by health in Work:");
    assert_eq!(
        lines,
        vec!["1) Only (no probes this session)".to_string()],
        "all-unknown shape renders sanely"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t64_no_switching_behavior_added(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, TestOutcome};
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let live = store
        .add(t64_seed_in_group("Live45", "Work", "10811"))
        .expect("seed works");
    store
        .add(t64_seed_in_group("NoSample", "Work", "10812"))
        .expect("seed works");
    let down = store
        .add(t64_seed_in_group("Down", "Work", "10813"))
        .expect("seed works");
    let preset = store
        .create_preset(
            "Base",
            "manual",
            None,
            &[serde_json::json!({"action": "direct"})],
        )
        .expect("preset seeds");
    let other = store
        .create_preset(
            "Other",
            "manual",
            None,
            &[serde_json::json!({"action": "proxy", "name": "Live45"})],
        )
        .expect("preset seeds");
    store
        .set_config_preset(&live.id, Some(&preset.id))
        .expect("attach works");
    let before_rows = store.list().expect("rows read");
    let before_presets = store.list_presets().expect("presets read");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.health_store.record_outcome(&TestOutcome {
                success: None,
                sample: HealthSample {
                    config_id: live.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: Some(45),
                    error: None,
                },
            });
            view.health_store.record_outcome(&TestOutcome {
                success: None,
                sample: HealthSample {
                    config_id: down.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: None,
                    error: Some("dial refused".to_string()),
                },
            });
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(manyproxy_ui::tray::TrayAction::SelectExit, window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor").is_some()),
        "the monitor renders for the selected row's group"
    );
    for index in 0..3 {
        assert!(
            cx.update(|window, _| window
                .try_find(format!("preset-attach-monitor-{index}"))
                .is_some()),
            "entry {index} of three renders"
        );
    }
    assert!(
        cx.update(|window, _| window.try_find("preset-attach-monitor-3").is_none()),
        "exactly three entries — capped, nothing extra"
    );
    assert!(
        cx.update(|window, _| window
            .try_find(format!("preset-attach-{}", other.id))
            .is_some()),
        "the Attach button for the unattached preset is untouched"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-detach").is_some()),
        "the Detach button is untouched"
    );
    let ((monitor_title, monitor_lines), after_rows, after_presets, engine_idle, still_attached) =
        view.read_with(cx, |view, _| {
            let rows = view.store.list().expect("rows read");
            let group = rows
                .first()
                .map(|row| row.group.clone())
                .unwrap_or_default();
            let filtered = rows
                .iter()
                .filter(|row| row.group == group)
                .cloned()
                .collect::<Vec<_>>();
            let monitor = manyproxy_ui::health::best_rows_monitor_lines(
                &group,
                &filtered,
                &view.health_store,
            )
            .expect("monitor lines");
            let attached = view
                .row_config(&live.id)
                .and_then(|row| row.preset_id.clone())
                .is_some();
            (
                monitor,
                rows,
                view.store.list_presets().expect("presets read"),
                view.engine.running_ids().is_empty(),
                attached,
            )
        });
    assert_eq!(monitor_title, "Best by health in Work:");
    assert_eq!(
        monitor_lines,
        vec![
            "1) Live45 (45ms)".to_string(),
            "2) NoSample (no probes this session)".to_string(),
            "3) Down (dead)".to_string(),
        ],
        "monitor text is exactly names + row summaries — display-only, nothing added"
    );
    for line in &monitor_lines {
        assert!(!line.contains("vless"), "protocol stays out: {line}");
        assert!(
            !line.contains("198.51.100.9"),
            "params/server addresses stay out: {line}"
        );
        assert!(!line.contains("uuid"), "params stay out: {line}");
        assert!(!line.contains("out-"), "outbound tags stay out: {line}");
    }
    assert_eq!(
        after_rows, before_rows,
        "library state untouched by the dialog"
    );
    assert_eq!(
        after_presets, before_presets,
        "presets untouched by the dialog"
    );
    assert!(
        engine_idle,
        "engine stands still — no outbound was switched"
    );
    assert!(still_attached, "the attachment survives dialog open");
    cx.update(|window, cx| window.close_all_dialogs(cx));
    let (after_close_rows, after_close_presets) = view.read_with(cx, |view, _| {
        (
            view.store.list().expect("rows read"),
            view.store.list_presets().expect("presets read"),
        )
    });
    assert_eq!(
        after_close_rows, before_rows,
        "library still untouched after closing"
    );
    assert_eq!(
        after_close_presets, before_presets,
        "presets still untouched after closing"
    );
}

#[gpui_kit::test]
fn t61_panel_renders_three_boxes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let preset = store
        .create_preset("t61-panel", "manual", None, &[])
        .expect("preset seeds");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_edit_dialog(preset.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("preset-simple-rules").is_some()),
        "the Simple rules fold renders in the edit dialog"
    );
    for action in ["direct", "block", "proxy"] {
        assert!(
            cx.update(|window, _| window
                .try_find(format!("preset-simple-label-{action}"))
                .is_some()),
            "the {action} guideline row renders"
        );
        assert!(
            cx.update(|window, _| window
                .try_find(format!("field-preset-simple-{action}"))
                .is_some()),
            "the {action} box renders"
        );
    }
    assert!(
        cx.update(|window, _| window
            .try_find("field-preset-remote-url".to_string())
            .is_some()),
        "the Add remote ruleset URL row renders"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-new-caption").is_some()),
        "the New preset caption renders"
    );
    // K3: the grammar explainer and the raw rules JSON textarea are gone.
    assert!(
        cx.update(|window, _| window.try_find("preset-simple-help").is_none()),
        "no grammar explainer paragraph"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-preset-rules").is_none()),
        "no Rules JSON textarea"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-simple-invalid-lines").is_none()),
        "boxes start clean: no invalid fold"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    // The manager dialog carries the same fold: every PresetForm surface
    // offers the boxes.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    for id in [
        "preset-simple-rules",
        "preset-simple-label-direct",
        "field-preset-simple-direct",
        "preset-simple-label-block",
        "field-preset-simple-block",
        "preset-simple-label-proxy",
        "field-preset-simple-proxy",
        "field-preset-remote-url",
        "preset-new-caption",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "manager dialog renders {id}"
        );
    }
    assert!(
        cx.update(|window, _| window.try_find("preset-simple-help").is_none()),
        "manager dialog carries no grammar explainer"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-preset-rules").is_none()),
        "manager dialog carries no raw Rules JSON textarea"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t61_remote_url_commit_offers_download_confirm(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let dir = std::env::temp_dir().join(format!(
        "manyproxy-t61remote-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock works")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("staging temp works");
    let _env = t62_rule_sets_env(&dir);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = view.preset_form.clone().expect("manager stashes its form");
            form.name.update(cx, |state, cx| {
                state.set_value("k3-url-preset", window, cx);
            });
            form.remote_url.update(cx, |state, cx| {
                state.set_value("https://rule-sets.example.com/k3ads.srs", window, cx);
            });
        });
    });
    cx.update(|window, cx| window.click("presets-dialog-ok", cx));
    cx.run_until_parked();

    // K3: the URL row is content — a URL-only form still creates the preset.
    let presets = view.read_with(cx, |view, _| view.store.list_presets().expect("list works"));
    assert_eq!(presets.len(), 1, "one preset from the URL-only form");
    assert_eq!(presets[0].name, "k3-url-preset");
    // ...and the same commit hands off to the explicit-download confirm. The
    // handoff is deferred past the footer's close-all, so the confirm must
    // still be on screen here.
    assert!(
        cx.update(|window, _| window.try_find("presets-dialog-ok").is_none()),
        "the manager closed on the successful commit"
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the Download confirm replaced the manager"
    );
    assert!(
        t62_staged_rows(&view, cx).is_empty(),
        "the confirm stages nothing before Download"
    );
    assert!(
        !dir.join("k3ads.srs").exists(),
        "no file is written before the confirm"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    assert!(
        t62_staged_rows(&view, cx).is_empty(),
        "backing out of the confirm never stages"
    );
}

#[gpui_kit::test]
fn t61_save_appends_box_rules(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = view.preset_form.clone().expect("manager stashes its form");
            form.name.update(cx, |state, cx| {
                state.set_value("t61-mix", window, cx);
            });
            form.rules.update(cx, |state, cx| {
                state.set_value(
                    r#"[{"action":"direct","domain":["a.example"]}]"#.to_string(),
                    window,
                    cx,
                );
            });
            form.simple_direct.update(cx, |state, cx| {
                state.set_value("domain:b.example, suffix:c.example".to_string(), window, cx);
            });
            form.simple_block.update(cx, |state, cx| {
                state.set_value("# ports aside\nkeyword:ads".to_string(), window, cx);
            });
            form.simple_proxy.update(cx, |state, cx| {
                state.set_value("ruleset:streaming\nip:1.1.1.1/32".to_string(), window, cx);
            });
        });
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("presets-dialog-ok", cx);
    });
    cx.run_until_parked();
    let presets = view.read_with(cx, |view, _| view.store.list_presets().expect("list works"));
    assert_eq!(presets.len(), 1, "one preset created from both layers");
    let rules = view.read_with(cx, |view, _| {
        view.store
            .get_preset(&presets[0].id)
            .expect("get works")
            .expect("preset there")
            .rules()
            .expect("rules decode")
    });
    assert_eq!(
        rules.len(),
        5,
        "JSON rule + direct line + block line + two proxy lines"
    );
    assert_eq!(
        rules[0],
        serde_json::json!({"action": "direct", "domain": ["a.example"]}),
        "the structured JSON layer stays verbatim"
    );
    assert_eq!(
        rules[1],
        serde_json::json!({
            "action": "direct",
            "domain": ["b.example"],
            "domain_suffix": ["c.example"],
        }),
        "the direct line merges into one rule"
    );
    assert_eq!(
        rules[2],
        serde_json::json!({"action": "block", "domain_keyword": ["ads"]}),
        "comments skip; the block line stamps its Throne spelling"
    );
    assert_eq!(
        rules[3],
        serde_json::json!({"action": "proxy", "rule_set": ["streaming"]}),
        "ruleset: becomes a rule_set ref"
    );
    assert_eq!(
        rules[4],
        serde_json::json!({"action": "proxy", "ip_cidr": ["1.1.1.1/32"]}),
        "one rule per proxy line"
    );
    assert_eq!(notification_count(cx), before + 1, "one honest toast");
}

#[gpui_kit::test]
fn t61_invalid_lines_block_save(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = view.preset_form.clone().expect("manager stashes its form");
            form.name.update(cx, |state, cx| {
                state.set_value("t61-broken", window, cx);
            });
            form.simple_direct.update(cx, |state, cx| {
                state.set_value("domain:ok.example\nwarp:maybe".to_string(), window, cx);
            });
        });
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("presets-dialog-ok", cx);
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "invalid box lines keep the dialog open"
    );
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("preset-simple-invalid-lines").is_some()),
        "the invalid fold mirrors the parse that blocked the save"
    );
    assert!(
        cx.update(|window, _| window.try_find("preset-simple-invalid-0").is_some()),
        "the raw line renders under the fold"
    );
    let presets = view.read_with(cx, |view, _| view.store.list_presets().expect("list works"));
    assert!(presets.is_empty(), "nothing saved while lines are invalid");
    assert!(
        notification_count(cx) > before,
        "the failure is loud, not silent"
    );
    let kept = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.preset_form
                .clone()
                .expect("form kept while the dialog stays open")
                .simple_direct
                .read(cx)
                .value()
                .to_string()
        })
    });
    assert_eq!(
        kept, "domain:ok.example\nwarp:maybe",
        "invalid lines survive verbatim in the box for this session"
    );
}

#[gpui_kit::test]
fn t10_routing_chooser_opens_attach_for_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(tick12_seed("Work", "10871")).expect("seed works");
    store
        .create_preset(
            "ads",
            "manual",
            None,
            &[serde_json::json!({"action":"direct"})],
        )
        .expect("preset seeds");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_chooser_for_selected(window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "chooser opens the attach picker"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection(window, cx);
        });
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_chooser_for_selected(window, cx)
        });
    });
    assert_eq!(
        notification_count(cx),
        before + 1,
        "no selection explains itself"
    );
}

#[gpui_kit::test]
fn t10_tray_select_and_config_copy(cx: &mut TestAppContext) {
    use manyproxy_ui::tray::TrayAction;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(tick12_seed("Work", "10871")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::SelectPreset, window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Select preset opens the manager"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::SelectExit, window, cx)
        });
    });
    assert_eq!(
        notification_count(cx),
        before + 1,
        "no selection explains itself"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::OpenConfig, window, cx)
        });
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(
        clip,
        manyproxy_ui::tray::config_folder(),
        "config folder copies"
    );
    assert!(!clip.is_empty(), "folder never empty");
    assert_eq!(
        std::path::Path::new(&clip)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .as_deref(),
        Some(manyproxy_ui::store::DATA_DIR_NAME),
        "copied config folder is the app-relative data dir"
    );
}

#[gpui_kit::test]
fn restore_on_launch_starts_loopback_and_gates_lan(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let loopback = store
        .add(restore_seed("Loop", "", "10871"))
        .expect("seed works");
    let lan = store
        .add(restore_seed("Lan", "0.0.0.0", "10872"))
        .expect("seed works");
    store.set_restore_running(true).expect("opt-in writes");
    store
        .set_last_clean_exit(true)
        .expect("clean marker writes, no T-34 toast");
    store
        .save_running_snapshot(&[loopback.id.clone(), lan.id.clone(), "9999".to_string()])
        .expect("snapshot writes");

    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.restore_on_launch(window, cx));
    });

    assert!(
        view.read_with(cx, |view, _| view.engine.is_running(&loopback.id)),
        "loopback snapshot row restores at boot, not a frame later"
    );
    assert!(
        !view.read_with(cx, |view, _| view.engine.is_running(&lan.id)),
        "LAN snapshot row waits for its confirm"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "the restore toast lands after Root, never during boot"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "the LAN confirm lands after Root, never during boot"
    );

    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "LAN confirm dialog opens on the first frame after Root"
    );
    assert_eq!(
        notification_count(cx),
        1,
        "the restore toast arrives exactly once"
    );

    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert_eq!(
        notification_count(cx),
        1,
        "the restore toast is one-shot, not one per frame"
    );

    cx.update(|window, cx| window.close_all_dialogs(cx));
    assert!(
        !view.read_with(cx, |view, _| view.engine.is_running(&lan.id)),
        "dismissing the confirm starts nothing"
    );
}

#[gpui_kit::test]
fn restore_off_restores_nothing(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(restore_seed("Loop", "", "10873"))
        .expect("seed works");
    store
        .save_running_snapshot(std::slice::from_ref(&row.id))
        .expect("snapshot writes");

    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.restore_on_launch(window, cx));
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "opt-out restores nothing"
    );
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

/// Defect: boot runs inside `cx.open_window`'s closure, and the window does
/// not adopt its `gpui_component::Root` layer until that closure returns.
/// Pushing a notification or opening a dialog then panics inside
/// `Root::update` ("window first layer should be a gpui_component::Root").
/// A window whose root view is `AppView` itself is exactly the boot shape:
/// every boot path must survive it, because nothing has a Root to reach.
#[gpui_kit::test]
fn boot_paths_survive_a_window_without_root(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let loopback = store
        .add(restore_seed("Loop", "", "10876"))
        .expect("seed works");
    let lan = store
        .add(restore_seed("Lan", "0.0.0.0", "10877"))
        .expect("seed works");
    store.set_restore_running(true).expect("opt-in writes");
    store
        .set_last_clean_exit(true)
        .expect("clean marker writes");
    store
        .save_running_snapshot(&[loopback.id.clone(), lan.id.clone()])
        .expect("snapshot writes");

    // No `Root::new` wrapper: this window has AppView as its first layer,
    // which is what boot sees before the closure hands the Root back.
    let (view, cx) = cx.add_window_view(move |window, cx| AppView::with_store(window, cx, store));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // c) restore: approval toast + LAN confirm both boot-time.
            view.restore_on_launch(window, cx);
            // d) open-with intake: the chooser dialog is boot-time too.
            view.handle_launch_intake(
                OpenWithKind::BareHttp("https://example.com/feed".to_string()),
                window,
                cx,
            );
        });
    });

    assert!(
        view.read_with(cx, |view, _| view.engine.is_running(&loopback.id)),
        "the restore itself still runs at boot, before any Root exists"
    );
    assert!(
        !view.read_with(cx, |view, _| view.engine.is_running(&lan.id)),
        "the LAN row still waits for its confirm"
    );
}

/// Defect: the deferred intake dialog must still open — one frame later,
/// exactly once, and without a notification nobody asked for.
#[gpui_kit::test]
fn launch_intake_opens_the_frame_after_root(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_launch_intake(
                OpenWithKind::BareHttp("https://example.com/feed".to_string()),
                window,
                cx,
            );
        });
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "the intake dialog waits for the frame after boot"
    );
    assert_eq!(notification_count(cx), 0, "boot intake says nothing extra");

    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the intake chooser opens once Root exists"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "the deferred intake is not a toast"
    );
}

/// Defect pin: the functions above must not reach `push_notification` /
/// `open_dialog` / `open_alert_dialog` before the Root exists. Source scan,
/// because the failure is "no Root in this call stack", which only real
/// boot (or a Root-less window) reproduces. Coverage is text, so literals
/// and comments are stripped before the bodies are brace-matched.
#[test]
fn boot_scan_survives_crlf_line_endings() {
    // A checkout with CRLF (Windows `core.autocrlf`) must not make the
    // multi-line markers miss their function.
    let source = format!(
        "{}\n{}",
        include_str!("../../../src/main.rs"),
        include_str!("../src/app/bootstrap.rs"),
    );
    let crlf = source.replace("\r\n", "\n").replace("\n", "\r\n");
    assert!(
        crlf.contains("\r\n"),
        "the CRLF fixture really carries CRLF endings"
    );
    let found = direct_overlay_calls(&crlf, &BOOT_FUNCTIONS);
    assert!(
        found.is_empty(),
        "the same boot functions are clean under CRLF: {found:?}"
    );
}

#[test]
fn boot_paths_never_push_overlays_before_root() {
    let source = format!(
        "{}\n{}",
        include_str!("../../../src/main.rs"),
        include_str!("../src/app/bootstrap.rs"),
    );
    let found = direct_overlay_calls(&source, &BOOT_FUNCTIONS);
    assert!(
        found.is_empty(),
        "these boot functions reach an overlay before the Root exists; \
             use the *_after_root / notify_now_or_after_root helper: {found:?}"
    );
}

/// Hardening pin: a `}` inside a literal or a comment used to end the
/// brace-matched body early, which hid every later push from the scan —
/// a reintroduced direct push would have slipped through silently.
#[test]
fn boot_scan_sees_past_braces_in_literals_and_comments() {
    let source = r##"
    fn create_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let quoted = "}";
        let ch = '}';
        let raw = r#"}}}"#;
        // } not a close
        /* } neither */
        window.push_notification(note, cx);
    }
"##;
    let found = direct_overlay_calls(source, &["fn create_tray(&mut self"]);
    assert_eq!(
        found.len(),
        1,
        "the push after a brace in a literal is still seen: {found:?}"
    );
    assert!(found[0].contains(".push_notification("), "{found:?}");
}

/// Hardening pin: matching by method name catches a push on a receiver the
/// old `window.`-only spelling would have missed.
#[test]
fn boot_scan_matches_any_receiver() {
    let source = r#"
    fn handle_launch_intake(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = boot_window(window);
        handle.push_notification(note, cx);
        handle.open_dialog(cx, build);
        handle.open_alert_dialog(cx, build);
    }
"#;
    let found = direct_overlay_calls(source, &["fn handle_launch_intake("]);
    assert_eq!(
        found.len(),
        OVERLAY_CALLS.len(),
        "every overlay spelling is caught: {found:?}"
    );
}

/// Hardening pin, the other direction: helper calls, comments about the
/// defect, and quoted call text must not read as violations.
#[test]
fn boot_scan_ignores_helper_calls_and_comments() {
    let source = r#"
    fn restore_on_launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Never window.push_notification(here): it panics before the Root.
        self.notify_after_root(Notification::error(db_list_error_message(&error)), window, cx);
        self.notify_now_or_after_root(Notification::error(text), window, cx);
        let quoted = "window.open_dialog(";
    }
"#;
    let found = direct_overlay_calls(source, &["fn restore_on_launch(&mut self"]);
    assert!(found.is_empty(), "no false positives: {found:?}");
}

/// Defect: `reload` boots through `restore_on_launch` ->
/// `commit_restore_subset`, so a failing `store.list()` reached a window
/// with no Root yet and panicked in `Root::update`. No Root here either: the
/// call must survive and leave the toast for the first frame.
#[gpui_kit::test]
fn reload_before_root_defers_the_store_error(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(restore_seed("Loop", "", "10878"))
        .expect("seed works");
    store
        .set_last_clean_exit(true)
        .expect("clean marker writes");

    let (view, cx) = cx.add_window_view(move |window, cx| AppView::with_store(window, cx, store));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    assert!(
        cx.update(|window, _| !AppView::root_installed(window)),
        "the boot window really has no Root layer"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.store.break_schema_for_test().expect("schema breaks");
            view.reload(window, cx);
        });
    });

    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "the failed reload left the library untouched instead of panicking"
    );

    // Production installs the Root right after the boot closure returns:
    // do the same, then run one frame. The queued toast must arrive exactly
    // once — proving the error was deferred, not dropped.
    cx.update(|window, cx| {
        let view = view.clone();
        window.replace_root(cx, move |window, cx| Root::new(view, window, cx));
    });
    assert_eq!(
        notification_count(cx),
        0,
        "nothing was pushed into the Root-less window"
    );
    cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(
        notification_count(cx),
        1,
        "the boot-time store error arrives once the Root exists"
    );
}

/// The same path after Root: `reload` must report the failure in the same
/// turn, exactly once — ordinary use keeps its timing, no extra frames.
#[gpui_kit::test]
fn reload_after_root_delivers_the_store_error_in_the_same_turn(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    view.read_with(cx, |view, _| {
        view.store.break_schema_for_test().expect("schema breaks");
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reload(window, cx));
    });
    assert_eq!(
        notification_count(cx),
        1,
        "a post-Root reload reports the failure in the same turn"
    );

    cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(
        notification_count(cx),
        1,
        "and exactly once: no duplicate on the following frame"
    );
}

#[gpui_kit::test]
fn start_all_snapshots_running_set_and_stop_clears(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(restore_seed("Alpha", "", "10874"))
        .expect("seed works");
    let beta = store
        .add(restore_seed("Beta", "", "10875"))
        .expect("seed works");

    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    let mut snapshot = view.read_with(cx, |view, _| view.store.load_running_snapshot());
    snapshot.sort();
    let mut expected = vec![alpha.id.clone(), beta.id.clone()];
    expected.sort();
    assert_eq!(snapshot, expected, "running set snapshots on start");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_stop_all(window, cx));
    });
    assert!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot())
            .is_empty(),
        "stop-all clears the snapshot"
    );
}

#[gpui_kit::test]
fn settings_restore_commit_round_trips(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_restore_opt_in(true, window, cx));
    });
    assert!(view.read_with(cx, |view, _| view.store.restore_running()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_restore_opt_in(false, window, cx));
    });
    assert!(!view.read_with(cx, |view, _| view.store.restore_running()));
    assert_eq!(notification_count(cx), 2, "each commit toasts once");
}

#[gpui_kit::test]
fn settings_auto_restart_commit_round_trips(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    assert!(
        view.read_with(cx, |view, _| view.auto_restart_enabled()),
        "missing meta reads as enabled"
    );
    assert!(
        view.read_with(cx, |view, _| view.engine.auto_restart().enabled),
        "engine defaults enabled"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_auto_restart(false, window, cx));
    });
    assert!(!view.read_with(cx, |view, _| view.auto_restart_enabled()));
    assert!(
        !view.read_with(cx, |view, _| view.engine.auto_restart().enabled),
        "commit pushes the choice into the engine"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_auto_restart(true, window, cx));
    });
    assert!(view.read_with(cx, |view, _| view.auto_restart_enabled()));
    assert!(view.read_with(cx, |view, _| view.engine.auto_restart().enabled));
    assert_eq!(notification_count(cx), 2, "each commit toasts once");
}

#[gpui_kit::test]
fn settings_notify_from_tray_commit_round_trips(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    assert!(
        view.read_with(cx, |view, _| view.notify_from_tray_enabled()),
        "missing meta reads as on"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_notify_from_tray(false, window, cx)
        });
    });
    assert!(!view.read_with(cx, |view, _| view.notify_from_tray_enabled()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_notify_from_tray(true, window, cx)
        });
    });
    assert!(view.read_with(cx, |view, _| view.notify_from_tray_enabled()));
    assert_eq!(notification_count(cx), 2, "each commit toasts once");
}

#[gpui_kit::test]
fn startup_applies_persisted_auto_restart(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_meta(manyproxy_ui::META_AUTO_RESTART, "0")
        .expect("meta writes");
    let (view, cx) = mount_view(store, cx);
    assert!(
        !view.read_with(cx, |view, _| view.engine.auto_restart().enabled),
        "a persisted opt-out is pushed into the engine at startup"
    );
    assert!(
        !view.read_with(cx, |view, _| view.auto_restart_enabled()),
        "the persisted value is read back"
    );
}

#[gpui_kit::test]
fn settings_dialog_opens_with_switches(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    // The nav's Settings opens the modal directly — no one-item menu.
    cx.update(|window, cx| {
        window.click("nav-settings", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    // T-04: General carries appearance, startup and the close behaviour; the
    // engine rows live in the Engine section.
    for id in [
        "settings-row-autostart",
        "settings-autostart-switch",
        "settings-row-autostart-help",
        "settings-row-accent",
        "settings-accent-blue",
        "settings-accent-violet",
        "settings-row-link-handler",
        "settings-link-handler-switch",
        "settings-row-link-handler-help",
        "settings-row-notify-from-tray",
        "settings-notify-from-tray-switch",
        "settings-row-notify-from-tray-help",
        "settings-row-restore",
        "settings-restore-switch",
        "settings-row-restore-help",
        "settings-row-window-behaviour",
        "settings-row-show-tray",
        "settings-show-tray-switch",
        "settings-row-show-tray-help",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "settings dialog renders {id}"
        );
    }
    // B17: one column — the Engine rows are in the same tree as General's.
    for id in [
        "settings-row-auto-restart",
        "settings-auto-restart-switch",
        "settings-row-auto-restart-help",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "the Engine section renders {id}"
        );
    }
}

#[gpui_kit::test]
fn quit_from_tray_stops_engine_and_clears_snapshot(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(restore_seed("Solo", "", "10876"))
        .expect("seed works");

    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 1);
    assert_eq!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot()),
        vec![row.id.clone()]
    );

    cx.simulate_resize(size(px(1120.), px(760.)));
    let live_bounds = cx.update(|window, _| window.bounds());

    cx.update(|window, cx| {
        view.update(cx, |view, _| view.quit_from_tray(window));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 0);
    assert_eq!(
        view.read_with(cx, |view, _| view.store.window_geometry()),
        Some(
            WindowGeometry {
                x: f32::from(live_bounds.origin.x),
                y: f32::from(live_bounds.origin.y),
                width: f32::from(live_bounds.size.width),
                height: f32::from(live_bounds.size.height),
            }
            .clamped()
            .expect("the harness window has finite bounds")
        ),
        "tray quit saves the live rectangle (T-32)"
    );
    assert_eq!(
        f32::from(live_bounds.size.width),
        1120.,
        "the simulated resize is what gets saved"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot())
            .is_empty(),
        "quit clears the snapshot"
    );
    assert!(view.read_with(cx, |view, _| view.tray.is_none()));

    let quit = cx.update(|window, cx| view.update(cx, |view, cx| view.poll_tray_once(window, cx)));
    assert!(!quit, "no tray, no quit, no event drain");
}

/// T-34: every orderly exit path writes `last_clean_exit = "1"`; the boot
/// write (`"0"`) is what a kill/crash leaves behind.
#[gpui_kit::test]
fn t84_clean_quit_flags_last_clean_exit(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(restore_seed("Clean quit", "", "10881"))
        .expect("seed works");
    assert!(!store.last_clean_exit(), "absent meta reads unclean");

    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot()),
        vec![row.id.clone()],
        "a running set is snapshotted before the exit"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, _| view.quit_from_tray(window));
    });
    assert!(
        view.read_with(cx, |view, _| view.store.last_clean_exit()),
        "tray Quit writes the clean marker"
    );
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.store
                .get_meta(Store::META_LAST_CLEAN_EXIT)
                .expect("meta reads")
        }),
        Some("1".to_string()),
        "the marker's on-disk shape is \"1\""
    );
    assert!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot())
            .is_empty(),
        "tray Quit still clears the snapshot"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.set_last_clean_exit(false).expect("reset writes")
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, _| view.confirm_close_stop(window));
    });
    assert!(
        view.read_with(cx, |view, _| view.store.last_clean_exit()),
        "confirm-close-stop writes the clean marker on its own"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.set_last_clean_exit(false).expect("reset writes")
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.mark_clean_exit());
    });
    assert!(
        view.read_with(cx, |view, _| view.store.last_clean_exit()),
        "the zero-rows plain close flips through the same helper its branch calls"
    );
}

/// T-34: boot reads the previous verdict, then always writes the dirty
/// assumption for the session it is starting.
#[gpui_kit::test]
fn t84_boot_assumes_dirty_until_quits_cleanly(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_last_clean_exit(true)
        .expect("clean marker writes");

    let (view, cx) = mount_view(store, cx);
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.store
                .get_meta(Store::META_LAST_CLEAN_EXIT)
                .expect("meta reads")
        }),
        Some("0".to_string()),
        "boot overwrites a clean marker with the dirty assumption"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, _| view.quit_from_tray(window));
    });
    assert!(
        view.read_with(cx, |view, _| view.store.last_clean_exit()),
        "only the clean quit turns the session marker back to clean"
    );
}

/// T-34: an unclean boot that left a running snapshot behind toasts once;
/// the toast text is the ticket's exact string.
#[gpui_kit::test]
fn t84_unclean_boot_with_running_snapshot_toasts_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(restore_seed("Crashed run", "", "10882"))
        .expect("seed works");
    store
        .save_running_snapshot(std::slice::from_ref(&row.id))
        .expect("snapshot writes");
    assert!(
        !store.last_clean_exit(),
        "a snapshot without a clean marker is the crash shape"
    );

    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert_eq!(
        notification_count(cx),
        1,
        "the unclean boot toasts exactly once"
    );
    assert_eq!(
        manyproxy_ui::unclean_exit_notice_text(),
        "Previous run carried running endpoints and did not exit cleanly.",
        "the toast carries the ticket's exact string"
    );

    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert_eq!(
        notification_count(cx),
        1,
        "the toast is one-shot, not one per frame"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.restore_on_launch(window, cx));
    });
    assert_eq!(
        notification_count(cx),
        1,
        "restore-on-launch never repeats the toast"
    );
}

/// T-34: the clean marker suppresses the toast whatever the snapshot
/// holds — and that suppression proves the read precedes the boot write.
#[gpui_kit::test]
fn t84_clean_boot_never_toasts_even_with_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(restore_seed("Clean run", "", "10883"))
        .expect("seed works");
    store
        .save_running_snapshot(std::slice::from_ref(&row.id))
        .expect("snapshot writes");
    store
        .set_last_clean_exit(true)
        .expect("clean marker writes");

    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert_eq!(
        notification_count(cx),
        0,
        "a clean previous exit never toasts, even with a snapshot"
    );
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.store
                .get_meta(Store::META_LAST_CLEAN_EXIT)
                .expect("meta reads")
        }),
        Some("0".to_string()),
        "the boot read the clean value before writing the dirty one"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.last_snapshot.clone()),
        vec![row.id.clone()],
        "snapshot hydration is unchanged by the marker"
    );

    let mut empty = Store::open_in_memory().expect("in-memory store opens");
    empty
        .set_last_clean_exit(true)
        .expect("clean marker writes");
    let (_empty_view, cx2) = mount_view(empty, cx);
    cx2.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert_eq!(
        notification_count(cx2),
        0,
        "clean boot with an empty snapshot toasts nothing either"
    );
}

/// T-34: the marker is a meta value; orderly exits create no files.
#[gpui_kit::test]
fn t84_no_marker_file_ever_written(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    // Fixed name (no PID suffix): the cleanup below runs before any
    // connection exists, so it self-heals the previous run's leftovers.
    // A PID suffix made each suite pass leave a fresh directory behind.
    let dir = std::env::temp_dir().join("manyproxy-t84-marker");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir creates");
    let names = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("dir reads")
            .map(|entry| {
                entry
                    .expect("entry reads")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    };

    let store = Store::open_file(&dir.join("library.db")).expect("file store opens");
    let before = names(&dir);
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    assert_eq!(notification_count(cx), 0, "nothing to report on this boot");

    cx.update(|window, cx| {
        view.update(cx, |view, _| view.quit_from_tray(window));
    });
    assert!(
        view.read_with(cx, |view, _| view.store.last_clean_exit()),
        "the clean quit markers the session"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "a clean quit pushes no notification"
    );

    let after = names(&dir);
    assert_eq!(
        after, before,
        "the clean quit writes no files beyond the database family"
    );
    assert!(
        after.iter().all(|name| name.starts_with("library.db")),
        "only the database family exists: {after:?}"
    );
    assert!(
        !after
            .iter()
            .any(|name| name.to_ascii_lowercase().contains("marker")),
        "the rejected running.marker sibling never appears: {after:?}"
    );

    drop(view);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn t48_tray_dispatch_every_menu_id_runs_its_handler(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::tray::{action_for_menu_id, menu_id, TrayAction};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10871"))
        .expect("seed works");
    let beta = store.add(tick12_seed("Beta", "10872")).expect("seed works");

    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });

    for (id, action) in [
        (menu_id::SHOW, TrayAction::Show),
        (menu_id::START_ALL, TrayAction::StartAll),
        (menu_id::STOP_ALL, TrayAction::StopAll),
        (menu_id::QUIT, TrayAction::Quit),
    ] {
        assert_eq!(action_for_menu_id(id), Some(action), "id {id} dispatches");
        assert_eq!(
            manyproxy_ui::tray::menu_id_for_action(action),
            id,
            "id {id} round-trips"
        );
    }
    assert_eq!(action_for_menu_id("foreign-id"), None);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::StartAll, window, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "StartAll serves the library"
    );
    let mut snapshot = view.read_with(cx, |view, _| view.store.load_running_snapshot());
    snapshot.sort();
    let mut expected = vec![alpha.id.clone(), beta.id.clone()];
    expected.sort();
    assert_eq!(snapshot, expected, "snapshot names the serving set");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::Show, window, cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "Show never touches the engine"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::StopAll, window, cx)
        });
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "confirm gates the kill"
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "StopAll dispatch opens the confirm"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_stop_all(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "confirmed StopAll stops every endpoint"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot())
            .is_empty(),
        "stop clears the snapshot"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::StartAll, window, cx)
        });
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 2);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.handle_tray_action(TrayAction::Quit, window, cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "Quit stops the engine"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot())
            .is_empty(),
        "Quit clears the snapshot"
    );
    assert!(
        view.read_with(cx, |view, _| view.tray.is_none()),
        "Quit drops the icon"
    );
}

#[test]
fn t48_resolve_fetch_redirect_table() {
    use gpui_kit::http_client::http::StatusCode;
    use manyproxy_ui::{is_fetch_redirect, resolve_fetch_redirect};
    for code in [301u16, 302, 303, 307, 308] {
        assert!(
            is_fetch_redirect(StatusCode::from_u16(code).expect("status builds")),
            "{code} follows by hand"
        );
    }
    for code in [200u16, 204, 404, 500] {
        assert!(
            !is_fetch_redirect(StatusCode::from_u16(code).expect("status builds")),
            "{code} is not a redirect"
        );
    }
    assert_eq!(
        resolve_fetch_redirect(
            "https://example.com/sub.txt",
            "https://cdn.example.com/sub.txt"
        ),
        Some("https://cdn.example.com/sub.txt".to_string())
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com/a/sub.txt", "/b/sub.txt"),
        Some("https://example.com/b/sub.txt".to_string())
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com:8443/a", "/b"),
        Some("https://example.com:8443/b".to_string())
    );

    for (current, location) in [
        ("https://example.com/a", "b/sub.txt"),
        ("https://example.com/a", "../sub.txt"),
        ("https://example.com/a", "ftp://example.com/sub.txt"),
        ("https://example.com/a", "file:///etc/passwd"),
        ("https://example.com/a", ""),
        ("https://example.com/a", "   "),
        ("not-a-url", "/b"),
        ("https://", "/b"),
    ] {
        assert_eq!(
            resolve_fetch_redirect(current, location),
            None,
            "refused: {current:?} + {location:?}"
        );
    }
}

#[test]
fn t41_save_listen_gate_passthrough_and_refusal() {
    assert_eq!(
        manyproxy_ui::save_listen_gate("Row", "127.0.0.1", ""),
        Ok(())
    );
    assert_eq!(
        manyproxy_ui::save_listen_gate("Row", "127.0.0.1", "   "),
        Ok(())
    );
    assert_eq!(
        manyproxy_ui::save_listen_gate("Row", "127.0.0.1", "not-a-port"),
        Ok(())
    );
    assert_eq!(
        manyproxy_ui::save_listen_gate("Row", "127.0.0.1", "0"),
        Ok(())
    );
    assert_eq!(
        manyproxy_ui::save_listen_gate("Row", "127.0.0.1", "10801"),
        Ok(())
    );
    assert_eq!(manyproxy_ui::save_listen_gate("Row", "", "10801"), Ok(()));
    let err = manyproxy_ui::save_listen_gate("Row", "127.0.0.1", "18501")
        .expect_err("clash range refused");
    assert!(
        err.contains("clash-controller range 18501-18999"),
        "names range, got {err}"
    );
    let err = manyproxy_ui::save_listen_gate("Row", "127.0.0.1", "11801")
        .expect_err("bridge range refused");
    assert!(
        err.contains("xray-bridge range 11801-11999"),
        "names range, got {err}"
    );
    let err = manyproxy_ui::save_listen_gate("Row", "999.999.0.1", "10801")
        .expect_err("garbage bind refused");
    assert!(err.contains("bad bind address"), "names bind, got {err}");
}

#[test]
fn t48_resolve_fetch_redirect_edges() {
    use gpui_kit::http_client::http::StatusCode;
    use manyproxy_ui::{is_fetch_redirect, resolve_fetch_redirect};
    assert_eq!(
        resolve_fetch_redirect("https://example.com/a/sub.txt", "  /b/sub.txt  "),
        Some("https://example.com/b/sub.txt".to_string())
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com/a", "HTTPS://cdn.example.com/sub.txt"),
        Some("HTTPS://cdn.example.com/sub.txt".to_string())
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com:8443/a/b?x=1", "/c?y=2"),
        Some("https://example.com:8443/c?y=2".to_string())
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com/a", "//evil.com/x"),
        Some("https://example.com//evil.com/x".to_string())
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com/a", "https:/cdn.example.com/x"),
        None
    );
    assert_eq!(
        resolve_fetch_redirect("https://example.com/a", "javascript:alert(1)"),
        None
    );
    assert!(
        !is_fetch_redirect(StatusCode::from_u16(304).expect("status builds")),
        "304 is not followed"
    );
    assert!(
        !is_fetch_redirect(StatusCode::from_u16(300).expect("status builds")),
        "300 is not followed"
    );
}

#[test]
fn t48_fetch_redirect_to_internal_blocked_without_fetch() {
    let (client, fetched) = t48_recording_client(|uri| {
        if uri == "https://example.com/sub.txt" {
            t48_response(302, Some("http://127.0.0.1/evil"), "")
        } else {
            t48_response(200, None, "must never fetch")
        }
    });
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect_err("internal redirect blocked");
    assert!(error.contains("internal"), "SSRF cause, got {error}");
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec!["https://example.com/sub.txt".to_string()],
        "the evil hop is never fetched"
    );
}

#[test]
fn t48_fetch_short_form_literal_redirect_blocked_without_fetch() {
    let (client, fetched) = t48_recording_client(|uri| {
        if uri == "https://example.com/sub.txt" {
            t48_response(302, Some("http://2130706433/evil"), "")
        } else {
            t48_response(200, None, "must never fetch")
        }
    });
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect_err("short-form internal redirect blocked");
    assert!(error.contains("internal"), "SSRF cause, got {error}");
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec!["https://example.com/sub.txt".to_string()],
        "the evil hop is never fetched"
    );
}

#[test]
fn t48_fetch_resolved_internal_and_dead_hosts_block_without_fetch() {
    let (client, fetched) = t48_recording_client(|_| t48_response(200, None, "unreachable"));
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://rebind.example.com/sub.txt",
        t48_internal_resolver,
    ))
    .expect_err("rebind to loopback blocked");
    assert!(error.contains("internal"), "rebind cause, got {error}");
    assert!(
        fetched.lock().expect("read works").is_empty(),
        "blocked before any fetch"
    );

    let (client, fetched) = t48_recording_client(|_| t48_response(200, None, "unreachable"));
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://dead.example.com/sub.txt",
        t48_dead_resolver,
    ))
    .expect_err("unresolvable host fails closed");
    assert!(error.contains("resolve"), "loud cause, got {error}");
    assert!(
        fetched.lock().expect("read works").is_empty(),
        "blocked before any fetch"
    );
}

#[test]
fn t48_fetch_redirect_to_public_followed_once() {
    let (client, fetched) = t48_recording_client(|uri| {
        if uri == "https://example.com/sub.txt" {
            t48_response(302, Some("https://cdn.example.com/sub.txt"), "")
        } else {
            t48_response(200, None, "cdn-body")
        }
    });
    let downloaded = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect("public redirect followed");
    assert_eq!(downloaded.body, "cdn-body");
    assert_eq!(downloaded.userinfo, None, "no header means no quota");
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec![
            "https://example.com/sub.txt".to_string(),
            "https://cdn.example.com/sub.txt".to_string(),
        ]
    );

    let (client, fetched) = t48_recording_client(|_| t48_response(200, None, "unreachable"));
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "http://127.0.0.1/direct",
        t48_public_resolver,
    ))
    .expect_err("direct internal blocked");
    assert!(error.contains("internal"), "SSRF cause, got {error}");
    assert!(
        fetched.lock().expect("read works").is_empty(),
        "blocked before any fetch"
    );
}

#[test]
fn t48_fetch_redirect_loop_bounded_and_locationless_loud() {
    let (client, fetched) =
        t48_recording_client(|_| t48_response(302, Some("https://example.com/elsewhere"), ""));
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect_err("loop bounded");
    assert!(
        error.contains("too many redirects"),
        "bounded cause, got {error}"
    );
    assert_eq!(
        fetched.lock().expect("read works").len(),
        6,
        "initial plus five hops, then give up"
    );

    let (client, _) = t48_recording_client(|_| t48_response(302, None, ""));
    let error = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect_err("location-less redirect fails");
    assert!(error.contains("bad redirect"), "loud cause, got {error}");
}

#[test]
fn t56_feed_fetch_policy_blocks_internal_hosts() {
    // a literal loopback feed is refused before any byte moves
    let (client, fetched, header_log) =
        t56_recording_feed_client(|_| t48_response(200, None, "must never fetch"));
    let error = futures::executor::block_on(manyproxy_ui::fetch_release_feed(
        client,
        "https://127.0.0.1/updates/releases/latest",
        t56_public_resolver,
    ))
    .expect_err("loopback feed blocked");
    assert!(error.contains("internal"), "SSRF cause, got {error}");
    assert!(
        fetched.lock().expect("read works").is_empty(),
        "blocked before any network"
    );
    assert!(t56_no_hwid_header(&header_log.lock().expect("read works")));

    // a redirect back into a private address dies without fetching the hop
    let (client, fetched, _) = t56_recording_feed_client(|uri| {
        if uri.ends_with("/releases/latest") {
            t48_response(302, Some("http://192.168.0.5/evil"), "")
        } else {
            t48_response(200, None, "must never fetch")
        }
    });
    let error = futures::executor::block_on(manyproxy_ui::fetch_release_feed(
        client,
        "https://example.com/updates/releases/latest",
        t56_public_resolver,
    ))
    .expect_err("private redirect blocked");
    assert!(error.contains("internal"), "rebind cause, got {error}");
    assert_eq!(
        fetched.lock().expect("read works").len(),
        1,
        "only the announced feed hop ever happened"
    );

    // the same policy refuses non-http(s) feeds outright
    let (client, fetched, _) =
        t56_recording_feed_client(|_| t48_response(200, None, "must never fetch"));
    let error = futures::executor::block_on(manyproxy_ui::fetch_release_feed(
        client,
        "ftp://example.com/feed.json",
        t56_public_resolver,
    ))
    .expect_err("non-http(s) feed blocked");
    assert!(error.contains("http(s)"), "scheme cause, got {error}");
    assert!(
        fetched.lock().expect("read works").is_empty(),
        "blocked before any network"
    );
}

#[test]
fn t56_semver_compare_matrix() {
    use manyproxy_ui::{compare_release_to_running, ReleaseCompare};
    // equal tags and their v-aliases stay put
    assert_eq!(
        compare_release_to_running("0.0.1", "0.0.1"),
        ReleaseCompare::UpToDate
    );
    assert_eq!(
        compare_release_to_running("v0.0.1", "0.0.1"),
        ReleaseCompare::UpToDate
    );
    // newer stable, with or without the v prefix
    assert_eq!(
        compare_release_to_running("0.0.2", "0.0.1"),
        ReleaseCompare::Newer
    );
    assert_eq!(
        compare_release_to_running("v0.0.2", "0.0.1"),
        ReleaseCompare::Newer
    );
    assert_eq!(
        compare_release_to_running("1.0.0", "0.9.9"),
        ReleaseCompare::Newer
    );
    // older releases are ignored
    assert_eq!(
        compare_release_to_running("0.0.0", "0.0.1"),
        ReleaseCompare::Older
    );
    assert_eq!(
        compare_release_to_running("0.0.1", "0.0.2"),
        ReleaseCompare::Older
    );
    // rule 10: a plain release outranks its own prerelease
    assert_eq!(
        compare_release_to_running("0.0.2-beta.1", "0.0.2"),
        ReleaseCompare::Older
    );
    assert_eq!(
        compare_release_to_running("0.0.2", "0.0.2-beta.1"),
        ReleaseCompare::Newer
    );
    // rule 11: prerelease identifiers order numerically and lexically
    assert_eq!(
        compare_release_to_running("0.0.2-beta.2", "0.0.2-beta.1"),
        ReleaseCompare::Newer
    );
    assert_eq!(
        compare_release_to_running("0.0.2-alpha", "0.0.2-beta"),
        ReleaseCompare::Older
    );
    assert_eq!(
        compare_release_to_running("0.0.2-beta", "0.0.2-beta.1"),
        ReleaseCompare::Older
    );
    // unparseable tags never alarm
    assert_eq!(
        compare_release_to_running("nickname-of-my-cat", "0.0.1"),
        ReleaseCompare::UpToDate
    );
}

#[test]
fn t56_404_renders_no_releases_message() {
    // fetch half: 404 maps to the exact no-releases text and nothing else
    let (client, fetched, header_log) =
        t56_recording_feed_client(|_| t48_response(404, None, "{\"message\":\"Not Found\"}"));
    let feed_url = manyproxy_ui::effective_update_feed_url(None, manyproxy_ui::APP_VERSION);
    assert_eq!(feed_url, manyproxy_ui::UPDATE_FEED_URL_DEFAULT);
    let why = futures::executor::block_on(manyproxy_ui::fetch_release_feed(
        client,
        &feed_url,
        t56_public_resolver,
    ))
    .expect_err("404 maps to the no-releases text");
    assert_eq!(why, manyproxy_ui::UPDATE_FEED_EMPTY_MESSAGE);
    assert_eq!(why, "No releases published yet.");
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec![feed_url.clone()],
        "the request targets the feed URL only"
    );
    assert!(
        t56_no_hwid_header(&header_log.lock().expect("read works")),
        "no hardware id headers ride the update check"
    );

    // outcome half: the empty feed is a dialog state, not an error class
    let outcome = manyproxy_ui::update_check_outcome(Err(why.clone()))
        .expect("empty feed never surfaces as an error");
    assert_eq!(
        outcome,
        manyproxy_ui::UpdateCheckOutcome::NoReleases,
        "404 renders exactly the no-releases state"
    );

    // malformed body: loud plain-error detail, never a body or URL echo
    let (client, fetched, _) =
        t56_recording_feed_client(|_| t48_response(200, None, "<html>nope</html>"));
    let malformed = futures::executor::block_on(manyproxy_ui::fetch_release_feed(
        client,
        &feed_url,
        t56_public_resolver,
    ))
    .expect_err("malformed feed rejected");
    assert_eq!(malformed, "update feed is not valid JSON");
    assert_eq!(fetched.lock().expect("read works").len(), 1);
    let failed = manyproxy_ui::update_check_outcome(Err(malformed))
        .expect_err("malformed feeds keep failing loudly");
    assert_eq!(failed, "update feed is not valid JSON");
}

#[gpui_kit::test]
fn t56_no_releases_dialog_and_malformed_error_toast(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_update_check(
                Err(manyproxy_ui::UPDATE_FEED_EMPTY_MESSAGE.to_string()),
                window,
                cx,
            );
        });
    });
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "an empty feed renders the dialog"
    );
    assert_painted(cx, "update-no-releases");
    assert_absent(cx, "update-open-releases".to_string());
    assert_eq!(notification_count(cx), 0, "404 is not an error toast class");

    // a malformed feed is the sibling edge: plain error toast, no dialog
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_update_check(Err("update feed is not valid JSON".to_string()), window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "malformed feed never opens the dialog"
    );
    assert_eq!(
        notification_count(cx),
        1,
        "malformed feed is a plain error toast"
    );
}

#[test]
fn t56_passive_update_fetches_only_after_a_week() {
    use manyproxy_ui::{passive_update_check_due, PASSIVE_UPDATE_CHECK_INTERVAL_SECS};

    let now = 1_800_000_000;
    let week = PASSIVE_UPDATE_CHECK_INTERVAL_SECS;
    assert!(
        passive_update_check_due(None, now, true),
        "the first launch has no stamp to wait on"
    );
    assert!(
        !passive_update_check_due(None, now, false),
        "the opt-out wins over a missing stamp"
    );
    assert!(
        !passive_update_check_due(Some(now - week + 1), now, true),
        "one second early is early"
    );
    assert!(
        passive_update_check_due(Some(now - week), now, true),
        "a full week opens the gate"
    );
    assert!(
        !passive_update_check_due(Some(now + 3600), now, true),
        "a clock that moved backwards does not fetch early"
    );

    let body = t56_release_json(
        "v999.0.1",
        false,
        "https://github.com/mahyartdb/ManyProxy/releases/tag/v999.0.1",
        "ManyProxy v999.0.1",
    );
    let (client, fetched, _) = t56_recording_feed_client(move |_| t48_response(200, None, &body));
    let url = manyproxy_ui::UPDATE_FEED_URL_DEFAULT;
    let closed = futures::executor::block_on(manyproxy_ui::fetch_passive_update(
        false,
        client.clone(),
        url,
        t56_public_resolver,
    ));
    assert!(closed.is_none(), "a closed gate returns no result");
    assert!(
        fetched.lock().expect("read works").is_empty(),
        "a closed gate never reaches the client"
    );
    let open = futures::executor::block_on(manyproxy_ui::fetch_passive_update(
        true,
        client,
        url,
        t56_public_resolver,
    ))
    .expect("an open gate returns the fetch result");
    assert!(
        open.expect("the feed parses").is_some(),
        "the configured feed is read"
    );
    assert_eq!(
        fetched.lock().expect("read works").len(),
        1,
        "an open gate reads the feed exactly once"
    );
}

#[gpui_kit::test]
fn t56_passive_failure_is_silent(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let epoch = view.read_with(cx, |view, _| view.passive_update_epoch);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(
                Err("update feed fetch failed".to_string()),
                epoch,
                cx,
            );
        });
    });
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        0,
        "a background failure never raises a toast"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "a background failure never opens a dialog"
    );
    assert!(
        view.read_with(cx, |view, _| view.update_available.is_none()),
        "a failure does not invent a hint"
    );
}

#[gpui_kit::test]
fn t56_passive_newer_hint_opens_the_dialog_and_clears(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let epoch = view.read_with(cx, |view, _| view.passive_update_epoch);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(Ok(Some(t56_newer_release())), epoch, cx);
        });
    });
    paint_frame(cx);
    assert_painted(cx, "action-bar-update-dot");
    assert_eq!(
        label_of("action-bar-update-dot", cx),
        "Update available — choose Check for updates… from the action bar's overflow menu",
        "the dot carries the pinned copy"
    );

    cx.update(|window, cx| window.click("action-bar-overflow", cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("action-overflow-updates", cx));
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the overflow's update item opens the update dialog"
    );
    assert_painted(cx, "update-new-version");
    assert_absent(cx, "action-bar-update-dot".to_string());
    assert!(
        view.read_with(cx, |view, _| view.update_available.is_none()),
        "the hint is consumed by the dialog"
    );
}

#[gpui_kit::test]
fn t56_passive_result_clears_when_no_longer_newer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let epoch = view.read_with(cx, |view, _| view.passive_update_epoch);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(Ok(Some(t56_newer_release())), epoch, cx);
        });
    });
    paint_frame(cx);
    assert_painted(cx, "action-bar-update-dot");

    let older = manyproxy_ui::FetchedRelease {
        tag_name: "v0.0.0".to_string(),
        ..t56_newer_release()
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(Ok(Some(older)), epoch, cx);
        });
    });
    paint_frame(cx);
    assert_absent(cx, "action-bar-update-dot".to_string());
    assert!(
        view.read_with(cx, |view, _| view.update_available.is_none()),
        "an older release takes the hint away"
    );
}

#[gpui_kit::test]
fn t56_manual_check_clears_the_passive_hint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    // A loopback feed is refused before any DNS thread or socket, so the
    // manual check finishes inside the test executor.
    store
        .set_update_feed_url("http://127.0.0.1/feed.json")
        .expect("feed override writes");
    let (view, cx) = mount_view(store, cx);
    let epoch = view.read_with(cx, |view, _| view.passive_update_epoch);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(Ok(Some(t56_newer_release())), epoch, cx);
        });
    });
    assert!(view.read_with(cx, |view, _| view.update_available.is_some()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.check_for_updates(window, cx));
    });
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.update_available.is_none()),
        "a manual check supersedes the passive hint"
    );
    assert!(
        view.read_with(cx, |view, _| view.passive_update_epoch) > epoch,
        "a manual check discards any passive fetch in flight"
    );
}

#[gpui_kit::test]
fn t56_passive_toggle_defaults_on_persists_and_takes_effect_now(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    assert!(
        view.read_with(cx, |view, _| view.passive_update_check_enabled()),
        "a missing meta reads as on"
    );
    let epoch = view.read_with(cx, |view, _| view.passive_update_epoch);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(Ok(Some(t56_newer_release())), epoch, cx);
        });
    });
    assert!(view.read_with(cx, |view, _| view.update_available.is_some()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_passive_update_check(false, window, cx)
        });
    });
    assert!(
        !view.read_with(cx, |view, _| view.passive_update_check_enabled()),
        "the opt-out persists"
    );
    assert!(
        view.read_with(cx, |view, _| view.update_available.is_none()),
        "the opt-out clears the hint on the spot"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view
            .store
            .get_meta(manyproxy_ui::META_PASSIVE_UPDATE_CHECK)
            .expect("meta reads")),
        Some("0".to_string())
    );
    // A result already in flight belongs to the check the user just turned
    // off; it must not resurrect the hint.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.finish_passive_update_check(Ok(Some(t56_newer_release())), epoch, cx);
        });
    });
    assert!(view.read_with(cx, |view, _| view.update_available.is_none()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_passive_update_check(true, window, cx)
        });
    });
    assert!(view.read_with(cx, |view, _| view.passive_update_check_enabled()));
    assert_eq!(notification_count(cx), 2, "each commit toasts once");
}

#[gpui_kit::test]
fn t56_passive_opt_out_is_read_at_startup(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_meta(manyproxy_ui::META_PASSIVE_UPDATE_CHECK, "0")
        .expect("meta writes");
    let (view, cx) = mount_view(store, cx);
    assert!(
        !view.read_with(cx, |view, _| view.passive_update_check_enabled()),
        "a persisted opt-out survives the next launch"
    );
}

#[gpui_kit::test]
fn t56_settings_updates_switch_defaults_on(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-row-passive-update-check");
    assert_painted(cx, "settings-row-passive-update-check");
    assert_painted(cx, "settings-passive-update-check-switch");
    assert_painted(cx, "settings-row-passive-update-check-help");
    assert!(
        view.read_with(cx, |view, _| view.passive_update_check_enabled()),
        "the switch ships on"
    );

    cx.update(|window, cx| window.click("settings-passive-update-check-switch", cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        !view.read_with(cx, |view, _| view.passive_update_check_enabled()),
        "clicking the switch commits the opt-out"
    );
}

#[test]
fn t56_prerelease_visibility_follows_app_channel() {
    // stable builds stay on the single `releases/latest` endpoint
    assert_eq!(
        manyproxy_ui::effective_update_feed_url(None, "0.0.1"),
        manyproxy_ui::UPDATE_FEED_URL_DEFAULT
    );
    // a prerelease build switches to the releases list endpoint
    assert_eq!(
        manyproxy_ui::effective_update_feed_url(None, "0.1.0-beta.1"),
        "https://api.github.com/repos/mahyartdb/ManyProxy/releases"
    );
    // an explicit Settings override wins verbatim on either channel
    assert_eq!(
        manyproxy_ui::effective_update_feed_url(Some(" https://mirror.example/feed "), "0.0.1"),
        "https://mirror.example/feed"
    );

    let list = format!(
        "[{},{}]",
        t56_release_json(
            "v0.0.3-beta.1",
            true,
            "https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.3-beta.1",
            "Beta one",
        ),
        t56_release_json(
            "v0.0.2",
            false,
            "https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.2",
            "Stable build",
        ),
    );

    // a prerelease binary sees the newest prerelease tag
    let picked = manyproxy_ui::parse_release_feed(&list, true, "0.1.0-beta.1")
        .expect("list feed parses")
        .expect("prerelease channel surfaces prereleases");
    assert_eq!(picked.tag_name, "v0.0.3-beta.1");
    assert!(picked.prerelease);

    // a stable binary ignores prereleases and picks the newest stable
    let picked = manyproxy_ui::parse_release_feed(&list, true, "0.0.1")
        .expect("list feed parses")
        .expect("stable channel still finds the stable release");
    assert_eq!(picked.tag_name, "v0.0.2");

    // prerelease-only feed with a stable running version is up-to-date
    let only_prerelease = r#"[{"tag_name":"v0.0.3-beta.1","prerelease":true,"html_url":"https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.3-beta.1"}]"#;
    assert_eq!(
        manyproxy_ui::parse_release_feed(only_prerelease, true, manyproxy_ui::APP_VERSION)
            .expect("parses"),
        None,
        "nothing eligible means up-to-date, never an alarm"
    );
    assert_eq!(
        manyproxy_ui::parse_release_feed("[]", true, manyproxy_ui::APP_VERSION).expect("parses"),
        None
    );
}

#[gpui_kit::test]
fn t56_dialog_reports_new_version_with_browser_link(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let release = manyproxy_ui::FetchedRelease {
        tag_name: "v999.0.1".to_string(),
        prerelease: false,
        html_url: Some("https://github.com/mahyartdb/ManyProxy/releases/tag/v999.0.1".to_string()),
        release_name: Some("ManyProxy v999.0.1".to_string()),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_update_check_dialog(
                manyproxy_ui::UpdateCheckOutcome::Newer(release),
                window,
                cx,
            );
        });
    });
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "update dialog opens"
    );
    assert_painted(cx, "update-new-version");
    assert_painted(cx, "update-release-name");
    assert_painted(cx, "update-open-releases");

    // the same compare drives the exact message strings
    let outcome = manyproxy_ui::update_check_outcome(Ok(Some(manyproxy_ui::FetchedRelease {
        tag_name: "v999.0.1".to_string(),
        prerelease: false,
        html_url: None,
        release_name: None,
    })))
    .expect("newer tag compares");
    let (id, message, name, url) =
        manyproxy_ui::update_check_presentation(&outcome, manyproxy_ui::APP_VERSION);
    assert_eq!(id, "update-new-version");
    assert_eq!(
        message,
        format!(
            "New version available: v999.0.1 (running {})",
            manyproxy_ui::APP_VERSION
        )
    );
    assert_eq!(
        name, None,
        "a release title equal to the tag is not repeated"
    );
    assert_eq!(url, None, "a missing page never buys the browser button");

    // up-to-date renders its own fixed copy, without any link button
    let (id, message, name, url) = manyproxy_ui::update_check_presentation(
        &manyproxy_ui::UpdateCheckOutcome::UpToDate,
        manyproxy_ui::APP_VERSION,
    );
    assert_eq!(id, "update-up-to-date");
    assert_eq!(
        message,
        format!("ManyProxy {} is up to date.", manyproxy_ui::APP_VERSION)
    );
    assert_eq!(name, None);
    assert_eq!(url, None);
}

#[gpui_kit::test]
fn t56_settings_feed_url_round_trip_and_validation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    // store-level round trip: blank = default, override = verbatim trimmed
    assert_eq!(store.update_feed_url(), None, "no feed override by default");
    store
        .set_update_feed_url(" https://mirror.example/feed ")
        .expect("override saves");
    assert_eq!(
        store.update_feed_url(),
        Some("https://mirror.example/feed".to_string())
    );
    store
        .clear_update_feed_url()
        .expect("clear restores default");
    assert_eq!(store.update_feed_url(), None);

    // view-level commit path mirrors the test-URL field's save pattern
    let (view, cx) = mount_view(store, cx);
    let ok = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_update_feed_url("https://mirror.example/feed.json", window, cx)
        })
    });
    assert!(ok, "valid override saves");
    assert_eq!(
        view.read_with(cx, |view, _| view.store.update_feed_url()),
        Some("https://mirror.example/feed.json".to_string())
    );

    // an internal host is refused at save time, before any check runs
    let toasts = notification_count(cx);
    let blocked = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_update_feed_url("https://127.0.0.1/feed.json", window, cx)
        })
    });
    assert!(!blocked, "internal host refused");
    assert_eq!(
        notification_count(cx),
        toasts + 1,
        "the refusal surfaces the field error"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.update_feed_url()),
        Some("https://mirror.example/feed.json".to_string()),
        "a refused URL never overwrites the working override"
    );

    // a blank field clears the override back to the default feed
    let cleared = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_update_feed_url("   ", window, cx)
        })
    });
    assert!(cleared, "blank clears");
    assert_eq!(
        view.read_with(cx, |view, _| view.store.update_feed_url()),
        None
    );

    // plaintext http applies per the same fetch policy (warn, not refuse)
    let http_ok = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_update_feed_url("http://mirror.example/feed.json", window, cx)
        })
    });
    assert!(http_ok, "plaintext http only warns");
    assert_eq!(
        view.read_with(cx, |view, _| view.store.update_feed_url()),
        Some("http://mirror.example/feed.json".to_string())
    );
}

#[test]
fn t57_user_agent_header_on_subscription_fetch() {
    assert_eq!(
        manyproxy_ui::user_agent_header(),
        t57_expected_user_agent(),
        "the production helper builds the same static identity"
    );
    // subscription-style URL with a redirect: the header rides the
    // builder on every hop of the manual No-Follow loop
    let (client, fetched, header_log) = t56_recording_feed_client(|uri| {
        if uri == "https://example.com/sub.txt" {
            t48_response(302, Some("https://cdn.example.com/sub.txt"), "")
        } else {
            t48_response(200, None, "sub-body")
        }
    });
    let downloaded = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect("subscription body fetches");
    assert_eq!(downloaded.body, "sub-body");
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec![
            "https://example.com/sub.txt".to_string(),
            "https://cdn.example.com/sub.txt".to_string(),
        ],
        "both hops went out"
    );
    t57_assert_user_agent_on_every_capture(&header_log.lock().expect("read works"));
}

#[test]
fn t57_user_agent_header_on_remote_preset_fetch() {
    // remote presets refresh through the shared fetch_subscription_body
    // helper (start_preset_refresh), so this exercises the exact builder
    // that path owns — a preset-style URL, single non-redirect request
    let (client, fetched, header_log) =
        t56_recording_feed_client(|_| t48_response(200, None, "preset-body"));
    let downloaded = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://relay.example.com/presets/throne.json",
        t48_public_resolver,
    ))
    .expect("preset body fetches");
    assert_eq!(downloaded.body, "preset-body");
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec!["https://relay.example.com/presets/throne.json".to_string()],
        "the preset request went out once"
    );
    t57_assert_user_agent_on_every_capture(&header_log.lock().expect("read works"));
}

#[test]
fn t57_no_device_identifier_headers_anywhere() {
    // positive match on the UA and a negative match on the hwid/
    // x-device family across the entire fetch layer: subscription
    // bodies, remote presets (same helper), and the release feed
    let (client, fetched, header_log) =
        t56_recording_feed_client(|_| t48_response(200, None, "sub-body"));
    let downloaded = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client,
        "https://example.com/sub.txt",
        t48_public_resolver,
    ))
    .expect("subscription fetch works");
    assert_eq!(downloaded.body, "sub-body");
    assert_eq!(
        fetched.lock().expect("read works").len(),
        1,
        "the subscription entry point recorded one request"
    );
    t57_assert_user_agent_on_every_capture(&header_log.lock().expect("read works"));
    t57_assert_no_device_identifier_headers(&header_log.lock().expect("read works"));

    let (client, fetched, header_log) = t56_recording_feed_client(|_| {
        t48_response(
            200,
            None,
            r#"{"tag_name":"v0.0.1","prerelease":false,"html_url":"https://github.com/mahyartdb/ManyProxy/releases/tag/v0.0.1"}"#,
        )
    });
    let release = futures::executor::block_on(manyproxy_ui::fetch_release_feed(
        client,
        manyproxy_ui::UPDATE_FEED_URL_DEFAULT,
        t56_public_resolver,
    ))
    .expect("release feed fetches and parses")
    .expect("the latest endpoint holds one eligible release");
    assert_eq!(release.tag_name, "v0.0.1");
    assert_eq!(
        fetched.lock().expect("read works").len(),
        1,
        "the release-feed entry point recorded one request"
    );
    t57_assert_user_agent_on_every_capture(&header_log.lock().expect("read works"));
    t57_assert_no_device_identifier_headers(&header_log.lock().expect("read works"));
}

#[gpui_kit::test]
fn t08_density_lock_paints_without_clipping(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let third = store
        .add(row19_seed("Cabin", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    for id in [
        "nav",
        "nav-all-configs",
        "action-bar",
        "filter-slot",
        "engine-bar",
    ] {
        assert_painted(cx, id);
        assert_within_frame(cx, id);
    }
    assert_absent(cx, "facet-bar".to_string());
    assert_absent(cx, "facet-group".to_string());

    for id in [&first.id, &second.id, &third.id] {
        let row = format!("library-row-{id}");
        assert_painted(cx, &row);
        assert_within_frame(cx, &row);
    }
    let tops = cx.update(|window, _| {
        [&first.id, &second.id, &third.id]
            .iter()
            .map(|id| {
                window
                    .try_find(format!("library-row-{id}"))
                    .expect("dense row paints")
                    .bounds()
                    .origin
                    .y
            })
            .collect::<Vec<_>>()
    });
    assert!(
        tops[1] > tops[0] && tops[2] > tops[1],
        "dense rows stack without overlap: {tops:?}"
    );

    assert_painted(cx, "config-detail");
    assert_within_frame(cx, "config-detail");
    assert_painted(cx, "config-endpoints");
    assert_within_frame(cx, "config-endpoints");
    for id in [
        "inspector-identity",
        "config-endpoints",
        "config-health",
        "inspector-actions",
        "inspector-console-link",
        "engine-start",
        "engine-stop",
        "health-test",
        "row-menu",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} renders in the stacked inspector"
        );
    }

    assert_painted(cx, "action-bar-count");
    assert_within_frame(cx, "action-bar-count");
    for id in [
        "engine-bar",
        "engine-bar-status",
        "engine-bar-running",
        "engine-bar-traffic",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} renders in the engine bar"
        );
    }
}

#[gpui_kit::test]
fn t11_minimum_frame_paints_without_clipping(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);

    for id in [
        "nav",
        "nav-all-configs",
        "action-bar",
        "filter-slot",
        "engine-bar",
    ] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    assert_absent(cx, "facet-bar".to_string());
    assert_absent(cx, "facet-group".to_string());

    let first_row = format!("library-row-{}", first.id);
    let second_row = format!("library-row-{}", second.id);
    assert_painted(cx, &first_row);
    assert_within_frame_min(cx, &first_row);
    assert_painted(cx, &second_row);
    assert_within_frame_min(cx, &second_row);
    let (first_top, second_top) = cx.update(|window, _| {
        let first = window
            .try_find(first_row)
            .expect("first row paints")
            .bounds()
            .origin
            .y;
        let second = window
            .try_find(second_row)
            .expect("second row paints")
            .bounds()
            .origin
            .y;
        (first, second)
    });
    assert!(
        second_top > first_top,
        "dense rows stack without overlap at minimum size"
    );

    assert_painted(cx, "config-detail");
    assert_within_frame_min(cx, "config-detail");
    assert_painted(cx, "config-endpoints");
    assert_within_frame_min(cx, "config-endpoints");

    assert_painted(cx, "action-bar-count");
    assert_within_frame_min(cx, "action-bar-count");
    for id in [
        "engine-bar",
        "engine-bar-status",
        "engine-bar-running",
        "engine-bar-traffic",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} renders in the engine bar at minimum size"
        );
        assert_within_frame_min(cx, id);
    }
}

#[gpui_kit::test]
fn laneb_row_menu_names_its_actions(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    let label = cx.update(|window, _| {
        window
            .try_find("row-menu".to_string())
            .unwrap_or_else(|| panic!("row-menu renders"))
            .label()
            .map(str::to_string)
    });
    assert_eq!(
        label.as_deref(),
        Some("More actions"),
        "K1/A4-8: the kebab announces its accessible name (the glyph is `…`)"
    );
}

#[gpui_kit::test]
fn laneb_engine_bar_counts_the_running_rows(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(t26_engine_seed("Work", "10911"))
        .expect("seed works");
    store
        .add_traffic_batch(&[(seeded.id.clone(), 2_048, 1_024)])
        .expect("traffic accrues");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
        view.update(cx, |view, cx| {
            view.set_status_facet(manyproxy_ui::StatusFacet::Stopped, true, cx);
            view.select_visible_row(0, cx);
        });
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let keystroke = Keystroke::parse("f5").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    for id in ["engine-bar-running", "engine-bar-traffic"] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    assert_absent(cx, "status-ports".to_string());
    let running = label_of("engine-bar-running", cx);
    assert!(
        running.contains("1 running"),
        "the engine bar names the running count: {running}"
    );
}

#[gpui_kit::test]
fn throne04_status_aggregates_without_naming_a_profile(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::{HealthKind, HealthSample, ProbeSuccess, TestOutcome};
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let rows = ["Alpha", "Beta", "Gamma"]
        .into_iter()
        .zip(["10931", "10932", "10933"])
        .map(|(name, port)| store.add(t30_engine_seed(name, port)).expect("seed works"))
        .collect::<Vec<_>>();
    store
        .add_traffic_batch(&[(rows[0].id.clone(), 2_048, 1_024)])
        .expect("traffic accrues");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_all(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        3,
        "three endpoints serve"
    );

    for (index, row) in rows.iter().enumerate() {
        let live = index < 2;
        cx.update(|_, cx| {
            view.update(cx, |view, _cx| {
                view.health_store.record_outcome(&TestOutcome {
                    sample: HealthSample {
                        config_id: row.id.clone(),
                        at: 1,
                        kind: HealthKind::UrlTest,
                        latency_ms: live.then_some(12),
                        error: if live {
                            None
                        } else {
                            Some("timeout".to_string())
                        },
                    },
                    success: live.then_some(ProbeSuccess {
                        latency_ms: 12,
                        bytes_rx: 0,
                        bytes_tx: 0,
                    }),
                });
            });
        });
    }
    let (total, engine_text, health, totals) = view.read_with(cx, |view, cx| {
        (
            view.libraries.read(cx).delegate().counts().0,
            view.engine.status_text(),
            view.health_counts(),
            manyproxy_ui::summed_traffic_text(&view.store),
        )
    });
    assert_eq!(total, 3, "three rows counted");
    assert_eq!(health, (2, 1), "health counts the live and the dead");
    assert_eq!(engine_text, "engine running");
    assert_eq!(
        manyproxy_ui::status_count_text(total, 3),
        "3 configs · 3 set for launch"
    );
    let totals = totals.expect("traffic totals render once bytes flow");
    assert_eq!(totals, "out 1.0 KB · in 2.0 KB", "traffic totals read");

    for text in [&engine_text, &totals] {
        for name in ["Alpha", "Beta", "Gamma"] {
            assert!(
                !text.contains(name),
                "footer never names one profile ({name} in {text})"
            );
        }
    }
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "action-bar-count",
        "engine-bar-status",
        "engine-bar-traffic",
    ] {
        assert_painted(cx, id);
    }
}

/// F4: the last engine error outlives the toast. A failed start leaves the
/// controller's error set while the status is merely `stopped`; the strip must
/// carry the reason until a later start clears it.
#[gpui_kit::test]
fn f04_last_engine_error_paints_after_the_toast(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "192.0.2.10", "10801"))
        .expect("seed works");
    let rows = store.list().expect("list works");
    let (mut engine, handle) = EngineController::fake();
    handle.fail_check("boom: bad field");
    engine.start_all(&rows).expect_err("the fake check fails");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.engine = engine);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "engine-bar-status");
    let text = label_of("engine-bar-error", cx);
    assert!(
        text.contains("boom: bad field"),
        "the failed start stays on the bar: {text}"
    );
    assert_eq!(
        label_of("engine-bar-status", cx),
        "engine stopped",
        "the bar still names the engine state"
    );
}

/// F4: the persistent line is only as useful as its install hint — the missing
/// core path must say what to install, not just that a file is absent.
#[test]
fn f04_missing_binary_error_carries_the_install_hint() {
    use manyproxy_ui::engine::EngineError;
    let error = EngineError::MissingBinary {
        binary: "sing-box.exe".to_string(),
        searched: "C:\\app".to_string(),
    };
    let text = error.to_string();
    assert!(text.contains("engine binary not found"), "{text}");
    assert!(text.contains("install sing-box"), "{text}");
    assert!(
        text.contains("never downloads executables silently"),
        "{text}"
    );
}

/// F4: Settings names the source the persistent error line points at.
#[gpui_kit::test]
fn f04_settings_offer_the_core_guide(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    // B17: the Engine section sits below the fold of the one column, so the
    // test scrolls to it exactly as a reader would.
    scroll_settings_into_view(cx, "settings-engine-cores-help");
    assert_painted(cx, "settings-engine-cores-help");
    cx.update(|window, cx| {
        window.click("settings-engine-cores-help", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_painted(cx, "cores-help-body");
    assert_painted(cx, "cores-help-open-sing-box");
    assert_painted(cx, "cores-help-open-xray");
    assert_painted(cx, "cores-help-dialog-close");
}

/// F6: the flag means restore-on-launch. Two rows with one flag cleared: the
/// count says `1 set for launch` — never "enabled", which read as "will
/// start" while Start all started every row — and the row menu offers the
/// renamed toggle.
#[gpui_kit::test]
fn f06_restore_flag_count_and_row_menu(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let on = store
        .add(row19_seed("Work", "192.0.2.11", "10801"))
        .expect("seed works");
    let off = store
        .add(row19_seed("Home", "192.0.2.12", "10802"))
        .expect("seed works");
    store.set_enabled(&on.id, true).expect("flag sets");
    store.set_enabled(&off.id, false).expect("flag clears");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        label_of("action-bar-count", cx),
        "2 configs · 1 set for launch",
        "the count names the launch contract, not a start promise"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    open_inspector(&view, cx);
    paint_frame_at(cx, 1000., 1600.);
    assert_painted(cx, "row-menu");
    cx.update(|window, cx| {
        window.click("row-menu", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    assert_painted(cx, "kebab-auto-start");
    assert_eq!(
        label_of("kebab-auto-start", cx),
        "Restore on launch",
        "the menu item names what the flag does"
    );
}

#[gpui_kit::test]
fn t11_engine_bar_keeps_library_totals_at_minimum(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    store
        .add_traffic_batch(&[(seeded.id.clone(), 2_048, 1_024)])
        .expect("traffic accrues");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);

    assert_painted(cx, "action-bar-count");
    assert_within_frame_min(cx, "action-bar-count");

    for id in [
        "engine-bar",
        "engine-bar-status",
        "engine-bar-running",
        "engine-bar-traffic",
    ] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
}

#[gpui_kit::test]
fn t10_engine_bar_slots_hold_width_across_value_changes(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(t30_engine_seed("Work", "10801"))
        .expect("seed works");
    store
        .add_traffic_batch(&[(seeded.id.clone(), 2_048, 1_024)])
        .expect("traffic accrues");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let keystroke = Keystroke::parse("f5").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "a port serves in the before-state"
    );
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let region_widths = |cx: &mut VisualTestContext| {
        [
            "engine-bar-status",
            "engine-bar-running",
            "engine-bar-traffic",
        ]
        .iter()
        .map(|id| {
            cx.update(|window, _| {
                window
                    .try_find(id.to_string())
                    .unwrap_or_else(|| panic!("{id} renders"))
                    .bounds()
                    .size
                    .width
            })
        })
        .collect::<Vec<_>>()
    };
    let before = region_widths(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.store
                .add(t30_engine_seed("Home", "10802"))
                .expect("seed works");
            let accruals: Vec<(String, u64, u64)> = view
                .store
                .list()
                .expect("list works")
                .iter()
                .map(|row| (row.id.clone(), 5_000_000, 2_500_000))
                .collect();
            view.store
                .add_traffic_batch(&accruals)
                .expect("traffic accrues");
            view.reload(window, cx);
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "the same single port serves in the after-state"
    );
    let after = region_widths(cx);
    assert_eq!(
        before, after,
        "fixed slots hold across value changes: {before:?} vs {after:?}"
    );
}

#[gpui_kit::test]
fn k12_engine_bar_names_engine_running_and_traffic(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    store
        .set_enabled(&seeded.id, true)
        .expect("row enables for the count grammar");
    store
        .add_traffic_batch(&[(seeded.id.clone(), 2_048, 1_024)])
        .expect("traffic accrues");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        label_of("action-bar-count", cx),
        "1 config · 1 set for launch",
        "the action bar counts the library"
    );
    assert_eq!(
        label_of("engine-bar-status", cx),
        "engine stopped",
        "the engine bar names the engine state"
    );
    assert_eq!(
        label_of("engine-bar-running", cx),
        "· 0 running",
        "the engine bar names the running count"
    );
    assert_eq!(
        label_of("engine-bar-traffic", cx),
        "out 1.0 KB · in 2.0 KB",
        "usage names both directions in words"
    );

    for id in [
        "action-bar",
        "engine-bar",
        "engine-bar-status",
        "engine-bar-running",
        "engine-bar-traffic",
    ] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "status-ports".to_string());
    assert_absent(cx, "conn-counters".to_string());
}

/// K12/A5-2: nothing paints left of the first segment, and a bar with no
/// usage holds no lone separator — the stray dash the video caught.
#[gpui_kit::test]
fn k12_engine_bar_counts_stay_honest(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    let bounds = |id: &str, cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
        })
    };
    let bar = bounds("engine-bar", cx);
    let status = bounds("engine-bar-status", cx);
    assert_eq!(
        status.origin.x,
        bar.origin.x + px(12.),
        "the first segment starts inside the bar's padding: {bar:?} vs {status:?}"
    );
    assert_eq!(
        label_of("action-bar-count", cx),
        "0 configs · 0 set for launch",
        "empty counts stay honest"
    );
    assert_eq!(
        label_of("engine-bar-traffic", cx),
        "out 0 B · in 0 B",
        "zero usage stays honest instead of posing as traffic"
    );
    assert_absent(cx, "status-filter".to_string());
    assert_absent(cx, "status-ports".to_string());
    assert_absent(cx, "conn-counters".to_string());
}

/// K12/A5-1: the marker is a word, never a glyph — `Health · asc`, not
/// `Health ↑` (which degraded to `Health 1` at the header size).
#[test]
fn k12_sort_header_reads_as_words() {
    use manyproxy_ui::store::{LibrarySort, LibrarySortKey, SortDirection};
    let marker =
        |key, direction| manyproxy_ui::sort_header_marker(LibrarySort::new(key, direction));
    assert_eq!(
        marker(LibrarySortKey::None, SortDirection::Ascending),
        "",
        "library order adds no marker"
    );
    assert_eq!(
        marker(LibrarySortKey::Latency, SortDirection::Ascending),
        " · asc"
    );
    assert_eq!(
        marker(LibrarySortKey::Latency, SortDirection::Descending),
        " · desc"
    );
    assert_eq!(
        marker(LibrarySortKey::TypeSecurity, SortDirection::Descending),
        " · fixed",
        "the retired toggle keeps its one fixed direction"
    );
    for text in [" · asc", " · desc", " · fixed"] {
        assert!(
            !text.contains('↑') && !text.contains('↓'),
            "no arrow glyph survives: {text:?}"
        );
    }
}

/// K12/A5-1: the rendered header carries the word marker, and the default
/// sort adds none.
#[gpui_kit::test]
fn k12_sort_header_renders_words(cx: &mut TestAppContext) {
    use manyproxy_ui::store::LibrarySortKey;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        label_of("sort-header-health", cx),
        "Health",
        "library order adds no marker"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_library_sort(LibrarySortKey::Latency, window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    let live = view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().sort);
    let expected = format!("Health{}", manyproxy_ui::sort_header_marker(live));
    assert_eq!(label_of("sort-header-health", cx), expected);
    assert!(
        expected.contains(" · asc") || expected.contains(" · desc"),
        "the direction is a word: {expected:?}"
    );
    assert_ne!(label_of("sort-header-health", cx), "Health 1");
}

/// K12/A1-1: the custom chrome paints the drag band and three caption cells,
/// tiled edge to edge at the trailing end.
#[gpui_kit::test]
fn k12_titlebar_controls_render(cx: &mut TestAppContext) {
    use manyproxy_ui::app::window::{TITLEBAR_CONTROL_WIDTH, TITLEBAR_HEIGHT};
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    for id in [
        "title-bar",
        "titlebar-drag-region",
        "titlebar-title",
        "titlebar-controls",
        "titlebar-minimize",
        "titlebar-maximize",
        "titlebar-close",
    ] {
        assert_painted(cx, id);
    }

    let bounds = |id: &str, cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
        })
    };
    let bar = bounds("title-bar", cx);
    assert_eq!(
        bar.size.height,
        px(TITLEBAR_HEIGHT),
        "the chrome keeps its vertical budget"
    );
    let cells = [
        bounds("titlebar-minimize", cx),
        bounds("titlebar-maximize", cx),
        bounds("titlebar-close", cx),
    ];
    for cell in cells {
        assert_eq!(cell.size.width, px(TITLEBAR_CONTROL_WIDTH));
        assert_eq!(
            cell.size.height,
            bar.size.height - px(1.),
            "the cell fills the band above its bottom hairline"
        );
    }
    assert!(
        (cells[0].origin.x.as_f32() + TITLEBAR_CONTROL_WIDTH - cells[1].origin.x.as_f32()).abs()
            < 0.5,
        "min | max tiles without a gap: {cells:?}"
    );
    assert!(
        (cells[1].origin.x.as_f32() + TITLEBAR_CONTROL_WIDTH - cells[2].origin.x.as_f32()).abs()
            < 0.5,
        "max | close tiles without a gap: {cells:?}"
    );
    assert!(
        (cells[2].origin.x.as_f32() + cells[2].size.width.as_f32()
            - bar.origin.x.as_f32()
            - bar.size.width.as_f32())
        .abs()
            < 0.5,
        "close ends the bar: {bar:?} vs {cells:?}"
    );
    let drag = bounds("titlebar-drag-region", cx);
    assert!(
        (drag.origin.x.as_f32() + drag.size.width.as_f32() - cells[0].origin.x.as_f32()).abs()
            < 0.5,
        "the drag region reaches the first control: {drag:?} vs {cells:?}"
    );

    assert_eq!(label_of("titlebar-minimize", cx), "Minimize the window");
    assert_eq!(label_of("titlebar-maximize", cx), "Maximize the window");
    assert_eq!(label_of("titlebar-close", cx), "Close the window");
}

/// K12/A1-1: each caption id maps to its window action, and a double click on
/// the title area asks for the same toggle as the maximize cell. The test
/// platform's `zoom`/`minimize` are unimplemented, so `apply_titlebar_action`
/// only counts the call here; the mapping itself is data.
#[gpui_kit::test]
fn k12_titlebar_control_actions_and_double_click(cx: &mut TestAppContext) {
    use manyproxy_ui::app::window::{
        titlebar_control_action, titlebar_double_click_action, TitlebarAction, TitlebarControl,
        TITLEBAR_ACTION_CALLS,
    };
    use std::sync::atomic::Ordering;

    assert_eq!(
        titlebar_control_action(TitlebarControl::Minimize),
        TitlebarAction::Minimize
    );
    assert_eq!(
        titlebar_control_action(TitlebarControl::Maximize),
        TitlebarAction::ToggleMaximize
    );
    assert_eq!(
        titlebar_control_action(TitlebarControl::Close),
        TitlebarAction::Close
    );
    assert_eq!(
        titlebar_double_click_action(),
        TitlebarAction::ToggleMaximize,
        "double-click toggles maximize"
    );
    assert_eq!(TitlebarControl::Minimize.id(), "titlebar-minimize");
    assert_eq!(TitlebarControl::Maximize.id(), "titlebar-maximize");
    assert_eq!(TitlebarControl::Close.id(), "titlebar-close");

    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    let before = TITLEBAR_ACTION_CALLS.load(Ordering::SeqCst);
    cx.update(|window, cx| window.click("titlebar-maximize".to_string(), cx));
    assert_eq!(
        TITLEBAR_ACTION_CALLS.load(Ordering::SeqCst),
        before + 1,
        "the maximize cell reaches the window action"
    );

    let before = TITLEBAR_ACTION_CALLS.load(Ordering::SeqCst);
    cx.update(|window, cx| window.click("titlebar-drag-region".to_string(), cx));
    assert_eq!(
        TITLEBAR_ACTION_CALLS.load(Ordering::SeqCst),
        before,
        "a single click never toggles"
    );
    cx.update(|window, cx| window.double_click("titlebar-drag-region".to_string(), cx));
    assert_eq!(
        TITLEBAR_ACTION_CALLS.load(Ordering::SeqCst),
        before + 1,
        "double-clicking the title area toggles maximize"
    );
}

#[gpui_kit::test]
fn t11_add_dialog_paints_inside_minimum_frame(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);

    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Add opens at minimum size"
    );
    for id in [
        "field-name",
        "protocol-select",
        "field-guided-server",
        "field-guided-port",
        "guided-hint",
        "add-dialog-ok",
        "add-dialog-cancel",
    ] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    assert_absent(cx, "field-guided-identity".to_string());
    assert_absent(cx, "advanced-settings-section".to_string());

    // K2: the compact Group / Bind / Port row sits below the fold at the
    // minimum frame; the dialog body scrolls to reach it.
    cx.update(|window, cx| {
        window.scroll(
            "field-guided-server",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    for id in ["field-group", "field-bind", "field-port"] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }

    cx.update(|window, cx| {
        window.scroll(
            "field-group",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    // F5: identity sits between Common and Advanced, so at the minimum frame
    // the Advanced toggle is below the fold and the body scrolls to reach it.
    // Unfolding the disclosure is covered at height by t02_vless_panels_paint;
    // a guided protocol never grows the raw JSON row either way.
    assert_absent(cx, "field-params".to_string());
}

#[gpui_kit::test]
fn t08_keyboard_add_start_copy_urls_pointer_free(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);

    let expected = [
        ("Add configuration", "Ctrl-N"),
        ("Import configurations", "Ctrl-O"),
        ("Paste clipboard into Import", "Ctrl-V"),
        ("Start selected", "F5"),
        ("Stop selected", "Shift-F5"),
        ("Test selected", "Ctrl-T"),
        ("Stop Testing", "Ctrl-."),
        ("Edit endpoint", "Ctrl-E"),
        ("Edit config", "Ctrl-Shift-E"),
        ("Delete selected", "Delete"),
        ("Extend selection up", "Shift-Up"),
        ("Extend selection down", "Shift-Down"),
        ("Move focus up", "Up"),
        ("Move focus down", "Down"),
        ("Move focus to first row", "Home"),
        ("Move focus to last row", "End"),
        ("Toggle focused row", "Space"),
        ("Select all visible", "Ctrl-A"),
        ("Focus search", "Ctrl-F"),
        ("Toggle inspector", "Ctrl-I"),
        ("Toggle console", "Ctrl-`"),
        ("Clear selection", "Esc"),
        ("Copy URLs", "Ctrl-U"),
        ("Command palette", "Ctrl-K"),
        ("Update the current scope", "Ctrl-R"),
    ];
    assert_eq!(manyproxy_ui::SHORTCUT_LINES.len(), expected.len());
    for (line, want) in manyproxy_ui::SHORTCUT_LINES.iter().zip(expected) {
        assert_eq!(*line, want);
    }
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            use manyproxy_ui::engine::EngineController;
            use manyproxy_ui::health::FakeHealthProber;
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, _) = FakeHealthProber::shared();
            view.prober = Box::new(prober);
        });
    });

    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let press = |cx: &mut VisualTestContext, key: &str| {
        let keystroke = Keystroke::parse(key).expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        });
    };

    press(cx, "ctrl-n");
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl-N opens Add"
    );
    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));
    let added = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_add(t26_engine_seed("Keyflow", "10981"), window, cx)
        })
    });
    assert!(added, "shared add commit stores the row");
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let before = notification_count(cx);
    press(cx, "f5");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "F5 starts the selected row"
    );
    assert_eq!(notification_count(cx), before + 1, "start toasts once");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            manyproxy_ui::app::actions::run_action(
                view,
                "copy-urls",
                manyproxy_ui::app::actions::ActionTarget::Selection,
                window,
                cx,
            )
        });
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    let (bind, port) = view.read_with(cx, |view, cx| {
        let row = view.selected_config(cx).expect("selected");
        (row.bind_ip.clone(), row.port)
    });
    let (socks, http) = manyproxy_ui::store::proxy_urls(&bind, port);
    assert_eq!(clip, format!("{socks}\n{http}"));
    assert!(clip.contains("socks5://") && clip.contains("\nhttp://"));
    assert!(
        !clip.contains("198.51.100.9") && !clip.contains("11111111"),
        "clipboard carries bind+port only: {clip}"
    );

    let before = notification_count(cx);
    press(cx, "shift-f5");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "Shift-F5 stops the selected row"
    );
    assert_eq!(notification_count(cx), before + 1, "stop toasts once");

    let before = notification_count(cx);
    press(cx, "ctrl-t");
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        before + 1,
        "Ctrl-T routes with nothing running"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts"
    );

    let id = view.read_with(cx, |view, cx| {
        view.selected_config(cx).expect("selected").id
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&id, cx);
        });
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let before = notification_count(cx);
    press(cx, "escape");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        0,
        "Escape clears the selection"
    );
    assert_eq!(notification_count(cx), before, "Escape clear is silent");
    assert_eq!(
        view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id)),
        None,
        "Escape clears the selection"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&id, cx);
        });
    });
    cx.update(|window, cx| {
        window.open_dialog(cx, |dialog: Dialog, _, _| dialog.title("Guard probe"));
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let kept = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx)
        })
    });
    assert!(!kept, "escape keeps the set while a dialog is open");
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 1);
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    let focus = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    let before = notification_count(cx);
    press(cx, "down");
    press(cx, "up");
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "arrows open nothing"
    );
    assert_eq!(notification_count(cx), before, "arrows toast nothing");
    assert_eq!(
        view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id)),
        Some(id.clone()),
        "arrows keep the selection"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        1,
        "the guard probe leaves its one-row selection in place"
    );
}

#[gpui_kit::test]
fn t69_sweep_clears_only_dead_excluding_running_and_editing(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let live = store
        .add(t19_auto_clear_seed("Live", "10931", "g"))
        .expect("add live works");
    let failing = store
        .add(t19_auto_clear_seed("Fail", "10932", "g"))
        .expect("add fail works");
    let guarded = store
        .add(t19_auto_clear_seed("Guarded", "10933", "g"))
        .expect("add guarded works");
    let followed = store
        .add(t19_auto_clear_seed("Followed", "10934", "g"))
        .expect("add followed works");
    let free = store
        .add(t19_auto_clear_seed("Free", "10935", "g"))
        .expect("add free works");

    // Followed belongs to a subscription: stamp it as such through the
    // export/restore fidelity path so the row-level batch exclusion has a
    // real subscription to see.
    let payload = store.export_library_json().expect("export works");
    let mut value: serde_json::Value = serde_json::from_str(&payload).expect("payload parses");
    for entry in value
        .get_mut("library")
        .and_then(|v| v.as_array_mut())
        .into_iter()
        .flatten()
    {
        if entry.get("id").and_then(serde_json::Value::as_str) == Some(followed.id.as_str()) {
            entry["subscription_id"] = serde_json::Value::String("sub-e".to_string());
        }
    }
    let mut store = Store::open_in_memory().expect("re-open works");
    // T-22: the hand-edited fixture is re-sealed so its checksum matches.
    let edited = manyproxy_ui::store::seal_library_export(&value);
    store.restore_library_json(&edited).expect("restore works");
    // Dead snapshots live only in the source store — export/restore
    // carries the library, not health state — so seed them on the
    // restored store the sweep will consume.
    for id in [&guarded.id, &followed.id, &free.id] {
        store
            .save_health_snapshot(&t19_dead_snapshot(id, "upstream refused"))
            .expect("dead snapshot works");
    }
    store
        .set_auto_clear_group(
            "g",
            AutoClearGroupConfig {
                enabled: true,
                on_date: 5,
                note_seen: true,
            },
        )
        .expect("flag enables");

    let (view, cx) = mount_view(store, cx);

    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
            view.refresh.members.insert("sub-e".to_string());
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // A filter hides every row; the sweep must still reach the
            // library, not the visible slice.
            view.set_status_facet(manyproxy_ui::StatusFacet::Running, true, cx);
            view.start_row(&live.id, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&failing.id, window, cx));
    });
    cx.run_until_parked();
    // Simulate the edit dialog for Guarded being open while the sweep
    // runs: opened after the starts above (commit_start_selected's
    // reload drops a stale marker before the dialog truly opened).
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.edit_form_open = Some(guarded.id.clone());
            // The sweep is group-scoped, and the group now comes from the
            // selection.
            view.select_visible_row(0, cx);
        });
    });
    let running = view.read_with(cx, |view, _| view.engine.running_ids());
    assert!(
        running.contains(&live.id) && running.contains(&failing.id),
        "Live and Fail run while Guarded/Followed/Free stay stopped: {running:?}"
    );
    handle.script_latency(&live.id, 21);
    handle.script_failure(&failing.id, "upstream refused");

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_group_health(window, cx));
    });
    cx.run_until_parked();

    let names: Vec<String> = view.read_with(cx, |view, _| {
        view.store
            .list()
            .expect("list works")
            .into_iter()
            .map(|row| row.name)
            .collect()
    });
    assert!(
        !names.contains(&"Free".to_string())
            && names.contains(&"Live".to_string())
            && names.contains(&"Fail".to_string())
            && names.contains(&"Guarded".to_string())
            && names.contains(&"Followed".to_string()),
        "auto-clear removed exactly the unprotected dead row: {names:?}"
    );
    assert_eq!(
        notification_count(cx),
        before + 2,
        "sweep summary plus ONE auto-clear summary, nothing else"
    );
}

#[test]
fn t69_summary_notification_names_count_and_kept() {
    use manyproxy_ui::auto_clear_summary_toast;

    // Exact ticket wording: count always named, always plural "rows",
    // kept reasons named only when non-empty, canonical order.
    assert_eq!(
        auto_clear_summary_toast("g", 1, &["running".to_string(), "editing".to_string()],),
        "Auto-cleared 1 unavailable rows in \"g\" (kept: running, editing)",
    );
    assert_eq!(
        auto_clear_summary_toast(
            "g",
            2,
            &[
                "running".to_string(),
                "editing".to_string(),
                "in-flight batch".to_string(),
            ],
        ),
        "Auto-cleared 2 unavailable rows in \"g\" (kept: running, editing, \
             in-flight batch)",
    );
    assert_eq!(
        auto_clear_summary_toast("g", 3, &[]),
        "Auto-cleared 3 unavailable rows in \"g\"",
        "nothing kept: no kept clause",
    );
    assert_eq!(
        auto_clear_summary_toast("g", 0, &["editing".to_string()]),
        "Auto-cleared 0 unavailable rows in \"g\" (kept: editing)",
        "all candidates protected: count names the zero",
    );
    assert_eq!(
        auto_clear_summary_toast("travel", 1, &["running".to_string()]),
        "Auto-cleared 1 unavailable rows in \"travel\" (kept: running)",
    );
}

#[gpui_kit::test]
fn t69_sweep_auto_clear_needs_the_group_opt_in(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let live = store
        .add(t19_auto_clear_seed("Live", "10941", "g"))
        .expect("add live works");
    let stray = store
        .add(t19_auto_clear_seed("Stray", "10942", "g"))
        .expect("add stray works");
    store
        .save_health_snapshot(&t19_dead_snapshot(&stray.id, "upstream refused"))
        .expect("dead snapshot works");
    // Meta AUTO_CLEAR_GROUPS deliberately absent: nothing else — a delete
    // path, a confirm toggle, anything — arms the sweep auto-clear by itself.

    let (view, cx) = mount_view(store, cx);
    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&live.id, window, cx));
    });
    cx.run_until_parked();
    handle.script_latency(&live.id, 21);

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_status_facet(manyproxy_ui::StatusFacet::Running, true, cx);
            view.test_group_health(window, cx);
        });
    });
    cx.run_until_parked();

    assert_eq!(
        notification_count(cx),
        before + 1,
        "sweep summary only — no auto-clear notice without the opt-in"
    );
    let names: Vec<String> = view.read_with(cx, |view, _| {
        view.store
            .list()
            .expect("list works")
            .into_iter()
            .map(|row| row.name)
            .collect()
    });
    assert!(
        names.contains(&"Stray".to_string()),
        "the sweep without the opt-in removed nothing: {names:?}"
    );

    // And the data-layer note gate still refuses: acknowledgment, not bypass.
    let refused = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .set_auto_clear_group(
                    "g",
                    AutoClearGroupConfig {
                        enabled: true,
                        on_date: 1,
                        note_seen: false,
                    },
                )
                .is_err()
        })
    });
    assert!(refused, "enable without note_seen stays an error");
}

#[test]
fn t69_optin_confirm_note_required() {
    use manyproxy_ui::auto_clear_opt_in_note;

    // The toggle-on dialog posts this exact ticket wording as its own ACK;
    // skip-delete-confirm never replaces it.
    assert_eq!(
        auto_clear_opt_in_note("g"),
        "Unavailable rows in \"g\" will be removed automatically after group tests pass \
             with dead rows. Running rows and the row being edited are never removed.",
    );

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let refused = store.set_auto_clear_group(
        "g",
        AutoClearGroupConfig {
            enabled: true,
            on_date: 5,
            note_seen: false,
        },
    );
    assert!(
        matches!(refused, Err(StoreError::Invalid(_))),
        "enable without the note ACK is an Invalid error"
    );
    assert!(
        !store.auto_clear_enabled("g"),
        "refused enable leaves the flag off"
    );
    store
        .set_auto_clear_group(
            "g",
            AutoClearGroupConfig {
                enabled: true,
                on_date: 5,
                note_seen: true,
            },
        )
        .expect("note ACK enables");
    assert!(
        store.auto_clear_enabled("g"),
        "confirmed opt-in arms the flag"
    );
}

#[test]
fn t67_shortcut_lines_lists_stop_testing_key() {
    assert!(
        manyproxy_ui::SHORTCUT_LINES.contains(&("Stop Testing", "Ctrl-.")),
        "SHORTCUT_LINES lists Stop Testing: Ctrl-."
    );
    assert_eq!(
        manyproxy_ui::SHORTCUT_LINES
            .iter()
            .filter(|(action, _)| *action == "Stop Testing")
            .count(),
        1,
        "exactly one Stop Testing line"
    );
}

/// T-04: the Shortcuts section lists EVERY binding `bind_app_keys` installs.
/// The test reads the live keymap, so the list cannot silently drop a key —
/// the old 16-line subset lost Ctrl-4/5/6 and Ctrl-R.
#[gpui_kit::test]
fn t04_shortcut_lines_cover_every_bound_key(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let added = bound_key_strings(cx);

    assert_eq!(
        added.len(),
        manyproxy_ui::SHORTCUT_LINES.len(),
        "the reference lists every binding: {added:?} vs {:?}",
        manyproxy_ui::SHORTCUT_LINES
    );
    for (action, keys) in manyproxy_ui::SHORTCUT_LINES.iter() {
        let canonical = canonical_shortcut(keys);
        assert!(
            added.contains(&canonical),
            "{action} is bound as {canonical}: {added:?}"
        );
    }
    for key in ["ctrl-r", "ctrl-`"] {
        assert!(
            added.contains(&key.to_string()),
            "{key} is bound and must be listed"
        );
    }
    for key in ["ctrl-1", "ctrl-2", "ctrl-3", "ctrl-4", "ctrl-5", "ctrl-6"] {
        assert!(
            !added.contains(&key.to_string()),
            "{key} no longer switches inspector panes and must not be bound"
        );
    }
}

#[gpui_kit::test]
fn t67_ctrl_period_stops_group_test(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    use std::sync::atomic::Ordering;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let press = |cx: &mut VisualTestContext, key: &str| {
        let keystroke = Keystroke::parse(key).expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        });
    };

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.probe_stop.store(false, Ordering::SeqCst);
        });
    });
    let before = notification_count(cx);
    press(cx, "ctrl-.");
    cx.run_until_parked();
    let idle_flipped = view.read_with(cx, |view, _| view.probe_stop.load(Ordering::SeqCst));
    assert!(!idle_flipped, "idle press leaves the stop flag untouched");
    assert_eq!(
        notification_count(cx),
        before + 1,
        "idle press reports through the existing no-op toast"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl-. stays dialog-free"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.group_test_in_flight = true;
            view.probe_stop.store(false, Ordering::SeqCst);
        });
    });
    let before = notification_count(cx);
    press(cx, "ctrl-.");
    cx.run_until_parked();
    let flipped = view.read_with(cx, |view, _| view.probe_stop.load(Ordering::SeqCst));
    assert!(
        flipped,
        "Ctrl-. flips probe_stop exactly like the Tools menu item"
    );
    assert_eq!(notification_count(cx), before + 1, "stopping toasts once");
}

#[gpui_kit::test]
fn t67_engine_bar_menu_names_the_action_alone(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("engine-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let label = cx.update(|window, _| {
        window
            .try_find("engine-bar-stop-testing".to_string())
            .unwrap_or_else(|| panic!("Stop testing renders in the engine bar menu"))
            .label()
            .map(str::to_string)
    });
    assert_eq!(
        label.as_deref(),
        Some("Stop testing"),
        "the engine bar's Stop testing announces the action alone"
    );
    // The binding lives in the palette, not in a chip beside the row.
    assert!(
        cx.debug_bounds("kbd:ctrl-.").is_none(),
        "the engine bar's menu row carries no shortcut chip"
    );
}

#[gpui_kit::test]
fn t67_show_security_toggles_in_place_without_toast(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    let before = notification_count(cx);
    // The toolbar's Show security item is retired with the toolbar; the
    // delegate's own flag stays until the protocol column carries the
    // security suffix.
    assert_absent(cx, "show-security".to_string());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_show_security(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, cx| view
            .libraries
            .read(cx)
            .delegate()
            .show_security),
        "the security flag flips the delegate in place"
    );
    assert_eq!(
        notification_count(cx),
        before,
        "the security flag renders in place — no toast"
    );
}

#[gpui_kit::test]
fn t08_secrets_audit_surfaces_carry_no_credentials(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    fn secret_seed(name: &str, port: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),

            params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"SECRET-UUID-08\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\",\
                    \"password\":\"hunter2\",\"token\":\"tok-abc123-08\"}"
                .to_string()
                .into(),
            group: "travel".to_string(),
            bind_ip: "127.0.0.1".to_string(),
            port: port.to_string(),
        }
    }
    const CANARIES: &[&str] = &["hunter2", "SECRET-UUID-08", "tok-abc123-08"];
    fn assert_no_canaries(text: &str, surface: &str) {
        for &canary in CANARIES {
            assert!(
                !text.contains(canary),
                "{surface} surfaces {canary}: {text}"
            );
        }
    }
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store.add(secret_seed("Work", "10982")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let row = view.read_with(cx, |view, cx| view.selected_config(cx).expect("selected"));
    assert_eq!(row.id, seeded.id);

    let (socks, http) = manyproxy_ui::store::proxy_urls(&row.bind_ip, row.port);
    assert_eq!(socks, "socks5://127.0.0.1:10982");
    assert_eq!(http, "http://127.0.0.1:10982");
    assert_no_canaries(&socks, "proxy_urls socks");
    assert_no_canaries(&http, "proxy_urls http");
    let upstream = manyproxy_ui::upstream_host_port(&row).expect("upstream reads");
    assert_eq!(upstream, "198.51.100.9:443");
    assert_no_canaries(&upstream, "upstream");
    let cells =
        manyproxy_ui::library_row_cells(&row, &upstream, Some("87ms"), true, false, None, false);
    let route = manyproxy_ui::library_row_full_route(&row, &upstream);
    assert!(
        route.contains("127.0.0.1:10982"),
        "bind+port visible on the route tooltip: {route}"
    );
    let shown = [row.name.clone(), upstream.clone(), route.clone()]
        .into_iter()
        .chain(cells.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
    assert_no_canaries(&shown, "row cells");
    let base = manyproxy_core::library_row_lines(&row, Some("87ms"), true, None).join("\n");
    assert_no_canaries(&base, "base row");
    let badge = manyproxy_ui::serving_badge(true, &row.bind_ip, row.port, Some("87ms"));
    assert_no_canaries(&badge, "serving badge");
    assert_no_canaries(
        &manyproxy_ui::health::redact_secrets(
            "dial tcp 127.0.0.1:10982: password=hunter2 uuid=SECRET-UUID-08 token=tok-abc123-08",
        ),
        "redact_secrets",
    );

    let cached = view.read_with(cx, |view, cx| {
        let row = view.selected_config(cx).expect("selected");
        view.cached_upstream(cx, &row)
    });
    assert_eq!(cached, "198.51.100.9:443");
    assert_no_canaries(&cached, "inspector upstream");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            manyproxy_ui::app::actions::run_action(
                view,
                "copy-urls",
                manyproxy_ui::app::actions::ActionTarget::Selection,
                window,
                cx,
            )
        });
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(clip, format!("{socks}\n{http}"));
    assert_no_canaries(&clip, "clipboard");
    assert!(
        !clip.contains("198.51.100.9"),
        "clipboard carries bind+port only: {clip}"
    );

    cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.logs.store.push_raw(
                    "dial tcp 127.0.0.1:10982: password=hunter2 uuid=SECRET-UUID-08 token=tok-abc123-08",
                    &manyproxy_ui::health::LogTagIndex::default(),
                );
            });
        });
    let (stored, visible) = view.read_with(cx, |view, cx| {
        let stored = view
            .logs
            .store
            .all()
            .iter()
            .map(|record| record.message.clone())
            .collect::<Vec<_>>();
        (stored, view.log_lines(cx))
    });
    assert_eq!(stored.len(), 1);
    assert_no_canaries(&stored[0], "stored log");
    assert!(
        stored[0].contains("127.0.0.1:10982"),
        "bind+port survives redaction: {}",
        stored[0]
    );
    for line in &visible {
        assert_no_canaries(line, "visible log");
    }

    let status = view.read_with(cx, |view, _| {
        (
            manyproxy_ui::summed_traffic_text(&view.store).unwrap_or_default(),
            view.engine.status_text(),
        )
    });
    assert_no_canaries(&status.0, "traffic status");
    assert_no_canaries(&status.1, "engine status");

    cx.update(|_, cx| {
            view.update(cx, |view, _| {
                use manyproxy_ui::engine::EngineController;
                use manyproxy_ui::health::FakeHealthProber;
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                let (prober, handle) = FakeHealthProber::shared();
                handle.script_failure(
                    &seeded.id,
                    "dial tcp 127.0.0.1:10982: password=hunter2 uuid=SECRET-UUID-08 token=tok-abc123-08",
                );
                view.prober = Box::new(prober);
            });
        });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_engine(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "probe target runs"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_selected_health(window, cx));
    });
    cx.run_until_parked();
    cx.run_until_parked();
    paint_frame(cx);
    let (summary, detail) = view.read_with(cx, |view, cx| {
        let summary = view
            .libraries
            .read(cx)
            .delegate()
            .health
            .get(&seeded.id)
            .cloned()
            .unwrap_or_default();
        (summary, view.health_store.detail_line(&seeded.id))
    });
    assert!(summary.contains("dead"), "failure recorded: {summary}");
    assert_no_canaries(&summary, "row health");
    assert_no_canaries(&detail, "inspector health");

    use manyproxy_ui::store::{export_confirm_description, export_warning_text};
    assert_eq!(export_confirm_description(), export_warning_text());
    for class in ["private keys", "tokens", "passwords", "plaintext"] {
        assert!(
            export_warning_text().contains(class),
            "warning names {class}: {}",
            export_warning_text()
        );
    }
    let preview =
        manyproxy_ui::export_preview_description(1, 1, "/tmp/manyproxy-t08-export.json", false);
    assert!(preview.contains('1'), "preview counts: {preview}");
    assert_no_canaries(&preview, "export preview");
    let overwrite =
        manyproxy_ui::export_overwrite_confirm_description("/tmp/manyproxy-t08-export.json");
    assert!(
        overwrite.contains("/tmp/manyproxy-t08-export.json"),
        "overwrite names the path: {overwrite}"
    );
    assert_no_canaries(&overwrite, "export overwrite");
    let target =
        std::env::temp_dir().join(format!("manyproxy-t08-export-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&target);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_export_dialog(window, cx));
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        cx.update(|window, _| window.try_find("export-warning").is_some()),
        "verbatim warning paints in the dialog"
    );
    assert!(!target.exists(), "nothing is written until confirm");

    let before = notification_count(cx);
    let mut body = None;
    for _ in 0..3 {
        let _ = std::fs::remove_file(&target);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_export_write_to(&target.display().to_string(), window, cx)
            });
        });
        if let Ok(text) = std::fs::read_to_string(&target) {
            body = Some(text);
            break;
        }
    }
    let body: String = match body {
        Some(text) => text,
        None => {
            // Name the underlying store error so a failing runner (e.g.
            // windows-arm64) reports the cause, not just "3 attempts".
            let detail = view
                .read_with(cx, |view, _| view.store.commit_export(&target, true))
                .err()
                .map(|error| error.to_string())
                .unwrap_or_else(|| "store write reported success".to_string());
            let _ = std::fs::remove_file(&target);
            panic!("export file written within 3 attempts (store error: {detail})");
        }
    };
    assert!(
        notification_count(cx) > before,
        "commit pushes a notification per attempt"
    );
    assert!(body.contains("library"), "payload has library section");
    let _ = std::fs::remove_file(&target);
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn cua02_settings_tail_clears_footer_both_frames(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    for paint in [
        paint_frame as fn(&mut VisualTestContext),
        paint_frame_min as fn(&mut VisualTestContext),
    ] {
        cx.run_until_parked();
        paint(cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
        });
        paint_dialog_open(cx);

        // B17: one column — the section headers read top to bottom in the
        // fixed order, and every section is present in that one column.
        let tops: Vec<f32> = cx.update(|window, _| {
            [
                "settings-section-general",
                "settings-section-probing",
                "settings-section-updates",
                "settings-section-engine",
                "settings-section-maintenance",
                "settings-section-shortcuts",
            ]
            .iter()
            .map(|id| {
                window
                    .try_find(id.to_string())
                    .unwrap_or_else(|| panic!("{id} renders"))
                    .bounds()
                    .origin
                    .y
                    .as_f32()
            })
            .collect()
        });
        assert!(
            tops.windows(2).all(|pair| pair[0] < pair[1]),
            "the spine keeps its order: {tops:?}"
        );
        assert!(
            cx.update(|window, _| window.try_find("settings-ticks".to_string()).is_none()),
            "K9: the Timers line is gone from the Engine section entirely"
        );

        // Maintenance is the longest section; its last row must clear the
        // footer button in every frame, reached by scrolling the one column.
        cx.update(|window, cx| {
            window.scroll(
                "settings-body",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-100_000.))),
                cx,
            );
        });
        cx.run_until_parked();
        paint_dialog_open(cx);
        // B17: the column's tail is the shortcuts list — the last thing the one
        // scroll reaches.
        assert_painted(cx, "settings-shortcuts-list");
        let (tail_bottom, body_bottom, footer_top) = cx.update(|window, _| {
            (
                window
                    .try_find("settings-shortcuts-list".to_string())
                    .expect("tail renders")
                    .bounds()
                    .bottom(),
                window
                    .try_find("settings-body".to_string())
                    .expect("the body renders")
                    .bounds()
                    .bottom(),
                window
                    .try_find("settings-dialog-save".to_string())
                    .expect("footer renders")
                    .bounds()
                    .origin
                    .y,
            )
        });
        assert!(
            tail_bottom <= body_bottom,
            "the tail sits inside the scrolling body: tail {tail_bottom:?} vs body {body_bottom:?}"
        );
        assert!(
            body_bottom <= footer_top,
            "the section body clears the footer: body {body_bottom:?} vs footer {footer_top:?}"
        );
        cx.update(|window, cx| window.close_all_dialogs(cx));
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn bug15_log_tail_gate_uses_push_seq_past_ring_cap(cx: &mut TestAppContext) {
    use manyproxy_ui::health::LogTagIndex;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let cap = 5_000;
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let tags = LogTagIndex::default();
            for i in 0..cap {
                view.logs.store.push_raw(&format!("info line {i}"), &tags);
            }
            assert_eq!(view.logs.store.all().len(), cap, "ring pins at the cap");
            view.open_logs_dialog(window, cx);
        });
    });
    paint_dialog_open(cx);
    let seen = view.read_with(cx, |view, _| view.logs.seen);
    assert_eq!(
        seen,
        view.read_with(cx, |view, _| view.logs.store.push_seq()),
        "opening seeds the sequence"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs
                .store
                .push_raw("info one more", &LogTagIndex::default());
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.store.all().len()),
        cap,
        "length stays pinned at the cap (a length gate would freeze)"
    );
    assert!(
        view.read_with(cx, |view, _| view.logs.store.push_seq()) > seen,
        "push sequence still advances past the cap"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reload(window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.seen),
        seen + 1,
        "the sequence gate fires past the cap and re-seeds"
    );
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "the sheet reopens around the fresh tail"
    );
}

#[gpui_kit::test]
fn bug16_import_cancel_drops_late_batch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog(BUG16_LINK, "travel", None, window, cx);
        });
    });
    paint_dialog_open(cx);
    cx.update(|window, cx| {
        let started = view.update(cx, |view, cx| {
            view.commit_import_dialog(
                BUG16_LINK.to_string(),
                "travel".to_string(),
                None,
                None,
                None,
                window,
                cx,
            )
        });
        assert!(!started, "async parse leaves the dialog open");
    });
    assert!(
        view.read_with(cx, |view, _| {
            view.import_parse_in_flight
                && view
                    .import_form
                    .as_ref()
                    .is_some_and(|form| form.in_flight.get())
        }),
        "parse is in flight and the footer mirror follows"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.cancel_import_parse(cx));
        window.close_all_dialogs(cx);
    });
    // A dialog opened after Cancel must survive the late completion.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    let before = notification_count(cx);
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "cancelled batch writes nothing"
    );
    assert_eq!(notification_count(cx), before, "cancelled batch is silent");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "unrelated dialog survives the late completion"
    );
    assert!(
        !view.read_with(cx, |view, _| view.import_parse_in_flight),
        "in-flight flag cleared"
    );
}

#[gpui_kit::test]
fn bug16_import_completion_commits_when_not_cancelled(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        let started = view.update(cx, |view, cx| {
            view.commit_import_dialog(
                BUG16_LINK.to_string(),
                "travel".to_string(),
                None,
                None,
                None,
                window,
                cx,
            )
        });
        assert!(!started, "async parse leaves the dialog open");
    });
    assert!(
        view.read_with(cx, |view, _| view.import_parse_in_flight),
        "parse is in flight"
    );
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "uncancelled batch commits"
    );
    assert_eq!(notification_count(cx), 1, "one success toast");
    assert!(
        !view.read_with(cx, |view, _| view.import_parse_in_flight),
        "in-flight flag cleared"
    );
}

#[test]
fn bug25_log_level_tooltip_matches_exact_filter() {
    use manyproxy_ui::health::LogLevel;
    use manyproxy_ui::{log_level_matches, log_level_tooltip};
    assert_eq!(log_level_tooltip(None), "Show logs of every level");
    assert_eq!(
        log_level_tooltip(Some(LogLevel::Warn)),
        "Show only Warn records"
    );
    for label in ["All", "Debug", "Info", "Warn", "Error"] {
        let tip = log_level_tooltip(LogLevel::from_label(label)).to_lowercase();
        assert!(
            !tip.contains("and above"),
            "no cumulative promise for {label}: {tip}"
        );
    }
    assert!(
        !log_level_matches(LogLevel::Error, Some(LogLevel::Warn)),
        "the filter stays exact-match, matching the tooltip"
    );
}

#[test]
fn bug26_subscription_interval_failure_message_is_honest() {
    use manyproxy_ui::subscription_interval_failure_message;
    let added = subscription_interval_failure_message("added", "Nightly", "database is locked");
    assert!(added.contains("Nightly"), "{added}");
    assert!(added.contains("added"), "{added}");
    assert!(added.contains("refresh interval"), "{added}");
    assert!(added.contains("database is locked"), "{added}");
    let updated = subscription_interval_failure_message("updated", "Daily", "nope");
    assert!(
        updated.contains("updated") && updated.contains("Daily") && updated.contains("nope"),
        "{updated}"
    );
}

#[test]
fn bug28_bulk_start_failure_messages_name_skips() {
    use manyproxy_ui::{bulk_start_enable_flag_failed_message, bulk_start_unreadable_message};
    let unreadable = bulk_start_unreadable_message(&["10801".to_string(), "10802".to_string()]);
    assert!(unreadable.contains('2'), "{unreadable}");
    assert!(unreadable.contains("10801"), "{unreadable}");
    assert!(unreadable.contains("10802"), "{unreadable}");
    assert!(
        unreadable.to_lowercase().contains("could not read"),
        "{unreadable}"
    );

    let flag = bulk_start_enable_flag_failed_message(&["Alpha".to_string()]);
    assert!(flag.contains('1'), "{flag}");
    assert!(flag.contains("Alpha"), "{flag}");
    assert!(flag.to_lowercase().contains("started"), "{flag}");
}

/// T-05: the batch start names the row it refused — and only that row.
#[test]
fn t05_bulk_start_messages_name_the_refused_row() {
    use manyproxy_ui::{bulk_start_failed_message, bulk_start_refused_message};
    let failed = vec![(
        "Squatter".to_string(),
        "port unavailable: 127.0.0.1:10982 is already in use".to_string(),
    )];
    let message = bulk_start_failed_message(19, &failed);
    assert!(message.contains("19"), "{message}");
    assert!(message.contains("1 failed"), "{message}");
    assert!(message.contains("Squatter"), "{message}");
    assert!(message.contains("10982"), "{message}");
    assert!(
        !message.contains("Alpha") && !message.contains("Gamma"),
        "only the refused row is named: {message}"
    );

    let refused = bulk_start_refused_message(
        20,
        "port unavailable: 127.0.0.1:10982 is already in use (\"Squatter\" cannot start there; another process may hold it)",
        &failed,
    );
    assert!(refused.contains("20"), "{refused}");
    assert!(refused.contains("Squatter"), "{refused}");
    assert!(
        refused.to_lowercase().contains("nothing new started"),
        "the refusal never claims the rest carried on: {refused}"
    );
}

#[gpui_kit::test]
fn bug37_logs_sheet_keeps_typed_draft_across_control_change(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let (filter, _, _) = view.logs.inputs.clone().expect("inputs stored");
            filter.update(cx, |input, cx| {
                input.set_value("draft-only".to_string(), window, cx);
            });
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.filter.clone()),
        "",
        "typing alone never commits"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_log_tail(window, cx));
    });
    cx.run_until_parked();
    let typed = view.read_with(cx, |view, cx| {
        view.logs
            .inputs
            .as_ref()
            .expect("inputs stored")
            .0
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(typed, "draft-only", "typed draft survives the control swap");
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.filter.clone()),
        "",
        "commit-on-Apply is unchanged"
    );
}

#[gpui_kit::test]
fn t62_rule_set_download_requires_explicit_confirm(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let dir = std::env::temp_dir().join(format!(
        "manyproxy-t62confirm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock works")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("staging temp works");
    let _env = t62_rule_sets_env(&dir);
    assert_eq!(
        manyproxy_ui::store::rule_sets_dir(),
        dir,
        "the override routes staging into the test root"
    );

    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    // 1. An internal URL is refused wherever the gate runs — and it never
    //    reaches the confirm, so nothing can be fetched silently.
    assert!(
        !t62_begin(&view, cx, "t62-set", "http://127.0.0.1/ads.srs"),
        "the add flow reports the policy block instead of confirming"
    );
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "a blocked URL never opens the Download confirm"
    );
    assert!(
        t62_staged_rows(&view, cx).is_empty(),
        "no staged row from a refused gate"
    );
    assert!(
        !dir.join("t62-set.srs").exists(),
        "no staged file without a confirmed download"
    );

    // 2. Plaintext http is a warning, not a block: the gate proceeds to
    //    the confirm, exactly like subscription policy.
    assert!(
        t62_begin(&view, cx, "t62-set", "http://rule-sets.example.com/t62.srs"),
        "plaintext http warns but does not block"
    );
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the warn path still goes through the explicit confirm"
    );
    assert!(t62_staged_rows(&view, cx).is_empty(), "warn stages nothing");
    cx.update(|window, cx| window.close_all_dialogs(cx));

    // 3. A real https URL: the confirm offers itself BEFORE any fetch;
    //    closing it stages nothing, and a second identical request must
    //    re-offer its own confirmation (no batch, no reuse).
    let url = "https://rule-sets.example.com/t62.srs";
    assert!(
        t62_begin(&view, cx, "t62-set", url),
        "the gate passes and offers the confirm"
    );
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the confirm dialog is up"
    );
    assert!(
        t62_staged_rows(&view, cx).is_empty(),
        "staging starts only on the confirm, never at gate time"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "closing the confirm leaves nothing in flight"
    );
    assert!(t62_staged_rows(&view, cx).is_empty(), "cancel never stages");
    cx.run_until_parked();
    assert!(
        t62_begin(&view, cx, "t62-set", url),
        "the re-request re-offers the confirm"
    );
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the second confirm is its own explicit step"
    );
    assert!(
        t62_staged_rows(&view, cx).is_empty(),
        "two requests, still zero rows: confirms do not batch"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    // 4. The confirmed side. The confirm's Download click runs
    //    start_rule_set_download → fetch_rule_set_body (policy-proven by
    //    the parity test) → finish; the fetch is faked here so this test
    //    covers staging exactly, with fixed bytes.
    let staged_bytes = vec![0x52, 0x53, 0x31, 0x01, 0x02, 0x03, 0xFF, 0x04];
    let grant = manyproxy_ui::RuleSetDownloadGrant {
        name: "t62-set".to_string(),
        file_name: manyproxy_ui::store::rule_set_file_name("t62-set"),
        url: url.to_string(),
        dir: dir.clone(),
    };
    assert_eq!(grant.file_name, "t62-set.srs");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_rule_set_download(&grant, Ok(staged_bytes.clone()), window, cx)
        });
    });
    cx.run_until_parked();

    let staged = t62_staged_rows(&view, cx);
    assert_eq!(staged.len(), 1, "exactly the confirmed set stages");
    assert_eq!(staged[0].name, "t62-set");
    assert_eq!(staged[0].file_name, "t62-set.srs");
    assert_eq!(staged[0].url, url);
    assert_eq!(staged[0].bytes, staged_bytes.len() as u64);
    assert_eq!(
        std::fs::read(dir.join("t62-set.srs")).expect("staged file reads"),
        staged_bytes,
        "the staged bytes are the fetched bytes, verbatim"
    );

    // The staged catalog renders in the manager's Rule sets section with
    // per-row Re-Download/Remove actions.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    for id in [
        "rule-sets-section-title".to_string(),
        "rule-set-add".to_string(),
        format!("rule-set-redownload-{}", staged[0].id),
        format!("rule-set-remove-{}", staged[0].id),
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id.clone()).is_some()),
            "the manager renders {id}"
        );
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t62_rule_set_fetch_policy_parity_internal_blocked() {
    // The .srs download rides the same pipeline as subscriptions:
    // identical policy verdicts, byte-level parity of errors, and zero
    // requests for blocked targets.
    use gpui_kit::http_client::AsyncBody;

    // Direct internal URL and rebind-via-resolver both block pre-fetch.
    for (target, resolver) in [
        (
            "http://127.0.0.1/ads.srs",
            t48_public_resolver as manyproxy_ui::SubscriptionHostResolver,
        ),
        ("https://rebind.example.com/ads.srs", t48_internal_resolver),
    ] {
        let (client, fetched) = t48_recording_client(|_| {
            gpui_kit::http_client::http::Response::builder()
                .status(200)
                .body(AsyncBody::from("must never be reached"))
                .expect("response builds")
        });
        let subscription = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
            client.clone(),
            target,
            resolver,
        ))
        .expect_err("subscriptions block internal targets");
        let rule_set = futures::executor::block_on(manyproxy_ui::fetch_rule_set_body(
            client.clone(),
            target,
            resolver,
        ))
        .expect_err("rule sets block internal targets the same way");
        assert_eq!(
            rule_set, subscription,
            "one shared policy surface, byte-identical verdicts"
        );
        assert!(subscription.contains("internal"), "{subscription}");
        assert!(
            fetched.lock().expect("read works").is_empty(),
            "the gate runs before any request leaves"
        );
    }

    // Success parity: the rule-set path keeps raw bytes (a compiled .srs
    // is binary) where the subscription wrapper refuses non-UTF-8 — both
    // read the exact same body through the shared pipeline.
    let raw: Vec<u8> = vec![0x52, 0x53, 0x31, 0xFF, 0x00, 0x7F];
    let body = raw.clone();
    let (client, fetched) = t48_recording_client(move |_| {
        gpui_kit::http_client::http::Response::builder()
            .status(200)
            .body(AsyncBody::from(body.clone()))
            .expect("response builds")
    });
    let rule_set = futures::executor::block_on(manyproxy_ui::fetch_rule_set_body(
        client.clone(),
        "https://rule-sets.example.com/ads.srs",
        t48_public_resolver,
    ))
    .expect("binary body passes the rule-set fetch");
    assert_eq!(rule_set, raw, "raw bytes out, modulo nothing");
    let subscription = futures::executor::block_on(manyproxy_ui::fetch_subscription_body(
        client.clone(),
        "https://rule-sets.example.com/ads.srs",
        t48_public_resolver,
    ))
    .expect_err("the same body is not UTF-8");
    assert!(
        subscription.contains("not UTF-8"),
        "subscriptions keep their UTF-8 contract: {subscription}"
    );
    assert_eq!(
        *fetched.lock().expect("read works"),
        vec![
            "https://rule-sets.example.com/ads.srs".to_string(),
            "https://rule-sets.example.com/ads.srs".to_string(),
        ],
        "both flows request the confirmed URL exactly once"
    );
}

#[gpui_kit::test]
fn t65_failed_tone_distinct_from_unknown_in_row_text(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    use manyproxy_ui::health::{HealthKind, HealthSample, LatencyTone, TestOutcome};
    use manyproxy_ui::health_tone_color;
    use manyproxy_ui::store::now_unix;

    cx.update(gpui_kit::init);
    let (view, cx, failed, untested, gamma) =
        t65_mount_with_failed_probe(cx, ["10881", "10882", "10883"]);

    assert!(
        view.read_with(cx, |view, _| view.health_store.row_summary(&failed))
            .contains("dead"),
        "the failed probe keeps its cause text"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.health_store.health_tone(&failed)),
        LatencyTone::Failed,
        "a failed probe tones its row Failed"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.health_store.health_tone(&untested)),
        LatencyTone::Unknown,
        "an untested row keeps the neutral tone"
    );

    let delegate_tones = view.read_with(cx, |view, cx| {
        view.libraries.read(cx).delegate().health_tone.clone()
    });
    assert_eq!(
        delegate_tones.get(&failed),
        Some(&LatencyTone::Failed),
        "the row health map carries the Failed tone for the failed row"
    );
    assert_eq!(
        delegate_tones.get(&untested),
        None,
        "the untested row renders without a probe tone, so it stays neutral"
    );
    assert_painted(cx, &format!("row-health-{failed}"));
    assert_painted(cx, &format!("row-health-{untested}"));

    cx.update(|_, cx| {
        let theme = cx.theme();
        assert_ne!(
            health_tone_color(LatencyTone::Failed, theme),
            health_tone_color(LatencyTone::Unknown, theme),
            "the failed probe renders darker than the neutral unknown tone"
        );
        assert_eq!(
            health_tone_color(LatencyTone::Fast, theme),
            manyproxy_ui::success_text_color(theme),
            "the fast band resolves to the light-safe success tone"
        );
        assert_eq!(
            health_tone_color(LatencyTone::Slow, theme),
            manyproxy_ui::warning_text_color(theme),
            "the slow band resolves to the light-safe warning tone"
        );
        assert_eq!(
            health_tone_color(LatencyTone::Poor, theme),
            manyproxy_ui::danger_text_color(theme),
            "the poor band resolves to the light-safe danger tone"
        );
        let failed_color = health_tone_color(LatencyTone::Failed, theme);
        let poor_color = health_tone_color(LatencyTone::Poor, theme);
        assert!(
            contrast_ratio(failed_color, theme.background)
                > contrast_ratio(poor_color, theme.background),
            "the failed tone outranks the poor band: {failed_color:?} vs {poor_color:?}"
        );
    });

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: gamma.id.clone(),
                    at: now_unix(),
                    kind: HealthKind::SpeedTest,
                    latency_ms: Some(2700),
                    error: None,
                },
                success: None,
            });
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reload(window, cx));
    });
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.health_store.row_summary(&gamma.id)),
        "2.7s",
        "the speed summary text stays a duration"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.health_store.health_tone(&gamma.id)),
        LatencyTone::Unknown,
        "a speed-only summary tone stays neutral"
    );
    let (failed_row_tone, gamma_row_tone) = view.read_with(cx, |view, cx| {
        let delegate = view.libraries.read(cx).delegate();
        (
            delegate.health_tone.get(&failed).copied(),
            delegate.health_tone.get(&gamma.id).copied(),
        )
    });
    assert_eq!(failed_row_tone, Some(LatencyTone::Failed));
    assert_ne!(
        failed_row_tone, gamma_row_tone,
        "a failed probe tones the speed-summary-rendering row text distinctly"
    );
}

#[gpui_kit::test]
fn t65_failed_tone_distinct_in_health_section(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{sparkline_caption_tone, LatencyTone};
    use manyproxy_ui::health_tone_color;
    use manyproxy_ui::store::HealthHistoryEntry;

    cx.update(gpui_kit::init);
    let (view, cx, failed, untested, _gamma) =
        t65_mount_with_failed_probe(cx, ["10891", "10892", "10893"]);

    let (failed_tone, untested_tone) = view.read_with(cx, |view, _| {
        (
            view.health_store.health_tone(&failed),
            view.health_store.health_tone(&untested),
        )
    });
    assert_eq!(failed_tone, LatencyTone::Failed);
    assert_eq!(untested_tone, LatencyTone::Unknown);
    let (failed_detail, untested_detail) = view.read_with(cx, |view, _| {
        (
            view.health_store.detail_line(&failed),
            view.health_store.detail_line(&untested),
        )
    });
    assert!(
        failed_detail.contains("upstream refused"),
        "the failed detail line keeps its cause text: {failed_detail}"
    );
    assert!(
        untested_detail.contains("never tested"),
        "the untested detail line keeps its no-probe text: {untested_detail}"
    );
    cx.update(|_, cx| {
        use gpui_kit::component::ActiveTheme as _;
        assert_ne!(
            health_tone_color(failed_tone, cx.theme()),
            health_tone_color(untested_tone, cx.theme()),
            "failed and untested tones render distinctly in the Health section"
        );
    });

    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);

    assert_painted(cx, "config-health");
    assert_painted(cx, "config-history");
    assert_painted(cx, "config-history-few");

    let caption = |at: i64, latency: Option<u64>, error: Option<&str>| HealthHistoryEntry {
        config_id: "row".to_string(),
        at,
        kind: "url-test".to_string(),
        latency_ms: latency,
        error: error.map(|cause| cause.to_string()),
    };
    assert_eq!(
        sparkline_caption_tone(&[caption(1, Some(90), None)]),
        LatencyTone::Fast,
        "one fast probe tones its caption green"
    );
    assert_eq!(
        sparkline_caption_tone(&[caption(1, None, Some("upstream refused"))]),
        LatencyTone::Failed,
        "one failed probe tones its caption Failed, not neutral"
    );
    assert_eq!(sparkline_caption_tone(&[]), LatencyTone::Unknown);
}

/// T-27: every key ranks by its named source, both directions, and `none`
/// keeps the pre-feature order byte for byte.
#[test]
fn t77_sort_keys_order_correctly() {
    use manyproxy_ui::store::{LibrarySortKey as Key, SortDirection as Dir};
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        t77_row("1", "One", "vless", "g", 10801),
        t77_row("2", "Two", "vless", "g", 10802),
        t77_row("3", "Three", "vless", "g", 10803),
        t77_row("4", "Four", "vless", "g", 10804),
        t77_row("5", "Five", "vless", "g", 10805),
        t77_row("6", "Six", "vless", "g", 10806),
    ]);
    delegate.set_latency(t77_map_u64(&[("1", 120), ("2", 40), ("3", 80)]));
    delegate.set_data_traffic(t77_map_pair(&[
        ("1", 100, 10),
        ("2", 50, 90),
        ("3", 70, 5),
        ("4", 300, 1),
    ]));
    delegate.set_traffic(t77_map_pair(&[
        ("1", 1000, 10),
        ("2", 500, 900),
        ("3", 700, 5),
        ("4", 0, 0),
        ("5", 100, 100),
    ]));
    delegate.set_out_ips(t77_map_text(&[
        ("1", "203.0.113.9"),
        ("2", "198.51.100.7"),
        ("3", "203.0.113.10"),
    ]));

    let base = vec!["One", "Two", "Three", "Four", "Five", "Six"];
    assert_eq!(t77_names(&delegate), base, "the default order is the base");

    // `none` is the pre-feature order, unchanged by the direction.
    delegate.set_sort(t77_sort(Key::None, Dir::Descending));
    assert_eq!(t77_names(&delegate), base, "none ignores the direction");
    assert_eq!(
        delegate
            .rows
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>(),
        vec!["1", "2", "3", "4", "5", "6"],
        "sorting never reorders the store rows"
    );

    delegate.set_sort(t77_sort(Key::Latency, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Two", "Three", "One", "Four", "Five", "Six"],
        "latency asc: 40, 80, 120, then the untested rows by row id"
    );
    delegate.set_sort(t77_sort(Key::Latency, Dir::Descending));
    assert_eq!(
        t77_names(&delegate),
        vec!["One", "Three", "Two", "Four", "Five", "Six"],
        "latency desc reverses the known values, unknowns stay pinned last"
    );

    delegate.set_sort(t77_sort(Key::Download, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Two", "Three", "One", "Four", "Five", "Six"],
        "download reads the current session rx counter"
    );
    delegate.set_sort(t77_sort(Key::Upload, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Four", "Three", "One", "Two", "Five", "Six"],
        "upload reads the current session tx counter"
    );
    delegate.set_sort(t77_sort(Key::Download, Dir::Descending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Four", "One", "Three", "Two", "Five", "Six"],
        "download desc"
    );

    delegate.set_sort(t77_sort(Key::TrafficTotal, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Four", "Five", "Three", "One", "Two", "Six"],
        "traffic total is the accumulated rx+tx of the counting window"
    );
    delegate.set_sort(t77_sort(Key::TrafficDown, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Four", "Five", "Two", "Three", "One", "Six"],
        "traffic down is the accumulated rx"
    );
    delegate.set_sort(t77_sort(Key::TrafficUp, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Four", "Three", "One", "Five", "Two", "Six"],
        "traffic up is the accumulated tx"
    );
    delegate.set_sort(t77_sort(Key::TrafficTotal, Dir::Descending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Two", "One", "Three", "Five", "Four", "Six"],
        "traffic total desc"
    );

    delegate.set_sort(t77_sort(Key::OutIp, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Two", "Three", "One", "Four", "Five", "Six"],
        "out-IP compares the resolved text lexically, 203.0.113.10 < 203.0.113.9"
    );
    delegate.set_sort(t77_sort(Key::OutIp, Dir::Descending));
    assert_eq!(
        t77_names(&delegate),
        vec!["One", "Three", "Two", "Four", "Five", "Six"],
        "out-IP desc reverses the resolved text, unresolved rows stay last"
    );
}

/// T-27: unknown/missing values are pinned last deterministically — row id
/// numeric as the secondary — and repeated ticks with the same maps never
/// reshuffle the tail.
#[test]
fn t77_unknown_values_sort_deterministically_last() {
    use manyproxy_ui::store::{LibrarySortKey as Key, SortDirection as Dir};
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        t77_row("100", "Hundred", "vless", "g", 10801),
        t77_row("10", "Ten", "vless", "g", 10802),
        t77_row("9", "Nine", "vless", "g", 10803),
    ]);
    delegate.set_latency(t77_map_u64(&[("10", 50)]));
    delegate.set_sort(t77_sort(Key::Latency, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Ten", "Nine", "Hundred"],
        "unknown values sort by numeric row id (9 before 100), not lexically"
    );

    // A second tick carrying the same mirrors must not move anything.
    let first = t77_names(&delegate);
    for _ in 0..3 {
        delegate.set_latency(t77_map_u64(&[("10", 50)]));
        assert_eq!(t77_names(&delegate), first, "no flicker between ticks");
    }

    // A dead sample is an absent value: it joins the pinned tail.
    delegate.set_latency(t77_map_u64(&[("10", 50), ("100", 70)]));
    assert_eq!(
        t77_names(&delegate),
        vec!["Ten", "Hundred", "Nine"],
        "the row that gained a sample leaves the pinned tail"
    );

    // Unresolved out-IP rows are last too, again by numeric row id.
    delegate.set_out_ips(t77_map_text(&[("100", "203.0.113.7")]));
    delegate.set_sort(t77_sort(Key::OutIp, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Hundred", "Nine", "Ten"],
        "only the resolved row ranks; unresolved ones keep the row-id tail"
    );
    let tail = t77_names(&delegate);
    for _ in 0..3 {
        assert_eq!(t77_names(&delegate), tail, "out-IP tail is stable too");
    }
}

/// T-27: the retired toggle became the `type_security` key. Its ordering is
/// `compare_type_security` exactly — protocol asc, security rank desc, name
/// — and the direction toggle does not move it.
#[test]
fn t77_type_security_key_matches_old_toggle() {
    use manyproxy_ui::store::{
        compare_type_security as compare_old, LibrarySortKey as Key, SortDirection as Dir,
    };
    let rows = vec![
        t77_row("1", "Trojan", "trojan", "g", 10801),
        t77_row("2", "VlessTls", "vless", "g", 10802),
        t77_row("3", "VlessReality", "vless", "g", 10803),
        t77_row("4", "Zeta", "vmess", "g", 10804),
        t77_row("5", "Alpha", "vmess", "g", 10805),
        t77_row("6", "Plain", "shadowsocks", "g", 10806),
    ];
    let mut with_security = rows.clone();
    with_security[1].params = r#"{"security":"tls"}"#.to_string().into();
    with_security[2].params = r#"{"security":"reality"}"#.to_string().into();

    let mut sorted_rows = with_security.clone();
    sorted_rows.sort_by(compare_old);
    let old_toggle_order: Vec<String> = sorted_rows.iter().map(|row| row.name.clone()).collect();
    assert_eq!(
        old_toggle_order,
        vec![
            "Plain",
            "Trojan",
            "VlessReality",
            "VlessTls",
            "Alpha",
            "Zeta"
        ],
        "the retired toggle: protocol asc, security rank desc, name asc"
    );

    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(with_security.clone());
    delegate.set_sort(t77_sort(Key::TypeSecurity, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        old_toggle_order,
        "type_security asc == the retired toggle"
    );
    delegate.set_sort(t77_sort(Key::TypeSecurity, Dir::Descending));
    assert_eq!(
        t77_names(&delegate),
        old_toggle_order,
        "the key keeps the toggle's fixed direction"
    );
    assert_eq!(
        delegate
            .rows
            .iter()
            .map(|row| row.name.clone())
            .collect::<Vec<_>>(),
        vec![
            "Trojan",
            "VlessTls",
            "VlessReality",
            "Zeta",
            "Alpha",
            "Plain"
        ],
        "the store order survives the sort"
    );
}

/// T-27: sorting runs after the port predicate and the facets — it reorders
/// the kept rows and never reintroduces a filtered-out one.
#[test]
fn t77_sort_composes_with_facets_and_port_filter() {
    use manyproxy_ui::store::{LibrarySortKey as Key, SortDirection as Dir};
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        t77_row("1", "One", "vless", "travel", 10801),
        t77_row("2", "Two", "vless", "travel", 10802),
        t77_row("3", "Three", "vmess", "travel", 10803),
        t77_row("4", "Four", "vless", "team", 10804),
        t77_row("5", "Five", "vless", "travel", 19001),
    ]);
    delegate.set_latency(t77_map_u64(&[
        ("1", 90),
        ("2", 20),
        ("3", 30),
        ("4", 10),
        ("5", 40),
    ]));
    delegate.set_sort(t77_sort(Key::Latency, Dir::Ascending));
    assert_eq!(
        t77_names(&delegate),
        vec!["Four", "Two", "Three", "Five", "One"],
        "no filter: the whole library ranks by latency"
    );

    let mut query = manyproxy_ui::FilterQuery::default();
    query.set_search("port=10801:10803");
    assert_eq!(
        t77_names_q(&delegate, &query),
        vec!["Two", "Three", "One"],
        "the port predicate keeps its rows and the sort ranks them"
    );

    query.protocols.insert("vless".to_string());
    assert_eq!(
        t77_names_q(&delegate, &query),
        vec!["Two", "One"],
        "the protocol facet ANDs with the port predicate before the sort"
    );

    // K11: the status facet narrows the survivors — Three serves dead, One
    // and Two serve live.
    delegate.set_running(vec!["1".to_string(), "2".to_string(), "3".to_string()]);
    delegate.set_health(
        [("1", "20ms"), ("2", "30ms"), ("3", "dead")]
            .into_iter()
            .map(|(id, verdict)| (id.to_string(), verdict.to_string()))
            .collect(),
    );
    query.statuses.insert(manyproxy_ui::StatusFacet::Running);
    assert_eq!(
        t77_names_q(&delegate, &query),
        vec!["Two", "One"],
        "status facet removes Three, latency order holds"
    );

    delegate.set_sort(t77_sort(Key::Latency, Dir::Descending));
    assert_eq!(
        t77_names_q(&delegate, &query),
        vec!["One", "Two"],
        "direction change re-ranks only what the filters kept"
    );

    query = manyproxy_ui::FilterQuery::default();
    assert_eq!(
        t77_names_q(&delegate, &query),
        vec!["One", "Five", "Three", "Two", "Four"],
        "clearing the filters restores the full sorted set"
    );
}

/// R11: every header opens the same Explorer menu. The visibility rows follow
/// the layout (legacy seed included), and More reaches the columns sheet.
#[gpui_kit::test]
fn t77_header_menu_renders_pickers(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_meta(Store::META_LIBRARY_COLUMNS, "upstream,health,traffic")
        .expect("legacy column choice writes");
    store
        .set_library_sort(manyproxy_ui::store::LibrarySort::new(
            manyproxy_ui::store::LibrarySortKey::TrafficTotal,
            manyproxy_ui::store::SortDirection::Ascending,
        ))
        .expect("sort choice writes");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    // F1: the open drawer occludes the columns it covers, and this test is
    // about the header menu, so keep the inspector closed at full table width.
    view.update(cx, |view, cx| {
        view.inspector_visible = false;
        cx.notify();
    });
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().sort),
        manyproxy_ui::store::LibrarySort::new(
            manyproxy_ui::store::LibrarySortKey::TrafficTotal,
            manyproxy_ui::store::SortDirection::Ascending
        ),
        "launch reads the persisted sort beside the legacy column seed"
    );

    cx.update(|window, _| {
        assert!(
            window.try_find("sort-header-health".to_string()).is_some(),
            "the health header paints when the column is visible"
        );
        assert!(
            window.try_find("sort-header-traffic".to_string()).is_some(),
            "the traffic header paints when the column is visible"
        );
        assert!(
            window.try_find("sort-header-group".to_string()).is_none(),
            "hidden columns have no header"
        );
    });

    cx.update(|window, cx| {
        window.right_click("sort-header-traffic".to_string(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "columns-menu-size-this",
        "columns-menu-size-all",
        "columns-menu-name",
        "columns-menu-endpoint",
        "columns-menu-health",
        "columns-menu-group",
        "columns-menu-protocol",
        "columns-menu-upstream",
        "columns-menu-traffic",
        "columns-menu-more",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} renders in every header menu"
        );
    }
    assert!(
        cx.update(|window, _| window
            .try_find("sort-header-traffic-none".to_string())
            .is_none()),
        "the retired per-column sort picker is gone"
    );

    // The hidden group can be restored from this menu and the header returns.
    cx.update(|window, cx| window.click("columns-menu-group".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Group),
        "the menu shows the group again"
    );
    assert_painted(cx, "sort-header-group");
}

/// T-27: the header itself is the mouse and keyboard route to sorting —
/// click picks the column's natural key, click again reverses it, and a
/// focused header activates on Enter/Space like any button.
#[gpui_kit::test]
fn t77_header_click_and_keyboard_sort(cx: &mut TestAppContext) {
    use manyproxy_ui::store::{LibrarySort, LibrarySortKey, SortDirection};

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_meta(Store::META_LIBRARY_COLUMNS, "upstream,health,traffic")
        .expect("legacy column choice writes");
    store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.inspector_visible = false;
        cx.notify();
    });
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    let sort = |view: &Entity<AppView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            (
                view.libraries.read(cx).delegate().sort,
                view.store.library_sort(),
            )
        })
    };
    assert_eq!(
        sort(&view, cx).0,
        LibrarySort::default(),
        "the library starts unsorted"
    );

    cx.update(|window, cx| window.click("sort-header-health", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        sort(&view, cx),
        (
            LibrarySort::new(LibrarySortKey::Latency, SortDirection::Ascending),
            LibrarySort::new(LibrarySortKey::Latency, SortDirection::Ascending),
        ),
        "a click on a cold header picks its natural key and persists it"
    );

    cx.update(|window, cx| window.click("sort-header-health", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        sort(&view, cx).0,
        LibrarySort::new(LibrarySortKey::Latency, SortDirection::Descending),
        "clicking the active header reverses it"
    );

    // The click focused the header, so Enter and Space are the keyboard route.
    for (key, direction) in [
        ("enter", SortDirection::Ascending),
        ("space", SortDirection::Descending),
    ] {
        let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.dispatch_event(
                gpui_kit::PlatformInput::KeyUp(gpui_kit::KeyUpEvent { keystroke }),
                cx,
            );
        });
        cx.run_until_parked();
        paint_frame(cx);
        assert_eq!(
            sort(&view, cx).0,
            LibrarySort::new(LibrarySortKey::Latency, direction),
            "{key} activates the focused header"
        );
    }
}

#[test]
fn t78_qr_content_equals_proxy_urls_lines() {
    use manyproxy_ui::{proxy_urls, qr_grid, share_qr_payloads};

    for (bind_ip, port) in [
        ("127.0.0.1", 10801u16),
        ("0.0.0.0", 10899),
        ("192.168.1.20", 18081),
        ("::1", 10900),
        ("2001:db8::1", 18082),
        ("a-rather-long-listener-hostname.internal.lan", 10805),
    ] {
        let row = t78_row(bind_ip, port);
        let (socks, http) = proxy_urls(bind_ip, port);
        let payloads = share_qr_payloads(&row);

        assert_eq!(payloads[0].0, "socks", "socks comes first");
        assert_eq!(payloads[1].0, "http", "http comes second");
        assert_eq!(payloads[0].1, socks, "socks line is proxy_urls' line");
        assert_eq!(payloads[1].1, http, "http line is proxy_urls' line");

        for (label, payload) in &payloads {
            let scheme = if *label == "socks" {
                "socks5://"
            } else {
                "http://"
            };
            assert!(
                payload.starts_with(scheme),
                "{label} keeps its scheme: {payload}"
            );
            assert!(
                !payload.starts_with("manyproxy://"),
                "no share link ever reaches a QR: {payload}"
            );
            assert!(!payload.contains('@'), "no credentials in a QR: {payload}");
            assert!(
                !payload.contains('?') && !payload.contains('#'),
                "no params or fragment in a QR: {payload}"
            );
            assert!(
                payload.ends_with(&format!(":{port}")),
                "{label} carries the listener port: {payload}"
            );
            assert!(
                qr_grid(payload).is_ok(),
                "{label} payload encodes at any listener length: {payload}"
            );
        }

        assert_ne!(
            qr_grid(&payloads[0].1).expect("socks encodes"),
            qr_grid(&payloads[1].1).expect("http encodes"),
            "one grid per scheme, never one two-line code"
        );
    }

    // Wildcard binds are transcribed literally, exactly like the dialog text.
    let wildcard = share_qr_payloads(&t78_row("0.0.0.0", 10801));
    assert_eq!(wildcard[0].1, "socks5://0.0.0.0:10801");
    assert_eq!(wildcard[1].1, "http://0.0.0.0:10801");
    // IPv6 keeps its brackets, as proxy_urls writes them.
    let v6 = share_qr_payloads(&t78_row("::1", 10801));
    assert_eq!(v6[0].1, "socks5://[::1]:10801");
    assert_eq!(v6[1].1, "http://[::1]:10801");
}

#[test]
fn t78_qr_grid_known_payload_shape() {
    use manyproxy_ui::{qr_grid, SHARE_QR_QUIET_ZONE};

    let grid = qr_grid(T78_KNOWN_PAYLOAD).expect("known payload encodes");
    assert_eq!(
        grid.side, 33,
        "version 2 at EC M: 25 core modules + a 4-module quiet zone per side"
    );
    assert_eq!(
        grid.fingerprint(),
        T78_KNOWN_FINGERPRINT,
        "known payload keeps its exact module grid"
    );

    let side = grid.side;
    let quiet = SHARE_QR_QUIET_ZONE;
    for i in 0..side {
        for ring in 0..quiet {
            assert!(!grid.is_dark(i, ring), "top quiet row {ring} is light");
            assert!(
                !grid.is_dark(i, side - 1 - ring),
                "bottom quiet row {ring} is light"
            );
            assert!(!grid.is_dark(ring, i), "left quiet column {ring} is light");
            assert!(
                !grid.is_dark(side - 1 - ring, i),
                "right quiet column {ring} is light"
            );
        }
    }

    // Finder patterns: 7x7 dark ring, light middle ring, 3x3 dark core.
    for (x, y) in [
        (quiet, quiet),
        (side - 1 - quiet - 6, quiet),
        (quiet, side - 1 - quiet - 6),
    ] {
        assert!(grid.is_dark(x, y), "finder corner ({x},{y}) dark");
        assert!(grid.is_dark(x + 6, y), "finder corner ({x},{y}) dark");
        assert!(grid.is_dark(x, y + 6), "finder corner ({x},{y}) dark");
        assert!(grid.is_dark(x + 6, y + 6), "finder corner ({x},{y}) dark");
        assert!(!grid.is_dark(x + 1, y + 1), "finder ring ({x},{y}) light");
        assert!(grid.is_dark(x + 2, y + 2), "finder core ({x},{y}) dark");
        assert!(grid.is_dark(x + 4, y + 4), "finder core ({x},{y}) dark");
        assert!(!grid.is_dark(x + 5, y + 5), "finder ring ({x},{y}) light");
    }

    // A longer listener host grows the version instead of failing.
    let long = format!("socks5://{}.internal.lan:10801", "a".repeat(150));
    let grown = qr_grid(&long).expect("long listener URL still encodes");
    assert!(
        grown.side > side,
        "version auto-grows for longer payloads: {}",
        grown.side
    );

    // The quiet zone is part of the grid, so it is never a separate, clippable band.
    assert_eq!(
        grid.dark.len(),
        side * side,
        "every module including the quiet zone lives in the grid"
    );
}

#[gpui_kit::test]
fn t78_share_dialog_renders_two_qrs(cx: &mut TestAppContext) {
    use manyproxy_ui::{QrPlate, SHARE_QR_PLATE_PX};

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t78_new("127.0.0.1", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_share_for_row(&row.id, window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);

    let region = cx.update(|window, _| window.find("share-qr-region").bounds());
    assert!(region.size.width.as_f32() > 0., "the QR region is laid out");
    let socks = cx.update(|window, _| window.find("share-qr-socks").bounds());
    let http = cx.update(|window, _| window.find("share-qr-http").bounds());
    for (label, plate) in [("socks", socks), ("http", http)] {
        assert!(
            plate.size.width.as_f32() >= SHARE_QR_PLATE_PX,
            "{label} plate keeps its minimum size: {}",
            plate.size.width.as_f32()
        );
        assert!(
            (plate.size.width.as_f32() - plate.size.height.as_f32()).abs() < 0.5,
            "{label} plate stays square"
        );
    }
    assert_eq!(
        socks.origin.y, http.origin.y,
        "both QRs sit on one row, side by side"
    );
    assert!(
        socks.origin.x < http.origin.x,
        "socks is left of http: {:?} vs {:?}",
        socks.origin.x,
        http.origin.x
    );
    assert!(
        cx.update(|window, _| window
            .try_find("share-qr-socks-label".to_string())
            .is_some()),
        "the socks plate is labeled"
    );
    assert!(
        cx.update(|window, _| window.try_find("share-qr-http-label".to_string()).is_some()),
        "the http plate is labeled"
    );
    assert!(
        cx.update(|window, _| window.try_find("share-link-text".to_string()).is_some()),
        "the link text stays"
    );

    let warning = cx.update(|window, _| window.find("share-warning").bounds());
    let link = cx.update(|window, _| window.find("share-link-text").bounds());
    assert!(
        warning.origin.y < socks.origin.y,
        "the credential warning stays above the QRs"
    );
    assert!(
        link.origin.y < socks.origin.y,
        "the QRs sit below the link text"
    );

    // Resize discipline: the grid (quiet zone included) always fits its plate, the
    // cell never shrinks as the plate grows, and cells stay whole pixels above 1:1.
    let mut last_cell = 0.0_f32;
    for edge in [
        0.,
        12.,
        25.,
        40.,
        96.,
        120.,
        SHARE_QR_PLATE_PX,
        160.,
        240.,
        400.,
    ] {
        let plate = QrPlate::fit(edge, edge, socks_qr_side());
        assert!(
            plate.plate_side() <= edge + 0.001,
            "the whole grid fits at {edge}px: {}",
            plate.plate_side()
        );
        assert!(plate.x >= 0. && plate.y >= 0., "grid origin stays inside");
        assert!(
            plate.cell >= last_cell,
            "cell size is monotone on resize: {} then {}",
            last_cell,
            plate.cell
        );
        if edge >= plate.side as f32 {
            assert_eq!(
                plate.cell,
                plate.cell.floor(),
                "whole-pixel cells keep modules crisp"
            );
        }
        last_cell = plate.cell;
    }
    assert!(
        (SHARE_QR_PLATE_PX / socks_qr_side() as f32) >= 3.,
        "the plate keeps at least a 3 px module for the known grid"
    );
}

#[test]
fn t78_no_scanner_refusal_untouched() {
    use manyproxy_ui::import::reject_file_bytes;

    let refusal = "image file (paste the link text instead — this build has no image/QR scan)";
    let images: [&[u8]; 6] = [
        b"\x89PNG\r\n\x1a\n",
        b"\xff\xd8\xff\xe0",
        b"GIF87a",
        b"GIF89a",
        b"BM",
        b"RIFF\x00\x00\x00\x00WEBPVP8 ",
    ];
    for bytes in images {
        assert_eq!(
            reject_file_bytes(bytes),
            Some(refusal),
            "the typed scan refusal stays byte-identical"
        );
    }
    assert_eq!(
        reject_file_bytes(b"vless://11111111-2222-4333-8444-555555555555@a.example.com:443"),
        None,
        "text intake still works"
    );

    // Encode-only: the wrapper has no decode/scan/camera entry point, and a QR of a
    // share link is out of scope by decision.
    assert!(manyproxy_ui::qr_grid("socks5://127.0.0.1:10801").is_ok());
    assert!(
        manyproxy_ui::qr_grid(&"x".repeat(3_000)).is_err(),
        "oversized payloads fail closed instead of panicking"
    );
}

#[test]
fn t79_top9_other_split_math() {
    use manyproxy_ui::{traffic_top9_split, TRAFFIC_TOP_EXITS};

    let exits: Vec<ExitTraffic> = (0..12u64)
        .map(|index| {
            let rank = 12 - index;
            t79_exit(
                &format!("e{index:02}"),
                &format!("Exit {index:02}"),
                rank * 1_000,
                rank * 10,
            )
        })
        .collect();

    let split = traffic_top9_split(&exits);
    assert_eq!(TRAFFIC_TOP_EXITS, 9);
    assert_eq!(split.top.len(), 9, "exactly nine rows stay listed");
    assert_eq!(split.other_exits, 3, "everything below rank 9 rolls up");
    assert_eq!(split.other_rx, 6_000, "3 + 2 + 1 KB of the tail");
    assert_eq!(split.other_tx, 60);
    for pair in split.top.windows(2) {
        assert!(
            pair[0].total() >= pair[1].total(),
            "listed rows stay heaviest-first"
        );
    }
    let listed_rx: u64 = split.top.iter().map(|exit| exit.rx_bytes).sum();
    let all_rx: u64 = exits.iter().map(|exit| exit.rx_bytes).sum();
    assert_eq!(listed_rx + split.other_rx, all_rx, "top 9 + other = all");
    assert_eq!(
        split.top.iter().map(|exit| exit.tx_bytes).sum::<u64>() + split.other_tx,
        exits.iter().map(|exit| exit.tx_bytes).sum::<u64>()
    );

    // Ties break on row_id, then name/bind — never on insertion order.
    let tied = vec![
        t79_exit("b", "Beta", 100, 5),
        t79_exit("a", "Alpha", 100, 5),
        t79_exit("c", "Gamma", 100, 5),
    ];
    let tied_order = traffic_top9_split(&tied);
    let order: Vec<&str> = tied_order
        .top
        .iter()
        .map(|exit| exit.row_id.as_str())
        .collect();
    assert_eq!(order, ["a", "b", "c"]);
    let mut reversed = tied.clone();
    reversed.reverse();
    assert_eq!(
        traffic_top9_split(&reversed),
        traffic_top9_split(&tied),
        "same data paints the same order"
    );

    // Nine or fewer: no Other line at all.
    let nine = traffic_top9_split(&exits[..9]);
    assert_eq!(nine.top.len(), 9);
    assert_eq!((nine.other_exits, nine.other_rx, nine.other_tx), (0, 0, 0));
    assert_eq!(nine.other_line(), None);
}

#[test]
fn t79_periods_compute_independently() {
    use manyproxy_ui::store::TrafficPeriod;
    use manyproxy_ui::traffic_top9_split;

    let dataset = |period: TrafficPeriod, rows: u64| -> Vec<ExitTraffic> {
        let scale = match period {
            TrafficPeriod::Day => 1_000u64,
            TrafficPeriod::Week => 2_000,
            TrafficPeriod::Month => 3_000,
            TrafficPeriod::Quarter => 4_000,
        };
        (0..rows)
            .map(|index| {
                t79_exit(
                    &format!("{}-{index:02}", period.label()),
                    &format!("{} exit {index:02}", period.label()),
                    (rows - index) * scale,
                    scale,
                )
            })
            .collect()
    };

    let mut others = Vec::new();
    for period in TrafficPeriod::ALL {
        let exits = dataset(period, 11);
        let first = traffic_top9_split(&exits);
        let second = traffic_top9_split(&exits);
        assert_eq!(first, second, "{} recomputes identically", period.label());
        assert_eq!(first.top.len(), 9, "{} keeps nine rows", period.label());
        assert_eq!(first.other_exits, 2, "{} rolls up two", period.label());
        let scale = match period {
            TrafficPeriod::Day => 1_000u64,
            TrafficPeriod::Week => 2_000,
            TrafficPeriod::Month => 3_000,
            TrafficPeriod::Quarter => 4_000,
        };
        assert_eq!(first.other_rx, 3 * scale, "{}", period.label());
        assert_eq!(
            first.other_tx,
            2 * scale,
            "two rolled-up rows carry one scale of tx each ({})",
            period.label()
        );
        let all_rx: u64 = exits.iter().map(|exit| exit.rx_bytes).sum();
        assert_eq!(
            first.top.iter().map(|exit| exit.rx_bytes).sum::<u64>() + first.other_rx,
            all_rx,
            "{} accounts every byte",
            period.label()
        );
        others.push(first.other_rx);
    }
    let mut distinct = others.clone();
    distinct.sort_unstable();
    distinct.dedup();
    // Honesty note: `Store::traffic_period_summary` ignores `_period` and
    // traffic lives in all-time `proxy_configs` columns, so the four
    // periods receive identical data today (the dialog tooltip says
    // "all-time totals throughout"). The distinct scales here are fixture
    // inputs; the split itself is a pure per-render fn over whatever rows
    // each period hands it.
    assert_eq!(distinct.len(), 4, "each period splits its own data");

    // The gate is per period too: six Day exits need no Other line while
    // Quarter's eleven do — and Day recomputed later is unchanged.
    let day_small = traffic_top9_split(&dataset(TrafficPeriod::Day, 6));
    assert_eq!(day_small.top.len(), 6);
    assert_eq!(day_small.other_line(), None);
    assert!(
        traffic_top9_split(&dataset(TrafficPeriod::Quarter, 11))
            .other_line()
            .is_some(),
        "Quarter still rolls up"
    );
    assert_eq!(
        day_small,
        traffic_top9_split(&dataset(TrafficPeriod::Day, 6)),
        "no split state survives a render"
    );
}

#[test]
fn t80_sampler_delta_calculations() {
    use manyproxy_ui::cpu_percent_of_one_core;
    use std::time::Duration;

    let window = Duration::from_secs(5);
    // 5 s of CPU time inside a 5 s window = one full core.
    let full_core = 5 * 10_000_000;
    assert_eq!(cpu_percent_of_one_core(0, full_core, window), Some(100.0));
    assert_eq!(
        cpu_percent_of_one_core(7, 7 + full_core / 2, window),
        Some(50.0)
    );
    assert_eq!(
        cpu_percent_of_one_core(7, 7 + full_core * 2, window),
        Some(200.0),
        "two busy threads are 200% of one core"
    );
    assert_eq!(
        cpu_percent_of_one_core(1_000, 1_000, window),
        Some(0.0),
        "an idle process reads zero, not absent"
    );
    assert_eq!(
        cpu_percent_of_one_core(5_000, 4_000, window),
        None,
        "a backwards counter is no sample at all"
    );
    assert_eq!(
        cpu_percent_of_one_core(0, full_core, Duration::ZERO),
        None,
        "an empty window has no rate"
    );

    let odd = cpu_percent_of_one_core(0, 1_234_567, window).expect("odd delta samples");
    assert!(
        (odd - 2.469_135_8).abs() < 0.001,
        "delta / window, got {odd}"
    );
    // Same CPU delta over half the wall time doubles the rate: the
    // normalization is by elapsed wall time, not by tick count.
    assert_eq!(
        cpu_percent_of_one_core(0, full_core / 2, Duration::from_millis(2_500)),
        Some(100.0)
    );
}

#[test]
fn t80_ring_evicts_dead_shards() {
    use manyproxy_ui::{
        fold_shard_samples, push_shard_sample, SampleRead, ShardRing, SHARD_SAMPLE_RING,
    };
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    let mut rings: BTreeMap<String, ShardRing> = BTreeMap::new();
    let mut baselines = BTreeMap::new();
    let t0 = Instant::now();
    let read = sampled(12_000_000, 64 * 1_048_576);
    let live = [shard_pair("stock", 41), shard_pair("fork", 42)];

    for step in 0..3u64 {
        let at = t0 + Duration::from_secs(5 * step);
        fold_shard_samples(&mut rings, &mut baselines, &live, &read, at);
    }
    assert_eq!(
        rings["stock"].cpu.len(),
        2,
        "the first tick only primes the baseline"
    );
    assert_eq!(rings["stock"].ram.len(), 2);
    assert_eq!(rings["fork"].cpu.len(), 2);

    // fork stops: its label leaves the cohort, so its ring goes with it.
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &[shard_pair("stock", 41)],
        &read,
        t0 + Duration::from_secs(20),
    );
    assert!(!rings.contains_key("fork"), "dead shard evicted");
    assert!(!baselines.contains_key("fork"), "no baseline left behind");
    assert!(rings.contains_key("stock"), "live shard keeps its history");

    // A rename is the same eviction: the old label disappears, the new one
    // starts with an empty ring.
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &[shard_pair("stock", 41), shard_pair("extra-9", 43)],
        &read,
        t0 + Duration::from_secs(25),
    );
    assert!(rings["extra-9"].cpu.is_empty(), "new label starts clean");

    // A pid the sampler reports gone drops silently.
    let gone = |_pid: u32| SampleRead::Gone;
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &[shard_pair("extra-9", 43)],
        &gone,
        t0 + Duration::from_secs(30),
    );
    assert!(!rings.contains_key("extra-9"), "gone pid drops silently");

    // A refused handle is named, never plotted.
    let denied = |_pid: u32| SampleRead::Denied;
    let refused = fold_shard_samples(
        &mut rings,
        &mut baselines,
        &[shard_pair("xray", 44)],
        &denied,
        t0 + Duration::from_secs(35),
    );
    assert!(refused.contains("xray"), "permission failure is named");
    assert!(!rings.contains_key("xray"), "nothing is plotted for it");

    // The ring caps at 60: the oldest samples fall off the front.
    let mut ring = ShardRing::default();
    for step in 0..(SHARD_SAMPLE_RING as i32 + 10) {
        push_shard_sample(&mut ring.cpu, step as f32);
        push_shard_sample(&mut ring.ram, 1.0);
    }
    assert_eq!(ring.cpu.len(), SHARD_SAMPLE_RING);
    assert_eq!(ring.ram.len(), SHARD_SAMPLE_RING);
    assert_eq!(ring.cpu.first().copied(), Some(10.0), "oldest ten evicted");
    assert_eq!(
        ring.cpu.last().copied(),
        Some((SHARD_SAMPLE_RING + 9) as f32)
    );
}

#[test]
fn t80_no_sampling_race_after_restart() {
    use manyproxy_ui::{fold_shard_samples, ShardRing};
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    let mut rings: BTreeMap<String, ShardRing> = BTreeMap::new();
    let mut baselines = BTreeMap::new();
    let t0 = Instant::now();
    let first = [shard_pair("stock", 41)];

    // Generation one: prime at 1 s of CPU, then +1 s over a 5 s window.
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &first,
        &sampled(10_000_000, 100 * 1_048_576),
        t0,
    );
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &first,
        &sampled(20_000_000, 101 * 1_048_576),
        t0 + Duration::from_secs(5),
    );
    assert_eq!(rings["stock"].cpu, vec![20.0]);
    assert_eq!(rings["stock"].ram, vec![101.0]);

    // The shard restarts under a new pid whose counter starts near zero.
    // Its CPU time is far below the old baseline: reusing that baseline
    // would underflow or fabricate a bogus rate.
    let second = [shard_pair("stock", 55)];
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &second,
        &sampled(500_000, 30 * 1_048_576),
        t0 + Duration::from_secs(10),
    );
    assert!(
        rings["stock"].cpu.is_empty() && rings["stock"].ram.is_empty(),
        "respawn starts a fresh ring, got {:?}",
        rings["stock"]
    );

    // The next tick deltas against the new generation only.
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &second,
        &sampled(3_000_000, 31 * 1_048_576),
        t0 + Duration::from_secs(15),
    );
    assert_eq!(rings["stock"].cpu, vec![5.0], "2.5 s CPU / 5 s window");
    assert_eq!(rings["stock"].ram, vec![31.0]);

    // A backwards counter for the same pid can never push a sample, and
    // the baseline re-primes so the next tick stays sane.
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &second,
        &sampled(100, 31 * 1_048_576),
        t0 + Duration::from_secs(20),
    );
    assert_eq!(rings["stock"].cpu, vec![5.0], "no negative rate");
    fold_shard_samples(
        &mut rings,
        &mut baselines,
        &second,
        &sampled(2_500_100, 32 * 1_048_576),
        t0 + Duration::from_secs(25),
    );
    assert_eq!(rings["stock"].cpu, vec![5.0, 5.0]);
    assert_eq!(rings["stock"].ram, vec![31.0, 32.0]);
}

/// T-04: the close behaviour is a persisted setting. A fresh store reads the
/// default (`Tray` — the window closes and the proxies keep running), and the
/// choice survives a round trip through the meta table.
#[test]
fn t04_close_action_round_trips_and_defaults_to_tray() {
    use manyproxy_ui::store::{CloseAction, Store};
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    assert_eq!(
        store.close_action(),
        CloseAction::Tray,
        "a fresh library closes to the tray"
    );
    store
        .set_close_action(CloseAction::Quit)
        .expect("the choice stores");
    assert_eq!(
        store.close_action(),
        CloseAction::Quit,
        "the choice round-trips"
    );
    assert_eq!(
        store
            .get_meta(Store::META_CLOSE_ACTION)
            .expect("meta reads"),
        Some("quit".to_string()),
        "the meta row carries the choice"
    );
    store
        .set_close_action(CloseAction::Tray)
        .expect("the choice stores back");
    assert_eq!(store.close_action(), CloseAction::Tray);
    // A corrupt meta row reads as the default, never as a guess.
    store
        .set_meta(Store::META_CLOSE_ACTION, "nonsense")
        .expect("meta writes");
    assert_eq!(
        store.close_action(),
        CloseAction::Tray,
        "a corrupt value reads as the default"
    );
}

/// T-04: the close button's route follows the stored choice. `Tray` hides
/// the window only when a tray exists to hide to; `Quit` always takes the
/// stop-and-exit path.
#[gpui_kit::test]
fn t04_close_route_follows_the_stored_choice(cx: &mut TestAppContext) {
    use manyproxy_ui::app::settings::CloseRoute;
    use manyproxy_ui::store::CloseAction;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(true)),
        CloseRoute::HideToTray,
        "the default choice hides to the tray"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(false)),
        CloseRoute::Quit,
        "without a tray there is nothing to hide to"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .set_close_action(CloseAction::Quit)
                .expect("the choice stores")
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(true)),
        CloseRoute::Quit,
        "the quit choice quits even with a tray"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(false)),
        CloseRoute::Quit,
        "the quit choice quits without a tray too"
    );
}

/// T-04: the tray hint shows exactly once. The flag is written before the
/// toast goes out, so a second hide never repeats it.
#[gpui_kit::test]
fn t04_tray_hint_shows_exactly_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    assert!(
        !view.read_with(cx, |view, _| view.store.seen_tray_hint()),
        "a fresh library has not seen the hint"
    );
    let before = notification_count(cx);
    let first =
        cx.update(|window, cx| view.update(cx, |view, cx| view.show_tray_hint_once(window, cx)));
    assert!(first, "the first hide shows the hint");
    assert_eq!(notification_count(cx), before + 1, "one toast");
    assert!(
        view.read_with(cx, |view, _| view.store.seen_tray_hint()),
        "the flag is persisted"
    );
    let second =
        cx.update(|window, cx| view.update(cx, |view, cx| view.show_tray_hint_once(window, cx)));
    assert!(!second, "the second hide stays quiet");
    assert_eq!(notification_count(cx), before + 1, "no second toast");
}

/// Task 14: turning the icon off makes close mean Quit, records the one-time
/// hint, and turning it back on restores the setting. The stored close choice
/// is preserved for when the icon returns.
#[gpui_kit::test]
fn task14_tray_toggle_forces_quit_and_hints_once(cx: &mut TestAppContext) {
    use manyproxy_ui::app::settings::CloseRoute;

    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    assert!(
        view.read_with(cx, |view, _| view.store.show_tray_icon()),
        "the icon ships on"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(true)),
        CloseRoute::HideToTray,
        "with a tray the default close still hides"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-row-show-tray");
    assert_painted(cx, "settings-row-show-tray");
    assert_painted(cx, "settings-show-tray-switch");

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_show_tray_icon(false, window, cx),
                "the switch commits"
            );
        });
    });
    assert!(
        !view.read_with(cx, |view, _| view.store.show_tray_icon()),
        "the choice persists"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.seen_tray_disable_hint()),
        "the first disable records the hint"
    );
    assert_eq!(notification_count(cx), before + 1, "one hint toast");
    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(true)),
        CloseRoute::Quit,
        "with the icon off, close quits even if a stale tray flag says otherwise"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.close_action()),
        manyproxy_ui::store::CloseAction::Tray,
        "the saved close choice survives underneath"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_show_tray_icon(true, window, cx),
                "the switch turns back on"
            );
        });
    });
    assert!(
        view.read_with(cx, |view, _| view.store.show_tray_icon()),
        "the icon setting is restored"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.close_route(true)),
        CloseRoute::HideToTray,
        "the saved close choice works again with the icon back"
    );
}

/// T-04: the Settings dialog's Window behaviour row writes the persisted
/// choice, and the radio shows it.
#[gpui_kit::test]
fn t04_settings_window_behaviour_saves_the_choice(cx: &mut TestAppContext) {
    use manyproxy_ui::store::CloseAction;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "settings-row-window-behaviour");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                view.commit_close_action(CloseAction::Quit, window, cx),
                "the choice commits"
            );
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.store.close_action()),
        CloseAction::Quit,
        "the choice is persisted"
    );
}

/// T-04: the dialog's geometry, measured. The update-feed URL shapes to
/// 520.8 px at the input's 14 px text size; the field is wider than that, so
/// the value renders in full instead of clipping mid-URL. Buttons size to
/// their content and sit against the body's right edge.
#[gpui_kit::test]
fn t04_settings_dialog_geometry_holds_the_feed_url(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);

    // The feed URL's own width at the text size the input paints.
    let feed = manyproxy_ui::UPDATE_FEED_URL_DEFAULT.to_string();
    let shaped = cx.update(|window, _| {
        let style = window.text_style();
        let mut font = gpui_kit::font(style.font_family.clone());
        font.weight = style.font_weight;
        font.style = style.font_style;
        let runs = vec![gpui_kit::TextRun {
            len: feed.len(),
            font,
            color: gpui_kit::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        window
            .text_system()
            .shape_line(feed.clone().into(), px(14.), &runs, None)
            .width
    });
    assert!(shaped > px(400.), "the feed URL really is long: {shaped:?}");

    paint_dialog_open(cx);

    let dialog_width = cx
        .debug_bounds("dialog-0")
        .map(|bounds| bounds.size.width)
        .expect("the dialog paints");
    assert_eq!(
        dialog_width,
        px(820.),
        "the dialog keeps its measured width"
    );

    let (field_width, body_width) = cx.update(|window, _| {
        (
            window
                .try_find("field-settings-update-feed".to_string())
                .map(|snapshot| snapshot.bounds().size.width),
            window
                .try_find("settings-body".to_string())
                .map(|snapshot| snapshot.bounds().size.width),
        )
    });
    let field_width = field_width.expect("the feed field renders");
    let body_width = body_width.expect("the body renders");
    // The kit's input adds 10 px of padding on each side.
    assert!(
        field_width >= shaped + px(20.),
        "the feed URL renders in full: field {field_width:?} vs text {shaped:?}"
    );

    // K9/A7-2: the section carries no Save of its own; the one footer pair
    // commits, with `Cancel` to the left of the primary `Save`.
    assert_absent(cx, "settings-save-update-feed".to_string());
    let (save, cancel) = cx.update(|window, _| {
        (
            window
                .try_find("settings-dialog-save".to_string())
                .expect("the footer Save renders")
                .bounds(),
            window
                .try_find("settings-dialog-cancel".to_string())
                .expect("the footer Cancel renders")
                .bounds(),
        )
    });
    assert!(
        save.size.width < body_width,
        "the Save button does not stretch: {:?} vs body {body_width:?}",
        save.size.width
    );
    assert!(
        cancel.right() < save.left(),
        "Cancel sits left of Save: {cancel:?} vs {save:?}"
    );
}

#[gpui_kit::test]
fn t80_runtime_stats_render_per_shard(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::{shard_stat_rows, ShardRing};
    use std::collections::{BTreeMap, HashSet};

    let mut rings: BTreeMap<String, ShardRing> = BTreeMap::new();
    rings.insert(
        "stock".to_string(),
        ShardRing {
            cpu: vec![4.5, 6.0],
            ram: vec![31.0, 33.5],
        },
    );
    let shards = vec![
        ("stock".to_string(), Some(41)),
        ("fork".to_string(), None),
        ("xray".to_string(), Some(43)),
        ("extra-9".to_string(), Some(44)),
    ];
    let denied: HashSet<String> = ["xray".to_string()].into_iter().collect();

    let rows = shard_stat_rows(&shards, &rings, &denied);
    assert_eq!(rows.len(), 4, "one row per shard slot");
    assert_eq!(rows[0].label, "stock");
    assert_eq!(rows[0].cpu, vec![4.5, 6.0], "series follow the shard label");
    assert_eq!(rows[0].ram, vec![31.0, 33.5]);
    assert_eq!(rows[0].caption, None, "samples mean charts");
    assert_eq!(
        rows[1].caption,
        Some(manyproxy_ui::ShardCaption::NotRunning)
    );
    assert_eq!(
        rows[2].caption,
        Some(manyproxy_ui::ShardCaption::SamplingUnavailable)
    );
    assert_eq!(
        rows[3].caption,
        Some(manyproxy_ui::ShardCaption::Collecting)
    );
    assert_eq!(
        manyproxy_ui::shard_stats_summary(&rows),
        "3 live shards, 1 charted, sampling unavailable"
    );

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, stock) = EngineController::fake();
            stock.set_pid(Some(41));
            view.engine = engine;
            view.shard_rings = rings.clone();
            view.shard_denied = denied.clone();
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_console(window, cx);
            view.console.tab = manyproxy_ui::ConsoleTab::Traffic;
            cx.notify();
        });
    });
    paint_frame(cx);
    cx.update(|window, cx| {
        window.scroll(
            "console-traffic",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "console-runtime",
        "console-runtime-title",
        "console-runtime-stat-0",
        "console-runtime-stat-8",
        "console-runtime-shards",
        "shard-stat-stock",
        "shard-label-stock",
        "shard-chart-stock",
        "shard-caption-fork",
        "shard-caption-extra-0",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "{id} paints in the console's Runtime group"
        );
    }
    assert!(
        cx.update(|window, _| window.try_find("shard-chart-fork").is_none()),
        "a shard with no samples paints no chart"
    );
}

#[gpui_kit::test]
fn t83_hostname_rejected_in_list_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Loop", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    let error = cx
        .update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_bind_host("example.com", window, cx)
            })
        })
        .expect_err("a hostname never enters the list");
    assert_eq!(
        error,
        Store::BIND_HOST_LITERAL_ONLY,
        "the inline error is the field's own sentence"
    );
    assert_eq!(
        t83_bind_hosts_of(&view, cx),
        vec!["127.0.0.1".to_string(), "0.0.0.0".to_string()],
        "a rejected add leaves the list untouched"
    );

    let hosts = cx
        .update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_bind_host(" 192.168.1.20 ", window, cx)
            })
        })
        .expect("an IPv4 literal is accepted");
    assert_eq!(
        hosts,
        vec![
            "127.0.0.1".to_string(),
            "0.0.0.0".to_string(),
            "192.168.1.20".to_string(),
        ],
        "the literal lands after the constants"
    );
    let duplicate = cx
        .update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_bind_host("192.168.1.20", window, cx)
            })
        })
        .expect_err("duplicates are refused");
    assert!(
        duplicate.contains("already in the list"),
        "duplicate refusal reads inline: {duplicate}"
    );
    let ipv6 = cx
        .update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_bind_host("FD00:0::7", window, cx)
            })
        })
        .expect("an IPv6 literal is accepted");
    assert_eq!(
        ipv6.last().map(String::as_str),
        Some("fd00::7"),
        "IPv6 lands canonical"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    // T-04/B17: the address list lives in the Engine section, in the one
    // column.
    paint_dialog_open(cx);
    for id in [
        "settings-addresses-note",
        "settings-bind-host-127.0.0.1",
        "settings-bind-host-builtin-127.0.0.1",
        "settings-bind-host-0.0.0.0",
        "settings-bind-host-builtin-0.0.0.0",
        "settings-bind-host-192.168.1.20",
        "settings-bind-host-remove-192.168.1.20",
        "settings-bind-host-error",
        "settings-bind-host-add",
        "field-settings-bind-host",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "the Settings list editor renders {id}"
        );
    }
    assert_absent(cx, "settings-bind-host-remove-127.0.0.1".to_string());
}

#[gpui_kit::test]
fn t83_lan_confirm_unchanged_for_nonloopback_bind(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let lan = store
        .add(t30_engine_seed("Lan", "10961"))
        .expect("seed works");
    let loopback = store
        .add(t30_engine_seed("Loop", "10962"))
        .expect("seed works");
    let patch = manyproxy_ui::ConfigPatch {
        bind_ip: Some("192.168.1.44".to_string()),
        ..Default::default()
    };
    store.update(&lan.id, patch).expect("bind moves");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    paint_frame(cx);

    let saved = view
        .read_with(cx, |view, _| view.store.get(&lan.id))
        .expect("store reads")
        .expect("row survives");
    assert_eq!(saved.bind_ip, "192.168.1.44");
    assert!(
        manyproxy_ui::engine::requires_lan_confirm(&saved.bind_ip),
        "a non-loopback bind still asks"
    );
    let text = manyproxy_ui::engine::lan_confirm_text(&saved.name, &saved.bind_ip);
    assert!(
        text.contains("Lan") && text.contains("192.168.1.44"),
        "the unchanged confirm text names the row and bind: {text}"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&lan.id, window, cx));
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Start on a non-loopback bind opens the same LAN confirm"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts before the confirm"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    assert!(
        !manyproxy_ui::engine::requires_lan_confirm("127.0.0.1"),
        "loopback keeps its no-confirm path"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&loopback.id, window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "a loopback row starts without a dialog"
    );
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn t83_custom_config_rows_unaffected(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let params = "{\"inbounds\":[{\"type\":\"mixed\",\"listen\":\"0.0.0.0\",\
            \"listen_port\":10970}]}";
    let custom = store
        .add(NewConfig {
            name: "Custom".to_string(),
            protocol: "custom-config".to_string(),
            params: params.to_string().into(),
            group: String::new(),
            bind_ip: String::new(),
            port: "10970".to_string(),
        })
        .expect("custom-config row seeds");
    let generated = store
        .add(t30_engine_seed("Generated", "10971"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    assert!(
        manyproxy_ui::engine::is_custom_config_protocol(&custom.protocol),
        "the row is JSON-governed"
    );
    // A custom-config row's listens come from its JSON document, so it gets
    // no endpoint editor. A generated row keeps both entry points.
    select_library_row(&view, cx, &custom.id);
    assert_absent(cx, format!("row-endpoint-edit-{}", custom.id));
    assert_absent(cx, "detail-endpoint-edit".to_string());
    select_library_row(&view, cx, &generated.id);
    assert_painted(cx, &format!("row-endpoint-edit-{}", generated.id));
    assert_painted(cx, "detail-endpoint-edit");

    let listens = manyproxy_ui::engine::custom_config_listens(
        &serde_json::from_str::<serde_json::Value>(&custom.params).expect("params are JSON"),
    );
    assert_eq!(
        listens,
        vec![("0.0.0.0".to_string(), 10970)],
        "the custom listen still comes from the JSON document"
    );
}
