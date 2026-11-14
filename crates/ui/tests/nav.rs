//! nav: one binary of the split UI suite. Each binary runs as its own
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

#[gpui_kit::test]
fn nav_lists_all_configs_groups_ungrouped_and_sources(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "Home", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "Home", "10802"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vless", "Work", "10803"))
        .expect("seed works");
    store.add(seed_input("Loose")).expect("seed works");
    let sub = store
        .add_subscription("Free plan", "https://example.com/free.txt", "Home", false)
        .expect("sub works");
    store
        .refresh_with_body(
            &sub.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#feed",
            500,
        )
        .expect("refresh works");
    let sub_id = sub.id.clone();
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    for id in [
        "nav-all-configs",
        "nav-all-configs-count",
        "nav-groups-header",
        "nav-group-Home",
        "nav-group-Work",
        "nav-ungrouped",
        "nav-sources-header",
        "nav-routing-presets",
        "nav-settings",
    ] {
        assert_painted(cx, id);
    }
    assert_painted(cx, &format!("nav-source-{sub_id}"));

    assert_eq!(label_of("nav-all-configs-count", cx), "5");
    assert_eq!(label_of("nav-group-Home-count", cx), "3");
    assert_eq!(label_of("nav-group-Work-count", cx), "1");
    assert_eq!(label_of("nav-ungrouped-count", cx), "1");
    assert_eq!(label_of(&format!("nav-source-{sub_id}-count"), cx), "1");
    assert_eq!(label_of("nav-group-Home", cx), "Home");
    assert_eq!(label_of(&format!("nav-source-{sub_id}"), cx), "Free plan");

    assert!(
        nav_y("nav-all-configs", cx) < nav_y("nav-groups-header", cx),
        "All configs sits first"
    );
    assert!(
        nav_y("nav-group-Home", cx) < nav_y("nav-group-Work", cx),
        "groups are alphabetical"
    );
    assert!(
        nav_y("nav-group-Work", cx) < nav_y("nav-ungrouped", cx),
        "Ungrouped sits last"
    );
    assert!(
        nav_y("nav-ungrouped", cx) < nav_y("nav-sources-header", cx),
        "sources follow the groups"
    );
    assert!(
        nav_y("nav-sources-header", cx) < nav_y("nav-routing-presets", cx),
        "the utilities sit at the bottom"
    );
}

#[gpui_kit::test]
fn nav_counts_are_scope_totals_and_ignore_the_filter(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "Home", "10811"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "Home", "10812"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vless", "Work", "10813"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Group("Home".to_string()), window, cx)
        });
    });
    paint_frame_at(cx, 1440., 900.);
    assert_eq!(
        view.read_with(cx, |view, cx| view.filter_count_text(cx)),
        "2 of 2",
        "the whole scope is visible before a search"
    );

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("Alpha".to_string(), cx)
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    assert_eq!(
        view.read_with(cx, |view, cx| view.filter_count_text(cx)),
        "1 of 2",
        "N narrows, M stays the scope total"
    );
    assert_eq!(
        label_of("nav-group-Home-count", cx),
        "2",
        "the nav count ignores the search"
    );
    assert_eq!(label_of("nav-group-Work-count", cx), "1");
    assert_eq!(label_of("nav-all-configs-count", cx), "3");
}

#[gpui_kit::test]
fn switching_scope_clears_the_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "Home", "10821"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "Home", "10822"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vless", "Work", "10823"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[alpha.id.clone(), bravo.id.clone()], cx)
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_ids()),
        vec![alpha.id.clone(), bravo.id.clone()]
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Group("Work".to_string()), window, cx)
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    view.read_with(cx, |view, _| {
        assert!(view.selection.is_empty(), "the old scope's rows are gone");
        assert!(view.selection_anchor.is_none(), "the anchor goes");
        assert!(view.selection_focus.is_none(), "the focus goes");
    });
    let visible = view.read_with(cx, |view, cx| view.visible_row_ids(cx));
    assert_eq!(visible.len(), 1, "only the Work row is visible");
}

