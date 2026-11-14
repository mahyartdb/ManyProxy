//! table: one binary of the split UI suite. Each binary runs as its own
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

/// The keyboard owns one focus and one set: Down/Up move the focus and the
/// selection together, Shift grows from the anchor, Home/End jump, Space
/// toggles the focused row, Ctrl+A takes the visible rows and Escape clears.
#[gpui_kit::test]
fn keyboard_selection_moves_focus_and_range(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let charlie = store
        .add(facet_seed("Charlie", "vless", "team", "10803"))
        .expect("seed works");
    let delta = store
        .add(facet_seed("Delta", "vless", "team", "10804"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    focus_library(&view, cx);

    press_key(cx, "down");
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_focus.clone()),
        Some(alpha.id.clone()),
        "Down enters at the first row"
    );
    assert_eq!(selected_ids(&view, cx), vec![alpha.id.clone()]);

    press_key(cx, "down");
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_focus.clone()),
        Some(bravo.id.clone()),
        "Down moves the focus"
    );
    assert_eq!(selected_ids(&view, cx), vec![bravo.id.clone()]);

    press_key(cx, "shift-down");
    assert_eq!(
        selected_ids(&view, cx),
        vec![bravo.id.clone(), charlie.id.clone()],
        "Shift+Down grows from the anchor"
    );
    press_key(cx, "shift-up");
    assert_eq!(
        selected_ids(&view, cx),
        vec![bravo.id.clone()],
        "Shift+Up shrinks the same range without re-anchoring"
    );

    press_key(cx, "end");
    assert_eq!(selected_ids(&view, cx), vec![delta.id.clone()]);
    press_key(cx, "home");
    assert_eq!(selected_ids(&view, cx), vec![alpha.id.clone()]);

    press_key(cx, "space");
    assert_eq!(selected_ids(&view, cx), Vec::<String>::new());
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        None,
        "emptying the set drops the anchor"
    );

    press_key(cx, "ctrl-a");
    assert_eq!(
        selected_ids(&view, cx),
        vec![
            alpha.id.clone(),
            bravo.id.clone(),
            charlie.id.clone(),
            delta.id.clone()
        ],
        "Ctrl+A takes every visible row"
    );

    // Escape clears with the list focused too: the app's binding names the
    // list as a descendant of the app, so it outranks the kit's Cancel. It
    // then hands the keyboard back to the shell.
    press_key(cx, "escape");
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 0);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        None
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_focus.clone()),
        None
    );
    let list_focus = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    assert_ne!(
        cx.update(|window, cx| window.focused(cx)),
        Some(list_focus),
        "Escape hands the keyboard back to the shell, not the list"
    );

    // The shell's own context keeps the shortcut when the table does not hold
    // the keyboard.
    press_key(cx, "ctrl-a");
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 4);
    press_key(cx, "escape");
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 0);
}

/// The kit List can be handed an index; it must not become a second
/// selection. The delegate keeps no index and writes nothing to AppView.
#[gpui_kit::test]
fn delegate_never_writes_selection(cx: &mut TestAppContext) {
    use gpui_kit::component::list::ListDelegate as _;
    use gpui_kit::component::IndexPath;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &alpha.id);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                state
                    .delegate_mut()
                    .set_selected_index(Some(IndexPath::new(1)), window, list_cx);
                list_cx.notify();
            });
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone()],
        "the delegate cannot move the selection"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.libraries.read(cx).selected_index()),
        None,
        "the list never keeps an index of its own"
    );
    assert!(!row_aria_selected(cx, &bravo.id));
}

/// A scope change is not a filter: the rows the old scope held are gone, so
/// the set, the anchor and the focus all go with it.
#[gpui_kit::test]
fn scope_change_clears_selection_anchor_and_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let charlie = store
        .add(facet_seed("Charlie", "vless", "team", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[alpha.id.clone(), bravo.id.clone()], cx);
            view.extend_to_row(&charlie.id, cx);
        });
    });
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone(), charlie.id.clone()]
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        Some(alpha.id.clone())
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_focus.clone()),
        Some(charlie.id.clone())
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_selection_for_scope_change(window, cx);
        });
    });
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 0);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        None
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_focus.clone()),
        None
    );
}

