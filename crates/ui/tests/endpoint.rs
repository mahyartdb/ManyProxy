//! endpoint: one binary of the split UI suite. Each binary runs as its own
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
fn endpoint_chip_pencil_inspector_edit_and_ctrl_e_open_the_same_draft(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Editable", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    // Entry 1: the chip's pencil opens the row popover.
    cx.update(|window, cx| {
        window.click(format!("row-endpoint-edit-{}", row.id), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "endpoint-form");
    assert_painted(cx, "endpoint-bind-input");
    assert_painted(cx, "endpoint-port-input");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .endpoint_draft
            .as_ref()
            .map(|draft| draft.host)),
        Some(EndpointHost::RowPopover)
    );
    assert_eq!(
        endpoint_values(&view, cx),
        ("127.0.0.1".to_string(), "10951".to_string())
    );

    view.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
    paint_frame(cx);

    // Entry 2: the inspector's Edit opens the same values inline.
    select_library_row(&view, cx, &row.id);
    assert_painted(cx, "detail-endpoint-edit");
    cx.update(|window, cx| window.click("detail-endpoint-edit", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "endpoint-form");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .endpoint_draft
            .as_ref()
            .map(|draft| draft.host)),
        Some(EndpointHost::InspectorInline)
    );
    assert_eq!(
        endpoint_values(&view, cx),
        ("127.0.0.1".to_string(), "10951".to_string())
    );

    view.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_inspector(window, cx);
        });
    });
    paint_frame(cx);

    // Entry 3: Ctrl+E with the inspector hidden anchors the row popover.
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.update(|window, cx| {
        let keystroke = gpui_kit::Keystroke::parse("ctrl-e").expect("test keystroke parses");
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "endpoint-form");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .endpoint_draft
            .as_ref()
            .map(|draft| (draft.row_id.clone(), draft.host))),
        Some((row.id.clone(), EndpointHost::RowPopover))
    );
    assert_eq!(
        endpoint_values(&view, cx),
        ("127.0.0.1".to_string(), "10951".to_string())
    );
    view.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
}

#[gpui_kit::test]
fn endpoint_apply_is_disabled_until_dirty_and_valid(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Stopped", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &row.id, EndpointHost::RowPopover);

    assert!(!endpoint_can_apply(&view, cx), "a clean draft cannot apply");
    endpoint_apply_clicked(cx);
    assert!(view.read_with(cx, |view, _| view.endpoint_draft.is_some()));
    assert_eq!(
        endpoint_saved(&view, cx, &row.id),
        ("127.0.0.1".to_string(), 10951)
    );
    assert_eq!(notification_count(cx), 0, "no toast before Apply lands");

    let invalid_port = manyproxy_ui::t!("endpoint-invalid-port");
    for port in ["", "0", "auto", "-1", "65536"] {
        endpoint_set(&view, cx, "port", port);
        assert_eq!(
            endpoint_error(&view, cx).as_deref(),
            Some(invalid_port.as_str()),
            "port {port:?} is refused live"
        );
        assert!(
            !endpoint_can_apply(&view, cx),
            "port {port:?} disables Apply"
        );
        endpoint_apply_clicked(cx);
        assert!(
            view.read_with(cx, |view, _| view.endpoint_draft.is_some()),
            "a refused apply leaves the draft open"
        );
        assert_eq!(
            endpoint_saved(&view, cx, &row.id).1,
            10951,
            "a refused apply writes nothing"
        );
    }

    endpoint_set(&view, cx, "port", "1");
    let privileged = endpoint_error(&view, cx).expect("port 1 is refused live");
    assert!(
        privileged.contains("privileged"),
        "port 1 names elevation: {privileged}"
    );
    assert_painted(cx, "endpoint-error");
    assert!(!endpoint_can_apply(&view, cx));

    endpoint_set(&view, cx, "port", "10952");
    assert_eq!(endpoint_error(&view, cx), None);
    assert!(
        endpoint_can_apply(&view, cx),
        "a free valid port enables Apply"
    );
    endpoint_apply_clicked(cx);
    assert!(view.read_with(cx, |view, _| view.endpoint_draft.is_none()));
    assert_eq!(
        endpoint_saved(&view, cx, &row.id),
        ("127.0.0.1".to_string(), 10952)
    );
    assert_eq!(notification_count(cx), 1, "one Endpoint updated toast");
}