#[gpui_kit::test]
fn scope_change_keeps_search_status_and_protocol(cx: &mut TestAppContext) {
    use manyproxy_ui::StatusFacet;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "Home", "10831"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "Work", "10832"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_search_now("Alpha".to_string(), cx);
            view.set_status_facet(StatusFacet::Enabled, true, cx);
            view.set_protocol_facet("vless".to_string(), true, cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Group("Work".to_string()), window, cx)
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.query.search, "Alpha");
        assert!(view.query.statuses.contains(&StatusFacet::Enabled));
        assert!(view.query.protocols.contains("vless"));
    });
    // The facet state is unchanged; Home has no Alpha, so the scope is empty.
    assert_eq!(
        view.read_with(cx, |view, cx| view.filter_count_text(cx)),
        "0 of 1"
    );
}

#[gpui_kit::test]
fn filter_row_counts_inside_the_scope(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "Home", "10841"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "Home", "10842"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vless", "Work", "10843"))
        .expect("seed works");
    store.set_enabled(&alpha.id, true).expect("enable works");
    store.set_enabled(&bravo.id, true).expect("enable works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Group("Home".to_string()), window, cx)
        });
    });
    paint_frame_at(cx, 1440., 900.);
    assert_eq!(
        view.read_with(cx, |view, cx| view.filter_count_text(cx)),
        "2 of 2"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.scope_label()),
        "Home",
        "the action bar names the scope"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.scope_enabled),
        2,
        "the action bar's launch count is inside the scope"
    );
}

#[gpui_kit::test]
fn new_group_appears_with_zero_count_and_refuses_the_reserved_name(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "Zulu", "10851"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    let reserved = cx
        .update(|window, cx| view.update(cx, |view, cx| view.create_group("default", window, cx)));
    assert!(!reserved, "the default name is reserved");
    assert_eq!(notification_count(cx), 1, "the refusal names itself");

    let created =
        cx.update(|window, cx| view.update(cx, |view, cx| view.create_group("Alpha", window, cx)));
    assert!(created, "a fresh group is created");
    assert_eq!(
        view.read_with(cx, |view, _| view.scope.clone()),
        manyproxy_ui::LibraryScope::Group("Alpha".to_string()),
        "the new group becomes the scope"
    );

    let duplicate =
        cx.update(|window, cx| view.update(cx, |view, cx| view.create_group("Alpha", window, cx)));
    assert!(!duplicate, "a duplicate is refused");
    assert_eq!(notification_count(cx), 2);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::All, window, cx)
        });
    });
    paint_frame_at(cx, 1440., 900.);
    assert_painted(cx, "nav-group-Alpha");
    assert_eq!(label_of("nav-group-Alpha-count", cx), "0");
    assert!(
        nav_y("nav-group-Alpha", cx) < nav_y("nav-group-Zulu", cx),
        "a new group lands in alphabetical order"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_new_group_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "group-new-ok");
    assert_painted(cx, "field-group-new-field");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn group_manage_sheet_renames_and_deletes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "Home", "10861"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Group("Home".to_string()), window, cx);
            view.rename_group_scope("Home", "Office", window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    assert_eq!(
        view.read_with(cx, |view, _| view.scope.clone()),
        manyproxy_ui::LibraryScope::Group("Office".to_string()),
        "the scope follows the rename"
    );
    assert_painted(cx, "nav-group-Office");
    assert_absent(cx, "nav-group-Home".to_string());

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_manage_sheet("Office", window, cx)
        });
    });
    paint_dialog_open(cx);
    for id in [
        "group-manage-rename",
        "group-manage-edit",
        "group-manage-refresh",
        "group-manage-auto-clear-switch",
        "group-manage-prune-0",
        "group-manage-prune-1",
        "group-manage-prune-2",
        "group-manage-prune-3",
        "group-manage-delete",
        "group-manage-ok",
    ] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.delete_group_scope("Office", window, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        notification_count(cx),
        before + 1,
        "a group that still holds a row refuses"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.group_counts().unwrap()),
        vec![("Office".to_string(), 1)]
    );
}