/// R29: `Show failures` narrows the table to the ids the batch failed on and
/// leaves the selection alone; the ordinary filter controls clear it.
#[gpui_kit::test]
fn show_failures_filters_to_the_last_batch_failed_ids(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let charlie = store
        .add(facet_seed("Charlie", "vless", "team", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[alpha.id.clone(), bravo.id.clone()], cx);
            view.batch = Some(manyproxy_ui::BatchRun {
                kind: manyproxy_ui::BatchKind::Test,
                ids: vec![alpha.id.clone(), bravo.id.clone()],
                done: 2,
                ok: 1,
                failed: vec![bravo.id.clone()],
            });
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "batch-show-failures");

    cx.update(|window, cx| window.click("batch-show-failures", cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        view.read_with(cx, |view, cx| view.visible_row_ids(cx)),
        vec![bravo.id.clone()],
        "the table holds exactly the batch's failures"
    );
    assert_painted(cx, &format!("row-name-text-{}", bravo.id));
    assert_absent(cx, format!("row-name-text-{}", alpha.id));
    assert_absent(cx, format!("row-name-text-{}", charlie.id));
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        2,
        "the filter does not touch the selection"
    );

    // The ordinary filter controls take the failure filter away again.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now(String::new(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.visible_row_ids(cx))
            .len(),
        3,
        "clearing the facets restores every row"
    );
}

/// The chip copies the row's SOCKS URL; its chevron offers HTTP and both.
/// Neither click selects the row.
#[gpui_kit::test]
fn endpoint_chip_copies_socks_and_chevron_copies_http_and_both(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let chip = format!("row-endpoint-chip-{}", row.id);
    let menu = format!("row-endpoint-menu-{}", row.id);
    assert_painted(cx, &chip);
    assert_eq!(
        label_of(&chip, cx),
        "Click to copy SOCKS",
        "the chip names its copy action"
    );

    cx.update(|window, cx| window.click(chip.clone(), cx));
    cx.run_until_parked();
    assert_eq!(clipboard_text(cx), "socks5://127.0.0.1:10801");
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        0,
        "the chip owns its click; the row does not select"
    );

    cx.update(|window, cx| window.click(menu.clone(), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let http = format!("row-endpoint-copy-http-{}", row.id);
    assert_painted(cx, &http);
    cx.update(|window, cx| window.click(http, cx));
    cx.run_until_parked();
    assert_eq!(clipboard_text(cx), "http://127.0.0.1:10801");

    cx.update(|window, cx| window.click(menu, cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let both = format!("row-endpoint-copy-both-{}", row.id);
    assert_painted(cx, &both);
    cx.update(|window, cx| window.click(both, cx));
    cx.run_until_parked();
    assert_eq!(
        clipboard_text(cx),
        "socks5://127.0.0.1:10801\nhttp://127.0.0.1:10801",
        "both forms travel, SOCKS first"
    );
}

/// The batch bar takes the search's 36px slot: same origin, same height, and
/// the table below never moves.
#[gpui_kit::test]
fn batch_bar_swaps_into_the_same_slot_without_moving_the_table(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let slot = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            let bounds = window
                .try_find("filter-slot")
                .expect("slot paints")
                .bounds();
            (
                bounds.origin.x.as_f32(),
                bounds.origin.y.as_f32(),
                bounds.size.height.as_f32(),
            )
        })
    };
    let header_top = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find("library-column-headers")
                .expect("header paints")
                .bounds()
                .origin
                .y
                .as_f32()
        })
    };
    let first_row_top = |cx: &mut VisualTestContext, id: &str| {
        cx.update(|window, _| {
            window
                .try_find(format!("row-surface-{id}"))
                .expect("row paints")
                .bounds()
                .origin
                .y
                .as_f32()
        })
    };

    assert_painted(cx, "library-search");
    let search_slot = slot(cx);
    let search_header = header_top(cx);
    let search_row = first_row_top(cx, &alpha.id);
    assert_eq!(search_slot.2, 36., "the filter slot is 36px");

    select_library_row(&view, cx, &alpha.id);
    paint_frame(cx);
    assert_painted(cx, "batch-bar");
    assert_absent(cx, "library-search".to_string());

    assert_eq!(
        slot(cx),
        search_slot,
        "the slot keeps its origin and height"
    );
    assert_eq!(header_top(cx), search_header, "the header never moves");
    assert_eq!(
        first_row_top(cx, &alpha.id),
        search_row,
        "the first row never moves"
    );
    let bar_bounds = cx.update(|window, _| {
        window
            .try_find("batch-bar")
            .expect("batch bar paints")
            .bounds()
    });
    assert!(
        bar_bounds.size.height.as_f32() <= 36.,
        "the batch bar fits inside the 36px slot, got {}",
        bar_bounds.size.height.as_f32()
    );
}

