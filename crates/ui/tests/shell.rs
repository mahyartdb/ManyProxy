//! shell: one binary of the split UI suite. Each binary runs as its own
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

/// The shell's four regions are painted, and the three retired surfaces are
/// gone for good.
#[gpui_kit::test]
fn shell_renders_nav_action_bar_filter_slot_and_engine_bar(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    for id in ["nav", "action-bar", "filter-slot", "engine-bar"] {
        assert_painted(cx, id);
    }
    for id in ["manyproxy-toolbar", "facet-bar", "status-count"] {
        assert_absent(cx, id.to_string());
    }
}

/// The engine bar sits below the content row and outside the resizable split:
/// it spans the table and the inspector, and never the nav.
#[gpui_kit::test]
fn engine_bar_spans_the_split_but_not_the_nav(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    open_inspector(&view, cx);
    paint_frame_at(cx, 1440., 900.);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    let (nav, bar) = cx.update(|window, _| {
        let bounds = |id: &str| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
        };
        (bounds("nav"), bounds("engine-bar"))
    });
    assert_eq!(
        bar.origin.x,
        nav.origin.x + nav.size.width,
        "the engine bar starts where the nav ends: {nav:?} vs {bar:?}"
    );
    assert_eq!(
        bar.origin.x + bar.size.width,
        px(1440.),
        "the engine bar ends at the window's right edge: {bar:?}"
    );
}

/// The nav collapses below the breakpoint and expands at it, and the labels
/// live only in the expanded nav.
#[gpui_kit::test]
fn nav_collapses_at_1199_and_expands_at_1200(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    paint_frame_at(cx, 1199., 700.);
    cx.run_until_parked();
    paint_frame_at(cx, 1199., 700.);
    let narrow = cx.update(|window, _| {
        window
            .try_find("nav".to_string())
            .expect("the nav renders")
            .bounds()
            .size
            .width
    });
    assert_eq!(narrow, px(56.), "the nav collapses below the breakpoint");
    assert_absent(cx, "nav-all-configs-label".to_string());
    assert_painted(cx, "nav-all-configs-letter");

    paint_frame_at(cx, 1200., 700.);
    cx.run_until_parked();
    paint_frame_at(cx, 1200., 700.);
    let wide = cx.update(|window, _| {
        window
            .try_find("nav".to_string())
            .expect("the nav renders")
            .bounds()
            .size
            .width
    });
    assert_eq!(wide, px(220.), "the nav expands at the breakpoint");
    assert_painted(cx, "nav-all-configs-label");
    assert_absent(cx, "nav-all-configs-letter".to_string());
}

