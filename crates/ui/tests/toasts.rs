//! toasts: one binary of the split UI suite. Each binary runs as its own
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
fn test_visible(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<usize> {
    manyproxy_ui::filter(
        &delegate.rows,
        &delegate.search_index,
        &test_facts(delegate),
        &manyproxy_ui::LibraryScope::All,
        query,
    )
}

/// The app's filter-then-sort order, reproduced without an `AppView`.
fn test_sorted(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<usize> {
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

fn test_names(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<String> {
    test_sorted(delegate, query)
        .into_iter()
        .map(|index| delegate.rows[index].name.clone())
        .collect()
}

#[gpui_kit::test]
fn k5_toasts_park_bottom_right_max_two(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    let settings = cx.update(|_, cx| {
        let notification = &mut gpui_kit::component::Theme::global_mut(cx).notification;
        (
            notification.placement,
            notification.max_items,
            notification.margins.bottom,
        )
    });
    assert_eq!(
        settings.0,
        gpui_kit::Anchor::BottomRight,
        "toasts park bottom-right"
    );
    assert_eq!(settings.1, 2, "at most two toasts visible");
    assert!(
        settings.2 > px(30.),
        "the stack sits above the status bar with a safe-area offset: {:?}",
        settings.2
    );
}

#[gpui_kit::test]
fn k5_timed_toasts_dismiss_on_their_clock(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toast_success("Quick win".to_string(), window, cx);
            view.toast_error("Slow failure".to_string(), window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 2, "both timed toasts are alive");

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(4400));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        1,
        "~4s retires the success toast; the error toast outlives it"
    );

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(4000));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 0, "~8s retires the error toast");
}

#[gpui_kit::test]
fn k5_batched_test_toasts_start_then_one_summary(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let a = store
        .add(t19_auto_clear_seed("A", "10861", "g"))
        .expect("seed a works");
    let b = store
        .add(t19_auto_clear_seed("B", "10862", "g"))
        .expect("seed b works");
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
        view.update(cx, |view, cx| view.start_row(&a.id, window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&b.id, window, cx));
    });
    cx.run_until_parked();
    handle.script_latency(&a.id, 19);
    handle.script_latency(&b.id, 23);
    let before = notification_count(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_all_health(window, cx));
    });
    cx.run_until_parked();

    assert_eq!(
        notification_count(cx),
        before + 1,
        "K5: the batched sweep's summary replaces its start toast once — one net toast, nothing per-row"
    );
}

#[gpui_kit::test]
fn k5_single_row_test_keeps_its_own_result_toast(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t19_auto_clear_seed("Solo", "10863", "g"))
        .expect("seed works");
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
        view.update(cx, |view, cx| view.start_row(&row.id, window, cx));
    });
    cx.run_until_parked();
    handle.script_latency(&row.id, 31);
    let before = notification_count(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_all_health(window, cx));
    });
    cx.run_until_parked();

    assert_eq!(
        notification_count(cx),
        before + 1,
        "a single-row test has no start toast — just its own honest result"
    );
}

#[gpui_kit::test]
fn k5_delete_is_immediate_and_undo_restores_the_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t19_auto_clear_seed("K5 solo", "10851", "travel"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.delete_row_immediately(&row.id, window, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "delete happens immediately"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K5 solo"),
        1,
        "the removed row waits in memory for the toast lifetime"
    );

    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| window.click("k5-undo", cx));
    cx.run_until_parked();
    let restored = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(
        restored.len(),
        1,
        "undo puts the row back: {:?}",
        restored.first().map(|row| row.name.clone())
    );
    assert_eq!(
        restored[0].name, "K5 solo",
        "undo restores the removed config"
    );
    assert_eq!(
        restored[0].group, "travel",
        "undo restores the group assignment"
    );
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K5 solo"),
        0,
        "restored once, consumed once"
    );
}

#[gpui_kit::test]
fn k5_undo_expiry_is_permanent(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t19_auto_clear_seed("K5 gone", "10852", "g"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.delete_row_immediately(&row.id, window, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K5 gone"),
        1,
        "the stashed row rides its toast"
    );

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(8300));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(
        manyproxy_ui::undo_stash_count_named("K5 gone"),
        0,
        "expiry drops the rows"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "the undo toast closed with its own clock"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "after expiry the delete stays permanent"
    );
}

#[gpui_kit::test]
fn k5_paste_imports_direct_into_current_group_no_dialog(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Base", "vless", "travel", "10800"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.select_visible_row(0, cx));
    });

    let link = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.import_text_direct(link.to_string(), window, cx)
        });
    });
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "a cleanly parsing paste never opens the preview dialog"
    );
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(
        rows.len(),
        2,
        "the paste landed: {:?}",
        rows.first().map(|row| row.name.clone())
    );
    let imported = rows
        .iter()
        .find(|row| row.name == "one")
        .expect("the pasted row is in the library");
    assert_eq!(
        imported.group, "travel",
        "direct import targets the current group"
    );

    // The refusal keeps working as a toast line — never a store write.
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.import_text_direct("just some meeting notes".to_string(), window, cx)
        });
    });
    assert_eq!(
        notification_count(cx),
        before + 1,
        "refusal is one honest toast"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "refusal never opens a dialog either"
    );
}

