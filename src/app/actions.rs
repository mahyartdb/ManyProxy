use crate::*;

/// What an action runs on. The context menu passes the row it was opened on;
/// shortcuts, the palette and the inspector `…` menu pass the current
/// selection. Handlers resolve the target at invocation — never at render.
#[derive(Clone, Copy)]
pub(crate) enum ActionTarget<'a> {
    Row(&'a str),
    Selection,
}

/// How a handler addresses its row. `Row` handlers take the resolved id, so a
/// context menu can act on a row that is not the selection. `Selected`
/// handlers already work off the selection.
pub(crate) enum Handler {
    Row(fn(&mut AppView, &str, &mut Window, &mut Context<AppView>)),
    Selected(fn(&mut AppView, &mut Window, &mut Context<AppView>)),
}

/// One row of the app's action table. Every entry point (context menu,
/// inspector menu, command palette, keymap) reads its label, keywords,
/// shortcut and enabled state from here, and dispatches through
/// [`run_action`], so a verb cannot drift between surfaces.
pub(crate) struct ActionDef {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) keywords: &'static [&'static str],
    pub(crate) tooltip: Option<&'static str>,
    /// (keystroke, chip) pairs. A slice because ctrl-1/ctrl-3 alias one id.
    pub(crate) shortcuts: &'static [(&'static str, &'static str)],
    pub(crate) enabled: fn(&AppView, ActionTarget<'_>, &App) -> bool,
    pub(crate) run: Handler,
}