/// A batch captures its ids when it is invoked. Changing the selection while
/// it runs never edits the snapshot, and the next batch captures the new set.
#[gpui_kit::test]
fn batch_actions_snapshot_selection(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10991"))
        .expect("seed works");
    let bravo = store
        .add(t30_engine_seed("Bravo", "10992"))
        .expect("seed works");
    let gamma = store
        .add(t30_engine_seed("Gamma", "10993"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let (engine, _stock) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.engine = engine);
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[alpha.id.clone(), gamma.id.clone()], cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|window, cx| window.click("batch-start", cx));
    cx.run_until_parked();
    paint_frame(cx);
    let serving = view.read_with(cx, |view, _| view.engine.running_ids());
    assert!(serving.contains(&alpha.id) && serving.contains(&gamma.id));
    assert!(
        !serving.contains(&bravo.id),
        "the unselected row never enters the snapshot"
    );

    // The finished batch's result belongs to its snapshot: a selection change
    // drops it, and a new batch captures the new set.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(std::slice::from_ref(&bravo.id), cx)
        });
    });
    assert!(
        view.read_with(cx, |view, _| view.batch.is_none()),
        "a finished batch is not re-attributed to a new selection"
    );
    cx.update(|window, cx| window.click("batch-start", cx));
    cx.run_until_parked();
    paint_frame(cx);
    let serving = view.read_with(cx, |view, _| view.engine.running_ids());
    assert!(serving.contains(&bravo.id), "the new snapshot starts Beta");
    assert!(
        serving.contains(&alpha.id) && serving.contains(&gamma.id),
        "starting Beta leaves the earlier rows serving"
    );
}

/// R22: density owns every chrome height and the row pitch, and the choice
/// survives a store reopen.
#[gpui_kit::test]
fn density_round_trips_and_changes_row_pitch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "manyproxy-density-{}-{stamp}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let mut store = Store::open_file(&path).expect("file store opens");
        store
            .add(facet_seed("Alpha", "vless", "team", "10801"))
            .expect("seed works");
        store
            .add(facet_seed("Bravo", "vless", "team", "10802"))
            .expect("seed works");
        store
            .set_row_density(manyproxy_ui::Density::Compact)
            .expect("density saves");
    }
    let store = Store::open_file(&path).expect("reopen works");
    assert_eq!(store.row_density(), manyproxy_ui::Density::Compact);
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.density),
        manyproxy_ui::Density::Compact
    );

    let row = |cx: &mut VisualTestContext, id: &str| {
        cx.update(|window, _| {
            window
                .try_find(format!("row-surface-{id}"))
                .expect("row paints")
                .bounds()
        })
    };
    let slot_height = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find("filter-slot")
                .expect("slot paints")
                .bounds()
                .size
                .height
                .as_f32()
        })
    };
    let header_height = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .try_find("library-column-headers")
                .expect("header paints")
                .bounds()
                .size
                .height
                .as_f32()
        })
    };

    let ids: Vec<String> = view.read_with(cx, |view, cx| view.visible_row_ids(cx));
    assert!(ids.len() >= 2, "two seeded rows are visible");
    let first = row(cx, &ids[0]);
    let second = row(cx, &ids[1]);
    assert_eq!(first.size.height.as_f32(), 30., "compact row height");
    assert_eq!(
        second.origin.y.as_f32() - first.origin.y.as_f32(),
        30.,
        "compact row pitch"
    );
    assert_eq!(slot_height(cx), 34., "compact filter slot");
    assert_eq!(header_height(cx), 30., "compact header");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_density(manyproxy_ui::Density::Comfortable, window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.density),
        manyproxy_ui::Density::Comfortable
    );
    let first = row(cx, &ids[0]);
    let second = row(cx, &ids[1]);
    assert_eq!(first.size.height.as_f32(), 36., "comfortable row height");
    assert_eq!(
        second.origin.y.as_f32() - first.origin.y.as_f32(),
        36.,
        "comfortable row pitch"
    );
    assert_eq!(slot_height(cx), 36., "comfortable filter slot");
    assert_eq!(header_height(cx), 32., "comfortable header");

    let reopened = Store::open_file(&path).expect("the density is readable");
    assert_eq!(
        reopened.row_density(),
        manyproxy_ui::Density::Comfortable,
        "the choice is written on change"
    );
}

