//! engine: one binary of the split UI suite. Each binary runs as its own
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
fn tick_reprobe_running_records_and_persists(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10851"))
        .expect("seed works");
    let beta = store.add(tick12_seed("Beta", "10852")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
        });
    });
    handle.script_latency(&alpha.id, 21);
    handle.script_failure(&beta.id, "upstream refused");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });

    let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
    assert_eq!(reprobed, 2, "both running rows re-probed");

    cx.run_until_parked();
    assert_eq!(handle.calls_for(&alpha.id).len(), 1);
    assert_eq!(handle.calls_for(&beta.id).len(), 1);

    let (alpha_live, beta_dead) = view.read_with(cx, |view, _| {
        (
            view.health_store.status(&alpha.id),
            view.health_store.status(&beta.id),
        )
    });
    assert_eq!(alpha_live.latency_ms, Some(21));
    assert!(beta_dead
        .cause
        .as_deref()
        .unwrap_or("")
        .contains("upstream refused"));
    let snaps = view.read_with(cx, |view, _| {
        view.store.load_health_snapshots().expect("snapshots load")
    });
    assert_eq!(snaps.len(), 2, "latest samples persisted");
}

#[gpui_kit::test]
fn tick_reprobe_skips_rows_on_crashed_shards(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10861"))
        .expect("seed works");
    let beta = store.add(tick12_seed("Beta", "10862")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let (prober, handle) = FakeHealthProber::shared();
    let stock = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, stock, _fork) = EngineController::fake_pair();
            view.engine = engine;
            view.engine.set_auto_restart(AutoRestartConfig {
                enabled: false,
                ..AutoRestartConfig::default()
            });
            view.prober = Box::new(prober);
            stock
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert!(
        view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)),
        "rows run before the crash"
    );

    stock.inject_crash("signal: segmentation fault");
    let poll = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
    assert!(poll.is_some(), "the crash is reported once");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.ids_on_crashed_shards().len()),
        2,
        "both stock rows sit on the crashed shard"
    );

    let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
    assert_eq!(reprobed, 0, "crashed rows are never re-probed");
    cx.run_until_parked();
    assert!(
        handle.calls_for(&alpha.id).is_empty() && handle.calls_for(&beta.id).is_empty(),
        "no probe ever dials a dead socket"
    );
    let untouched = view.read_with(cx, |view, _| {
        (
            view.health_store.status(&alpha.id),
            view.health_store.status(&beta.id),
        )
    });
    assert!(
        matches!(
            untouched,
            (
                manyproxy_ui::health::RowHealth {
                    state: manyproxy_ui::health::RowState::Unknown,
                    ..
                },
                manyproxy_ui::health::RowHealth {
                    state: manyproxy_ui::health::RowState::Unknown,
                    ..
                },
            )
        ),
        "no manufactured samples for crashed rows: {untouched:?}"
    );

    // Re-enabling auto-restart lets the next production tick recover the shard:
    // the sticky crashed state is gone and probing resumes.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine.set_auto_restart(AutoRestartConfig {
                enabled: true,
                ..AutoRestartConfig::default()
            });
        });
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
    assert!(
        view.read_with(cx, |view, _| view.engine.ids_on_crashed_shards().is_empty()),
        "auto-restart clears the crashed shard"
    );
    let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
    assert_eq!(reprobed, 2, "recovered rows are re-probed again");
    cx.run_until_parked();
    assert_eq!(
        handle.calls_for(&alpha.id).len(),
        1,
        "alpha is probed after recovery"
    );
}

#[gpui_kit::test]
fn restart_restores_last_latency_and_dead_flags(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let path = std::env::temp_dir().join(format!(
        "manyproxy-tick12-restart-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let (alpha_id, beta_id) = {
        let mut store = Store::open_file(&path).expect("file store opens");
        let alpha = store
            .add(tick12_seed("Alpha", "10853"))
            .expect("seed works");
        let beta = store.add(tick12_seed("Beta", "10854")).expect("seed works");
        (alpha.id.clone(), beta.id.clone())
    };
    let store = Store::open_file(&path).expect("reopen works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
        });
    });
    handle.script_latency(&alpha_id, 33);
    handle.script_failure(&beta_id, "upstream refused");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    let tested =
        cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(tested, 2);

    cx.run_until_parked();
    drop(view);

    let store = Store::open_file(&path).expect("restart reopen works");
    let stash2: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash2_for_window = stash2.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let restarted = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash2_for_window.borrow_mut() = Some(restarted.clone());
        Root::new(restarted, window, cx)
    });
    let restarted = stash2.borrow().clone().expect("AppView stashed");

    let (live, dead) = restarted.read_with(cx, |view, _| {
        (
            view.health_store.status(&alpha_id),
            view.health_store.status(&beta_id),
        )
    });
    assert_eq!(live.latency_ms, Some(33), "latency survives restart");
    assert!(
        dead.cause
            .as_deref()
            .unwrap_or("")
            .contains("upstream refused"),
        "dead flag + cause survive restart: {:?}",
        dead.cause
    );
    let _ = std::fs::remove_file(&path);
}

#[gpui_kit::test]
fn stopped_ticks_start_nothing(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10855"))
        .expect("seed works");
    store
        .add_subscription("Nightly", "https://example.com/sub.txt", "travel", true)
        .expect("due sub (never refreshed)");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.due_subscriptions().len()),
        1
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| view.stop_background_ticks());
    });
    let poll = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
    assert_eq!(poll, None);
    let subs = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_due_subscriptions(cx)));
    assert_eq!(subs, 0, "stopped pass starts no fetch");
    let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
    assert_eq!(reprobed, 0, "stopped pass probes nothing");
    assert!(
        handle.calls_for(&alpha.id).is_empty(),
        "stopped ticks never touch the network"
    );
}

#[gpui_kit::test]
fn bug18_custom_global_test_url_excluded_from_traffic(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10864"))
        .expect("seed works");
    store
        .set_test_url("http://203.0.113.9/generate_204")
        .expect("custom global url sets");

    let (port, server) = stub_clash_server(
        vec![bug18_probe_body(&alpha.id, Some((500, 60)), (700, 70))],
        "s3cret",
    );
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (mut engine, _) = EngineController::fake();
            engine.set_clash_targets(vec![ClashTarget {
                core: "stock".to_string(),
                port,
                secret: "s3cret".to_string().into(),
            }]);
            view.engine = engine;
        });
    });

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 1, "only the data-plane flow is credited");
    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(
        totals.get(&alpha.id),
        Some(&(1000, 50)),
        "gstatic and the custom-url probe bytes are both excluded"
    );
    let snapshot = view.read_with(cx, |view, _| view.conn.snapshot.clone());
    assert_eq!(snapshot.len(), 1, "no probe flow reaches the snapshot");
    server.join().expect("stub drains");
}

#[gpui_kit::test]
fn bug18_row_override_test_url_excluded_from_traffic(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10865"))
        .expect("seed works");
    store
        .set_row_test_url(&alpha.id, "http://203.0.113.9/health")
        .expect("row override sets");

    let (port, server) =
        stub_clash_server(vec![bug18_probe_body(&alpha.id, None, (700, 70))], "s3cret");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine = EngineController::fake().0;
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.engine.set_clash_targets(vec![ClashTarget {
                core: "stock".to_string(),
                port,
                secret: "s3cret".to_string().into(),
            }]);
        });
    });

    let exclusions = view.read_with(cx, |view, _| view.traffic_probe_exclusions());
    assert!(
        exclusions.excludes("203.0.113.9"),
        "a running row's override is excluded: {:?}",
        exclusions.hosts
    );

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 1);
    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(
        totals.get(&alpha.id),
        Some(&(1000, 50)),
        "the row-override probe bytes are excluded"
    );
    server.join().expect("stub drains");
}

#[gpui_kit::test]
fn tick_traffic_poll_persists_deltas_and_mirrors(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10856"))
        .expect("seed works");

    let (port, server) = stub_clash_server(
        vec![
            traffic_body(&alpha.id, 1000, 50),
            traffic_body(&alpha.id, 1500, 70),
            traffic_body(&alpha.id, 1500, 70),
        ],
        "s3cret",
    );

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (mut engine, _) = EngineController::fake();
            engine.set_clash_targets(vec![ClashTarget {
                core: "stock".to_string(),
                port,
                secret: "s3cret".to_string().into(),
            }]);
            view.engine = engine;
        });
    });

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 1, "one row gains data-plane bytes");
    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(
        totals.get(&alpha.id),
        Some(&(1000, 50)),
        "probe bytes never reach the traffic columns"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.health_store.data_traffic(&alpha.id)),
        Some((1000, 50)),
        "mirror tracks persisted absolutes"
    );

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 1, "grown connections persist the delta");
    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(totals.get(&alpha.id), Some(&(1500, 70)));

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(
        updated, 0,
        "identical poll persists nothing (no double count)"
    );
    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(totals.get(&alpha.id), Some(&(1500, 70)));
    server.join().expect("stub drains");
}

#[gpui_kit::test]
fn traffic_delta_rx_up_tx_down_no_underflow(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10859"))
        .expect("seed works");

    // Same connection key: download grows while upload shrinks. A lexicographic
    // `(rx, tx) >= (last_rx, last_tx)` compare passes (rx grew) and underflows
    // `tx - last_tx` — a debug panic, a ~1.8e19 wrap in release. The component
    // guard must fall back to the fresh counters instead.
    let (port, server) = stub_clash_server(
        vec![
            traffic_body(&alpha.id, 1000, 50),
            traffic_body(&alpha.id, 1500, 20),
        ],
        "s3cret",
    );

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (mut engine, _) = EngineController::fake();
            engine.set_clash_targets(vec![ClashTarget {
                core: "stock".to_string(),
                port,
                secret: "s3cret".to_string().into(),
            }]);
            view.engine = engine;
        });
    });

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 1, "first poll persists the fresh counters");

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 1, "shrunken upload falls back without underflow");

    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(
        totals.get(&alpha.id),
        Some(&(2500, 70)),
        "rx accumulated, tx fell back to a fresh count"
    );
    server.join().expect("stub drains");
}

#[gpui_kit::test]
fn t47_spawn_traffic_poll_off_ui_and_skips_in_flight(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10858"))
        .expect("seed works");

    let (port, server) = stub_clash_server(vec![traffic_body(&alpha.id, 1000, 50)], "s3cret");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (mut engine, _) = EngineController::fake();
            engine.set_clash_targets(vec![ClashTarget {
                core: "stock".to_string(),
                port,
                secret: "s3cret".to_string().into(),
            }]);
            view.engine = engine;
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, _| view.traffic_tick_in_flight = true);
    });
    let skipped = cx.update(|_, cx| view.update(cx, |view, cx| view.spawn_traffic_poll(cx)));
    assert_eq!(skipped, 0, "in-flight tick skipped");
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.traffic_tick_in_flight = false);
    });

    let started = cx.update(|_, cx| view.update(cx, |view, cx| view.spawn_traffic_poll(cx)));
    assert_eq!(started, 1, "poll started off-thread");
    let dup = cx.update(|_, cx| view.update(cx, |view, cx| view.spawn_traffic_poll(cx)));
    assert_eq!(dup, 0, "second tick skipped while one runs");

    cx.run_until_parked();
    let totals = view.read_with(cx, |view, _| {
        view.store.traffic_totals().expect("totals read")
    });
    assert_eq!(
        totals.get(&alpha.id),
        Some(&(1000, 50)),
        "probe bytes never reach the traffic columns"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.health_store.data_traffic(&alpha.id)),
        Some((1000, 50)),
        "mirror tracks persisted absolutes"
    );
    assert!(
        !view.read_with(cx, |view, _| view.traffic_tick_in_flight),
        "flag clears on completion"
    );
    server.join().expect("stub drains");
}

#[gpui_kit::test]
fn t05_connections_snapshot_excludes_probes_and_tracks_deltas(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10856"))
        .expect("seed works");
    let beta = store.add(tick12_seed("Beta", "10857")).expect("seed works");

    let (port, server) = stub_clash_server(
        vec![
            traffic_body_multi(&[(&alpha.id, 1000, 50), (&beta.id, 3000, 90)]),
            traffic_body_multi(&[(&alpha.id, 1500, 70), (&beta.id, 3600, 120)]),
        ],
        "s3cret",
    );
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (mut engine, _) = EngineController::fake();
            engine.set_clash_targets(vec![ClashTarget {
                core: "stock".to_string(),
                port,
                secret: "s3cret".to_string().into(),
            }]);
            view.engine = engine;
        });
    });

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 2, "both exits gain data-plane bytes");

    let snapshot = view.read_with(cx, |view, _| view.conn.snapshot.clone());
    assert_eq!(snapshot.len(), 2, "probe flow never reaches the snapshot");
    for (_, entry) in &snapshot {
        assert!(
            !entry.dest.contains("gstatic"),
            "no probe dest in the snapshot: {}",
            entry.dest
        );
    }

    let (first, second) = view.read_with(cx, |view, _| {
        let rows = manyproxy_ui::build_conn_rows(
            &view.conn.snapshot,
            &view.conn.delta,
            &std::collections::HashSet::new(),
            false,
            true,
        );
        (rows[0].row_id.clone(), rows[1].row_id.clone())
    });
    assert_eq!(first.as_deref(), Some(beta.id.as_str()), "heaviest first");
    assert_eq!(second.as_deref(), Some(alpha.id.as_str()));

    let (join, unclaimed) = view.read_with(cx, |view, cx| {
        let rows = view.libraries.read(cx).delegate().rows.clone();
        (
            manyproxy_ui::conn_exit_of(&rows, Some(&alpha.id)),
            manyproxy_ui::conn_exit_of(&rows, Some("no-such-row")),
        )
    });
    assert_eq!(join.0, "Alpha", "exit name joins");
    assert!(
        join.1.ends_with(":10856"),
        "exit bind:port joins: {}",
        join.1
    );
    assert_eq!(
        join.2, "vless",
        "row protocol joins (stored, never sniffed)"
    );
    assert_eq!(
        unclaimed,
        ("unclaimed".to_string(), "—".to_string(), "—".to_string()),
        "unclaimed rows read unclaimed"
    );

    let updated = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(updated, 2, "grown connections persist the delta");
    let deltas = view.read_with(cx, |view, _| view.conn.delta.clone());
    assert_eq!(deltas.get("stock/conn-0"), Some(&(500, 20)));
    assert_eq!(deltas.get("stock/conn-1"), Some(&(600, 30)));
    let throttled = cx.update(|_, cx| view.update(cx, |view, cx| view.spawn_traffic_poll(cx)));
    assert_eq!(
        throttled, 0,
        "hidden Connections pane throttles the 1Hz loop to the base cadence"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.console.open = true;
            view.console.tab = manyproxy_ui::ConsoleTab::Connections;
            view.console.scope = manyproxy_ui::ConsoleScope::All;
            view.select_visible_row(0, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "console-connections");
    assert!(
        cx.update(|window, _| window.try_find("console-conn-copy-0".to_string()).is_some()),
        "first flow copies"
    );
    assert!(
        cx.update(|window, _| window.try_find("console-conn-copy-1".to_string()).is_some()),
        "second flow copies"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.console.scope = manyproxy_ui::ConsoleScope::Selected;
            view.select_visible_row(1, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, _| window.try_find("console-conn-copy-0".to_string()).is_some()),
        "selected row keeps its flow"
    );
    assert_absent(cx, "console-conn-copy-1".to_string());

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.copy_connection_dest("flow-1.example.com", window, cx)
        });
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(clip, "flow-1.example.com", "click copies the dest: {clip}");
    for canary in ["11111111", "198.51.100.9", "uuid", "password"] {
        assert!(
            !clip.contains(canary),
            "connection copy carries the dest only, found {canary}: {clip}"
        );
    }
    server.join().expect("stub drains");
}

