//! library: one binary of the split UI suite. Each binary runs as its own
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

/// F3: the checkbox is a bulk-set edit, not a selection gesture. A real
/// pointer click on another row's checkbox must check that row without
/// moving the selection, and the second click must leave both alone.
/// The one selection model: a plain click replaces the set, a Ctrl click
/// toggles one row, and a Shift click replaces the set with the range from
/// the anchor.
/// The one selection model: a plain click replaces the set, a Ctrl click
/// toggles one row, and a Shift click replaces the set with the range from
/// the anchor.
#[gpui_kit::test]
fn selection_click_modifiers(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("First", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Second", "b.example.com", "10802"))
        .expect("seed works");
    let third = store
        .add(row19_seed("Third", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    // A plain click replaces the set.
    select_library_row(&view, cx, &first.id);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![first.id.clone()],
        "the plain click selects exactly its row"
    );

    // A Ctrl click toggles the row and leaves the rest alone.
    click_row_with(
        cx,
        &format!("row-name-text-{}", second.id),
        gpui_kit::Modifiers {
            control: true,
            ..Default::default()
        },
    );
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![first.id.clone(), second.id.clone()],
        "the Ctrl click adds its row"
    );

    // A Shift click replaces the set with the range from the anchor.
    click_row_with(
        cx,
        &format!("row-name-text-{}", third.id),
        gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![first.id.clone(), second.id.clone(), third.id.clone()],
        "the Shift click replaces the set with the range"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        Some(first.id.clone()),
        "the anchor stays where it was"
    );
}

#[test]
fn t01_detail_header_names_acting_row() {
    use manyproxy_ui::acting_on_label;
    use manyproxy_ui::store::ProxyConfig;
    fn row(id: &str, name: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: r#"{"server":"a.example.com"}"#.to_string().into(),
            group: "g".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let acting = row("2", "Beta");
    assert_eq!(acting_on_label(&acting), "Acting on: Beta");
    assert_ne!(acting.id, "1", "the header names the selection");
}

#[gpui_kit::test]
fn t44_ui_bars_and_the_batch_bar_paint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, "facet-bar".to_string());
    assert_absent(cx, "facet-group".to_string());
    assert_painted(cx, "filter-slot");
    assert_painted(cx, "library-search");
    // The checkbox is retired: no row carries one.
    assert_absent(cx, format!("check-{}", first.id));
    assert_absent(cx, format!("check-{}", second.id));

    // A selection swaps the search for the batch bar.
    select_library_row(&view, cx, &first.id);
    assert_painted(cx, "batch-bar");
    assert_painted(cx, "batch-count");
    assert_painted(cx, "batch-start");
    assert_painted(cx, "batch-stop");
    assert_painted(cx, "batch-test");
    assert_painted(cx, "batch-delete");
    assert_painted(cx, "batch-more");
    assert_absent(cx, "library-search".to_string());
    // No checked control survives anywhere in the row.
    assert_absent(cx, format!("row-check-bar-{}", first.id));
    assert_absent(cx, format!("row-check-bar-{}", second.id));
    assert_absent(cx, format!("row-check-chip-{}", first.id));
    assert_absent(cx, format!("row-check-chip-{}", second.id));
    // The old Copy SOCKS cell is gone; the endpoint chip replaces it.
    assert_painted(cx, &format!("row-endpoint-chip-{}", first.id));

    // The count names the selection.
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 1);

    // The batch bar's More is a popover with the signed verbs.
    cx.update(|window, cx| {
        window.click("batch-more", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "batch-more-copy",
        "batch-more-toggle",
        "batch-more-move",
        "batch-more-preset",
        "batch-more-reset-traffic",
        "batch-more-reset-usage",
        "batch-more-delete",
    ] {
        assert_painted(cx, id);
    }
}

#[gpui_kit::test]
fn k04_batch_bar_single_line_within_min_frame(cx: &mut TestAppContext) {
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
            view.select_rows(&[first.id.clone(), second.id.clone()], cx);
        });
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    assert_painted(cx, "batch-bar");
    assert_within_frame_min(cx, "batch-bar");
    assert_painted(cx, "batch-count");
    for id in ["batch-start", "batch-stop", "batch-more"] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    // The bar is one line, and its content ends before the 260px inspector
    // drawer can cover it at the 880px minimum width.
    let (bar_bounds, start_height, more_bounds) = cx.update(|window, _| {
        let bar = window
            .try_find("batch-bar".to_string())
            .expect("batch bar renders")
            .bounds();
        let start = window
            .try_find("batch-start".to_string())
            .expect("batch start renders")
            .bounds();
        let more = window
            .try_find("batch-more".to_string())
            .expect("batch more renders")
            .bounds();
        (bar, start.size.height, more)
    });
    assert!(
        bar_bounds.size.height <= start_height + px(20.),
        "the batch bar stays on ONE line: bar {bar_bounds:?} vs button height {start_height:?}"
    );
    assert!(
        more_bounds.origin.x + more_bounds.size.width
            <= bar_bounds.origin.x + bar_bounds.size.width,
        "the batch More stays inside the bar: {more_bounds:?} in {bar_bounds:?}"
    );
}

/// K4: the compact bar's Start is the SAME batch path the menu used to own.
/// Clicking the real button must reach `start_many_additive` — one apply for
/// every checked row, never a per-row spawn loop.
/// The batch bar's Start is the SAME batch path the menu used to own.
#[gpui_kit::test]
fn k04_batch_start_button_uses_batch_path(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10991"))
        .expect("seed works");
    let beta = store
        .add(t30_engine_seed("Beta", "10992"))
        .expect("seed works");
    let gamma = store
        .add(t30_engine_seed("Gamma", "10993"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let (engine, stock) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = engine;
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[alpha.id.clone(), gamma.id.clone()], cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "batch-start");

    cx.update(|window, cx| {
        window.click("batch-start", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        stock.check_calls(),
        1,
        "the button clicks into ONE batch apply"
    );
    let serving = view.read_with(cx, |view, _| view.engine.running_ids());
    assert!(
        serving.contains(&alpha.id) && serving.contains(&gamma.id),
        "both selected rows serve: {serving:?}"
    );
    assert!(
        !serving.contains(&beta.id),
        "the unselected row was not started: {serving:?}"
    );
    let served = stock.last_config().expect("the batch served a config");
    let value: serde_json::Value =
        serde_json::from_str(&served).expect("the served config is JSON");
    let ports: Vec<u64> = value["inbounds"]
        .as_array()
        .expect("inbounds array")
        .iter()
        .filter_map(|inbound| inbound["listen_port"].as_u64())
        .collect();
    for port in [10991u64, 10993] {
        assert!(ports.contains(&port), "port {port} in the batch: {ports:?}");
    }
    assert!(
        !ports.contains(&10992),
        "the unselected port never enters the batch: {ports:?}"
    );
}

#[gpui_kit::test]
fn t01_selection_grammar_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let third = store
        .add(row19_seed("Lab", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[first.id.clone(), second.id.clone()], cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    // One selection model: the selected rows carry the wash and the rim, and
    // no row carries a check bar or a check chip.
    assert_absent(cx, format!("row-check-bar-{}", first.id));
    assert_absent(cx, format!("row-check-bar-{}", second.id));
    assert_absent(cx, format!("row-check-bar-{}", third.id));
    assert_absent(cx, format!("row-check-chip-{}", first.id));
    assert_absent(cx, format!("row-check-chip-{}", second.id));
    assert_absent(cx, format!("row-check-chip-{}", third.id));
    // Every row carries the lead and the endpoint chip.
    assert_painted(cx, &format!("row-lead-{}", first.id));
    assert_painted(cx, &format!("row-endpoint-chip-{}", first.id));
    assert_painted(cx, &format!("row-endpoint-menu-{}", first.id));
    assert_painted(cx, "batch-bar");
    assert_painted(cx, "batch-count");
    assert_painted(cx, "inspector-selected-count");
    assert_painted(cx, "inspector-use-selection-bar");
    assert_absent(cx, "detail-acting-on".to_string());
}

#[gpui_kit::test]
fn r04_columns_sheet_toggles_and_persists(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_columns_sheet(window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "columns sheet opens"
    );
    for id in [
        "columns-sheet-toggle-upstream",
        "columns-sheet-toggle-health",
        "columns-sheet-toggle-group",
        "columns-sheet-toggle-name",
        "columns-sheet-up-group",
        "columns-sheet-down-group",
        "columns-sheet-reset-widths",
        "columns-sheet-reset-order",
        "columns-sheet-reset-visibility",
        "columns-sheet-reset-all",
        "columns-sheet-done",
    ] {
        assert_painted(cx, id);
    }
    let layout = view.read_with(cx, |view, _| view.column_layout);
    assert!(
        layout.is_visible(ColumnId::Health)
            && layout.is_visible(ColumnId::Group)
            && !layout.is_visible(ColumnId::Upstream)
            && !layout.is_visible(ColumnId::Protocol)
            && !layout.is_visible(ColumnId::Traffic),
        "the signed defaults: {layout:?}"
    );
    // Upstream is off by default; turn it on so the toggle has a subject.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_column_visible(ColumnId::Upstream, true, window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_painted(cx, &format!("row-upstream-{}", seeded.id));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_column_visible(ColumnId::Upstream, false, window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    let after = view.read_with(cx, |view, cx| {
        (
            view.column_layout,
            view.libraries.read(cx).delegate().row_metrics.layout,
            view.store.library_column_layout(),
        )
    });
    assert!(
        !after.0.is_visible(ColumnId::Upstream)
            && after.0.is_visible(ColumnId::Health)
            && after.0.is_visible(ColumnId::Group),
        "toggle flips one column, keeps the rest: {:?}",
        after.0
    );
    assert_eq!(after.0, after.1, "delegate mirrors the choice");
    assert_eq!(after.0, after.2, "the layout key persists the choice");
    assert_absent(cx, format!("row-upstream-{}", seeded.id));

    // Locked columns cannot be hidden even through the API the checkbox calls.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_column_visible(ColumnId::Name, false, window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Name),
        "Name is always visible"
    );
}

/// R11: the retired `row-columns` palette id now opens the Columns sheet.
#[gpui_kit::test]
fn columns_palette_action_opens_the_sheet(cx: &mut TestAppContext) {
    use manyproxy_ui::app::actions::{palette_action_at, PALETTE_GROUPS};
    use gpui_kit::component::IndexPath;

    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let (section, row) = PALETTE_GROUPS
        .iter()
        .enumerate()
        .find_map(|(section, (_, ids))| {
            ids.iter()
                .position(|candidate| *candidate == "row-columns")
                .map(|row| (section, row))
        })
        .expect("row-columns is in a palette group");
    let op = IndexPath::new(row).section(section);
    assert_eq!(palette_action_at(op), Some("row-columns"));
    assert_eq!(
        manyproxy_ui::t!("action-label-row-columns"),
        "Columns…",
        "the palette names the sheet, not the retired dialog"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.run_palette_op(op, window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "the palette opens the columns sheet"
    );
}

/// T-05: `Start checked` is ADDITIVE. The rows that already serve keep
/// serving, and the whole batch runs ONE apply — the fake engine's check
/// counter is the proof (one `check_config` per apply, never per row).
#[gpui_kit::test]
fn start_selected_is_additive(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10941"))
        .expect("seed works");
    let beta = store
        .add(t30_engine_seed("Beta", "10942"))
        .expect("seed works");
    let gamma = store
        .add(t30_engine_seed("Gamma", "10943"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let (engine, stock) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = engine;
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&alpha.id, window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&beta.id, window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "A and B serve before the batch"
    );
    let checks_before = stock.check_calls();

    // Only C is selected.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(std::slice::from_ref(&gamma.id), cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_batch(window, cx));
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        3,
        "the batch ADDS C, it does not replace the serving set"
    );
    assert_eq!(
        stock.check_calls(),
        checks_before + 1,
        "one apply for the whole batch"
    );
    let serving = view.read_with(cx, |view, _| view.engine.running_ids());
    for id in [&alpha.id, &beta.id, &gamma.id] {
        assert!(serving.contains(id), "{id} still serves: {serving:?}");
    }
    // The ONE applied config carries all three listeners — A and B were not
    // torn down on the way.
    let served = stock.last_config().expect("the batch served a config");
    let value: serde_json::Value =
        serde_json::from_str(&served).expect("the served config is JSON");
    let ports: Vec<u64> = value["inbounds"]
        .as_array()
        .expect("inbounds array")
        .iter()
        .filter_map(|inbound| inbound["listen_port"].as_u64())
        .collect();
    for port in [10941u64, 10942, 10943] {
        assert!(ports.contains(&port), "port {port} still served: {ports:?}");
    }
}

/// T-05: twenty checked rows are ONE apply. The fake's check counter is
/// the proof — it counts applies, never per-row starts.
/// Twenty selected rows are ONE apply. The fake's check counter is the
/// proof — it counts applies, never per-row starts.
#[gpui_kit::test]
fn bulk_start_twenty_rows_applies_once(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids: Vec<String> = Vec::new();
    for index in 0..20 {
        let created = store
            .add(t30_engine_seed(
                &format!("Row{index}"),
                &format!("{}", 10951 + index),
            ))
            .expect("seed works");
        ids.push(created.id);
    }
    let (view, cx) = mount_view(store, cx);
    let (engine, stock) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = engine;
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&ids, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_batch(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        20,
        "every selected row serves"
    );
    assert_eq!(
        stock.check_calls(),
        1,
        "twenty rows, ONE apply — no per-row flap"
    );
}

/// T-05: a checked row whose port is occupied is NAMED and skipped while
/// the rest start. The batch is not refused and no every-row failure is
/// reported.
/// A selected row whose port is occupied is NAMED and skipped while the
/// rest start. The batch is not refused and no every-row failure is
/// reported.
#[gpui_kit::test]
fn bulk_start_skips_the_occupied_row_and_starts_the_rest(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let held = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
    let port = held.local_addr().expect("addr").port();
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10981"))
        .expect("seed works");
    let squatter = store
        .add(t30_engine_seed("Squatter", &port.to_string()))
        .expect("seed works");
    let gamma = store
        .add(t30_engine_seed("Gamma", "10983"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let (engine, stock) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = engine;
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(
                &[alpha.id.clone(), squatter.id.clone(), gamma.id.clone()],
                cx,
            );
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_batch(window, cx));
    });
    let serving = view.read_with(cx, |view, _| view.engine.running_ids());
    assert_eq!(serving.len(), 2, "the two free rows serve: {serving:?}");
    assert!(
        serving.contains(&alpha.id) && serving.contains(&gamma.id),
        "the free rows are up: {serving:?}"
    );
    assert!(
        !serving.contains(&squatter.id),
        "the occupied row is skipped: {serving:?}"
    );
    assert_eq!(stock.check_calls(), 1, "one apply for the batch");
    assert_eq!(
        notification_count(cx),
        1,
        "one toast — the refusal names the row, not every row"
    );
    drop(held);
}

#[test]
fn r01_legacy_column_seed_reads_the_default_layout() {
    let layout = ColumnLayout::default();
    assert!(
        layout.is_visible(ColumnId::Name)
            && layout.is_visible(ColumnId::Endpoint)
            && layout.is_visible(ColumnId::Health)
            && layout.is_visible(ColumnId::Group)
            && !layout.is_visible(ColumnId::Protocol)
            && !layout.is_visible(ColumnId::Upstream)
            && !layout.is_visible(ColumnId::Traffic),
        "the signed defaults: {layout:?}"
    );
    assert_eq!(
        layout.order(),
        &ColumnId::ALL,
        "Name, Endpoint, Health, Group, Protocol, Upstream, Traffic"
    );
    assert_eq!(
        ColumnLayout::seed_from_legacy("upstream,health"),
        {
            let mut seeded = ColumnLayout::default();
            seeded.set_hidden(ColumnId::Group, true);
            seeded.set_hidden(ColumnId::Upstream, false);
            seeded
        },
        "the retired token list seeds visibility once"
    );
}

#[test]
fn t01_row_cells_carry_one_fact_each() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::{library_row_cells, library_row_full_route};
    let row = ProxyConfig {
        id: "7".to_string(),
        name: "Work".to_string(),
        protocol: "vless".to_string(),
        params: "{}".to_string().into(),
        group: "travel".to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let cells = library_row_cells(
        &row,
        "a.example.com:443",
        Some("87ms"),
        true,
        false,
        None,
        false,
    );
    assert_eq!(
        cells[0], "a.example.com:443",
        "upstream cell carries upstream"
    );
    assert!(
        cells[1].contains('●') && cells[1].contains("87ms"),
        "health cell carries state plus probe:\n{}",
        cells[1]
    );
    assert_eq!(cells[2], "travel", "group cell carries group");
    assert_eq!(cells[3], "", "no traffic without totals");
    let busy = library_row_cells(
        &row,
        "a.example.com:443",
        Some("87ms"),
        true,
        false,
        Some((1_048_782, 88)),
        false,
    );
    assert!(
        busy[3].contains("1.0 MB"),
        "traffic cell carries totals:\n{}",
        busy[3]
    );
    assert!(
        !busy[1].contains("1.0 MB"),
        "health cell stays state-only:\n{}",
        busy[1]
    );
    let idle = library_row_cells(&row, "a.example.com:443", None, false, false, None, false);
    assert_eq!(
        idle[1], "",
        "stopped health cell stays empty — the gray lamp carries stopped"
    );
    let secured = library_row_cells(&row, "a.example.com:443", None, true, false, None, true);
    assert_eq!(
        secured[0], "a.example.com:443 · none",
        "security posture splices onto upstream:\n{}",
        secured[0]
    );
    let full = library_row_full_route(&row, "a.example.com:443");
    for want in [
        "vless",
        "a.example.com:443",
        "travel",
        "127.0.0.1:10801",
        "enabled",
    ] {
        assert!(
            full.contains(want),
            "{want} stays on the full route tooltip:\n{full}"
        );
    }
}

/// One selection model: the selected row wears the wash and the rim, and
/// no row carries a second, checked control.
#[gpui_kit::test]
fn t01_the_selection_wears_the_wash_and_the_rim(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let only = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&only.id, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, &format!("library-row-{}", only.id));
    assert_painted(cx, &format!("row-surface-{}", only.id));
    assert_painted(cx, "detail-acting-on");

    // The checkbox is retired: the row carries no check control at all.
    assert_absent(cx, format!("check-{}", only.id));
    assert_absent(cx, format!("row-check-bar-{}", only.id));
    assert_absent(cx, format!("row-check-chip-{}", only.id));
    assert_painted(cx, "batch-bar");
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        1,
        "the selection holds exactly one row"
    );
}

/// The lead is 24px wide and holds one 14px glyph. It is status, not a
/// control: clicking it changes no selection and no filter.
#[gpui_kit::test]
fn row_lead_reports_phase_without_being_clickable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                state.delegate_mut().running.insert(seeded.id.clone());
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let id = format!("row-lead-{}", seeded.id);
    assert_painted(cx, &id);
    let bounds = cx.update(|window, _| window.try_find(id.clone()).expect("lead finds").bounds());
    assert_eq!(
        bounds.size.width,
        px(manyproxy_ui::RowMetrics::LEAD),
        "the lead is exactly 24px wide: {bounds:?}"
    );
    // The lead carries no button role: it is a mark, not a control.
    let role = cx.update(|window, _| {
        window
            .try_find(id.clone())
            .expect("lead finds")
            .role()
            .map(|role| format!("{role:?}"))
    });
    assert!(
        role.as_deref().is_none_or(|role| !role.contains("Button")),
        "the lead is not a button: {role:?}"
    );

    // Clicking the lead, centre or edge, changes no selection and no filter.
    let before = view.read_with(cx, |view, _| view.selection_count());
    cx.update(|window, cx| window.click(id.clone(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        before,
        "the lead is status only"
    );
    let status = view.read_with(cx, |view, _| view.query.statuses.clone());
    assert!(status.is_empty(), "the lead is not a filter trigger");
    cx.update(|window, cx| window.click_at(id, point(px(1.), px(12.)), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        before,
        "the whole 24px lead is inert"
    );
    let status = view.read_with(cx, |view, _| view.query.statuses.clone());
    assert!(status.is_empty(), "the lead never writes the status facet");
}

#[gpui_kit::test]
fn t44_ui_escape_clears_the_selection_only_without_dialog(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10841"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&first.id, cx);
        });
    });

    let cleared = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx)
        })
    });
    assert!(cleared, "escape clears with no dialog open");
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 0);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&first.id, cx);
        });
    });
    cx.update(|window, cx| {
        window.open_dialog(cx, |dialog: Dialog, _, _| dialog.title("Guard probe"));
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let cleared = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx)
        })
    });
    assert!(
        !cleared,
        "escape keeps the selection while a dialog is open"
    );
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 1);
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t44_ui_bulk_move_and_group_manager(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10821"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10822"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[first.id.clone(), second.id.clone()], cx);
        });
    });
    let moved = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_bulk_move(&[first.id.clone(), second.id.clone()], "team", window, cx)
        })
    });
    assert!(moved, "move commits and closes");
    let groups = view.read_with(cx, |view, _| {
        view.store.group_counts().expect("counts work")
    });
    assert_eq!(groups, vec![("team".to_string(), 2)]);

    let single = [first.id.clone()];
    let kept = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_bulk_move(&single, "   ", window, cx)
        })
    });
    assert!(!kept, "empty group keeps the dialog open");

    let renamed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_rename_group("team", "crew", window, cx)
        })
    });
    assert!(renamed, "rename commits");
    let groups = view.read_with(cx, |view, _| {
        view.store.group_counts().expect("counts work")
    });
    assert_eq!(groups, vec![("crew".to_string(), 2)]);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_delete_group("crew", window, cx));
    });
    assert_eq!(
        notification_count(cx),
        before + 1,
        "non-empty delete refuses loudly"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_delete_group("ghost-label", window, cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.store.list().expect("list works").len()
        }),
        2,
        "refused delete removes nothing"
    );
}

#[gpui_kit::test]
fn t44_ui_rename_group_refreshes_facets_and_inspector(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("One", "vless", "home", "10841"))
        .expect("seed works");
    store
        .add(facet_seed("Two", "vless", "home", "10842"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let renamed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_rename_group("home", "cabin", window, cx)
        })
    });
    assert!(renamed, "rename commits");

    let groups = view.read_with(cx, |view, _| {
        view.store.group_counts().expect("counts work")
    });
    assert_eq!(groups, vec![("cabin".to_string(), 2)]);

    let selected = view.read_with(cx, |view, cx| view.selected_config(cx));
    assert_eq!(
        selected.map(|row| row.group),
        Some("cabin".to_string()),
        "inspector row carries the renamed group"
    );
}

#[gpui_kit::test]
fn t44_ui_delete_group_refuses_while_rows_remain(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("One", "vless", "site/east", "10843"))
        .expect("seed works");
    store
        .add(facet_seed("Two", "vless", "site/east", "10844"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_delete_group("site/east", window, cx)
        });
    });
    assert_eq!(
        notification_count(cx),
        before + 1,
        "non-empty delete refuses loudly"
    );
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(rows.len(), 2, "refused delete removes nothing");
    assert!(
        rows.iter().all(|row| row.group == "site/east"),
        "rows keep the verbatim label"
    );
}

#[gpui_kit::test]
fn laneb_group_delete_confirms_naming_count(cx: &mut TestAppContext) {
    assert!(
        manyproxy_ui::text::group_delete_confirm_text("crew", 0).contains("\"crew\""),
        "empty arm names the group"
    );
    assert!(
        manyproxy_ui::text::group_delete_confirm_text("site/east", 2).contains("2 row(s)"),
        "non-empty arm names the count"
    );
    assert!(
        manyproxy_ui::text::group_delete_confirm_text("site/east", 2).contains("refuse"),
        "non-empty arm warns up front"
    );
    let mixed = manyproxy_ui::group_delete_confirm_description("site/east", 2, 3);
    assert!(
        mixed.contains("2 row(s) and 3 subscription(s)"),
        "mixed arm names both counts: {mixed}"
    );
    let sub_only = manyproxy_ui::group_delete_confirm_description("site/east", 0, 3);
    assert!(
        sub_only.contains("3 subscription(s)"),
        "subscription-only arm kept: {sub_only}"
    );
    assert_eq!(
        manyproxy_ui::group_delete_confirm_description("crew", 0, 0),
        manyproxy_ui::text::group_delete_confirm_text("crew", 0),
        "empty group keeps the existing copy"
    );
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("One", "vless", "site/east", "10843"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_delete_confirm("site/east", window, cx)
        });
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "group delete confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t44_slugify_group_id_keeps_plain_names_stable() {
    assert_eq!(manyproxy_ui::slugify_group_id("travel"), "travel");
    assert_eq!(manyproxy_ui::slugify_group_id("team"), "team");
    assert_eq!(manyproxy_ui::slugify_group_id("site/east"), "site-east");
    assert_eq!(manyproxy_ui::slugify_group_id("my group"), "my-group");
    assert_eq!(manyproxy_ui::slugify_group_id("a/b/c"), "a-b-c");
    assert!(!manyproxy_ui::slugify_group_id("a/b").contains('/'));
    assert_eq!(manyproxy_ui::slugify_group_id(""), "group");
}

#[gpui_kit::test]
fn t44_ui_facets_filter_and_clear(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10831"))
        .expect("seed works");
    store
        .add(row19_seed("Home", "b.example.com", "10832"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_bind_facet("127.0.0.1".to_string(), true, cx)
        });
    });
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 2);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_bind_facet("10.9.9.9".to_string(), true, cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        2,
        "the binds OR inside the facet, so the first host keeps its rows"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_bind_facet("127.0.0.1".to_string(), false, cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        0,
        "an absent bind still counts as active and shows nothing"
    );
    assert!(view.read_with(cx, |view, _| view.query.is_active()));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.clear_filters(cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 2);
    assert!(!view.read_with(cx, |view, _| view.query.is_active()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.clear_query(window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 2);
}

#[test]
fn w1_search_never_matches_credentials() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    use manyproxy_core::{library_row_matches, library_row_matches_full, MatchContext};
    let row = ProxyConfig {
            id: "1".to_string(),
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: r#"{"server":"a.example.com","server_port":443,"password":"hunter2","uuid":"SECRET-UUID","token":"abc123"}"#.to_string().into(),
            group: "travel".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };

    assert!(library_row_matches(&row, "a.example.com"));
    assert!(library_row_matches(&row, "10801"));
    let ctx = MatchContext::default();
    assert!(library_row_matches_full(&row, "a.example.com", &ctx));

    for secret in ["hunter2", "SECRET-UUID", "abc123", "HUNTER2"] {
        assert!(
            !library_row_matches(&row, secret),
            "base matcher leaks {secret}"
        );
        assert!(
            !library_row_matches_full(&row, secret, &ctx),
            "extended matcher leaks {secret}"
        );
    }

    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![row]);
    for secret in ["hunter2", "SECRET-UUID", "abc123"] {
        let mut query = manyproxy_ui::FilterQuery::default();
        query.set_search(secret);
        assert!(
            test_visible(&delegate, &query).is_empty(),
            "index leaks {secret}"
        );
    }
    let mut query = manyproxy_ui::FilterQuery::default();
    query.set_search("a.example.com");
    assert_eq!(
        test_visible(&delegate, &query).len(),
        1,
        "upstream still searchable"
    );
}

#[gpui_kit::test]
fn w1_library_fifty_rows_paint_and_filter(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for index in 0..50 {
        let row = store
            .add(seed_input(&format!("row-{index:02}")))
            .expect("seed works");
        ids.push(row.id);
    }
    let (view, cx) = mount_view(store, cx);
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 50);
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 50);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "config-list");
    assert_painted(cx, "action-bar-count");
    assert_painted(cx, &format!("library-row-{}", ids[0]));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("row-01".to_string(), cx)
        });
    });
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 1);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now(String::new(), cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 50);
}