pub(crate) const ACTIONS: &[ActionDef] = &[
    selected("add", "Add configuration", &["new", "create"], &[("ctrl-n", "Ctrl+N")], AppView::open_add_dialog),
    selected(
        "import",
        "Import configurations",
        &["import", "link", "file"],
        &[("ctrl-o", "Ctrl+O")],
        open_import,
    ),
    selected(
        "paste-import",
        "Import from clipboard",
        &["clipboard", "paste", "import"],
        &[("ctrl-v", "Ctrl+V")],
        AppView::open_import_from_clipboard,
    ),
    selected(
        "export-library",
        "Export library",
        &["export", "backup", "json"],
        &[],
        AppView::open_export_dialog,
    ),
    selected(
        "open-palette",
        "Command palette",
        &["command", "palette", "run"],
        &[("ctrl-k", "Ctrl+K")],
        AppView::open_palette,
    ),
    selected(
        "focus-search",
        "Focus search field",
        &["search", "find", "filter"],
        &[("ctrl-f", "Ctrl+F")],
        AppView::focus_library_search,
    ),
    selected(
        "clear-checked",
        "Clear checked",
        &["clear", "uncheck", "escape"],
        &[("escape", "Esc")],
        clear_checked,
    ),
    selected(
        "update-current-group",
        "Update current group",
        &["update", "group"],
        &[("ctrl-r", "Ctrl+R")],
        AppView::update_current_group,
    ),
    selected(
        "stop-testing",
        "Stop testing",
        &["stop", "test", "cancel"],
        &[("ctrl-.", "Ctrl+.")],
        AppView::stop_group_tests,
    ),
    selected("start-all", "Start all", &["start", "all", "serve"], &[], AppView::start_all_engines),
    selected("stop-all", "Stop all", &["stop", "all"], &[], AppView::stop_all_engines),
    selected("test-all", "Test all", &["test", "all", "probe"], &[], test_all),
    selected("subscriptions", "Subscriptions", &["subscription", "refresh"], &[], AppView::open_subscriptions_dialog),
    selected("presets", "Routing presets", &["preset", "routing", "rules"], &[], AppView::open_presets_dialog),
    selected("groups", "Groups", &["group", "move"], &[], AppView::open_groups_dialog),
    selected("logs", "Logs", &["log", "output"], &[], AppView::open_logs_dialog),
    selected("settings", "Settings", &["settings", "options", "test url"], &[], AppView::open_settings_dialog),
    selected("about", "About", &["about", "version", "shortcuts"], &[], AppView::open_about_dialog),
    selected(
        "runtime-stats",
        "Runtime stats",
        &["runtime", "stats", "health", "cpu", "memory"],
        &[],
        AppView::open_runtime_stats_dialog,
    ),
    selected(
        "traffic-stats",
        "Traffic stats",
        &["traffic", "stats", "usage", "profile"],
        &[],
        AppView::open_traffic_stats_dialog,
    ),
    selected(
        "row-columns",
        "Row columns",
        &["column", "columns", "rows", "display"],
        &[],
        AppView::open_row_columns_dialog,
    ),
    selected(
        "inspector-details",
        "Show Details tab",
        &["details", "tab"],
        &[("ctrl-1", "Ctrl+1"), ("ctrl-3", "Ctrl+3")],
        show_details,
    ),
    selected(
        "inspector-health",
        "Show Health tab",
        &["health", "tab"],
        &[("ctrl-2", "Ctrl+2")],
        show_health,
    ),
    selected(
        "inspector-logs",
        "Show Logs tab",
        &["log", "output", "tab"],
        &[("ctrl-4", "Ctrl+4")],
        show_logs,
    ),
    selected(
        "inspector-links",
        "Show Links tab",
        &["links", "connections", "tab"],
        &[("ctrl-5", "Ctrl+5")],
        show_links,
    ),
    selected(
        "inspector-traffic",
        "Show Traffic tab",
        &["traffic", "tab"],
        &[("ctrl-6", "Ctrl+6")],
        show_traffic,
    ),
    row(
        "start",
        "Start",
        &["start", "serve", "f5"],
        &[("f5", "F5")],
        "Start this row's endpoint",
        AppView::start_row,
    ),
    row(
        "stop",
        "Stop",
        &["stop"],
        &[("shift-f5", "Shift+F5")],
        "Stop this row's endpoint",
        AppView::stop_row,
    ),
    row(
        "test",
        "Test",
        &["test", "probe", "health"],
        &[("ctrl-t", "Ctrl+T")],
        "URL-test this row's endpoint",
        AppView::test_row,
    ),
    row(
        "copy-urls",
        "Copy URLs",
        &["copy", "clipboard", "url", "endpoint"],
        &[("ctrl-u", "Ctrl+U")],
        "Copy bind endpoints (no secrets)",
        AppView::copy_row_urls,
    ),
    row(
        "copy-proxy",
        "Copy SOCKS",
        &["copy", "socks", "proxy"],
        &[],
        "Copy this row's SOCKS proxy URL only",
        AppView::copy_row_socks,
    ),
    row(
        "edit",
        "Edit",
        &["edit", "rename"],
        &[("ctrl-e", "Ctrl+E")],
        "Edit this row",
        AppView::open_edit_for_row,
    ),
    row(
        "delete",
        "Delete",
        &["delete", "remove"],
        &[("delete", "Delete")],
        "Delete this row",
        AppView::open_remove_for_row,
    ),
    row(
        "move",
        "Move to group",
        &["move", "group"],
        &[],
        "Move this row to a group",
        AppView::open_row_move_dialog,
    ),
    row(
        "share",
        "Share…",
        &["share", "link", "qr"],
        &[],
        "Copy this row's share link (carries credentials)",
        AppView::open_share_for_row,
    ),
    row(
        "duplicate",
        "Duplicate",
        &["duplicate", "copy", "clone"],
        &[],
        "Duplicate this row (disabled copy)",
        AppView::commit_duplicate,
    ),
    row("ping", "Ping", &["ping", "tcp"], &[], "TCP-ping this row's endpoint", AppView::ping_row),
    row(
        "reset-traffic",
        "Reset traffic",
        &["reset", "traffic", "counters"],
        &[],
        "Reset this row's traffic counters",
        AppView::open_traffic_reset_for_row,
    ),
    row(
        "reset-usage",
        "Reset usage",
        &["reset", "usage", "counters"],
        &[],
        "Reset this row's usage counters",
        AppView::open_usage_reset_for_row,
    ),
    row(
        "auto-start",
        "Restore on launch",
        &["auto", "start", "launch"],
        &[],
        "Restore this row on launch — Start and Stop control serving now",
        AppView::toggle_row_enabled,
    ),
];

fn always(_view: &AppView, _target: ActionTarget<'_>, _cx: &App) -> bool {
    true
}

/// A row verb only runs when there is a row. A `Row` target is always one;
/// a selection target can be empty, and that refusal is the only place the
/// palette, the menus and the shortcuts say "select a row first".
fn needs_row(view: &AppView, target: ActionTarget<'_>, cx: &App) -> bool {
    target_row(view, target, cx).is_some()
}

