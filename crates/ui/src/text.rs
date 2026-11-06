//! UI copy helpers. The core crate cannot reach the Fluent bundle, so any
//! shared string that names UI state (title, counts, columns) lives here; the
//! rest of the pure builders stay in `manyproxy_core::text`.

use crate::app::settings::SettingsSection;
use crate::*;

/// T-26: the one `· N running` suffix shared by the window title and the tray
/// tooltip, so the two surfaces can never drift. Empty while nothing runs.
pub fn running_title_suffix(running: usize) -> String {
    if running == 0 {
        String::new()
    } else {
        format!(" {}", t!("running-suffix", running = running))
    }
}
/// T-26: composed window title for a running count — `ManyProxy` at zero,
/// `ManyProxy · N running` above it. Consumed by the OS window title, the
/// custom titlebar label, and (through `tray::tray_tooltip`) the tray tooltip.
pub fn window_title_text(running: usize) -> String {
    format!("{}{}", t!("app-title"), running_title_suffix(running))
}
pub fn search_count_text(visible: usize, total: usize) -> String {
    t!("search-count", visible = visible, total = total)
}
/// K12/A5-1: the status strip's first segment — library totals, singular at 1.
/// F6: the flag it counts means restore-on-launch, not "will start now".
pub fn status_count_text(total: usize, enabled: usize) -> String {
    t!("status-count", total = total, enabled = enabled)
}
/// K12/A5-1: the sort header names its direction in words. The old `↑`/`↓`
/// glyphs degrade to `1`/`T` at the small header size, so the marker is a word
/// suffix that reads at any scale. `None` means "library order" and adds
/// nothing; the retired toggle keeps its fixed direction label.
pub fn sort_header_marker(sort: LibrarySort) -> String {
    match sort.key {
        LibrarySortKey::None => String::new(),
        LibrarySortKey::TypeSecurity => format!(" {}", t!("sort-marker-fixed")),
        _ if sort.direction == SortDirection::Descending => {
            format!(" {}", t!("sort-marker-desc"))
        }
        _ => format!(" {}", t!("sort-marker-asc")),
    }
}
pub fn facet_tip_text(
    dimension: &str,
    active_display: &str,
    visible: usize,
    total: usize,
) -> String {
    t!(
        "facet-tip",
        dimension = dimension,
        active = active_display,
        visible = visible,
        total = total
    )
}
pub fn no_matches_text() -> String {
    t!("library-no-matches")
}
pub fn empty_library_title() -> String {
    t!("library-empty-title")
}
pub fn empty_library_text() -> String {
    t!("library-empty-text")
}

/// The core enums keep their labels for persistence and core tests; the UI
/// names them through these wrappers so the visible words come from Fluent.
pub(crate) fn library_column_label(column: LibraryColumn) -> String {
    match column {
        LibraryColumn::Group => t!("column-group"),
        LibraryColumn::Protocol => t!("column-protocol"),
        LibraryColumn::Upstream => t!("column-upstream"),
        LibraryColumn::Endpoint => t!("column-endpoint"),
        LibraryColumn::Health => t!("column-health"),
        LibraryColumn::Traffic => t!("column-traffic"),
    }
}

pub(crate) fn library_sort_key_label(key: LibrarySortKey) -> String {
    match key {
        LibrarySortKey::None => t!("sort-key-none"),
        LibrarySortKey::Latency => t!("sort-key-latency"),
        LibrarySortKey::Download => t!("sort-key-download"),
        LibrarySortKey::Upload => t!("sort-key-upload"),
        LibrarySortKey::OutIp => t!("sort-key-out-ip"),
        LibrarySortKey::TrafficTotal => t!("sort-key-traffic-total"),
        LibrarySortKey::TrafficDown => t!("sort-key-traffic-down"),
        LibrarySortKey::TrafficUp => t!("sort-key-traffic-up"),
        LibrarySortKey::TypeSecurity => t!("sort-key-type-security"),
    }
}

pub(crate) fn sort_direction_label(direction: SortDirection) -> String {
    match direction {
        SortDirection::Ascending => t!("sort-direction-ascending"),
        SortDirection::Descending => t!("sort-direction-descending"),
    }
}