/// The retired checkbox left no copy behind: no label says "checked" and the
/// shortcut reference names selection, not checking.
#[gpui_kit::test]
fn no_checked_copy_or_control_remains(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &row.id);
    paint_frame(cx);

    let labels: Vec<String> = cx.update(|window, _| {
        gpui_kit::base::test_support::snapshots(window)
            .iter()
            .filter_map(|snapshot| snapshot.label().map(str::to_string))
            .collect()
    });
    for label in &labels {
        let lowered = label.to_lowercase();
        assert!(
            !lowered.contains("uncheck"),
            "no copy says Uncheck: {label:?}"
        );
        assert!(
            !lowered.contains("checked"),
            "no copy says checked: {label:?}"
        );
    }
    assert_eq!(label_of("batch-count", cx), "1 selected");
    assert!(
        manyproxy_ui::SHORTCUT_LINES.contains(&("Clear selection", "Esc")),
        "the reference names Esc as Clear selection"
    );
    assert!(
        !manyproxy_ui::SHORTCUT_LINES
            .iter()
            .any(|(action, _)| action.to_lowercase().contains("uncheck")),
        "no shortcut is named Uncheck"
    );
}

/// The row's trailing reserve holds Start or Stop plus More. Start is the
/// row's own verb, never the selection's, and More opens that row's menu.
#[gpui_kit::test]
fn row_hover_actions_start_stop_and_more(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10991"))
        .expect("seed works");
    let bravo = store
        .add(t30_engine_seed("Bravo", "10992"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let (engine, _stock) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.engine = engine);
    });
    cx.run_until_parked();
    paint_frame(cx);
    // The focused row shows its reserve: Start/Stop plus More.
    select_library_row(&view, cx, &alpha.id);
    paint_frame(cx);
    let start = format!("row-start-{}", alpha.id);
    let more = format!("row-more-{}", alpha.id);
    assert_painted(cx, &format!("row-actions-{}", alpha.id));
    assert_painted(cx, &start);
    assert_painted(cx, &more);

    cx.update(|window, cx| window.click(start, cx));
    cx.run_until_parked();
    paint_frame(cx);
    let serving = view.read_with(cx, |view, _| view.engine.running_ids());
    assert!(
        serving.contains(&alpha.id),
        "the row's Start starts the row it belongs to"
    );
    assert!(
        !serving.contains(&bravo.id),
        "another row never starts by accident"
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone()],
        "the click does not move the selection"
    );
    assert_painted(cx, &format!("row-stop-{}", alpha.id));

    cx.update(|window, cx| window.click(format!("row-stop-{}", alpha.id), cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)),
        "Stop stops the same row"
    );
    assert_painted(cx, &format!("row-start-{}", alpha.id));

    cx.update(|window, cx| window.click(format!("row-more-{}", alpha.id), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, &format!("rowctx-test-{}", alpha.id));
    assert_absent(cx, format!("rowctx-test-{}", bravo.id));
}

/// The header and the blank area under the rows are outside the selection:
/// a plain click there clears it and hands the keyboard back to the shell.
#[gpui_kit::test]
fn header_and_blank_table_clicks_clear_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    select_library_row(&view, cx, &alpha.id);
    paint_frame(cx);
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 1);

    // The Name header cell sorts like every other header; the strip owns the
    // click that clears the selection.
    cx.update(|window, cx| window.click("sort-header-name", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        0,
        "a header click clears the selection"
    );

    select_library_row(&view, cx, &alpha.id);
    paint_frame(cx);
    assert_eq!(view.read_with(cx, |view, _| view.selection_count()), 1);

    click_blank_table(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        0,
        "a blank-table click clears the selection"
    );
}