#[gpui_kit::test]
fn t74_click_dest_copies_exact_string(cx: &mut TestAppContext) {
    let dest = "flow-0.example.com";
    let (_view, cx, _endpoint) = t74_conn_setup(cx, dest);
    assert_painted(cx, "console-connections");
    assert_painted(cx, "console-conn-copy-0");

    cx.update(|window, cx| {
        window.click("console-conn-copy-0", cx);
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(
        clip, dest,
        "dest cell copies the displayed dest string exactly: {clip}"
    );
    for canary in [
        "11111111",
        "198.51.100.9",
        "uuid",
        "password",
        "server_port",
    ] {
        assert!(
            !clip.contains(canary),
            "dest copy carries the display value only, found {canary}: {clip}"
        );
    }

    // Repeat click copies again (idempotent; no state jam).
    cx.update(|window, cx| {
        window.click("console-conn-copy-0", cx);
    });
    let clip_again = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(
        clip_again, dest,
        "repeat click re-copies the dest: {clip_again}"
    );
}

#[gpui_kit::test]
fn t74_click_endpoint_copies_endpoint_only(cx: &mut TestAppContext) {
    let dest = "flow-0.example.com";
    let (_view, cx, endpoint) = t74_conn_setup(cx, dest);
    assert_painted(cx, "console-conn-endpoint-0");

    cx.update(|window, cx| {
        window.click("console-conn-endpoint-0", cx);
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert_eq!(
        clip, endpoint,
        "endpoint cell copies the row's bind:port contact only: {clip}"
    );
    assert!(
        clip.contains(':'),
        "endpoint stays bind:port shaped: {clip}"
    );
    assert_ne!(clip, dest, "endpoint copy never carries the destination");
    for canary in [dest, "11111111", "198.51.100.9", "uuid", "password"] {
        assert!(
            !clip.contains(canary),
            "endpoint copy excludes every other value, found {canary}: {clip}"
        );
    }
}

#[gpui_kit::test]
fn t74_copied_tooltip_transient_clears(cx: &mut TestAppContext) {
    let (_view, cx, _endpoint) = t74_conn_setup(cx, "flow-0.example.com");
    assert_eq!(
        AppView::CONN_COPIED_TOOLTIP_MS,
        1500,
        "the Copied! marker window is ~1.5s"
    );
    assert_absent(cx, "console-conn-copied-dest-0".to_string());

    cx.update(|window, cx| {
        window.click("console-conn-copy-0", cx);
    });
    t74_paint(cx);
    assert_painted(cx, "console-conn-copied-dest-0");

    // Still painted just before the window elapses; a tick runs no timer yet.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(
            AppView::CONN_COPIED_TOOLTIP_MS - 200,
        ));
    let _ = cx.executor().tick();
    t74_paint(cx);
    assert_painted(cx, "console-conn-copied-dest-0");

    // Past the window it auto-dismisses with no further interaction.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    t74_paint(cx);
    assert_absent(cx, "console-conn-copied-dest-0".to_string());
}

#[gpui_kit::test]
fn t74_other_cells_not_click_copyable(cx: &mut TestAppContext) {
    let (_view, cx, _endpoint) = t74_conn_setup(cx, "flow-0.example.com");
    assert_painted(cx, "console-conn-copy-0");
    assert_painted(cx, "console-conn-endpoint-0");
    assert_painted(cx, "console-conn-meta-0");

    // The name · protocol · traffic cell has no copy handler: clicking it
    // leaves the clipboard untouched and lights no marker.
    let meta = cx
        .update(|window, _| window.try_find("console-conn-meta-0".to_string()))
        .expect("meta cell renders");
    let bounds = meta.bounds();
    cx.update(|window, cx| {
        window.click_at(
            "console-conn-meta-0",
            gpui_kit::point(px(2.), bounds.size.height * 0.5),
            cx,
        );
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert!(
        clip.is_empty(),
        "clicking a non-target cell copies nothing: {clip}"
    );
    assert_absent(cx, "console-conn-copied-dest-0".to_string());
    assert_absent(cx, "console-conn-copied-endpoint-0".to_string());

    // A dest click lights only the dest marker, never the endpoint's.
    cx.update(|window, cx| {
        window.click("console-conn-copy-0", cx);
    });
    t74_paint(cx);
    assert_painted(cx, "console-conn-copied-dest-0");
    assert_absent(cx, "console-conn-copied-endpoint-0".to_string());
}

/// T-25/K12: a fake engine serves one row (no binaries, no network) while the
/// poll-result merge seeds a two-core snapshot. The merge still counts; the
/// status strip no longer renders it — the Connections tab owns that number.
#[gpui_kit::test]
fn t75_status_bar_keeps_connections_in_the_inspector(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{ClashTarget, EngineController};
    use manyproxy_ui::health::{ClashConn, ProbeExclusions};

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10971"))
        .expect("seed works");
    let row_id = alpha.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _handle) = EngineController::fake();
            view.engine = engine;
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_all(window, cx));
    });
    assert!(
        view.read_with(cx, |view, _| view.engine.running_count() > 0),
        "fake core runs without a sidecar"
    );

    let conn = |id: &str, network: &str| ClashConn {
        id: id.to_string(),
        row_id: Some(row_id.clone()),
        rx_bytes: 100,
        tx_bytes: 10,
        dest: "flow.example.com".to_string(),
        network: network.to_string(),
    };
    let results = vec![
        (
            ClashTarget {
                core: "stock".to_string(),
                port: 19090,
                secret: "s3cret".to_string().into(),
            },
            Ok(vec![conn("a", "tcp"), conn("b", "udp")]),
        ),
        (
            ClashTarget {
                core: "fork".to_string(),
                port: 19091,
                secret: "s3cret".to_string().into(),
            },
            Ok(vec![conn("c", "tcp"), conn("d", "")]),
        ),
    ];
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_traffic_results(results, &ProbeExclusions::new(), cx);
        });
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.conn_counter_text()),
        Some("4 connections · 2 TCP · 1 UDP".to_string()),
        "counts merge both core snapshots; blank network stays active-only"
    );
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert_absent(cx, "conn-counters".to_string());
    assert_painted(cx, "engine-bar");

    // The region's min-width/truncate/tooltip pattern keeps the segment in
    // the bar at the minimum frame instead of dropping it.
    cx.simulate_resize(size(px(1100.), px(640.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    let bounds = cx
        .update(|window, _| window.try_find("engine-bar-status".to_string()))
        .expect("the engine segment survives the minimum width")
        .bounds();
    assert!(
        bounds.size.width > px(0.),
        "narrow window truncates, never zero-width: {bounds:?}"
    );
    assert!(
        bounds.origin.x + bounds.size.width <= px(1100.) + px(1.),
        "the engine segment stays inside the minimum frame: {bounds:?}"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_traffic_results(Vec::new(), &ProbeExclusions::new(), cx);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.conn_counter_text()),
        Some("0 connections · 0 TCP · 0 UDP".to_string()),
        "running but idle still reports zeros"
    );
}

#[gpui_kit::test]
fn t75_counter_hidden_when_no_running_cores(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{ClashConn, ConnCounts};

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Alpha", "10972"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            // A stale snapshot must not leak numbers once every core stops.
            view.conn.snapshot = vec![(
                "stock".to_string(),
                ClashConn {
                    id: "conn-0".to_string(),
                    row_id: None,
                    rx_bytes: 0,
                    tx_bytes: 0,
                    dest: "flow.example.com".to_string(),
                    network: "tcp".to_string(),
                },
            )];
            view.conn.counts = ConnCounts {
                active: 1,
                tcp: 1,
                udp: 0,
            };
            cx.notify();
        });
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.conn_counter_text()),
        None,
        "no running cores = no counter line"
    );
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert_absent(cx, "conn-counters".to_string());
    assert_painted(cx, "engine-bar");
}

#[test]
fn t76_running_title_suffix_shapes() {
    use manyproxy_ui::{running_title_suffix, window_title_text, APP_TITLE};

    assert_eq!(running_title_suffix(0), "", "idle contributes no suffix");
    assert_eq!(running_title_suffix(1), " · 1 running");
    assert_eq!(running_title_suffix(12), " · 12 running");

    assert_eq!(window_title_text(0), APP_TITLE);
    assert_eq!(window_title_text(0), "ManyProxy");
    assert_eq!(window_title_text(3), "ManyProxy · 3 running");

    // Count only: no preset marker, no `[Admin]`-family token, no extra text.
    for text in [window_title_text(0), window_title_text(7)] {
        assert!(
            !text.contains('[') && !text.contains(']'),
            "no token: {text}"
        );
        assert!(
            !text.to_lowercase().contains("preset"),
            "no preset marker: {text}"
        );
        assert!(
            text.starts_with(APP_TITLE),
            "always starts with the base title: {text}"
        );
        assert_eq!(
            text.matches('·').count(),
            usize::from(text != APP_TITLE),
            "exactly one separator when running, none at zero: {text}"
        );
    }
}

/// T-26: the label the custom titlebar renders and the OS title both come
/// from `window_title_text`, fed by the same `ENGINE_POLL_TICK_SECS`
/// (`poll_shards`-driven) count the tray tooltip is fed from.
#[gpui_kit::test]
fn t76_titlebar_label_follows_running_count(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Alpha", "10981"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _handle) = EngineController::fake();
            view.engine = engine;
        });
    });

    let label_text = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            manyproxy_ui::window_title_text(view.engine.running_count())
        })
    };
    assert_eq!(label_text(cx), "ManyProxy", "zero running renders plain");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_all(window, cx));
    });
    let running = view.read_with(cx, |view, _| view.engine.running_count());
    assert!(running > 0, "fake core runs without a sidecar");
    assert_eq!(
        label_text(cx),
        format!("ManyProxy · {running} running"),
        "the count reaches the label text"
    );

    // The frame that draws the composed label renders at that title.
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert_painted(cx, "main-content");
    assert_eq!(label_text(cx), format!("ManyProxy · {running} running"));

    cx.update(|_, cx| view.update(cx, |view, _| view.engine.stop_all()));
    assert_eq!(
        label_text(cx),
        "ManyProxy",
        "stopping all clears the marker"
    );
}

/// T-26: one helper feeds both surfaces, so the tray tooltip and the
/// titlebar label carry the same text at zero and while running.
#[gpui_kit::test]
fn t76_tray_tooltip_stays_in_sync(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::tray::tray_tooltip;

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(t30_engine_seed("Alpha", "10982"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _handle) = EngineController::fake();
            view.engine = engine;
        });
    });

    let surfaces = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let running = view.engine.running_count();
            (running, manyproxy_ui::window_title_text(running))
        })
    };
    let (running, title) = surfaces(cx);
    assert_eq!(running, 0);
    assert_eq!(title, manyproxy_ui::APP_TITLE);
    assert_eq!(tray_tooltip(running), title, "idle surfaces agree");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_start_all(window, cx));
    });
    let (running, title) = surfaces(cx);
    assert!(running > 0, "fake core runs without a sidecar");
    assert_eq!(
        tray_tooltip(running),
        title,
        "running surfaces carry the same text"
    );
    assert_eq!(title, format!("ManyProxy · {running} running"));
}

#[gpui_kit::test]
fn t05_traffic_aggregates_per_exit_without_direct(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10856"))
        .expect("seed works");
    let beta = store.add(tick12_seed("Beta", "10857")).expect("seed works");
    store
        .add_traffic_batch(&[(alpha.id.clone(), 1000, 50), (beta.id.clone(), 3000, 90)])
        .expect("traffic persists");
    let rows = store.list().expect("rows list");
    let totals = store.traffic_totals().expect("totals read");
    let exits = manyproxy_ui::aggregate_exit_traffic(&rows, &totals);
    assert_eq!(
        exits.len(),
        2,
        "one aggregate per exit with traffic — never a Direct line"
    );
    assert_eq!(exits[0].row_id, beta.id, "heaviest exit first");
    assert_eq!(exits[1].row_id, alpha.id);
    for exit in &exits {
        assert_ne!(
            exit.name.to_ascii_lowercase(),
            "direct",
            "no fabricated Direct aggregate"
        );
    }

    store
        .add(tick12_seed("Gamma", "10858"))
        .expect("seed works");
    let rows = store.list().expect("rows list");
    let totals = store.traffic_totals().expect("totals read");
    assert_eq!(
        manyproxy_ui::aggregate_exit_traffic(&rows, &totals).len(),
        2,
        "idle rows stay quiet"
    );
}

#[gpui_kit::test]
fn t05_log_include_exclude_regex_keep_redaction(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let tags = manyproxy_ui::health::LogTagIndex::default();
            view.logs
                .store
                .push_raw("info alpha connection established", &tags);
            view.logs.store.push_raw("info alpha probe finished", &tags);
            view.logs
                .store
                .push_raw("error beta failed password=hunter2-secret", &tags);
            view.logs.tail = true;
        });
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.logs.filter = "alpha".to_string();
            view.logs.exclude.clear();
            view.logs.regex.clear();
            view.logs.level = None;
            assert_eq!(view.log_lines(cx).len(), 2, "include narrows");
            view.logs.exclude = "probe".to_string();
            assert_eq!(
                view.log_lines(cx),
                vec!["[info] info alpha connection established".to_string()],
                "exclude drops the noisy leg"
            );
            view.logs.filter.clear();
            view.logs.exclude.clear();
            view.logs.regex = "established|failed".to_string();
            let visible = view.log_lines(cx);
            assert_eq!(visible.len(), 2, "regex replaces the keywords");
            for line in &visible {
                assert!(
                    !line.contains("hunter2-secret"),
                    "redacted on the regex leg: {line}"
                );
            }
            view.logs.regex = "((".to_string();
            assert_eq!(
                view.log_lines(cx).len(),
                3,
                "invalid regex fails open, never a silent empty"
            );
            view.logs.regex.clear();
            view.logs.level = Some(manyproxy_ui::health::LogLevel::Error);
            let visible = view.log_lines(cx);
            assert_eq!(visible.len(), 1, "level still composes");
            assert!(visible[0].starts_with("[error]"));
            assert!(
                !visible[0].contains("hunter2-secret"),
                "redacted on the level leg"
            );
        });
    });
}

#[gpui_kit::test]
fn t05_traffic_poll_cadence_visible_fast_hidden_slow(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    use manyproxy_ui::ConsoleTab;
    let now = 1_000_000i64;
    assert!(
        manyproxy_ui::traffic_poll_due(true, ConsoleTab::Connections, now, now),
        "an open Connections tab polls immediately"
    );
    assert!(
        manyproxy_ui::traffic_poll_due(true, ConsoleTab::Traffic, now, now),
        "an open Traffic tab polls immediately"
    );
    assert!(
        !manyproxy_ui::traffic_poll_due(true, ConsoleTab::Logs, now, now),
        "Logs waits a just-ran poll"
    );
    assert!(
        !manyproxy_ui::traffic_poll_due(false, ConsoleTab::Logs, now, now - 4),
        "a closed console waits the base cadence"
    );
    assert!(
        manyproxy_ui::traffic_poll_due(false, ConsoleTab::Logs, now, now - 5),
        "a closed console fires once the base cadence elapses"
    );
}

#[gpui_kit::test]
fn restart_restores_traffic_mirror(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let path = std::env::temp_dir().join(format!(
        "manyproxy-tick36-restart-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let alpha_id = {
        let mut store = Store::open_file(&path).expect("file store opens");
        let alpha = store
            .add(tick12_seed("Alpha", "10857"))
            .expect("seed works");
        store
            .add_traffic(&alpha.id, 1_048_782, 88)
            .expect("traffic persists");
        alpha.id.clone()
    };
    let store = Store::open_file(&path).expect("reopen works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let restarted = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(restarted.clone());
        Root::new(restarted, window, cx)
    });
    let restarted = stash.borrow().clone().expect("AppView stashed");

    assert_eq!(
        restarted.read_with(cx, |view, _| view.health_store.data_traffic(&alpha_id)),
        None,
        "lazy totals: no mirror before the first tick"
    );
    let ticked = cx.update(|_, cx| restarted.update(cx, |view, cx| view.tick_traffic_poll(cx)));
    assert_eq!(ticked, 0, "no targets, no rows updated");
    assert_eq!(
        restarted.read_with(cx, |view, _| view.health_store.data_traffic(&alpha_id)),
        Some((1_048_782, 88)),
        "counters resume from persisted totals on first tick"
    );

    cx.update(|_, cx| {
        restarted.update(cx, |view, _| {
            view.store = Store::open_in_memory().expect("swap works");
        });
    });
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let sibling = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        let _ = std::fs::remove_file(&sibling);
    }
    assert!(!path.exists(), "restart temp db removed");
}

#[gpui_kit::test]
fn live_sidecar_lines_appear_under_right_config_redacted(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::{inbound_tag, EngineController};

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store.add(seed_input("Work")).expect("seed works");
    let beta = store.add(seed_input("Play")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let stock = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (controller, stock, _fork, _xray, _extra) = EngineController::fake_quartet();
            view.engine = controller;
            stock
        })
    });
    stock.emit_log(&format!(
        "warn {}: dial failed password=hunter2",
        inbound_tag(&alpha.id)
    ));
    stock.emit_log("info engine listening on 127.0.0.1:10801");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let rows = view.store.list().expect("seeded rows list");
            view.drain_engine_logs(&rows);
        });
    });

    let (alpha_lines, beta_lines) = view.read_with(cx, |view, _| {
        (
            view.logs
                .store
                .for_config(&alpha.id)
                .iter()
                .map(|record| record.message.clone())
                .collect::<Vec<_>>(),
            view.logs.store.for_config(&beta.id).len(),
        )
    });
    assert_eq!(alpha_lines.len(), 1, "only its lines show");
    assert!(
        !alpha_lines[0].contains("hunter2"),
        "redacted on ingest: {}",
        alpha_lines[0]
    );
    assert!(
        alpha_lines[0].contains(&inbound_tag(&alpha.id)),
        "tag preserved: {}",
        alpha_lines[0]
    );
    assert_eq!(beta_lines, 0, "no cross-config attribution");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            view.reload(window, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
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

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("log-lines").is_some()));
}

#[gpui_kit::test]
fn subscriptions_dialog_opens_with_fields(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
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

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("field-sub-label").is_some()));
    assert!(cx.update(|window, _| window.try_find("field-sub-url").is_some()));
    assert!(cx.update(|window, _| window.try_find("field-sub-group").is_some()));
}

#[gpui_kit::test]
fn engine_start_all_and_stop_all_via_fake(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::store::ConfigPatch;

    cx.update(gpui_kit::init);

    fn engine_seed(name: &str, port: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: port.to_string(),
        }
    }

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(engine_seed("Alpha", "10831"))
        .expect("seed works");
    let beta = store.add(engine_seed("Beta", "10832")).expect("seed works");
    let _gamma = store
        .add(engine_seed("Gamma", "10833"))
        .expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let handle = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, handle) = EngineController::fake();
            view.engine = engine;
            handle
        })
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 3);
    assert!(view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)));
    let recorded: serde_json::Value =
        serde_json::from_str(&handle.last_config().expect("mock recorded the config"))
            .expect("generated config is JSON");
    assert_eq!(recorded["inbounds"].as_array().expect("inbounds").len(), 3);
    assert_eq!(
        recorded["route"]["rules"].as_array().expect("rules").len(),
        3
    );
    assert_eq!(recorded["route"]["final"], serde_json::json!("direct"));

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, _| window.try_find("engine-bar-status").is_some()));

    assert!(cx.update(|window, _| window.try_find("engine-bar-start-all").is_some()));
    assert!(cx.update(|window, _| window.try_find("engine-bar-stop-all").is_some()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_stop_all(window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 0);
    assert!(!handle.is_running());

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let _ = view.store.update(
                &alpha.id,
                ConfigPatch {
                    port: Some("10901".to_string()),
                    ..Default::default()
                },
            );
            let _ = view.store.update(
                &beta.id,
                ConfigPatch {
                    port: Some("10901".to_string()),
                    ..Default::default()
                },
            );
            view.start_all_engines(window, cx);
        });
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 0);
    let error = view.read_with(cx, |view, _| {
        view.engine.last_error().unwrap_or("").to_string()
    });
    assert!(
        error.contains("Alpha") && error.contains("Beta"),
        "conflict names both configs, got {error}"
    );
}