pub(crate) fn latency_bucket_label(bucket: LatencyBucket) -> String {
    match bucket {
        LatencyBucket::Any => t!("latency-any"),
        LatencyBucket::Ms100 => t!("latency-100"),
        LatencyBucket::Ms300 => t!("latency-300"),
        LatencyBucket::Ms1000 => t!("latency-1000"),
    }
}

pub(crate) fn group_prune_label(kind: GroupPruneKind) -> String {
    match kind {
        GroupPruneKind::Duplicates => t!("prune-duplicates"),
        GroupPruneKind::Unavailable => t!("prune-unavailable"),
        GroupPruneKind::Invalid => t!("prune-invalid"),
        GroupPruneKind::ClearTests => t!("prune-clear-tests"),
    }
}

/// The toolbar's one start/stop control. The glyphs are not copy; the name is.
pub(crate) fn global_toggle_text(running: usize, busy: bool) -> (String, String) {
    let glyph = if busy {
        "…"
    } else if running == 0 {
        "▶"
    } else {
        "■"
    };
    let name = if busy {
        t!("toolbar-connecting")
    } else if running == 0 {
        t!("action-start-all")
    } else {
        t!("action-stop-all")
    };
    (glyph.to_string(), name)
}

/// K12/A5-1: the center segment names the engine state only; the endpoint
/// count lives in the first segment.
pub(crate) fn engine_status_text(engine: &EngineController) -> String {
    match engine.status() {
        EngineStatus::Stopped => t!("engine-status-stopped"),
        EngineStatus::Running => t!("engine-status-running"),
        EngineStatus::Crashed { cause } => t!("engine-status-crashed", cause = cause),
    }
}

/// T-34: one session-sourced notice — the previous run left a running
/// snapshot behind without reaching any clean-exit path.
pub(crate) const UNCLEAN_EXIT_NOTICE: &str =
    "Previous run carried running endpoints and did not exit cleanly.";