#[test]
fn w1_row_phase_precedence_and_start_copy() {
    use manyproxy_ui::RowPhase;
    assert_eq!(RowPhase::of(false, false, false), RowPhase::Stopped);
    assert_eq!(RowPhase::of(true, false, false), RowPhase::Running);
    assert_eq!(RowPhase::of(true, false, true), RowPhase::Failed);

    assert_eq!(RowPhase::of(false, false, true), RowPhase::Failed);

    assert_eq!(RowPhase::of(false, true, false), RowPhase::Starting);
    assert_eq!(RowPhase::of(true, true, false), RowPhase::Starting);
    assert_eq!(RowPhase::of(true, true, true), RowPhase::Starting);
    assert_eq!(RowPhase::Stopped.start_label(), "Start");
    assert_eq!(RowPhase::Running.start_label(), "Start");
    assert_eq!(RowPhase::Failed.start_label(), "Start");
    assert_eq!(RowPhase::Starting.start_label(), "Starting…");
}

#[test]
fn t47_delegate_caches_survive_params_poison() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    use manyproxy_core::{library_row_matches, upstream_host_port};
    let row = ProxyConfig {
        id: "1".to_string(),
        name: "Alpha".to_string(),
        protocol: "vless".to_string(),
        params: r#"{"server":"a.example.com","server_port":443}"#.to_string().into(),
        group: "g".to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![row]);
    let before = delegate.upstream_for("1").map(str::to_string);
    assert_eq!(before.as_deref(), Some("a.example.com:443"));

    delegate.rows[0].params = "not json{{{".to_string().into();
    let mut query = manyproxy_ui::FilterQuery::default();
    query.set_search("a.example.com");
    assert_eq!(
        test_visible(&delegate, &query).len(),
        1,
        "filter served from the index, not a live parse"
    );
    assert_eq!(
        delegate.upstream_for("1").map(str::to_string),
        before,
        "render served from the cache, not a live parse"
    );

    assert!(!library_row_matches(&delegate.rows[0], "a.example.com"));
    assert_eq!(upstream_host_port(&delegate.rows[0]), None);
}

#[test]
fn lan_hint_for_bind() {
    use manyproxy_ui::lan_hint_for_bind;
    assert_eq!(lan_hint_for_bind("127.0.0.1"), None);
    assert_eq!(lan_hint_for_bind(""), None, "blank means loopback default");
    assert_eq!(lan_hint_for_bind("  "), None);
    let hint = lan_hint_for_bind("0.0.0.0").expect("LAN binds hint");
    assert!(hint.contains("0.0.0.0"), "names the bind: {hint}");
    assert!(hint.contains("LAN confirm"), "names the gate: {hint}");
}

#[gpui_kit::test]
fn add_dialog_shows_lan_hint_only_for_lan_bind(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let lan = store
        .add(NewConfig {
            name: "Lan".to_string(),
            protocol: "vless".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: "0.0.0.0".to_string(),
            port: String::new(),
        })
        .expect("seed works");
    let loopback = store.add(seed_input("Loop")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(lan, window, cx));
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
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("lan-hint").is_some()));

    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(loopback, window, cx));
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
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        cx.update(|window, _| window.try_find("lan-hint").is_none()),
        "loopback binds show no LAN hint"
    );
}

#[test]
fn import_files_label_basenames() {
    use manyproxy_ui::import_files_label;
    assert_eq!(import_files_label(None, None), None);
    assert_eq!(
        import_files_label(Some("C:\\drops\\home.conf"), None),
        Some("File: home.conf".to_string())
    );
    assert_eq!(
        import_files_label(
            None,
            Some(&[
                ("/tmp/a.conf".to_string(), "x".to_string()),
                ("b.json".to_string(), "y".to_string())
            ])
        ),
        Some("Files: a.conf, b.json".to_string())
    );
}

#[gpui_kit::test]
fn dialog_footer_ok_commits_import_end_to_end(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog(
                "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
                "travel",
                None,
                window,
                cx,
            );
        });
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("import-dialog-ok").is_some()));
    cx.update(|window, cx| {
        window.click("import-dialog-ok", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "OK click closes the dialog"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "one click commits exactly one row (no double-commit)"
    );
    assert_eq!(notification_count(cx), 1, "one click fires one toast");
    let group = view.read_with(cx, |view, _| {
        view.store.list().expect("list works").remove(0).group
    });
    assert_eq!(group, "travel");
}

#[gpui_kit::test]
fn dialog_footer_cancel_closes_add_without_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("add-dialog-cancel").is_some()));
    cx.update(|window, cx| {
        window.click("add-dialog-cancel", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel closes the dialog"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "Cancel creates no row"
    );
    assert_eq!(notification_count(cx), 0, "Cancel fires no toast");
}

#[gpui_kit::test]
fn dialog_footers_audit_every_plain_dialog(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    for (open, ok, cancel) in [
        ("add", "add-dialog-ok", "add-dialog-cancel"),
        ("edit", "edit-dialog-ok", "edit-dialog-cancel"),
        ("import", "import-dialog-ok", "import-dialog-cancel"),
        ("export", "export-dialog-ok", "export-dialog-cancel"),
        (
            "subscriptions",
            "subscriptions-dialog-ok",
            "subscriptions-dialog-cancel",
        ),
        ("presets", "presets-dialog-ok", "presets-dialog-cancel"),
        ("logs", "logs-dialog-ok", "logs-dialog-cancel"),
    ] {
        let target = seeded.clone();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| match open {
                "add" => view.open_add_dialog(window, cx),
                "edit" => view.open_edit_dialog(target, window, cx),
                "import" => view.open_import_dialog("vless://x", "travel", None, window, cx),

                "export" => view.open_export_dialog(window, cx),
                "subscriptions" => view.open_subscriptions_dialog(window, cx),
                "presets" => view.open_presets_dialog(window, cx),
                _ => view.open_logs_dialog(window, cx),
            });
        });
        paint_dialog_open(cx);

        let is_sheet = open == "subscriptions" || open == "logs";
        if is_sheet {
            assert!(
                cx.update(|window, cx| window.has_active_sheet(cx)),
                "{open} opens as a sheet"
            );
            assert!(
                !cx.update(|window, cx| window.has_active_dialog(cx)),
                "{open} opens no dialog"
            );
        } else {
            assert!(
                cx.update(|window, cx| window.has_active_dialog(cx)),
                "{open} dialog opens"
            );
        }
        assert!(
            cx.update(|window, _| window.try_find(ok).is_some()),
            "{open} dialog renders footer OK ({ok})"
        );
        assert!(
            cx.update(|window, _| window.try_find(cancel).is_some()),
            "{open} dialog renders footer Cancel ({cancel})"
        );
        cx.update(|window, cx| {
            window.close_all_dialogs(cx);
            window.close_sheet(cx);
        });
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn dialog_footer_ok_invalid_import_stays_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog("bogus://x", "travel", None, window, cx);
        });
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        window.click("import-dialog-ok", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "invalid OK keeps the dialog open (same as Enter)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
    assert_eq!(notification_count(cx), 1, "one error notice, never silent");
}

#[test]
fn honest_footer_labels_name_their_effect() {
    use manyproxy_ui::footer_commit_label;
    for dialog in [
        "settings",
        "groups",
        "preset-attach",
        "runtime-stats",
        "traffic-stats",
        "bare-url",
        "logs",
        "subscriptions",
        "preset-edit",
        "subscription-edit",
        "import",
        "export",
        "group-rename",
    ] {
        let label = footer_commit_label(dialog);
        assert!(
            !label.eq_ignore_ascii_case("ok") && !label.eq_ignore_ascii_case("okay"),
            "Ticket 05: no bare OK footer ({dialog})"
        );
    }
    assert_eq!(
        footer_commit_label("settings"),
        "Close",
        "K5: the dead settings mapping is gone — the modal owns Save/Cancel"
    );
    assert_eq!(footer_commit_label("groups"), "Close");
    assert_eq!(footer_commit_label("preset-attach"), "Close");
    assert_eq!(footer_commit_label("bare-url"), "Close");
    assert_eq!(
        footer_commit_label("logs"),
        "Apply filter",
        "committing the filter is distinct from discarding via Cancel"
    );
    assert_eq!(footer_commit_label("presets"), "OK");
    assert_eq!(footer_commit_label("subscriptions"), "Add");
    assert_eq!(footer_commit_label("add"), "OK");
    assert_eq!(footer_commit_label("edit"), "OK");
    assert_eq!(footer_commit_label("preset-edit"), "Save");
    assert_eq!(footer_commit_label("subscription-edit"), "Save");
    assert_eq!(footer_commit_label("import"), "Import");
    assert_eq!(footer_commit_label("export"), "Export");
    assert_eq!(footer_commit_label("bulk-move"), "Move");
    assert_eq!(footer_commit_label("group-rename"), "Rename");
}

#[gpui_kit::test]
fn single_close_footers_render_one_close_and_esc_closes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let esc = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    // K9/A7-2: Settings carries one footer pair, not a lone `Done`.
    assert!(
        cx.update(|window, _| window.try_find("settings-dialog-save").is_some()),
        "Settings renders its Save"
    );
    assert!(
        cx.update(|window, _| window.try_find("settings-dialog-cancel").is_some()),
        "Settings renders its Cancel sibling"
    );
    assert!(
        cx.update(|window, _| window.try_find("settings-dialog-done").is_none()),
        "the old Done button is gone"
    );
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.dispatch_keystroke(esc.clone(), cx);
    });
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Esc closes Settings"
    );
    assert_eq!(notification_count(cx), before, "Esc commits nothing");
}

#[gpui_kit::test]
fn honest_footers_close_silently_without_side_effects(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store.add(seed_input("Work")).expect("seed works");
    store
        .create_preset(
            "ads",
            "manual",
            None,
            &[serde_json::json!({"action": "block"})],
        )
        .expect("preset seeds");
    let row_id = row.id.clone();
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("settings-dialog-save", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Settings footer closes"
    );
    assert_eq!(notification_count(cx), before, "Settings footer is silent");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "Settings footer stores nothing"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_manage_sheet("Work", window, cx)
        });
    });
    paint_dialog_open(cx);

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("group-manage-ok", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_sheet(cx)),
        "Group manage footer closes"
    );
    assert_eq!(
        notification_count(cx),
        before,
        "Group manage footer is silent"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "Group manage footer stores nothing"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preset_attach_dialog(row.clone(), vec![row.id.clone()], window, cx)
        });
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("preset-attach-ok", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "picker footer closes"
    );
    assert_eq!(notification_count(cx), before, "picker footer is silent");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .store
            .get(&row_id)
            .expect("get works")
            .expect("row kept")
            .preset_id
            .clone()),
        None,
        "picker footer attaches nothing"
    );
}

#[gpui_kit::test]
fn honest_footers_add_creates_preset(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_presets_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = view.preset_form.clone().expect("manager stashes its form");
            form.name.update(cx, |state, cx| {
                state.set_value("ads", window, cx);
            });
            form.rules.update(cx, |state, cx| {
                state.set_value("[{\"action\":\"block\"}]", window, cx);
            });
        });
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("presets-dialog-ok", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Add closes the manager"
    );
    let names = view.read_with(cx, |view, _| {
        view.store
            .list_presets()
            .expect("list works")
            .iter()
            .map(|preset| preset.name.clone())
            .collect::<Vec<_>>()
    });
    assert!(
        names.contains(&"ads".to_string()),
        "preset stored: {names:?}"
    );
    assert_eq!(notification_count(cx), before + 1, "one honest toast");
}

#[gpui_kit::test]
fn logs_footer_commits_filter_while_cancel_discards(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_log_filter("boom".to_string(), cx);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.filter.clone()),
        "boom",
        "footer commit stores the filter"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("logs-dialog-ok").is_some()));
    assert!(cx.update(|window, _| window.try_find("logs-dialog-cancel").is_some()));
    cx.update(|window, cx| {
        window.click("logs-dialog-cancel", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_sheet(cx)),
        "Cancel closes"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.filter.clone()),
        "boom",
        "Cancel discards the edit instead of storing it"
    );
}

#[gpui_kit::test]
fn destructive_confirms_gate_on_named_affirmative(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);

    // K5: single-row delete no longer confirms — it is immediate with an
    // Undo toast; the other destructive classes below still confirm.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.delete_row_immediately(&row.id, window, cx)
        });
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "K5: single-row delete is immediate"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "the delete already happened"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_delete_confirm("default", window, cx)
        });
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "group delete confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_clear_traffic_confirm(window, cx));
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "traffic wipe confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t10_bulk_delete_result_reports_per_row_outcomes() {
    use manyproxy_ui::bulk_delete_result_message;
    let ok = bulk_delete_result_message(&["Alpha".to_string(), "Beta".to_string()], &[]);
    assert!(
        ok.contains("\"Alpha\"") && ok.contains("\"Beta\""),
        "names every removed row: {ok}"
    );
    let partial = bulk_delete_result_message(&["Alpha".to_string()], &["Beta".to_string()]);
    assert!(
        partial.contains("\"Beta\""),
        "names the failed row: {partial}"
    );
    assert!(
        partial.contains('1') && partial.contains("failed"),
        "counts both sides: {partial}"
    );
    let failed = bulk_delete_result_message(&[], &["Beta".to_string()]);
    assert!(
        failed.contains("\"Beta\"") && failed.contains("failed"),
        "total failure names the row: {failed}"
    );
    let empty = bulk_delete_result_message(&[], &[]);
    assert!(
        empty.contains('0'),
        "empty set reads zero, never a bare colon: {empty}"
    );
}

#[gpui_kit::test]
fn bulk_delete_single_is_immediate_and_multi_confirms(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    // The Undo click happens inside the toast's slide-in; reduce motion keeps
    // it hit-testable in the same frame the toast is pushed (K5 idiom).
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    // K17: names carry a ticket prefix - the undo stash is shared across
    // this process's tests and other suites delete rows named Work/Home
    // without consuming their batch (bug07 leaves one behind on purpose).
    let work = store
        .add(t30_engine_seed("K17 Work", "10981"))
        .expect("seed works");
    let home = store
        .add(t30_engine_seed("K17 Home", "10982"))
        .expect("seed works");
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

    // One row deletes at once: no dialog, and the Undo toast is the safety net.
    let before_single = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(std::slice::from_ref(&work.id), cx);
            view.delete_selected_batch(window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "one row deletes without a confirm"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "the single row is gone at once"
    );
    assert_eq!(
        notification_count(cx),
        before_single + 1,
        "one Undo toast acknowledges the delete"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K17 Work"),
        1,
        "the single row waits in memory for the toast lifetime"
    );
    paint_frame(cx);
    cx.update(|window, cx| window.click("k5-undo", cx));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        2,
        "Undo restores the single row"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K17 Work"),
        0,
        "the restored row leaves the stash"
    );
    assert!(
        view.read_with(cx, |view, _| view
            .store
            .get(&home.id)
            .expect("store reads")
            .is_some()),
        "the other row never left"
    );

    // Undo restores the row with a new id, not the selection; pick the
    // restored row again before F5.
    let restored_work = view.read_with(cx, |view, _| {
        view.store
            .list()
            .expect("list works")
            .into_iter()
            .find(|row| row.name == "K17 Work")
            .expect("Undo put the row back")
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(std::slice::from_ref(&restored_work.id), cx);
        });
    });
    // The toast button took the keyboard; F5 lives on the shell.
    cx.update(|window, cx| window.focus(&focus, cx));
    let keystroke = Keystroke::parse("f5").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "one endpoint serves before the bulk delete"
    );

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let all: Vec<String> = view
                .store
                .list()
                .expect("list works")
                .iter()
                .map(|row| row.id.clone())
                .collect();
            view.select_rows(&all, cx);
            // Two or more selected: one confirm names the rows before
            // anything is removed.
            view.delete_selected_batch(window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "two or more rows confirm before deleting"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        2,
        "the confirm leaves the store alone"
    );
    paint_frame(cx);
    cx.update(|window, cx| window.click("ok", cx));
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "the confirm closes after Delete"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "every selected row is removed"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "the running endpoint stopped first"
    );
    assert_eq!(
        notification_count(cx),
        before + 1,
        "one batched Undo toast for the whole set"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K17 Work"),
        1,
        "the removed row waits in memory for the toast lifetime"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K17 Home"),
        1,
        "every removed row waits, not just the first"
    );

    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("k5-undo", cx));
    cx.run_until_parked();
    let restored = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(
        restored.len(),
        2,
        "one Undo restores the whole selection: {:?}",
        restored
            .iter()
            .map(|row| row.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K17 Work"),
        0,
        "restored once, consumed once"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K17 Home"),
        0,
        "the batch is consumed together"
    );
}

#[gpui_kit::test]
fn bug04_bulk_stop_counts_only_real_stops_and_keeps_flag(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10981"))
        .expect("seed works");
    store
        .add(t30_engine_seed("Home", "10982"))
        .expect("seed works");
    let rows = store.list().expect("list works");
    for row in &rows {
        store.set_enabled(&row.id, true).expect("enable works");
    }
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let (mut engine, stock) = EngineController::fake();
    engine.start_all(&rows).expect("fake rows active");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.engine = engine);
    });

    let before = notification_count(cx);
    cx.update(|window, cx| {
        stock.fail_check("re-gen broke");
        view.update(cx, |view, cx| {
            view.select_rows(&ids, cx);
            view.stop_selected_batch(window, cx);
        });
    });
    assert_eq!(notification_count(cx), before + 1, "one outcome toast");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "failed stops keep serving"
    );
    for id in &ids {
        assert!(
            view.read_with(cx, |view, _| view
                .store
                .get(id)
                .expect("get works")
                .expect("row exists")
                .enabled),
            "the flag stays set for a row that did not stop"
        );
    }

    stock.clear_failures();
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&ids, cx);
            view.stop_selected_batch(window, cx);
        });
    });
    assert_eq!(notification_count(cx), before + 1, "one outcome toast");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "clean stops clear the engine"
    );
    for id in &ids {
        assert!(
            !view.read_with(cx, |view, _| view
                .store
                .get(id)
                .expect("get works")
                .expect("row exists")
                .enabled),
            "the flag is cleared only after a real stop"
        );
    }
}

#[gpui_kit::test]
fn bug07_bulk_delete_keeps_rows_whose_stop_failed(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10981"))
        .expect("seed works");
    store
        .add(t30_engine_seed("Home", "10982"))
        .expect("seed works");
    let rows = store.list().expect("list works");
    let targets: Vec<(String, String)> = rows
        .iter()
        .map(|row| (row.id.clone(), row.name.clone()))
        .collect();
    let (mut engine, stock) = EngineController::fake();
    engine.start_all(&rows).expect("fake rows active");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.engine = engine);
    });

    stock.fail_check("re-gen broke");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_bulk_delete(&targets, window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        2,
        "rows whose stop failed stay in the library"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "the engine still owns the endpoints, no orphans"
    );

    stock.clear_failures();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_bulk_delete(&targets, window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "clean stops allow the delete"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "delete stops the engine"
    );
}

#[gpui_kit::test]
fn bug07_prune_keeps_rows_whose_stop_failed(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::store::GroupPruneKind;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    for (name, port) in [("A", "10981"), ("B", "10982"), ("C", "10983")] {
        store.add(t30_engine_seed(name, port)).expect("seed works");
    }
    let rows = store.list().expect("list works");
    let (mut engine, stock) = EngineController::fake();
    engine.start_all(&rows).expect("fake rows active");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = engine;
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.select_visible_row(0, cx));
    });

    stock.fail_check("re-gen broke");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_prune(
                GroupPruneKind::Duplicates,
                manyproxy_core::DEFAULT_GROUP,
                window,
                cx,
            )
        });
    });
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        3,
        "a row that fails to stop is skipped, not deleted"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        3,
        "no orphaned listeners"
    );

    stock.clear_failures();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_prune(
                GroupPruneKind::Duplicates,
                manyproxy_core::DEFAULT_GROUP,
                window,
                cx,
            )
        });
    });
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "clean stops prune the duplicates"
    );
}

#[test]
fn t05_bind_guard_says_leaving_loopback_exposes_machine() {
    use manyproxy_ui::lan_hint_for_bind;
    let hint = lan_hint_for_bind("0.0.0.0").expect("LAN bind guards");
    assert!(
        hint.contains("Leaving loopback exposes this machine"),
        "plain-words guard, got {hint}"
    );
    assert!(hint.contains("0.0.0.0"), "names the bind, got {hint}");
    assert!(hint.contains("LAN confirm"), "names the gate, got {hint}");
    assert!(
        lan_hint_for_bind("127.0.0.1").is_none(),
        "loopback stays quiet"
    );
    assert!(lan_hint_for_bind("").is_none(), "blank stays quiet");
}

#[gpui_kit::test]
fn t05_export_caution_offers_safer_path_and_cancel_writes_nothing(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_export_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_painted(cx, "export-warning");
    assert!(
        cx.update(|window, _| window.try_find("export-safer-path").is_some()),
        "safer path paints"
    );
    assert_painted(cx, "export-preview");
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("export-dialog-cancel", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Cancel closes the dialog"
    );
    assert_eq!(
        notification_count(cx),
        before,
        "Cancel fires no toast and writes nothing"
    );
}

#[test]
fn t05_guided_label_and_hint_name_protocol_needs() {
    use manyproxy_ui::{guided_hint_line, guided_identity_label};
    assert_eq!(guided_identity_label("vless"), "Identity (uuid)");
    assert_eq!(guided_identity_label("trojan"), "Identity (password)");
    assert_eq!(guided_identity_label("bogus"), "Identity");
    assert_eq!(
        guided_hint_line("vless").as_deref(),
        Some("vless needs uuid.")
    );
    assert_eq!(
        guided_hint_line("tuic").as_deref(),
        Some("tuic needs uuid and password.")
    );
    assert_eq!(
        guided_hint_line("socks").as_deref(),
        Some("No secret needed for socks.")
    );
    assert_eq!(guided_hint_line("bogus"), None);
}

#[test]
fn t06_panel_plan_helpers_mirror_guided_pages() {
    use manyproxy_ui::{
        block_owns_identity, has_advanced_dialog, has_dial_panel, has_network_panel,
        secret_state_caption, tls_panel_plan,
    };
    for protocol in [
        "vless",
        "vmess",
        "trojan",
        "shadowsocks",
        "wireguard",
        "amneziawg",
        "hysteria",
        "hysteria2",
        "tuic",
        "anytls",
        "naive",
        "ssh",
        "snell",
        "xray",
    ] {
        assert!(
            block_owns_identity(protocol),
            "{protocol} owns its secret in the type box"
        );
    }
    for protocol in [
        "socks",
        "http",
        "https",
        "shadowtls",
        "direct",
        "chain",
        "extra-core",
        "mieru",
        "bogus",
    ] {
        assert!(
            !block_owns_identity(protocol),
            "{protocol} keeps the generic identity row"
        );
    }
    assert_eq!(tls_panel_plan("vless"), Some(false), "optional TLS box");
    assert_eq!(tls_panel_plan("http"), Some(false), "https shares http");
    assert_eq!(tls_panel_plan("anytls"), Some(true), "always-on TLS box");
    assert_eq!(tls_panel_plan("tuic"), Some(true), "forced TLS box");
    assert_eq!(tls_panel_plan("shadowsocks"), None, "mux-only settings");
    assert_eq!(tls_panel_plan("wireguard"), None, "endpoint parity");
    assert_eq!(tls_panel_plan("bogus"), None, "unknown has no TLS");
    assert!(has_network_panel("trojan"), "trojan takes Network");
    assert!(!has_network_panel("anytls"), "anytls has no transport");
    assert!(has_dial_panel("ssh"), "ssh takes dial fields");
    assert!(!has_dial_panel("xray"), "xray bridge has no dial");
    assert!(!has_dial_panel("chain"), "chain has no dial");
    assert!(has_advanced_dialog("anytls"), "dial plus TLS extras");
    assert!(has_advanced_dialog("socks"), "dial fields alone");
    assert!(!has_advanced_dialog("xray"), "nothing advanced on xray");
    assert!(!has_advanced_dialog("mieru"), "no guided page, no dialog");
    assert_eq!(secret_state_caption(""), "Not Set", "blank reads Not Set");
    assert_eq!(
        secret_state_caption("pw"),
        "Already Set",
        "presence never renders the value"
    );
}

#[gpui_kit::test]
fn t05_guided_add_without_json_creates_row(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("First", window, cx);
            });
            form.guided_server.update(cx, |state, cx| {
                state.set_value("198.51.100.9", window, cx);
            });
            form.guided_port.update(cx, |state, cx| {
                state.set_value("443", window, cx);
            });
            form.guided_identity.update(cx, |state, cx| {
                state.set_value("11111111-2222-4333-8444-555555555555", window, cx);
            });
            let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
            let box_text = form.params.read(cx).value().to_string();
            assert!(
                box_text.contains("\"uuid\":\"\""),
                "raw JSON untouched: {box_text}"
            );
            assert!(!*form.advanced.borrow(), "Advanced never opened");
            closed
        })
    });
    assert!(closed, "guided-valid commit closes");
    assert!(form_error.borrow().is_none(), "no inline error");
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(rows.len(), 1, "one row lands");
    assert!(rows[0].params.contains("198.51.100.9"), "server folds in");
    assert!(
        rows[0]
            .params
            .contains("11111111-2222-4333-8444-555555555555"),
        "identity folds in"
    );
}

/// F12: a successful Add confirms by selecting the row it created.
#[gpui_kit::test]
fn f12_add_selects_the_created_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let existing = store
        .add(row19_seed("Existing", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &existing.id);

    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = guided_add_form("Fresh", window, cx);
            view.commit_add_with_focus(&form, &form_error, window, cx)
        })
    });
    assert!(closed, "the guided form commits and closes");

    let (created, selected) = view.read_with(cx, |view, _cx| {
        let rows = view.store.list().expect("list reads");
        let created = rows
            .iter()
            .find(|row| row.name == "Fresh")
            .expect("created row persists")
            .id
            .clone();
        (created, view.selection_focus.clone())
    });
    assert_eq!(
        selected.as_deref(),
        Some(created.as_str()),
        "Add lands on the row it created"
    );
}

/// F12: a filter that hides the new row cannot land on it, so the previous
/// selection must stay exactly where the user left it.
#[gpui_kit::test]
fn f12_add_under_excluding_filter_keeps_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_window, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("alpha".to_string(), cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &alpha.id);

    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = guided_add_form("Fresh", window, cx);
            view.commit_add_with_focus(&form, &form_error, window, cx)
        })
    });
    assert!(closed, "the guided form commits and closes");
    let selected = view.read_with(cx, |view, _cx| view.selection_focus.clone());
    assert_eq!(
        selected.as_deref(),
        Some(alpha.id.as_str()),
        "a hidden new row leaves the selection unchanged"
    );
}

/// F5: the guided gate names the empty row before `store.add`, so the dialog
/// can point at the UUID input instead of opening the raw JSON textarea.
#[gpui_kit::test]
fn f5_guided_blank_uuid_names_the_field_and_focuses_it(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    use gpui_kit::Focusable as _;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let (closed, focused, advanced) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("Half", window, cx);
            });
            form.guided_server.update(cx, |state, cx| {
                state.set_value("198.51.100.9", window, cx);
            });
            let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
            let handle = form.vless_uuid.read(cx).focus_handle(cx);
            let focused = window.focused(cx).as_ref() == Some(&handle);
            let advanced = *form.advanced.borrow();
            (closed, focused, advanced)
        })
    });
    assert!(!closed, "invalid commit keeps the dialog open");
    let (field, message) = form_error.borrow().clone().expect("inline error recorded");
    assert_eq!(field, "uuid", "the error names the UUID row");
    assert_eq!(message, "UUID is required");
    assert!(focused, "focus lands on the UUID input");
    assert!(
        !advanced,
        "a guided protocol never opens the raw JSON textarea"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
}