#[gpui_kit::test]
fn health_test_all_via_fake_flags_dead(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    fn health_seed(name: &str, port: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: port.to_string(),
        }
    }

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(health_seed("Alpha", "10841"))
        .expect("seed works");
    let beta = store.add(health_seed("Beta", "10842")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, handle) = FakeHealthProber::shared();
            handle.script_latency(&alpha.id, 33);
            handle.script_failure(&beta.id, "upstream refused");
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    let tested =
        cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
    assert_eq!(tested, 2);

    cx.run_until_parked();

    let (alpha_live, beta_dead) = view.read_with(cx, |view, _| {
        (
            view.health_store.status(&alpha.id),
            view.health_store.status(&beta.id),
        )
    });
    assert_eq!(alpha_live.latency_ms, Some(33));
    assert!(
        beta_dead
            .cause
            .as_deref()
            .unwrap_or("")
            .contains("upstream refused"),
        "dead row names cause: {:?}",
        beta_dead.cause
    );
    assert_eq!(view.read_with(cx, |view, _| view.health_counts()), (1, 1));

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, _| window.try_find("engine-bar-overflow").is_some()));
    assert!(cx.update(|window, _| window.try_find("engine-bar-start-all").is_some()));
    assert!(cx.update(|window, _| window.try_find("engine-bar-status").is_some()));
}

#[gpui_kit::test]
fn logs_dialog_filters_to_selected_and_redacts(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::inbound_tag;

    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let config = store.add(seed_input("Work")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            let rows = vec![config.clone()];
            let tags = manyproxy_ui::health::LogTagIndex::build(&rows);
            view.logs.store.push_raw(
                &format!(
                    "warn {}: dial failed password=hunter2",
                    inbound_tag(&config.id)
                ),
                &tags,
            );
            view.logs.store.push_raw("info engine started", &tags);
        });
    });

    let lines = view.read_with(cx, |view, _cx| {
        view.logs
            .store
            .for_config(&config.id)
            .iter()
            .map(|record| record.message.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(lines.len(), 1, "only its lines show");
    assert!(!lines[0].contains("hunter2"), "redacted: {}", lines[0]);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
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

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("field-log-filter").is_some()));
    assert!(cx.update(|window, _| window.try_find("log-lines").is_some()));
}

#[test]
fn subscription_label_strips_credentials_and_query() {
    use manyproxy_ui::subscription_label_from_url;
    assert_eq!(
        subscription_label_from_url("https://example.com/sub?id=42"),
        "example.com"
    );
    let labeled =
        subscription_label_from_url("https://user:hunter2@example.com:8443/sub?token=abc");
    assert!(!labeled.contains("hunter2"), "no password: {labeled}");
    assert!(!labeled.contains("token=abc"), "no query: {labeled}");
    assert!(labeled.contains("example.com"), "host kept: {labeled}");
    assert_eq!(subscription_label_from_url(""), "Subscription");
}

#[test]
fn copy_urls_carry_binds_not_secrets() {
    use manyproxy_ui::store::proxy_urls;
    let (socks, http) = proxy_urls("127.0.0.1", 10801);
    assert_eq!(socks, "socks5://127.0.0.1:10801");
    assert_eq!(http, "http://127.0.0.1:10801");
    for canary in ["hunter2", "LOCAL-PRIVATE", "SECRET-UUID", "token=abc"] {
        assert!(!socks.contains(canary));
        assert!(!http.contains(canary));
    }
}

#[gpui_kit::test]
fn lan_single_start_opens_confirm_and_stays_stopped(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_lan_input("Lan")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

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

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts before confirm"
    );
}

#[gpui_kit::test]
fn lan_start_all_opens_confirm_and_stays_stopped(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Loop")).expect("seed works");
    store.add(seed_lan_input("Lan")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts before confirm"
    );
}

#[gpui_kit::test]
fn loopback_single_start_skips_confirm(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Loop")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            view.start_selected_engine(window, cx);
        });
    });
    cx.run_until_parked();

    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[test]
fn redact_subscription_url_strips_userinfo() {
    use manyproxy_ui::redact_subscription_url;
    let shown = redact_subscription_url("https://user:hunter2@example.com:8443/sub?token=abc#frag");
    assert!(!shown.contains("hunter2"), "no password: {shown}");
    assert!(!shown.contains("user:"), "no userinfo: {shown}");
    assert!(!shown.contains("token=abc"), "no query: {shown}");
    assert!(!shown.contains("#frag"), "no fragment: {shown}");
    assert_eq!(shown, "https://example.com:8443/sub");
    assert_eq!(
        redact_subscription_url("https://example.com/sub"),
        "https://example.com/sub"
    );
    let long = format!("https://user:pw@example.com/{}", "x".repeat(200));
    let shown = redact_subscription_url(&long);
    assert!(
        !shown.contains("user:pw"),
        "stripped before truncate: {shown}"
    );
    assert!(shown.ends_with('…'), "still truncated: {shown}");
}

#[test]
fn addsub_error_message_surfaces_original_reason() {
    use manyproxy_ui::addsub_error_message;
    use manyproxy_ui::import::parse_addsub_link;
    let bad_b64 = parse_addsub_link("throne://addsub/!!!").expect_err("bad base64 fails");
    let text = addsub_error_message(&bad_b64);
    assert!(
        text.contains("bad-base64"),
        "original kind kept, got {text}"
    );
    assert!(
        !text.contains("unknown throne command"),
        "not generic: {text}"
    );
    let bad_host = parse_addsub_link("throne://addsub/aGk").expect_err("non-http fails");
    let text = addsub_error_message(&bad_host);
    assert!(text.contains("bad-host"), "original kind kept, got {text}");
}

#[test]
fn t45_interval_and_last_refresh_format() {
    use manyproxy_ui::{format_interval_secs, format_last_refresh, INTERVAL_PRESETS};
    assert_eq!(format_interval_secs(0), "always");
    assert_eq!(format_interval_secs(-5), "always");
    assert_eq!(format_interval_secs(3_600), "1h");
    assert_eq!(format_interval_secs(86_400), "24h");
    assert_eq!(format_interval_secs(604_800), "7d");
    assert!(INTERVAL_PRESETS.contains(&86_400));
    assert_eq!(format_last_refresh(None, 1_700_000_000), "never refreshed");
    assert_eq!(
        format_last_refresh(Some(1_699_999_940), 1_700_000_000),
        "1m ago"
    );
    assert_eq!(
        format_last_refresh(Some(1_699_996_400), 1_700_000_000),
        "1h ago"
    );
    assert_eq!(
        format_last_refresh(Some(1_699_913_600), 1_700_000_000),
        "1d ago"
    );
}

#[gpui_kit::test]
fn t45_kill_switch_gates_auto_not_manual(cx: &mut TestAppContext) {
    use manyproxy_ui::store::now_unix;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let due = store
        .add_subscription("Due", "https://example.com/due.txt", "g", true)
        .expect("seed works");
    let _fresh = store
        .add_subscription("Fresh", "https://example.com/fresh.txt", "g", true)
        .expect("seed works");

    let now = now_unix();
    store
        .refresh_with_body(
            &_fresh.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
            now,
        )
        .expect("refresh stamps");
    let (view, cx) = mount_view(store, cx);
    let (auto, manual) = view.read_with(cx, |view, _| {
        (
            view.due_subscriptions()
                .iter()
                .map(|sub| sub.id.clone())
                .collect::<Vec<_>>(),
            view.due_for_manual_refresh()
                .iter()
                .map(|sub| sub.id.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(auto, vec![due.id.clone()]);
    assert_eq!(manual, vec![due.id.clone()]);

    view.update(cx, |view, _| {
        view.store
            .set_subs_auto_refresh_enabled(false)
            .expect("kill writes");
    });
    let (auto, manual) = view.read_with(cx, |view, _| {
        (
            view.due_subscriptions().len(),
            view.due_for_manual_refresh()
                .iter()
                .map(|sub| sub.id.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(auto, 0, "kill-switch gates the automatic pass");
    assert_eq!(manual, vec![due.id.clone()], "manual still lists due");
}

#[gpui_kit::test]
fn t45_usage_reset_zeroes_and_totals_sum(cx: &mut TestAppContext) {
    use manyproxy_ui::store::NewConfig;
    use manyproxy_ui::summed_traffic_text;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(NewConfig {
            name: "R".to_string(),
            protocol: "vless".to_string(),
            params: Secret::default(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("row seeds");
    store
        .add_traffic_batch(&[(row.id.clone(), 2_048, 1_024)])
        .expect("traffic accrues");
    let (view, cx) = mount_view(store, cx);
    let before = view.read_with(cx, |view, _| summed_traffic_text(&view.store));
    assert!(before.is_some(), "totals render while rows exist");
    let text = before.expect("totals render");
    assert!(text.contains("2.0 KB"), "down sums: {text}");
    assert!(text.contains("1.0 KB"), "up sums: {text}");
    let row_id = row.id.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_usage_reset(&row_id, window, cx));
    });
    let after = view.read_with(cx, |view, _| {
        (
            view.store
                .traffic_totals()
                .expect("totals read")
                .get(&row_id)
                .copied(),
            summed_traffic_text(&view.store),
        )
    });
    assert_eq!(after.0, Some((0, 0)), "counters zeroed");
    assert_eq!(notification_count(cx), 1, "reset notifies");
}

#[gpui_kit::test]
fn t45_logs_save_matches_visible(cx: &mut TestAppContext) {
    use manyproxy_ui::health::LogLevel;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw(
                "error boom one",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
            view.logs.store.push_raw(
                "info chatter",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
            view.logs.store.push_raw(
                "warn boom two",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
            view.logs.tail = true;
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.logs.level = Some(LogLevel::Error);
            let visible = view.log_lines(cx);
            assert_eq!(visible, vec!["[error] error boom one".to_string()]);
        });
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.logs.level = None;
            view.logs.tail = false;
            view.logs.filter = "boom".to_string();
            let visible = view.log_lines(cx);
            assert_eq!(
                visible,
                vec![
                    "[error] error boom one".to_string(),
                    "[warn] warn boom two".to_string(),
                ]
            );
            let dir =
                std::env::temp_dir().join(format!("manyproxy-45-logs-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("temp dir creates");
            let path = dir.join("visible.txt");
            view.write_visible_logs_to(&path, cx).expect("save works");
            let saved = std::fs::read_to_string(&path).expect("saved file reads");
            assert_eq!(saved, visible.join("\n"), "file matches visible lines");
            std::fs::remove_dir_all(&dir).expect("temp dir cleans");
        });
    });
}

#[gpui_kit::test]
fn laneb_logs_single_level_control(cx: &mut TestAppContext) {
    use manyproxy_ui::health::LogLevel;
    cx.update(gpui_kit::init);

    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "log-level-row");
    assert!(
        cx.update(|window, _| window.try_find("logs-level".to_string()).is_none()),
        "no second cycle button beside Copy/tail/Save"
    );
    cx.update(|window, cx| {
        window.within("log-level-row").click(4_usize, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.level),
        Some(LogLevel::Error),
        "level row narrows the filter"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn import_commit_all_invalid_stays_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let body =
        "vless://not-a-uuid@b.example.com:notaport#bad\nbogus://x\ntrojan://pw@c.example.com:99999";
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(body.to_string(), "g".to_string(), None, window, cx)
        })
    });
    assert!(!closed, "total failure keeps the dialog open");
    assert_eq!(notification_count(cx), 1, "all failures in one notice");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );
}

#[gpui_kit::test]
fn import_commit_bad_addsub_stays_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(
                "throne://addsub/!!!".to_string(),
                "g".to_string(),
                None,
                window,
                cx,
            )
        })
    });
    assert!(!closed, "bad addsub keeps the dialog open");
    assert_eq!(notification_count(cx), 1);
}

#[gpui_kit::test]
fn import_commit_generic_single_invalid_stays_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import("bogus://x".to_string(), "g".to_string(), None, window, cx)
        })
    });
    assert!(!closed, "all-invalid single line stays open too");
    assert_eq!(notification_count(cx), 1);
}

#[gpui_kit::test]
fn db_error_on_toggle_notifies(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            view.store
                .set_query_only_for_test()
                .expect("readonly works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_selected_enabled(window, cx));
    });
    assert_eq!(notification_count(cx), 1, "toggle failure notifies");
    let enabled = view.read_with(cx, |view, cx| {
        view.selected_config(cx).map(|row| row.enabled)
    });
    assert_eq!(enabled, Some(false), "failed toggle changes nothing");
}

#[gpui_kit::test]
fn db_error_on_reload_notifies(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.break_schema_for_test().expect("break works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reload(window, cx));
    });
    assert!(notification_count(cx) >= 1, "list failure notifies");
}

#[gpui_kit::test]
fn db_error_on_subscription_refresh_notifies(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.break_schema_for_test().expect("break works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.refresh_subscription_by_id(&sub_id, window, cx)
        });
    });
    assert_eq!(notification_count(cx), 1, "refresh failure notifies");
}

#[gpui_kit::test]
fn refresh_unknown_subscription_warns_without_fetch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.refresh_subscription_by_id("999", window, cx)
        });
    });
    assert_eq!(notification_count(cx), 1, "gone sub warns, no fetch");
}

#[gpui_kit::test]
fn db_error_on_subscription_auto_toggle_notifies(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.break_schema_for_test().expect("break works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_subscription_auto(&sub_id, window, cx)
        });
    });
    assert_eq!(notification_count(cx), 1, "auto-toggle failure notifies");
}

#[gpui_kit::test]
fn remove_subscription_db_error_is_truthful(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.break_schema_for_test().expect("break works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_subscription_remove(&sub_id, window, cx)
        });
    });

    assert_eq!(notification_count(cx), 1);
}

#[gpui_kit::test]
fn subscriptions_dialog_db_error_notifies(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.break_schema_for_test().expect("break works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(notification_count(cx) >= 1, "listing failure notifies");
}

#[gpui_kit::test]
fn background_refresh_deleted_sub_is_silent(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let label = sub.label.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .remove_subscription(&sub_id)
                .expect("remove works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                &label,
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "bogus://x".to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
            view.finish_subscription_refresh(
                &sub_id,
                &label,
                Err("subscription fetch failed".to_string()),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 0, "deleted sub: no failure toast");
}

#[gpui_kit::test]
fn background_refresh_live_sub_failure_still_notifies(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let sub_id = sub.id.clone();
    let label = sub.label.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &sub_id,
                &label,
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "bogus://x\nbogus://y".to_string(),
                    userinfo: None,
                }),
                999,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 1, "live sub failure notifies");
}

#[test]
fn effective_addsub_group_dialog_wins() {
    use manyproxy_ui::effective_addsub_group;
    assert_eq!(effective_addsub_group("edited", "payload-group"), "edited");
    assert_eq!(effective_addsub_group("", "payload-group"), "payload-group");
    assert_eq!(
        effective_addsub_group("default", "payload-group"),
        "payload-group",
        "untouched default keeps the payload group"
    );
    assert_eq!(effective_addsub_group("default", "default"), "default");
    assert_eq!(
        effective_addsub_group("  edited  ", "payload-group"),
        "edited"
    );
}

#[test]
fn export_count_message_states_both_numbers() {
    use manyproxy_ui::export_count_message;
    assert_eq!(
        export_count_message(3, 3, "/tmp/x.json"),
        "Exported 3 configs to /tmp/x.json"
    );
    let both = export_count_message(4, 3, "/tmp/x.json");
    assert!(both.contains('4'), "total in: {both}");
    assert!(both.contains('3'), "importable in: {both}");
    assert!(both.contains("/tmp/x.json"), "path in: {both}");
}

#[gpui_kit::test]
fn import_commit_stale_conf_hint_parses_edited_links(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let links = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(
                links.to_string(),
                "travel".to_string(),
                Some("drop.conf".to_string()),
                window,
                cx,
            )
        })
    });
    assert!(closed, "edited links commit and close");
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].protocol, "vless");
    assert_eq!(rows[0].group, "travel");
}

#[gpui_kit::test]
fn import_commit_multi_files_keep_stems(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
    let b = "[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
    let merged = format!("{a}\n{b}");
    let files = vec![
        ("a.conf".to_string(), a.to_string()),
        ("b.conf".to_string(), b.to_string()),
    ];
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import_with_context(
                merged,
                "travel".to_string(),
                None,
                None,
                Some(files),
                window,
                cx,
            )
        })
    });
    assert!(closed, "multi commit closes");
    let mut names = view.read_with(cx, |view, _| {
        view.store
            .list()
            .expect("list works")
            .into_iter()
            .map(|row| row.name)
            .collect::<Vec<_>>()
    });
    names.sort();
    assert_eq!(names, vec!["a-p1".to_string(), "b-p1".to_string()]);
}

#[gpui_kit::test]
fn import_commit_edited_multi_parses_edited_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
    let files = vec![("a.conf".to_string(), a.to_string())];
    let links = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import_with_context(
                links.to_string(),
                "travel".to_string(),
                Some("a.conf".to_string()),
                Some(a.to_string()),
                Some(files),
                window,
                cx,
            )
        })
    });
    assert!(closed, "edited text commits");
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].protocol, "vless");
}

#[test]
fn crud_toast_messages_name_the_row() {
    use manyproxy_ui::row_action_message;
    assert_eq!(row_action_message("Added", "Work"), "Added \"Work\"");
    assert_eq!(row_action_message("Updated", "Work"), "Updated \"Work\"");
    assert_eq!(row_action_message("Removed", "Work"), "Removed \"Work\"");
}

#[gpui_kit::test]
fn crud_commits_toast_once_naming_row(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ConfigPatch;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.commit_add(seed_input("Work"), window, cx));
            view.store.list().expect("list works").remove(0).id
        })
    });
    assert_eq!(notification_count(cx), 1, "add toasts once");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.commit_edit(
                &id,
                ConfigPatch {
                    name: Some("Cabin".to_string()),
                    ..Default::default()
                },
                window,
                cx,
            ));
        });
    });
    assert_eq!(notification_count(cx), 2, "edit toasts once");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_remove(&id, window, cx));
    });
    assert_eq!(notification_count(cx), 3, "remove toasts once");
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);
}

#[test]
fn import_preview_counts_and_message() {
    use manyproxy_core::import_preview_counts;
    use manyproxy_ui::import_preview_message;
    let body = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x";
    assert_eq!(import_preview_counts(body, None), (1, 1));
    assert_eq!(import_preview_message(1, 1), "1 valid · 1 invalid");

    assert_eq!(import_preview_counts("", None), (0, 0));
    assert_eq!(import_preview_counts("   \n  ", None), (0, 0));
    assert_eq!(import_preview_message(0, 0), "0 valid · 0 invalid");

    let conf = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
    assert_eq!(import_preview_counts(conf, Some("home.conf")), (1, 0));
}