#[gpui_kit::test]
fn endpoint_duplicate_names_the_row_and_use_port_fills_the_suggestion(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10961"))
        .expect("seed works");
    store
        .add(t30_engine_seed("Bravo", "10962"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &alpha.id, EndpointHost::RowPopover);

    endpoint_set(&view, cx, "port", "10962");
    let found = view
        .read_with(cx, |view, cx| {
            view.endpoint_check(cx).and_then(|check| check.collision)
        })
        .expect("Bravo holds the port");
    assert_eq!(found.name, "Bravo");
    assert_eq!(found.port, 10962);
    assert!(!endpoint_can_apply(&view, cx), "a collision blocks Apply");
    assert_painted(cx, "endpoint-collision");
    assert_painted(cx, "endpoint-use-port");
    assert_eq!(label_of("endpoint-use-port", cx), "Use 10963");

    cx.update(|window, cx| window.click("endpoint-use-port", cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        endpoint_values(&view, cx),
        ("127.0.0.1".to_string(), "10963".to_string()),
        "the suggestion button writes the free port into the field"
    );
    let check = view
        .read_with(cx, |view, cx| view.endpoint_check(cx))
        .expect("draft open");
    assert!(check.collision.is_none());
    assert!(check.can_apply());
    assert_absent(cx, "endpoint-collision".to_string());

    endpoint_apply_clicked(cx);
    assert_eq!(
        endpoint_saved(&view, cx, &alpha.id),
        ("127.0.0.1".to_string(), 10963)
    );
}

#[gpui_kit::test]
fn endpoint_apply_on_a_stopped_row_writes_store_and_closes(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Stopped", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let handle = endpoint_fake_engine(&view, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &row.id, EndpointHost::RowPopover);

    endpoint_set(&view, cx, "port", "10952");
    assert!(endpoint_can_apply(&view, cx));
    endpoint_apply_clicked(cx);

    assert_eq!(
        endpoint_saved(&view, cx, &row.id),
        ("127.0.0.1".to_string(), 10952)
    );
    assert!(view.read_with(cx, |view, _| view.endpoint_draft.is_none()));
    assert_eq!(
        handle.last_config(),
        None,
        "a stopped row never reaches the engine"
    );
    assert_eq!(notification_count(cx), 1, "one Endpoint updated toast");
    let (bind, port) = view.read_with(cx, |view, cx| {
        let row = &view.libraries.read(cx).delegate().rows[0];
        (row.bind_ip.clone(), row.port)
    });
    assert_eq!(
        manyproxy_core::endpoint_display(&bind, port),
        ":10952",
        "the delegate the chip paints reloads with the saved endpoint"
    );
}

#[gpui_kit::test]
fn endpoint_apply_on_a_running_row_restarts_and_closes(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Running", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let handle = endpoint_fake_engine(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&row.id, window, cx));
    });
    assert_eq!(endpoint_served_ports(&handle), vec![10951]);
    paint_frame(cx);
    endpoint_open(&view, cx, &row.id, EndpointHost::RowPopover);

    endpoint_set(&view, cx, "port", "10953");
    endpoint_apply_clicked(cx);

    assert_eq!(
        endpoint_saved(&view, cx, &row.id),
        ("127.0.0.1".to_string(), 10953)
    );
    assert!(view.read_with(cx, |view, _| view.endpoint_draft.is_none()));
    assert_eq!(
        endpoint_served_ports(&handle),
        vec![10953],
        "the running row is restarted on the new endpoint"
    );
    assert!(
        view.read_with(cx, |view, _| view.engine.running_ids().contains(&row.id)),
        "the row is still serving"
    );
}