/// F5: the guided error paints under its own row, not at the form foot.
#[gpui_kit::test]
fn f5_guided_error_renders_beside_the_uuid_row(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    use gpui_kit::ParentElement as _;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form = cx.update(|window, cx| view.update(cx, |_, cx| ConfigForm::blank(window, cx)));
    let element_view = view.clone();
    cx.update(|window, cx| {
        manyproxy_ui::open_app_dialog(window, cx, move |dialog, _, cx| {
            let error = Some(("uuid".to_string(), "UUID is required".to_string()));
            dialog
                .title("Add proxy configuration")
                .w(px(560.))
                .child(
                    form.clone()
                        .element(element_view.clone(), cx, None, error, false, None),
                )
                .footer(manyproxy_ui::dialog_footer(
                    "uuid-dialog-ok",
                    "uuid-dialog-cancel",
                    "Add",
                    Rc::new(|_: &mut Window, _: &mut gpui_kit::App| false),
                ))
        });
    });
    paint_dialog_open(cx);
    assert_painted(cx, "vless-uuid");
    assert_painted(cx, "uuid-field-error");
    let y_of = |id: &str, cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
                .origin
                .y
        })
    };
    let row = y_of("vless-uuid", cx);
    let error = y_of("uuid-field-error", cx);
    assert!(
        error > row,
        "the error sits under the UUID row ({row:?} < {error:?})"
    );
    assert_absent(cx, "form-field-error".to_string());
}

/// F5: server and port are guided inputs too — each names itself.
#[gpui_kit::test]
fn f5_guided_blank_server_and_port_name_their_rows(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let run = |name: &str, server: &str, port: &str, cx: &mut VisualTestContext| {
        let field = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let form = ConfigForm::blank(window, cx);
                form.name.update(cx, |state, cx| {
                    state.set_value(name, window, cx);
                });
                form.guided_server.update(cx, |state, cx| {
                    state.set_value(server, window, cx);
                });
                form.guided_port.update(cx, |state, cx| {
                    state.set_value(port, window, cx);
                });
                assert!(!view.commit_add_with_focus(&form, &form_error, window, cx));
            });
            form_error
                .borrow()
                .clone()
                .expect("inline error recorded")
                .0
        });
        form_error.borrow_mut().take();
        field
    };
    assert_eq!(run("Half", "", "443", cx), "guided_server");
    assert_eq!(run("Half", "198.51.100.9", "0", cx), "guided_port");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
}

/// F5(a): the rendered dialog puts the protocol identity between Common and
/// Advanced, so expanding Advanced never hides the UUID.
#[gpui_kit::test]
fn f5_dialog_orders_identity_between_common_and_advanced(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let y_of = |id: &str, cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
                .origin
                .y
        })
    };
    let common = y_of("common-section", cx);
    let identity = y_of("vless-uuid", cx);
    let advanced = y_of("advanced-settings-caption", cx);
    assert!(
        common < identity && identity < advanced,
        "order is Common ({common:?}) -> identity ({identity:?}) -> Advanced ({advanced:?})"
    );
}

/// F5(c): the kit owns the dialog scrollbar, so the theme sets its mode — a
/// body that overflows keeps its scrollbar instead of fading after two seconds.
#[gpui_kit::test]
fn f5_scrollbar_mode_stays_visible(cx: &mut TestAppContext) {
    use gpui_kit::component::scroll::ScrollbarMode;
    use gpui_kit::component::Theme;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let _ = mount_view(store, cx);
    cx.update(|cx| {
        assert_eq!(Theme::global(cx).scrollbar_mode, ScrollbarMode::Always);
    });
}

/// F17: the modal scrim paints `Theme::overlay`, so this reads the exact value
/// the backdrop draws. Light reads as a backdrop (15%); dark keeps the kit's
/// 20% and survives the light override rather than inheriting it.
#[gpui_kit::test]
fn f17_light_modal_scrim_alpha(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;
    cx.update(gpui_kit::init);
    cx.update(|cx| manyproxy_ui::apply_appearance("light", manyproxy_ui::store::Accent::Blue, None, cx));
    cx.update(|cx| {
        let overlay = Theme::global(cx).overlay;
        assert_eq!(
            overlay.a,
            manyproxy_ui::LIGHT_MODAL_OVERLAY_ALPHA,
            "light scrim uses the 15% alpha"
        );
        assert!(
            overlay.h == 0. && overlay.s == 0. && overlay.l == 0.,
            "scrim stays black, got {overlay:?}"
        );
    });
    cx.update(|cx| manyproxy_ui::apply_appearance("dark", manyproxy_ui::store::Accent::Blue, None, cx));
    cx.update(|cx| {
        let alpha = Theme::global(cx).overlay.a;
        assert!(
            (alpha - 0.2).abs() < 1e-6,
            "dark keeps the kit's 20% scrim, got {alpha}"
        );
    });
}

#[gpui_kit::test]
fn t05_edit_prefills_guided_and_collapses_advanced(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &seeded, None);
            assert_eq!(
                form.guided_server.read(cx).value().to_string(),
                "198.51.100.9"
            );
            assert_eq!(form.guided_port.read(cx).value().to_string(), "443");
            assert_eq!(
                form.guided_identity.read(cx).value().to_string(),
                "11111111-2222-4333-8444-555555555555"
            );
            assert!(
                !*form.advanced.borrow(),
                "K2: Advanced Settings starts collapsed"
            );
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(seeded, window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        cx.update(|window, _| window.try_find("field-guided-server").is_some()),
        "guided rows stay visible in the edit dialog"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-params").is_none()),
        "K2: guided protocols never show the raw JSON textarea"
    );
    assert_painted(cx, "advanced-settings-caption");
    assert!(
        cx.update(|window, _| window.try_find("advanced-settings-section").is_none()),
        "K2: the Advanced Settings disclosure starts collapsed"
    );
    cx.update(|window, cx| window.click("advanced-settings", cx));
    paint_dialog_open(cx);
    assert_painted(cx, "advanced-settings-section");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t05_import_commit_never_starts_endpoints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog(
                "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
                "travel",
                None,
                window,
                cx,
            );
        });
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        window.click("import-dialog-ok", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "OK click closes the dialog"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "one row lands"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "import never starts endpoints"
    );
}

#[gpui_kit::test]
fn t05_lan_start_gates_with_naming_confirm(cx: &mut TestAppContext) {
    use manyproxy_ui::{lan_confirm_text, lan_exposure_warning};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_lan_input("Lan")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            view.start_selected_engine(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "LAN start stops at the confirm"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts before confirm"
    );
    let confirm = lan_confirm_text("Lan", "0.0.0.0");
    assert!(
        confirm.contains("\"Lan\"") && confirm.contains("0.0.0.0"),
        "confirm names config + bind: {confirm}"
    );
    let warning = lan_exposure_warning("Lan", "0.0.0.0").expect("non-loopback warns post-start");
    assert!(
        warning.contains("\"Lan\"") && warning.contains("0.0.0.0"),
        "post-start warning names config + bind: {warning}"
    );
}

#[gpui_kit::test]
fn library_paints_two_stored_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);
    assert_painted(cx, &format!("library-row-{}", first.id));
    assert_painted(cx, &format!("library-row-{}", second.id));
}

#[gpui_kit::test]
fn library_rows_paint_flat_without_gaps(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    let first_card = format!("row-surface-{}", first.id);
    let second_card = format!("row-surface-{}", second.id);
    assert_painted(cx, &first_card);
    assert_painted(cx, &second_card);
    let (first_bounds, second_bounds) = cx.update(|window, _| {
        let first = window
            .try_find(first_card)
            .expect("first row surface finds")
            .bounds();
        let second = window
            .try_find(second_card)
            .expect("second row surface finds")
            .bounds();
        (first, second)
    });
    let pitch = manyproxy_ui::Density::Comfortable.pitch();
    assert_eq!(
        first_bounds.size.height.as_f32(),
        pitch,
        "the first row fills the density pitch"
    );
    assert_eq!(
        second_bounds.size.height.as_f32(),
        pitch,
        "the second row fills the density pitch"
    );
    assert_eq!(
        second_bounds.origin.y.as_f32(),
        first_bounds.origin.y.as_f32() + pitch,
        "flat rows meet on the hairline: first {first_bounds:?}, second {second_bounds:?}"
    );
}

#[gpui_kit::test]
fn library_row_lamp_paints_in_every_state(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                state.delegate_mut().running.insert(seeded.id.clone());
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, &format!("row-lead-{}", seeded.id));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                state.delegate_mut().running.remove(&seeded.id);
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, &format!("row-lead-{}", seeded.id));
}

#[gpui_kit::test]
fn t02_row_anatomy_uniform_across_states(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let running = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let stopped = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let dead = store
        .add(row19_seed("Lab", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                let delegate = state.delegate_mut();
                delegate.running.insert(running.id.clone());
                delegate.running.insert(dead.id.clone());
                delegate
                    .health
                    .insert(running.id.clone(), "87ms".to_string());
                delegate.health.insert(dead.id.clone(), "dead".to_string());
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    for row in [&running, &stopped, &dead] {
        assert_painted(cx, &format!("row-surface-{}", row.id));
        assert_painted(cx, &format!("row-lead-{}", row.id));
        assert_painted(cx, &format!("row-name-text-{}", row.id));
        assert_painted(cx, &format!("row-endpoint-chip-{}", row.id));
        assert_painted(cx, &format!("row-health-{}", row.id));
        assert_painted(cx, &format!("row-group-{}", row.id));
        // The lead is status only: no checkbox rides the row.
        assert_absent(cx, format!("check-{}", row.id));
    }

    let (lamp_x, card_h): (Vec<_>, Vec<_>) = cx.update(|window, _| {
        [&running, &stopped, &dead]
            .iter()
            .map(|row| {
                let lamp = window
                    .try_find(format!("row-lead-{}", row.id))
                    .expect("lamp finds")
                    .bounds();
                let card = window
                    .try_find(format!("row-surface-{}", row.id))
                    .expect("card finds")
                    .bounds();
                (lamp.origin.x, card.size.height)
            })
            .unzip()
    });
    assert!(
        lamp_x.windows(2).all(|pair| pair[0] == pair[1]),
        "lamps align in one column: {lamp_x:?}"
    );
    assert!(
        card_h.windows(2).all(|pair| pair[0] == pair[1]),
        "cards share one height: {card_h:?}"
    );

    // Health is the serving state's word; a stopped row keeps the empty cell
    // and lets the gray lead carry the state, so the grid never shifts.
    assert_painted(cx, &format!("row-health-{}", running.id));
    assert_painted(cx, &format!("row-health-{}", stopped.id));
    assert_painted(cx, &format!("row-health-{}", dead.id));

    // Protocol and Upstream are opt-in columns. The default row paints neither.
    assert_painted(cx, &format!("row-endpoint-chip-{}", running.id));
    assert_absent(cx, format!("row-upstream-{}", running.id));
    assert_absent(cx, format!("row-protocol-{}", running.id));

    // Clicking the name selects the row; the lead is not a target.
    cx.update(|window, cx| {
        window.click(format!("row-name-text-{}", stopped.id), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.is_selected(&stopped.id)),
        "the name click selects its row"
    );
}

#[gpui_kit::test]
fn t02_row_anatomy_stress_stays_uniform(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for i in 0..50 {
        let row = store
            .add(row19_seed(
                &format!("node-{i}"),
                &format!("h{i}.example.com"),
                &format!("{}", 11_000 + i),
            ))
            .expect("seed works");
        ids.push(row.id.clone());
        let _ = row;
    }
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                let delegate = state.delegate_mut();
                for (i, id) in ids.iter().enumerate() {
                    if i % 3 == 0 {
                        delegate.running.insert(id.clone());
                        delegate.health.insert(
                            id.clone(),
                            if i == 21 {
                                "dead".to_string()
                            } else {
                                "12ms".to_string()
                            },
                        );
                    }
                }
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    fn painted_cards(cx: &mut VisualTestContext, ids: &[String]) -> Vec<(String, String)> {
        cx.update(|window, _| {
            ids.iter()
                .filter_map(|id| window.try_find(format!("row-surface-{id}")))
                .map(|snapshot| {
                    let bounds = snapshot.bounds();
                    (
                        format!("{:?}", bounds.origin.x),
                        format!("{:?}", bounds.size),
                    )
                })
                .collect()
        })
    }
    fn assert_column(label: &str, cx: &mut VisualTestContext, ids: &[String]) {
        let cards = painted_cards(cx, ids);
        assert!(cards.len() >= 2, "{label}: a window of rows paints");
        assert!(
            cards.windows(2).all(|pair| pair[0] == pair[1]),
            "{label}: no ragged cards or overflow: {cards:?}"
        );
        let lamps = cx.update(|window, _| {
            ids.iter()
                .filter(|id| window.try_find(format!("row-surface-{id}")).is_some())
                .filter(|id| window.try_find(format!("row-lead-{id}")).is_some())
                .count()
        });
        assert_eq!(lamps, cards.len(), "{label}: every card keeps its lamp");
    }
    assert_column("full library", cx, &ids);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("node-4".to_string(), cx)
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_column("filtered library", cx, &ids);
}

/// T-05: latency buckets are inclusive, compose with the search and the
/// other facets, and the unprobed toggle is an independent predicate.
#[test]
fn t05_latency_facet_filters_inclusively_and_composes() {
    use manyproxy_ui::{FilterQuery, LatencyBucket, LatencyFacet, StatusFacet};

    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        t77_row("fast", "Fast", "vless", "travel", 10801),
        t77_row("edge", "Edge", "vless", "travel", 10802),
        t77_row("slow", "Slow", "vmess", "team", 10803),
        t77_row("unprobed", "Unprobed", "vless", "team", 10804),
        t77_row("toolow", "TooLow", "vless", "travel", 10805),
    ]);
    delegate.set_latency(t77_map_u64(&[
        ("fast", 100),
        ("edge", 300),
        ("slow", 1000),
        ("toolow", 1001),
    ]));
    delegate.rows[3].enabled = false;

    let mut query = FilterQuery {
        latency: LatencyFacet {
            bucket: LatencyBucket::Ms100,
            include_unprobed: true,
        },
        ..FilterQuery::default()
    };
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Fast", "Unprobed"],
        "exactly 100 ms is in the bucket and unprobed defaults in"
    );

    query.latency = LatencyFacet {
        bucket: LatencyBucket::Ms300,
        include_unprobed: true,
    };
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Fast", "Edge", "Unprobed"]
    );

    query.latency = LatencyFacet {
        bucket: LatencyBucket::Ms1000,
        include_unprobed: true,
    };
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Fast", "Edge", "Slow", "Unprobed"],
        "1000 ms is in; 1001 ms is out"
    );

    query.latency = LatencyFacet {
        bucket: LatencyBucket::Ms300,
        include_unprobed: false,
    };
    assert_eq!(test_names(&delegate, &query), vec!["Fast", "Edge"]);

    query.protocols.insert("vmess".to_string());
    query.set_search("vless");
    assert!(
        test_visible(&delegate, &query).is_empty(),
        "search AND protocol AND latency all have to agree"
    );

    query.protocols.clear();
    delegate.set_running(vec!["fast".to_string()]);
    query.statuses.insert(StatusFacet::Running);
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Fast"],
        "the status facet narrows the latency survivors"
    );

    query = FilterQuery::default();
    assert_eq!(
        test_names(&delegate, &query).len(),
        5,
        "Clear restores the library"
    );

    query.latency = LatencyFacet {
        bucket: LatencyBucket::Any,
        include_unprobed: false,
    };
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Fast", "Edge", "Slow", "TooLow"],
        "excluding unprobed filters even without a bucket"
    );
    assert!(query.is_active(), "the toggle alone is a live facet");
    assert_eq!(query.long_tail_count(), 1);
}

/// The picker writes the session query; a stored latency facet from an older
/// build never seeds the launch.
#[gpui_kit::test]
fn t05_latency_facet_filters_and_clears(cx: &mut TestAppContext) {
    use manyproxy_ui::{LatencyBucket, LatencyFacet};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let fast = store
        .add(facet_seed("Fast", "vless", "travel", "10801"))
        .expect("seed works");
    let mid = store
        .add(facet_seed("Mid", "vmess", "travel", "10802"))
        .expect("seed works");
    let unprobed = store
        .add(facet_seed("Unprobed", "vless", "team", "10803"))
        .expect("seed works");
    store
        .set_latency_facet(LatencyFacet {
            bucket: LatencyBucket::Ms300,
            include_unprobed: false,
        })
        .expect("old facet writes");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.query.latency),
        LatencyFacet::default(),
        "a stored facet never seeds the launch"
    );

    view.update(cx, |view, cx| {
        view.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_latency(t77_map_u64(&[
                (fast.id.as_str(), 100),
                (mid.id.as_str(), 300),
            ]));
            list_cx.notify();
        });
        view.set_latency_facet(
            LatencyFacet {
                bucket: LatencyBucket::Ms100,
                include_unprobed: true,
            },
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, &format!("library-row-{}", fast.id));
    assert_absent(cx, format!("library-row-{}", mid.id));
    assert_painted(cx, &format!("library-row-{}", unprobed.id));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_latency_facet(
                LatencyFacet {
                    bucket: LatencyBucket::Ms100,
                    include_unprobed: false,
                },
                cx,
            );
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, format!("library-row-{}", unprobed.id));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.clear_query(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.query.latency),
        LatencyFacet::default()
    );
    for row in [&fast, &mid, &unprobed] {
        assert_painted(cx, &format!("library-row-{}", row.id));
    }
}

#[gpui_kit::test]
fn dead_row_health_section_renders_alert(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, ProbeSuccess, TestOutcome};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, _cx| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: seeded.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: None,
                    error: Some("timeout".to_string()),
                },
                success: None,
            });
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    // Slice 6: all sections ride one scroll, so the frame must be tall enough
    // to show the Health section without scrolling.
    paint_frame_at(cx, 1400., 1600.);

    assert_painted(cx, "config-health-alert");
    assert_painted(cx, "config-health");

    cx.update(|_, cx| {
        view.update(cx, |view, _cx| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: seeded.id.clone(),
                    at: 2,
                    kind: HealthKind::UrlTest,
                    latency_ms: Some(12),
                    error: None,
                },
                success: Some(ProbeSuccess {
                    latency_ms: 12,
                    bytes_rx: 0,
                    bytes_tx: 0,
                }),
            });
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "config-health");
    assert_absent(cx, "config-health-alert".to_string());
}

#[gpui_kit::test]
fn health_pane_carries_failure_and_row_actions(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, TestOutcome};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            cx.notify();
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, _cx| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: seeded.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: None,
                    error: Some("timeout".to_string()),
                },
                success: None,
            });
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);

    // Slice 6: the five tabs are one stacked scroll. Every section — and the
    // tab strip itself — is accounted for in a single frame.
    assert_absent(cx, "inspector-tabs".to_string());
    for id in [
        "inspector-identity",
        "config-endpoints",
        "inspector-health-section",
        "inspector-actions",
        "inspector-console-link",
    ] {
        assert_painted(cx, id);
    }

    assert_painted(cx, "config-health-alert");
    assert_painted(cx, "config-health");
    assert!(
        cx.update(|window, _| window.try_find("config-history".to_string()).is_some()),
        "config-history renders in the Health section"
    );

    // K1/A4-8: the stacked inspector still rides the single-tier operate row.
    for id in ["engine-start", "engine-stop", "health-test", "row-menu"] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} operates in the Actions section"
        );
    }

    assert_absent(cx, "config-preset".to_string());
}

#[gpui_kit::test]
fn laneb_stacked_inspector_shares_detail_header(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "detail-acting-on");
    assert_painted(cx, "detail-serving");
    assert_painted(cx, "config-endpoints");
    assert!(
        cx.update(|window, _| window.try_find("detail-operate".to_string()).is_some()),
        "detail-operate renders in the stacked scroll"
    );
}

/// F16: the table is the single source. Ids are unique and round-trip, every
/// palette row points at a real action, no two actions claim one keystroke,
/// and rows past a group's end resolve to nothing.
#[test]
fn action_table_is_the_single_source() {
    use manyproxy_ui::app::actions::{action, palette_action_at, ACTIONS, PALETTE_GROUPS};
    use gpui_kit::component::IndexPath;
    use std::collections::HashMap;

    let mut ids = std::collections::HashSet::new();
    let mut keystrokes: HashMap<&str, &str> = HashMap::new();
    for def in ACTIONS {
        assert!(ids.insert(def.id), "duplicate action id {}", def.id);
        assert_eq!(action(def.id).id, def.id, "{} round-trips", def.id);
        assert!(
            !def.label().is_empty(),
            "{} needs a label every surface can show",
            def.id
        );
        for &(keystroke, _) in def.shortcuts {
            assert!(
                gpui_kit::Keystroke::parse(keystroke).is_ok(),
                "{} binds an unparseable keystroke {keystroke}",
                def.id
            );
            if let Some(other) = keystrokes.insert(keystroke, def.id) {
                panic!("{keystroke} is claimed by both {other} and {}", def.id);
            }
        }
    }
    for (section, (label, group)) in PALETTE_GROUPS.iter().enumerate() {
        for (row, id) in group.iter().enumerate() {
            assert_eq!(
                palette_action_at(IndexPath::new(row).section(section)),
                Some(*id),
                "{label} row {row}"
            );
            assert!(
                ids.contains(id),
                "palette {label} row {row} names a real action"
            );
        }
        assert_eq!(
            palette_action_at(IndexPath::new(group.len()).section(section)),
            None,
            "{label} ends after its last row"
        );
    }
    assert_eq!(
        palette_action_at(IndexPath::new(0).section(PALETTE_GROUPS.len())),
        None,
        "no group exists past the table's last section"
    );
}

/// F16: the palette, the row's context menu and the inspector's `…` menu all
/// read one `ActionDef`, and the keymap binds the keystroke that same def
/// carries — so "Copy URLs" is one verb with one label and one enabled state.
#[gpui_kit::test]
fn f16_palette_menus_and_keys_share_one_action(cx: &mut TestAppContext) {
    use manyproxy_ui::app::actions::{action, palette_action_at, ActionTarget, PALETTE_GROUPS};
    use gpui_kit::component::IndexPath;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    let def = action("copy-urls");
    let palette_row = PALETTE_GROUPS[0]
        .1
        .iter()
        .position(|id| *id == def.id)
        .expect("the palette lists Copy URLs");
    assert_eq!(
        palette_action_at(IndexPath::new(palette_row).section(0)),
        Some(def.id),
        "the palette row resolves to the table id"
    );
    assert_eq!(
        def.shortcuts.first().map(|(keystroke, _)| *keystroke),
        Some("ctrl-u"),
        "the keymap binds the def's keystroke"
    );
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(
            (def.enabled)(view, ActionTarget::Row(&row.id), cx),
            "a row target enables the verb"
        );
        assert!(
            (def.enabled)(view, ActionTarget::Selection, cx),
            "the selection enables it too"
        );
    });

    let card = format!("row-surface-{}", row.id);
    cx.update(|window, cx| window.right_click(card, cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    // Copy URLs sits in the card menu's `More ▾` submenu: one Up opens it.
    cx.update(|window, cx| {
        let up = gpui_kit::Keystroke::parse("up").expect("test keystroke parses");
        window.dispatch_keystroke(up, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    let ctx_id = format!("rowctx-{}-{}", def.id, row.id);
    assert_eq!(
        label_of(&ctx_id, cx),
        def.label(),
        "the context menu shows the table's label"
    );
    let escape = || gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(escape(), cx));
    cx.run_until_parked();
    paint_frame(cx);

    open_inspector(&view, cx);
    paint_frame_at(cx, 1000., 1600.);
    cx.run_until_parked();
    cx.update(|window, cx| window.click("row-menu", cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    let kebab = format!("kebab-{}", def.id);
    assert_eq!(
        label_of(&kebab, cx),
        def.label(),
        "the inspector menu shows the table's label"
    );
    cx.update(|window, cx| window.dispatch_keystroke(escape(), cx));
    cx.run_until_parked();
    paint_frame(cx);

    // The palette runs the very same handler.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(IndexPath::new(palette_row).section(0), window, cx);
        });
    });
    cx.run_until_parked();
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert!(
        clip.contains("socks5://") && clip.contains('\n'),
        "the palette row ran copy-urls: {clip:?}"
    );

    // enabled-when is the table's too: no selection, no selection target.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection(window, cx);
        });
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(
            !(def.enabled)(view, ActionTarget::Selection, cx),
            "no selection refuses the selection target"
        );
        assert!(
            (def.enabled)(view, ActionTarget::Row(&row.id), cx),
            "the context menu's row target still runs"
        );
    });
}

#[gpui_kit::test]
fn command_palette_opens_and_runs_add(cx: &mut TestAppContext) {
    use gpui_kit::component::IndexPath;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "palette opens a dialog"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(IndexPath::new(0).section(0), window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("add-dialog-ok").is_some()),
        "Add op opens the Add dialog"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

/// F8: the palette stopped advertising commands the app does not run. The
/// "Show Presets tab" row is gone and "Clear selection" (which cleared
/// checkboxes) is "Clear checked". The filter reads labels AND keywords, so
/// the query counts below are the label truth, not a snapshot of pixels.
#[gpui_kit::test]
fn f08_palette_rows_stop_lying(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    let state = view.read_with(cx, |view, _| view.palette.clone().expect("palette state"));
    let matched = |cx: &mut VisualTestContext, query: &str| -> usize {
        cx.update(|window, cx| {
            state.update(cx, |state, cx| state.set_query(query, window, cx));
        });
        cx.run_until_parked();
        paint_frame(cx);
        state.read_with(cx, |state, _| state.matched_count())
    };

    assert_eq!(
        matched(cx, "presets"),
        1,
        "only Routing presets matches: the Show Presets tab row is gone"
    );
    assert_eq!(
        matched(cx, "selection"),
        1,
        "the Clear selection row matches on its label"
    );
    assert_eq!(matched(cx, "checked"), 0, "no row says 'checked' any more");
    assert_eq!(matched(cx, "uncheck"), 0, "no row says 'uncheck' any more");
    assert_eq!(
        matched(cx, "escape"),
        1,
        "the Esc binding keyword still finds the row"
    );
    // The surviving row is the ClearSelection op, at its renumbered path.
    assert_eq!(
        manyproxy_ui::AppView::palette_op_at(gpui_kit::component::IndexPath::new(10).section(0)),
        Some("clear-selection"),
        "Clear selection stays on the op it names"
    );
}

/// F10: the palette states its keys, and every row that really is bound wears
/// that key as a chip. The chip is row content (an action would dispatch
/// twice), so the chip's own debug id is the proof it painted.
#[gpui_kit::test]
fn f10_palette_footer_and_bound_row_chips(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, "palette-footer");
    assert_eq!(
        label_of("palette-footer", cx),
        manyproxy_ui::app::open::palette_footer_hint(),
        "the footer spells the keys that move, run and close the palette"
    );
    assert_eq!(
        manyproxy_ui::app::open::palette_footer_hint(),
        "↑↓ move · Enter run · Esc close"
    );

    const BOUND_CHIPS: [&str; 13] = [
        "kbd:ctrl-n",
        "kbd:ctrl-o",
        "kbd:f5",
        "kbd:shift-f5",
        "kbd:ctrl-t",
        "kbd:ctrl-u",
        "kbd:ctrl-e",
        "kbd:delete",
        "kbd:ctrl-f",
        "kbd:escape",
        "kbd:ctrl-1",
        "kbd:ctrl-2",
        "kbd:ctrl-v",
    ];
    // The palette opens on the whole list, so the first rows and their chips
    // are the ones on screen.
    assert!(
        cx.debug_bounds("kbd:ctrl-n").is_some(),
        "Add configuration wears Ctrl+N"
    );
    assert!(
        cx.debug_bounds("kbd:ctrl-o").is_some(),
        "Import configurations wears Ctrl+O"
    );

    let state = view.read_with(cx, |view, _| view.palette.clone().expect("palette state"));
    let filter = |cx: &mut VisualTestContext, query: &str| {
        cx.update(|window, cx| {
            state.update(cx, |state, cx| state.set_query(query, window, cx));
        });
        cx.run_until_parked();
        paint_frame(cx);
        state.read_with(cx, |state, _| state.matched_count())
    };

    assert_eq!(filter(cx, "add"), 1, "only Add configuration matches");
    assert!(
        cx.debug_bounds("kbd:ctrl-n").is_some(),
        "the chip rides along when the row is filtered to the top"
    );

    // Export library is the one Library row with no key of its own.
    assert_eq!(filter(cx, "export"), 1, "only Export library matches");
    for chip in BOUND_CHIPS {
        assert!(
            cx.debug_bounds(chip).is_none(),
            "{chip} stays off a row that has no binding"
        );
    }
}