#[gpui_kit::test]
fn import_dialog_shows_preview_and_files(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog_with_files(
                a,
                "default",
                None,
                Some(vec![("a.conf".to_string(), a.to_string())]),
                window,
                cx,
            );
        });
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
    assert!(cx.update(|window, _| window.try_find("import-preview").is_some()));
    assert!(cx.update(|window, _| window.try_find("import-files").is_some()));
}

#[test]
fn t07_preview_two_valid_one_invalid_names_first_failure() {
    use manyproxy_core::{first_import_failure_reason, import_preview_counts};
    use manyproxy_ui::import_preview_message;
    let body = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nvless://22222222-3333-4444-8555-666666666666@b.example.com:443#two\nbogus://x";
    assert_eq!(import_preview_counts(body, None), (2, 1));
    assert_eq!(import_preview_message(2, 1), "2 valid · 1 invalid");
    let reason = first_import_failure_reason(body, None).expect("failure shown");
    assert!(reason.contains("item 3"), "item number in: {reason}");
    assert!(reason.contains("bogus://x"), "label in: {reason}");
    assert!(
        reason.to_ascii_lowercase().contains("unknown"),
        "reason in: {reason}"
    );
}

#[test]
fn t07_preview_multi_files_match_commit_parse() {
    use manyproxy_ui::preview_batch_for_form;
    let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
    let b = "[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
    let merged = format!("{a}\n{b}");
    let files = vec![
        ("a.conf".to_string(), a.to_string()),
        ("b.conf".to_string(), b.to_string()),
    ];

    let batch = preview_batch_for_form(&merged, None, Some(merged.clone()), Some(files.clone()));
    assert_eq!(batch.configs.len(), 2, "both files preview");
    assert!(batch.failures.is_empty());
    let mut names: Vec<String> = batch
        .configs
        .iter()
        .map(|config| config.name.clone())
        .collect();
    names.sort();
    assert_eq!(names, vec!["a-p1".to_string(), "b-p1".to_string()]);

    let links = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";
    let edited = preview_batch_for_form(links, None, Some(merged), Some(files));
    assert_eq!(edited.configs.len(), 1, "edited text previews alone");
    assert_eq!(edited.configs[0].protocol, "vless");

    let empty = preview_batch_for_form("", None, None, None);
    assert!(empty.configs.is_empty() && empty.failures.is_empty());
}

#[test]
fn t07_success_summary_pins_counts() {
    use manyproxy_ui::import::{ImportErrorKind, ImportFailure};
    use manyproxy_ui::import_success_summary;
    let clean = import_success_summary(2, 0, &[]);
    assert_eq!(clean, "Imported 2 · 0 duplicates skipped · 0 failed");
    let failures = vec![ImportFailure {
        index: 2,
        label: "bogus://x".to_string(),
        reason: "unknown-scheme: unknown scheme 'bogus'".to_string(),
        code: ImportErrorKind::UnknownScheme,
    }];
    let partial = import_success_summary(2, 1, &failures);
    assert!(
        partial.contains("Imported 2")
            && partial.contains("1 duplicates skipped")
            && partial.contains("1 failed"),
        "all three counts in: {partial}"
    );
    assert!(
        partial.contains("item 3") && partial.contains("bogus://x"),
        "failure listed in: {partial}"
    );
}

#[gpui_kit::test]
fn t07_total_failure_stays_open_lists_every_failure(cx: &mut TestAppContext) {
    use manyproxy_ui::import::format_import_failures;
    use manyproxy_ui::import::import_text;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let body =
        "vless://not-a-uuid@b.example.com:notaport#bad\nbogus://x\ntrojan://pw@c.example.com:99999";
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(body.to_string(), "g".to_string(), None, window, cx)
        })
    });
    assert!(!closed, "total failure keeps the dialog open");
    assert_eq!(notification_count(cx), 1, "all failures in one notice");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "nothing stored"
    );

    let batch = import_text(body);
    assert_eq!(batch.configs.len(), 0);
    assert_eq!(batch.failures.len(), 3);
    let listed = format_import_failures(&batch.failures);
    for item in ["item 1", "item 2", "item 3"] {
        assert!(listed.contains(item), "{item} listed in: {listed}");
    }
}

#[gpui_kit::test]
fn t07_partial_import_toasts_counts_and_selects_checks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let body = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nvless://22222222-3333-4444-8555-666666666666@b.example.com:443#two\nbogus://x";
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_import(body.to_string(), "travel".to_string(), None, window, cx)
        })
    });
    assert!(closed, "partial success closes");
    assert_eq!(notification_count(cx), 1, "one honest toast");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        2,
        "both valid rows land"
    );
    assert!(
        view.read_with(cx, |view, _| view.selection_focus.clone())
            .is_some(),
        "first imported row owns focus"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        2,
        "all imported rows arrive checked"
    );
}

#[gpui_kit::test]
fn t07_dialog_paints_preview_and_first_failure(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let body = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nvless://22222222-3333-4444-8555-666666666666@b.example.com:443#two\nbogus://x";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog(body, "travel", None, window, cx);
        });
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
    assert!(cx.update(|window, _| window.try_find("import-preview").is_some()));
    assert!(cx.update(|window, _| window.try_find("import-first-failure").is_some()));
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn no_persistent_drop_hint_band(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = mount_view(store, cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, _| window.try_find("drop-hint").is_none()));
}

#[test]
fn export_preview_shows_counts_path_and_overwrite() {
    use manyproxy_ui::export_preview_description;
    let fresh = export_preview_description(3, 3, "/tmp/x.json", false);
    assert!(fresh.contains('3'), "count in: {fresh}");
    assert!(fresh.contains("/tmp/x.json"), "path in: {fresh}");
    assert!(!fresh.contains("overwrit"), "no warning for fresh: {fresh}");
    let both = export_preview_description(4, 3, "/tmp/x.json", false);
    assert!(
        both.contains('4') && both.contains('3'),
        "both numbers: {both}"
    );
    let exists = export_preview_description(3, 3, "/tmp/x.json", true);
    assert!(
        exists.contains("/tmp/x.json") && exists.contains("overwrit"),
        "overwrite warning names the path: {exists}"
    );
}

#[test]
fn subscription_failures_text_renders_items() {
    use manyproxy_ui::import::{ImportErrorKind, ImportFailure};
    use manyproxy_ui::subscription_failures_text;
    assert_eq!(
        subscription_failures_text(&[]),
        "",
        "clean subs show no failures line"
    );
    let failures = vec![ImportFailure {
        index: 1,
        label: "bogus://x".to_string(),
        reason: "unknown-scheme: unknown scheme 'bogus'".to_string(),
        code: ImportErrorKind::UnknownScheme,
    }];
    let text = subscription_failures_text(&failures);
    assert!(text.contains("bogus://x"), "label in: {text}");
    assert!(text.contains("unknown-scheme"), "reason in: {text}");
}

#[gpui_kit::test]
fn subscriptions_dialog_shows_auto_and_failures(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Nightly", "https://example.com/sub.txt", "travel", true)
        .expect("seed works");
    store
        .refresh_with_body(
            &sub.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x",
            100,
        )
        .expect("refresh works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
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

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("field-sub-auto").is_some()));
    assert!(
        cx.update(move |window, _| window.try_find(format!("sub-failures-{sub_id}")).is_some()),
        "stored failures render per subscription"
    );
}

#[gpui_kit::test]
fn sub_add_form_auto_defaults_on(cx: &mut TestAppContext) {
    use manyproxy_ui::store::{DEFAULT_AUTO_REFRESH_SECS, DEFAULT_GROUP};
    use manyproxy_ui::SubForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let (auto, interval) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = SubForm::restored(
                window,
                cx,
                "",
                "",
                DEFAULT_GROUP,
                true,
                DEFAULT_AUTO_REFRESH_SECS,
            );
            (form.auto_refresh.clone(), form.interval_secs.clone())
        })
    });
    assert!(*auto.borrow(), "auto-refresh defaults on");
    assert_eq!(
        *interval.borrow(),
        DEFAULT_AUTO_REFRESH_SECS,
        "interval defaults daily"
    );
}

#[gpui_kit::test]
fn t08_interval_picker_one_grouped_exclusive_choice(cx: &mut TestAppContext) {
    use manyproxy_ui::INTERVAL_PRESETS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
    });
    paint_dialog_open(cx);

    assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

    assert_painted(cx, "sub-interval-add-caption");

    assert_eq!(INTERVAL_PRESETS.len(), 6, "six interval choices");
    assert_painted(cx, "sub-interval-add-group");

    cx.update(|window, cx| {
        window.within("sub-interval-add-group").click(1_usize, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| *view
            .subs_form
            .clone()
            .expect("subs form stashed")
            .interval_secs
            .borrow()),
        3_600,
        "clicking 1h marks 1h current"
    );
    cx.update(|window, cx| {
        window.within("sub-interval-add-group").click(4_usize, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| *view
            .subs_form
            .clone()
            .expect("subs form stashed")
            .interval_secs
            .borrow()),
        86_400,
        "clicking 24h moves the single mark (exclusive)"
    );
    cx.update(|window, cx| window.close_sheet(cx));
}

#[test]
fn t08_interval_presets_six_distinct_labels() {
    use manyproxy_ui::{app::bits::INTERVAL_PRESETS, format_interval_secs};
    assert_eq!(
        INTERVAL_PRESETS,
        &[0, 3_600, 21_600, 43_200, 86_400, 604_800]
    );
    let mut labels = INTERVAL_PRESETS
        .iter()
        .map(|secs| format_interval_secs(*secs))
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), 6, "six distinct labels: {labels:?}");
}

#[gpui_kit::test]
fn t08_global_off_explicit_refresh_still_reports(cx: &mut TestAppContext) {
    use manyproxy_ui::refresh_summary_text;
    use manyproxy_ui::store::now_unix;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let one = store
        .add_subscription("One", "https://example.com/one.txt", "g", true)
        .expect("seed works");
    let two = store
        .add_subscription("Two", "https://example.com/two.txt", "g", true)
        .expect("seed works");
    let one_id = one.id.clone();
    let one_label = one.label.clone();
    let two_id = two.id.clone();
    let now = now_unix();

    store
        .refresh_with_body(
            &two_id,
            "vless://11111111-2222-4333-8444-555555555555@b.example.com:443#two",
            now,
        )
        .expect("refresh stamps");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .set_subs_auto_refresh_enabled(false)
                .expect("kill writes");
        });
    });

    let (auto, manual) = view.read_with(cx, |view, _| {
        (
            view.due_subscriptions().len(),
            view.due_for_manual_refresh()
                .iter()
                .map(|sub| sub.id.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(auto, 0, "global off gates the automatic pass");
    assert_eq!(
        manual,
        vec![one_id.clone()],
        "explicit refresh still sees One"
    );

    let (_, summary) = refresh_summary_text(&one_label, 1, 0, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[]);
    assert!(
        summary.starts_with("One:"),
        "outcome names its sub: {summary}"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_subscription_refresh(
                &one_id,
                &one_label,
                Ok(manyproxy_ui::FetchedSubscription {
                    body: "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#solo"
                        .to_string(),
                    userinfo: None,
                }),
                now + 10,
                window,
                cx,
            );
        });
    });
    assert_eq!(notification_count(cx), 1, "only that subscription reports");
    let (one_after, two_after) = view.read_with(cx, |view, _| {
        (
            view.store
                .get_subscription(&one_id)
                .expect("get works")
                .expect("one there")
                .last_refresh_at,
            view.store
                .get_subscription(&two_id)
                .expect("get works")
                .expect("two there")
                .last_refresh_at,
        )
    });
    assert_eq!(one_after, Some(now + 10), "that subscription refreshed");
    assert_eq!(two_after, Some(now), "the other subscription untouched");
}