const fn selected(
    id: &'static str,
    label: &'static str,
    keywords: &'static [&'static str],
    shortcuts: &'static [(&'static str, &'static str)],
    run: fn(&mut AppView, &mut Window, &mut Context<AppView>),
) -> ActionDef {
    ActionDef {
        id,
        label,
        keywords,
        tooltip: None,
        shortcuts,
        enabled: always,
        run: Handler::Selected(run),
    }
}

const fn row(
    id: &'static str,
    label: &'static str,
    keywords: &'static [&'static str],
    shortcuts: &'static [(&'static str, &'static str)],
    tooltip: &'static str,
    run: fn(&mut AppView, &str, &mut Window, &mut Context<AppView>),
) -> ActionDef {
    ActionDef {
        id,
        label,
        keywords,
        tooltip: Some(tooltip),
        shortcuts,
        enabled: needs_row,
        run: Handler::Row(run),
    }
}

/// The palette's groups, in render order. Ids, not copies of the verbs.
pub(crate) const PALETTE_GROUPS: &[(&str, &[&str])] = &[
    (
        "Library",
        &[
            "add",
            "import",
            "start",
            "stop",
            "test",
            "copy-urls",
            "edit",
            "delete",
            "focus-search",
            "clear-checked",
            "inspector-details",
            "inspector-health",
            "paste-import",
            "export-library",
            "move",
            "share",
        ],
    ),
    (
        "Manage",
        &[
            "subscriptions",
            "presets",
            "groups",
            "logs",
            "settings",
            "about",
            "start-all",
            "stop-all",
            "test-all",
            "runtime-stats",
            "traffic-stats",
            "row-columns",
        ],
    ),
];

pub(crate) fn action(id: &str) -> &'static ActionDef {
    ACTIONS
        .iter()
        .find(|def| def.id == id)
        .unwrap_or_else(|| panic!("unknown action id {id:?}"))
}

/// The row an action will run on, resolved now. A `Row` target is trusted as
/// given — the row ops already report a vanished row ("Already removed").
fn target_row(view: &AppView, target: ActionTarget<'_>, cx: &App) -> Option<String> {
    match target {
        ActionTarget::Row(id) => Some(id.to_string()),
        ActionTarget::Selection => view.selected_config(cx).map(|config| config.id),
    }
}

pub(crate) fn run_action(
    view: &mut AppView,
    id: &'static str,
    target: ActionTarget<'_>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let def = action(id);
    if !(def.enabled)(view, target, cx) {
        window.push_notification(Notification::info(no_selection_message()), cx);
        return;
    }
    match def.run {
        Handler::Row(run) => {
            let Some(row_id) = target_row(view, target, cx) else {
                return;
            };
            run(view, &row_id, window, cx);
        }
        Handler::Selected(run) => run(view, window, cx),
    }
    cx.notify();
}

/// A keybinding's way in: the overlay owns the keyboard, so a shortcut fired
/// under a dialog is refused rather than acting on the screen underneath.
pub(crate) fn shortcut_action(
    view: &mut AppView,
    id: &'static str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    if overlay_open(window, cx) {
        refuse_shortcut_under_overlay(window, cx);
        return;
    }
    run_action(view, id, ActionTarget::Selection, window, cx);
}

pub(crate) fn palette_action_at(index: IndexPath) -> Option<&'static str> {
    PALETTE_GROUPS.get(index.section)?.1.get(index.row).copied()
}

fn clear_checked(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.clear_checked_unless_dialog(window, cx);
}

fn open_import(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
}

fn test_all(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.test_all_health(window, cx);
}

fn show_details(view: &mut AppView, _window: &mut Window, _cx: &mut Context<AppView>) {
    view.toggle_inspector(InspectorTab::Details);
}

fn show_health(view: &mut AppView, _window: &mut Window, _cx: &mut Context<AppView>) {
    view.set_inspector_tab(InspectorTab::Health);
}

fn show_logs(view: &mut AppView, _window: &mut Window, _cx: &mut Context<AppView>) {
    view.set_inspector_tab(InspectorTab::Logs);
}

fn show_links(view: &mut AppView, _window: &mut Window, _cx: &mut Context<AppView>) {
    view.set_inspector_tab(InspectorTab::Links);
}

fn show_traffic(view: &mut AppView, _window: &mut Window, _cx: &mut Context<AppView>) {
    view.set_inspector_tab(InspectorTab::Traffic);
}