#[gpui_kit::test]
fn t08_reference_flows_open_as_sheets(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_manage_sheet("Work", window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "Group manage opens a sheet"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Group manage opens no dialog"
    );
    assert_painted(cx, "group-manage-ok");
    assert!(
        cx.update(|window, _| window.try_find("config-list").is_some()),
        "library stays behind the sheet"
    );

    cx.update(|window, cx| {
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_sheet(cx)),
        "Escape closes the sheet"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "Logs opens a sheet"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Logs opens no dialog"
    );
    assert_painted(cx, "logs-dialog-ok");
    cx.update(|window, cx| window.close_sheet(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "Subscriptions opens a sheet"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Subscriptions opens no dialog"
    );
    assert_painted(cx, "subscriptions-dialog-ok");
    cx.update(|window, cx| window.close_sheet(cx));
}

/// F13: the sheet header close is a kit icon-only button, so it reads as an exit
/// only when the kit asset source is registered (main does that). Pins both
/// halves: the icon bytes resolve, and the control paints real bounds.
#[gpui_kit::test]
fn f13_sheet_header_close_paints_and_closes(cx: &mut TestAppContext) {
    use gpui::AssetSource as _;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    let icon = gpui_kit::assets::Assets
        .load("icons/close.svg")
        .expect("the kit asset source answers")
        .expect("icons/close.svg ships with the kit");
    assert!(!icon.is_empty(), "the close icon has bytes to paint");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "Logs opens a sheet"
    );
    assert_painted(cx, "close");

    cx.update(|window, cx| window.click("close", cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_sheet(cx)),
        "the header close is a working exit"
    );
}

#[gpui_kit::test]
fn t08_palette_menu_only_ops_dispatch_same_handlers(cx: &mut TestAppContext) {
    use manyproxy_ui::app::actions::PALETTE_GROUPS;
    use gpui_kit::component::IndexPath;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);

    // Resolve by id: the palette is ordered by group, and ids survive the
    // retirement of the stats verbs.
    let palette_row = |id: &str| {
        PALETTE_GROUPS
            .iter()
            .enumerate()
            .find_map(|(section, (_, ids))| {
                ids.iter()
                    .position(|entry| *entry == id)
                    .map(|row| IndexPath::new(row).section(section))
            })
            .unwrap_or_else(|| panic!("{id} is in the palette"))
    };

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(palette_row("export-library"), window, cx)
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "palette Export opens the export dialog"
    );
    assert_painted(cx, "export-warning");
    assert_painted(cx, "export-preview");
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(palette_row("paste-import"), window, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        before + 1,
        "empty clipboard warns once"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "no dialog on empty clipboard"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_sheet(cx)),
        "no sheet on empty clipboard"
    );
}

#[gpui_kit::test]
fn t08_engine_bar_regions_hold_order(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "engine-bar-status",
        "engine-bar-running",
        "engine-bar-traffic",
    ] {
        assert_painted(cx, id);
    }
    let origins = cx.update(|window, _| {
        [
            "engine-bar-status",
            "engine-bar-running",
            "engine-bar-traffic",
        ]
        .iter()
        .map(|id| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
                .origin
                .x
        })
        .collect::<Vec<_>>()
    });
    assert!(
        origins[0] < origins[1] && origins[1] < origins[2],
        "state/running/traffic read in order: {origins:?}"
    );
}

#[gpui_kit::test]
fn t08_palette_button_owns_the_hint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    // The retired status strip carried a Ctrl-K chip. The palette button
    // replaces it: the binding lives in the button's tooltip, not in a
    // second affordance on the bar.
    assert_painted(cx, "action-bar-palette");
    assert_absent(cx, "palette-hint".to_string());
    assert_eq!(
        label_of("action-bar-palette", cx),
        "Command palette",
        "the palette button names itself"
    );
    let (bar, button) = cx.update(|window, _| {
        let bar = window
            .try_find("action-bar".to_string())
            .unwrap_or_else(|| panic!("action-bar renders"))
            .bounds();
        let button = window
            .try_find("action-bar-palette".to_string())
            .unwrap_or_else(|| panic!("action-bar-palette renders"))
            .bounds();
        (bar, button)
    });
    assert!(
        button.origin.x >= bar.origin.x
            && button.origin.x + button.size.width <= bar.origin.x + bar.size.width + px(1.),
        "the palette button lives inside the action bar ({button:?} in {bar:?})"
    );
}

#[gpui_kit::test]
fn t08_refresh_batch_progress_and_streaming_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add_subscription("Nightly", "https://example.com/a.txt", "travel", true)
        .expect("seed works");
    let second = store
        .add_subscription("Daily", "https://example.com/b.txt", "travel", true)
        .expect("seed works");
    let (a, b) = (first.id.clone(), second.id.clone());
    let (view, cx) = mount_view(store, cx);

    assert_eq!(
        view.read_with(cx, |view, _| view.refresh_batch_progress()),
        None,
        "no batch open"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.begin_refresh_batch(&[a.clone(), b.clone()]);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.refresh_batch_progress()),
        Some((0, 2)),
        "opened batch starts at 0 of 2"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "subs-refresh-progress");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(
                !view.accumulate_batch_member(&a, 1, 0, 0, 0, 0, 0, &[], &[], &[], &[]),
                "first of two keeps the batch open"
            );
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.refresh_batch_progress()),
        Some((1, 2)),
        "one completion reads 1 of 2"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(view.note_refresh_started(&b), "fetch starts");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "subs-refresh-progress");
    assert_painted(cx, format!("sub-refresh-spinner-{b}").as_str());
    assert_painted(cx, format!("sub-refreshing-{b}").as_str());

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert!(
                view.accumulate_batch_member(&b, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[]),
                "last member closes the batch"
            );
            view.refresh.inflight.remove(&b);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.refresh_batch_progress()),
        None,
        "closed batch reads None"
    );
    cx.update(|window, cx| window.close_sheet(cx));
}

#[gpui_kit::test]
fn t08_dead_row_alert_keeps_operate_row_inline(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, TestOutcome};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            cx.notify();
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, _cx| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: seeded.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: None,
                    error: Some("timeout".to_string()),
                },
                success: None,
            });
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);

    assert_painted(cx, "config-health-alert");
    // K1/A4-2: the actions follow the content inside the ONE pane scroll, so
    // the alert + Health module can push the row below the fold. Scroll to
    // the end: the row is inline after the content, not pinned over a gap.
    cx.update(|window, cx| {
        window.scroll(
            "config-detail-body",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "detail-operate");
}

#[gpui_kit::test]
fn t04_detail_block_shelf_packs_modules_with_operate_row(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{HealthKind, HealthSample, TestOutcome};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
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
    paint_frame_at(cx, 1000., 1600.);

    assert_painted(cx, "detail-acting-on");

    assert_painted(cx, "config-endpoints");
    assert_within_frame(cx, "config-endpoints");
    assert!(
        cx.update(|window, _| window.try_find("detail-operate".to_string()).is_some()),
        "detail-operate renders the single operate row"
    );

    let operate_bounds = cx.update(|window, _| {
        window
            .try_find("detail-operate".to_string())
            .expect("operate row renders")
            .bounds()
    });
    assert!(
        operate_bounds.origin.x >= px(0.)
            && operate_bounds.origin.x + operate_bounds.size.width <= px(1000.) + px(1.),
        "operate row fits the frame width: {operate_bounds:?}"
    );
    let (endpoints_bottom, operate_top) = cx.update(|window, _| {
        let endpoints = window
            .try_find("config-endpoints".to_string())
            .expect("endpoints paints")
            .bounds();
        let operate = window
            .try_find("detail-operate".to_string())
            .expect("operate row paints")
            .bounds();
        (endpoints.origin.y + endpoints.size.height, operate.origin.y)
    });
    assert!(
        endpoints_bottom <= operate_top,
        "endpoints sit above the operate row: {endpoints_bottom:?} <= {operate_top:?}"
    );

    // K1/A4-8: one tier — Start · Stop · Test plus the `…` kebab. Ping, Logs
    // and Copy URLs moved into the kebab.
    for id in ["engine-start", "engine-stop", "health-test", "row-menu"] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} rides the operate row"
        );
    }

    cx.update(|_, cx| {
        view.update(cx, |view, _cx| {
            view.health_store.record_outcome(&TestOutcome {
                sample: HealthSample {
                    config_id: seeded.id.clone(),
                    at: 1,
                    kind: HealthKind::UrlTest,
                    latency_ms: None,
                    error: Some("timeout".to_string()),
                },
                success: None,
            });
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);

    assert_painted(cx, "config-health-alert");
    assert_painted(cx, "config-health");
    assert!(
        cx.update(|window, _| window.try_find("detail-operate".to_string()).is_some()),
        "detail-operate renders in the stacked Actions section"
    );
    for id in ["engine-start", "health-test", "row-menu"] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} rides the stacked operate row"
        );
    }
}

#[gpui_kit::test]
fn operate_row_reachable_in_the_stacked_inspector(cx: &mut TestAppContext) {
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
    cx.run_until_parked();
    paint_frame(cx);

    assert!(
        cx.update(|window, _| window.try_find("detail-operate".to_string()).is_some()),
        "detail-operate renders"
    );
    // K1/A4-2: the actions follow the content inside the ONE pane scroll —
    // not pinned to the panel bottom. Scrolling to the end brings the row
    // fully inside the 700px frame.
    cx.update(|window, cx| {
        window.scroll(
            "config-detail-body",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame(cx);
    let (operate_top, operate_bottom) = cx.update(|window, _| {
        let bounds = window
            .try_find("detail-operate".to_string())
            .expect("operate row renders")
            .bounds();
        (bounds.origin.y, bounds.origin.y + bounds.size.height)
    });
    assert!(
        operate_top >= px(0.) && operate_bottom <= px(700.) + px(1.),
        "operate row sits inside the 700px frame at the end of the scroll, not \
         clipped: {operate_top:?}..{operate_bottom:?}"
    );

    for id in ["engine-start", "engine-stop", "health-test", "row-menu"] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} rides the operate row"
        );
    }

    assert!(
        cx.update(|window, _| window.try_find("config-history".to_string()).is_some()),
        "History module renders"
    );
    assert!(
        cx.update(|window, _| window.try_find("config-history-few".to_string()).is_some()),
        "History states its empty case instead of painting a silent box"
    );
}

#[gpui_kit::test]
fn inspector_body_scrolls_to_reach_below_fold_sections(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let select_row = |view: &Entity<AppView>, cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.select_visible_row(0, cx);
                cx.notify();
            });
        });
        cx.run_until_parked();
        open_inspector(view, cx);
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
    };
    select_row(&view, cx);

    // K1/A4-2: the pane is ONE scroll. Slice 5 slimmed the endpoint section to
    // a summary plus Edit, so the Health pane now fits at the 620px window
    // minimum; paint a deliberately short frame so the content genuinely
    // overflows and the scroll range is observable.
    paint_frame_at(cx, 1000., 540.);

    let max_offset = cx.update(|_, cx| view.read(cx).detail_scroll.max_offset());
    assert!(
        max_offset.y > px(0.),
        "the Health pane overflows the panel, so there is something below \
             the fold to scroll to: {max_offset:?}"
    );

    let scrolled = cx.update(|window, cx| {
        window.scroll(
            "config-detail-body",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-80.))),
            cx,
        );
        view.read(cx).detail_scroll.offset().y
    });
    assert!(
        scrolled < px(0.),
        "wheeling the inspector body scrolls it down: offset={scrolled:?}"
    );

    let over = cx.update(|window, cx| {
        window.scroll(
            "config-detail-body",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
        let view = view.read(cx);
        (
            view.detail_scroll.offset().y,
            view.detail_scroll.max_offset().y,
        )
    });
    assert_eq!(
        over.0, -over.1,
        "the pane clamps at its maximum offset: {:?}",
        over
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.focus.focus(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_inspector();
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|_window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_inspector();
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let (visible, offset) = cx.update(|_, cx| {
        let view = view.read(cx);
        (view.inspector_visible, view.detail_scroll.offset().y)
    });
    assert!(visible, "Ctrl+I reopened the inspector");
    assert_eq!(
        offset,
        px(0.),
        "reopening the inspector starts scrolled to the top, not mid-history"
    );
}

#[gpui_kit::test]
fn t04_detail_fits_minimum_window(cx: &mut TestAppContext) {
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
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);

    for id in [
        "config-endpoints",
        "endpoint-value-socks",
        "endpoint-value-http",
        "copy-endpoints",
    ] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }

    // The stacked inspector is one scroll: Identity + Endpoint fit at the
    // minimum window, while Actions sit below the fold until scrolled to.
    cx.update(|window, cx| {
        window.scroll(
            "config-detail-body",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    assert_painted(cx, "engine-start");
    assert_within_frame_min(cx, "engine-start");
}

#[gpui_kit::test]
fn t04_operate_row_fits_minimum_width(cx: &mut TestAppContext) {
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
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);

    let (operate_bounds, start_height) = cx.update(|window, _| {
        let operate = window
            .try_find("detail-operate".to_string())
            .expect("operate row renders")
            .bounds();
        let start = window
            .try_find("engine-start".to_string())
            .expect("start renders")
            .bounds();
        (operate, start.size.height)
    });
    assert!(
        operate_bounds.origin.x >= px(0.)
            && operate_bounds.origin.x + operate_bounds.size.width <= px(1100.) + px(1.),
        "operate row never scrolls horizontally: {operate_bounds:?}"
    );

    assert!(
        operate_bounds.size.height <= start_height * 4. + px(24.),
        "operate row wraps inside the inspector's width (four lines at 240px): \
         row {operate_bounds:?} vs button {start_height:?}"
    );

    assert!(
        cx.update(|window, _| window.try_find("row-menu".to_string()).is_some()),
        "row-menu trigger rides the operate row"
    );
    for id in [
        "edit-config",
        "delete-config",
        "duplicate-config",
        "reset-traffic",
        "usage-reset",
    ] {
        assert_absent(cx, id.to_string());
    }
}

#[gpui_kit::test]
fn t05_operate_primaries_grouped_dispatch_same_handler(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t26_engine_seed("Work", "10981"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, _) = FakeHealthProber::shared();
            view.prober = Box::new(prober);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1000., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);

    for id in ["engine-start", "engine-stop", "health-test", "row-menu"] {
        assert_painted(cx, id);
    }

    let mut primaries = Vec::new();
    for id in ["engine-start", "engine-stop", "health-test"] {
        let bounds = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
        });
        primaries.push((bounds.origin.y, bounds.origin.x));
    }
    assert!(
        primaries[0] <= primaries[1] && primaries[1] <= primaries[2],
        "primaries group in Start/Stop/Test order: {primaries:?}"
    );

    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-bar-start-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "the engine bar's Start all starts every endpoint (the one seeded row)"
    );
    assert_eq!(
        notification_count(cx),
        before + 1,
        "the engine bar's Start all toasts once"
    );
    let before = notification_count(cx);
    // The toast park sits at the bottom-right, where the engine bar's own
    // controls live. Clearing the toast first keeps the click on the button.
    cx.update(|window, cx| window.clear_notifications(cx));
    cx.update(|window, cx| {
        window.click("engine-bar-stop-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "the engine bar's Stop all asks first: the confirm gates the kill"
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the engine bar's Stop all opens the stop-all confirm"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_stop_all(window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "the engine bar's Stop all stops it again once confirmed"
    );
    assert_eq!(
        notification_count(cx),
        before + 1,
        "the engine bar's Stop all toasts once after the confirm"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-start", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "operate Start starts the selected row"
    );
    assert_eq!(
        notification_count(cx),
        before + 1,
        "operate Start toasts once"
    );

    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    let (operate_bounds, start_height) = cx.update(|window, _| {
        let operate = window
            .try_find("detail-operate".to_string())
            .expect("operate row renders")
            .bounds();
        let start = window
            .try_find("engine-start".to_string())
            .expect("start renders")
            .bounds();
        (operate, start.size.height)
    });
    assert!(
        operate_bounds.origin.x >= px(0.)
            && operate_bounds.origin.x + operate_bounds.size.width <= px(1100.) + px(1.),
        "grouped operate row never scrolls horizontally: {operate_bounds:?}"
    );
    assert!(
        operate_bounds.size.height <= start_height * 4. + px(24.),
        "grouped operate row wraps inside the inspector's width (four lines at 240px): \
             row {operate_bounds:?} vs button {start_height:?}"
    );
}

#[gpui_kit::test]
fn t05_bulk_bar_grouped_delete_trailing(cx: &mut TestAppContext) {
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
            view.select_rows(&[first.id.clone(), second.id.clone()], cx);
        });
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    assert_painted(cx, "batch-bar");
    assert_painted(cx, "batch-count");

    let verbs = ["batch-start", "batch-stop", "batch-more"];
    for id in verbs {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    let mut order = Vec::new();
    for id in verbs {
        let bounds = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
        });
        order.push((bounds.origin.y, bounds.origin.x));
    }
    for pair in order.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "bulk bar reads Start | Stop | More: {order:?}"
        );
    }
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 2);

    let (bar_bounds, button_height) = cx.update(|window, _| {
        let bar = window
            .try_find("batch-bar".to_string())
            .expect("bulk bar renders")
            .bounds();
        let button = window
            .try_find("batch-start".to_string())
            .expect("bulk start renders")
            .bounds();
        (bar, button.size.height)
    });
    assert!(
        bar_bounds.size.height <= button_height + px(20.),
        "bulk bar stays on ONE line: bar {bar_bounds:?} vs button {button_height:?}"
    );

    cx.update(|window, cx| {
        window.click("batch-more", cx);
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    // F6: Enable/Disable left the bulk menu — the flag is restore-on-launch,
    // not a bulk operation.
    let menu = ["batch-more-copy", "batch-more-move", "batch-more-delete"];
    let mut menu_order = Vec::new();
    for id in menu {
        assert_painted(cx, id);
        let bounds = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
        });
        menu_order.push((bounds.origin.y, bounds.origin.x));
    }
    for pair in menu_order.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "More keeps Delete trailing: {menu_order:?}"
        );
    }
    let escape = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame_min(cx);
    assert_absent(cx, "batch-more-delete".to_string());
}

#[gpui_kit::test]
fn t05_row_menu_holds_overflow_actions_only(cx: &mut TestAppContext) {
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
    paint_frame_at(cx, 1000., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    assert_painted(cx, "row-menu");
    // K1/A4-8: the `…` kebab carries only the five overflow actions; row
    // management (Edit/Delete/Duplicate/Reset) lives in the row's context menu.
    for id in [
        "kebab-auto-start",
        "kebab-ping",
        "kebab-logs",
        "kebab-copy-urls",
        "kebab-copy-proxy",
        "edit-config",
        "delete-config",
        "duplicate-config",
        "reset-traffic",
        "usage-reset",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| {
        window.click("row-menu", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    for id in [
        "kebab-auto-start",
        "kebab-ping",
        "kebab-logs",
        "kebab-copy-urls",
        "kebab-copy-proxy",
    ] {
        assert_painted(cx, id);
    }
    for id in [
        "edit-config",
        "delete-config",
        "duplicate-config",
        "reset-traffic",
        "usage-reset",
    ] {
        assert_absent(cx, id.to_string());
    }
    assert_absent(cx, "clone-config".to_string());
}

#[gpui_kit::test]
fn t04_stacked_inspector_names_acting_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "detail-acting-on");
    let label = view.read_with(cx, |view, cx| {
        view.selected_config(cx)
            .map(|config| manyproxy_ui::acting_on_label(&config))
            .unwrap_or_default()
    });
    assert_eq!(
        label, "Acting on: Work",
        "the stacked inspector names its row"
    );
}

#[gpui_kit::test]
fn t06_endpoint_copy_copies_the_rows_urls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");

    store
        .add(t30_engine_seed("Work", "10981"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1000., 1600.);
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    // K1/A4-4: ONE copy action for the block; the per-row buttons are gone.
    assert!(
        cx.update(|window, _| window.try_find("copy-endpoints".to_string()).is_some()),
        "copy-endpoints rides the endpoints block"
    );
    for id in ["copy-socks", "copy-http", "copy-upstream"] {
        assert_absent(cx, id.to_string());
    }
    let (bind, port) = view.read_with(cx, |view, cx| {
        let row = view.selected_config(cx).expect("selected");
        (row.bind_ip.clone(), row.port)
    });
    let (socks, http) = manyproxy_ui::store::proxy_urls(&bind, port);
    let read_clip = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap_or_default()
                .to_string()
        })
    };
    cx.update(|window, cx| {
        window.click("copy-endpoints", cx);
    });
    cx.run_until_parked();
    let clip = read_clip(cx);
    assert_eq!(
        clip,
        format!("{socks}\n{http}"),
        "the block's Copy puts both URLs on the clipboard"
    );
    for canary in ["11111111", "198.51.100.9", "uuid", "password"] {
        assert!(
            !clip.contains(canary),
            "clipboard carries bind+port only, found {canary}: {clip}"
        );
    }
    // The kebab's Copy SOCKS is the row-scoped copy: bind:port only,
    // never the HTTP form and never the selection's other rows.
    cx.update(|window, cx| {
        window.click("row-menu", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    cx.update(|window, cx| {
        window.click("kebab-copy-proxy", cx);
    });
    cx.run_until_parked();
    let clip = read_clip(cx);
    assert_eq!(clip, socks, "Copy SOCKS copies the SOCKS URL alone: {clip}");
    assert!(!clip.contains(&http), "no HTTP form: {clip}");
    for canary in ["11111111", "198.51.100.9", "uuid", "password"] {
        assert!(
            !clip.contains(canary),
            "clipboard carries bind+port only, found {canary}: {clip}"
        );
    }
}

#[gpui_kit::test]
fn t02_detail_modules_share_container_grammar(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    let row_id = view.read_with(cx, |view, cx| {
        view.libraries.read(cx).delegate().rows[0].id.clone()
    });

    let (parts, line) = view.read_with(cx, |view, _| {
        (view.usage_parts(&row_id), view.usage_line(&row_id))
    });
    match parts {
        Some((down, up)) => assert_eq!(
            line,
            format!("usage down {down} · up {up}"),
            "boxed buckets match the honest line"
        ),
        None => assert_eq!(line, "usage unavailable", "no buckets, no lying zero"),
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);

    assert_painted(cx, "config-endpoints");
    for id in [
        "endpoint-value-socks",
        "endpoint-value-http",
        "copy-endpoints",
    ] {
        assert_painted(cx, id);
    }
    assert_painted(cx, "config-usage");
    assert_painted(cx, "config-health");
    assert!(
        cx.update(|window, _| window.try_find("config-history".to_string()).is_some()),
        "config-history renders in the Health section"
    );
}

#[gpui_kit::test]
fn t10_copy_selected_urls_carry_binds_not_secrets(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");

    store
        .add(t30_engine_seed("Work", "10981"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    let before = notification_count(cx);
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
    assert_eq!(
        clip,
        format!("{socks}\n{http}"),
        "both bind+port forms and nothing else: {clip}"
    );
    for canary in ["11111111", "198.51.100.9", "uuid", "password"] {
        assert!(
            !clip.contains(canary),
            "clipboard carries bind+port only, found {canary}: {clip}"
        );
    }
    assert_eq!(notification_count(cx), before + 1, "one copy toast");
}

#[gpui_kit::test]
fn in_flight_flags_show_button_spinners(cx: &mut TestAppContext) {
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
    let has_spinner = |cx: &mut VisualTestContext, id: &str| {
        cx.update(|window, _| window.try_find(id.to_string()).is_some())
    };

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.start_in_flight = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        has_spinner(cx, "engine-start-spinner"),
        "start shows loading"
    );
    assert!(
        !has_spinner(cx, "engine-stop-spinner"),
        "stop stays live and unlit during a start"
    );
    assert!(
        !has_spinner(cx, "health-test-spinner"),
        "no probe running, no test spinner"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.start_in_flight = false;
            view.probe_url_in_flight = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !has_spinner(cx, "engine-start-spinner"),
        "start flag cleared"
    );
    assert!(has_spinner(cx, "health-test-spinner"), "test shows loading");
    assert!(
        !has_spinner(cx, "engine-start-spinner"),
        "start spinner stays off while a probe runs"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.probe_url_in_flight = false;
            view.probe_ping_in_flight = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    // K1: Ping moved into the `…` kebab — the operate row has no ping spinner,
    // and a ping in flight lights nothing.
    assert!(
        !has_spinner(cx, "health-test-spinner"),
        "url-test flag cleared"
    );
    assert!(
        !has_spinner(cx, "engine-start-spinner") && !has_spinner(cx, "engine-stop-spinner"),
        "a ping lights no operate-row spinner (Stop never lights one)"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.probe_ping_in_flight = false;
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "engine-start-spinner",
        "engine-stop-spinner",
        "health-test-spinner",
    ] {
        assert!(!has_spinner(cx, id), "{id} leaves with the flag");
    }
}

#[test]
fn t09_motion_fast_budget_capped() {
    use manyproxy_ui::AppView;

    const {
        assert!(
            AppView::MOTION_FAST_MS <= 200,
            "Ticket 09 enter fades stay within the 200ms budget"
        );
        assert!(
            AppView::motion_accent_halves_ms() * 2 <= 200,
            "B18: the row accent's two halves stay inside the same budget"
        );
    }
    assert_eq!(
        2 * AppView::motion_accent_halves_ms(),
        AppView::MOTION_FAST_MS,
        "one budget: the accent spends it, it does not add to it"
    );
}

#[gpui_kit::test]
fn t09_motion_fast_duration_honors_reduce_motion(cx: &mut TestAppContext) {
    use std::time::Duration;

    use manyproxy_ui::AppView;
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        assert_eq!(
            AppView::motion_fast_duration(cx),
            Duration::ZERO,
            "reduced motion means no animation"
        );

        assert_eq!(
            AppView::motion_fast_duration(cx),
            Duration::ZERO,
            "reduced motion is stable across reads"
        );
        cx.set_reduce_motion(false);
        assert_eq!(
            AppView::motion_fast_duration(cx),
            Duration::from_millis(AppView::MOTION_FAST_MS),
            "motion stays on the capped budget"
        );

        cx.set_reduce_motion(true);
        assert_eq!(
            AppView::motion_fast_duration(cx),
            Duration::ZERO,
            "turning reduced motion back on collapses the fade again"
        );
    });
}

#[test]
fn t09_start_busy_copy_names_starting() {
    use manyproxy_ui::start_busy_message;
    assert_eq!(
        start_busy_message(),
        "Starting… already in progress",
        "second Start explains itself with the Starting copy"
    );
}

/// B18: a running set that gains exactly one id is a single-row start, and that
/// row wears the accent. Under reduced motion the strip is not painted at all —
/// not just handed a zero duration that would paint its end state.
#[gpui_kit::test]
fn b18_row_accent_is_off_under_reduce_motion(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let started = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let settled_quiet = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    let running = |cx: &mut VisualTestContext, ids: Vec<String>| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    state.delegate_mut().set_running(ids);
                    list_cx.notify();
                });
            });
        });
    };
    let finds = |cx: &mut VisualTestContext, id: String| {
        cx.update(|window, _| window.try_find(id).is_some())
    };

    running(cx, vec![started.id.clone()]);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        finds(cx, format!("row-accent-{}", started.id)),
        "the settled row carries the accent with motion on"
    );

    cx.update(|_, cx| cx.set_reduce_motion(true));
    running(cx, vec![started.id.clone(), settled_quiet.id.clone()]);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        finds(cx, format!("row-surface-{}", settled_quiet.id)),
        "the row itself still paints"
    );
    assert!(
        !finds(cx, format!("row-accent-{}", settled_quiet.id)),
        "reduced motion paints no accent strip at all"
    );
}