#[gpui_kit::test]
fn endpoint_engine_failure_restores_store_and_restarts_old_endpoint(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Running", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let handle = endpoint_fake_engine(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_row(&row.id, window, cx));
    });
    handle.fail_start("address already in use");
    paint_frame(cx);
    let toasts_before = notification_count(cx);
    endpoint_open(&view, cx, &row.id, EndpointHost::RowPopover);

    endpoint_set(&view, cx, "port", "10952");
    endpoint_apply_clicked(cx);

    assert_eq!(
        endpoint_saved(&view, cx, &row.id),
        ("127.0.0.1".to_string(), 10951),
        "the store rolls back to the old endpoint"
    );
    assert_eq!(
        endpoint_values(&view, cx),
        ("127.0.0.1".to_string(), "10952".to_string()),
        "the attempted values stay in the open draft"
    );
    let reason = endpoint_failed_reason(&view, cx).expect("phase Failed");
    assert!(
        reason.contains("address already in use"),
        "the engine error reaches the draft: {reason}"
    );
    assert_painted(cx, "endpoint-error");
    assert_eq!(
        endpoint_served_ports(&handle),
        vec![10951],
        "the old endpoint serves again"
    );
    assert!(handle.is_running());
    assert_eq!(
        notification_count(cx),
        toasts_before + 1,
        "one Endpoint not changed toast"
    );
}

#[gpui_kit::test]
fn endpoint_port_65535_applies_and_port_1_shows_privileged_error(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Ports", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &row.id, EndpointHost::RowPopover);

    endpoint_set(&view, cx, "port", "1");
    let privileged = endpoint_error(&view, cx).expect("port 1 is refused live");
    assert!(privileged.contains("privileged"), "{privileged}");
    assert_painted(cx, "endpoint-error");
    assert!(!endpoint_can_apply(&view, cx));

    endpoint_set(&view, cx, "port", "65535");
    assert_eq!(endpoint_error(&view, cx), None);
    assert!(endpoint_can_apply(&view, cx), "65535 is a valid free port");
    endpoint_apply_clicked(cx);
    assert_eq!(
        endpoint_saved(&view, cx, &row.id),
        ("127.0.0.1".to_string(), 65535)
    );
}

#[gpui_kit::test]
fn endpoint_escape_reverts_dirty_and_closes_clean(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Escaped", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &row.id, EndpointHost::RowPopover);

    endpoint_set(&view, cx, "bind", "localhost");
    assert!(endpoint_can_apply(&view, cx));
    endpoint_press(&view, cx, "escape");
    assert!(
        view.read_with(cx, |view, _| view.endpoint_draft.is_some()),
        "a dirty Escape reverts instead of closing"
    );
    assert_eq!(
        endpoint_values(&view, cx),
        ("127.0.0.1".to_string(), "10951".to_string()),
        "the stored values come back"
    );

    endpoint_press(&view, cx, "escape");
    assert!(
        view.read_with(cx, |view, _| view.endpoint_draft.is_none()),
        "a clean Escape closes the draft"
    );
}