#[gpui_kit::test]
fn t08_http_warning_beside_url_field(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
    });
    paint_dialog_open(cx);

    cx.update(|window, cx| {
        let form = view.read_with(cx, |view, _| view.subs_form.clone());
        let form = form.expect("subs form stashed on open");
        form.url.update(cx, |state, cx| {
            state.set_value("http://example.com/sub.txt", window, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "sub-add-http-warning");

    cx.update(|window, cx| {
        let form = view.read_with(cx, |view, _| view.subs_form.clone());
        let form = form.expect("subs form restashed");
        form.url.update(cx, |state, cx| {
            state.set_value("https://example.com/sub.txt", window, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
    });
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("sub-add-http-warning").is_none()),
        "https stays quiet"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn coalesced_refresh_message_single_toast() {
    use manyproxy_ui::coalesced_refresh_message;
    assert_eq!(coalesced_refresh_message(0, 0, 0, 0, 0, 0, 0), None);
    let (warn, text) = coalesced_refresh_message(3, 5, 1, 0, 0, 0, 0).expect("batch reports");
    assert!(!warn);
    assert!(
        text.contains('3') && text.contains("[+]Added 5"),
        "one line: {text}"
    );
    let (warn, text) = coalesced_refresh_message(3, 5, 1, 0, 2, 0, 0).expect("batch reports");
    assert!(warn, "failures warn: {text}");
    assert!(text.contains('2'), "failed count in: {text}");

    let (warn, text) = coalesced_refresh_message(3, 5, 1, 0, 0, 2, 0).expect("batch reports");
    assert!(warn, "conflicts warn: {text}");
    assert!(
        text.contains('2') && text.contains("conflict"),
        "conflict count + copy in: {text}"
    );
}

#[gpui_kit::test]
fn background_batch_coalesces_to_one_toast(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for (label, group) in [("One", "g1"), ("Two", "g2"), ("Three", "g3")] {
        let sub = store
            .add_subscription(label, "https://example.com/sub.txt", group, true)
            .expect("seed works");
        ids.push((sub.id.clone(), sub.label.clone()));
    }
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let keys: Vec<String> = ids.iter().map(|(id, _)| id.clone()).collect();
            view.begin_refresh_batch(&keys);
        });
    });
    for (id, label) in &ids {
        let id = id.clone();
        let label = label.clone();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.finish_subscription_refresh(
                    &id,
                    &label,
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
    }
    assert_eq!(notification_count(cx), 1, "three refreshes, one toast");
}

#[test]
fn honest_start_stop_messages() {
    use manyproxy_ui::{already_stopped_message, start_all_message, stop_all_message};
    let empty = start_all_message(0);
    assert!(empty.contains("Nothing to start"), "honest: {empty}");
    assert!(!empty.contains("Started 0"), "never Started 0: {empty}");
    assert!(start_all_message(3).contains('3'));
    let idle = stop_all_message(0);
    assert!(idle.contains("Nothing to stop"), "honest: {idle}");

    let stopped = stop_all_message(2);
    assert!(stopped.contains('2'), "names the count: {stopped}");
    assert!(already_stopped_message("Work").contains("Work"));
    assert!(already_stopped_message("Work").contains("already stopped"));
}

#[test]
fn ticket43_honest_mutation_copy() {
    use manyproxy_ui::{
        duplicate_name, removed_message, restart_to_apply_message, stop_all_confirm_description,
    };
    let confirm = stop_all_confirm_description(&["A".to_string(), "B".to_string()]);
    assert!(confirm.contains('2'), "count: {confirm}");
    assert!(confirm.contains("\"A\""), "names A: {confirm}");
    assert!(confirm.contains("\"B\""), "names B: {confirm}");
    let restart = restart_to_apply_message("Work");
    assert!(restart.contains("Work"), "names row: {restart}");
    assert!(restart.contains("restart"), "says restart: {restart}");
    let removed_running = removed_message("Work", true);
    assert!(
        removed_running.contains("Work"),
        "names row: {removed_running}"
    );
    assert!(
        removed_running.contains("stopped"),
        "says stopped first: {removed_running}"
    );
    let removed_idle = removed_message("Work", false);
    assert!(
        !removed_idle.contains("stopped"),
        "no stop claim: {removed_idle}"
    );
    assert_eq!(duplicate_name("Work", &[]), "Work copy");
    assert_eq!(
        duplicate_name("Work", &["Work copy".to_string()]),
        "Work copy 2"
    );
    assert_eq!(
        duplicate_name(
            "Work",
            &["Work copy".to_string(), "Work copy 2".to_string()]
        ),
        "Work copy 3"
    );
}

#[test]
fn ticket43_probe_result_toasts() {
    use manyproxy_ui::health::{HealthKind, HealthSample, ProbeSuccess, TestOutcome};
    use manyproxy_ui::{probe_batched_toast, probe_result_toast, probe_start_toast};
    fn outcome(id: &str, latency: Option<u64>, error: Option<&str>) -> TestOutcome {
        TestOutcome {
            sample: HealthSample {
                config_id: id.to_string(),
                at: 1,
                kind: HealthKind::UrlTest,
                latency_ms: latency,
                error: error.map(str::to_string),
            },
            success: latency.map(|ms| ProbeSuccess {
                latency_ms: ms,
                bytes_rx: 0,
                bytes_tx: 0,
            }),
        }
    }
    let live = probe_result_toast("Work", &outcome("1", Some(33), None), false);
    assert!(live.contains("Work"), "names row: {live}");
    assert!(live.contains("33ms"), "names latency: {live}");
    let dead = probe_result_toast("Work", &outcome("1", None, Some("timeout")), false);
    assert!(dead.contains("Work"), "names row: {dead}");
    assert!(dead.contains("timeout"), "names cause: {dead}");
    let ping = probe_result_toast("Work", &outcome("1", Some(7), None), true);
    assert!(ping.contains("7ms"), "ping latency: {ping}");
    // K5: the batched run is one start line and one summary, no per-row prose.
    assert_eq!(probe_start_toast(3), "Testing 3…");
    assert_eq!(probe_batched_toast(3, 2, 1), "Tested 3: 2 live · 1 dead");
}

#[test]
fn ticket43_test_url_rules() {
    use manyproxy_ui::{effective_probe_url, normalize_test_url, DEFAULT_TEST_URL};
    assert_eq!(
        normalize_test_url("https://example.com/ping").expect("valid"),
        "https://example.com/ping"
    );
    assert!(
        normalize_test_url("ftp://example.com").is_err(),
        "scheme gated"
    );
    assert!(normalize_test_url("https://").is_err(), "host required");
    assert!(normalize_test_url("   ").is_err(), "blank rejected");
    assert_eq!(
        effective_probe_url(Some("http://row/"), Some("http://global/")),
        "http://row/"
    );
    assert_eq!(
        effective_probe_url(None, Some("http://global/")),
        "http://global/"
    );
    assert_eq!(effective_probe_url(None, None), DEFAULT_TEST_URL);
}

#[test]
fn ticket43_traffic_text() {
    use manyproxy_ui::{format_bytes, traffic_status_text, traffic_summary_text};
    assert_eq!(format_bytes(88), "88 B");
    assert_eq!(format_bytes(2048), "2.0 KB");
    assert_eq!(format_bytes(1_048_782), "1.0 MB");
    assert_eq!(traffic_summary_text(0, 0), None, "zero hides");
    let line = traffic_summary_text(1_048_782, 88).expect("nonzero shows");
    assert!(line.contains("1.0 MB") && line.contains("88 B"), "{line}");
    assert_eq!(traffic_status_text(0, 0), None, "zero hides");
    let status = traffic_status_text(1000, 50).expect("nonzero shows");
    assert!(status.contains("traffic"), "{status}");
}

#[test]
fn ticket43_search_covers_sub_preset_state() {
    use manyproxy_core::{library_row_matches_full, MatchContext};
    use manyproxy_ui::store::ProxyConfig;
    fn row() -> ProxyConfig {
        ProxyConfig {
            id: "7".to_string(),
            name: "Work".to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443}"
                .to_string()
                .into(),
            group: "default".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: Some("sub-1".to_string()),
            identity_key: None,
            preset_id: Some("preset-1".to_string()),
        }
    }
    let mut ctx = MatchContext::empty();
    ctx.sub_labels
        .insert("sub-1".to_string(), "News Feed".to_string());
    ctx.preset_names
        .insert("preset-1".to_string(), "Direct Out".to_string());
    ctx.running.insert("7".to_string());
    ctx.health.insert("7".to_string(), "33ms".to_string());
    let alpha = row();
    assert!(library_row_matches_full(&alpha, "news", &ctx), "sub label");
    assert!(
        library_row_matches_full(&alpha, "direct", &ctx),
        "preset name"
    );
    assert!(
        library_row_matches_full(&alpha, "enabled", &ctx),
        "enabled word"
    );
    assert!(!library_row_matches_full(&alpha, "disabled", &ctx));
    assert!(
        library_row_matches_full(&alpha, "running", &ctx),
        "running word"
    );
    assert!(!library_row_matches_full(&alpha, "stopped", &ctx));
    assert!(library_row_matches_full(&alpha, "live", &ctx), "live word");
    assert!(
        library_row_matches_full(&alpha, "33ms", &ctx),
        "latency text"
    );
    assert!(!library_row_matches_full(&alpha, "dead", &ctx));
    let empty = MatchContext::empty();
    assert!(
        !library_row_matches_full(&alpha, "news", &empty),
        "no ctx, no match"
    );
    assert!(
        library_row_matches_full(&alpha, "work", &empty),
        "name still matches"
    );
}

#[test]
fn ticket43_params_format_validate() {
    use manyproxy_ui::{format_params_json, validate_params_json};
    let pretty = format_params_json("{\"a\":1}").expect("valid formats");
    assert!(pretty.contains('\n'), "multiline: {pretty}");
    assert!(format_params_json("not json").is_err(), "bad JSON fails");
    assert!(validate_params_json("{\"a\":1}").is_ok());
    assert!(
        validate_params_json("   ").is_ok(),
        "blank uses the default"
    );
    assert!(validate_params_json("{oops").is_err(), "broken JSON named");
}

#[test]
fn ticket43_import_preview_and_browse() {
    use manyproxy_core::{first_import_failure_reason, merge_browse_results};
    let reason = first_import_failure_reason("bogus://x", None).expect("reason shown");
    assert!(!reason.is_empty(), "names something");
    assert_eq!(
        first_import_failure_reason("", None),
        None,
        "empty names nothing"
    );
    let files = vec![("a.conf".to_string(), "[Interface]\n".to_string())];
    let (merged, hint, multi) = merge_browse_results("typed\n", &files);
    assert!(merged.contains("typed") && merged.contains("[Interface]"));
    assert_eq!(hint, Some("a.conf".to_string()), "single keeps hint");
    assert!(multi.is_none(), "single is not multi");
    let files2 = vec![
        ("a.conf".to_string(), "x".to_string()),
        ("b.txt".to_string(), "y".to_string()),
    ];
    let (_, hint2, multi2) = merge_browse_results("", &files2);
    assert_eq!(hint2, None, "multi has no single hint");
    assert_eq!(multi2.expect("multi kept").len(), 2);
}

#[test]
fn ticket43_export_logs_subs_copy() {
    use manyproxy_core::tick_interval_summary;
    use manyproxy_ui::health::LogLevel;
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::{
        export_overwrite_confirm_description, log_level_matches, subscription_member_names,
    };
    let desc = export_overwrite_confirm_description("/tmp/x.json");
    assert!(desc.contains("/tmp/x.json"), "names path: {desc}");
    assert!(desc.contains("Overwrite"), "says overwrite: {desc}");
    assert!(log_level_matches(LogLevel::Error, None));
    assert!(log_level_matches(LogLevel::Error, Some(LogLevel::Error)));
    assert!(!log_level_matches(LogLevel::Info, Some(LogLevel::Error)));
    assert!(log_level_matches(LogLevel::Warn, Some(LogLevel::Warn)));
    assert!(
        !log_level_matches(LogLevel::Error, Some(LogLevel::Warn)),
        "level filter stays exact-match"
    );
    fn member(name: &str, sub: Option<&str>) -> ProxyConfig {
        ProxyConfig {
            id: format!("id-{name}"),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{}".to_string().into(),
            group: "g".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: sub.map(str::to_string),
            identity_key: None,
            preset_id: None,
        }
    }
    let rows = vec![
        member("A", Some("s1")),
        member("B", Some("s1")),
        member("C", None),
    ];
    assert_eq!(subscription_member_names(&rows, "s1"), vec!["A", "B"]);
    assert!(subscription_member_names(&rows, "s9").is_empty());
    let ticks = tick_interval_summary();
    assert!(
        ticks.contains("5s") && ticks.contains("300s") && ticks.contains("60s"),
        "{ticks}"
    );
}

#[gpui_kit::test]
fn ticket43_delete_running_stops_first(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10851"))
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
    assert!(view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)));
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_remove(&alpha.id, window, cx));
    });
    assert_eq!(notification_count(cx), before + 1, "one remove toast");
    assert!(
        !view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)),
        "endpoint stopped first"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "no orphan serves"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.get(&alpha.id).expect("get works"))
            .is_none(),
        "row disappears"
    );
}

#[gpui_kit::test]
fn ticket43_edit_running_prompts_restart(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::store::ConfigPatch;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10852"))
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
    let before = notification_count(cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit(
                &alpha.id,
                ConfigPatch {
                    name: Some("Alpha2".to_string()),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
    });
    assert!(closed, "valid edit closes");
    assert_eq!(notification_count(cx), before + 1, "restart prompt toasts");
    assert!(
        view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)),
        "edit never stops the endpoint by itself"
    );
}

#[gpui_kit::test]
fn ticket43_no_selection_toasts(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(ticket43_seed("Work", "10853"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    assert!(view
        .read_with(cx, |view, cx| view.selected_config(cx))
        .is_none());
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.stop_selected_engine(window, cx));
    });
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
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_selected_enabled(window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_selected_health(window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            manyproxy_ui::app::actions::run_action(
                view,
                "ping",
                manyproxy_ui::app::actions::ActionTarget::Selection,
                window,
                cx,
            )
        });
    });
    assert_eq!(notification_count(cx), before + 5, "every action toasts");
}

#[gpui_kit::test]
fn ticket43_duplicate_creates_disabled_auto_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10854"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_duplicate(&alpha.id, window, cx));
    });
    assert_eq!(notification_count(cx), before + 1, "duplicate toasts");
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(rows.len(), 2, "copy lands");
    let copy = rows
        .iter()
        .find(|row| row.id != alpha.id)
        .expect("copy there");
    assert_eq!(copy.name, "Alpha copy");
    assert!(!copy.enabled, "copy lands disabled");
    assert_ne!(copy.port, alpha.port, "fresh auto port");
    assert!(
        manyproxy_ui::store::is_auto_port(copy.port),
        "port is auto-range"
    );
    assert_eq!(copy.protocol, alpha.protocol);
    assert_eq!(copy.params, alpha.params);
}

#[gpui_kit::test]
fn t03_row_actions_target_clicked_row_not_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10854"))
        .expect("seed works");
    let beta = store
        .add(ticket43_seed("Beta", "10855"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.copy_row_urls(&beta.id, window, cx));
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    let (socks, http) = manyproxy_ui::store::proxy_urls(&beta.bind_ip, beta.port);
    assert_eq!(
        clip,
        format!("{socks}\n{http}"),
        "clipboard carries row B endpoints only"
    );
    let selected = view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id));
    assert_eq!(
        selected.as_deref(),
        Some(alpha.id.as_str()),
        "selection still row A"
    );
}

#[gpui_kit::test]
fn t03_row_duplicate_and_move_ignore_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10854"))
        .expect("seed works");
    let beta = store
        .add(ticket43_seed("Beta", "10855"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_duplicate(&beta.id, window, cx);
            assert!(view.commit_bulk_move(std::slice::from_ref(&beta.id), "team", window, cx));
        });
    });
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(rows.len(), 3, "duplicate lands beside both rows");
    let copy = rows
        .iter()
        .find(|row| row.name == "Beta copy")
        .expect("copy of B there");
    assert!(!copy.enabled, "copy lands disabled");
    assert_ne!(copy.port, beta.port, "fresh auto port");
    assert!(
        manyproxy_ui::store::is_auto_port(copy.port),
        "port is auto-range"
    );
    let moved = rows
        .iter()
        .find(|row| row.id == beta.id)
        .expect("row B there");
    assert_eq!(moved.group, "team", "row B moved");
    let kept = rows
        .iter()
        .find(|row| row.id == alpha.id)
        .expect("row A there");
    assert_ne!(kept.group, "team", "row A untouched");
    let selected = view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id));
    assert_eq!(
        selected.as_deref(),
        Some(alpha.id.as_str()),
        "selection still row A"
    );
}

#[gpui_kit::test]
fn t08_paste_binding_imports_direct_without_dialog(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x"
                .to_string(),
        ));
    });

    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let paste = Keystroke::parse("ctrl-v").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(paste, cx);
    });
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "K5: paste no longer blocks on the preview dialog"
    );
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert_eq!(
        rows.len(),
        1,
        "the valid share link lands even beside an invalid line: {:?}",
        rows.iter().map(|row| row.name.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "paste imports directly"
    );
}

#[test]
fn cua08_paste_intake_refusal_gates_prose_not_links() {
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal(""),
        Some("Clipboard has no text to import".to_string())
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal("  \n "),
        Some("Clipboard has no text to import".to_string())
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal(
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
        ),
        None
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal(
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x"
        ),
        None,
        "partially-valid paste still opens the dialog"
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal(
            "throne://addsub/demo?url=https://example.com/feed"
        ),
        None
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal("throne://route/demo"),
        None
    );
    assert_eq!(
        manyproxy_ui::AppView::paste_intake_refusal(r#"{"rules": [{"action": "direct"}]}"#),
        None
    );
    let prose = manyproxy_ui::AppView::paste_intake_refusal("just some meeting notes for Thursday")
        .expect("prose refuses");
    assert!(
        !prose.contains("Thursday"),
        "refusal echoes nothing, got {prose}"
    );
    let bogus = manyproxy_ui::AppView::paste_intake_refusal("bogus://x").expect("bogus refuses");
    assert!(
        !bogus.contains("bogus"),
        "refusal echoes nothing, got {bogus}"
    );
}

#[gpui_kit::test]
fn t08_direct_form_commits_bare_marker(cx: &mut TestAppContext) {
    use manyproxy_ui::store::protocol_params_template;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let params = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::blank(window, cx);
            form.protocol_select.update(cx, |state, cx| {
                state.set_selected_value(&"direct".to_string(), window, cx);
            });
            form.sync_protocol_template(window, cx);
            form.effective_params(cx)
        })
    });
    assert_eq!(params, protocol_params_template("direct"));
    assert_eq!(params, "{\"type\":\"direct\"}");
}

#[gpui_kit::test]
fn ticket43_reauto_port_via_auto_word(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ConfigPatch;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10855"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_edit(
                &alpha.id,
                ConfigPatch {
                    port: Some("auto".to_string()),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
    });
    assert!(closed, "re-auto closes");
    let row = view.read_with(cx, |view, _| {
        view.store
            .get(&alpha.id)
            .expect("get works")
            .expect("row there")
    });
    assert_ne!(row.port, 10855, "concrete port released");
    assert!(
        manyproxy_ui::store::is_auto_port(row.port),
        "back to auto-assign"
    );
}

#[gpui_kit::test]
fn ticket43_stop_all_confirms_naming_count(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(ticket43_seed("Alpha", "10856"))
        .expect("seed works");
    store
        .add(ticket43_seed("Beta", "10857"))
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
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 2);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.stop_all_engines(window, cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "confirm gates the kill — nothing stops yet"
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "confirm opens"
    );
    assert_eq!(notification_count(cx), before, "no toast before confirm");
    cx.update(|window, cx| window.close_all_dialogs(cx));
    let names = view.read_with(cx, |view, _| view.running_names_for_close());
    assert_eq!(names.len(), 2, "confirm would name both rows");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_stop_all(window, cx));
    });
    assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 0);
    assert_eq!(
        notification_count(cx),
        before + 1,
        "counted toast after confirm"
    );
}

#[gpui_kit::test]
fn ticket43_test_url_commits(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10858"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let ok = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_test_url("https://example.com/ping", window, cx)
        })
    });
    assert!(ok, "valid URL saves");
    assert_eq!(
        view.read_with(cx, |view, _| view.store.test_url()),
        Some("https://example.com/ping".to_string())
    );
    let bad = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_test_url("ftp://x", window, cx))
    });
    assert!(!bad, "bad scheme fails");
    let cleared =
        cx.update(|window, cx| view.update(cx, |view, cx| view.commit_test_url("   ", window, cx)));
    assert!(cleared, "blank clears");
    assert_eq!(view.read_with(cx, |view, _| view.store.test_url()), None);
    let row_ok = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_row_test_url(&alpha.id, "http://192.0.2.1/h", window, cx)
        })
    });
    assert!(row_ok, "row override saves");
    let url = view.read_with(cx, |view, _| {
        let rows = vec![view.store.get(&alpha.id).expect("get works").expect("row")];
        view.effective_test_url(&rows)
    });
    assert_eq!(url, "http://192.0.2.1/h", "row beats global");
    let row_bad = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_row_test_url(&alpha.id, "gopher://x", window, cx)
        })
    });
    assert!(!row_bad, "bad row URL fails");
}

#[gpui_kit::test]
fn bug27_edit_commit_keeps_dialog_open_on_bad_row_url(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10861"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);

    let (ok, field) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::prefilled(window, cx, &alpha, None);
            form.name.update(cx, |state, cx| {
                state.set_value("Alpha Renamed", window, cx);
            });
            let patch = form.as_patch_vs(cx, &alpha);
            let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
            let ok = view.commit_edit_with_test_url(
                &alpha.id,
                patch,
                "gopher://x",
                &form,
                &form_error,
                window,
                cx,
            );
            let outcome = (ok, form_error.borrow().clone());
            outcome
        })
    });

    assert!(!ok, "rejected row URL keeps the dialog open");
    assert_eq!(
        field.map(|(name, _)| name).as_deref(),
        Some("test_url"),
        "inline error names the row test URL field"
    );
    let row = view.read_with(cx, |view, _| {
        view.store
            .get(&alpha.id)
            .expect("get works")
            .expect("row kept")
    });
    assert_eq!(row.name, "Alpha Renamed", "row data still saved");
    assert_eq!(
        view.read_with(cx, |view, _| view.store.row_test_url(&alpha.id)),
        None,
        "rejected URL not stored"
    );
}

#[gpui_kit::test]
fn ticket43_probe_toast_and_traffic_resets(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(ticket43_seed("Alpha", "10859"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, handle) = FakeHealthProber::shared();
            handle.script_latency(&alpha.id, 33);
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reload(window, cx));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_selected_health(window, cx));
    });
    cx.run_until_parked();
    assert_eq!(notification_count(cx), before + 1, "selected test toasts");
    let latency = view.read_with(cx, |view, _| view.health_store.status(&alpha.id).latency_ms);
    assert_eq!(latency, Some(33), "latency recorded");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .add_traffic(&alpha.id, 1000, 50)
                .expect("delta lands");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_reset_traffic(&alpha.id, window, cx)
        });
    });
    let totals = view.read_with(cx, |view, _| view.store.traffic_totals().expect("totals"));
    assert_eq!(totals.get(&alpha.id), Some(&(0, 0)), "row reset zeroes");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_clear_traffic(window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_clear_health(window, cx));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_clear_snapshot(window, cx));
    });
    assert_eq!(
        notification_count(cx),
        before + 5,
        "each maintenance toasts"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.load_running_snapshot())
            .is_empty(),
        "snapshot cleared while the endpoint still runs"
    );
}

#[gpui_kit::test]
fn ticket43_preset_edit_and_duplicate(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let preset = store
        .create_preset("Base", "manual", None, &[])
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let bad = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_edit(
                &preset.id,
                "Base".to_string(),
                "{oops".to_string(),
                window,
                cx,
            )
        })
    });
    assert!(!bad, "bad rules stay open");
    let ok = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_edit(&preset.id, "Renamed".to_string(), String::new(), window, cx)
        })
    });
    assert!(ok, "valid edit closes");
    let renamed = view.read_with(cx, |view, _| {
        view.store
            .get_preset(&preset.id)
            .expect("get works")
            .expect("preset there")
    });
    assert_eq!(renamed.name, "Renamed");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_preset_duplicate(&preset.id, window, cx)
        });
    });
    let presets = view.read_with(cx, |view, _| view.store.list_presets().expect("list works"));
    assert_eq!(presets.len(), 2, "duplicate lands");
    assert!(presets.iter().any(|preset| preset.name == "Renamed copy"));
}