/// B18: the accent is decoration on the card's own leading edge, so it may
/// never move the geometry: every row keeps CARD_HEIGHT, the pitch stays
/// ROW_PITCH, and the list paints the same rows before and after it fires.
#[gpui_kit::test]
fn b18_row_accent_keeps_row_pitch_and_count(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let rows: Vec<String> = (0..3)
        .map(|i| {
            store
                .add(row19_seed(
                    &format!("node-{i}"),
                    &format!("h{i}.example.com"),
                    &format!("{}", 11_000 + i),
                ))
                .expect("seed works")
                .id
        })
        .collect();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    // (origin.y, height) per painted card, in row order.
    let cards = |cx: &mut VisualTestContext, rows: &[String]| {
        cx.update(|window, _| {
            rows.iter()
                .filter_map(|id| window.try_find(format!("row-surface-{id}")))
                .map(|snapshot| {
                    let bounds = snapshot.bounds();
                    (bounds.origin.y.as_f32(), bounds.size.height.as_f32())
                })
                .collect::<Vec<_>>()
        })
    };
    let quiet = cards(cx, &rows);
    assert_eq!(
        quiet.len(),
        rows.len(),
        "every row paints before the accent"
    );
    assert!(
        quiet
            .iter()
            .all(|(_, height)| *height == manyproxy_ui::Density::Comfortable.row_height()),
        "quiet rows keep their card height: {quiet:?}"
    );

    // The engine reports one new running id: the single-row start settles.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                state.delegate_mut().set_running(vec![rows[1].clone()]);
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    let accented = cards(cx, &rows);
    assert_eq!(
        accented, quiet,
        "the accent moves no row: same count, same origins, same heights"
    );
    assert!(
        accented
            .windows(2)
            .all(|pair| pair[1].0 - pair[0].0 == manyproxy_ui::Density::Comfortable.pitch()),
        "the pitch is untouched: {accented:?}"
    );
    let (card, accent) = cx.update(|window, _| {
        (
            window
                .try_find(format!("row-surface-{}", rows[1]))
                .expect("the settled row paints")
                .bounds(),
            window
                .try_find(format!("row-accent-{}", rows[1]))
                .expect("the settled row wears the accent")
                .bounds(),
        )
    });
    assert_eq!(
        accent.size.width,
        px(3.),
        "the accent is the card's own leading edge"
    );
    assert_eq!(
        accent.origin.x, card.origin.x,
        "the strip is the row's own leading edge"
    );
    assert_eq!(
        accent.size.height + px(1.),
        card.size.height,
        "the strip runs the row's full interior, inside its hairline"
    );
}

#[gpui_kit::test]
fn t09_import_double_commit_ignored(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog("vless://x", "travel", None, window, cx);
        });
    });
    paint_dialog_open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.set_import_parse_in_flight(true);
        });
    });
    paint_dialog_open(cx);
    // The parse begins after the commit click, while this dialog stays open;
    // its footer has to show the wait instead of looking idle.
    assert_painted(cx, "import-dialog-spinner");
    assert_painted(cx, "import-dialog-ok");
    let import_label = cx.update(|window, _| {
        window
            .try_find("import-dialog-ok")
            .and_then(|snapshot| snapshot.label().map(str::to_string))
    });
    assert_eq!(import_label.as_deref(), Some("Importing…"));
    let before = notification_count(cx);
    let committed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import_dialog(
                String::new(),
                "Work".to_string(),
                None,
                None,
                None,
                window,
                cx,
            )
        })
    });
    assert!(!committed, "second commit ignored while parsing");
    assert!(
        view.read_with(cx, |view, _| view.import_parse_in_flight),
        "in-flight parse undisturbed"
    );
    assert_eq!(
        notification_count(cx),
        before,
        "silent ignore, like the empty-text path"
    );
}

#[gpui_kit::test]
fn t09_reduce_motion_state_changes_stay_perceivable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
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

    // Slice 6: the five panes are one stacked scroll; the reduction is that
    // every section still paints and the operate row is still reachable.
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1400., 1600.);
    assert_painted(cx, "config-detail-body");
    for marker in [
        "inspector-identity",
        "config-endpoints",
        "config-health",
        "config-usage",
        "inspector-actions",
        "inspector-console-link",
    ] {
        assert_painted(cx, marker);
    }

    assert!(
        cx.update(|window, _| window.try_find("detail-operate".to_string()).is_some()),
        "operate row present in the stacked inspector"
    );

    let row_id = view.read_with(cx, |view, cx| {
        view.libraries.read(cx).delegate().rows[0].id.clone()
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&row_id, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, "batch-bar");
    assert_painted(cx, "batch-count");
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 1);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.clear_selection(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert!(
        cx.update(|window, _| window.try_find("batch-bar".to_string()).is_none()),
        "the batch bar leaves the slot when the selection empties"
    );
    assert_painted(cx, "library-search");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_painted(cx, "field-name");
}

#[gpui_kit::test]
fn t09_bulk_clear_returns_focus_to_shell(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let row_id = view.read_with(cx, |view, cx| {
        view.libraries.read(cx).delegate().rows[0].id.clone()
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&row_id, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "batch-count");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.clear_selection(window, cx));
    });
    let home = cx.update(|window, cx| {
        let shell = view.read(cx).focus.clone();
        window.focused(cx).as_ref() == Some(&shell)
    });
    assert!(home, "bulk clear returns keyboard focus to the shell");
}

#[gpui_kit::test]
fn shortcut_buttons_pin_action_names_and_key_tooltips(cx: &mut TestAppContext) {
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
    cx.run_until_parked();
    paint_frame(cx);

    // T-02: the accessible name is the action alone. The key rides in the
    // tooltip, never inside the name.
    for (id, pinned) in [
        ("engine-start", "Start the selected endpoint"),
        ("engine-stop", "Stop"),
        ("health-test", "Test"),
    ] {
        let label = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .label()
                .map(str::to_string)
        });
        assert_eq!(
            label.as_deref(),
            Some(pinned),
            "{id} announces the action alone, without the key"
        );
        assert!(
            !label.as_deref().unwrap_or_default().contains('('),
            "{id} keeps every key out of its accessible name"
        );
    }
    // The key stays visible as a chip beside the button.
    for chip in ["kbd:f5", "kbd:shift-f5", "kbd:ctrl-t"] {
        assert!(
            cx.debug_bounds(chip).is_some(),
            "{chip} paints the shortcut hint as a chip on the operate row"
        );
    }
    // …and the tooltip is where the key lives.
    assert_eq!(
        manyproxy_ui::app::detail::operate_start_tooltip(),
        "Start the selected endpoint (F5)"
    );
    assert_eq!(
        manyproxy_ui::app::detail::operate_stop_tooltip(),
        "Stop the selected endpoint (Shift-F5)"
    );
    assert_eq!(
        manyproxy_ui::app::detail::operate_test_tooltip(),
        "Test the selected endpoint (Ctrl-T)"
    );

    // The retired Tools menu carried the chipped rows. The engine bar's menu
    // names its actions alone; the bindings live in the palette.
    cx.update(|window, cx| {
        window.click("engine-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for (id, pinned) in [
        ("engine-bar-test-all", "Test all"),
        ("engine-bar-stop-testing", "Stop testing"),
    ] {
        let label = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .label()
                .map(str::to_string)
        });
        assert_eq!(
            label.as_deref(),
            Some(pinned),
            "{id} announces the action alone, without the key"
        );
        assert!(
            !label.as_deref().unwrap_or_default().contains('('),
            "{id} keeps every key out of its accessible name"
        );
    }
    assert!(
        cx.debug_bounds("kbd:ctrl-.").is_none(),
        "the engine bar's menu row carries no shortcut chip"
    );
    cx.update(|window, cx| {
        use gpui_kit::Keystroke;
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

#[gpui_kit::test]
fn clear_filters_hint_clears_query(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("zzz-no-hit".to_string(), cx)
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-clear-filters");
    cx.update(|window, cx| {
        window.click("library-clear-filters", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.query.search.clone()),
        String::new(),
        "the hint clears the query"
    );
    assert_painted(cx, &format!("library-row-{}", seeded.id));
}

#[gpui_kit::test]
fn empty_library_points_at_the_action_bar_only(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    // F18: the canvas now carries the CTAs; the action bar keeps working as
    // the intake home for populated libraries.
    assert_painted(cx, "empty-add");
    assert_painted(cx, "empty-import");
    assert_painted(cx, "action-bar-add");
    assert_painted(cx, "action-bar-import");

    cx.update(|window, cx| {
        window.click("action-bar-add", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Add opens the add dialog"
    );
    cx.update(|window, cx| {
        window.close_all_dialogs(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("action-bar-import", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Import opens the import dialog"
    );
}

#[gpui_kit::test]
fn library_paints_empty_state(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, "library-empty");
}

#[gpui_kit::test]
fn library_mutations_repaint_and_keep_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let third_id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.commit_add(seed_input("Cabin"), window, cx));
            view.store
                .list()
                .expect("list works")
                .into_iter()
                .find(|row| row.name == "Cabin")
                .expect("added row stored")
                .id
        })
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 3);
    assert_painted(cx, &format!("library-row-{}", first.id));
    assert_painted(cx, &format!("library-row-{}", second.id));
    assert_painted(cx, &format!("library-row-{third_id}"));
    assert_eq!(
        view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id)),
        Some(first.id.clone()),
        "selection preserved by id across add"
    );
    assert_painted(cx, "config-endpoints");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_remove(&third_id, window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);
    assert_painted(cx, &format!("library-row-{}", first.id));
    assert_painted(cx, &format!("library-row-{}", second.id));
    assert_absent(cx, format!("library-row-{third_id}"));
    assert_eq!(
        view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id)),
        Some(first.id.clone()),
        "selection preserved by id across remove"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now("home".to_string(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, &format!("library-row-{}", second.id));
    assert_absent(cx, format!("library-row-{}", first.id));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now(String::new(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, &format!("library-row-{}", first.id));
    assert_painted(cx, &format!("library-row-{}", second.id));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_remove(&first.id, window, cx);
            view.commit_remove(&second.id, window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);
    assert_painted(cx, "library-empty");
    assert_eq!(
        view.read_with(cx, |view, cx| view.selected_config(cx)),
        None,
        "removing the selected row clears the selection"
    );
}

#[gpui_kit::test]
fn library_health_tick_repaints_rows(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store
        .add(row19_seed("Work", "198.51.100.9", "10811"))
        .expect("seed works");
    let second = store
        .add(row19_seed("Home", "203.0.113.7", "10812"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
        });
    });
    handle.script_latency(&first.id, 33);
    handle.script_failure(&second.id, "upstream refused");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    let tested =
        cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(tested, 2);

    cx.run_until_parked();
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, &format!("library-row-{}", first.id));
    assert_painted(cx, &format!("library-row-{}", second.id));
    let health = view.read_with(cx, |view, cx| {
        view.libraries.read(cx).delegate().health.clone()
    });
    assert_eq!(
        health.get(&first.id).map(String::as_str),
        Some("33ms"),
        "live row summary refreshed: {health:?}"
    );
    assert_eq!(
        health.get(&second.id).map(String::as_str),
        Some("dead"),
        "dead row summary refreshed: {health:?}"
    );
}

#[test]
fn library_row_lines_show_complete_content() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_core::library_row_lines;
    fn row(enabled: bool, params: &str) -> ProxyConfig {
        ProxyConfig {
            id: "7".to_string(),
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: params.to_string().into(),
            group: "travel".to_string(),
            enabled,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let full = library_row_lines(
        &row(true, "{\"server\":\"a.example.com\",\"server_port\":443}"),
        Some("87ms"),
        true,
        Some((1_048_782, 88)),
    )
    .join("\n");
    for want in [
        "Work",
        "vless",
        "a.example.com:443",
        "travel",
        "127.0.0.1:10801",
        "enabled",
        "87ms",
        "1.0 MB",
        "88 B",
    ] {
        assert!(full.contains(want), "{want} visible in:\n{full}");
    }
    let plain = library_row_lines(&row(false, "{}"), None, false, None).join("\n");
    assert!(
        plain.contains("disabled"),
        "enabled state visible:\n{plain}"
    );
    assert!(
        plain.contains("no upstream"),
        "metadata-only rows say so honestly:\n{plain}"
    );
    assert_eq!(
        library_row_lines(&row(false, "{}"), None, false, None).len(),
        4,
        "badge line always present (stopped here)"
    );
    assert_eq!(
        library_row_lines(&row(false, "{}"), None, false, Some((0, 0))).len(),
        4,
        "zero traffic adds no line"
    );
    assert_eq!(
        library_row_lines(&row(false, "{}"), None, false, Some((10, 0))).len(),
        5,
        "flowing traffic appends its line"
    );
}

#[test]
fn library_row_cells_split_state_route_and_accounting() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::{library_row_cells, library_row_full_route};
    let row = ProxyConfig {
        id: "7".to_string(),
        name: "Work".to_string(),
        protocol: "vless".to_string(),
        params: "{\"server\":\"a.example.com\",\"server_port\":443,\"password\":\"s3cret\"}"
            .to_string()
            .into(),
        group: "travel".to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let cells = library_row_cells(
        &row,
        "a.example.com:443",
        Some("87ms"),
        true,
        false,
        Some((1_048_782, 88)),
        false,
    );
    assert_eq!(
        cells[0], "a.example.com:443",
        "upstream cell is upstream only"
    );
    assert!(
        cells[1].contains("Running") && cells[1].contains("87ms"),
        "health cell is state plus probe:\n{cells:?}"
    );
    assert!(
        !cells[1].contains("1.0 MB"),
        "accounting stays out of health:\n{cells:?}"
    );
    assert_eq!(cells[2], "travel", "group has its own cell");
    assert!(
        cells[3].contains("1.0 MB"),
        "traffic cell carries totals:\n{cells:?}"
    );
    let route = library_row_full_route(&row, "a.example.com:443");
    for want in ["travel", "enabled", "127.0.0.1:10801"] {
        assert!(
            route.contains(want),
            "{want} stays on the full route tooltip:\n{route}"
        );
    }
    let shown = [row.name.clone(), route.clone()]
        .into_iter()
        .chain(cells.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!shown.contains("s3cret"), "no secrets in row cells");
    let idle = library_row_cells(&row, "no upstream", None, false, false, None, false);
    assert_eq!(
        idle[1], "",
        "idle health cell empty — gray lamp carries stopped"
    );
}

#[test]
fn upstream_host_port_reads_only_host_keys() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::upstream_host_port;
    fn row(params: &str) -> ProxyConfig {
        ProxyConfig {
            id: "7".to_string(),
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: params.to_string().into(),
            group: "default".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    assert_eq!(
        upstream_host_port(&row("{\"server\":\"a.example.com\",\"server_port\":443,\
                    \"uuid\":\"SECRET-UUID\",\"password\":\"hunter2\"}"))
        .as_deref(),
        Some("a.example.com:443")
    );
    assert_eq!(
        upstream_host_port(&row("{\"address\":\"10.0.0.5\",\"port\":\"51820\"}")).as_deref(),
        Some("10.0.0.5:51820"),
        "address/port aliases work"
    );
    assert_eq!(
        upstream_host_port(&row("{\"server\":\"a.example.com\"}")).as_deref(),
        Some("a.example.com"),
        "host alone still shows"
    );
    assert_eq!(upstream_host_port(&row("{}")), None);
    assert_eq!(upstream_host_port(&row("not json")), None);
    for (params, canary) in [
        (
            "{\"server\":\"a.example.com\",\"server_port\":443,\"password\":\"hunter2\"}",
            "hunter2",
        ),
        (
            "{\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"SECRET-UUID\"}",
            "SECRET-UUID",
        ),
        (
            "{\"server\":\"a.example.com\",\"server_port\":443,\"token\":\"abc123\"}",
            "abc123",
        ),
    ] {
        let shown = upstream_host_port(&row(params)).unwrap_or_default();
        assert!(!shown.contains(canary), "no credentials in {shown}");
    }
}

/// A filter narrows the visible set; the selection is the app's, and a
/// hidden row stays selected so the batch still acts on it.
#[test]
fn filter_narrows_the_visible_set_never_the_wrong_row() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(id: &str, name: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{}".to_string().into(),
            group: "team".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![row("1", "Alpha"), row("2", "Beta"), row("3", "Gamma")]);
    assert_eq!(
        test_visible(&delegate, &manyproxy_ui::FilterQuery::default()).len(),
        3,
        "no filter shows every row"
    );

    let mut query = manyproxy_ui::FilterQuery::default();
    query.set_search("gamma");
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Gamma"],
        "the filter narrows the visible set"
    );

    query.set_search("alpha");
    assert_eq!(
        test_names(&delegate, &query),
        vec!["Alpha"],
        "the filter names the rows it kept"
    );
}

#[gpui_kit::test]
fn background_refresh_never_hijacks_add_dialog(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "foreign dialog survives the refresh"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-name").is_some()),
        "Add dialog keeps its fields"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-sub-url").is_none()),
        "Subscriptions dialog never opens uninvited"
    );
    assert_eq!(notification_count(cx), 0, "no toast for a non-event");
}

#[gpui_kit::test]
fn subs_dialog_reopen_preserves_form_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
    });
    paint_dialog_open(cx);

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

    cx.update(|window, cx| {
        let form = view.read_with(cx, |view, _| view.subs_form.clone());
        let form = form.expect("subs form stashed on open");
        form.url.update(cx, |state, cx| {
            state.set_value("https://example.com/typed.txt", window, cx);
        });
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
    });
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "subs sheet reopens"
    );
    let url = view.read_with(cx, |view, cx| {
        view.subs_form
            .clone()
            .expect("form restashed")
            .url
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(url, "https://example.com/typed.txt", "typed text survives");
}

#[gpui_kit::test]
fn edit_commit_writes_only_user_changed_fields(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);

    let snapshot = view.read_with(cx, |view, _| {
        view.store
            .get(&seeded.id)
            .expect("get works")
            .expect("row there")
    });

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            use manyproxy_ui::store::ConfigPatch;
            view.store
                .update(
                    &seeded.id,
                    ConfigPatch {
                        params: Some(Secret::new("{\"type\":\"vless\",\"server\":\"9.9.9.9\"}")),
                        ..Default::default()
                    },
                )
                .expect("concurrent edit works");
        });
    });

    let renamed = cx.update(|window, cx| {
        view.update(cx, |_view, cx| {
            use manyproxy_ui::ConfigForm;
            let form = ConfigForm::prefilled(window, cx, &snapshot, None);
            form.name.update(cx, |state, cx| {
                state.set_value("Renamed", window, cx);
            });
            form.as_patch_vs(cx, &snapshot)
        })
    });
    assert!(renamed.name.is_some(), "touched field writes");
    assert!(renamed.params.is_none(), "untouched field stays None");
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit(&seeded.id, renamed, window, cx)
        })
    });
    assert!(closed, "valid edit closes");
    let row = view.read_with(cx, |view, _| {
        view.store
            .get(&seeded.id)
            .expect("get works")
            .expect("row there")
    });
    assert_eq!(row.name, "Renamed");
    assert_eq!(
        row.params, "{\"type\":\"vless\",\"server\":\"9.9.9.9\"}",
        "concurrent refresh survives on the untouched field"
    );
}

#[gpui_kit::test]
fn edit_commit_missing_row_warns_and_closes(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ConfigPatch;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.remove(&seeded.id).expect("remove works");
        });
    });
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit(&seeded.id, ConfigPatch::default(), window, cx)
        })
    });
    assert!(closed, "ghost edit closes");
    assert_eq!(notification_count(cx), 1, "removal warns once");
}

#[gpui_kit::test]
fn batch_union_never_resets_open_batch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for label in ["One", "Two", "Three"] {
        let sub = store
            .add_subscription(label, "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        ids.push((sub.id.clone(), sub.label.clone()));
    }
    let (view, cx) = mount_view(store, cx);
    let gen0 = view.read_with(cx, |view, _| view.refresh.generation);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.begin_refresh_batch(&[ids[0].0.clone(), ids[1].0.clone()]);
        });
    });
    let gen1 = view.read_with(cx, |view, _| view.refresh.generation);
    assert_eq!(gen1, gen0 + 1, "fresh batch bumps the generation");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &ids[0].0,
                &ids[0].1,
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                        .to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 0, "open batch stays silent");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.begin_refresh_batch(&[ids[2].0.clone()]);
        });
    });
    let (gen2, total) = view.read_with(cx, |view, _| (view.refresh.generation, view.refresh.total));
    assert_eq!(gen2, gen1, "union keeps the epoch");
    assert_eq!(total, 3, "union grows the total");
    assert_eq!(
        view.read_with(cx, |view, _| view.refresh.added),
        1,
        "accumulated churn survives the union"
    );
    for (id, label) in ids.iter().skip(1) {
        let id = id.clone();
        let label = label.clone();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.finish_subscription_refresh(
                    &id,
                    &label,
                    Ok(manyproxy_ui::FetchedSubscription {
                        body: "vless://11111111-2222-4333-8444-555555555555@b.example.com:443#two"
                            .to_string(),
                        userinfo: None,
                    }),
                    999,
                    window,
                    cx,
                );
            });
        });
    }
    assert_eq!(notification_count(cx), 1, "one batch, one toast");
}

#[gpui_kit::test]
fn manual_refresh_adopts_inflight_fetch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.begin_refresh_batch(std::slice::from_ref(&sub_id));
            assert!(view.note_refresh_started(&sub_id), "first fetch starts");
            assert!(!view.note_refresh_started(&sub_id), "second fetch dedupes");
        });
    });

    let started = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.refresh_subscription_by_id(&sub_id, window, cx)
        })
    });
    assert!(!started, "in-flight fetch is adopted, not restarted");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                "Nightly",
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                        .to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(
        notification_count(cx),
        1,
        "adopted completion notifies once"
    );
}

#[test]
fn t08_still_in_use_counts_in_single_and_coalesced_copy() {
    use manyproxy_ui::{coalesced_refresh_message, refresh_summary_text};
    let (warn, text) = refresh_summary_text("Nightly", 0, 0, 1, 0, 0, 0, 0, 1, &[], &[], &[], &[]);
    assert!(!warn);
    assert!(text.contains("[=] Still in use 1"), "{text}");
    let (warn, text) = coalesced_refresh_message(2, 0, 0, 1, 0, 0, 1).expect("batch reports");
    assert!(!warn);
    assert!(text.contains("[=] Still in use 1"), "{text}");
    let (_, text) = refresh_summary_text("Nightly", 1, 0, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[]);
    assert!(!text.contains("[=]"), "quiet when nothing serves: {text}");
}

#[gpui_kit::test]
fn t08_manual_refresh_commits_quota_and_logs(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                "Nightly",
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                        .to_string(),
                    userinfo: manyproxy_ui::store::parse_subscription_userinfo(
                        "upload=1; download=2; total=100; expire=1735689600",
                    ),
                }),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 1, "manual single pops once");
    let (quota, lines) = view.read_with(cx, |view, _| {
        let kept = view
            .store
            .get_subscription(&sub_id)
            .expect("get works")
            .expect("sub there");
        let lines: Vec<String> = view
            .logs
            .store
            .all()
            .iter()
            .map(|record| record.message.clone())
            .collect();
        (
            (kept.quota_used, kept.quota_total, kept.quota_expire),
            lines,
        )
    });
    assert_eq!(
        quota,
        (Some(3), Some(100), Some(1_735_689_600)),
        "header quota commits"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Nightly") && line.contains("[+]Added 1")),
        "completion logs: {lines:?}"
    );
}

#[gpui_kit::test]
fn t08_refresh_failure_names_subscription_and_logs(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    store
        .refresh_with_body(
            &sub.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
            100,
        )
        .expect("seed member");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                "Nightly",
                Err("subscription fetch failed".to_string()),
                200,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 1, "failure pops once");
    let (members, error, lines) = view.read_with(cx, |view, _| {
        let members = view.store.list().expect("list works").len();
        let error = view
            .store
            .get_subscription(&sub_id)
            .expect("get works")
            .expect("sub there")
            .last_error
            .clone();
        let lines: Vec<String> = view
            .logs
            .store
            .all()
            .iter()
            .map(|record| record.message.clone())
            .collect();
        (members, error, lines)
    });
    assert_eq!(members, 1, "last-good kept");
    assert!(error.is_some(), "error recorded");
    assert!(
        lines.iter().any(|line| line.contains("Nightly")),
        "log names the sub: {lines:?}"
    );
}

#[test]
fn t08_group_rows_kind_quota_skip() {
    use manyproxy_ui::{group_quota_line, group_row_views};
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(NewConfig {
            name: "Hand".to_string(),
            protocol: "vless".to_string(),
            params: "{}".to_string().into(),
            group: "basic".to_string(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("hand add works");
    let fed = store
        .add_subscription("S1", "https://example.com/s1.txt", "sub", true)
        .expect("sub works");
    let now = 1_700_000_100;
    store
        .refresh_with_body(
            &fed.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
            now,
        )
        .expect("refresh works");
    store
        .set_subscription_quota(
            &fed.id,
            manyproxy_ui::store::parse_subscription_userinfo("upload=2048; download=0; total=1048576")
                .map(|mut quota| {
                    quota.expire = Some(now + 259_200);
                    quota
                }),
        )
        .expect("quota works");
    store
        .add_subscription("S2", "https://example.com/s2.txt", "empty", true)
        .expect("sub works");
    store
        .set_group_skip_refresh("sub", true)
        .expect("flag works");
    let counts = store.group_counts().expect("counts work");
    let subs = store.list_subscriptions().expect("subs work");
    let rows = group_row_views(
        &counts,
        &subs,
        &|group| store.group_skip_refresh(group),
        &|_| false,
        now,
    );
    assert_eq!(rows.len(), 3, "union of row groups + fed groups");
    assert_eq!(rows[0].name, "basic");
    assert_eq!(rows[0].kind, "Basic group");
    assert_eq!(rows[0].quota, None);
    assert!(!rows[0].skip);
    assert!(rows[0].sub_ids.is_empty());
    assert_eq!(rows[1].name, "empty");
    assert_eq!(rows[1].kind, "Subscription (1)");
    assert_eq!(
        rows[1].quota.as_deref(),
        Some("Last update never refreshed")
    );
    assert_eq!(rows[2].name, "sub");
    assert_eq!(rows[2].kind, "Subscription (1)");
    assert!(rows[2].skip);
    assert_eq!(rows[2].sub_ids, vec![fed.id.clone()]);
    let quota = rows[2].quota.clone().expect("quota line");
    assert!(quota.contains("Last update just now"), "{quota}");
    assert!(quota.contains("Used 2.0 KB"), "{quota}");
    assert!(quota.contains("Remain 1022.0 KB"), "{quota}");
    assert!(quota.contains("Expire 3d left"), "{quota}");
    assert!(quota.contains("auto-refresh skipped"), "{quota}");
    assert_eq!(
        group_quota_line(&[], now),
        None,
        "basic groups have no line"
    );
}

#[test]
fn t08_format_expire_buckets() {
    use manyproxy_ui::format_expire;
    let now = 1_700_000_000;
    assert_eq!(format_expire(now - 1, now), "expired");
    assert_eq!(format_expire(now, now), "expired");
    assert_eq!(format_expire(now + 60, now), "1m left");
    assert_eq!(format_expire(now + 3_600, now), "60m left");
    assert_eq!(format_expire(now + 7_200, now), "2h left");
    assert_eq!(format_expire(now + 259_200, now), "3d left");
}

#[test]
fn t08_update_targets_partition_and_confirm() {
    use manyproxy_ui::{update_all_confirm_text, update_all_targets, update_target_subs};
    use std::collections::HashSet;
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let a = store
        .add_subscription("A", "https://example.com/a.txt", "g1", true)
        .expect("sub works");
    let b = store
        .add_subscription("B", "https://example.com/b.txt", "g1", true)
        .expect("sub works");
    store
        .add_subscription("C", "https://example.com/c.txt", "g2", true)
        .expect("sub works");
    let subs = store.list_subscriptions().expect("subs work");
    let targets = update_target_subs(&subs, "g1");
    assert_eq!(
        targets.iter().map(|sub| sub.id.clone()).collect::<Vec<_>>(),
        vec![a.id.clone(), b.id.clone()]
    );
    assert!(update_target_subs(&subs, "missing").is_empty());
    let flagged: HashSet<String> = ["g2".to_string()].into_iter().collect();
    let (included, skipped) = update_all_targets(subs, &flagged);
    assert_eq!(included.len(), 2);
    assert_eq!(skipped.len(), 1);
    assert_eq!(
        update_all_confirm_text(2, 1),
        "Update 2 subscription(s)? 1 skipped (auto-refresh off for their group)."
    );
    assert_eq!(
        update_all_confirm_text(2, 0),
        "Update all 2 subscription(s)?"
    );
}

#[gpui_kit::test]
fn t08_group_manage_sheet_paints_edit_quota(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("S1", "https://example.com/s1.txt", "travel", true)
        .expect("sub works");
    store
        .refresh_with_body(
            &sub.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
            999,
        )
        .expect("refresh works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_manage_sheet("travel", window, cx)
        });
    });
    paint_dialog_open(cx);
    assert_painted(cx, "group-manage-refresh");
    assert_painted(cx, "group-manage-edit");
    assert_painted(cx, "group-kind-travel");
    assert_painted(cx, "group-quota-travel");
    assert_painted(cx, "group-manage-ok");
    // F13: the footer Close is the sheet's one labelled exit.
    assert_absent(cx, "group-manage-cancel".to_string());
}

