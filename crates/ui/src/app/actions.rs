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
    Rows(fn(&mut AppView, &[String], &mut Window, &mut Context<AppView>)),
    Selected(fn(&mut AppView, &mut Window, &mut Context<AppView>)),
}

/// One row of the app's action table. Every entry point (context menu,
/// inspector menu, command palette, keymap) reads its label, keywords,
/// shortcut and enabled state from here, and dispatches through
/// [`run_action`], so a verb cannot drift between surfaces.
pub(crate) struct ActionDef {
    pub(crate) id: &'static str,
    /// Fluent key for the verb's label.
    pub(crate) label_key: &'static str,
    pub(crate) keywords: &'static [&'static str],
    /// Fluent key for the verb's tooltip, when it has one.
    pub(crate) tooltip_key: Option<&'static str>,
    /// (keystroke, chip) pairs. A slice because ctrl-1/ctrl-3 alias one id.
    pub(crate) shortcuts: &'static [(&'static str, &'static str)],
    pub(crate) enabled: fn(&AppView, ActionTarget<'_>, &App) -> bool,
    pub(crate) run: Handler,
}

impl ActionDef {
    pub(crate) fn label(&self) -> String {
        crate::i18n::translate(self.label_key, None)
    }

    pub(crate) fn tooltip(&self) -> Option<String> {
        self.tooltip_key
            .map(|key| crate::i18n::translate(key, None))
    }
}