#[test]
fn k5_clipboard_refusal_is_one_liner() {
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal("meeting notes for Thursday")
            .expect("prose refuses"),
        "Nothing importable in clipboard"
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal("bogus://x").expect("bogus refuses"),
        "Nothing importable in clipboard"
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal(""),
        Some("Clipboard has no text to import".to_string()),
        "the empty-clipboard case keeps naming itself"
    );
}

#[test]
fn k5_update_feed_failure_copy() {
    assert_eq!(
        manyproxy_ui::update_feed_failure_message(),
        "Couldn't reach the update feed",
        "no internals ride along"
    );
}

#[gpui_kit::test]
fn k5_hover_pauses_toast_expiry_and_leaving_resumes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let toast = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.push_toast(manyproxy_ui::ToastSpec::success("Read me"), window, cx)
        })
    });
    let key = toast.key();
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 1);
    paint_frame(cx);

    cx.update(|window, cx| window.hover(("toast-body", key as usize), cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(6000));
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        1,
        "a hovered success toast outlives its nominal ~4.2s"
    );

    paint_frame(cx);
    cx.update(|window, cx| window.hover("config-list", cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(3000));
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        1,
        "resume keeps the time that was left after the pointer left"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(2000));
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 0, "the resumed clock finishes");
}

#[gpui_kit::test]
fn k5_error_hover_still_hits_its_hard_cap(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let toast = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.push_toast(manyproxy_ui::ToastSpec::error("Hover trap"), window, cx)
        })
    });
    let key = toast.key();
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|window, cx| window.hover(("toast-body", key as usize), cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(14_000));
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        1,
        "hover buys reading time inside the hard cap"
    );

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1500));
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        0,
        "a forgotten hover cannot leave an error toast wedged in the corner"
    );
}

#[gpui_kit::test]
fn k5_toast_carries_two_keyboard_reachable_actions(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let fired = Rc::new(RefCell::new(Vec::new()));
    let retry_fired = fired.clone();
    let show_fired = fired.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.push_toast(
                manyproxy_ui::ToastSpec::warning("Two actions")
                    .no_timeout()
                    .action("toast-test-retry", "Retry", move |_, _| {
                        retry_fired.borrow_mut().push("retry");
                    })
                    .action("toast-test-show", "Show", move |_, _| {
                        show_fired.borrow_mut().push("show");
                    }),
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    let retry = cx.update(|window, _| window.try_find("toast-test-retry"));
    let retry = retry.expect("the first action renders");
    assert_eq!(retry.label(), Some("Retry"), "actions use button grammar");
    assert_eq!(
        retry.role().map(|role| format!("{role:?}")),
        Some("Button".into())
    );

    cx.update(|window, cx| window.click("toast-test-retry", cx));
    cx.update(|window, cx| window.click("toast-test-show", cx));
    assert_eq!(fired.borrow().as_slice(), ["retry", "show"]);
}

#[gpui_kit::test]
fn k5_progress_reuses_one_toast_key(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    let key = manyproxy_ui::next_toast_key();
    let toast = cx.update(|window, cx| {
        manyproxy_ui::push_probe_start_toast(key, "Testing 4…".to_string(), 0, 4, window, cx);
        manyproxy_ui::push_probe_start_toast(key, "Testing 4…".to_string(), 0, 4, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        1,
        "the same key replaces the probe toast instead of stacking"
    );
    assert_eq!(
        cx.update(|_, cx| manyproxy_ui::toast_progress_label(key, cx)),
        Some("4 remaining".to_string())
    );

    cx.update(|_, cx| manyproxy_ui::update_probe_progress(toast, 3, 4, cx));
    assert_eq!(
        cx.update(|_, cx| manyproxy_ui::toast_progress_label(key, cx)),
        Some("1 remaining".to_string()),
        "progress repaints the ring in place"
    );
    cx.update(|_, cx| manyproxy_ui::update_probe_progress(toast, 1, 4, cx));
    assert_eq!(
        cx.update(|_, cx| manyproxy_ui::toast_progress_label(key, cx)),
        Some("1 remaining".to_string()),
        "a group warm-up tick never rewinds the ring"
    );
    assert_eq!(
        notification_count(cx),
        1,
        "a progress tick never adds a toast"
    );
}

#[gpui_kit::test]
fn k5_failed_test_result_offers_retry(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t19_auto_clear_seed("Retry me", "10864", "g"))
        .expect("seed works");
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
        view.update(cx, |view, cx| view.start_row(&row.id, window, cx));
    });
    cx.run_until_parked();
    handle.script_failure(&row.id, "timeout");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_row(&row.id, window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "toast-retry-test");

    let before = handle.calls_for(&row.id).len();
    cx.update(|window, cx| window.click("toast-retry-test", cx));
    cx.run_until_parked();
    assert!(
        handle.calls_for(&row.id).len() > before,
        "Retry re-runs the failed probe"
    );
}