#[gpui_kit::test]
fn t08_edit_group_commit_renames_and_flags(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("S1", "https://example.com/s1.txt", "travel", true)
        .expect("sub works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit_group("travel", "team", true, window, cx)
        })
    });
    assert!(closed, "success closes the edit dialog");
    let (flag, group) = view.read_with(cx, |view, _| {
        (
            view.store.group_skip_refresh("team"),
            view.store
                .get_subscription(&sub_id)
                .expect("get works")
                .map(|sub| sub.group),
        )
    });
    assert!(flag, "skip persists");
    assert_eq!(group.as_deref(), Some("team"), "sub follows rename");
    assert_eq!(notification_count(cx), 1, "one outcome toast");
    paint_dialog_open(cx);
    assert_painted(cx, "group-manage-refresh");
    let kept = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit_group("team", "   ", false, window, cx)
        })
    });
    assert!(!kept, "empty rename keeps the dialog open");
}

#[gpui_kit::test]
fn t08_due_subscriptions_respect_skip_flag(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add_subscription("S1", "https://example.com/s1.txt", "travel", true)
        .expect("sub works");
    let (view, cx) = mount_view(store, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.due_subscriptions().len()),
        1,
        "never-refreshed auto sub is due"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .set_group_skip_refresh("travel", true)
                .expect("flag works");
        });
    });
    assert!(
        view.read_with(cx, |view, _| view.due_subscriptions())
            .is_empty(),
        "flagged group never auto-refreshes"
    );
}

#[gpui_kit::test]
fn t08_current_group_follows_the_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.current_group(cx)),
        None,
        "nothing focused"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.select_visible_row(0, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, cx| view.current_group(cx)),
        Some("default".to_string()),
        "the selected row names its group"
    );
}

#[gpui_kit::test]
fn commit_subscription_auto_rereads_current(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .set_subscription_auto_refresh(&sub_id, false)
                .expect("flip works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_subscription_auto(&sub_id, window, cx)
        });
    });
    let current = view.read_with(cx, |view, _| {
        view.store
            .get_subscription(&sub_id)
            .expect("get works")
            .expect("sub there")
            .auto_refresh
    });
    assert!(current, "fresh false toggles back to true (not stale on)");
}

#[test]
fn db_list_error_names_database_never_empty() {
    use manyproxy_ui::db_list_error_message;
    use manyproxy_ui::store::StoreError;
    let text = db_list_error_message(&StoreError::NotFound("7".to_string()));
    assert!(text.contains("Cannot load library"), "names it: {text}");
    assert!(!text.contains("empty"), "never the empty claim: {text}");
}

#[gpui_kit::test]
fn corrupt_db_start_all_errors_never_empty_claim(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.break_schema_for_test().expect("break works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert_eq!(notification_count(cx), 1, "db error notifies once");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts on an unreadable library"
    );
}

#[test]
fn enabled_flag_message_names_row_and_meaning() {
    use manyproxy_ui::enabled_flag_message;
    let on = enabled_flag_message("Work", true);
    assert!(on.contains("Work"), "names the row: {on}");
    assert!(on.contains("restore on launch"), "one semantic: {on}");
    assert!(
        !on.contains("flag only"),
        "the old vague label is gone: {on}"
    );
    let off = enabled_flag_message("Work", false);
    assert!(off.contains("Work"), "names the row: {off}");
    assert!(
        off.contains("will not restore on launch"),
        "the off state names its meaning: {off}"
    );
}

#[gpui_kit::test]
fn toggle_success_toasts_flag_meaning(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_selected_enabled(window, cx));
    });
    assert_eq!(notification_count(cx), 1, "toggle explains itself");
    assert!(
        view.read_with(cx, |view, cx| view
            .selected_config(cx)
            .map(|row| row.enabled)
            .unwrap_or(false)),
        "flag flips"
    );
}

#[gpui_kit::test]
fn start_selected_flag_write_failure_is_loud(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.store
                .set_query_only_for_test()
                .expect("readonly works");
        });
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let id = view.read_with(cx, |view, cx| {
        view.selected_config(cx).expect("selected").id
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
    });
    assert_eq!(notification_count(cx), 1, "flag failure is loud");
}

#[test]
fn unreadable_drop_message_names_basenames() {
    use manyproxy_ui::unreadable_drop_message;
    let text = unreadable_drop_message(&["a.conf".to_string(), "b.txt".to_string()]);
    assert!(text.contains("a.conf") && text.contains("b.txt"), "{text}");
    assert!(!text.contains("Users"), "never full paths: {text}");
}

#[gpui_kit::test]
fn import_drop_names_unreadable_files(cx: &mut TestAppContext) {
    use gpui_kit::ExternalPaths;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let paths = ExternalPaths(
        vec![std::path::PathBuf::from(
            "E:\\definitely-not-here\\ghost.conf",
        )]
        .into(),
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.import_dropped_files(&paths, window, cx));
    });
    assert_eq!(notification_count(cx), 1, "unreadable drop names itself");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
}

#[test]
fn t07_read_import_file_guards() {
    use manyproxy_ui::import::MAX_IMPORT_FILE_BYTES;
    use manyproxy_ui::{read_import_file, ImportFileRead};
    assert_eq!(MAX_IMPORT_FILE_BYTES, 50_000_000, "50MB cap");
    let dir = std::env::temp_dir();
    let tag = format!("manyproxy-t07-guard-{}", std::process::id());
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.join(format!("{tag}-{name}"));
        std::fs::write(&path, bytes).expect("temp write works");
        path
    };
    let ok = write("ok.txt", b"vless://u@a.example.com:443#x");
    assert!(matches!(read_import_file(&ok), ImportFileRead::Text(_)));
    let png = write("img.png", b"\x89PNG\r\n\x1a\nnot-really-an-image");
    assert!(matches!(
        read_import_file(&png),
        ImportFileRead::Refused(reason) if reason.contains("image")
    ));
    let bin = write("bin.dat", b"ab\x00cd");
    assert!(matches!(
        read_import_file(&bin),
        ImportFileRead::Refused(reason) if reason.contains("NUL")
    ));
    let utf = write("utf.dat", b"\xFF\xFE\xFD");
    assert!(matches!(
        read_import_file(&utf),
        ImportFileRead::Refused(_) | ImportFileRead::Unreadable
    ));
    let ghost = dir.join(format!("{tag}-ghost.txt"));
    assert!(matches!(
        read_import_file(&ghost),
        ImportFileRead::Unreadable
    ));
    for path in [ok, png, bin, utf] {
        let _ = std::fs::remove_file(path);
    }
}

#[gpui_kit::test]
fn t07_drop_names_refused_files_with_reasons(cx: &mut TestAppContext) {
    use manyproxy_ui::import::MAX_IMPORT_FILE_BYTES;
    use gpui_kit::ExternalPaths;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let dir = std::env::temp_dir();
    let tag = format!("manyproxy-t07-drop-{}", std::process::id());
    let link_path = dir.join(format!("{tag}-ok.txt"));
    let img_path = dir.join(format!("{tag}-qr.png"));
    let bin_path = dir.join(format!("{tag}-junk.dat"));
    let big_path = dir.join(format!("{tag}-big.txt"));
    std::fs::write(&link_path, b"trojan://p@a.example.com:443#ok").expect("write works");
    std::fs::write(&img_path, b"\x89PNG\r\n\x1a\nfake-qr").expect("write works");
    std::fs::write(&bin_path, b"\x00\x01binary").expect("write works");
    std::fs::write(&big_path, vec![b'x'; (MAX_IMPORT_FILE_BYTES + 1) as usize])
        .expect("write works");
    let paths = ExternalPaths(
        vec![
            link_path.clone(),
            img_path.clone(),
            bin_path.clone(),
            big_path.clone(),
        ]
        .into(),
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.import_dropped_files(&paths, window, cx));
    });
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 1, "one toast names every refusal");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "readable bytes still open the dialog"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "a drop never commits by itself"
    );
    for path in [link_path, img_path, bin_path, big_path] {
        let _ = std::fs::remove_file(path);
    }
}

#[gpui_kit::test]
fn t07_bare_url_chooser_offers_both_paths(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_bare_url_chooser("https://example.com/feed".to_string(), window, cx)
        });
    });
    paint_dialog_open(cx);
    for id in [
        "bare-url-chooser",
        "bare-url-import-proxy",
        "bare-url-add-subscription",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id.to_string()).is_some()),
            "{id} renders"
        );
    }
    cx.update(|window, cx| {
        window.click("bare-url-import-proxy", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.try_find("field-import-text".to_string()).is_some()),
        "proxy arm opens the import dialog"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "opening the dialog stores nothing"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn keyboard_shortcuts_open_and_guard_dialogs(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
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
    press(cx, "ctrl-n");
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl+N opens Add"
    );
    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));

    press(cx, "ctrl-n");
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "dialog stays exactly one"
    );
    assert_eq!(
        notification_count(cx),
        1,
        "T-06: the guarded shortcut toasts the refusal instead of doing nothing"
    );
}

#[gpui_kit::test]
fn keyboard_f5_starts_selected(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t26_engine_seed("Work", "10911"))
        .expect("seed works");
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
        "F5 starts the selected row"
    );
}

#[gpui_kit::test]
fn laneb_keyboard_edit_delete_search(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let press = |cx: &mut VisualTestContext, key: &str| {
        let keystroke = Keystroke::parse(key).expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        });
        cx.run_until_parked();
    };

    // Slice 6: the five literal tabs (and every `Ctrl+1..6`) are gone. The
    // inspector stays exactly as it was when one of those keys is pressed.
    let inspector_before = view.read_with(cx, |view, _| view.inspector_visible);
    for key in ["ctrl-1", "ctrl-2", "ctrl-3", "ctrl-4", "ctrl-5", "ctrl-6"] {
        press(cx, key);
        assert_eq!(
            view.read_with(cx, |view, _| view.inspector_visible),
            inspector_before,
            "{key} no longer opens or closes the inspector"
        );
    }

    let before = notification_count(cx);
    press(cx, "ctrl-f");
    assert_eq!(notification_count(cx), before, "search focus is silent");
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "search focus opens nothing"
    );

    cx.update(|window, cx| window.focus(&focus, cx));
    press(cx, "ctrl-e");
    paint_frame(cx);
    assert_painted(cx, "endpoint-form");
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl-E edits the endpoint, not the full config form"
    );
    view.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
    paint_frame(cx);

    cx.update(|window, cx| window.focus(&focus, cx));
    press(cx, "ctrl-shift-e");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl-Shift-E edits the selected row's config"
    );
    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));
    cx.update(|window, cx| window.close_all_dialogs(cx));

    cx.update(|window, cx| window.focus(&focus, cx));
    let before = notification_count(cx);
    press(cx, "delete");
    // K5: Delete is immediate — no confirm dialog, the row is already gone,
    // and the undo toast is the delete acknowledgment.
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "K5 delete never confirms"
    );
    let remaining = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert!(
        !remaining.iter().any(|row| row.name == "Work"),
        "the selected row is deleted at once: {:?}",
        remaining
            .iter()
            .map(|row| row.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        notification_count(cx),
        before + 1,
        "the undo toast is the delete acknowledgment"
    );

    // The T-06 guard still applies while a dialog is open: select the other
    // row, open its editor, and Delete must refuse instead of acting.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    press(cx, "ctrl-shift-e");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl-Shift-E opens the selected row's config dialog"
    );
    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));
    cx.update(|window, cx| window.focus(&focus, cx));
    let guarded = notification_count(cx);
    press(cx, "delete");
    assert_eq!(
        notification_count(cx),
        guarded + 1,
        "T-06: the guarded Delete toasts the refusal instead of doing nothing"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-name").is_some()),
        "the dialog survives the guarded key"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection(window, cx);
        });
    });
    cx.update(|window, cx| window.focus(&focus, cx));
    let before = notification_count(cx);
    press(cx, "delete");
    assert_eq!(
        notification_count(cx),
        before + 1,
        "Delete with no selection toasts"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "no selection confirms nothing"
    );
}

#[gpui_kit::test]
fn logs_ok_closes_exactly_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        window.click("logs-dialog-ok", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Logs OK closes"
    );
    assert_eq!(notification_count(cx), 0, "filter apply is silent");
}

#[test]
fn refresh_summary_text_names_conflicts() {
    use manyproxy_ui::refresh_summary_text;
    let (warn, text) = refresh_summary_text("Nightly", 1, 0, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[]);
    assert!(!warn);
    assert!(
        text.contains("Nightly") && text.contains("[+]Added 1"),
        "{text}"
    );
    let (warn, text) = refresh_summary_text("Nightly", 1, 0, 0, 0, 0, 0, 2, 0, &[], &[], &[], &[]);
    assert!(warn, "conflicts warn: {text}");
    assert!(
        text.contains('2') && text.contains("conflict"),
        "conflict copy in: {text}"
    );
    assert!(
        text.contains("your edits kept"),
        "kept, not reverted: {text}"
    );
}

#[gpui_kit::test]
fn t70_named_report_renders_markers_and_kept_line(cx: &mut TestAppContext) {
    use manyproxy_ui::refresh_summary_text;
    let (warn, text) = refresh_summary_text(
        "Nightly",
        3,
        2,
        1,
        0,
        0,
        0,
        0,
        1,
        &["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
        &["node-x".to_string(), "node-y".to_string()],
        &["old-node".to_string()],
        &["ws-relay".to_string()],
    );
    assert!(!warn);
    assert_eq!(
        text,
        "Nightly: [+]Added 3 / [~]Updated 2 / [-]Deleted 1 / [=] Still in use 1\n\
             Added 3 profiles:\n\
             [+] alpha\n\
             [+] beta\n\
             [+] gamma\n\
             Updated 2 profiles:\n\
             [~] node-x\n\
             [~] node-y\n\
             Deleted 1 profile:\n\
             [-] old-node\n\
             Still in use, so kept:\n\
             [=] ws-relay"
    );

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.refresh.inflight_named.insert(sub_id.clone());
            view.finish_subscription_refresh(
                &sub_id,
                "Nightly",
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                        .to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 1, "manual refresh pops once");
    let lines = view.read_with(cx, |view, _| {
        view.logs
            .store
            .all()
            .iter()
            .map(|record| record.message.clone())
            .collect::<Vec<_>>()
    });
    assert!(
        lines.iter().any(|line| line
            == "subscription refresh Nightly: [+]Added 1 / [~]Updated 0 / [-]Deleted 0\n\
                    Added 1 profile:\n\
                    [+] one"),
        "manual summary carries the named report: {lines:?}"
    );
}

#[test]
fn t70_report_caps_at_twelve_names_with_more_line() {
    use manyproxy_ui::refresh_summary_text;
    let names: Vec<String> = (0..15).map(|index| format!("node-{index:02}")).collect();
    let (_, text) = refresh_summary_text(
        "Nightly",
        15,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        names.as_slice(),
        &[],
        &[],
        &[],
    );
    let report = text
        .split_once('\n')
        .map(|(_, rest)| rest.to_string())
        .expect("named report block present");
    assert_eq!(
        report.matches("[+] node").count(),
        12,
        "cap renders twelve: {report}"
    );
    assert!(report.contains("[+] node-11"), "last rendered: {report}");
    assert!(
        !report.contains("[+] node-12"),
        "cap hides the rest: {report}"
    );
    assert!(report.ends_with("+ 3 more…"), "overflow marker: {report}");
}

#[gpui_kit::test]
fn t70_auto_refresh_keeps_count_only_text(cx: &mut TestAppContext) {
    use manyproxy_ui::refresh_summary_text;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add_subscription("Alpha", "https://example.com/alpha.txt", "g1", true)
        .expect("seed works");
    let beta = store
        .add_subscription("Beta", "https://example.com/beta.txt", "g2", true)
        .expect("seed works");
    let alpha_id = alpha.id.clone();
    let beta_id = beta.id.clone();
    let (view, cx) = mount_view(store, cx);

    let (warn, text) = refresh_summary_text("Nightly", 0, 0, 1, 0, 0, 0, 0, 1, &[], &[], &[], &[]);
    assert!(!warn);
    assert_eq!(
        text,
        "Nightly: [+]Added 0 / [~]Updated 0 / [-]Deleted 1 / [=] Still in use 1"
    );
    let (_, text) = refresh_summary_text("Nightly", 1, 0, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[]);
    assert_eq!(text, "Nightly: [+]Added 1 / [~]Updated 0 / [-]Deleted 0");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.begin_refresh_batch(std::slice::from_ref(&alpha_id));
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &alpha_id,
                "Alpha",
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                        .to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 1, "one batch, one count-only toast");
    assert!(
        view.read_with(cx, |view, _| view.refresh.added_names.is_empty()),
        "auto refresh never aggregates names"
    );
    let lines = view.read_with(cx, |view, _| {
        view.logs
            .store
            .all()
            .iter()
            .map(|record| record.message.clone())
            .collect::<Vec<_>>()
    });
    assert!(
        lines.iter().all(|line| !line.contains("profiles:")),
        "auto refresh stays count-only: {lines:?}"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.refresh.inflight_named.insert(beta_id.clone());
            view.finish_subscription_refresh(
                &beta_id,
                "Beta",
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@b.example.org:443#one"
                        .to_string(),
                    userinfo: None,
                }),
                1000,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 2, "manual twin pops once");
    let lines = view.read_with(cx, |view, _| {
        view.logs
            .store
            .all()
            .iter()
            .map(|record| record.message.clone())
            .collect::<Vec<_>>()
    });
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("\nAdded 1 profile:\n[+] one")),
        "manual refresh carries the named report: {lines:?}"
    );
}

#[gpui_kit::test]
fn export_payload_restores_library(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let payload = store.export_library_json().expect("export works");

    store.add(seed_input("Extra")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);

    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(payload.clone(), "g".to_string(), None, window, cx)
        })
    });
    assert!(!closed, "restore waits for confirm");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "restore confirm opens"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_library_restore(&payload, window, cx)
        });
    });
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);
    let name = view.read_with(cx, |view, _| {
        view.store.list().expect("list works").remove(0).name
    });
    assert_eq!(name, "Work");
    assert_eq!(notification_count(cx), 1, "restore names its counts");
}

#[gpui_kit::test]
fn probe_dedupe_second_request_warns(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t26_engine_seed("Work", "10912"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, _) = FakeHealthProber::shared();
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    let before = notification_count(cx);
    let first =
        cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(first, 1, "first probe starts");
    let second =
        cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(second, 0, "second probe dedupes");
    cx.run_until_parked();

    assert_eq!(notification_count(cx) - before, 2, "dedupe explains itself");
    assert!(
        !view.read_with(cx, |view, _| view.probe_kind_in_flight(false)),
        "url flag clears on completion"
    );
}

#[gpui_kit::test]
fn t47_probe_flags_isolate_kinds(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let config = store
        .add(t26_engine_seed("Work", "10999"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, _) = FakeHealthProber::shared();
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });

    let started =
        cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(started, 1, "url probe starts");

    let dup = cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(dup, 0, "second url probe dedupes");

    let ping = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.spawn_probe(vec![config.clone()], true, false, None, cx)
        })
    });
    assert_eq!(ping, 1, "ping not blocked by the url tick");
    cx.run_until_parked();
    assert!(
        !view.read_with(cx, |view, _| view.probe_kind_in_flight(false)),
        "url flag clears"
    );
    assert!(
        !view.read_with(cx, |view, _| view.probe_kind_in_flight(true)),
        "ping flag clears"
    );
}

#[test]
fn t47_idle_cadences_minimal() {
    use manyproxy_ui::{STARTUP_REFRESH_DEFER_SECS, TRAY_PUMP_MILLIS};
    assert!(
        (500..=1000).contains(&TRAY_PUMP_MILLIS),
        "tray pump idle-cheap, got {TRAY_PUMP_MILLIS}ms"
    );
    assert!(
        (1..=10).contains(&STARTUP_REFRESH_DEFER_SECS),
        "refresh deferred past first paint, got {STARTUP_REFRESH_DEFER_SECS}s"
    );

    for (name, secs) in [
        ("engine", manyproxy_ui::ENGINE_POLL_TICK_SECS),
        ("traffic", manyproxy_ui::TRAFFIC_TICK_SECS),
        ("reprobe", manyproxy_ui::REPROBE_TICK_SECS),
        ("subs", manyproxy_ui::DUE_SUBSCRIPTION_TICK_SECS),
    ] {
        assert!(secs >= 1, "{name} tick is a single whole-interval sleep");
    }
}

#[test]
fn serving_badge_copy() {
    use manyproxy_ui::serving_badge;
    assert_eq!(
        serving_badge(true, "127.0.0.1", 10801, Some("87ms")),
        "● Running · 127.0.0.1:10801 · 87ms"
    );
    assert_eq!(
        serving_badge(true, "127.0.0.1", 10801, None),
        "● Running · 127.0.0.1:10801"
    );
    assert_eq!(
        serving_badge(true, "127.0.0.1", 10801, Some("  ")),
        "● Running · 127.0.0.1:10801"
    );
    assert_eq!(
        serving_badge(false, "127.0.0.1", 10801, Some("87ms")),
        "○ Stopped · 127.0.0.1:10801"
    );
    assert_eq!(
        serving_badge(false, "127.0.0.1", 10801, None),
        "○ Stopped · 127.0.0.1:10801"
    );
    for canary in ["hunter2", "SECRET-UUID", "token=abc"] {
        assert!(!serving_badge(true, "127.0.0.1", 10801, Some("87ms")).contains(canary));
    }
}

/// K1: the inspector header's badge — one state word, the bind:port, and a
/// latency only while the endpoint still serves. The lamp div carries the
/// color, so the string never wears a glyph dot.
#[test]
fn inspector_badge_copy() {
    use manyproxy_ui::{inspector_badge_text, BadgeState};
    assert_eq!(
        inspector_badge_text(BadgeState::Running, "127.0.0.1", 10801, Some(87)),
        "Running · 127.0.0.1:10801 · 87ms"
    );
    assert_eq!(
        inspector_badge_text(BadgeState::Running, "127.0.0.1", 10801, None),
        "Running · 127.0.0.1:10801",
        "no probe, no latency segment"
    );
    assert_eq!(
        inspector_badge_text(BadgeState::Failed, "127.0.0.1", 10801, Some(87)),
        "Failed · 127.0.0.1:10801",
        "a failing endpoint never wears a stale latency"
    );
    assert_eq!(
        inspector_badge_text(BadgeState::Stopped, "127.0.0.1", 10801, Some(87)),
        "Stopped · 127.0.0.1:10801",
        "a stopped endpoint never wears a stale latency"
    );
    assert_eq!(
        inspector_badge_text(BadgeState::Stopped, "  ", 10801, None),
        "Stopped · :10801",
        "empty bind still prints an honest endpoint"
    );
    for glyph in ["●", "○"] {
        assert!(
            !inspector_badge_text(BadgeState::Running, "127.0.0.1", 10801, Some(87))
                .contains(glyph),
            "the lamp div is the only dot"
        );
    }
}

#[test]
fn t01_status_text_equivalents() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_core::{library_row_lines, serving_badge};
    let row = ProxyConfig {
        id: "1".to_string(),
        name: "paris".to_string(),
        protocol: "vless".to_string(),
        params: "{}".to_string().into(),
        group: "g".to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    assert_eq!(
        serving_badge(true, "127.0.0.1", 10801, Some("live")),
        "● Running · 127.0.0.1:10801 · live"
    );
    let lines = library_row_lines(&row, Some("live"), true, None);
    assert!(
        lines.iter().any(|line| line.contains("paris")),
        "the row names itself: {lines:?}"
    );
}

#[test]
fn row_lines_badge_follows_serving_not_flag() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_core::{library_row_lines, serving_badge};
    fn row() -> ProxyConfig {
        ProxyConfig {
            id: "7".to_string(),
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: "{\"server\":\"a.example.com\",\"server_port\":443}"
                .to_string()
                .into(),
            group: "travel".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let running = library_row_lines(&row(), Some("87ms"), true, None).join("\n");
    assert!(
        running.contains(&serving_badge(true, "127.0.0.1", 10801, Some("87ms"))),
        "running badge in row:\n{running}"
    );
    assert!(
        running.contains("disabled"),
        "flag stays secondary:\n{running}"
    );
    let stopped = library_row_lines(&row(), Some("87ms"), false, None).join("\n");
    assert!(
        stopped.contains("○ Stopped"),
        "stopped badge in row:\n{stopped}"
    );
    assert!(
        !stopped.contains("87ms"),
        "stopped rows show no latency:\n{stopped}"
    );
}

#[test]
fn honest_engine_feedback_copies() {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::{
        already_running_message, crash_notification_text, empty_library_text, no_matches_text,
        no_selection_message, search_count_text, start_all_partial_message, start_busy_message,
    };
    let restarted = already_running_message("Work");
    assert!(restarted.contains("Work"), "{restarted}");
    assert!(restarted.contains("already running"), "{restarted}");
    assert!(restarted.contains("restarted"), "{restarted}");
    assert!(
        !restarted.starts_with("Started \""),
        "never bare Started: {restarted}"
    );
    assert_eq!(no_selection_message(), "Select a configuration first");
    assert!(
        start_busy_message().contains("Starting"),
        "{}",
        start_busy_message()
    );
    let partial = start_all_partial_message(4, &["\"Broken\"".to_string()]);
    assert!(partial.contains('4'), "{partial}");
    assert!(partial.contains('1'), "{partial}");
    assert!(partial.contains("Broken"), "{partial}");

    assert!(manyproxy_ui::is_row_local_start_error(
        &manyproxy_ui::engine::EngineError::BadParams {
            name: "Broken".to_string(),
            field: "server",
        }
    ));
    assert!(!manyproxy_ui::is_row_local_start_error(
        &manyproxy_ui::engine::EngineError::PortConflict {
            bind: "127.0.0.1".to_string(),
            port: 10901,
            first: "Alpha".to_string(),
            second: "Beta".to_string(),
        }
    ));
    assert!(manyproxy_ui::is_port_bind_error(
        &manyproxy_ui::engine::EngineError::PortConflict {
            bind: "127.0.0.1".to_string(),
            port: 10901,
            first: "Alpha".to_string(),
            second: "Beta".to_string(),
        }
    ));
    assert!(!manyproxy_ui::is_port_bind_error(
        &manyproxy_ui::engine::EngineError::BadParams {
            name: "Broken".to_string(),
            field: "server",
        }
    ));
    assert_eq!(search_count_text(0, 5), "0 of 5");
    assert_eq!(search_count_text(4, 5), "4 of 5");
    assert!(
        no_matches_text().contains("No configs match"),
        "{}",
        no_matches_text()
    );
    let empty = empty_library_text();
    assert!(empty.contains("Add"), "{empty}");
    assert!(empty.contains("import"), "{empty}");
    let detail = EngineController::crash_detail(&[
        ("stock".to_string(), "signal: killed".to_string()),
        ("xray".to_string(), "signal: segmentation fault".to_string()),
    ]);
    assert!(detail.contains("stock"), "{detail}");
    assert!(detail.contains("xray"), "{detail}");
    assert!(detail.contains("killed"), "{detail}");
    let single =
        EngineController::crash_detail(&[("stock".to_string(), "signal: killed".to_string())]);
    assert!(
        single.contains("stock"),
        "single crash keeps its shard: {single}"
    );
    let notice = crash_notification_text(&single);
    assert!(notice.contains("stock"), "{notice}");
    assert!(notice.contains("killed"), "{notice}");
    assert!(notice.contains("View logs"), "{notice}");
    for text in [restarted, partial, notice, empty] {
        for canary in ["hunter2", "SECRET-UUID", "token=abc"] {
            assert!(!text.contains(canary), "no secrets in {text}");
        }
    }
}

#[gpui_kit::test]
fn t03_inspector_docks_right_and_drawer_below_the_breakpoint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    // F19: the inspector starts closed; picking the row is the way in.
    select_library_row(&view, cx, &work.id);
    paint_frame_at(cx, 1400., 800.);

    // Docked: the inspector takes its place on the right, the table keeps
    // at least its 640px floor.
    assert_absent(cx, "inspector-drawer".to_string());
    let (list_width, inspector_width) = cx.update(|window, _| {
        let list = window
            .try_find("config-list".to_string())
            .expect("the table paints")
            .bounds();
        let inspector = window
            .try_find("config-detail".to_string())
            .expect("the inspector paints")
            .bounds();
        (list.size.width.as_f32(), inspector.size.width.as_f32())
    });
    assert!(
        list_width >= 640.,
        "the table keeps its 640px floor at 1400px, got {list_width}"
    );
    assert!(
        (240. ..=360.).contains(&inspector_width),
        "the inspector opens at its 300px default (240-360 range), got {inspector_width}"
    );

    // Narrow: the inspector slides over the table as a drawer, and the list
    // pane reserves the drawer's share so the table keeps its 640px floor.
    paint_frame_at(cx, 1000., 700.);
    assert_painted(cx, "inspector-drawer");
    let (narrow_table, narrow_drawer) = cx.update(|window, _| {
        let list = window
            .try_find("config-list".to_string())
            .expect("the table paints")
            .bounds();
        let drawer = window
            .try_find("inspector-drawer".to_string())
            .expect("the drawer paints")
            .bounds();
        (
            list.size.width.as_f32() - drawer.size.width.as_f32(),
            drawer.size.width.as_f32(),
        )
    });
    assert_eq!(narrow_drawer, 300., "the drawer opens at its 300px width");
    assert!(
        narrow_table >= 640.,
        "the table keeps its 640px floor in the drawer, got {narrow_table}"
    );

    // Esc closes layers in order: the drawer first, the selection second.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx);
        });
    });
    cx.run_until_parked();
    let (open, selected) = view.read_with(cx, |view, _cx| {
        (view.inspector_visible, view.selection_count())
    });
    assert!(!open, "the first Esc closes the drawer");
    assert_eq!(selected, 1, "the selection survives the first Esc");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx);
        });
    });
    cx.run_until_parked();
    let (open, selected) = view.read_with(cx, |view, _cx| {
        (view.inspector_visible, view.selection_count())
    });
    assert!(!open, "the drawer stays closed");
    assert_eq!(selected, 0, "the second Esc clears the selection");
}