#[gpui_kit::test]
fn endpoint_outside_click_cancels(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(t30_engine_seed("Alpha", "10961"))
        .expect("seed works");
    let bravo = store
        .add(t30_engine_seed("Bravo", "10962"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &alpha.id, EndpointHost::RowPopover);
    endpoint_set(&view, cx, "port", "10963");
    assert!(view.read_with(cx, |view, _| view.endpoint_draft.is_some()));

    cx.update(|window, cx| {
        window.click(format!("row-name-text-{}", bravo.id), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.endpoint_draft.is_none()),
        "an outside click discards the draft"
    );
    assert_eq!(
        endpoint_saved(&view, cx, &alpha.id),
        ("127.0.0.1".to_string(), 10961),
        "discarding writes nothing"
    );
}

#[gpui_kit::test]
fn endpoint_popover_flips_above_when_the_row_is_near_the_bottom(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut ids = Vec::new();
    for i in 0..20 {
        let row = store
            .add(t30_engine_seed(
                &format!("Row {i}"),
                &(10900 + i).to_string(),
            ))
            .expect("seed works");
        ids.push(row.id);
    }
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    endpoint_open(&view, cx, &ids[0], EndpointHost::RowPopover);
    assert!(
        !view.read_with(cx, |view, cx| view.endpoint_frame(cx).expect("frame").flip),
        "a top row opens below its chip"
    );
    view.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
    paint_frame(cx);

    endpoint_open(&view, cx, &ids[10], EndpointHost::RowPopover);
    assert!(
        view.read_with(cx, |view, cx| view.endpoint_frame(cx).expect("frame").flip),
        "a row near the bottom flips the popover above its chip"
    );
}

#[gpui_kit::test]
fn endpoint_popover_closes_when_the_list_virtualizes_its_row_away(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let mut first = None;
    for i in 0..30 {
        let row = store
            .add(t30_engine_seed(
                &format!("Row {i}"),
                &(10900 + i).to_string(),
            ))
            .expect("seed works");
        first.get_or_insert(row);
    }
    let first = first.expect("one row");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);
    endpoint_open(&view, cx, &first.id, EndpointHost::RowPopover);
    assert!(view.read_with(cx, |view, _| view.endpoint_draft.is_some()));

    view.update(cx, |view, cx| {
        view.libraries
            .read(cx)
            .scroll_handle()
            .scroll_to_item(25, gpui_kit::ScrollStrategy::Top);
        cx.notify();
    });
    paint_frame(cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        view.read_with(cx, |view, _| view.endpoint_draft.is_none()),
        "scrolling the row out of the virtualized list closes its popover"
    );
}

#[gpui_kit::test]
fn endpoint_old_per_row_inputs_and_pending_commits_are_gone(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Only", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    select_library_row(&view, cx, &row.id);

    for id in [
        format!("row-bind-input-{}", row.id),
        format!("row-port-input-{}", row.id),
        format!("row-endpoint-warning-{}", row.id),
    ] {
        assert_absent(cx, id);
    }
    assert_painted(cx, &format!("row-endpoint-chip-{}", row.id));
    assert_painted(cx, &format!("row-endpoint-edit-slot-{}", row.id));
    assert_eq!(
        view.read_with(cx, |view, cx| view.libraries.read(cx).delegate().rows.len()),
        1,
        "the delegate holds rows, not per-row endpoint inputs"
    );
}

#[gpui_kit::test]
fn ctrl_e_opens_endpoint_and_ctrl_shift_e_opens_config(cx: &mut TestAppContext) {
    use manyproxy_ui::app::endpoint_editor::EndpointHost;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);

    let endpoint = manyproxy_ui::app::actions::action("edit-endpoint");
    assert_eq!(endpoint.shortcuts, &[("ctrl-e", "Ctrl+E")]);
    assert_eq!(
        manyproxy_ui::t!("action-label-edit-endpoint"),
        "Edit endpoint"
    );
    let config = manyproxy_ui::app::actions::action("edit");
    assert_eq!(config.shortcuts, &[("ctrl-shift-e", "Ctrl+Shift+E")]);
    assert_eq!(manyproxy_ui::t!("action-label-edit"), "Edit config…");
    assert!(manyproxy_ui::SHORTCUT_LINES.contains(&("Edit endpoint", "Ctrl-E")));
    assert!(manyproxy_ui::SHORTCUT_LINES.contains(&("Edit config", "Ctrl-Shift-E")));

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(t30_engine_seed("Keyboard", "10951"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    select_library_row(&view, cx, &row.id);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_inspector(window, cx);
        });
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));

    let press = |cx: &mut VisualTestContext, key: &str| {
        cx.update(|window, cx| {
            let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
            window.dispatch_keystroke(keystroke, cx);
        });
        cx.run_until_parked();
        paint_frame(cx);
    };

    press(cx, "ctrl-e");
    assert_painted(cx, "endpoint-form");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .endpoint_draft
            .as_ref()
            .map(|draft| draft.host)),
        Some(EndpointHost::RowPopover),
        "Ctrl+E edits the endpoint"
    );
    view.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
    paint_frame(cx);
    cx.update(|window, cx| window.focus(&focus, cx));

    press(cx, "ctrl-shift-e");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Ctrl+Shift+E keeps the full config form"
    );
    assert!(
        cx.update(|window, _| window.try_find("field-name").is_some()),
        "the config form names the field"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

/// T-06: Esc on the last overlay hands keyboard focus back to the shell, so the
/// next shortcut works without a click first.
#[gpui_kit::test]
fn t06_esc_dismiss_reclaims_shell_focus_for_the_next_shortcut(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(cx.update(overlay_open), "the add dialog is an overlay");
    assert!(
        cx.update(|window, cx| window.focused(cx)).is_some(),
        "the dialog holds focus while it is open"
    );

    let esc = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(esc, cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(!cx.update(overlay_open), "escape dismisses the add dialog");
    let shell_focus = view.read_with(cx, |view, _| view.focus.clone());
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(shell_focus),
        "the shell reclaims keyboard focus once no overlay is open"
    );

    // The defect: the next shortcut silently no-op'd until the user clicked.
    let ctrl_n = gpui_kit::Keystroke::parse("ctrl-n").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(ctrl_n, cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        cx.update(overlay_open),
        "ctrl-n opens the add dialog with no click between"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

/// T-06: choosing a palette op closes the palette and opens a second overlay in
/// the same handler; focus lands in the new overlay, not on the shell.
#[gpui_kit::test]
fn t06_palette_second_overlay_keeps_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(cx.update(overlay_open), "the palette is an overlay");

    let settings_op = IndexPath {
        section: 1,
        row: 3,
        column: 0,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.run_palette_op(settings_op, window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert!(cx.update(overlay_open), "the second overlay opens");
    assert_painted(cx, "settings-dialog-save");
    let shell_focus = view.read_with(cx, |view, _| view.focus.clone());
    let focused = cx.update(|window, cx| window.focused(cx));
    assert!(focused.is_some(), "focus lands in the new overlay");
    assert_ne!(
        focused,
        Some(shell_focus),
        "the reclaim leaves the new overlay's focus alone"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

/// T-06: a shortcut pressed while an overlay owns the keyboard is refused with
/// a toast, not swallowed.
#[gpui_kit::test]
fn t06_refused_shortcut_toasts_instead_of_silence(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    paint_frame(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(notification_count(cx), 0, "opening a dialog is silent");

    let ctrl_o = gpui_kit::Keystroke::parse("ctrl-o").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(ctrl_o, cx));
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        notification_count(cx),
        1,
        "the refused shortcut toasts instead of doing nothing"
    );
    assert_eq!(
        manyproxy_ui::overlay_refusal_note(),
        "Close the open dialog first",
        "the refusal names the way out"
    );
    assert!(
        cx.update(overlay_open),
        "the add dialog stays the only layer"
    );

    // One Esc still closes everything: no second overlay was stacked.
    let esc = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(esc, cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert!(
        !cx.update(overlay_open),
        "the refused shortcut opened no second overlay"
    );
}

/// T-06: the selected row card is the app's custom control while the keyboard
/// lands on the list. The frame hands the list's live focus handle to the
/// delegate, and the card paints the kit focus ring while that handle is
/// focused. The harness cannot read a painted ring's colour, so this pins the
/// handle handoff - the ring's only input - and the row's paint.
#[gpui_kit::test]
fn t06_selected_row_wears_the_kit_focus_ring(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(facet_seed("Ring", "vless", "team", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    select_library_row(&view, cx, &seeded.id);
    assert_painted(cx, &format!("row-surface-{}", seeded.id));

    let list_handle = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&list_handle, cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(list_handle.clone()),
        "the library list holds the keyboard"
    );
    let handed = view.read_with(cx, |view, cx| {
        view.libraries.read(cx).delegate().list_focus.clone()
    });
    assert_eq!(
        handed,
        Some(list_handle),
        "the frame hands the live list handle to the delegate"
    );
    assert_painted(cx, &format!("row-surface-{}", seeded.id));

    // Focus elsewhere: the row stays selected, the ring decision turns off.
    let shell = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&shell, cx));
    cx.run_until_parked();
    paint_frame(cx);
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(shell),
        "the shell can take the keyboard back"
    );
    assert_painted(cx, &format!("row-surface-{}", seeded.id));
}

/// T-06: once the list holds the keyboard, Up/Down walk the rows, and the
/// selection the kit reports to UIA is the one the delegate renders.
#[gpui_kit::test]
fn t06_list_arrows_traverse_and_report_the_selected_row(cx: &mut TestAppContext) {
    use gpui_kit::Focusable as _;
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Beta", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    // Anchor the selection at the first row, then give the list the keyboard.
    let list_handle = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
        });
        window.focus(&list_handle, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);

    fn press(cx: &mut VisualTestContext, key: &str) {
        let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
        cx.update(|window, cx| window.dispatch_keystroke(keystroke, cx));
        cx.run_until_parked();
        paint_frame(cx);
    }

    press(cx, "down");
    let (focus, second_id) = view.read_with(cx, |view, cx| {
        let delegate = view.libraries.read(cx).delegate();
        let second = view
            .visible
            .get(1)
            .and_then(|&row| delegate.rows.get(row))
            .map(|row| row.id.clone());
        (view.selection_focus.clone(), second)
    });
    assert_eq!(
        focus,
        second_id.clone(),
        "Down moves the app's focus to the next row"
    );
    let second_id = second_id.expect("second row exists");
    assert_painted(cx, &format!("row-surface-{second_id}"));

    press(cx, "up");
    let focus = view.read_with(cx, |view, _| view.selection_focus.clone());
    assert!(focus.is_some(), "Up walks back");
}

/// T-06: removing the selected row clears the list's own selection too, so UIA
/// stops naming a row that no longer exists.
#[gpui_kit::test]
fn t06_removing_the_selected_row_clears_the_lists_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(manyproxy_ui::bind_app_keys);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let doomed = store
        .add(facet_seed("Doomed", "vless", "team", "10801"))
        .expect("seed works");
    store
        .add(facet_seed("Keeper", "vmess", "team", "10802"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    select_library_row(&view, cx, &doomed.id);
    assert!(
        view.read_with(cx, |view, _| view.is_selected(&doomed.id)),
        "the click selects the row"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_remove(&doomed.id, window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);

    let (focus, count, still_selected) = view.read_with(cx, |view, _| {
        (
            view.selection_focus.clone(),
            view.selection_count(),
            view.is_selected(&doomed.id),
        )
    });
    assert_eq!(focus, None, "the removed row is no longer the focus");
    assert_eq!(count, 0, "the selection is pruned with the row");
    assert!(!still_selected, "the removed row is no longer selected");
    assert_absent(cx, format!("row-surface-{}", doomed.id));
}

/// T-08: a data folder that refuses a write raises the standing banner, names
/// the folder it cannot write to, and leaves the keyboard alone. The store is
/// opened first and the denial comes second, so reads keep working — the shape
/// of the real bug: everything looks fine until a save evaporates.
#[cfg(windows)]
#[gpui_kit::test]
fn t08_refused_write_raises_the_banner_naming_the_folder(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut folder = T08TempFolder::new("banner");
    let store = Store::open_file(&folder.path().join("library.db")).expect("file store opens");
    folder.deny_write();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    let path = folder.path().display().to_string();
    let pinned = format!("Changes will not be saved — cannot write to the data folder ({path}).");
    let (reason, text) = view.read_with(cx, |view, _| {
        (view.write_blocked.clone(), view.write_banner_text())
    });
    let reason = reason.expect("a refused probe is recorded as the banner's cause");
    assert!(
        reason.contains(&path),
        "the recorded reason says which folder refused: {reason}"
    );
    assert_eq!(
        text.as_deref(),
        Some(pinned.as_str()),
        "the banner sentence is pinned, with the folder named"
    );

    assert_painted(cx, "write-banner");
    assert_painted(cx, "write-banner-text");
    assert_painted(cx, "write-banner-open-folder");
    let label = cx.update(|window, _| {
        window
            .try_find("write-banner-text".to_string())
            .expect("the banner text renders")
            .label()
            .map(str::to_string)
    });
    assert_eq!(
        label.as_deref(),
        Some(pinned.as_str()),
        "the banner announces the sentence it paints"
    );

    let slot = cx
        .update(|window, _| window.try_find("filter-slot".to_string()))
        .expect("the filter slot renders")
        .bounds();
    let banner = cx
        .update(|window, _| window.try_find("write-banner".to_string()))
        .expect("the banner renders")
        .bounds();
    let row = cx
        .update(|window, _| window.try_find("library-column-headers".to_string()))
        .expect("the table header renders")
        .bounds();
    assert!(
        banner.origin.y >= slot.origin.y + slot.size.height - px(1.),
        "the banner sits under the filter slot: banner {banner:?}, slot {slot:?}"
    );
    assert!(
        row.origin.y >= banner.origin.y + banner.size.height - px(1.),
        "the table starts under the banner: banner {banner:?}, header {row:?}"
    );
    assert_eq!(
        cx.update(|window, _| window
            .try_find("write-banner".to_string())
            .expect("the banner renders")
            .focused()),
        None,
        "the banner never takes the keyboard"
    );

    folder.allow_write();
}

/// T-08: the banner earns its height instead of covering the table. At the
/// app's minimum frame it stays fully inside, and the list starts below it with
/// a real row still painted.
#[cfg(windows)]
#[gpui_kit::test]
fn t08_banner_fits_the_minimum_frame_and_the_table_stays_below(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut folder = T08TempFolder::new("min-frame");
    let mut store = Store::open_file(&folder.path().join("library.db")).expect("file store opens");
    let seeded = store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    folder.deny_write();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame_min(cx);
    cx.run_until_parked();
    paint_frame_min(cx);

    assert!(
        view.read_with(cx, |view, _| view.write_blocked.is_some()),
        "the banner is up at the minimum frame"
    );
    for id in [
        "write-banner",
        "write-banner-text",
        "write-banner-open-folder",
    ] {
        assert_painted(cx, id);
        assert_within_frame_min(cx, id);
    }
    assert_painted(cx, "config-list");
    assert_within_frame_min(cx, "config-list");
    let row = format!("library-row-{}", seeded.id);
    assert_painted(cx, &row);
    assert_within_frame_min(cx, &row);

    let banner = cx
        .update(|window, _| window.try_find("write-banner".to_string()))
        .expect("the banner renders")
        .bounds();
    let list = cx
        .update(|window, _| window.try_find("config-list".to_string()))
        .expect("the list renders")
        .bounds();
    let row_bounds = cx
        .update(|window, _| window.try_find(row.clone()))
        .expect("the seeded row renders")
        .bounds();
    assert!(
        list.origin.y <= banner.origin.y + px(1.),
        "the banner lives inside the list pane: banner {banner:?}, list {list:?}"
    );
    assert!(
        row_bounds.origin.y >= banner.origin.y + banner.size.height - px(1.),
        "the seeded row is not covered by the banner: banner {banner:?}, row {row_bounds:?}"
    );

    folder.allow_write();
}

/// T-08: restoring write permission clears the banner on the next existing
/// tick — no restart and no new timer thread.
#[cfg(windows)]
#[gpui_kit::test]
fn t08_tick_clears_the_banner_when_the_folder_writes_again(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut folder = T08TempFolder::new("recover");
    let store = Store::open_file(&folder.path().join("library.db")).expect("file store opens");
    folder.deny_write();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "write-banner");
    assert!(
        view.read_with(cx, |view, _| view.write_blocked.is_some()),
        "the refused startup probe raised the banner"
    );

    folder.allow_write();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tick_engine_poll(cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        view.read_with(cx, |view, _| view.write_blocked.clone()),
        None,
        "the next probe clears the standing condition"
    );
    assert!(
        view.read_with(cx, |view, _| view.write_banner_text())
            .is_none(),
        "and the sentence goes with it"
    );
    assert_absent(cx, "write-banner".to_string());
}

/// T-08: a writable data folder is silent — no banner at startup, and none
/// after a tick re-probes it (no false alarm).
#[cfg(windows)]
#[gpui_kit::test]
fn t08_writable_folder_shows_no_banner(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let folder = T08TempFolder::new("writable");
    let mut store = Store::open_file(&folder.path().join("library.db")).expect("file store opens");
    store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert!(
        view.read_with(cx, |view, _| view.write_blocked.is_none()),
        "a writable folder has nothing to stand up"
    );
    assert_absent(cx, "write-banner".to_string());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tick_engine_poll(cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, "write-banner".to_string());
}

/// T-08: an in-memory store has no folder to lose, so its probe is clean and
/// the banner can never fire — a false alarm there would be a defect.
#[gpui_kit::test]
fn t08_in_memory_store_never_raises_the_banner(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("Alpha", "a.example.com", "10801"))
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert!(
        view.read_with(cx, |view, _| view.write_blocked.is_none()),
        "an in-memory store probes clean"
    );
    assert!(
        view.read_with(cx, |view, _| view.write_banner_text())
            .is_none(),
        "and names no folder"
    );
    assert!(
        view.read_with(cx, |view, _| !view.write_blocked_pinned),
        "a deliberate in-memory store is not a failed open"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.write_blocked_folder.clone()),
        None,
        "it claims no folder to blame"
    );
    assert_absent(cx, "write-banner".to_string());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tick_engine_poll(cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, "write-banner".to_string());
}

/// T-08: a save that fails because the folder refuses writes raises the
/// standing banner and suppresses the transient toast. The store is forced
/// read-only as well: an open SQLite handle can still push a commit through
/// WAL inside an otherwise denied folder, so the denial alone does not make
/// the save fail.
#[cfg(windows)]
#[gpui_kit::test]
fn t08_failed_save_raises_the_banner_instead_of_a_toast(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut folder = T08TempFolder::new("save");
    let store = Store::open_file(&folder.path().join("library.db")).expect("file store opens");
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_absent(cx, "write-banner".to_string());
    let notifications_before = notification_count(cx);

    folder.deny_write();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.store
                .set_query_only_for_test()
                .expect("read-only sets");
        });
    });
    let added = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.commit_add(seed_input("Lost"), window, cx)
        })
    });
    assert!(!added, "the save fails");
    assert_eq!(
        notification_count(cx),
        notifications_before,
        "no toast: the standing condition gets the standing surface"
    );

    let path = folder.path().display().to_string();
    let pinned = format!("Changes will not be saved — cannot write to the data folder ({path}).");
    let text = view.read_with(cx, |view, _| view.write_banner_text());
    assert_eq!(
        text.as_deref(),
        Some(pinned.as_str()),
        "the failed save names the folder it could not write to"
    );
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "write-banner");

    folder.allow_write();
}