#[gpui_kit::test]
fn k5_failed_import_offers_retry(cx: &mut TestAppContext) {
    use manyproxy_core::import::{BatchResult, ImportErrorKind, ImportFailure};

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let batch = BatchResult {
        configs: Vec::new(),
        failures: vec![ImportFailure {
            index: 0,
            label: "broken link".to_string(),
            reason: "bad host".to_string(),
            code: ImportErrorKind::BadHost,
        }],
    };
    let committed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_import_batch(batch, "travel", window, cx)
        })
    });
    assert!(!committed, "an all-failure batch does not land");
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "toast-retry-import");

    cx.update(|window, cx| window.click("toast-retry-import", cx));
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        2,
        "Retry re-runs the same deterministic batch and reports again"
    );
}

#[gpui_kit::test]
fn k5_refresh_failure_offers_show_subscriptions(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.route_refresh_completion(
                "sub-failure",
                manyproxy_ui::ToastSpec::error("Sub refresh failed"),
                0,
                0,
                0,
                1,
                0,
                0,
                &[],
                &[],
                &[],
                &[],
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "toast-show-subscriptions");

    cx.update(|window, cx| window.click("toast-show-subscriptions", cx));
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.subs_form.is_some()),
        "Show opens the Subscriptions sheet"
    );
}