/// F1: the drawer is an input-occluding overlay. A click on its blank padding
/// at the y of a non-selected row must not reach — and select — that row.
#[gpui_kit::test]
fn t03_inspector_drawer_occludes_blank_clicks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let _first = store
        .add(facet_seed("First", "vless", "team", "10801"))
        .expect("seed works");
    let selected_row = store
        .add(facet_seed("Second", "vless", "team", "10802"))
        .expect("seed works");
    let other_row = store
        .add(facet_seed("Third", "vless", "team", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &selected_row.id);

    // The drawer's p_2 padding is blank: 3px inside its left edge sits on no
    // control, so pre-fix the click falls through to the row card at that y.
    let offset = cx.update(|window, _| {
        let drawer = window
            .try_find("inspector-drawer")
            .expect("the drawer paints")
            .bounds();
        let covered = window
            .try_find(format!("row-surface-{}", other_row.id))
            .expect("the non-selected row paints")
            .bounds();
        point(px(3.), covered.center().y - drawer.origin.y)
    });
    cx.update(|window, cx| window.click_at("inspector-drawer", offset, cx));
    cx.run_until_parked();
    paint_frame(cx);

    let selected = view.read_with(cx, |view, _cx| view.selection_focus.clone());
    assert_eq!(
        selected.as_deref(),
        Some(selected_row.id.as_str()),
        "a blank drawer click leaves the inspector's subject alone"
    );

    // The occluding root must not swallow the drawer's own controls.
    cx.update(|window, cx| window.click("inspector-drawer-close", cx));
    cx.run_until_parked();
    paint_frame(cx);
    let open = view.read_with(cx, |view, _| view.inspector_visible);
    assert!(
        !open,
        "the close button still works under the drawer's occluding hitbox"
    );
}

/// F2: the layout contract is pure, so both presentations pin exact numbers —
/// including the 880px clamp that keeps the drawer table on its 640px floor.
#[test]
fn t03_inspector_layout_pins_the_drawer_contract() {
    use manyproxy_ui::app::render::InspectorLayout;

    let cases = [
        (1000., 260., false, true, 260., 740.),
        (1000., 260., false, false, 0., 1000.),
        (880., 260., false, true, 240., 640.),
        (1080., 260., true, true, 0., 820.),
        // F19: a closed inspector costs the table nothing in either mode.
        (1080., 260., true, false, 0., 1080.),
        (1400., 260., true, false, 0., 1400.),
    ];
    for (viewport, inspector, docked, visible, drawer_w, table_w) in cases {
        let layout = InspectorLayout::compute(viewport, inspector, docked, visible);
        assert_eq!(
            layout.drawer_w, drawer_w,
            "drawer at {viewport}px, docked={docked}, visible={visible}"
        );
        assert_eq!(
            layout.table_w, table_w,
            "table at {viewport}px, docked={docked}, visible={visible}"
        );
    }
}

/// F19: the inspector starts closed in both presentations — a fresh view
/// paints no drawer and no docked pane, so the table takes the whole frame.
#[gpui_kit::test]
fn f19_inspector_starts_closed_in_both_presentations(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    let row_width = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find(format!("library-row-{}", work.id))
                .expect("the row paints")
                .bounds()
                .size
                .width
                .as_f32()
        })
    };

    // Below the breakpoint: no drawer, and the row owns the full frame.
    paint_frame_at(cx, 1000., 700.);
    assert_absent(cx, "inspector-drawer".to_string());
    assert_absent(cx, "config-detail".to_string());
    let narrow = row_width(cx);
    assert!(
        (narrow - 944.).abs() <= 1.,
        "a closed drawer costs the table only the nav's width, got {narrow}"
    );

    // Docked: the split pane's inspector side is gone, not just empty.
    paint_frame_at(cx, 1400., 800.);
    assert_absent(cx, "config-detail".to_string());
    let wide = row_width(cx);
    assert!(
        (wide - 1180.).abs() <= 1.,
        "a closed dock takes no split space, got {wide}"
    );
    assert!(!view.read_with(cx, |view, _| view.inspector_visible));
}

/// F19: a row pick is the way in and Esc is the way out — the same flag, the
/// same result, in both presentations.
#[gpui_kit::test]
fn f19_selection_opens_and_esc_closes_in_both_presentations(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let next = store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    // Drawer: the real click path opens it.
    paint_frame_at(cx, 1000., 700.);
    select_library_row(&view, cx, &work.id);
    assert_painted(cx, "inspector-drawer");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 700.);
    assert_absent(cx, "inspector-drawer".to_string());
    assert!(!view.read_with(cx, |view, _| view.inspector_visible));

    // Docked: the same click opens the split pane, the same Esc closes it.
    paint_frame_at(cx, 1400., 800.);
    cx.update(|window, cx| window.click(format!("row-name-text-{}", next.id), cx));
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 800.);
    assert_painted(cx, "config-detail");
    assert_absent(cx, "inspector-drawer".to_string());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_unless_dialog(window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1400., 800.);
    assert_absent(cx, "config-detail".to_string());
    assert!(!view.read_with(cx, |view, _| view.inspector_visible));
}

/// F19: `Ctrl+I` owns presence, so it toggles. The old per-pane tab actions
/// are gone with the tab strip.
#[gpui_kit::test]
fn f19_inspector_shortcut_toggles_presence(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 700.);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.focus.focus(window, cx));
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(manyproxy_ui::ToggleInspector), cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 700.);
    assert_painted(cx, "inspector-drawer");

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(manyproxy_ui::ToggleInspector), cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 700.);
    assert_absent(cx, "inspector-drawer".to_string());

    cx.update(|window, cx| {
        window.dispatch_action(Box::new(manyproxy_ui::ToggleInspector), cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 700.);
    assert_painted(cx, "inspector-drawer");
    assert!(
        cx.update(|window, _| window.try_find("inspector-tabs".to_string()).is_none()),
        "the inspector opens stacked, with no tab strip"
    );
}

/// F11: Start leads while the endpoint is stopped. Stop and Test stay live
/// everywhere and answer a stopped row with an honest toast, never by faking
/// their own unavailability.
#[gpui_kit::test]
fn f11_operate_buttons_follow_the_running_state(cx: &mut TestAppContext) {
    let has_spinner = |cx: &mut VisualTestContext, id: &str| {
        cx.update(|window, _| window.try_find(id.to_string()).is_some())
    };
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t26_engine_seed("Work", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let (engine, _) = manyproxy_ui::engine::EngineController::fake();
            view.engine = engine;
            let (prober, _) = manyproxy_ui::health::FakeHealthProber::shared();
            view.prober = Box::new(prober);
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1000., 1600.);

    let notifications = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-stop", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "a stopped endpoint has nothing to stop"
    );
    assert_eq!(
        notification_count(cx),
        notifications + 1,
        "a stopped Stop answers with the already-stopped toast"
    );

    cx.update(|window, cx| {
        window.click("health-test", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    assert!(
        !has_spinner(cx, "health-test-spinner"),
        "a stopped endpoint has nothing to test"
    );
    assert_eq!(
        notification_count(cx),
        notifications + 2,
        "a stopped Test answers with the not-running toast"
    );

    cx.update(|window, cx| {
        window.click("engine-start", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "start stays live while stopped"
    );

    let after_start = notification_count(cx);
    cx.update(|window, cx| {
        window.click("health-test", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    assert!(
        notification_count(cx) > after_start,
        "test wakes once the endpoint runs"
    );
}

/// F11: the header is the acting echo and the serving badge — the protocol and
/// group tags that used to bracket them are gone.
#[gpui_kit::test]
fn f11_inspector_header_is_the_acting_echo_and_serving_badge(cx: &mut TestAppContext) {
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

    assert_painted(cx, "detail-header-line");
    assert_painted(cx, "detail-acting-on");
    assert_painted(cx, "detail-serving");
}

/// R11: the table never hides a visible column to make the widths fit. The
/// default set plus Upstream exceeds 740px, so the content scrolls instead.
#[test]
fn t03_row_metrics_keep_every_visible_column() {
    use manyproxy_ui::{ColumnId, ColumnLayout, RowMetrics};

    let mut layout = ColumnLayout::default();
    layout.set_hidden(ColumnId::Upstream, false);
    let at_740 = RowMetrics::for_table_width(740., layout);
    assert!(
        at_740.column_visible(ColumnId::Upstream),
        "upstream stays visible at 740"
    );
    assert!(
        at_740.column_visible(ColumnId::Health),
        "health is never budgeted away"
    );
    assert!(
        layout.content_width(740.) > 740.,
        "a too-wide table scrolls instead of dropping a column"
    );

    // The default visible set (Name, Endpoint, Health, Group) still fits the
    // floor with the leading and trailing reserves.
    let default_content = ColumnLayout::default().content_width(740.);
    assert!(
        default_content <= 740.,
        "the signed default fits the floor: {default_content}"
    );
    assert_eq!(
        RowMetrics::for_table_width(740., ColumnLayout::default()).column_width(ColumnId::Name),
        740. - RowMetrics::LEAD
            - RowMetrics::ACTIONS
            - RowMetrics::ENDPOINT
            - RowMetrics::HEALTH
            - RowMetrics::GROUP
            - 3. * RowMetrics::GAP,
        "an unset Name absorbs the leftover"
    );
}

/// T-03: one gesture from edge to edge: the app layout follows every move, the
/// store only hears about the final width, and a double-click restores the
/// computed default.
#[gpui_kit::test]
fn t03_column_drag_resizes_persists_and_double_click_resets(cx: &mut TestAppContext) {
    use manyproxy_ui::{ColumnId, RowMetrics};
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Resize", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.inspector_visible = false;
        cx.notify();
    });
    paint_frame(cx);

    assert_eq!(
        view.read_with(cx, |view, cx| {
            view.libraries
                .read(cx)
                .delegate()
                .row_metrics
                .column_width(ColumnId::Health)
        }),
        RowMetrics::HEALTH,
        "the persisted default starts at the computed width"
    );

    drag_column_edge(cx, "column-resize-health", 40.);
    let (live, saved) = view.read_with(cx, |view, cx| {
        (
            view.libraries
                .read(cx)
                .delegate()
                .row_metrics
                .column_width(ColumnId::Health),
            view.store.library_column_layout().width(ColumnId::Health),
        )
    });
    assert_eq!(live, RowMetrics::HEALTH + 40., "the row and header reflow");
    assert_eq!(saved, live, "mouse-up writes the final width");

    drag_column_edge(cx, "column-resize-endpoint", -500.);
    let endpoint = view.read_with(cx, |view, cx| {
        view.libraries
            .read(cx)
            .delegate()
            .row_metrics
            .column_width(ColumnId::Endpoint)
    });
    assert_eq!(
        endpoint,
        ColumnId::Endpoint.min_width(),
        "a drag past the floor stops at the column minimum"
    );

    double_click_column_edge(cx, "column-resize-health");
    let (live, saved) = view.read_with(cx, |view, cx| {
        (
            view.libraries
                .read(cx)
                .delegate()
                .row_metrics
                .column_width(ColumnId::Health),
            view.store.library_column_layout().width(ColumnId::Health),
        )
    });
    assert_eq!(
        live,
        RowMetrics::HEALTH,
        "double-click restores the default"
    );
    assert_eq!(saved, RowMetrics::HEALTH, "the reset persists");
}

/// R11/R7: every header opens the same Explorer-style menu — two fit verbs,
/// seven checkmarked visibility rows, and More. Locked rows are checked and
/// disabled; the sheet's Reset All clears every override.
#[gpui_kit::test]
fn t03_column_header_menu_and_sheet_reset(cx: &mut TestAppContext) {
    use manyproxy_ui::{ColumnId, ColumnLayout, RowMetrics};
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Menu", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.inspector_visible = false;
        cx.notify();
    });
    paint_frame(cx);

    cx.update(|window, cx| {
        window.right_click("sort-header-health".to_string(), cx);
    });
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
        assert_painted(cx, id);
    }
    assert_absent(cx, "sort-header-health-latency".to_string());

    // Every header's menu is the same menu, Name included.
    cx.update(|window, cx| {
        window.right_click("sort-header-name".to_string(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "columns-menu-size-this");
    assert_painted(cx, "columns-menu-more");

    // A visibility row flips one column and persists; Name refuses to hide.
    cx.update(|window, cx| window.click("columns-menu-group".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    let hidden = view.read_with(cx, |view, _| view.column_layout);
    assert!(!hidden.is_visible(ColumnId::Group), "group hides");
    assert_eq!(
        hidden,
        view.read_with(cx, |view, _| view.store.library_column_layout()),
        "the hide persists"
    );
    assert_absent(cx, "sort-header-group".to_string());

    cx.update(|window, cx| {
        window.right_click("sort-header-name".to_string(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Name),
        "a disabled locked row cannot hide Name"
    );

    // Size to fit writes an explicit width; it stops Name flexing and persists.
    cx.update(|window, cx| {
        window.right_click("sort-header-endpoint".to_string(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-size-this".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    let fitted = view.read_with(cx, |view, _| view.column_layout);
    assert!(
        fitted.is_custom(ColumnId::Endpoint),
        "fit writes an explicit width"
    );
    assert_eq!(
        fitted,
        view.read_with(cx, |view, _| view.store.library_column_layout()),
        "the fitted width persists"
    );
    assert!(fitted.width(ColumnId::Endpoint) >= ColumnId::Endpoint.min_width());

    // More opens the sheet; Reset All restores every dimension.
    cx.update(|window, cx| {
        window.right_click("sort-header-health".to_string(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-more".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_sheet(cx)),
        "More opens the columns sheet"
    );
    cx.update(|window, cx| {
        window.click("columns-sheet-reset-all".to_string(), cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    let (layout, saved) = view.read_with(cx, |view, _| {
        (view.column_layout, view.store.library_column_layout())
    });
    assert_eq!(
        layout,
        ColumnLayout::default(),
        "Reset All clears overrides"
    );
    assert_eq!(saved, ColumnLayout::default(), "the reset persists");
    assert_eq!(
        layout.width(ColumnId::Health),
        RowMetrics::HEALTH,
        "the default width is back"
    );
}

/// R11: the same Explorer menu opens on every header, Name and Endpoint are
/// the locked rows, and the other five toggle from it.
#[gpui_kit::test]
fn header_menu_is_identical_on_every_column(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Menu", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            view.set_column_visible(ColumnId::Traffic, true, window, cx);
            cx.notify();
        });
    });
    paint_frame(cx);

    let menu_ids = [
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
    ];
    for header in [
        "sort-header-name",
        "sort-header-endpoint",
        "sort-header-traffic",
    ] {
        cx.update(|window, cx| window.right_click(header.to_string(), cx));
        cx.run_until_parked();
        paint_frame(cx);
        for id in menu_ids {
            assert_painted(cx, id);
        }
        cx.update(|window, cx| {
            let escape = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
            window.dispatch_keystroke(escape, cx);
        });
        cx.run_until_parked();
    }

    // Locked rows refuse; the other columns obey.
    cx.update(|window, cx| window.right_click("sort-header-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Name),
        "Name cannot be hidden from the menu"
    );

    cx.update(|window, cx| window.right_click("sort-header-traffic".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-traffic".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !view
            .read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Traffic),
        "Traffic hides from the same menu"
    );
}

/// R7: every visible column sorts from its header; a second click reverses it
/// and the marker names the direction.
#[gpui_kit::test]
fn header_click_sorts_every_visible_column_and_reverses(cx: &mut TestAppContext) {
    use manyproxy_ui::store::{LibrarySort, LibrarySortKey, SortDirection};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    store
        .add(row19_seed("Zulu", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            view.set_column_visible(ColumnId::Protocol, true, window, cx);
            view.set_column_visible(ColumnId::Traffic, true, window, cx);
            cx.notify();
        });
    });
    paint_frame(cx);

    for (header, key) in [
        ("sort-header-name", LibrarySortKey::Name),
        ("sort-header-protocol", LibrarySortKey::Protocol),
        ("sort-header-traffic", LibrarySortKey::TrafficTotal),
    ] {
        cx.update(|window, cx| window.click(header.to_string(), cx));
        cx.run_until_parked();
        paint_frame(cx);
        assert_eq!(
            view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().sort),
            LibrarySort::new(key, SortDirection::Ascending),
            "{header} picks its natural key"
        );
        assert!(
            label_of(header, cx).contains("asc"),
            "{header} shows the ascending marker"
        );

        cx.update(|window, cx| window.click(header.to_string(), cx));
        cx.run_until_parked();
        paint_frame(cx);
        assert_eq!(
            view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().sort),
            LibrarySort::new(key, SortDirection::Descending),
            "{header} reverses on the second click"
        );
        assert!(
            label_of(header, cx).contains("desc"),
            "{header} shows the descending marker"
        );
    }
}

/// R11: the menu's checkmarks toggle visibility and the header follows.
#[gpui_kit::test]
fn header_menu_checkmarks_toggle_visibility(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Toggle", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.inspector_visible = false;
        cx.notify();
    });
    paint_frame(cx);
    assert_absent(cx, "sort-header-protocol".to_string());

    cx.update(|window, cx| window.right_click("sort-header-health".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-protocol".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Protocol),
        "Protocol is on"
    );
    assert_painted(cx, "sort-header-protocol");
    assert_painted(cx, &format!("row-protocol-{}", row.id));

    cx.update(|window, cx| window.right_click("sort-header-health".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-protocol".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !view
            .read_with(cx, |view, _| view.column_layout)
            .is_visible(ColumnId::Protocol),
        "Protocol is off again"
    );
    assert_absent(cx, "sort-header-protocol".to_string());
    assert_absent(cx, format!("row-protocol-{}", row.id));

    cx.update(|window, cx| window.right_click("sort-header-health".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-group".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, "sort-header-group".to_string());
    assert_absent(cx, format!("row-group-{}", row.id));
}

/// R17: fit measures every row, not the virtualized page. The long name sits
/// far below the fold and still widens the column.
#[gpui_kit::test]
fn size_column_to_fit_uses_the_widest_value_not_the_visible_page(cx: &mut TestAppContext) {
    use manyproxy_ui::RowMetrics;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    for index in 0..40 {
        store
            .add(row19_seed(
                &format!("short {index}"),
                "a.example.com",
                &format!("{}", 10801 + index),
            ))
            .expect("seed works");
    }
    store
        .add(row19_seed(
            "A very long configuration name that lives below the virtualized fold",
            "a.example.com",
            "10999",
        ))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.inspector_visible = false;
        cx.notify();
    });
    paint_frame(cx);

    cx.update(|window, cx| window.right_click("sort-header-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-size-this".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);

    let layout = view.read_with(cx, |view, _| view.column_layout);
    assert!(
        layout.is_custom(ColumnId::Name),
        "fit writes an explicit width"
    );
    assert!(
        layout.width(ColumnId::Name) > RowMetrics::NAME_MIN + 60.,
        "the off-screen name, not the short page, sets the width: {}",
        layout.width(ColumnId::Name)
    );
}

/// R11/R17: Size All fits every visible column in one write; with all seven
/// visible the fitted table is wider than the pane and scrolls.
#[gpui_kit::test]
fn size_all_columns_to_fit_starts_horizontal_scroll_when_needed(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed(
            "A configuration name long enough to widen the fitted table",
            "a-very-long-upstream-hostname-for-fit.example.com",
            "10801",
        ))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            for column in ColumnId::ALL {
                view.set_column_visible(column, true, window, cx);
            }
            cx.notify();
        });
    });
    paint_frame(cx);

    cx.update(|window, cx| window.right_click("sort-header-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-size-all".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);

    let layout = view.read_with(cx, |view, _| view.column_layout);
    for column in ColumnId::ALL {
        assert!(layout.is_visible(column), "{column:?} stays visible");
        assert!(layout.is_custom(column), "{column:?} is fitted");
        let id = format!("sort-header-{}", column.id());
        assert!(
            cx.update(|window, _| window.try_find(id.clone()).is_some()),
            "{id} is still in the tree"
        );
    }
    assert!(
        layout.content_width(0.) > 1000.,
        "the fitted table is wider than the frame: {}",
        layout.content_width(0.)
    );
}

/// R11: Name and Endpoint refuse both visibility routes; the menu row is
/// disabled and the sheet checkbox is disabled.
#[gpui_kit::test]
fn locked_visibility_controls_are_disabled_in_the_sheet_and_menu(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Locked", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|window, cx| window.right_click("sort-header-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.right_click("sort-header-name".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("columns-menu-endpoint".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    let after_menu = view.read_with(cx, |view, _| view.column_layout);
    assert!(after_menu.is_visible(ColumnId::Name));
    assert!(after_menu.is_visible(ColumnId::Endpoint));

    cx.update(|window, cx| view.update(cx, |view, cx| view.open_columns_sheet(window, cx)));
    cx.run_until_parked();
    paint_dialog_open(cx);
    cx.update(|window, cx| window.click("columns-sheet-toggle-name".to_string(), cx));
    cx.update(|window, cx| window.click("columns-sheet-toggle-endpoint".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let after_sheet = view.read_with(cx, |view, _| view.column_layout);
    assert!(after_sheet.is_visible(ColumnId::Name));
    assert!(after_sheet.is_visible(ColumnId::Endpoint));
}

/// R11: the sheet's up/down buttons call the same `move_column` the header drag
/// uses and persist on each click; locked columns still reorder.
#[gpui_kit::test]
fn columns_sheet_reorders_with_buttons_and_persists(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Reorder", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_columns_sheet(window, cx)));
    cx.run_until_parked();
    paint_dialog_open(cx);

    cx.update(|window, cx| window.click("columns-sheet-down-name".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let moved = view.read_with(cx, |view, _| view.column_layout);
    assert_eq!(moved.order()[0], ColumnId::Endpoint, "Endpoint moves up");
    assert_eq!(moved.order()[1], ColumnId::Name, "Name moves down");
    assert_eq!(
        moved,
        view.read_with(cx, |view, _| view.store.library_column_layout()),
        "the button write persists"
    );

    // Locked rows are disabled; clicking them changes nothing.
    cx.update(|window, cx| window.click("columns-sheet-toggle-name".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    cx.update(|window, cx| window.click("columns-sheet-toggle-endpoint".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let after = view.read_with(cx, |view, _| view.column_layout);
    assert!(after.is_visible(ColumnId::Name), "Name stays visible");
    assert!(
        after.is_visible(ColumnId::Endpoint),
        "Endpoint stays visible"
    );

    cx.update(|window, cx| window.click("columns-sheet-up-name".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_eq!(
        view.read_with(cx, |view, _| *view.column_layout.order()),
        ColumnId::ALL,
        "Move up restores the default order"
    );
}

/// R11: each reset verb changes only the dimension it names; Reset All restores
/// every default.
#[gpui_kit::test]
fn reset_widths_order_visibility_and_all(cx: &mut TestAppContext) {
    use manyproxy_ui::RowMetrics;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Reset", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut layout = view.column_layout;
            layout.set_width(ColumnId::Health, 140.);
            layout.set_hidden(ColumnId::Group, true);
            layout.set_hidden(ColumnId::Traffic, false);
            layout.move_column(ColumnId::Traffic, ColumnId::Name, false);
            view.commit_column_layout(layout, window, cx);
            view.open_columns_sheet(window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);

    cx.update(|window, cx| window.click("columns-sheet-reset-widths".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let widths = view.read_with(cx, |view, _| view.column_layout);
    assert_eq!(widths.width(ColumnId::Health), RowMetrics::HEALTH);
    assert_eq!(widths.order()[0], ColumnId::Traffic, "order untouched");
    assert!(!widths.is_visible(ColumnId::Group), "visibility untouched");
    assert!(widths.is_visible(ColumnId::Traffic));

    cx.update(|window, cx| window.click("columns-sheet-reset-order".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let order = view.read_with(cx, |view, _| view.column_layout);
    assert_eq!(order.order(), &ColumnId::ALL);
    assert!(!order.is_visible(ColumnId::Group), "visibility untouched");
    assert!(order.is_visible(ColumnId::Traffic));

    cx.update(|window, cx| window.click("columns-sheet-reset-visibility".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let visibility = view.read_with(cx, |view, _| view.column_layout);
    assert_eq!(visibility, ColumnLayout::default());
    assert!(visibility.is_visible(ColumnId::Group));
    assert!(!visibility.is_visible(ColumnId::Traffic));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut layout = view.column_layout;
            layout.set_width(ColumnId::Health, 140.);
            layout.move_column(ColumnId::Traffic, ColumnId::Name, false);
            view.commit_column_layout(layout, window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    cx.update(|window, cx| window.click("columns-sheet-reset-all".to_string(), cx));
    cx.run_until_parked();
    paint_dialog_open(cx);
    let all = view.read_with(cx, |view, _| view.column_layout);
    assert_eq!(all, ColumnLayout::default());
    assert_eq!(
        all,
        view.read_with(cx, |view, _| view.store.library_column_layout()),
        "Reset All persists"
    );
}

/// R11: one horizontal scroll container owns the header and the rows, so a
/// scroll moves both by the same amount.
#[gpui_kit::test]
fn horizontal_scroll_moves_header_and_rows_together(cx: &mut TestAppContext) {
    use gpui_kit::ScrollDelta;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Scroll", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            let mut layout = view.column_layout;
            for column in ColumnId::ALL {
                layout.set_hidden(column, false);
                layout.set_width(column, 200.);
            }
            view.commit_column_layout(layout, window, cx);
        });
    });
    paint_frame_at(cx, 1100., 700.);

    let header_left = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find("sort-header-name".to_string())
                .expect("Name header renders")
                .bounds()
                .origin
                .x
                .as_f32()
        })
    };
    let row_left = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find(format!("row-name-{}", row.id))
                .expect("Name cell renders")
                .bounds()
                .origin
                .x
                .as_f32()
        })
    };
    let before_header = header_left(cx);
    let before_row = row_left(cx);
    assert!(
        (before_header - before_row).abs() < 1.,
        "header and row start at the same x: {before_header} vs {before_row}"
    );

    cx.update(|window, cx| {
        window.scroll(
            "library-hscroll",
            ScrollDelta::Pixels(point(px(-250.), px(0.))),
            cx,
        );
    });
    paint_frame_at(cx, 1100., 700.);
    let after_header = header_left(cx);
    let after_row = row_left(cx);
    assert!(
        before_header - after_header > 100.,
        "the scroll moved the header left: {before_header} -> {after_header}"
    );
    assert!(
        (after_header - after_row).abs() < 1.,
        "the rows keep the header's offset: {after_header} vs {after_row}"
    );
}

/// R11: at 1100px a wide visible layout scrolls; no column is auto-hidden.
#[gpui_kit::test]
fn narrow_window_never_auto_hides_a_visible_column(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Narrow", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            let mut layout = view.column_layout;
            for column in ColumnId::ALL {
                layout.set_hidden(column, false);
                layout.set_width(column, 200.);
            }
            view.commit_column_layout(layout, window, cx);
        });
    });
    paint_frame_at(cx, 1100., 700.);

    for column in ColumnId::ALL {
        let id = format!("sort-header-{}", column.id());
        assert!(
            cx.update(|window, _| window.try_find(id.clone()).is_some()),
            "{id} is still in the tree"
        );
    }
    let layout = view.read_with(cx, |view, _| view.column_layout);
    assert!(
        layout.content_width(0.) > 1100.,
        "the layout is wider than the window and scrolls: {}",
        layout.content_width(0.)
    );
}

/// Persistence: a corrupt v2 never falls back to the legacy string, and the
/// lock is enforced even for a payload that tries to hide Name.
#[gpui_kit::test]
fn corrupt_layout_ignores_legacy_and_keeps_name_endpoint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Corrupt", "a.example.com", "10801"))
        .expect("seed works");
    store
        .set_meta(Store::META_LIBRARY_COLUMN_LAYOUT, "{not json")
        .expect("corrupt layout writes");
    store
        .set_meta(Store::META_LIBRARY_COLUMNS, "upstream,health,traffic")
        .expect("legacy seed writes");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let layout = view.read_with(cx, |view, _| view.column_layout);
    assert_eq!(layout, ColumnLayout::default(), "corrupt reads defaults");
    assert!(layout.is_visible(ColumnId::Name));
    assert!(layout.is_visible(ColumnId::Endpoint));
    assert!(
        layout.is_visible(ColumnId::Group),
        "the legacy set is not resurrected"
    );
    assert!(!layout.is_visible(ColumnId::Protocol));
}

/// Persistence: the live drag updates the app and leaves the store alone;
/// mouse-up writes the final layout.
#[gpui_kit::test]
fn layout_persists_on_resize_finish_or_drop_finish(cx: &mut TestAppContext) {
    use manyproxy_ui::RowMetrics;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Persist", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let before = view.read_with(cx, |view, _| view.store.library_column_layout());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_column_resize(ColumnId::Health, 0., RowMetrics::HEALTH, cx);
            view.drag_column_resize(40., cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.store.library_column_layout()),
        before,
        "the store waits for mouse-up"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.column_layout.width(ColumnId::Health)),
        RowMetrics::HEALTH + 40.,
        "the live layout follows the pointer"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.finish_column_resize(window, cx));
    });
    cx.run_until_parked();
    let saved = view.read_with(cx, |view, _| view.store.library_column_layout());
    assert_eq!(
        saved.width(ColumnId::Health),
        RowMetrics::HEALTH + 40.,
        "mouse-up persists the final width"
    );
}

/// T-04/R11: a header drag carries the column and its sort behavior. The drop
/// line paints before release, the new order persists, and Name participates
/// like every other column.
#[gpui_kit::test]
fn t04_column_drag_reorders_persists_and_keeps_sort(cx: &mut TestAppContext) {
    use manyproxy_ui::{ColumnId, LibrarySortKey};
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Drag", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            // Protocol is an opt-in column; the drag needs it painted.
            view.set_column_visible(ColumnId::Protocol, true, window, cx);
            cx.notify();
        });
    });
    paint_frame(cx);

    let left = |cx: &mut VisualTestContext, id: &str| -> f32 {
        cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
                .origin
                .x
                .as_f32()
        })
    };

    drag_column_header(
        cx,
        "sort-header-health",
        "sort-header-protocol",
        false,
        Some("column-drop-protocol"),
    );

    let (layout, saved) = view.read_with(cx, |view, _| {
        (view.column_layout, view.store.library_column_layout())
    });
    assert_eq!(layout, saved, "the new order is written on release");
    assert_eq!(layout.order()[1], ColumnId::Endpoint);
    assert_eq!(layout.order()[2], ColumnId::Group);
    assert_eq!(layout.order()[3], ColumnId::Health);
    assert_eq!(layout.order()[4], ColumnId::Protocol);

    let health = left(cx, "sort-header-health");
    assert!(
        left(cx, "sort-header-name") < health && health < left(cx, "sort-header-protocol"),
        "Health now paints between the Name and Protocol headers"
    );
    let row_health = left(cx, &format!("row-health-{}", row.id));
    assert!(
        left(cx, &format!("row-name-{}", row.id)) < row_health
            && row_health < left(cx, &format!("row-protocol-{}", row.id)),
        "the row follows the header order"
    );

    // The sort gesture belongs to the column, not the slot it used to occupy.
    cx.update(|window, cx| window.click("sort-header-health".to_string(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    let sort = view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().sort);
    assert_eq!(
        sort.key,
        LibrarySortKey::Latency,
        "clicking the moved Health header still sorts by latency"
    );
}

/// R11: locked means "cannot hide", not "cannot move". Name is a normal drop
/// target and a normal drag source.
#[gpui_kit::test]
fn t04_name_column_participates_in_reordering(cx: &mut TestAppContext) {
    use manyproxy_ui::ColumnId;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Fixed", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.inspector_visible = false;
            view.set_column_visible(ColumnId::Protocol, true, window, cx);
            cx.notify();
        });
    });
    paint_frame(cx);

    begin_column_drag(cx, "sort-header-protocol");
    move_column_drag(cx, "sort-header-name", false);
    assert_painted(cx, "column-drop-name");

    release_column_drag(cx, "sort-header-name", false);
    let (layout, saved) = view.read_with(cx, |view, _| {
        (view.column_layout, view.store.library_column_layout())
    });
    assert_eq!(layout.order()[0], ColumnId::Protocol);
    assert_eq!(layout.order()[1], ColumnId::Name);
    assert_eq!(layout, saved, "the reorder persists");
}

/// F2: at the default 1000px frame the table reserves the drawer's 260px, so
/// the row Copy action paints left of the drawer instead of under it.
#[gpui_kit::test]
fn t03_drawer_table_keeps_copy_left_of_the_drawer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(facet_seed("Drawer", "vless", "team", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &row.id);
    paint_frame(cx);

    assert_painted(cx, "inspector-drawer");
    let copy_id = format!("row-endpoint-chip-{}", row.id);
    assert_painted(cx, &copy_id);

    let (copy_right, drawer_left) = cx.update(|window, _| {
        let copy = window
            .try_find(copy_id.clone())
            .expect("the row copy button paints")
            .bounds();
        let drawer = window
            .try_find("inspector-drawer")
            .expect("the drawer paints")
            .bounds();
        (copy.right().as_f32(), drawer.origin.x.as_f32())
    });
    assert!(
        copy_right <= drawer_left,
        "the row Copy action ends at {copy_right}px, left of the drawer at {drawer_left}px"
    );
}

#[gpui_kit::test]
fn t03_row_pitch_holds_at_the_minimum_window(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for index in 0..24 {
        let row = store
            .add(seed_input(&format!("row-{index:02}")))
            .expect("seed works");
        ids.push(row.id);
    }
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    // The 880x620 minimum: the narrowest frame the window allows.
    paint_frame_at(cx, 880., 620.);

    let measured = cx.update(|window, _| {
        let mut rows: Vec<(f32, f32)> = Vec::new();
        for id in &ids {
            let card = window.try_find(format!("row-surface-{id}"));
            if let Some(card) = card {
                let card = card.bounds();
                rows.push((card.origin.y.as_f32(), card.size.height.as_f32()));
            }
        }
        rows
    });
    assert!(
        measured.len() >= 8,
        "the minimum frame paints a screenful of rows, got {}",
        measured.len()
    );
    let pitch = manyproxy_ui::Density::Comfortable.pitch();
    for (_, height) in &measured {
        assert!(
            *height <= pitch,
            "no row is taller than the density pitch at the minimum, got {height}"
        );
    }
    for pair in measured.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        assert!(
            gap > 0. && gap <= pitch,
            "the row pitch stays on the density pitch at the minimum, got {gap}"
        );
    }
}

#[gpui_kit::test]
fn t03_dense_rows_measure_pitch_and_header_alignment(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for index in 0..24 {
        let row = store
            .add(seed_input(&format!("row-{index:02}")))
            .expect("seed works");
        ids.push(row.id);
    }
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    // The wide frame the acceptance names: 20+ rows must paint and the
    // MEASURED pitch must stay inside the 32px budget.
    cx.simulate_resize(size(px(1920.), px(1032.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    let measured: Vec<(f32, f32)> = cx.update(|window, _| {
        let mut rows: Vec<(f32, f32)> = Vec::new();
        for id in &ids {
            let card = window.try_find(format!("row-surface-{id}"));
            let name = window.try_find(format!("row-name-{id}"));
            if let (Some(card), Some(name)) = (card, name) {
                let card = card.bounds();
                let name = name.bounds();
                rows.push((card.origin.y.as_f32(), card.size.height.as_f32()));
                assert!(
                    (name.origin.y.as_f32() - card.origin.y.as_f32()).abs() < 32.,
                    "the name sits on the row's line"
                );
            }
        }
        rows
    });
    assert!(
        measured.len() >= 20,
        "20+ rows paint at 1920x1032, got {}",
        measured.len()
    );
    let density_pitch = manyproxy_ui::Density::Comfortable.pitch();
    let mut pitch: f32 = 0.;
    for pair in measured.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        assert!(
            gap > 0. && gap <= density_pitch,
            "the row pitch stays on the density pitch, got {gap}"
        );
        pitch = pitch.max(gap);
    }
    for (_, height) in &measured {
        assert!(
            *height <= density_pitch,
            "no row is taller than the density pitch, got {height}"
        );
    }
    assert!(pitch > 0., "the pitch is measurable");

    // The endpoint chip is the row's one framed control; it holds its fixed
    // cell with the whole label visible.
    let copy_label = cx.update(|window, _| {
        window
            .try_find(format!("row-endpoint-chip-{}", ids[0]))
            .and_then(|snapshot| snapshot.label().map(str::to_string))
    });
    assert_eq!(
        copy_label.as_deref(),
        Some("Click to copy SOCKS"),
        "the chip names its copy action"
    );
    let copy_width = cx.update(|window, _| {
        window
            .try_find(format!("row-endpoint-chip-{}", ids[0]))
            .map(|snapshot| snapshot.bounds().size.width.as_f32())
    });
    assert!(
        copy_width.is_some_and(|width| width <= 120.),
        "the chip holds its 120px cell, got {copy_width:?}"
    );

    // Every header label sits at its column's first character.
    let pairs: [(&str, &str); 7] = [
        ("sort-header-name", "row-name"),
        ("sort-header-group", "row-group"),
        ("sort-header-protocol", "row-protocol"),
        ("sort-header-upstream", "row-upstream"),
        ("sort-header-endpoint", "row-endpoint"),
        ("sort-header-health", "row-health"),
        ("sort-header-traffic", "row-traffic"),
    ];
    let mut checked = 0;
    for (header_id, cell_prefix) in pairs {
        let header_x = cx.update(|window, _| {
            window
                .try_find(header_id.to_string())
                .map(|snapshot| snapshot.bounds().origin.x.as_f32())
        });
        let Some(header_x) = header_x else {
            continue;
        };
        let cell_x = cx.update(|window, _| {
            ids.iter().find_map(|id| {
                window
                    .try_find(format!("{cell_prefix}-{id}"))
                    .map(|snapshot| snapshot.bounds().origin.x.as_f32())
            })
        });
        let Some(cell_x) = cell_x else {
            continue;
        };
        let drift = (header_x - cell_x).abs();
        assert!(
            drift <= 8.,
            "{header_id} sits within 8px of {cell_prefix}'s first character, got {drift}"
        );
        checked += 1;
    }
    assert!(checked >= 4, "several columns are checked, got {checked}");
}

/// F14: the name tooltip triggers on the text, not the whole flexible cell,
/// so hovering the blank remainder of a short name does nothing.
#[gpui_kit::test]
fn f14_name_tooltip_targets_the_text_not_the_cell(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Net", "a.example.com", "10801"))
        .expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    let (cell, text) = cx.update(|window, _| {
        (
            window
                .try_find(format!("row-name-{}", row.id))
                .expect("name cell paints")
                .bounds(),
            window
                .try_find(format!("row-name-text-{}", row.id))
                .expect("name text paints")
                .bounds(),
        )
    });
    assert!(
        text.size.width < cell.size.width,
        "a short name leaves blank cell space outside the tooltip trigger: text {text:?}, cell {cell:?}"
    );
}

#[gpui_kit::test]
fn t03_row_copy_proxy_copies_that_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10951"))
        .expect("seed works");
    let beta = store
        .add(t30_engine_seed("Beta", "10952"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    // The OTHER row is selected: a selection-scoped copy would carry Beta.
    select_library_row(&view, cx, &beta.id);
    // F1: the open drawer occludes the row's trailing Copy cell; this test
    // is about row-scoped copy, so put the action back in reach.
    view.update(cx, |view, _| view.inspector_visible = false);
    paint_frame(cx);
    let alpha_bind = view.read_with(cx, |view, _| {
        view.store
            .get(&alpha.id)
            .expect("store reads")
            .expect("row")
            .bind_ip
    });
    cx.update(|window, cx| {
        window.click(format!("row-endpoint-chip-{}", alpha.id), cx);
    });
    cx.run_until_parked();
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(
        clip,
        format!("socks5://{alpha_bind}:10951"),
        "the row's own proxy URL is copied"
    );
    assert!(
        !clip.contains("10952"),
        "the selection's row never leaks into the copy: {clip}"
    );
}

#[gpui_kit::test]
fn r3_row_serve_button_starts_that_row(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10951"))
        .expect("seed works");
    store
        .add(t30_engine_seed("Beta", "10952"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    paint_frame(cx);
    // K1/A4-8: per-row Start is gone; the ONE operate row acts on the
    // selection, so the row is selected first.
    select_library_row(&view, cx, &alpha.id);
    paint_frame_at(cx, 1000., 1600.);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-start", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "per-row Start serves exactly that row"
    );
    assert_eq!(notification_count(cx), before + 1, "one honest toast");
}

#[gpui_kit::test]
fn start_all_partial_names_failed_row_and_serves_rest(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    for (name, port) in [
        ("A", "10921"),
        ("B", "10922"),
        ("C", "10923"),
        ("D", "10924"),
    ] {
        store.add(t30_engine_seed(name, port)).expect("seed works");
    }

    store
        .add(NewConfig {
            name: "Broken".to_string(),
            protocol: "vless".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: String::new(),
            port: "10925".to_string(),
        })
        .expect("seed works");
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
        4,
        "valid rows keep serving"
    );
    assert_eq!(notification_count(cx), 1, "one partial toast");

    let copy = manyproxy_ui::start_all_partial_message(4, &["\"Broken\"".to_string()]);
    assert!(copy.contains('4') && copy.contains('1') && copy.contains("Broken"));
}

#[gpui_kit::test]
fn t41_save_gate_rejects_reserved_port_and_garbage_bind(cx: &mut TestAppContext) {
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

    let mut squatter = t30_engine_seed("Squatter", "18501");
    let added = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_add(squatter.clone(), window, cx))
    });
    assert!(!added, "reserved port never saves");

    squatter.port = "11801".to_string();
    let added = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_add(squatter.clone(), window, cx))
    });
    assert!(!added, "bridge range never saves");

    squatter.port = "10991".to_string();
    squatter.bind_ip = "999.999.0.1".to_string();
    let added = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_add(squatter.clone(), window, cx))
    });
    assert!(!added, "garbage bind never saves");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing written"
    );
    assert_eq!(notification_count(cx), 3, "one error toast per refusal");

    squatter.bind_ip = String::new();
    squatter.port = String::new();
    let added =
        cx.update(|window, cx| view.update(cx, |view, cx| view.commit_add(squatter, window, cx)));
    assert!(added, "auto port saves");
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);
}