#[gpui_kit::test]
fn group_delete_still_refuses_while_rows_remain(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(facet_seed("Alpha", "vless", "Home", "10871"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.delete_group_scope("Home", window, cx));
    });
    assert_eq!(notification_count(cx), 1, "the refusal notifies");
    assert_eq!(
        view.read_with(cx, |view, _| view.store.group_counts().unwrap()),
        vec![("Home".to_string(), 1)],
        "the row stays"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.store
                .move_to_group(std::slice::from_ref(&row.id), "Work")
                .expect("move works");
            view.reload(window, cx);
            view.delete_group_scope("Home", window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.store.group_counts().unwrap()),
        vec![("Work".to_string(), 1)],
        "an empty group deletes"
    );
}

#[gpui_kit::test]
fn drag_row_into_group_moves_that_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "Home", "10881"))
        .expect("seed works");
    let bravo = store
        .add(facet_seed("Bravo", "vless", "Home", "10882"))
        .expect("seed works");
    store
        .add(facet_seed("Charlie", "vless", "Work", "10883"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    assert_painted(cx, &format!("row-surface-{}", alpha.id));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_rows(&[alpha.id.clone(), bravo.id.clone()], cx)
        });
    });

    cx.update(|window, cx| {
        window.drag_to(
            format!("row-surface-{}", alpha.id),
            "nav-group-Work".to_string(),
            cx,
        );
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    let groups: Vec<(String, String)> = view.read_with(cx, |view, _| {
        view.store
            .list()
            .expect("list works")
            .into_iter()
            .map(|row| (row.name, row.group))
            .collect()
    });
    assert!(
        groups.contains(&("Alpha".to_string(), "Work".to_string())),
        "the dragged row moved: {groups:?}"
    );
    assert!(
        groups.contains(&("Bravo".to_string(), "Home".to_string())),
        "only the dragged row moved: {groups:?}"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.scope.clone()),
        manyproxy_ui::LibraryScope::All,
        "a drop never changes the scope"
    );
    assert_eq!(notification_count(cx), 1, "the move toasts once");
}

#[gpui_kit::test]
fn dropping_on_a_group_the_row_is_already_in_changes_nothing(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "Home", "10891"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    cx.update(|window, cx| {
        window.drag_to(
            format!("row-surface-{}", alpha.id),
            "nav-group-Home".to_string(),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.store
                .list()
                .expect("list works")
                .into_iter()
                .map(|row| row.group)
                .collect::<Vec<_>>()
        }),
        vec!["Home".to_string()],
        "the row did not move"
    );
    assert_eq!(notification_count(cx), 1, "the no-op still says so");
}