/// T-08: a folder that refuses the file store's OPEN is the primary case, not
/// an edge: SQLite must create `-wal`/`-shm`, the app falls back to memory,
/// and the banner — not just a toast — has to name the real reason and say
/// that nothing will be saved.
#[cfg(windows)]
#[gpui_kit::test]
fn t08_open_failure_raises_the_banner_naming_the_folder(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut folder = T08TempFolder::new("open-failure");
    let db = folder.path().join("library.db");
    folder.deny_write();
    let error = Store::open_file(&db)
        .err()
        .expect("a denied folder refuses the file store's open");
    let reason = error.to_string();
    let (view, cx) = mount_fallback_view(error, db, cx);
    cx.run_until_parked();
    paint_frame(cx);

    assert!(
        view.read_with(cx, |view, _| view.store.data_dir().is_none()),
        "the session runs in memory"
    );
    assert!(
        view.read_with(cx, |view, _| view.write_blocked_pinned),
        "the failed open pins the banner"
    );
    let path = folder.path().display().to_string();
    let pinned = format!("Changes will not be saved — cannot use the data folder ({path}).");
    let text = view.read_with(cx, |view, _| view.write_banner_text());
    assert_eq!(
        text.as_deref(),
        Some(pinned.as_str()),
        "the banner names the folder; the OS reason rides the tooltip"
    );
    assert!(
        !pinned.contains("cannot write to the data folder"),
        "an open failure is not reported as a write refusal: {pinned}"
    );
    let recorded = view
        .read_with(cx, |view, _| view.write_blocked.clone())
        .expect("the open failure is the recorded cause");
    assert_eq!(
        recorded, reason,
        "the tooltip keeps the OS reason, not a generic refusal"
    );
    assert_painted(cx, "write-banner");
    assert_painted(cx, "write-banner-open-folder");

    folder.allow_write();
}