#[gpui_kit::test]
fn ticket43_export_to_path_and_browse_pick(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(ticket43_seed("Alpha", "10860"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let dir = std::env::temp_dir().join("manyproxy-43-test");
    let _ = std::fs::create_dir_all(&dir);
    let target = dir.join("export-43.json");
    let _ = std::fs::remove_file(&target);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_export_write_to(&target.display().to_string(), window, cx)
        });
    });
    let body = std::fs::read_to_string(&target).expect("export file written");
    assert!(body.contains("library"), "payload has library section");
    let _ = std::fs::remove_file(&target);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.finish_browse_pick(
                "typed".to_string(),
                "g".to_string(),
                vec![("a.txt".to_string(), "vless://x".to_string())],
                Vec::new(),
                window,
                cx,
            );
        });
    });
    // K17/A7-10: the dialog's Files row is the one representation of the
    // picked files — no success toast repeats the names on top of it.
    assert_eq!(
        notification_count(cx),
        before,
        "browse pick toasts nothing when every file read"
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "dialog reopens merged"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn ticket43_logs_level_and_copy(cx: &mut TestAppContext) {
    use manyproxy_ui::health::LogLevel;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(ticket43_seed("Alpha", "10861"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw(
                "panic: sidecar blew up",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
            view.logs.store.push_raw(
                "listening on 127.0.0.1:10861",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_log_level(Some(LogLevel::Error), window, cx)
        });
    });
    let lines = view.read_with(cx, |view, cx| view.log_lines(cx));
    assert!(!lines.is_empty(), "error line survives the filter");
    assert!(
        lines.iter().all(|line| line.contains("[error]")),
        "only errors: {lines:?}"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.copy_visible_logs(window, cx));
    });
    let clip = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    });
    assert!(clip.contains("panic"), "clipboard carries visible lines");
}

#[gpui_kit::test]
fn laneb_logs_redact_and_save_gate(cx: &mut TestAppContext) {
    use manyproxy_ui::logs_overwrite_text;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(ticket43_seed("Alpha", "10861"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw(
                "warn dial failed password=hunter2",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
        });
    });
    let lines = view.read_with(cx, |view, cx| view.log_lines(cx));
    assert!(
        lines.iter().all(|line| !line.contains("hunter2")),
        "rendered lines redact: {lines:?}"
    );
    let path = std::env::temp_dir().join("manyproxy-laneb-logs.txt");
    assert!(
        logs_overwrite_text(&path).contains("Secrets stay redacted"),
        "overwrite gate promises redaction"
    );
    assert!(
        logs_overwrite_text(&path).contains(&path.display().to_string()),
        "overwrite gate names the destination"
    );

    let _ = std::fs::remove_file(&path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.write_visible_logs_to(&path, cx).expect("save works")
        });
    });
    let body = std::fs::read_to_string(&path).expect("logs file written");
    assert!(
        !body.contains("hunter2"),
        "saved file carries no secrets: {body}"
    );
    let _ = std::fs::remove_file(&path);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "log-redact-note");
    assert_painted(cx, "log-save-path");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t07_logs_virtualized_filter_level_tail(cx: &mut TestAppContext) {
    use manyproxy_ui::health::{LogLevel, LogTagIndex};
    cx.update(gpui_kit::init);

    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let tags = LogTagIndex::default();
            for i in 0..10_000 {
                if i % 7 == 0 {
                    view.logs.store.push_raw(
                        &format!("error boom {i} password=hunter2 token=secret-abc"),
                        &tags,
                    );
                } else if i % 11 == 0 {
                    view.logs.store.push_raw(
                        &format!(
                            "warn wobble {i} \
                                 vless://11111111-2222-4333-8444-555555555555@h.example.com:443"
                        ),
                        &tags,
                    );
                } else {
                    view.logs.store.push_raw(&format!("info line {i}"), &tags);
                }
            }
            view.logs.tail = true;
        });
    });

    let visible = view.read_with(cx, |view, cx| view.log_lines(cx));
    assert_eq!(visible.len(), 200, "visible window caps the tail");
    assert!(
        visible.last().expect("tail nonempty").contains("9999"),
        "tail follows the newest (i=9999 seeds a warn line: 9999 % 11 == 0): {:?}",
        visible.last()
    );
    for line in &visible {
        for secret in ["hunter2", "secret-abc", "555555555555"] {
            assert!(
                !line.contains(secret),
                "uuid/password/token forms stay redacted: {line}"
            );
        }
    }

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);
    for id in [
        "log-lines",
        "log-level-row",
        "log-tail-spinner",
        "log-redact-note",
        "log-save-path",
        "log-copy-visible",
        "log-tail-toggle",
        "logs-save",
        "logs-dialog-ok",
        "logs-dialog-cancel",
    ] {
        assert_painted(cx, id);
    }
    assert_painted(cx, "log-line-0");
    assert!(
        cx.update(|window, _| window.try_find("log-line-199").is_none()),
        "off-viewport rows never mount (virtualized, not a 200-child flex_col)"
    );

    let pause_width = cx.update(|window, _| {
        window
            .try_find("log-tail-toggle")
            .expect("toggle paints")
            .bounds()
            .size
            .width
    });
    cx.update(|window, cx| {
        window.click("log-tail-toggle", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window.try_find("log-tail-spinner").is_none()),
        "paused tail shows no spinner"
    );
    assert_painted(cx, "log-tail-toggle");
    let resume_width = cx.update(|window, _| {
        window
            .try_find("log-tail-toggle")
            .expect("toggle paints")
            .bounds()
            .size
            .width
    });
    assert_eq!(
        pause_width, resume_width,
        "Pause/Resume keeps a stable width (no row reflow)"
    );

    cx.update(|window, cx| {
        window.click("log-tail-toggle", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_painted(cx, "log-tail-spinner");

    cx.update(|window, cx| {
        window.within("log-level-row").click(4_usize, cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.level),
        Some(LogLevel::Error),
        "level row narrows the filter"
    );
    assert!(
        cx.update(|window, _| window.try_find("logs-level").is_none()),
        "no second level control beside Copy/tail/Save"
    );
    let narrowed = view.read_with(cx, |view, cx| view.log_lines(cx));
    assert!(
        !narrowed.is_empty() && narrowed.iter().all(|line| line.contains("[error]")),
        "only errors survive: {narrowed:?}"
    );
    assert_painted(cx, "log-line-0");
    assert_painted(cx, "log-tail-spinner");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.copy_visible_logs(window, cx));
    });
    let clip = cx
        .update(|_, cx| {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| text.to_string())
        })
        .unwrap_or_default();
    assert!(clip.contains("[error]"), "clipboard carries visible lines");
    for secret in ["hunter2", "secret-abc", "555555555555"] {
        assert!(!clip.contains(secret), "clipboard stays redacted: {clip}");
    }

    let dir = std::env::temp_dir().join(format!("manyproxy-t07-logs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir creates");
    let path = dir.join("visible.txt");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.write_visible_logs_to(&path, cx).expect("save works");
        });
    });
    let saved = std::fs::read_to_string(&path).expect("saved file reads");
    assert_eq!(saved, narrowed.join("\n"), "file matches visible lines");
    for secret in ["hunter2", "secret-abc", "555555555555"] {
        assert!(!saved.contains(secret), "saved file stays redacted");
    }
    std::fs::remove_dir_all(&dir).expect("temp dir cleans");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t07_logs_save_path_discloses_override() {
    use manyproxy_ui::logs_save_path_text;
    let text = logs_save_path_text();
    assert!(
        text.contains("MANYPROXY_LOGS"),
        "names the override: {text}"
    );
    assert!(
        text.contains(&Store::default_logs_path().display().to_string()),
        "names the exact destination: {text}"
    );
}