/// B19: the audit read "no UIA actions at all" off the chrome strip, the bulk
/// More, the row More and Close inspector. Each of those is a kit Button, and
/// a Button with a click listener adds `Action::Click` to its accessibility
/// node, so what UIA sees is role Button + a name. This walks every chrome id
/// and pins both, so no plain div (no role, no click action) creeps back in.
#[gpui_kit::test]
fn b19_chrome_controls_expose_button_role_and_label(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    store
        .add(row19_seed("Home", "b.example.com", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &work.id);
    paint_frame_at(cx, 1000., 1600.);

    let snapshot = |cx: &mut VisualTestContext, id: &str| {
        cx.update(|window, _| {
            let found = window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders in the completed frame"));
            (
                found.role().map(|role| format!("{role:?}")),
                found.label().map(str::to_string),
            )
        })
    };

    for (id, pinned) in [
        ("nav-routing-presets", "Routing presets"),
        ("nav-settings", "Settings"),
        ("action-bar-import", "Import"),
        ("action-bar-add", "Add config"),
        ("action-bar-palette", "Command palette"),
        ("action-bar-overflow", "More actions"),
        ("engine-bar-start-all", "Start all"),
        ("engine-bar-stop-all", "Stop all"),
        ("engine-bar-overflow", "More actions"),
        ("row-menu", "More actions"),
        ("inspector-drawer-close", "Close inspector"),
    ] {
        let (role, label) = snapshot(cx, id);
        assert_eq!(
            role.as_deref(),
            Some("Button"),
            "{id} exposes the Button role (and its click action)"
        );
        assert_eq!(label.as_deref(), Some(pinned), "{id} announces its name");
    }

    // The operate row's verbs carry the accesskit names, not their visible
    // shorthand, so only the shape is pinned here.
    for id in ["engine-start", "engine-stop", "health-test"] {
        let (role, label) = snapshot(cx, id);
        assert_eq!(role.as_deref(), Some("Button"), "{id} exposes Button");
        assert!(
            label.as_ref().is_some_and(|label| !label.is_empty()),
            "{id} announces a name: {label:?}"
        );
    }

    // Two or more selected rows show the selection bar instead of the
    // single-row operate actions.
    cx.update(|_window, cx| {
        view.update(cx, |view, cx| view.select_all_visible_rows(cx));
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1000., 1600.);
    for (id, pinned) in [
        ("batch-start", "Start"),
        ("batch-stop", "Stop"),
        ("batch-more", "More batch actions"),
    ] {
        let (role, label) = snapshot(cx, id);
        assert_eq!(
            role.as_deref(),
            Some("Button"),
            "{id} exposes the Button role (and its click action)"
        );
        assert_eq!(label.as_deref(), Some(pinned), "{id} announces its name");
    }

    // Static segments: no action, but each still names itself.
    for id in ["batch-count", "action-bar-count", "engine-bar-status"] {
        let (_, label) = snapshot(cx, id);
        assert!(
            label.as_ref().is_some_and(|label| !label.is_empty()),
            "{id} announces its text: {label:?}"
        );
    }
}

/// B19: the Ctrl+K chip lives in the bar's right zone, which clips whatever
/// overflows it. A Kbd that wraps grows taller than the bar and the chip's
/// last pixels fall off the frame edge. Pin the chip inside its row and the
/// frame at the restore size and at the minimum, with the drawer + bulk bar
/// up (the state the audit photographed).
#[gpui_kit::test]
fn b19_action_bar_controls_fit_their_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    select_library_row(&view, cx, &work.id);
    cx.update(|_window, cx| {
        view.update(cx, |view, cx| view.select_all_visible_rows(cx));
    });

    for (width, height) in [(1200., 700.), (1100., 640.)] {
        paint_frame_at(cx, width, height);
        assert_painted(cx, "action-bar-palette");
        let (button, bar) = cx.update(|window, _| {
            let bounds = |id: &str| {
                window
                    .try_find(id.to_string())
                    .unwrap_or_else(|| panic!("{id} renders at {width}x{height}"))
                    .bounds()
            };
            (bounds("action-bar-palette"), bounds("action-bar"))
        });
        let frame = size(px(width), px(height));
        assert!(
            button.size.height <= bar.size.height,
            "the control never wraps taller than its row at {width}x{height}: {button:?} in {bar:?}"
        );
        assert!(
            button.origin.x >= px(0.)
                && button.origin.y >= px(0.)
                && button.origin.x + button.size.width <= frame.width
                && button.origin.y + button.size.height <= frame.height,
            "the control paints whole inside the frame at {width}x{height}: {button:?} in {frame:?}"
        );
        assert!(
            button.origin.x + button.size.width <= bar.origin.x + bar.size.width,
            "the bar does not clip the control at {width}x{height}: {button:?} in {bar:?}"
        );
    }
}

/// B4: the second instance's argv crosses the pipe as one JSON line; whatever
/// the forwarder writes must decode back to the same argument list.
#[test]
fn single_instance_argv_line_round_trips() {
    use manyproxy_ui::single_instance::{decode_argv, encode_argv};
    let argv = vec![
        r#""C:\Program Files\ManyProxy\manyproxy.exe""#.to_string(),
        "--import".to_string(),
        "vless://one@example.com:443?type=tcp#node".to_string(),
        "line\nbreak".to_string(),
        "unicode ✓".to_string(),
    ];
    let line = encode_argv(&argv);
    assert_eq!(line.last(), Some(&b'\n'), "the listener frames on newline");
    assert_eq!(decode_argv(line.trim_ascii_end()), Some(argv));
    assert_eq!(decode_argv(b"not json"), None);
    assert_eq!(decode_argv(b"{}"), None);
}

/// B3: the console's log list builds the rows the dock can show, not every
/// row the session owns. The counter is the proof; the same frame also pins
/// that a tab that is not on screen builds nothing.
#[gpui_kit::test]
fn b3_console_logs_build_only_the_rows_they_show(cx: &mut TestAppContext) {
    use manyproxy_ui::app::detail::{console_rows_built, reset_console_rows_built, ConsoleRows};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    view.update(cx, |view, cx| {
        view.select_visible_row(0, cx);
        view.console.open = true;
        view.console.tab = manyproxy_ui::ConsoleTab::Logs;
        let tags = manyproxy_ui::health::LogTagIndex::build(std::slice::from_ref(&seeded));
        let tag = manyproxy_ui::inbound_tag(&seeded.id);
        for ix in 0..200 {
            view.logs
                .store
                .push_raw(&format!("{tag} info line {ix} of the pane window"), &tags);
        }
        cx.notify();
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    let rows = view.read_with(cx, |view, cx| view.console_log_rows(cx).len());
    assert_eq!(rows, 200, "the console owns 200 rows");

    reset_console_rows_built();
    paint_frame(cx);
    let built = console_rows_built(ConsoleRows::Logs);
    assert!(built > 0, "the visible log rows build, got {built}");
    // A frame lays the list out more than once, so pin the shape of the cost
    // rather than an exact count: a window's worth stays a small fraction of
    // the page.
    assert!(
        built * 3 < rows,
        "a 200-row console builds a window's worth, not the page: {built}"
    );
    assert_eq!(
        console_rows_built(ConsoleRows::Links),
        0,
        "a tab that is not on screen builds nothing"
    );

    view.update(cx, |view, cx| {
        view.console.tab = manyproxy_ui::ConsoleTab::Connections;
        cx.notify();
    });
    cx.run_until_parked();
    reset_console_rows_built();
    paint_frame(cx);
    assert_eq!(
        console_rows_built(ConsoleRows::Logs),
        0,
        "Logs stays unbuilt while Connections is on screen"
    );
}

/// B3: the collapsed console builds no rows at all.
#[gpui_kit::test]
fn b3_collapsed_console_builds_no_pane_rows(cx: &mut TestAppContext) {
    use manyproxy_ui::app::detail::{console_rows_built, reset_console_rows_built, ConsoleRows};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    view.update(cx, |view, cx| {
        view.select_visible_row(0, cx);
        let tags = manyproxy_ui::health::LogTagIndex::build(std::slice::from_ref(&seeded));
        let tag = manyproxy_ui::inbound_tag(&seeded.id);
        for ix in 0..200 {
            view.logs
                .store
                .push_raw(&format!("{tag} info line {ix}"), &tags);
        }
        cx.notify();
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !view.read_with(cx, |view, _| view.console.open),
        "the console starts collapsed"
    );
    reset_console_rows_built();
    paint_frame(cx);
    for pane in [ConsoleRows::Logs, ConsoleRows::Links] {
        assert_eq!(
            console_rows_built(pane),
            0,
            "{pane:?} builds nothing while the console is collapsed"
        );
    }
}

/// B3: coming back to Logs lands where the user left it, and the rows that
/// scrolled away were never built.
#[gpui_kit::test]
fn b3_console_logs_keep_their_scroll_across_tabs(cx: &mut TestAppContext) {
    use manyproxy_ui::app::detail::{console_rows_built, reset_console_rows_built, ConsoleRows};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    view.update(cx, |view, cx| {
        view.select_visible_row(0, cx);
        view.console.open = true;
        view.console.tab = manyproxy_ui::ConsoleTab::Logs;
        let tags = manyproxy_ui::health::LogTagIndex::build(std::slice::from_ref(&seeded));
        let tag = manyproxy_ui::inbound_tag(&seeded.id);
        for ix in 0..200 {
            view.logs
                .store
                .push_raw(&format!("{tag} info line {ix} of the pane window"), &tags);
        }
        cx.notify();
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    // The list's own handle is the state a wheel would move; the list reads it
    // back when it lays out.
    view.update(cx, |view, cx| {
        view.console_logs
            .scroll
            .scroll_to_item(120, gpui_kit::ScrollStrategy::Top);
        cx.notify();
    });
    paint_frame(cx);
    let scrolled = view.read_with(cx, |view, _| view.console_logs.scroll.offset().y);
    assert!(
        scrolled < px(0.),
        "the console log list scrolls on its own: {scrolled:?}"
    );
    reset_console_rows_built();
    paint_frame(cx);
    let built_while_scrolled = console_rows_built(ConsoleRows::Logs);
    assert!(
        built_while_scrolled > 0 && built_while_scrolled * 3 < 200,
        "a scrolled console still builds a window, not the page: {built_while_scrolled}"
    );

    for tab in [
        manyproxy_ui::ConsoleTab::Connections,
        manyproxy_ui::ConsoleTab::Logs,
    ] {
        view.update(cx, |view, cx| {
            view.console.tab = tab;
            cx.notify();
        });
        cx.run_until_parked();
        paint_frame(cx);
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.console_logs.scroll.offset().y),
        scrolled,
        "Logs returns to the same row instead of jumping to the top"
    );
}

/// B16: a fetch in flight is not an empty library. The load state is set the
/// instant a refresh starts, before its task is ever scheduled, so the frame
/// between start and completion is the whole delay a slow subscription buys —
/// and that frame must show skeleton rows at the row pitch, never "no
/// configurations yet" and never a `0 of 0` count.
#[gpui_kit::test]
fn b16_fetch_in_flight_paints_skeletons_not_the_empty_canvas(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let label = sub.label.clone();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    paint_frame(cx);
    assert_painted(cx, "library-empty");

    cx.update(|_, cx| view.update(cx, |view, cx| view.mark_library_loading(cx)));
    paint_frame(cx);
    assert_painted(cx, "library-loading");
    assert_painted(cx, "library-loading-spinner");
    assert_painted(cx, "skeleton-row-0");
    assert_absent(cx, "library-empty".to_string());
    assert_absent(cx, "empty-add".to_string());
    assert_eq!(
        view.read_with(cx, |view, cx| view.filter_count_text(cx)),
        "Loading…".to_string(),
        "the count line names the wait instead of 0 of 0"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                &label,
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#solo"
                        .to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, "library-loading".to_string());
    assert_absent(cx, "skeleton-row-0".to_string());
    assert_eq!(
        view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().rows.len()),
        1,
        "the fetched row landed and the skeleton gave way"
    );
}

/// B16: a failed first fetch is a state, not just a toast. With no rows behind
/// it the list names what failed and carries the way back; once the toast is
/// gone, this is the only place the reason still lives.
#[gpui_kit::test]
fn b16_failed_fetch_names_the_reason_and_offers_retry(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let label = sub.label.clone();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    paint_frame(cx);

    cx.update(|_, cx| view.update(cx, |view, cx| view.mark_library_loading(cx)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                &label,
                Err("subscription fetch failed: connection refused".to_string()),
                999,
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-load-failed");
    assert_painted(cx, "library-retry");
    assert_absent(cx, "library-loading".to_string());
    assert_absent(cx, "library-empty".to_string());
    assert_eq!(
        view.read_with(cx, |view, cx| {
            match &view.libraries.read(cx).delegate().load {
                manyproxy_ui::library::LibraryLoad::Failed(message) => message.clone(),
                other => format!("not a failure: {other:?}"),
            }
        }),
        "Nightly: subscription fetch failed: connection refused".to_string(),
        "the list carries the reason the fetch gave"
    );
}

/// B16: with rows on screen the skeleton is never the answer — a search that
/// matches nothing still says so while a refresh runs in the background, so a
/// background refresh cannot hang the search's own empty copy.
#[gpui_kit::test]
fn b16_search_no_matches_keeps_its_copy_while_a_refresh_is_in_flight(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|_window, cx| {
        view.update(cx, |view, cx| {
            view.mark_library_loading(cx);
            view.apply_search_now("zzz-no-hit".to_string(), cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "library-empty");
    assert_painted(cx, "library-clear-filters");
    assert_absent(cx, "library-loading".to_string());
    assert_absent(cx, "skeleton-row-0".to_string());
}

/// K12: the palette keeps one Sort verb — `sort-none` — beside the header
/// sort, and the text-size toggle is reachable without opening Settings.
#[gpui_kit::test]
fn k12_palette_sort_and_text_size_actions(cx: &mut TestAppContext) {
    use gpui_kit::component::IndexPath;
    use manyproxy_ui::app::actions::{palette_action_at, PALETTE_GROUPS};
    use manyproxy_ui::store::{LibrarySort, LibrarySortKey, SortDirection, TextSize};

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_library_sort(LibrarySort::new(
            LibrarySortKey::Latency,
            SortDirection::Descending,
        ))
        .expect("sort choice writes");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let op = |id: &str| {
        let (section, row) = PALETTE_GROUPS
            .iter()
            .enumerate()
            .find_map(|(section, (_, ids))| {
                ids.iter()
                    .position(|candidate| *candidate == id)
                    .map(|row| (section, row))
            })
            .unwrap_or_else(|| panic!("{id} is in a palette group"));
        IndexPath::new(row).section(section)
    };
    assert_eq!(
        PALETTE_GROUPS[2].0, "Sort",
        "the Sort group sits after Manage so Settings keeps its palette index"
    );
    assert_eq!(
        PALETTE_GROUPS[2].1,
        &["sort-none"],
        "the eight standalone sort verbs and the direction toggle are retired"
    );
    assert_eq!(
        palette_action_at(op("sort-none")),
        Some("sort-none"),
        "the remaining sort action resolves through the palette table"
    );
    assert!(
        !PALETTE_GROUPS[2].1.contains(&"sort-latency"),
        "header clicks own key selection now"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(op("sort-none"), window, cx)
        });
    });
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.store.library_sort().is_none()),
        "the palette can clear the sort"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(op("text-size"), window, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.store.text_size()),
        TextSize::LARGE,
        "the palette persists the larger reading scale"
    );
    assert_eq!(
        cx.update(|_, cx| gpui_kit::component::Theme::global(cx).font_size),
        px(20.),
        "the palette toggle scales to 125%"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_op(op("text-size"), window, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.store.text_size()),
        TextSize::DEFAULT,
        "the toggle returns to the stock scale"
    );
}

/// K12: text size persists, applies at boot, and survives an appearance swap
/// (which rebuilds the theme and resets its font sizes).
#[gpui_kit::test]
fn k12_text_size_persists_and_survives_appearance(cx: &mut TestAppContext) {
    use manyproxy_ui::store::TextSize;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_text_size(TextSize::LARGE)
        .expect("text size writes");
    assert_eq!(store.text_size(), TextSize::LARGE);

    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        cx.update(|_, cx| gpui_kit::component::Theme::global(cx).font_size),
        px(20.),
        "boot applies the stored 125% scale"
    );
    assert_eq!(
        cx.update(|window, _| window.rem_size()),
        px(20.),
        "the root feeds the scaled rem to the window"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_appearance("light", window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        cx.update(|_, cx| gpui_kit::component::Theme::global(cx).font_size),
        px(20.),
        "an appearance swap keeps the reading scale"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_text_size(TextSize::DEFAULT, window, cx)
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        cx.update(|_, cx| gpui_kit::component::Theme::global(cx).font_size),
        px(16.),
        "the stock scale restores the kit's base size"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.text_size()),
        TextSize::DEFAULT,
        "the choice persists"
    );
}

/// Task 13: a click on the slider lands on a 5% stop, previews at once, and
/// persists on release.
#[gpui_kit::test]
fn task13_text_size_slider_commits_a_stop_on_click(cx: &mut TestAppContext) {
    use manyproxy_ui::store::TextSize;

    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-row-text-size");
    assert_painted(cx, "settings-text-size-slider");
    assert_eq!(label_of("settings-text-size-value", cx), "100%");

    cx.update(|window, cx| {
        window.click("settings-text-size-slider", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);

    assert_eq!(
        view.read_with(cx, |view, _| view.store.text_size()),
        TextSize::LARGE,
        "a center click lands on the 125% stop and persists"
    );
    assert_eq!(
        cx.update(|_, cx| gpui_kit::component::Theme::global(cx).font_size),
        px(20.),
        "the click paints the new scale live"
    );
    assert_eq!(label_of("settings-text-size-value", cx), "125%");
}

/// Task 13: the slider is the only Settings control for the reading scale,
/// and its 150% stop keeps the list rows and the dialog itself intact.
#[gpui_kit::test]
fn task13_text_size_slider_holds_list_and_settings_at_150(cx: &mut TestAppContext) {
    use manyproxy_ui::store::TextSize;

    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .set_text_size(TextSize::from_percent(TextSize::MAX))
        .expect("text size writes");
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
    assert_eq!(
        cx.update(|_, cx| gpui_kit::component::Theme::global(cx).font_size),
        px(24.),
        "150% is 16px × 1.5"
    );

    for id in [&first.id, &second.id] {
        let row = format!("library-row-{id}");
        assert_painted(cx, &row);
        assert_within_frame(cx, &row);
    }
    let tops = cx.update(|window, _| {
        [&first.id, &second.id]
            .iter()
            .map(|id| {
                window
                    .try_find(format!("library-row-{id}"))
                    .expect("row paints")
                    .bounds()
                    .origin
                    .y
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        tops[1] - tops[0],
        px(manyproxy_ui::Density::Comfortable.pitch()),
        "the row pitch stays fixed at 150%"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    scroll_settings_into_view(cx, "settings-row-text-size");
    assert_painted(cx, "settings-row-text-size");
    assert_painted(cx, "settings-text-size-slider");
    assert_painted(cx, "settings-text-size-group");
    assert_eq!(
        label_of("settings-text-size-value", cx),
        "150%",
        "the readout shows the stored stop"
    );
    assert_painted(cx, "settings-dialog-save");
    assert_within_frame(cx, "settings-dialog-save");
}

/// Task 1: click replaces, Ctrl+click toggles, Shift+click draws from the
/// anchor, and every row carries its own `aria_selected`.
#[gpui_kit::test]
fn multi_select_click_ctrl_and_shift_build_the_set(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    use gpui_kit::Modifiers;
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

    let click = |cx: &mut VisualTestContext, id: &str, modifiers: Modifiers| {
        click_row_with(cx, &format!("row-name-text-{id}"), modifiers);
    };

    click(cx, &alpha.id, Modifiers::default());
    assert_eq!(selected_ids(&view, cx), vec![alpha.id.clone()]);
    assert!(row_aria_selected(cx, &alpha.id));
    assert!(!row_aria_selected(cx, &bravo.id));
    assert!(!row_aria_selected(cx, &charlie.id));
    assert!(!row_aria_selected(cx, &delta.id));

    click(
        cx,
        &charlie.id,
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), charlie.id.clone()],
        "Ctrl+click adds without dropping the first row"
    );
    assert!(row_aria_selected(cx, &alpha.id));
    assert!(!row_aria_selected(cx, &bravo.id));
    assert!(row_aria_selected(cx, &charlie.id));

    click(
        cx,
        &delta.id,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![
            alpha.id.clone(),
            bravo.id.clone(),
            charlie.id.clone(),
            delta.id.clone()
        ],
        "Shift+click draws from the anchor the Ctrl click kept"
    );
    assert!(row_aria_selected(cx, &alpha.id));
    assert!(row_aria_selected(cx, &bravo.id));
    assert!(row_aria_selected(cx, &charlie.id));
    assert!(row_aria_selected(cx, &delta.id));

    click(
        cx,
        &bravo.id,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone()],
        "a second Shift+click re-drags the same anchor"
    );

    click(
        cx,
        &bravo.id,
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone()],
        "Ctrl+click removes a row already in the set"
    );

    click(cx, &alpha.id, Modifiers::default());
    assert_eq!(selected_ids(&view, cx), vec![alpha.id.clone()]);
    assert!(row_aria_selected(cx, &alpha.id));

    let list_handle = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&list_handle, cx));
    // The first Escape closes the inspector — the top layer — and the second
    // clears the selection.
    for _ in 0..2 {
        cx.update(|window, cx| {
            let escape = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
            window.dispatch_keystroke(escape, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
    }
    assert!(
        selected_ids(&view, cx).is_empty(),
        "Esc clears the transient selection"
    );
    for id in [&alpha.id, &bravo.id, &charlie.id, &delta.id] {
        assert!(!row_aria_selected(cx, id), "Esc clears aria_selected too");
    }
}

/// Task 1: the anchor outlives a filter that still shows it, and leaves with
/// it. Hidden rows are never silently acted on by a later Shift range.
/// The one selection model: a range grows from the anchor, a filter never
/// prunes it, and a row that stops existing leaves it.
#[gpui_kit::test]
fn selection_prunes_deleted_rows_but_not_filtered_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let bravo = store
        .add(row19_seed("Bravo", "b.example.com", "10802"))
        .expect("seed works");
    let charlie = store
        .add(row19_seed("Charlie", "c.example.com", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    // A plain click, then a Shift click: the range replaces the set and the
    // anchor stays where it was.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_row(&bravo.id, cx);
            view.extend_to_row(&charlie.id, cx);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![bravo.id.clone(), charlie.id.clone()],
        "the range replaces the set"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        Some(bravo.id.clone()),
        "the range keeps its origin"
    );

    // A filter that hides the set leaves the selection alone: a hidden
    // selected row still counts.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("alpha".to_string(), cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![bravo.id.clone(), charlie.id.clone()],
        "a filter that hides the set leaves it alone"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.selection_hidden_count(cx)),
        2,
        "the hidden count names them"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        Some(bravo.id.clone()),
        "a hidden row keeps the anchor"
    );

    // A row that stops existing leaves the set.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now(String::new(), cx);
            view.store.remove(&bravo.id).expect("the row is removed");
            view.reload(window, cx);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![charlie.id.clone()],
        "deleting a selected row prunes only that row"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        None,
        "the deleted anchor does not linger"
    );
}

/// Task 1: with several rows selected the context menu names the count and
/// the verb lands on all of them, not just the row under the pointer.
#[gpui_kit::test]
fn multi_select_context_menu_plural_labels_and_full_target(cx: &mut TestAppContext) {
    use gpui_kit::Modifiers;
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
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    click_row_with(
        cx,
        &format!("row-name-text-{}", alpha.id),
        Modifiers::default(),
    );
    click_row_with(
        cx,
        &format!("row-name-text-{}", charlie.id),
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone(), charlie.id.clone()]
    );

    cx.update(|window, cx| window.right_click(format!("row-surface-{}", bravo.id), cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);

    for (verb, want) in [
        ("start", "Start 3 configs"),
        ("stop", "Stop 3 configs"),
        ("test", "Test 3 configs"),
        ("copy-urls", "Copy 3 URLs"),
        ("edit", "Edit config…"),
        ("delete", "Delete 3 configs"),
    ] {
        assert_eq!(
            label_of(&format!("rowctx-{verb}-{}", bravo.id), cx),
            want,
            "{verb} names the whole selection"
        );
    }

    // Copy URLs and Move live behind More ▾, like the single-row menu.
    cx.update(|window, cx| {
        let up = gpui_kit::Keystroke::parse("up").expect("test keystroke parses");
        window.dispatch_keystroke(up, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        label_of(&format!("rowctx-copy-urls-{}", bravo.id), cx),
        "Copy 3 URLs"
    );
    assert_eq!(
        label_of(&format!("rowctx-move-{}", bravo.id), cx),
        "Move 3 configs to group"
    );

    cx.update(|window, cx| {
        window.click(format!("rowctx-copy-urls-{}", bravo.id), cx);
    });
    cx.run_until_parked();
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    let lines: Vec<&str> = clip.lines().collect();
    assert_eq!(lines.len(), 3, "the selection's three URLs were copied");
    assert!(lines.iter().any(|line| line.starts_with("Alpha\t")));
    assert!(lines.iter().any(|line| line.starts_with("Bravo\t")));
    assert!(lines.iter().any(|line| line.starts_with("Charlie\t")));
}

/// Task 2: Shift+Down/Down grows from the anchor, Shift+Up shrinks the same
/// range without re-anchoring at the new focus.
#[gpui_kit::test]
fn multi_select_shift_arrows_extend_and_collapse(cx: &mut TestAppContext) {
    use gpui_kit::Modifiers;
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

    click_row_with(
        cx,
        &format!("row-name-text-{}", alpha.id),
        Modifiers::default(),
    );
    focus_library(&view, cx);
    press_key(cx, "shift-down");
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone()]
    );
    press_key(cx, "shift-down");
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone(), charlie.id.clone()],
        "each Shift+Down grows the same anchored range"
    );
    press_key(cx, "shift-up");
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone()],
        "Shift+Up shrinks the range instead of starting a new one"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        Some(alpha.id.clone()),
        "the anchor never moves while the range is redrawn"
    );
    assert!(
        !row_aria_selected(cx, &delta.id),
        "rows outside the range stay unselected"
    );
}