/// The inspector still starts hidden and still toggles with Ctrl-I through
/// the new shell.
#[gpui_kit::test]
fn inspector_still_starts_hidden_and_toggles_with_ctrl_i(cx: &mut TestAppContext) {
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
    assert!(
        !view.read_with(cx, |view, _| view.inspector_visible),
        "the inspector starts hidden"
    );
    assert_absent(cx, "config-detail".to_string());

    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let toggle = || Keystroke::parse("ctrl-i").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.inspector_visible),
        "Ctrl-I opens the inspector"
    );
    assert_painted(cx, "config-detail");

    cx.update(|window, cx| window.dispatch_keystroke(toggle(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !view.read_with(cx, |view, _| view.inspector_visible),
        "Ctrl-I closes the inspector again"
    );
}

/// The window's design minimum, and the clamp a persisted rectangle under it
/// gets on launch.
#[test]
fn window_minimum_is_1100_by_640() {
    assert_eq!(
        manyproxy_ui::window_min_size(None),
        size(px(1100.), px(640.)),
        "the design minimum is the new shell's floor"
    );
    let clamped = WindowGeometry {
        x: 10.,
        y: 10.,
        width: 800.,
        height: 500.,
    }
    .clamped()
    .expect("a sane rectangle survives");
    assert_eq!(
        (clamped.width, clamped.height),
        (1100., 640.),
        "a rectangle under the floor is clamped up to it"
    );
}

/// Every retired menu verb has one home left. The palette is navigation, not
/// a second home, so it does not count here.
#[gpui_kit::test]
fn every_retired_menu_action_has_one_reachable_surface(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);

    // The homes this slice builds.
    for id in [
        "nav-routing-presets",
        "nav-settings",
        "action-bar-import",
        "action-bar-add",
        "action-bar-palette",
        "engine-bar-start-all",
        "engine-bar-stop-all",
    ] {
        assert_painted(cx, id);
    }

    // The action bar's overflow carries the verbs that had no other home.
    cx.update(|window, cx| {
        window.click("action-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in [
        "action-overflow-export",
        "action-overflow-updates",
        "action-overflow-about",
    ] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| {
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);

    // The engine bar's overflow carries the pool-wide test verbs.
    cx.update(|window, cx| {
        window.click("engine-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    for id in ["engine-bar-test-all", "engine-bar-stop-testing"] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| {
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);

    // The verbs whose home a later slice builds stay in the palette until
    // then: the palette is their interim home, not a second one.
    let palette: Vec<&str> = manyproxy_ui::PALETTE_GROUPS
        .iter()
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect();
    for id in ["subscriptions", "logs", "row-columns"] {
        assert!(
            palette.contains(&id),
            "{id} stays in the palette until its own home lands"
        );
    }
    // Slice 7: the all-groups sheet is gone. A group is managed from the nav's
    // group context menu, so the palette verb retires rather than duplicate it.
    assert!(
        !palette.contains(&"groups"),
        "groups moved into the nav's group menu"
    );
    // Slice 6 retires the two stats dialogs: their content lives in the
    // console, so the palette verbs are gone rather than duplicated.
    for id in ["runtime-stats", "traffic-stats"] {
        assert!(
            !palette.contains(&id),
            "{id} moved into the console and left the palette"
        );
    }
    for id in ["inspector-toggle", "toggle-console"] {
        assert!(
            palette.contains(&id),
            "{id} reaches the inspector and console from the palette"
        );
    }
}

/// The old table still filters through the new slot, and the action bar's
/// count follows it.
#[gpui_kit::test]
fn old_table_still_filters_through_the_slot_search(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_painted(cx, "filter-slot");
    assert_painted(cx, "library-search");
    assert_eq!(
        label_of("action-bar-count", cx),
        "2 configurations · 0 set for launch",
        "the action bar counts the whole library"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now("Work".to_string(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    let visible = view.read_with(cx, |view, _| view.visible.len());
    assert_eq!(visible, 1, "the slot's query still filters the rows");
    assert_eq!(
        label_of("action-bar-count", cx),
        "2 configurations · 0 set for launch",
        "the count names the library, not the filtered rows"
    );
}

/// The engine bar's actions dispatch the same handlers as the retired Tools
/// menu: the same confirmations, the same batch.
#[gpui_kit::test]
fn engine_bar_actions_use_the_existing_confirmations(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let solo = store
        .add(t30_engine_seed("Solo", "10964"))
        .expect("seed works");
    let mut lan = t30_engine_seed("Lan", "10963");
    lan.bind_ip = "0.0.0.0".to_string();
    store.add(lan).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
        });
    });
    paint_frame(cx);

    // Start all keeps the per-row LAN confirmation.
    cx.update(|window, cx| {
        window.click("engine-bar-start-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Start all still asks before a LAN bind"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing serves before the confirmation"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    // Stop all keeps the running-row confirmation.
    view.update(cx, |view, _| {
        let rows: Vec<_> = view
            .store
            .list()
            .expect("list works")
            .into_iter()
            .filter(|row| row.id == solo.id)
            .collect();
        view.engine
            .start_all(&rows)
            .expect("the fake engine starts the loopback row");
    });
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("engine-bar-stop-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Stop all still asks before it kills"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        1,
        "the endpoint serves until the confirmation"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    // Test all and Stop testing ride the engine bar's overflow.
    cx.update(|window, cx| {
        window.click("engine-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "engine-bar-test-all");
    assert_painted(cx, "engine-bar-stop-testing");
}