#[gpui_kit::test]
fn laneb_identity_masked_and_sub_full_url_confirm(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let (masked, roundtrip) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::blank(window, cx);
            let masked = form.guided_identity.read(cx).presentation().is_masked();
            assert!(
                !form.guided_server.read(cx).presentation().is_masked(),
                "server stays readable"
            );
            form.guided_identity.update(cx, |state, cx| {
                state.set_value("11111111-2222-4333-8444-555555555555", window, cx);
            });
            let kept = form.guided_identity.read(cx).value().to_string();
            (masked, kept)
        })
    });
    assert!(masked, "identity secret masked at rest");
    assert_eq!(
        roundtrip, "11111111-2222-4333-8444-555555555555",
        "masked values still set + read"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_sub_full_url_confirm("Nightly", "https://example.com/sub.txt", window, cx)
        });
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "full URL copy confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn shell_regions_carry_one_home_each(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);

    for id in [
        "nav",
        "nav-all-configs",
        "nav-routing-presets",
        "nav-settings",
        "action-bar",
        "action-bar-import",
        "action-bar-add",
        "action-bar-palette",
        "action-bar-overflow",
        "filter-slot",
        "engine-bar",
        "engine-bar-start-all",
        "engine-bar-stop-all",
        "engine-bar-overflow",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "{id} renders in the new shell"
        );
    }

    // The five-menu toolbar, the three-segment status strip and the facet
    // band are gone, not hidden.
    for id in [
        "add-config",
        "import-config",
        "library-menu",
        "manage-menu",
        "engine-menu",
        "help-menu",
        "program-menu",
        "settings-menu",
        "groups-menu",
        "routing-menu",
        "tools-menu",
        "global-start-stop",
        "toolbar-import",
        "toolbar-add-config",
        "status-left",
        "status-center",
        "status-right",
        "facet-bar",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_none()),
            "{id} stays retired"
        );
    }

    for (trigger, items) in [
        (
            "action-bar-overflow",
            &[
                "action-overflow-export",
                "action-overflow-updates",
                "action-overflow-about",
            ][..],
        ),
        (
            "engine-bar-overflow",
            &["engine-bar-test-all", "engine-bar-stop-testing"][..],
        ),
    ] {
        cx.update(|window, cx| {
            window.click(trigger, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
        for &id in items {
            assert!(
                cx.update(|window, _| window.try_find(id).is_some()),
                "{id} renders in the {trigger} menu"
            );
        }
        cx.update(|window, cx| {
            use gpui_kit::Keystroke;
            let escape = Keystroke::parse("escape").expect("test keystroke parses");
            window.dispatch_keystroke(escape, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
    }
}

#[gpui_kit::test]
fn t02_shell_surfaces_announce_names(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);

    for (id, pinned) in [
        ("nav-all-configs", "All configs"),
        ("nav-routing-presets", "Routing presets"),
        ("nav-settings", "Settings"),
        ("action-bar-import", "Import"),
        ("action-bar-add", "Add config"),
        ("action-bar-palette", "Command palette"),
        ("action-bar-overflow", "More actions"),
        ("engine-bar-start-all", "Start all"),
        ("engine-bar-stop-all", "Stop all"),
        ("engine-bar-overflow", "More actions"),
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
            "{id} announces its visible label"
        );
    }

    for (trigger, items) in [
        (
            "action-bar-overflow",
            &[
                ("action-overflow-export", "Export…"),
                ("action-overflow-updates", "Check for updates"),
                ("action-overflow-about", "About"),
            ][..],
        ),
        (
            "engine-bar-overflow",
            &[
                ("engine-bar-test-all", "Test all"),
                ("engine-bar-stop-testing", "Stop testing"),
            ][..],
        ),
    ] {
        cx.update(|window, cx| {
            window.click(trigger, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
        for (id, pinned) in items {
            let label = cx.update(|window, _| {
                window
                    .try_find(id.to_string())
                    .unwrap_or_else(|| panic!("{id} renders in the {trigger} menu"))
                    .label()
                    .map(str::to_string)
            });
            assert_eq!(
                label.as_deref(),
                Some(*pinned),
                "{id} announces its visible label"
            );
        }
        cx.update(|window, cx| {
            use gpui_kit::Keystroke;
            let escape = Keystroke::parse("escape").expect("test keystroke parses");
            window.dispatch_keystroke(escape, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
    }
}

/// F9: four menu triggers carry a caret and Settings stays a direct action.
/// The caret has no element id, so it is pinned by geometry: it is the only
/// variable in the trigger's trailing slot, and it is what pushes the three
/// seven-letter triggers past the caret-free eight-letter Settings button.
#[gpui_kit::test]
fn f09_overflow_ellipsis_rule(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);

    // Both overflows are one glyph wide; they name themselves for the
    // assistive tree and carry the full label in their tooltip.
    for id in ["action-bar-overflow", "engine-bar-overflow"] {
        let width = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .bounds()
                .size
                .width
        });
        assert!(width <= px(64.), "{id} stays one glyph wide: {width:?}");
    }

    // The ellipsis marks only a surface that needs input before it applies.
    assert_eq!(
        label_of("action-bar-overflow", cx),
        "More actions",
        "the overflow names itself"
    );
    let open = |cx: &mut VisualTestContext, trigger: &'static str| {
        cx.update(|window, cx| window.click(trigger, cx));
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
    };
    let close = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            let escape = Keystroke::parse("escape").expect("test keystroke parses");
            window.dispatch_keystroke(escape, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
    };

    open(cx, "action-bar-overflow");
    assert_eq!(
        label_of("action-overflow-export", cx),
        "Export…",
        "Export asks first"
    );
    assert_eq!(
        label_of("action-overflow-about", cx),
        "About",
        "About applies at once"
    );
    close(cx);

    open(cx, "engine-bar-overflow");
    assert_eq!(
        label_of("engine-bar-test-all", cx),
        "Test all",
        "Test all applies on the spot, no ellipsis"
    );
    close(cx);
}

#[gpui_kit::test]
fn cua07_action_overflow_uia_walk_matches_fixture(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("action-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/uia-action-overflow.json"))
            .expect("action bar overflow fixture parses");
    // The action bar owns intake; the overflow keeps the verbs that are
    // neither intake nor transport.
    for retired in [
        "menu-add-config",
        "menu-import-config",
        "import-clipboard",
        "import-files",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(retired.to_string()).is_none()),
            "{retired} stays out of the overflow"
        );
    }
    let items = fixture["items"].as_array().expect("fixture lists items");
    let names: Vec<&str> = items
        .iter()
        .map(|item| item["name"].as_str().expect("fixture name"))
        .collect();
    for expected in ["Export", "About"] {
        assert!(
            names.iter().any(|name| name.contains(expected)),
            "the overflow walk names {expected}: {names:?}"
        );
    }
    for item in items {
        let id = item["id"].as_str().expect("fixture id");
        let (role, label) = cx.update(|window, _| {
            let snapshot = window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders in the action bar's overflow"));
            (
                snapshot.role().map(|role| format!("{role:?}")),
                snapshot.label().map(str::to_string),
            )
        });
        assert_eq!(
            role.as_deref(),
            Some(item["role"].as_str().expect("fixture role")),
            "{id} exposes its fixture role"
        );
        assert_eq!(
            label.as_deref(),
            item["name"].as_str(),
            "{id} announces its fixture name"
        );
    }
    cx.update(|window, cx| {
        use gpui_kit::Keystroke;
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

#[gpui_kit::test]
fn cua07_card_context_menu_exposes_every_pinned_item(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let card = format!("row-surface-{}", row.id);
    assert!(
        cx.update(|window, _| window.try_find(card.clone()).is_some()),
        "seeded card paints"
    );
    cx.update(|window, cx| {
        window.right_click(card.clone(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    // R31: the row More menu is exhaustive and top-to-bottom. Every verb
    // the surface map gives a row lives here, in this order, and nothing is
    // demoted behind a second submenu any more.
    let core = [
        ("start", "Start"),
        ("stop", "Stop"),
        ("test", "Test"),
        ("edit", "Edit config…"),
        ("copy-urls", "Copy URLs"),
        ("move", "Move to group"),
        ("assign-preset", "Assign preset"),
        ("logs", "Logs"),
        ("share", "Share…"),
        ("duplicate", "Duplicate"),
        ("ping", "Ping"),
        ("auto-start", "Restore on launch"),
        ("delete", "Delete"),
    ];
    let mut rows = Vec::new();
    for (verb, name) in core {
        let id = format!("rowctx-{verb}-{}", row.id);
        let (role, label, y) = cx.update(|window, _| {
            let snapshot = window
                .try_find(id.clone())
                .unwrap_or_else(|| panic!("{id} renders in the row More menu"));
            (
                snapshot.role().map(|role| format!("{role:?}")),
                snapshot.label().map(str::to_string),
                snapshot.bounds().origin.y,
            )
        });
        assert_eq!(
            role.as_deref(),
            Some("MenuItem"),
            "{id} exposes its MenuItem role"
        );
        assert_eq!(label.as_deref(), Some(name), "{id} announces its name");
        rows.push((name, y));
    }
    for pair in rows.windows(2) {
        assert!(
            pair[0].1 < pair[1].1,
            "row More order is Start, Stop, Test, Edit, Copy URLs, Move to group, Assign preset, Logs, Share., Duplicate, Ping, Reset., Restore on launch, Delete: {rows:?}"
        );
    }

    let labels: Vec<String> = cx.update(|window, _| {
        gpui_kit::base::test_support::snapshots(window)
            .iter()
            .filter_map(|snapshot| snapshot.label().map(str::to_string))
            .collect()
    });
    assert!(
        labels.iter().any(|label| label == "Reset…"),
        "the Reset. submenu row renders: {labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label == "More ?"),
        "the old More submenu is retired: {labels:?}"
    );

    // K8: the Speedtest / Resolve Out-IP / Resolve domain verbs are retired.
    for verb in ["speedtest", "out-ip", "resolve-domain"] {
        let id = format!("rowctx-{verb}-{}", row.id);
        assert!(
            cx.update(|window, _| window.try_find(id.clone()).is_none()),
            "{id} is retired from every menu"
        );
    }

    cx.update(|window, cx| {
        use gpui_kit::Keystroke;
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

#[gpui_kit::test]
fn cua07_menus_keyboard_operable(cx: &mut TestAppContext) {
    use gpui_kit::Keystroke;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("action-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        let down = Keystroke::parse("down").expect("test keystroke parses");
        window.dispatch_keystroke(down, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        let enter = Keystroke::parse("enter").expect("test keystroke parses");
        window.dispatch_keystroke(enter, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Enter on the first overflow item opens Export"
    );
    let before = notification_count(cx);
    cx.update(|window, cx| {
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "Esc closes Add"
    );
    assert_eq!(notification_count(cx), before, "Esc commits nothing");
    let card = format!("row-surface-{}", row.id);
    cx.update(|window, cx| {
        window.right_click(card.clone(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    let edit = format!("rowctx-edit-{}", row.id);
    assert!(
        cx.update(|window, _| window.try_find(edit.clone()).is_some()),
        "card menu opens from the keyboard path too"
    );
    cx.update(|window, cx| {
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, _| window.try_find(edit.clone()).is_none()),
        "Esc dismisses the card menu"
    );
}

#[gpui_kit::test]
fn cua07_card_menu_invoke_matches_click(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(row19_seed("Work", "a.example.com", "10801"))
        .expect("seed works");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    let card = format!("row-surface-{}", row.id);
    cx.update(|window, cx| {
        window.right_click(card.clone(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click(format!("rowctx-edit-{}", row.id), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "card-menu Edit opens the Edit dialog"
    );
    cx.update(|window, cx| {
        window.close_all_dialogs(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        window.right_click(card.clone(), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click(format!("rowctx-test-{}", row.id), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        notification_count(cx),
        before + 1,
        "card-menu Test toasts on a stopped row"
    );
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "stopped-row Test opens no dialog and probes nothing"
    );
}

#[gpui_kit::test]
fn t02_shell_bars_fit_one_row_at_the_minimum(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    let action = [
        "action-bar-import",
        "action-bar-add",
        "action-bar-palette",
        "action-bar-overflow",
    ];
    let engine = [
        "engine-bar-start-all",
        "engine-bar-stop-all",
        "engine-bar-overflow",
    ];
    for id in action.iter().chain(engine.iter()) {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    for ids in [action.to_vec(), engine.to_vec()] {
        let ys = cx.update(|window, _| {
            ids.iter()
                .map(|id| {
                    window
                        .try_find(id.to_string())
                        .unwrap_or_else(|| panic!("{id} renders"))
                        .bounds()
                        .origin
                        .y
                })
                .collect::<Vec<_>>()
        });
        let (min_y, max_y) = ys.iter().fold((ys[0], ys[0]), |(min, max), y| {
            (
                if *y < min { *y } else { min },
                if *y > max { *y } else { max },
            )
        });
        assert!(
            max_y - min_y <= px(1.),
            "the bar shares one row at the minimum width: {ys:?}"
        );
    }
}

#[gpui_kit::test]
fn t02_shell_surfaces_run_the_same_handlers(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);

    cx.update(|window, cx| {
        window.click("action-bar-overflow", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("action-overflow-export", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the action bar's Export runs its handler"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        use gpui_kit::Keystroke;
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|window, cx| {
        window.click("nav-routing-presets", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_painted(cx, "presets-dialog-ok");
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    paint_frame(cx);

    cx.update(|window, cx| {
        window.click("nav-settings", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the nav's Settings opens the modal directly"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    paint_frame(cx);
    cx.update(|window, cx| {
        use gpui_kit::Keystroke;
        let escape = Keystroke::parse("escape").expect("test keystroke parses");
        window.dispatch_keystroke(escape, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);

    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-bar-start-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        notification_count(cx),
        before + 1,
        "the engine bar's Start all runs its handler"
    );
}

#[test]
fn t02_global_toggle_present_pins_three_states() {
    assert_eq!(
        manyproxy_ui::global_toggle_present(0, false),
        ("▶", "Start all")
    );
    assert_eq!(
        manyproxy_ui::global_toggle_present(3, false),
        ("■", "Stop all")
    );
    assert_eq!(
        manyproxy_ui::global_toggle_present(0, true),
        ("…", "Connecting…")
    );
    assert_eq!(
        manyproxy_ui::global_toggle_present(2, true),
        ("…", "Connecting…")
    );
}

#[gpui_kit::test]
fn t02_engine_bar_start_all_starts_every_row(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
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

    let label = cx.update(|window, _| {
        window
            .try_find("engine-bar-start-all".to_string())
            .expect("the engine bar's Start all renders")
            .label()
            .map(str::to_string)
    });
    assert_eq!(label.as_deref(), Some("Start all"));
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-bar-start-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        2,
        "global Start starts every endpoint"
    );
    assert_eq!(notification_count(cx), before + 1, "one honest toast");

    let label = cx.update(|window, _| {
        window
            .try_find("engine-bar-stop-all".to_string())
            .expect("the engine bar's Stop all renders")
            .label()
            .map(str::to_string)
    });
    assert_eq!(label.as_deref(), Some("Stop all"));
}

#[gpui_kit::test]
fn t02_engine_bar_start_all_toasts_honestly_when_empty(cx: &mut TestAppContext) {
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
    paint_frame(cx);
    let before = notification_count(cx);
    cx.update(|window, cx| {
        window.click("engine-bar-start-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing to run"
    );
    assert_eq!(
        notification_count(cx),
        before + 1,
        "honest empty-library toast"
    );
    assert!(manyproxy_ui::start_all_message(0).contains("Nothing to start"));
    let label = cx.update(|window, _| {
        window
            .try_find("engine-bar-start-all".to_string())
            .expect("the engine bar's Start all renders")
            .label()
            .map(str::to_string)
    });
    assert_eq!(label.as_deref(), Some("Start all"));
}

#[gpui_kit::test]
fn t02_engine_bar_start_all_confirms_lan_rows_first(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut lan = t30_engine_seed("Lan", "10953");
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
    cx.update(|window, cx| {
        window.click("engine-bar-start-all", cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "LAN gate confirms first"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing serves before confirm"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t02_shell_has_no_mode_controls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    let mut names: Vec<String> = Vec::new();

    for id in [
        "nav-all-configs",
        "nav-routing-presets",
        "nav-settings",
        "action-bar-import",
        "action-bar-add",
        "action-bar-palette",
        "action-bar-overflow",
        "engine-bar-start-all",
        "engine-bar-stop-all",
        "engine-bar-overflow",
    ] {
        names.push(cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders"))
                .label()
                .map(str::to_string)
                .unwrap_or_default()
        }));
    }

    for (trigger, items) in [
        (
            "action-bar-overflow",
            &[
                "action-overflow-export",
                "action-overflow-updates",
                "action-overflow-about",
            ][..],
        ),
        (
            "engine-bar-overflow",
            &["engine-bar-test-all", "engine-bar-stop-testing"][..],
        ),
    ] {
        cx.update(|window, cx| {
            window.click(trigger, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
        for &id in items {
            names.push(cx.update(|window, _| {
                window
                    .try_find(id.to_string())
                    .unwrap_or_else(|| panic!("{id} renders in the {trigger} menu"))
                    .label()
                    .map(str::to_string)
                    .unwrap_or_default()
            }));
        }
        cx.update(|window, cx| {
            use gpui_kit::Keystroke;
            let escape = Keystroke::parse("escape").expect("test keystroke parses");
            window.dispatch_keystroke(escape, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
    }
    for name in &names {
        let lowered = name.to_lowercase();
        for banned in [
            "tun",
            "system proxy",
            "system dns",
            "hijack",
            "warp",
            "elevation",
        ] {
            assert!(
                !lowered.contains(banned),
                "no mode control on the top bar, got {name:?}"
            );
        }
    }
    assert!(!names.is_empty(), "scanned the whole bar");
}

#[gpui_kit::test]
fn t03_action_bar_import_opens_dialog(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    paint_frame(cx);
    cx.update(|window, cx| {
        window.click("action-bar-import", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the action bar's Import button opens the import dialog"
    );
    assert_painted(cx, "import-dialog-ok");
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    paint_frame(cx);
}

#[gpui_kit::test]
fn t03_inline_search_field_focusable(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    assert_painted(cx, "library-search");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.focus_library_search(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);

    let query_focused = cx.update(|window, cx| {
        let handle = view.read(cx).search_input.read(cx).focus_handle(cx);
        window.focused(cx).as_ref() == Some(&handle)
    });
    assert!(
        query_focused,
        "Ctrl-F path focuses the inline library search field"
    );
}

#[gpui_kit::test]
fn t03_drop_while_dialog_open_refuses_honestly(cx: &mut TestAppContext) {
    use gpui_kit::ExternalPaths;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let before = notification_count(cx);
    let paths = ExternalPaths(vec![std::path::PathBuf::from("E:\\x\\a.conf")].into());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.import_dropped_files(&paths, window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        before + 1,
        "blocked drop toasts honestly"
    );

    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));
    assert!(cx.update(|window, _| window.try_find("import-dialog-ok").is_none()));
}

#[gpui_kit::test]
fn f07_drop_into_open_import_dialog_fills_textarea(cx: &mut TestAppContext) {
    use gpui_kit::ExternalPaths;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let dir = std::env::temp_dir();
    let tag = format!("manyproxy-f07-open-{}", std::process::id());
    let dropped = dir.join(format!("{tag}-dropped.txt"));
    std::fs::write(&dropped, b"vless://dropped@d.example.com:443#dropped")
        .expect("temp write works");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog("", "travel", None, window, cx);
        });
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let before = notification_count(cx);
    let paths = ExternalPaths(vec![dropped.clone()].into());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.import_dropped_files(&paths, window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        before,
        "an open Import dialog absorbs the drop instead of refusing it"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view
            .import_form
            .clone()
            .expect("import form stashed")
            .text
            .read(cx)
            .value()
            .to_string()),
        "vless://dropped@d.example.com:443#dropped",
        "the drop lands in the same textarea Browse fills"
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the dialog stays up"
    );
    // `close_all_dialogs` clears the layer without running the dialog's
    // on_close, so the stash self-heals on the next drop instead: a fresh
    // dialog must carry only the new bytes, never the dead form's text.
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();
    let second = dir.join(format!("{tag}-second.txt"));
    std::fs::write(&second, b"trojan://second@s.example.com:443#second").expect("write works");
    let paths = ExternalPaths(vec![second.clone()].into());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.import_dropped_files(&paths, window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view
            .import_form
            .clone()
            .expect("fresh form stashed")
            .text
            .read(cx)
            .value()
            .to_string()),
        "trojan://second@s.example.com:443#second",
        "a stale stash never swallows the next drop"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    let _ = std::fs::remove_file(dropped);
    let _ = std::fs::remove_file(second);
}

#[gpui_kit::test]
fn ticket43_chrome_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    // The action bar owns Add/Import; the empty canvas does not repeat them.
    for id in [
        "action-bar-add",
        "action-bar-import",
        "engine-bar-start-all",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "{id} renders"
        );
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_sub_expanded("sub-1", window, cx));
    });
    assert!(
        view.read_with(cx, |view, _| view.expanded_subs.contains("sub-1")),
        "expand flips on"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.toggle_sub_expanded("sub-1", window, cx));
    });
    assert!(
        !view.read_with(cx, |view, _| view.expanded_subs.contains("sub-1")),
        "expand flips off"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);

    assert_painted(cx, "field-guided-server");
    assert_painted(cx, "field-guided-port");
    assert_absent(cx, "field-guided-identity".to_string());
    assert_painted(cx, "guided-hint");
    assert_painted(cx, "advanced-settings-caption");
    assert_absent(cx, "advanced-settings-section".to_string());
    assert_absent(cx, "field-params".to_string());
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    // K2: the guided rows stay put; the advanced panels unfold under the one
    // disclosure, and a guided protocol never grows a raw JSON textarea.
    assert_painted(cx, "s2-caption");
    assert_painted(cx, "tls-security-caption");
    assert_painted(cx, "s4-caption");
    assert_absent(cx, "field-params".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog("bogus://x", "g", None, window, cx)
        });
    });
    paint_dialog_open(cx);
    assert_painted(cx, "import-browse");
    assert_painted(cx, "import-first-failure");
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_settings_dialog(window, cx));
    });
    paint_dialog_open(cx);
    // T-04/B17: one modal, six sections in one column.
    assert_painted(cx, "settings-section-general");
    assert_painted(cx, "settings-body-general");
    assert_painted(cx, "settings-row-appearance");
    // K9: one footer pair, bottom-right; the old `Done` is gone.
    assert_painted(cx, "settings-dialog-cancel");
    assert_painted(cx, "settings-dialog-save");
    assert_absent(cx, "settings-dialog-done".to_string());
    scroll_settings_into_view(cx, "field-settings-test-url");
    assert_painted(cx, "field-settings-test-url");
    // K9: no mid-body Save; the one footer Save commits this section too.
    assert_absent(cx, "settings-save-test-url".to_string());
    assert_absent(cx, "settings-footer-probing".to_string());
    scroll_settings_into_view(cx, "settings-row-engine-sing-box");
    // K9/A7-5: lean engine rows + Details; no timer jargon.
    assert_painted(cx, "settings-engine-details-sing-box");
    assert_absent(cx, "settings-ticks".to_string());
    scroll_settings_into_view(cx, "settings-section-addresses");
    assert_painted(cx, "settings-section-addresses");
    scroll_settings_into_view(cx, "settings-clear-traffic");
    for id in [
        "settings-clear-traffic",
        "settings-clear-health",
        "settings-clear-snapshot",
        "settings-reset-library",
    ] {
        assert!(
            cx.update(|window, _| window.try_find(id).is_some()),
            "{id} renders"
        );
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "log-level-row");
    assert_painted(cx, "log-copy-visible");
    assert_painted(cx, "log-tail-toggle");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn laneb_settings_clears_confirm(cx: &mut TestAppContext) {
    use manyproxy_ui::{
        settings_clear_health_text, settings_clear_snapshot_text, settings_clear_traffic_text,
    };
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    assert!(
        settings_clear_traffic_text(3).contains("all 3 configs"),
        "traffic confirm names scope"
    );
    assert!(
        settings_clear_health_text().contains("untested until the next probe"),
        "health confirm names consequence"
    );
    assert!(
        settings_clear_snapshot_text().contains("live engine is untouched"),
        "snapshot confirm names consequence"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_clear_traffic_confirm(window, cx));
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "traffic wipe confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_clear_health_confirm(window, cx));
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "health wipe confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_clear_snapshot_confirm(window, cx));
    });
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "snapshot reset confirms"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn empty_start_and_stopped_stop_warn_honestly(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let rows = view.store.list().expect("list works");
            for row in &rows {
                view.store.remove(&row.id).expect("remove works");
            }
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    assert_eq!(notification_count(cx), 1, "empty start warns once");
    assert_eq!(
        view.read_with(cx, |view, _| view.engine.running_count()),
        0,
        "nothing starts"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store.add(seed_input("Work")).expect("seed works");
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.reload(window, cx));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.stop_selected_engine(window, cx));
    });
    assert_eq!(notification_count(cx), 2, "stop-on-stopped warns once");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.stop_all_engines(window, cx));
    });
    assert_eq!(notification_count(cx), 3, "idle stop-all warns once");
}

#[test]
fn library_search_matches_host_bind_port() {
    use manyproxy_core::library_row_matches;
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(name: &str, server: &str, bind: &str, port: u16) -> ProxyConfig {
        ProxyConfig {
            id: format!("id-{name}"),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: format!("{{\"type\":\"vless\",\"server\":\"{server}\",\"server_port\":443}}")
                .into(),
            group: "default".to_string(),
            enabled: false,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let alpha = row("Alpha", "a.example.com", "127.0.0.1", 10801);
    let beta = row("Beta", "10.0.0.5", "10.0.0.5", 10802);
    assert!(library_row_matches(&beta, "10.0.0.5"), "bind/host match");
    assert!(library_row_matches(&alpha, "10801"), "port match");
    assert!(!library_row_matches(&beta, "a.example.com"));
    assert!(library_row_matches(&alpha, "a.example.com"), "host match");
    assert!(library_row_matches(&alpha, "alpha"), "name still matches");
    assert!(
        library_row_matches(&alpha, "vless"),
        "protocol still matches"
    );
    assert!(!library_row_matches(&alpha, "nope"), "no false positive");

    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![alpha, beta]);
    let mut query = manyproxy_ui::FilterQuery {
        search: "10.0.0.5".to_string(),
        ..Default::default()
    };
    assert_eq!(
        test_visible(&delegate, &query).len(),
        1,
        "filter narrows to the bind/host row"
    );
    query.search = "10801".to_string();
    assert_eq!(
        test_visible(&delegate, &query).len(),
        1,
        "filter narrows by port"
    );
    query.search.clear();
    assert_eq!(
        test_visible(&delegate, &query).len(),
        2,
        "empty query shows all"
    );
}

#[test]
fn t47_search_index_matches_scan_parity() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(
        id: &str,
        name: &str,
        protocol: &str,
        params: &str,
        group: &str,
        bind: &str,
        port: u16,
    ) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string().into(),
            group: group.to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let rows = vec![
        row(
            "1",
            "Alpha",
            "vless",
            r#"{"server":"a.example.com","server_port":443}"#,
            "g1",
            "127.0.0.1",
            10801,
        ),
        row(
            "2",
            "Beta",
            "trojan",
            r#"{"address":"10.0.0.9","port":"8443"}"#,
            "G2",
            "::1",
            10802,
        ),
        row(
            "3",
            "Gamma Ray",
            "WIREGUARD",
            "not json{{{",
            "",
            "0.0.0.0",
            10803,
        ),
        row(
            "4",
            "Unicodae",
            "ss",
            r#"{"server":"X.EXAMPLE.COM","server_port":8388,"password":"s3cret"}"#,
            "vpn",
            "192.168.1.2",
            10804,
        ),
        row(
            "5",
            "both-hosts",
            "vless",
            r#"{"server":"nomatch.invalid","address":"matchme.invalid"}"#,
            "g1",
            "127.0.0.1",
            10805,
        ),
        row(
            "6",
            "numport",
            "tuic",
            r#"{"server":"h.example.com","server_port":12345}"#,
            "g3",
            "127.0.0.1",
            10806,
        ),
    ];
    for query in [
        "",
        "  ",
        "ALPHA",
        "vless",
        "WIREGUARD",
        "a.example.com",
        "X.EXAMPLE",
        "1080",
        "8443",
        "g1",
        "G2",
        "::1",
        "0.0.0.0",
        "gamma ray",
        "matchme",
        "nomatch",
        "s3cret",
        "nope",
        "alpha vless",
        "8388",
    ] {
        let mut delegate = LibraryDelegate::new();
        delegate.set_rows(rows.clone());
        let filter_query = manyproxy_ui::FilterQuery {
            search: query.to_string(),
            ..Default::default()
        };
        let cached: Vec<String> = test_visible(&delegate, &filter_query)
            .into_iter()
            .map(|i| delegate.rows[i].id.clone())
            .collect();
        let expected: Vec<String> = rows
            .iter()
            .filter(|r| manyproxy_core::library_row_matches(r, query))
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(cached, expected, "index parity for query {query:?}");
    }
}

#[test]
fn t47_search_index_matches_full_with_context() {
    use manyproxy_core::{library_row_matches_full, MatchContext};
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(id: &str, name: &str, enabled: bool, sub: Option<&str>) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: r#"{"server":"a.example.com","server_port":443}"#.to_string().into(),
            group: "g".to_string(),
            enabled,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: sub.map(str::to_string),
            identity_key: None,
            preset_id: None,
        }
    }
    let rows = vec![
        row("1", "Alpha", true, Some("s1")),
        row("2", "Beta", false, None),
    ];
    let mut ctx = MatchContext::default();
    ctx.sub_labels
        .insert("s1".to_string(), "Nightly Sub".to_string());
    ctx.running.insert("1".to_string());
    ctx.health.insert("1".to_string(), "87ms".to_string());
    ctx.health.insert("2".to_string(), "dead".to_string());
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(rows.clone());
    delegate.set_running(vec!["1".to_string()]);
    delegate.set_health(ctx.health.clone());
    delegate.set_search_context(ctx.sub_labels.clone(), ctx.preset_names.clone());
    for query in [
        "nightly",
        "sub",
        "enabled",
        "disabled",
        "running",
        "stopped",
        "live",
        "dead",
        "87ms",
        "untested",
        "alpha",
        "zzz-no-match",
    ] {
        let filter_query = manyproxy_ui::FilterQuery {
            search: query.to_string(),
            ..Default::default()
        };
        let cached: Vec<String> = test_visible(&delegate, &filter_query)
            .into_iter()
            .map(|i| delegate.rows[i].id.clone())
            .collect();
        let expected: Vec<String> = rows
            .iter()
            .filter(|r| library_row_matches_full(r, query, &ctx))
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(cached, expected, "index/full parity for {query:?}");
    }
}

#[test]
fn t73_port_filter_exact_and_ranges() {
    use manyproxy_ui::PortPredicate;

    assert_eq!(
        manyproxy_ui::parse_port_predicate("port=10801:10910"),
        Some(PortPredicate {
            lo: 10801,
            hi: 10910
        }),
        "range token parses"
    );
    assert_eq!(
        manyproxy_ui::parse_port_predicate("port=10801"),
        Some(PortPredicate {
            lo: 10801,
            hi: 10801
        }),
        "exact token is a one-wide range"
    );

    let (exact, parsed) = t73_filtered(t73_rows(), "port=10801");
    assert_eq!(exact, vec![10801], "exact port matches only that port");
    assert_eq!(
        parsed.port,
        Some(PortPredicate {
            lo: 10801,
            hi: 10801
        }),
        "the token seeds the port facet"
    );
    assert!(
        parsed.search_text().is_empty(),
        "the token leaves free text"
    );

    let (range, parsed) = t73_filtered(t73_rows(), "port=10801:10910");
    assert_eq!(range, vec![10801, 10802, 10910], "range is inclusive");
    assert_eq!(
        parsed.port,
        Some(PortPredicate {
            lo: 10801,
            hi: 10910
        }),
        "the range is inclusive"
    );

    let (case_insensitive, _) = t73_filtered(t73_rows(), "PORT=10802");
    assert_eq!(
        case_insensitive,
        vec![10802],
        "port key is case-insensitive"
    );

    let (reversed, parsed) = t73_filtered(t73_rows(), "port=10910:10801");
    assert!(
        reversed.is_empty(),
        "reversed bounds never behave as a range"
    );
    assert_eq!(parsed.port, None, "reversed bounds stay literal text");
    assert_eq!(
        parsed.search_text(),
        "port=10910:10801",
        "the malformed token remains search text"
    );

    let (as_text, _) = t73_filtered(t73_rows(), "1080");
    assert_eq!(
        as_text,
        vec![10801, 10802],
        "bare digits keep today's substring behavior"
    );
}

#[test]
fn t73_port_filter_open_ended_forms() {
    let (open_top, parsed) = t73_filtered(t73_rows(), "port=10801:");
    assert_eq!(open_top, vec![10801, 10802, 10910, 20000], "N..=65535");
    assert_eq!(
        parsed.port,
        Some(manyproxy_ui::PortPredicate {
            lo: 10801,
            hi: u16::MAX
        }),
        "open top reaches the ceiling"
    );

    let (open_bottom, parsed) = t73_filtered(t73_rows(), "port=:10802");
    assert_eq!(open_bottom, vec![10801, 10802], "0..=M");
    assert_eq!(
        parsed.port,
        Some(manyproxy_ui::PortPredicate { lo: 0, hi: 10802 }),
        "open bottom starts at the floor"
    );

    let (everything, _) = t73_filtered(t73_rows(), "port=0:");
    assert_eq!(
        everything,
        vec![10801, 10802, 10910, 20000],
        "0: spans every u16 port"
    );

    let (ceiling, _) = t73_filtered(t73_rows(), "port=65535:");
    assert!(ceiling.is_empty(), "65535: is a valid empty range");
}

#[test]
fn t73_malformed_port_token_renders_literal() {
    use manyproxy_ui::LibraryDelegate;

    for token in [
        "port=",
        "port=x",
        "port=:",
        "port=5:1",
        "port=65536",
        "port=10801-10802",
        "port",
    ] {
        assert_eq!(
            manyproxy_ui::parse_port_predicate(token),
            None,
            "{token:?} is not a port predicate"
        );
    }

    let rows = vec![
        t73_port_row("a", "Alpha", "vless", "g1", 10801),
        t73_port_row("b", "port=x", "vless", "g1", 10803),
        t73_port_row("c", "port=5:1", "vless", "g1", 10804),
    ];

    let (hits, parsed) = t73_filtered(rows.clone(), "port=x");
    assert_eq!(
        hits,
        vec![10803],
        "port=x matches the row literally named port=x, not by port"
    );
    assert_eq!(parsed.port, None, "a malformed token stays literal text");
    assert_eq!(parsed.search_text(), "port=x");

    let (reversed, _) = t73_filtered(rows.clone(), "port=5:1");
    assert_eq!(
        reversed,
        vec![10804],
        "a reversed range is literal text, not an error"
    );

    let (prefix, _) = t73_filtered(rows.clone(), "port=");
    assert_eq!(
        prefix,
        vec![10803, 10804],
        "port= is a literal substring of both names"
    );

    let (nonsense, _) = t73_filtered(rows.clone(), "port=:");
    assert!(nonsense.is_empty(), "port=: neither parses nor matches");

    let (overflow, _) = t73_filtered(rows.clone(), "port=65536");
    assert!(
        overflow.is_empty(),
        "out-of-u16 value is literal, not a panic"
    );

    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(rows);
    let mut parsed = manyproxy_ui::FilterQuery::default();
    parsed.set_search("port=");
    assert_eq!(
        parsed.search, "port=",
        "the query string itself is never rewritten"
    );
    assert_eq!(parsed.port, None, "port= is not a predicate");
}

#[test]
fn t73_port_composes_with_facets() {
    use manyproxy_ui::{FilterQuery, LibraryDelegate, StatusFacet};

    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        t73_port_row("1", "Alpha", "vless", "g1", 10801),
        t73_port_row("2", "Alpha Two", "vless", "g2", 10802),
        t73_port_row("3", "Beta", "vmess", "g1", 10803),
    ]);
    let mut query = FilterQuery::default();
    query.set_search("port=10801:10803 alpha");
    assert_eq!(
        t73_visible_ports(&delegate, &query),
        vec![10801, 10802],
        "the text part keeps today's substring behavior inside the range"
    );

    query.protocols.insert("vmess".to_string());
    assert!(
        t73_visible_ports(&delegate, &query).is_empty(),
        "the protocol facet ANDs with the port predicate and the text"
    );
    query.protocols.clear();
    query.protocols.insert("vless".to_string());
    assert_eq!(
        t73_visible_ports(&delegate, &query),
        vec![10801, 10802],
        "vless keeps the two alpha rows"
    );
    query.protocols.clear();

    query.set_search("port=10801:10803");
    delegate.set_running(vec!["3".to_string()]);
    delegate.set_health(
        [("3".to_string(), "dead".to_string())]
            .into_iter()
            .collect(),
    );
    query.statuses.insert(StatusFacet::Dead);
    assert_eq!(t73_visible_ports(&delegate, &query), vec![10803]);
    query.statuses.clear();

    query.set_search("port=10801:10803 port=99999");
    assert!(
        t73_visible_ports(&delegate, &query).is_empty(),
        "a second port= token degrades to literal text (pinned)"
    );

    query.set_search("port=10801:10803");
    let mut unsorted = t73_visible_ports(&delegate, &query);
    delegate.set_sort(manyproxy_ui::store::LibrarySort::new(
        manyproxy_ui::store::LibrarySortKey::TypeSecurity,
        manyproxy_ui::store::SortDirection::Ascending,
    ));
    let mut sorted = t73_visible_ports(&delegate, &query);
    unsorted.sort_unstable();
    sorted.sort_unstable();
    assert_eq!(
        unsorted, sorted,
        "the type/security sort only reorders what the filter kept"
    );
}

/// R14: the search is session-only. Typing narrows the list but leaves no
/// meta row behind, and a restart opens with an empty field and the whole
/// library.
#[gpui_kit::test]
fn search_is_not_persisted(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;

    cx.update(gpui_kit::init);
    let path = std::env::temp_dir().join("manyproxy-slice3-session-search.db");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    {
        let mut store = Store::open_file(&path).expect("file store opens");
        store
            .add(facet_seed("Alpha", "vless", "travel", "10801"))
            .expect("seed works");
        store
            .add(facet_seed("Beta", "vless", "travel", "10802"))
            .expect("seed works");
    }

    let store = Store::open_file(&path).expect("reopen works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let handle = view.read_with(cx, |view, cx| view.search_input.read(cx).focus_handle(cx));
    cx.update(|window, cx| {
        window.focus(&handle, cx);
    });
    cx.simulate_input("port=10801");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(80));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.visible.len()),
        1,
        "the typed port filter applies"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view
            .store
            .get_meta("library_last_query")
            .expect("meta read")),
        None,
        "the search never writes a meta row"
    );

    drop(view);
    let store = Store::open_file(&path).expect("restart reopen works");
    let (restarted, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        restarted.read_with(cx, |view, cx| view
            .search_input
            .read(cx)
            .value()
            .to_string()),
        "",
        "a restart opens with an empty search field"
    );
    assert_eq!(
        restarted.read_with(cx, |view, _| view.visible.len()),
        2,
        "the whole library is visible after a restart"
    );
    drop(restarted);
    let _ = std::fs::remove_file(&path);
}

/// OR within a facet, AND across facets and the search. Counts are
/// self-excluded: each menu still sees what the other facets keep.
#[test]
fn t44_facets_and_with_text_query() {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::{FilterQuery, LibraryDelegate, StatusFacet};
    fn row(id: &str, name: &str, protocol: &str, group: &str, enabled: bool) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: r#"{"server":"a.example.com","server_port":443}"#.to_string().into(),
            group: group.to_string(),
            enabled,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let rows = vec![
        row("1", "Alpha", "vless", "team", true),
        row("2", "Beta", "vmess", "team", false),
        row("3", "Gamma", "vless", "lab", true),
    ];
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(rows);
    delegate.set_running(vec!["1".to_string()]);
    let mut health = std::collections::HashMap::new();
    health.insert("1".to_string(), "87ms".to_string());
    health.insert("3".to_string(), "dead".to_string());
    delegate.set_health(health);

    let ids = |delegate: &LibraryDelegate, query: &FilterQuery| {
        test_visible(delegate, query)
            .into_iter()
            .map(|index| delegate.rows[index].id.clone())
            .collect::<Vec<_>>()
    };

    let mut query = FilterQuery::default();
    query.set_search("vless");
    assert_eq!(test_visible(&delegate, &query).len(), 2);

    query.protocols.insert("vless".to_string());
    query.statuses.insert(StatusFacet::Enabled);
    assert_eq!(
        ids(&delegate, &query),
        vec!["1".to_string(), "3".to_string()]
    );

    query.statuses.insert(StatusFacet::Disabled);
    assert_eq!(
        ids(&delegate, &query),
        vec!["1".to_string(), "3".to_string()],
        "OR inside the status set: the protocol still excludes Beta"
    );

    query.protocols.clear();
    query.set_search("");
    query.protocols.insert("vmess".to_string());
    assert_eq!(
        ids(&delegate, &query),
        vec!["2".to_string()],
        "the protocol set ANDs with the status set"
    );

    query.protocols.clear();
    query.statuses.clear();
    query.statuses.insert(StatusFacet::Running);
    assert_eq!(ids(&delegate, &query), vec!["1".to_string()]);

    // Self-exclusion: the status menu counts the statuses with its own set
    // released but the search still applied.
    let mut query = FilterQuery::default();
    query.set_search("vless");
    let counts = manyproxy_ui::status_counts(
        &delegate.rows,
        &delegate.search_index,
        &test_facts(&delegate),
        &manyproxy_ui::LibraryScope::All,
        &query,
    );
    let count = |status: StatusFacet| {
        counts
            .iter()
            .find(|(candidate, _)| *candidate == status)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    };
    assert_eq!(count(StatusFacet::Enabled), 2);
    assert_eq!(count(StatusFacet::Disabled), 0, "the search excludes Beta");

    query.statuses.insert(StatusFacet::Enabled);
    let protocols = manyproxy_ui::protocol_counts(
        &delegate.rows,
        &delegate.search_index,
        &test_facts(&delegate),
        &manyproxy_ui::LibraryScope::All,
        &query,
    );
    assert_eq!(
        protocols,
        vec![("vless".to_string(), 2), ("vmess".to_string(), 0)],
        "the protocol menu ignores its own set but respects the status set"
    );

    query = FilterQuery::default();
    assert_eq!(test_visible(&delegate, &query).len(), 3);
    query.set_search("a");
    assert_eq!(test_visible(&delegate, &query).len(), 3);
}

#[test]
fn t09_show_security_and_type_security_sort() {
    use manyproxy_ui::apply_show_security;
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::LibraryDelegate;
    fn row(id: &str, name: &str, protocol: &str, params: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string().into(),
            group: "g".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }
    let tls_row = row("1", "A", "vless", r#"{"security":"tls"}"#);
    let mut cell = "a.example.com:443".to_string();
    apply_show_security(&mut cell, &tls_row);
    assert_eq!(cell, "a.example.com:443 · tls", "posture spliced");
    let mut plain = "b.example.com:443".to_string();
    apply_show_security(&mut plain, &row("2", "B", "vmess", "{}"));
    assert_eq!(plain, "b.example.com:443 · none", "plain protocols labeled");
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![
        row("1", "T", "trojan", "{}"),
        row("2", "Vtls", "vless", r#"{"security":"tls"}"#),
        row("3", "Vreal", "vless", r#"{"security":"reality"}"#),
    ]);
    let order =
        |delegate: &LibraryDelegate| test_names(delegate, &manyproxy_ui::FilterQuery::default());
    assert_eq!(order(&delegate), vec!["T", "Vtls", "Vreal"]);
    // T-27: the toggle retired into the sort picker; the equivalent key
    // (direction is fixed for it) must order exactly as the toggle did.
    delegate.set_sort(manyproxy_ui::store::LibrarySort::new(
        manyproxy_ui::store::LibrarySortKey::TypeSecurity,
        manyproxy_ui::store::SortDirection::Ascending,
    ));
    assert_eq!(order(&delegate), vec!["T", "Vreal", "Vtls"]);
    assert_eq!(
        delegate
            .rows
            .iter()
            .map(|row| row.name.clone())
            .collect::<Vec<_>>(),
        vec!["T", "Vtls", "Vreal"],
        "store order untouched"
    );
    assert!(!delegate.show_security);
    delegate.set_show_security(true);
    assert!(delegate.show_security);
}

#[test]
fn t09_group_toast_texts_name_scope() {
    use manyproxy_ui::group_test_summary_toast;
    assert_eq!(
        group_test_summary_toast("g", 3, 2, 1, false),
        "Tested 3 endpoints in group \"g\": 2 live · 1 dead"
    );
    assert!(
        group_test_summary_toast("g", 3, 1, 0, true).contains("(stopped early)"),
        "stop marked"
    );
}

/// B2: the render path borrows rows so a frame does not clone a config per
/// visible row; `row_at` stays the owning accessor for callers that need it.
/// B2: the render path borrows rows so a frame does not clone a config per
/// visible row.
#[test]
fn library_row_ref_borrows_the_stored_row() {
    let mut delegate = LibraryDelegate::new();
    delegate.set_rows(vec![ProxyConfig {
        id: "1".to_string(),
        name: "Alpha".to_string(),
        protocol: "vless".to_string(),
        params: r#"{"server":"a.example.com"}"#.to_string().into(),
        group: "g".to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    }]);
    let borrowed = delegate.row_by_id("1").expect("row is found");
    assert!(
        std::ptr::eq(borrowed, &delegate.rows[0]),
        "render reads the stored row, not a copy"
    );
    assert_eq!(delegate.rows[0].name, "Alpha");
    assert_eq!(
        delegate.row_by_id("1").map(|row| row.id.clone()),
        Some("1".to_string()),
        "the row is found by id"
    );
}