pub(crate) fn unclean_exit_notice(
    previous_clean: bool,
    last_snapshot: &[String],
) -> Option<Notification> {
    if previous_clean || last_snapshot.is_empty() {
        return None;
    }
    Some(Notification::warning(UNCLEAN_EXIT_NOTICE))
}
/// K17: the bulk delete outcome toast — one tint per truth (clean = success,
/// nothing removed = error, partial = warning) — and the sentence the Undo
/// toast carries. The blocking confirm the old copy described is gone
/// (spec line 91); naming the rows stays.
pub(crate) fn bulk_delete_note(removed: &[String], failed: &[String]) -> ToastSpec {
    let text = bulk_delete_result_message(removed, failed);
    if failed.is_empty() {
        ToastSpec::success(text)
    } else if removed.is_empty() {
        ToastSpec::error(text)
    } else {
        ToastSpec::warning(text)
    }
}
pub(crate) fn serving_tone(is_running: bool, health: Option<&str>, crashed: bool) -> ServingTone {
    let dead_evidence = crashed || health.map(str::trim) == Some("dead");
    if is_running && dead_evidence {
        ServingTone::Failed
    } else if is_running {
        ServingTone::Serving
    } else {
        ServingTone::Stopped
    }
}
pub(crate) fn lamp_access_label(tone: ServingTone, name: &str) -> String {
    match tone {
        ServingTone::Failed => t!("lamp-failed", name = name),
        ServingTone::Serving => t!("lamp-serving", name = name),
        ServingTone::Stopped => t!("lamp-stopped", name = name),
    }
}
/// T-33: the message a Settings field shows inline — the field's own sentence
/// when the store named one, otherwise the error's display text.
pub(crate) fn store_error_text(error: &StoreError) -> String {
    field_error_target(error)
        .map(|(_, message)| message)
        .unwrap_or_else(|| error.to_string())
}
pub(crate) fn settings_clear_traffic_text(total: usize) -> String {
    t!("settings-clear-traffic-confirm", total = total)
}
pub(crate) fn settings_clear_health_text() -> String {
    t!("settings-clear-health-confirm")
}
pub(crate) fn settings_clear_snapshot_text() -> String {
    t!("settings-clear-snapshot-confirm")
}
pub(crate) fn preset_summary_text(presets: Option<usize>) -> String {
    match presets {
        Some(count) => t!("preset-summary", count = count),
        None => t!("preset-summary-unavailable"),
    }
}
pub(crate) fn preset_delete_confirm_description(name: &str, attached: &[String]) -> String {
    t!(
        "preset-delete-description",
        name = name,
        count = attached.len(),
        names = attached.join(", ")
    )
}
pub(crate) fn preset_row_summary(preset: &store::RoutePreset, attached: &[String]) -> String {
    let rules = match preset.rules() {
        Ok(rules) => t!("preset-row-rules", count = rules.len()),
        Err(_) => t!("preset-row-invalid-rules"),
    };
    let attached = if attached.is_empty() {
        t!("preset-row-unattached")
    } else {
        t!("preset-row-attached", names = attached.join(", "))
    };
    t!(
        "preset-row-summary",
        name = preset.name.as_str(),
        source = preset.source.as_str(),
        rules = rules,
        attached = attached
    )
}
pub(crate) fn update_all_confirm_text(included: usize, skipped: usize) -> String {
    if skipped == 0 {
        t!("subs-update-all-confirm", included = included)
    } else {
        t!(
            "subs-update-all-confirm-skipped",
            included = included,
            skipped = skipped
        )
    }
}
pub(crate) fn logs_overwrite_text(path: &std::path::Path) -> String {
    t!("logs-overwrite", path = path.display().to_string())
}
pub(crate) fn logs_save_path_text() -> String {
    t!(
        "logs-save-path",
        path = Store::default_logs_path().display().to_string()
    )
}
pub(crate) fn group_delete_confirm_text(name: &str, count: usize) -> String {
    if count == 0 {
        t!("group-delete-empty", name = name)
    } else {
        t!("group-delete-rows", name = name, count = count)
    }
}
pub(crate) fn group_delete_confirm_description(
    name: &str,
    count: usize,
    subscriptions: usize,
) -> String {
    if subscriptions == 0 {
        return group_delete_confirm_text(name, count);
    }
    if count == 0 {
        return t!("group-delete-subs-only", name = name, subs = subscriptions);
    }
    t!(
        "group-delete-mixed",
        name = name,
        count = count,
        subs = subscriptions
    )
}
pub(crate) fn stop_all_confirm_description(names: &[String]) -> String {
    let named = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    t!("stop-all-confirm", count = names.len(), names = named)
}
pub(crate) fn close_confirm_description(running_names: &[String]) -> Option<String> {
    if running_names.is_empty() {
        return None;
    }
    let names = running_names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Some(t!(
        "close-confirm",
        count = running_names.len(),
        names = names
    ))
}
pub(crate) fn sidecar_presence_line(file_name: &str, found: bool) -> String {
    if found {
        t!("sidecar-found", file = file_name)
    } else {
        t!("sidecar-missing", file = file_name)
    }
}
pub(crate) fn about_body(
    version: &str,
    git_hash: &str,
    sidecars: &[String],
    db_path: &str,
    export_path: &str,
) -> String {
    let sidecars = sidecars
        .iter()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let shortcuts = SHORTCUT_LINES
        .iter()
        .map(|(action, keys)| format!("  {}: {keys}", shortcut_action_label(action)))
        .collect::<Vec<_>>()
        .join("\n");
    t!(
        "about-body",
        version = version,
        hash = git_hash,
        sidecars = sidecars,
        db = db_path,
        export = export_path,
        shortcuts = shortcuts
    )
}
pub(crate) fn export_count_fragment(total: usize, importable: usize) -> String {
    if importable == total {
        t!("export-count-all", total = total)
    } else {
        t!("export-count-some", total = total, importable = importable)
    }
}
pub(crate) fn export_count_message(total: usize, importable: usize, path: &str) -> String {
    t!(
        "export-count-message",
        count = export_count_fragment(total, importable),
        path = path
    )
}
pub(crate) fn lan_confirm_title() -> String {
    t!("lan-confirm-title")
}
pub(crate) fn lan_confirm_text(name: &str, bind: &str) -> String {
    t!("lan-confirm-text", name = name, bind = bind)
}
pub(crate) fn lan_confirm_description(entries: &[(String, String)]) -> String {
    let mut lines: Vec<String> = entries
        .iter()
        .map(|(name, bind)| t!("lan-entry", name = name.as_str(), bind = bind.as_str()))
        .collect();
    lines.sort();
    t!("lan-confirm-description", entries = lines.join(", "))
}
pub(crate) fn lan_exposure_warning(name: &str, bind: &str) -> Option<String> {
    if !requires_lan_confirm(bind) {
        None
    } else {
        Some(t!("lan-exposure-warning", name = name, bind = bind))
    }
}
pub(crate) fn lan_hint_for_bind(bind: &str) -> Option<String> {
    if bind.trim().is_empty() || !requires_lan_confirm(bind) {
        return None;
    }
    Some(t!("lan-hint", bind = bind.trim()))
}
pub(crate) fn import_preview_message(valid: usize, invalid: usize) -> String {
    t!("import-preview", valid = valid, invalid = invalid)
}
pub(crate) fn import_files_label(
    file_hint: Option<&str>,
    multi_files: Option<&[(String, String)]>,
) -> Option<String> {
    let (many, names): (bool, Vec<String>) = match multi_files {
        Some(files) => (
            true,
            files.iter().map(|(name, _)| file_basename(name)).collect(),
        ),
        None => (false, file_hint.map(file_basename).into_iter().collect()),
    };
    if names.is_empty() {
        return None;
    }
    if many {
        Some(t!("import-files-many", names = names.join(", ")))
    } else {
        Some(t!("import-files-one", name = names.join(", ")))
    }
}
pub(crate) fn export_preview_description(
    total: usize,
    importable: usize,
    path: &str,
    exists: bool,
) -> String {
    let count = export_count_fragment(total, importable);
    if exists {
        t!("export-preview-overwrite", count = count, path = path)
    } else {
        t!("export-preview", count = count, path = path)
    }
}
pub(crate) fn bind_port_collision_warning(
    rows: &[ProxyConfig],
    exclude_id: Option<&str>,
    bind_raw: &str,
    port_raw: &str,
) -> Option<String> {
    let port: u16 = port_raw.trim().parse().ok()?;
    if port == 0 {
        return None;
    }
    let bind = bind_raw.trim();
    let bind = if bind.is_empty() { DEFAULT_BIND } else { bind };
    rows.iter()
        .filter(|row| Some(row.id.as_str()) != exclude_id)
        .find(|row| row.bind_ip == bind && row.port == port)
        .map(|row| {
            t!(
                "bind-port-collision",
                bind = bind,
                port = i64::from(port),
                name = row.name.as_str()
            )
        })
}
pub(crate) fn http_subscription_warning(url: &str) -> Option<String> {
    match subscription_fetch_policy(url) {
        SubscriptionFetchPolicy::Warn { .. } => Some(t!("sub-http-warning")),
        _ => None,
    }
}
pub(crate) fn format_interval_label(secs: i64) -> String {
    if secs <= 0 {
        return t!("interval-always");
    }
    if secs < 60 {
        return t!("interval-seconds", secs = secs);
    }
    if secs < 3_600 {
        return t!("interval-minutes", minutes = secs / 60);
    }
    if secs < 604_800 {
        return t!("interval-hours", hours = secs / 3_600);
    }
    t!("interval-days", days = secs / 86_400)
}
pub(crate) fn format_last_refresh_label(last_refresh_at: Option<i64>, now: i64) -> String {
    match last_refresh_at {
        None => t!("last-refresh-never"),
        Some(last) => {
            let delta = now.saturating_sub(last);
            if delta < 60 {
                t!("last-refresh-just-now")
            } else if delta < 3_600 {
                t!("last-refresh-minutes", minutes = delta / 60)
            } else if delta < 86_400 {
                t!("last-refresh-hours", hours = delta / 3_600)
            } else {
                t!("last-refresh-days", days = delta / 86_400)
            }
        }
    }
}
pub(crate) fn format_expire_label(expire_at: i64, now: i64) -> String {
    let delta = expire_at.saturating_sub(now);
    if delta <= 0 {
        return t!("expire-expired");
    }
    if delta < 5_400 {
        return t!("expire-minutes", minutes = (delta / 60).max(1));
    }
    if delta < 172_800 {
        return t!("expire-hours", hours = delta / 3_600);
    }
    t!("expire-days", days = delta / 86_400)
}
fn group_quota_line(subs: &[&ProxySubscription], now: i64) -> Option<String> {
    if subs.is_empty() {
        return None;
    }
    let last = subs.iter().filter_map(|sub| sub.last_refresh_at).max();
    let mut segments = vec![t!(
        "group-quota-last-update",
        last = format_last_refresh_label(last, now)
    )];
    let mut used: i64 = 0;
    let mut used_known = false;
    let mut total: i64 = 0;
    let mut total_known = false;
    let mut expire: Option<i64> = None;
    for sub in subs {
        if let Some(value) = sub.quota_used {
            used = used.saturating_add(value.max(0));
            used_known = true;
        }
        if let Some(value) = sub.quota_total {
            total = total.saturating_add(value.max(0));
            total_known = true;
        }
        expire = match (expire, sub.quota_expire) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
    }
    if used_known {
        segments.push(t!(
            "group-quota-used",
            bytes = format_bytes(used.max(0) as u64)
        ));
    }
    if total_known {
        segments.push(t!(
            "group-quota-remain",
            bytes = format_bytes(total.saturating_sub(used).max(0) as u64)
        ));
    }
    if let Some(expire) = expire {
        segments.push(t!(
            "group-quota-expire",
            expire = format_expire_label(expire, now)
        ));
    }
    Some(segments.join(" | "))
}
pub(crate) fn export_confirm_description() -> String {
    t!("export-warning")
}
pub(crate) fn export_overwrite_confirm_description(path: &str) -> String {
    t!("export-overwrite-description", path = path)
}
pub(crate) fn log_level_label(level: Option<LogLevel>) -> String {
    match level {
        None => t!("logs-level-all"),
        Some(LogLevel::Debug) => t!("logs-level-debug"),
        Some(LogLevel::Info) => t!("logs-level-info"),
        Some(LogLevel::Warn) => t!("logs-level-warn"),
        Some(LogLevel::Error) => t!("logs-level-error"),
    }
}
pub(crate) fn log_level_tooltip(level: Option<LogLevel>) -> String {
    match level {
        None => t!("logs-level-tooltip-all"),
        Some(level) => t!("logs-level-tooltip", label = log_level_label(Some(level))),
    }
}
pub(crate) fn prune_duplicates_confirm_text(
    group: &str,
    total: usize,
    preview: &[String],
) -> String {
    if preview.is_empty() {
        return t!("prune-duplicates-confirm", group = group, total = total);
    }
    if total > preview.len() {
        return t!(
            "prune-duplicates-confirm-more",
            group = group,
            total = total,
            preview = preview.join(", "),
            more = total - preview.len()
        );
    }
    t!(
        "prune-duplicates-confirm-preview",
        group = group,
        total = total,
        preview = preview.join(", ")
    )
}
pub(crate) fn prune_unavailable_confirm_text(group: &str, count: usize) -> String {
    t!("prune-unavailable-confirm", group = group, count = count)
}
pub(crate) fn prune_invalid_confirm_text(group: &str, count: usize) -> String {
    t!("prune-invalid-confirm", group = group, count = count)
}
pub(crate) fn clear_group_tests_confirm_text(group: &str, count: usize) -> String {
    t!("prune-clear-tests-confirm", group = group, count = count)
}
pub(crate) fn auto_clear_opt_in_note(group: &str) -> String {
    t!("auto-clear-opt-in-note", group = group)
}
pub(crate) fn traffic_stats_summary_text(
    period: store::TrafficPeriod,
    exits: usize,
    rx: u64,
    tx: u64,
) -> String {
    t!(
        "stats-summary",
        period = period.label(),
        exits = exits,
        rx = format_bytes(rx),
        tx = format_bytes(tx)
    )
}
pub(crate) fn traffic_other_line(split: &TrafficExitSplit) -> Option<String> {
    (split.other_exits > 0).then(|| {
        t!(
            "stats-other",
            count = split.other_exits,
            down = format_bytes(split.other_rx),
            up = format_bytes(split.other_tx)
        )
    })
}
pub(crate) fn group_row_views(
    counts: &[(String, usize)],
    subs: &[ProxySubscription],
    skip: &dyn Fn(&str) -> bool,
    auto_clear: &dyn Fn(&str) -> bool,
    now: i64,
) -> Vec<GroupRowView> {
    use std::collections::BTreeMap;
    let mut counts_map: BTreeMap<&str, usize> = BTreeMap::new();
    for (group, count) in counts {
        counts_map.insert(group.as_str(), *count);
    }
    let mut feeding: BTreeMap<&str, Vec<&ProxySubscription>> = BTreeMap::new();
    for sub in subs {
        feeding.entry(sub.group.as_str()).or_default().push(sub);
    }
    let mut names: Vec<&str> = counts_map.keys().copied().collect();
    for name in feeding.keys() {
        if !counts_map.contains_key(name) {
            names.push(name);
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let feeding = feeding.get(name).cloned().unwrap_or_default();
            let count = counts_map.get(name).copied().unwrap_or(0);
            let kind = if feeding.is_empty() {
                t!("group-kind-basic")
            } else {
                t!("group-kind-subscription", count = feeding.len())
            };
            let mut quota = group_quota_line(&feeding, now);
            let skipped = skip(name);
            if skipped {
                let marker = t!("group-skip-marker");
                quota = Some(match quota {
                    Some(line) => t!("group-quota-with-skip", line = line, marker = marker),
                    None => marker,
                });
            }
            GroupRowView {
                name: name.to_string(),
                count,
                kind,
                quota,
                skip: skipped,
                auto_clear: auto_clear(name),
                sub_ids: feeding.iter().map(|sub| sub.id.clone()).collect(),
            }
        })
        .collect()
}
pub(crate) fn subscription_failures_text(failures: &[ImportFailure]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let items = failures
        .iter()
        .map(|failure| {
            t!(
                "import-item-failure",
                index = failure.index + 1,
                label = failure.label.as_str(),
                reason = failure.reason.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    truncate(
        &t!("sub-failures", count = failures.len(), items = items),
        400,
    )
}
pub(crate) fn rule_set_summary(row: &RuleSetRow, used_by: &[String]) -> String {
    let mut parts = vec![
        redact_link(&row.url),
        format_bytes(row.bytes),
        format_last_refresh_label(Some(row.downloaded_at), now_unix()),
    ];
    if !used_by.is_empty() {
        parts.push(t!("rule-set-used-by", names = used_by.join(", ")));
    }
    parts.join(" · ")
}
pub(crate) fn shortcut_action_label(action: &str) -> String {
    match action {
        "Add configuration" => t!("shortcut-add-configuration"),
        "Import configurations" => t!("shortcut-import-configurations"),
        "Paste clipboard into Import" => t!("shortcut-paste-clipboard"),
        "Start selected" => t!("shortcut-start-selected"),
        "Stop selected" => t!("shortcut-stop-selected"),
        "Test selected" => t!("shortcut-test-selected"),
        "Stop Testing" => t!("shortcut-stop-testing"),
        "Edit selected" => t!("shortcut-edit-selected"),
        "Delete selected" => t!("shortcut-delete-selected"),
        "Extend selection up" => t!("shortcut-extend-up"),
        "Extend selection down" => t!("shortcut-extend-down"),
        "Select all visible" => t!("shortcut-select-all"),
        "Focus search" => t!("shortcut-focus-search"),
        "Inspector details" => t!("shortcut-inspector-details"),
        "Inspector health" => t!("shortcut-inspector-health"),
        "Inspector logs" => t!("shortcut-inspector-logs"),
        "Inspector links" => t!("shortcut-inspector-links"),
        "Inspector traffic" => t!("shortcut-inspector-traffic"),
        "Uncheck all" => t!("shortcut-uncheck-all"),
        "Copy URLs" => t!("shortcut-copy-urls"),
        "Command palette" => t!("shortcut-command-palette"),
        "Update the group in focus" => t!("shortcut-update-group"),
        other => other.to_string(),
    }
}
pub(crate) fn simple_rules_box_label(r#box: SimpleRulesBox) -> String {
    match r#box {
        SimpleRulesBox::Direct => t!("simple-box-direct"),
        SimpleRulesBox::Block => t!("simple-box-block"),
        SimpleRulesBox::Proxy => t!("simple-box-proxy"),
    }
}
pub(crate) fn settings_section_label(section: SettingsSection) -> String {
    match section {
        SettingsSection::General => t!("settings-section-general"),
        SettingsSection::Probing => t!("settings-section-probing"),
        SettingsSection::Updates => t!("settings-section-updates"),
        SettingsSection::Engine => t!("settings-section-engine"),
        SettingsSection::Maintenance => t!("settings-section-maintenance"),
        SettingsSection::Shortcuts => t!("settings-section-shortcuts"),
    }
}
pub(crate) fn appearance_choice_label(value: &str) -> String {
    match value {
        "dark" => t!("settings-appearance-dark"),
        "light" => t!("settings-appearance-light"),
        _ => t!("settings-appearance-system"),
    }
}
pub(crate) fn appearance_choice_tip(value: &str) -> String {
    match value {
        "dark" => t!("settings-appearance-tip-dark"),
        "light" => t!("settings-appearance-tip-light"),
        _ => t!("settings-appearance-tip-system"),
    }
}
pub(crate) fn accent_label(accent: Accent) -> String {
    match accent {
        Accent::Blue => t!("accent-blue"),
        Accent::Indigo => t!("accent-indigo"),
        Accent::Violet => t!("accent-violet"),
        Accent::Magenta => t!("accent-magenta"),
        Accent::Rose => t!("accent-rose"),
        Accent::Teal => t!("accent-teal"),
        Accent::Amber => t!("accent-amber"),
    }
}
pub(crate) fn text_size_label(size: TextSize) -> String {
    t!("text-size-percent", percent = size.percent())
}
pub(crate) fn close_action_label(action: CloseAction) -> String {
    match action {
        CloseAction::Tray => t!("close-tray"),
        CloseAction::Quit => t!("close-quit"),
    }
}
pub(crate) fn close_action_explanation(action: CloseAction) -> String {
    match action {
        CloseAction::Tray => t!("close-tray-explanation"),
        CloseAction::Quit => t!("close-quit-explanation"),
    }
}
pub(crate) fn window_corners_label(corners: WindowCorners) -> String {
    match corners {
        WindowCorners::Auto => t!("corners-auto"),
        WindowCorners::Rounded => t!("corners-rounded"),
        WindowCorners::Small => t!("corners-small"),
        WindowCorners::Square => t!("corners-square"),
    }
}
pub(crate) fn window_corners_explanation(corners: WindowCorners) -> String {
    match corners {
        WindowCorners::Auto => t!("corners-auto-explanation"),
        WindowCorners::Rounded => t!("corners-rounded-explanation"),
        WindowCorners::Small => t!("corners-small-explanation"),
        WindowCorners::Square => t!("corners-square-explanation"),
    }
}
pub(crate) fn test_all_scope_label(scope: TestAllScope) -> String {
    match scope {
        TestAllScope::Running => t!("test-scope-running"),
        TestAllScope::Checked => t!("test-scope-checked"),
    }
}
pub(crate) fn shard_stats_summary(rows: &[ShardStatRow]) -> String {
    let live = rows
        .iter()
        .filter(|row| row.caption != Some(SHARD_NOT_RUNNING))
        .count();
    if live == 0 {
        return "no sidecar running".to_string();
    }
    let sampled = rows.iter().filter(|row| row.caption.is_none()).count();
    let mut summary = format!("{live} live shard(s), {sampled} charted");
    if rows
        .iter()
        .any(|row| row.caption == Some(SHARD_SAMPLING_UNAVAILABLE))
    {
        summary.push_str(", sampling unavailable");
    }
    summary
}