#[gpui_kit::test]
fn t41_commit_edit_into_reserved_range_rejected(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::store::ConfigPatch;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let created = store
        .add(t30_engine_seed("Work", "10931"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    let patch = ConfigPatch {
        port: Some("18501".to_string()),
        ..Default::default()
    };
    let updated = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit(&created.id, patch, window, cx)
        })
    });
    assert!(!updated, "reserved edit never writes");
    let row = view.read_with(cx, |view, _| {
        view.store
            .get(&created.id)
            .expect("read works")
            .expect("row kept")
    });
    assert_eq!(row.port, 10931, "stored port untouched, got {}", row.port);
}

#[gpui_kit::test]
fn t41_restart_crashed_recovers_and_idle_quiet(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10941"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let handle = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, handle) = EngineController::fake();
            view.engine = engine;
            view.engine.set_auto_restart(AutoRestartConfig {
                enabled: false,
                ..AutoRestartConfig::default()
            });
            handle
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_all(window, cx));
    });
    let id = view.read_with(cx, |view, _| view.engine.running_ids()[0].clone());
    assert!(
        view.read_with(cx, |view, _| view.engine.is_running(&id)),
        "serving before the crash"
    );
    handle.inject_crash("signal: segmentation fault");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.tick_engine_poll(cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.crashed_shards().len()),
        1,
        "crash registered"
    );
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.restart_crashed_engines(window, cx));
    });
    assert_eq!(notification_count(cx), before + 1, "one recovery toast");
    assert!(
        view.read_with(cx, |view, _| view.engine.crashed_shards().is_empty()),
        "crashed set cleared"
    );
    assert!(
        view.read_with(cx, |view, _| view.engine.clash_targets().len() == 1),
        "clash recovered"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.restart_crashed_engines(window, cx));
    });
    assert_eq!(notification_count(cx), before + 2, "one idle toast");
}

#[gpui_kit::test]
fn double_start_while_busy_is_ignored(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10931"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let handle = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, handle) = EngineController::fake();
            view.engine = engine;
            view.start_in_flight = true;
            handle
        })
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let checks_before = handle.check_calls();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_engine(window, cx));
    });
    assert_eq!(notification_count(cx), 1, "busy explains itself once");
    assert_eq!(handle.check_calls(), checks_before, "no second apply");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts while busy"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.start_in_flight = false;
        });
    });
}

#[gpui_kit::test]
fn start_on_running_restarts_honestly(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10932"))
        .expect("seed works");
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
    let id = view.read_with(cx, |view, cx| {
        view.selected_config(cx).expect("selected").id
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 1);
    assert_eq!(notification_count(cx), 1);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "restart serves exactly once"
    );
    assert_eq!(notification_count(cx), 2, "restart toasts honestly");
}

#[gpui_kit::test]
fn start_with_no_selection_explains_itself(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);

    assert!(view.read_with(cx, |view, cx| view.selected_config(cx).is_none()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_selected_engine(window, cx));
    });
    assert_eq!(notification_count(cx), 1, "no-selection explains itself");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts"
    );
}

#[gpui_kit::test]
fn crash_tick_notifies_with_shard_and_logs_path(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10933"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let stock = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, stock) = EngineController::fake();
            view.engine = engine;
            stock
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 1);
    let before = notification_count(cx);
    stock.inject_crash("signal: killed");
    let detail = cx
        .update(|window, cx| view.update(cx, |view, cx| view.tick_engine_poll_notify(window, cx)));
    let detail = detail.expect("crash reported");
    assert!(detail.contains("stock"), "names the shard: {detail}");
    assert!(detail.contains("killed"), "names the cause: {detail}");
    assert_eq!(
        notification_count(cx),
        before + 1,
        "crash notifies (not status-bar-only)"
    );
}

#[gpui_kit::test]
fn production_tick_auto_restarts_and_clears_crash(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Work", "10943"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let handle = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, handle) = EngineController::fake();
            view.engine = engine;
            handle
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_all(window, cx));
    });
    assert!(
        view.read_with(cx, |view, _| view.engine.running_count()) >= 1,
        "serving before the crash"
    );

    handle.inject_crash("signal: segmentation fault");
    // The production tick both reports the cause and recovers from the same
    // detection (BUG-01 + BUG-02) — no second drain, no sticky crashed set.
    let cause = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
    assert!(cause.is_some(), "tick carries the crash cause");
    assert!(
        view.read_with(cx, |view, _| view.engine.crashed_shards().is_empty()),
        "auto-restart cleared the crashed shard"
    );
    assert!(
        view.read_with(cx, |view, _| view.engine.running_count()) >= 1,
        "the core runs again after auto-restart"
    );
    assert!(
        view.read_with(cx, |view, _| view.engine.pending_crash().is_none()),
        "the pending cause is spent once handled"
    );
}

#[test]
fn search_no_hits_vs_empty_counts() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(id: &str, name: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{}".to_string().into(),
            group: "team".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![row("1", "Alpha"), row("2", "Beta")]);
    let mut query = manyproxy_ui::FilterQuery::default();
    query.set_search("zzz-no-hit");
    assert_eq!(test_visible(&delegate, &query).len(), 0);
    assert_eq!(manyproxy_ui::search_count_text(0, 2), "0 of 2");
    assert_eq!(manyproxy_ui::no_matches_text(), "No configs match");
    assert_eq!(manyproxy_ui::empty_library_title(), "No configs yet");
    let empty = LibraryDelegate::new();
    assert_eq!(
        manyproxy_ui::scope_total(&empty.rows, &manyproxy_ui::LibraryScope::All),
        0
    );
    assert!(manyproxy_ui::empty_library_text().contains("Add or import a config"));
}

#[gpui_kit::test]
fn no_hits_paints_and_detail_has_single_switch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("zzz-no-hit".to_string(), cx);
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now(String::new(), cx);
            view.select_visible_row(0, cx);
        });
    });
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, _| window
            .try_find(format!("library-row-{}", seeded.id))
            .is_some()),
        "row paints once the filter clears"
    );
    assert_painted(cx, "detail-serving");
    assert!(
        cx.update(|window, _| window.try_find("toggle-enabled".to_string()).is_none()),
        "no duplicate enable button — Switch only"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("zzz-no-hit".to_string(), cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    assert_painted(cx, "library-clear-filters");
    assert!(
        cx.update(|window, _| window.try_find("action-bar-count".to_string()).is_some()),
        "the action bar keeps the count"
    );
}

#[gpui_kit::test]
fn laneb_facet_empty_and_clear_all(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("One", "vless", "home", "10843"))
        .expect("seed works");
    store
        .add(facet_seed("Two", "vless", "home", "10844"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_bind_facet("10.9.9.9".to_string(), true, cx)
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    assert_painted(cx, "library-clear-filters");
    assert!(
        cx.update(|window, _| window.try_find("empty-library-ctas".to_string()).is_none()),
        "filtered-to-zero never offers Add/Import"
    );
    cx.update(|window, cx| {
        window.click("library-clear-filters", cx);
    });
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.visible.len()), 2);
}

#[gpui_kit::test]
fn ticket03_search_count_persistent_with_live_count(cx: &mut TestAppContext) {
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
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(manyproxy_ui::search_placeholder(), "Search configs");

    let (visible, total) = view.read_with(cx, |view, cx| {
        let delegate = view.libraries.read(cx).delegate();
        (
            view.visible.len(),
            manyproxy_ui::scope_total(&delegate.rows, &manyproxy_ui::LibraryScope::All),
        )
    });
    assert_eq!((visible, total), (2, 2), "unfiltered library counts");
    assert_eq!(manyproxy_ui::search_count_text(visible, total), "2 of 2");

    assert_painted(cx, "filter-slot");
    assert_painted(cx, "library-search");
    assert_painted(cx, "filter-count");
    assert_painted(cx, "action-bar-count");
    assert_absent(cx, "facet-bar".to_string());
    assert_absent(cx, "facet-group".to_string());
    assert_absent(cx, "facet-protocol".to_string());
    assert_absent(cx, "facet-status".to_string());
}

#[gpui_kit::test]
fn ticket03_clears_affect_only_their_target(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(facet_seed("Work", "vless", "travel", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Office", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("Work".to_string(), cx);
            view.set_bind_facet("127.0.0.1".to_string(), true, cx);
            view.select_row(&work.id, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    let pre = view.read_with(cx, |view, _| {
        (
            view.query.search.clone(),
            view.query.binds.clone(),
            view.is_selected(&work.id),
        )
    });
    assert_eq!(pre.0, "Work", "query established");
    assert!(pre.1.contains("127.0.0.1"), "bind facet established");
    assert!(pre.2, "row selected");
    assert_painted(cx, "batch-more");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.clear_search(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    let after_query = view.read_with(cx, |view, _| {
        (
            view.query.search.clone(),
            view.query.binds.clone(),
            view.is_selected(&work.id),
        )
    });
    assert_eq!(
        after_query.0,
        String::new(),
        "Clear search empties the query"
    );
    assert!(
        after_query.1.contains("127.0.0.1"),
        "Clear search keeps the bind facet"
    );
    assert!(after_query.2, "Clear search keeps the selection");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("Work".to_string(), cx);
            view.clear_filters(cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    let after_filters = view.read_with(cx, |view, _| {
        (
            view.query.search.clone(),
            view.query.binds.clone(),
            view.is_selected(&work.id),
        )
    });
    assert_eq!(after_filters.0, "Work", "Clear filters keeps the search");
    assert!(
        after_filters.1.is_empty(),
        "Clear filters drops the bind facet"
    );
    assert!(after_filters.2, "Clear filters keeps the selection");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_bind_facet("127.0.0.1".to_string(), true, cx)
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("batch-clear", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    let after_checked = view.read_with(cx, |view, _| {
        (
            view.query.search.clone(),
            view.query.binds.clone(),
            view.selection_count() == 0,
        )
    });
    assert_eq!(after_checked.0, "Work", "Clear checked keeps the query");
    assert!(
        after_checked.1.contains("127.0.0.1"),
        "Clear checked keeps the bind facet"
    );
    assert!(after_checked.2, "Clear checked empties the set");
}

#[gpui_kit::test]
fn t06_search_field_focusable_with_live_count(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
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
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(manyproxy_ui::search_placeholder(), "Search configs");
    assert_painted(cx, "filter-slot");
    assert_painted(cx, "library-search");

    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let keystroke = Keystroke::parse("ctrl-f").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    let query_focused = cx.update(|window, cx| {
        let handle = view.read(cx).search_input.read(cx).focus_handle(cx);
        window.focused(cx).as_ref() == Some(&handle)
    });
    assert!(query_focused, "Ctrl-F lands in the library search field");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now("Work".to_string(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    let (visible, total) = view.read_with(cx, |view, cx| {
        let delegate = view.libraries.read(cx).delegate();
        (
            view.visible.len(),
            manyproxy_ui::scope_total(&delegate.rows, &manyproxy_ui::LibraryScope::All),
        )
    });
    assert_eq!((visible, total), (1, 2), "query filters the library");
    assert_eq!(
        view.read_with(cx, |view, _| view.query.search.clone()),
        "Work",
        "query established"
    );
    assert_eq!(manyproxy_ui::search_count_text(visible, total), "1 of 2");
    assert_painted(cx, "library-search");
    assert_painted(cx, "filter-count");
}