#[gpui_kit::test]
fn source_strip_shows_freshness_counts_quota_and_traffic(cx: &mut TestAppContext) {
    use manyproxy_ui::store::{now_unix, parse_subscription_userinfo};
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let sub = store
        .add_subscription("Free plan", "https://example.com/free.txt", "Home", false)
        .expect("sub works");
    let now = now_unix();
    store
        .refresh_with_body(
            &sub.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#feed",
            now,
        )
        .expect("refresh works");
    store
        .set_subscription_quota(
            &sub.id,
            parse_subscription_userinfo("upload=2048; download=0; total=1048576").map(
                |mut quota| {
                    // Half a day of slack keeps the "4d left" rounding stable
                    // no matter when in the day the test runs.
                    quota.expire = Some(now + 4 * 86_400 + 43_200);
                    quota
                },
            ),
        )
        .expect("quota works");
    let sub_id = sub.id.clone();
    let row_id = store
        .list()
        .expect("list works")
        .into_iter()
        .find(|row| row.subscription_id.as_deref() == Some(sub_id.as_str()))
        .expect("the source feeds a row")
        .id;
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.libraries.update(cx, |state, list_cx| {
                let delegate = state.delegate_mut();
                delegate.running.insert(row_id.clone());
                delegate.traffic.insert(row_id.clone(), (100, 200));
                list_cx.notify();
            });
            view.refresh_scope_index(cx);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Source(sub_id.clone()), window, cx)
        });
    });
    paint_frame_at(cx, 1440., 900.);

    assert_painted(cx, "scope-strip");
    assert_eq!(label_of("scope-strip-name", cx), "Free plan");
    assert_eq!(label_of("scope-strip-status", cx), "Fresh");
    assert_eq!(label_of("scope-strip-configs", cx), "1 config");
    assert_eq!(label_of("scope-strip-running", cx), "1 running");
    assert!(
        label_of("scope-strip-updated", cx).starts_with("Updated "),
        "the strip says when the source last refreshed"
    );
    let quota = label_of("scope-strip-quota", cx);
    assert!(quota.contains("Used 2.0 KB of 1.0 MB"), "{quota}");
    assert!(quota.contains("Expires 4d left"), "{quota}");
    assert_eq!(label_of("scope-strip-traffic", cx), "↓100 B ↑200 B");
    assert_painted(cx, "scope-strip-refresh");
    assert_painted(cx, "scope-strip-manage");
    assert_painted(cx, "scope-strip-all");

    cx.update(|window, cx| {
        window.click("scope-strip-all", cx);
    });
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    assert_absent(cx, "scope-strip".to_string());
    assert_eq!(
        view.read_with(cx, |view, _| view.scope.clone()),
        manyproxy_ui::LibraryScope::All
    );
}

#[gpui_kit::test]
fn nav_source_refresh_refreshes_that_source_only(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let free = store
        .add_subscription("Free plan", "https://example.com/free.txt", "Home", false)
        .expect("sub works");
    let paid = store
        .add_subscription("Paid plan", "https://example.com/paid.txt", "Work", false)
        .expect("sub works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    assert_painted(cx, &format!("nav-source-refresh-{}", free.id));

    cx.update(|window, cx| {
        window.click(format!("nav-source-refresh-{}", free.id), cx);
    });
    let inflight = view.read_with(cx, |view, _| view.refresh.inflight.clone());
    assert!(inflight.contains(&free.id), "the clicked source refreshes");
    assert!(!inflight.contains(&paid.id), "the other source stays put");
}

#[gpui_kit::test]
fn sources_manage_opens_the_existing_sheet(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    cx.update(|window, cx| window.click("nav-manage-sources", cx));
    paint_dialog_open(cx);
    assert_painted(cx, "subscriptions-dialog-ok");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn routing_presets_utility_opens_the_presets_sheet(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);
    cx.update(|window, cx| window.click("nav-routing-presets", cx));
    paint_dialog_open(cx);
    assert_painted(cx, "presets-dialog-ok");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn nav_at_56px_keeps_every_entry_reachable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "Home", "10901"))
        .expect("seed works");
    store
        .add(facet_seed("Bravo", "vless", "Work", "10902"))
        .expect("seed works");
    store.add(seed_input("Loose")).expect("seed works");
    let sub = store
        .add_subscription("Free plan", "https://example.com/free.txt", "Home", false)
        .expect("sub works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1199., 700.);

    let width = cx.update(|window, _| {
        window
            .try_find("nav".to_string())
            .expect("the nav renders")
            .bounds()
            .size
            .width
    });
    assert_eq!(width, px(56.), "the nav collapses");
    for id in [
        "nav-all-configs-letter".to_string(),
        "nav-group-Home-letter".to_string(),
        "nav-group-Work-letter".to_string(),
        "nav-ungrouped-letter".to_string(),
        format!("nav-source-{sub_id}-dot"),
        "nav-new-group-row".to_string(),
        "nav-manage-sources-row".to_string(),
        "nav-routing-presets-letter".to_string(),
        "nav-settings-letter".to_string(),
    ] {
        assert_painted(cx, &id);
    }
    assert_absent(cx, "nav-all-configs-label".to_string());
    assert_absent(cx, "nav-group-Home-label".to_string());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for index in 0..40 {
                view.created_groups.push(format!("Group {index:02}"));
            }
            view.refresh_scope_index(cx);
        });
    });
    paint_frame_at(cx, 1199., 700.);
    let max = view.read_with(cx, |view, _| view.nav_scroll.max_offset());
    assert!(
        max.y > px(0.),
        "a long group list scrolls: max offset {max:?}"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.nav_scroll.set_offset(point(px(0.), max.y));
            cx.notify();
        });
    });
    paint_frame_at(cx, 1199., 700.);
    assert_painted(cx, "nav-routing-presets-letter");
    assert_painted(cx, "nav-settings-letter");
}