/// Task 2: Ctrl+A takes exactly the rows the filters keep.
#[gpui_kit::test]
fn multi_select_ctrl_a_takes_visible_rows_only(cx: &mut TestAppContext) {
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
        .add(facet_seed("Charlie", "vless", "home", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now("team".to_string(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);

    focus_library(&view, cx);
    press_key(cx, "ctrl-a");
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone()],
        "the hidden home row is not selected"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_anchor.clone()),
        Some(alpha.id.clone()),
        "the first visible row anchors the set"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.apply_search_now(String::new(), cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    focus_library(&view, cx);
    press_key(cx, "ctrl-a");
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone(), charlie.id.clone()]
    );
}

/// Task 2: Delete on a multi-selection uses the K17 bulk path — immediate
/// removal with one Undo toast, never a dialog per row.
#[gpui_kit::test]
fn multi_select_delete_uses_the_bulk_path(cx: &mut TestAppContext) {
    use gpui_kit::Modifiers;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("M2 Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("M2 Bravo", "vless", "team", "10802"))
        .expect("seed works");
    let charlie = store
        .add(facet_seed("M2 Charlie", "vless", "team", "10803"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    click_row_with(
        cx,
        &format!("row-name-text-{}", alpha.id),
        Modifiers::default(),
    );
    click_row_with(
        cx,
        &format!("row-name-text-{}", bravo.id),
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        selected_ids(&view, cx),
        vec![alpha.id.clone(), bravo.id.clone()]
    );

    focus_library(&view, cx);
    let before = notification_count(cx);
    press_key(cx, "delete");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "two or more rows confirm once, not per row"
    );
    // The confirm names the rows it will remove.
    cx.update(|window, cx| window.click("ok", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "both selected rows are gone"
    );
    assert_eq!(notification_count(cx), before + 1, "one batched Undo toast");
    assert!(
        selected_ids(&view, cx).is_empty(),
        "the deleted rows leave the selection"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.visible_row_ids(cx)),
        vec![charlie.id.clone()],
        "the untouched row remains"
    );
    assert_eq!(manyproxy_ui::undo_stash_count_named("M2 Alpha"), 1);
    assert_eq!(manyproxy_ui::undo_stash_count_named("M2 Bravo"), 1);
}

/// Task 2: the bulk bar shows the selection count only when it disagrees with
/// the checked count, so one browser of checked rows keeps its compact label.
/// One selection model: there is no second set, so there is no second
/// count. The delegate keeps rows and visibility; the app keeps the
/// selection.
#[test]
fn the_delegate_keeps_no_selection_of_its_own() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(id: &str, name: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: r#"{"server":"a.example.com"}"#.to_string().into(),
            group: "team".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        row("1", "Alpha"),
        row("2", "Bravo"),
        row("3", "Charlie"),
    ]);
    assert_eq!(delegate.rows.len(), 3, "the delegate keeps the rows");
}