/// T-08: a failed-open session keeps its banner even when the folder writes
/// again. The store is in memory, so its probe cannot see the folder at all;
/// and even a successful probe could not make this session persist, so
/// clearing the banner would be a lie. Only a restart that opens the store
/// clears it.
#[cfg(windows)]
#[gpui_kit::test]
fn t08_failed_open_banner_survives_a_succeeding_probe(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let mut folder = T08TempFolder::new("pinned");
    let db = folder.path().join("library.db");
    folder.deny_write();
    let error = Store::open_file(&db)
        .err()
        .expect("a denied folder refuses the file store's open");
    let (view, cx) = mount_fallback_view(error, db, cx);
    cx.run_until_parked();
    paint_frame(cx);
    assert_painted(cx, "write-banner");
    let before = view.read_with(cx, |view, _| view.write_blocked.clone());

    folder.allow_write();
    let proof = folder.path().join("proof.tmp");
    std::fs::write(&proof, b"writable again").expect("the folder writes again");
    std::fs::remove_file(&proof).expect("the proof file is removed");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tick_engine_poll(cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    assert_eq!(
        view.read_with(cx, |view, _| view.write_blocked.clone()),
        before,
        "a successful write is not proof that this in-memory session saves"
    );
    assert!(
        view.read_with(cx, |view, _| view.store.data_dir().is_none()),
        "the session still has no folder of its own"
    );
    assert_painted(cx, "write-banner");
}