pub(crate) const ACTIONS: &[ActionDef] = &[
    selected(
        "add",
        "action-label-add",
        &["new", "create"],
        &[("ctrl-n", "Ctrl+N")],
        AppView::open_add_dialog,
    ),
    selected(
        "import",
        "action-label-import",
        &["import", "link", "file"],
        &[("ctrl-o", "Ctrl+O")],
        open_import,
    ),
    selected(
        "paste-import",
        "action-label-paste-import",
        &["clipboard", "paste", "import"],
        &[("ctrl-v", "Ctrl+V")],
        AppView::open_import_from_clipboard,
    ),
    selected(
        "export-library",
        "action-label-export-library",
        &["export", "backup", "json"],
        &[],
        AppView::open_export_dialog,
    ),
    selected(
        "open-palette",
        "action-label-open-palette",
        &["command", "palette", "run"],
        &[("ctrl-k", "Ctrl+K")],
        AppView::open_palette,
    ),
    selected(
        "focus-search",
        "action-label-focus-search",
        &["search", "find", "filter"],
        &[("ctrl-f", "Ctrl+F")],
        AppView::focus_library_search,
    ),
    selected(
        "clear-selection",
        "action-label-clear-selection",
        &["clear", "deselect", "escape"],
        &[("escape", "Esc")],
        clear_selection,
    ),
    selected(
        "density-comfortable",
        "action-label-density-comfortable",
        &["density", "comfortable", "rows", "spacing"],
        &[],
        AppView::set_comfortable_density,
    ),
    selected(
        "density-compact",
        "action-label-density-compact",
        &["density", "compact", "rows", "spacing"],
        &[],
        AppView::set_compact_density,
    ),
    selected(
        "update-current-group",
        "action-label-update-current-group",
        &["update", "group"],
        &[("ctrl-r", "Ctrl+R")],
        AppView::update_current_group,
    ),
    selected(
        "stop-testing",
        "action-label-stop-testing",
        &["stop", "test", "cancel"],
        &[("ctrl-.", "Ctrl+.")],
        AppView::stop_group_tests,
    ),
    selected(
        "start-all",
        "action-label-start-all",
        &["start", "all", "serve"],
        &[],
        AppView::start_all_engines,
    ),
    selected(
        "stop-all",
        "action-label-stop-all",
        &["stop", "all"],
        &[],
        AppView::stop_all_engines,
    ),
    selected(
        "test-all",
        "action-label-test-all",
        &["test", "all", "probe"],
        &[],
        test_all,
    ),
    selected(
        "subscriptions",
        "action-label-subscriptions",
        &["subscription", "refresh"],
        &[],
        AppView::open_subscriptions_dialog,
    ),
    selected(
        "presets",
        "action-label-presets",
        &["preset", "routing", "rules"],
        &[],
        AppView::open_presets_dialog,
    ),
    selected(
        "groups",
        "action-label-groups",
        &["group", "move"],
        &[],
        AppView::open_groups_dialog,
    ),
    selected(
        "logs",
        "action-label-logs",
        &["log", "output"],
        &[],
        AppView::open_logs_dialog,
    ),
    selected(
        "settings",
        "action-label-settings",
        &["settings", "options", "test url"],
        &[],
        AppView::open_settings_dialog,
    ),
    selected(
        "about",
        "action-label-about",
        &["about", "version", "shortcuts"],
        &[],
        AppView::open_about_dialog,
    ),
    selected(
        "runtime-stats",
        "action-label-runtime-stats",
        &["runtime", "stats", "health", "cpu", "memory"],
        &[],
        AppView::open_runtime_stats_dialog,
    ),
    selected(
        "traffic-stats",
        "action-label-traffic-stats",
        &["traffic", "stats", "usage", "profile"],
        &[],
        AppView::open_traffic_stats_dialog,
    ),
    selected(
        "row-columns",
        "action-label-row-columns",
        &["column", "columns", "rows", "display"],
        &[],
        AppView::open_row_columns_dialog,
    ),
    selected(
        "text-size",
        "action-label-text-size",
        &["text", "size", "font", "zoom", "large", "accessibility"],
        &[],
        text_size,
    ),
    selected(
        "sort-latency",
        "action-label-sort-latency",
        &["sort", "latency", "ping", "health"],
        &[],
        sort_latency,
    ),
    selected(
        "sort-download",
        "action-label-sort-download",
        &["sort", "download", "speed", "traffic"],
        &[],
        sort_download,
    ),
    selected(
        "sort-upload",
        "action-label-sort-upload",
        &["sort", "upload", "speed", "traffic"],
        &[],
        sort_upload,
    ),
    selected(
        "sort-out-ip",
        "action-label-sort-out-ip",
        &["sort", "out", "ip", "egress"],
        &[],
        sort_out_ip,
    ),
    selected(
        "sort-type-security",
        "action-label-sort-type-security",
        &["sort", "type", "security", "protocol"],
        &[],
        sort_type_security,
    ),
    selected(
        "sort-traffic-total",
        "action-label-sort-traffic-total",
        &["sort", "traffic", "total", "usage"],
        &[],
        sort_traffic_total,
    ),
    selected(
        "sort-traffic-down",
        "action-label-sort-traffic-down",
        &["sort", "traffic", "download", "rx"],
        &[],
        sort_traffic_down,
    ),
    selected(
        "sort-traffic-up",
        "action-label-sort-traffic-up",
        &["sort", "traffic", "upload", "tx"],
        &[],
        sort_traffic_up,
    ),
    selected(
        "sort-none",
        "action-label-sort-none",
        &["sort", "clear", "none", "library", "order"],
        &[],
        sort_none,
    ),
    selected(
        "sort-direction",
        "action-label-sort-direction",
        &["sort", "reverse", "direction", "ascending", "descending"],
        &[],
        sort_direction,
    ),
    selected(
        "inspector-details",
        "action-label-inspector-details",
        &["details", "tab"],
        &[("ctrl-1", "Ctrl+1"), ("ctrl-3", "Ctrl+3")],
        show_details,
    ),
    selected(
        "inspector-health",
        "action-label-inspector-health",
        &["health", "tab"],
        &[("ctrl-2", "Ctrl+2")],
        show_health,
    ),
    selected(
        "inspector-logs",
        "action-label-inspector-logs",
        &["log", "output", "tab"],
        &[("ctrl-4", "Ctrl+4")],
        show_logs,
    ),
    selected(
        "inspector-links",
        "action-label-inspector-links",
        &["links", "connections", "tab"],
        &[("ctrl-5", "Ctrl+5")],
        show_links,
    ),
    selected(
        "inspector-traffic",
        "action-label-inspector-traffic",
        &["traffic", "tab"],
        &[("ctrl-6", "Ctrl+6")],
        show_traffic,
    ),
    selected(
        "inspector-toggle",
        "action-label-inspector-toggle",
        &["inspector", "panel", "toggle"],
        &[("ctrl-i", "Ctrl+I")],
        toggle_inspector_presence,
    ),
    rows(
        "start",
        "action-label-start",
        &["start", "serve", "f5"],
        &[("f5", "F5")],
        "action-tip-start",
        AppView::start_selected_rows,
    ),
    rows(
        "stop",
        "action-label-stop",
        &["stop"],
        &[("shift-f5", "Shift+F5")],
        "action-tip-stop",
        AppView::stop_selected_rows,
    ),
    rows(
        "test",
        "action-label-test",
        &["test", "probe", "health"],
        &[("ctrl-t", "Ctrl+T")],
        "action-tip-test",
        AppView::test_selected_rows,
    ),
    rows(
        "copy-urls",
        "action-label-copy-urls",
        &["copy", "clipboard", "url", "endpoint"],
        &[("ctrl-u", "Ctrl+U")],
        "action-tip-copy-urls",
        AppView::copy_selected_urls,
    ),
    rows(
        "copy-proxy",
        "action-label-copy-proxy",
        &["copy", "socks", "proxy"],
        &[],
        "action-tip-copy-proxy",
        AppView::copy_selected_socks,
    ),
    row(
        "edit",
        "action-label-edit",
        &["edit", "rename"],
        &[("ctrl-e", "Ctrl+E")],
        "action-tip-edit",
        AppView::open_edit_for_row,
    ),
    rows(
        "delete",
        "action-label-delete",
        &["delete", "remove"],
        &[("delete", "Delete")],
        "action-tip-delete",
        AppView::delete_selected_rows,
    ),
    rows(
        "move",
        "action-label-move",
        &["move", "group"],
        &[],
        "action-tip-move",
        AppView::move_selected_rows,
    ),
    row(
        "share",
        "action-label-share",
        &["share", "link", "qr"],
        &[],
        "action-tip-share",
        AppView::open_share_for_row,
    ),
    row(
        "assign-preset",
        "action-label-assign-preset",
        &["preset", "assign", "attach", "routing"],
        &[],
        "action-tip-assign-preset",
        AppView::open_preset_attach_for_row,
    ),
    row(
        "duplicate",
        "action-label-duplicate",
        &["duplicate", "copy", "clone"],
        &[],
        "action-tip-duplicate",
        AppView::commit_duplicate,
    ),
    row(
        "ping",
        "action-label-ping",
        &["ping", "tcp"],
        &[],
        "action-tip-ping",
        AppView::ping_row,
    ),
    row(
        "reset-traffic",
        "action-label-reset-traffic",
        &["reset", "traffic", "counters"],
        &[],
        "action-tip-reset-traffic",
        AppView::open_traffic_reset_for_row,
    ),
    row(
        "reset-usage",
        "action-label-reset-usage",
        &["reset", "usage", "counters"],
        &[],
        "action-tip-reset-usage",
        AppView::open_usage_reset_for_row,
    ),
    row(
        "auto-start",
        "action-label-auto-start",
        &["auto", "start", "launch"],
        &[],
        "action-tip-auto-start",
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
    target_row(view, target, cx).is_some() || !target_rows(view, target, cx).is_empty()
}

fn target_rows(view: &AppView, target: ActionTarget<'_>, cx: &App) -> Vec<String> {
    match target {
        ActionTarget::Row(id) => vec![id.to_string()],
        ActionTarget::Selection => {
            let libraries = view.libraries.read(cx);
            let delegate = libraries.delegate();
            let visible = delegate.visible_ids();
            let mut ids: Vec<String> = visible
                .into_iter()
                .filter(|id| view.is_selected(id))
                .collect();
            // A selected row a filter hides still counts: the selection is not
            // a visible-row set.
            let mut hidden: Vec<String> = delegate
                .row_ids()
                .into_iter()
                .filter(|id| view.is_selected(id))
                .filter(|id| !ids.contains(id))
                .collect();
            ids.append(&mut hidden);
            ids
        }
    }
}

const fn selected(
    id: &'static str,
    label_key: &'static str,
    keywords: &'static [&'static str],
    shortcuts: &'static [(&'static str, &'static str)],
    run: fn(&mut AppView, &mut Window, &mut Context<AppView>),
) -> ActionDef {
    ActionDef {
        id,
        label_key,
        keywords,
        tooltip_key: None,
        shortcuts,
        enabled: always,
        run: Handler::Selected(run),
    }
}

