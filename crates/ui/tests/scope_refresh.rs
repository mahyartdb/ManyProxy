//! scope-refresh: the two nav tests that drive a subscription refresh.
//!
//! They run as their own process (the slice 8 harness pattern): a crash in
//! one must not take the rest of the nav coverage with it.

#![allow(unused_imports)]
mod support;
use support::*;
// The whole test suite: pure-logic tests and gpui test-app paints.
// Belongs: every `#[test]`/`#[gpui_kit::test]` and its local fixtures.
// Not here: production helpers - a test that needs one promotes it to a module.
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::TestAppContext;
use manyproxy_ui::{run_action, ActionTarget};

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
    paint_frame_expanded(cx);
    assert_painted(cx, &format!("nav-source-refresh-{}", free.id));

    cx.update(|window, cx| {
        window.click(format!("nav-source-refresh-{}", free.id), cx);
    });
    let inflight = view.read_with(cx, |view, _| view.refresh.inflight.clone());
    assert!(inflight.contains(&free.id), "the clicked source refreshes");
    assert!(!inflight.contains(&paid.id), "the other source stays put");
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
    paint_frame_expanded(cx);

    // Adopt an in-flight fetch so the test proves the target, not the network.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.refresh.inflight.insert(work.id.clone());
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(
                manyproxy_ui::LibraryScope::Group("Work".to_string()),
                window,
                cx,
            );
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
            view.set_scope(
                manyproxy_ui::LibraryScope::Source(home.id.clone()),
                window,
                cx,
            );
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