#[gpui_kit::test]
fn update_current_scope_refreshes_the_scoped_source_or_group(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let work = store
        .add_subscription("Work feed", "https://example.com/work.txt", "Work", false)
        .expect("sub works");
    let home = store
        .add_subscription("Home feed", "https://example.com/home.txt", "Home", false)
        .expect("sub works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_at(cx, 1440., 900.);

    // Adopt an in-flight fetch so the test proves the target, not the network.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.refresh.inflight.insert(work.id.clone());
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Group("Work".to_string()), window, cx);
            manyproxy_ui::run_action(
                view,
                "update-current-group",
                manyproxy_ui::ActionTarget::Selection,
                window,
                cx,
            );
        });
    });
    let inflight = view.read_with(cx, |view, _| view.refresh.inflight.clone());
    assert!(inflight.contains(&work.id), "the group's source refreshes");
    assert!(!inflight.contains(&home.id), "the other group stays put");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.refresh.inflight.clear();
            view.refresh.inflight_named.clear();
            view.refresh.inflight.insert(home.id.clone());
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::Source(home.id.clone()), window, cx);
            manyproxy_ui::run_action(
                view,
                "update-current-group",
                manyproxy_ui::ActionTarget::Selection,
                window,
                cx,
            );
        });
    });
    let inflight = view.read_with(cx, |view, _| view.refresh.inflight.clone());
    assert!(inflight.contains(&home.id), "the source refreshes itself");
    assert!(!inflight.contains(&work.id), "the group source stays put");

    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.refresh.inflight.clear();
            view.refresh.inflight_named.clear();
        });
    });
    let before = notification_count(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(manyproxy_ui::LibraryScope::All, window, cx);
            manyproxy_ui::run_action(
                view,
                "update-current-group",
                manyproxy_ui::ActionTarget::Selection,
                window,
                cx,
            );
        });
    });
    assert_eq!(
        notification_count(cx),
        before + 1,
        "All configs refuses with its reason"
    );
    assert!(
        view.read_with(cx, |view, _| view.refresh.inflight.is_empty()),
        "All configs starts nothing"
    );
}

/// The focus surface follows the shell: the regions a Tab walk crosses are
/// laid out in the signed order — title bar → nav → action bar → filter row →
/// header → rows → engine bar, and the engine bar is the last stop in the
/// window. (The keystroke-level focus transitions are pinned by the escape
/// reclaim and selection focus tests; this pins the order they walk.)
#[gpui_kit::test]
fn focus_order_follows_the_shell(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let _ = alpha;
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let ys: Vec<(&str, f32)> = cx.update(|window, _| {
        [
            "title-bar",
            "nav",
            "action-bar",
            "filter-slot",
            "library-column-headers",
            "engine-bar",
        ]
        .into_iter()
        .filter_map(|id| {
            let bounds = window.try_find(id.to_string())?.bounds();
            Some((id, bounds.origin.y.as_f32()))
        })
        .collect()
    });
    for pair in ys.windows(2) {
        assert!(
            pair[0].1 <= pair[1].1,
            "the focus surface runs top-down through the shell: {} at {} then {} at {}",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1
        );
    }
    assert_eq!(
        ys.last().map(|(id, _)| *id),
        Some("engine-bar"),
        "the engine bar is the last stop in the window"
    );
}