const fn row(
    id: &'static str,
    label_key: &'static str,
    keywords: &'static [&'static str],
    shortcuts: &'static [(&'static str, &'static str)],
    tooltip_key: &'static str,
    run: fn(&mut AppView, &str, &mut Window, &mut Context<AppView>),
) -> ActionDef {
    ActionDef {
        id,
        label_key,
        keywords,
        tooltip_key: Some(tooltip_key),
        shortcuts,
        enabled: needs_row,
        run: Handler::Row(run),
    }
}

const fn rows(
    id: &'static str,
    label_key: &'static str,
    keywords: &'static [&'static str],
    shortcuts: &'static [(&'static str, &'static str)],
    tooltip_key: &'static str,
    run: fn(&mut AppView, &[String], &mut Window, &mut Context<AppView>),
) -> ActionDef {
    ActionDef {
        id,
        label_key,
        keywords,
        tooltip_key: Some(tooltip_key),
        shortcuts,
        enabled: needs_row,
        run: Handler::Rows(run),
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
            "clear-selection",
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
            "inspector-toggle",
            "start-all",
            "stop-all",
            "test-all",
            "runtime-stats",
            "traffic-stats",
            "row-columns",
            "text-size",
            "density-comfortable",
            "density-compact",
        ],
    ),
    (
        "Sort",
        &[
            "sort-latency",
            "sort-download",
            "sort-upload",
            "sort-out-ip",
            "sort-type-security",
            "sort-traffic-total",
            "sort-traffic-down",
            "sort-traffic-up",
            "sort-direction",
            "sort-none",
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
        Handler::Rows(run) => {
            let ids = target_rows(view, target, cx);
            if ids.is_empty() {
                window.push_notification(Notification::info(no_selection_message()), cx);
                return;
            }
            run(view, &ids, window, cx);
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

fn clear_selection(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.clear_selection_unless_dialog(window, cx);
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

/// Ctrl-I owns the inspector's presence: it opens the panel and closes the one
/// that is already open.
fn toggle_inspector_presence(view: &mut AppView, _window: &mut Window, _cx: &mut Context<AppView>) {
    view.toggle_inspector_presence();
}

fn sort_latency(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::Latency, window, cx);
}

fn sort_download(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::Download, window, cx);
}

fn sort_upload(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::Upload, window, cx);
}

fn sort_out_ip(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::OutIp, window, cx);
}

fn sort_type_security(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::TypeSecurity, window, cx);
}

fn sort_traffic_total(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::TrafficTotal, window, cx);
}

fn sort_traffic_down(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::TrafficDown, window, cx);
}

fn sort_traffic_up(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::TrafficUp, window, cx);
}

fn sort_none(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.set_library_sort(LibrarySortKey::None, window, cx);
}

fn sort_direction(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    view.toggle_library_sort_direction(window, cx);
}

fn text_size(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    // Task 13: the palette keeps its one-step toggle; "large" is the slider's
    // 125% stop, and any custom stop toggles back to the stock 100%.
    let next = if view.store.text_size() == TextSize::LARGE {
        TextSize::DEFAULT
    } else {
        TextSize::LARGE
    };
    view.commit_text_size(next, window, cx);
}
