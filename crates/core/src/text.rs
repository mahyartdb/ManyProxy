//! Copy and label helpers shared by more than one view.
//! Belongs: pure string builders that must not drift between surfaces.
//! Not here: layout, styling or one-off strings.

use crate::*;

/// T-26: the one `· N running` suffix shared by the window title and the tray
/// tooltip, so the two surfaces can never drift. Empty while nothing runs.
pub fn running_title_suffix(running: usize) -> String {
    if running == 0 {
        String::new()
    } else {
        format!(" · {running} running")
    }
}
/// T-26: composed window title for a running count — `ManyProxy` at zero,
/// `ManyProxy · N running` above it. Consumed by the OS window title, the
/// custom titlebar label, and (through `tray::tray_tooltip`) the tray tooltip.
pub fn window_title_text(running: usize) -> String {
    format!("{APP_TITLE}{}", running_title_suffix(running))
}
pub fn settings_clear_traffic_text(total: usize) -> String {
    format!("Clear traffic counters for all {total} configuration(s)? This cannot be undone.")
}
pub fn settings_clear_health_text() -> String {
    "Clear every persisted health snapshot? Rows read untested until the next probe. \
     This cannot be undone."
        .to_string()
}
pub fn settings_clear_snapshot_text() -> String {
    "Forget the running snapshot? The next launch starts with nothing running. The live \
     engine is untouched. This cannot be undone."
        .to_string()
}
pub fn preset_summary_text(presets: Option<usize>) -> String {
    match presets {
        Some(presets) => format!("{presets} routing preset(s) stored"),
        None => "Preset inventory unavailable".to_string(),
    }
}
pub fn logs_overwrite_text(path: &std::path::Path) -> String {
    format!(
        "Overwrite {} with the visible log lines? Secrets stay redacted.",
        path.display()
    )
}
pub fn logs_save_path_text() -> String {
    format!(
        "Save writes to {} (set MANYPROXY_LOGS to override the location)",
        Store::default_logs_path().display()
    )
}
pub fn group_delete_confirm_text(name: &str, count: usize) -> String {
    if count == 0 {
        format!("Delete empty group \"{name}\"? No rows move (none use it). This cannot be undone.")
    } else {
        format!(
            "Delete group \"{name}\" ({count} row(s))? Groups with rows refuse — move the rows \
             out first. This cannot be undone."
        )
    }
}
pub fn group_delete_confirm_description(name: &str, count: usize, subscriptions: usize) -> String {
    if subscriptions == 0 {
        return group_delete_confirm_text(name, count);
    }
    if count == 0 {
        return format!(
            "Delete group \"{name}\" ({subscriptions} subscription(s))? Groups feeding \
             subscriptions refuse — remove or regroup them first. This cannot be undone."
        );
    }
    format!(
        "Delete group \"{name}\" ({count} row(s) and {subscriptions} subscription(s))? Groups \
         with rows or subscriptions refuse — move the rows out and remove or regroup the \
         subscriptions first. This cannot be undone."
    )
}
pub fn close_confirm_description(running_names: &[String]) -> Option<String> {
    if running_names.is_empty() {
        return None;
    }
    let named = running_names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Closing now stops {} running endpoint(s): {named}. Stop all and close?",
        running_names.len()
    ))
}
pub fn sidecar_presence_line(file_name: &str, found: bool) -> String {
    if found {
        format!("{file_name}: found (app dir)")
    } else {
        format!("{file_name}: missing (place it next to the app exe)")
    }
}
pub fn about_body(
    version: &str,
    git_hash: &str,
    sidecars: &[String],
    db_path: &str,
    export_path: &str,
) -> String {
    let mut out = format!("ManyProxy v{version} (git {git_hash})\n\nSidecars (app dir only):\n");
    for line in sidecars {
        out.push_str(&format!("  {line}\n"));
    }
    out.push_str(&format!(
        "\nData:\n  library: {db_path}\n  export default: {export_path}\n"
    ));
    out.push_str("\nOpt-in only: autostart (HKCU Run entry) and session restore live in Settings, both default off; no auto-update.\n");
    out.push_str("\nShortcuts:\n");
    for (action, keys) in SHORTCUT_LINES {
        out.push_str(&format!("  {action}: {keys}\n"));
    }
    out
}
pub fn addsub_error_message(error: &ImportError) -> String {
    format!("Invalid subscription link: {error}")
}
pub fn effective_addsub_group(dialog_group: &str, payload_group: &str) -> String {
    let dialog = dialog_group.trim();
    if dialog.is_empty() {
        return payload_group.to_string();
    }
    if dialog == DEFAULT_GROUP && payload_group != DEFAULT_GROUP {
        return payload_group.to_string();
    }
    dialog.to_string()
}
pub fn export_count_fragment(total: usize, importable: usize) -> String {
    if importable == total {
        format!("{total} config(s)")
    } else {
        format!("{total} config(s) ({importable} importable)")
    }
}
pub fn export_count_message(total: usize, importable: usize, path: &str) -> String {
    format!(
        "Exported {} to {path}",
        export_count_fragment(total, importable)
    )
}
pub fn row_action_message(verb: &str, name: &str) -> String {
    format!("{verb} \"{name}\"")
}
pub fn import_preview_message(valid: usize, invalid: usize) -> String {
    format!("{valid} valid · {invalid} invalid")
}
pub fn import_success_summary(
    imported: usize,
    duplicates: usize,
    failures: &[crate::import::ImportFailure],
) -> String {
    let mut summary = format!(
        "Imported {imported} · {duplicates} duplicates skipped · {} failed",
        failures.len()
    );
    if !failures.is_empty() {
        summary.push_str(&format!(" — {}", format_import_failures(failures)));
    }
    summary
}
pub fn file_basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}
pub fn import_files_label(
    file_hint: Option<&str>,
    multi_files: Option<&[(String, String)]>,
) -> Option<String> {
    let (prefix, names): (&str, Vec<String>) = match multi_files {
        Some(files) => (
            "Files",
            files.iter().map(|(name, _)| file_basename(name)).collect(),
        ),
        None => ("File", file_hint.map(file_basename).into_iter().collect()),
    };
    if names.is_empty() {
        None
    } else {
        Some(format!("{prefix}: {}", names.join(", ")))
    }
}
pub fn export_preview_description(
    total: usize,
    importable: usize,
    path: &str,
    exists: bool,
) -> String {
    let mut out = format!("{} → {path}", export_count_fragment(total, importable));
    if exists {
        out.push_str(&format!(" — {path} already exists and will be overwritten"));
    }
    out
}
pub fn subscription_failures_text(failures: &[crate::import::ImportFailure]) -> String {
    if failures.is_empty() {
        String::new()
    } else {
        truncate(
            &format!(
                "item failures ({}): {}",
                failures.len(),
                format_import_failures(failures)
            ),
            400,
        )
    }
}
#[allow(clippy::too_many_arguments)]
pub fn coalesced_refresh_message(
    subs: usize,
    added: usize,
    updated: usize,
    removed: usize,
    failed: usize,
    conflicts: usize,
    in_use: usize,
) -> Option<(bool, String)> {
    if subs == 0 {
        return None;
    }
    let mut text = format!(
        "Background refresh: {subs} subscription(s): [+]Added {added} / [~]Updated {updated} / [-]Deleted {removed}"
    );
    if in_use > 0 {
        text.push_str(&format!(" / [=] Still in use {in_use}"));
    }
    if failed == 0 && conflicts == 0 {
        Some((false, text))
    } else {
        let mut full = format!("{text} · {failed} failed");
        if conflicts > 0 {
            full.push_str(&format!(" · {conflicts} conflict(s) — your edits kept"));
        }
        Some((true, full))
    }
}
pub fn start_all_message(count: usize) -> String {
    if count == 0 {
        "Nothing to start — the library is empty".to_string()
    } else {
        format!("Started {count} endpoint(s)")
    }
}
pub fn stop_all_message(running: usize) -> String {
    if running == 0 {
        "Nothing to stop — no endpoints are running".to_string()
    } else {
        format!("Stopped {running} endpoint(s)")
    }
}
pub fn stop_all_confirm_description(names: &[String]) -> String {
    let named = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Stop {} running endpoint(s): {named}? Endpoints close and flags clear.",
        names.len()
    )
}
pub fn restart_to_apply_message(name: &str) -> String {
    format!("Updated \"{name}\" — restart it to apply (still serving the previous config)")
}
pub fn removed_message(name: &str, was_running: bool) -> String {
    if was_running {
        format!("Removed \"{name}\" (endpoint stopped first)")
    } else {
        format!("Removed \"{name}\"")
    }
}
/// K5: the undo toast carries the same removed-row wording; the Undo action
/// button is the offer, the text never promises more than removal itself.
pub fn undo_restored_message(names: &[String], restored: usize) -> String {
    if restored == 1 {
        if let [single] = names {
            return format!("Restored \"{single}\" to its group");
        }
    }
    format!("Restored {restored} configuration(s) in their groups")
}
pub fn undo_failed_message(error: &StoreError) -> String {
    format!("Restore failed: {error}")
}
/// K5: the update feed is the only thing a failed check may name — no URLs,
/// statuses, or body echoes.
pub fn update_feed_failure_message() -> String {
    "Couldn't reach the update feed".to_string()
}
pub fn bulk_delete_result_message(removed: &[String], failed: &[String]) -> String {
    const SHOWN: usize = 3;
    fn listed(names: &[String]) -> String {
        let shown = names
            .iter()
            .take(SHOWN)
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ");
        if names.len() > SHOWN {
            format!("{shown} +{} more", names.len() - SHOWN)
        } else {
            shown
        }
    }
    if failed.is_empty() {
        if removed.is_empty() {
            return "Deleted 0 configuration(s)".to_string();
        }
        return format!(
            "Deleted {} configuration(s): {}",
            removed.len(),
            listed(removed)
        );
    }
    if removed.is_empty() {
        return format!(
            "Delete failed for {} configuration(s): {}",
            failed.len(),
            listed(failed)
        );
    }
    format!(
        "Deleted {} · {} failed: {}",
        removed.len(),
        failed.len(),
        listed(failed)
    )
}
pub fn duplicate_name(base: &str, taken: &[String]) -> String {
    let first = format!("{base} copy");
    if !taken.iter().any(|name| name == &first) {
        return first;
    }
    let mut index = 2usize;
    loop {
        let candidate = format!("{first} {index}");
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
        index += 1;
    }
}
pub fn probe_result_toast(name: &str, outcome: &TestOutcome, ping: bool) -> String {
    let verb = if ping { "Ping" } else { "Test" };
    match (outcome.sample.latency_ms, outcome.sample.error.as_deref()) {
        (Some(ms), _) => format!("{verb} \"{name}\": live, {ms}ms"),
        (None, Some(cause)) => format!("{verb} \"{name}\": dead — {cause}"),
        (None, None) => format!("{verb} \"{name}\": dead"),
    }
}
/// K5: the batched start toast (`Testing N…`) — groups and test-all runs say
/// once what is about to happen, then exactly one summary closes the story.
pub fn probe_start_toast(count: usize) -> String {
    format!("Testing {count}…")
}
/// K5: the batched result line; per-row `probe_result_toast` prose is for
/// single-row tests only.
pub fn probe_batched_toast(count: usize, live: usize, dead: usize) -> String {
    format!("Tested {count}: {live} live · {dead} dead")
}
pub fn apply_show_security(text: &mut String, row: &ProxyConfig) {
    text.push_str(" · ");
    text.push_str(row_security_label(&row.protocol, &row.params));
}
pub fn group_test_summary_toast(
    group: &str,
    count: usize,
    live: usize,
    dead: usize,
    stopped: bool,
) -> String {
    let base =
        format!("Tested {count} endpoint(s) in group \"{group}\": {live} live · {dead} dead");
    if stopped {
        format!("{base} (stopped early)")
    } else {
        base
    }
}
/// T-19: the toggle-on confirm's own acknowledgment text, exactly the ticket
/// wording; skip-delete-confirm can never substitute for it.
pub fn auto_clear_opt_in_note(group: &str) -> String {
    format!(
        "Unavailable rows in \"{group}\" will be removed automatically after group tests pass \
         with dead rows. Running rows and the row being edited are never removed."
    )
}
/// T-19: one summary per sweep; always plural "rows", the kept-reason list is
/// named only when not empty, in canonical running → editing → in-flight
/// batch order.
pub fn auto_clear_summary_toast(group: &str, removed: usize, kept: &[String]) -> String {
    if kept.is_empty() {
        format!("Auto-cleared {removed} unavailable rows in \"{group}\"")
    } else {
        format!(
            "Auto-cleared {removed} unavailable rows in \"{group}\" (kept: {})",
            kept.join(", ")
        )
    }
}
pub fn already_stopped_message(name: &str) -> String {
    format!("\"{name}\" is already stopped")
}
/// T-03: the badge carries the endpoint, not an orphaned colon — `Running ·
/// 127.0.0.1:10801 · 87ms`. The bind belongs to the endpoint, so the string
/// carries it; the latency segment appears only once a probe has run. A stopped
/// endpoint still names its bind so the header never has to repeat it.
pub fn serving_badge(is_running: bool, bind: &str, port: u16, health: Option<&str>) -> String {
    let bind = bind.trim();
    let endpoint = if bind.is_empty() {
        format!(":{port}")
    } else {
        format!("{bind}:{port}")
    };
    if !is_running {
        return format!("○ Stopped · {endpoint}");
    }
    match health.map(str::trim).filter(|summary| !summary.is_empty()) {
        Some(summary) => format!("● Running · {endpoint} · {summary}"),
        None => format!("● Running · {endpoint}"),
    }
}
/// K4: numeral chips left the rows, so this helper survives only for the
/// checked-set ordering tests that still pin the 1-based position.
#[cfg(any(test, feature = "test-support"))]
pub fn check_chip_tip(position: usize, total: usize) -> String {
    format!("Checked {position} of {total} — bulk action target")
}
pub fn acting_on_label(config: &ProxyConfig) -> String {
    format!("Acting on: {}", config.name)
}
pub fn already_running_message(name: &str) -> String {
    format!("\"{name}\" already running (restarted)")
}
pub fn dirty_endpoint_message(name: &str) -> String {
    format!("\"{name}\" has unapplied endpoint edits — press Enter to apply or Esc to revert")
}
pub fn skipped_dirty_message(names: &[String]) -> String {
    format!(
        "Skipped with unapplied edits (Enter applies, Esc reverts): {}",
        names.join(", ")
    )
}
pub fn bulk_start_unreadable_message(ids: &[String]) -> String {
    format!(
        "{} configuration(s) not started (could not read): {}",
        ids.len(),
        ids.join(", ")
    )
}
pub fn bulk_start_enable_flag_failed_message(names: &[String]) -> String {
    format!(
        "{} started but the enabled flag could not be saved: {}",
        names.len(),
        names.join(", ")
    )
}

/// T-05: the batch start named the rows it refused. ONE line, one reason
/// per row — a batch that started twenty rows and lost one says so.
pub fn bulk_start_failed_message(started: usize, failed: &[(String, String)]) -> String {
    let joined = failed
        .iter()
        .map(|(name, reason)| format!("{name}: {reason}"))
        .collect::<Vec<_>>()
        .join("; ");
    format!("Started {started}, {} failed: {joined}", failed.len())
}

/// T-05: the ONE apply refused the batch. Nothing new started, so the
/// message never claims that the rest carried on.
pub fn bulk_start_refused_message(
    requested: usize,
    reason: &str,
    failed: &[(String, String)],
) -> String {
    let mut detail = vec![reason.to_string()];
    detail.extend(failed.iter().map(|(name, why)| format!("{name}: {why}")));
    format!(
        "Could not start {requested} configuration(s): {} (nothing new started)",
        detail.join("; ")
    )
}
/// T-05: the bulk copy is honest about what it skipped — a stopped row is
/// never copied, and the toast says so instead of quietly counting less.
pub fn bulk_copy_proxies_message(copied: usize, skipped: usize) -> String {
    if skipped == 0 {
        format!("Copied {copied} proxies")
    } else {
        format!("Copied {copied} proxies ({skipped} checked rows are stopped and were skipped)")
    }
}

/// T-05: nothing was running, so nothing was copied — the toast says that
/// instead of claiming a copy of zero.
pub fn bulk_copy_nothing_message(skipped: usize) -> String {
    if skipped == 0 {
        "No running endpoint to copy".to_string()
    } else {
        format!("No running endpoint to copy ({skipped} checked rows are stopped and were skipped)")
    }
}

pub fn no_selection_message() -> String {
    "Select a configuration first".to_string()
}
pub fn start_busy_message() -> String {
    "Starting… already in progress".to_string()
}
pub fn global_toggle_present(running: usize, busy: bool) -> (&'static str, &'static str) {
    if busy {
        ("…", "Connecting…")
    } else if running == 0 {
        ("▶", "Start all")
    } else {
        ("■", "Stop all")
    }
}
pub fn start_all_partial_message(started: usize, failed_names: &[String]) -> String {
    let failed = failed_names.len();
    if failed == 0 {
        return start_all_message(started);
    }
    format!(
        "Started {started} endpoint(s), {failed} failed: {}",
        failed_names.join(", ")
    )
}
pub fn search_count_text(visible: usize, total: usize) -> String {
    format!("{visible} of {total}")
}
/// K12/A5-1: the status strip's first segment — library totals, singular at 1.
/// F6: the flag it counts means restore-on-launch, not "will start now".
pub fn status_count_text(total: usize, enabled: usize) -> String {
    format!(
        "{} {} · {} set for launch",
        total,
        if total == 1 {
            "configuration"
        } else {
            "configurations"
        },
        enabled
    )
}
/// K12/A5-1: the sort header names its direction in words. The old `↑`/`↓`
/// glyphs degrade to `1`/`T` at the small header size, so the marker is a word
/// suffix that reads at any scale. `None` means "library order" and adds
/// nothing; the retired toggle keeps its fixed direction label.
pub fn sort_header_marker(sort: LibrarySort) -> &'static str {
    match sort.key {
        LibrarySortKey::None => "",
        LibrarySortKey::TypeSecurity => " · fixed",
        _ if sort.direction == SortDirection::Descending => " · desc",
        _ => " · asc",
    }
}
pub fn facet_tip_text(
    dimension: &str,
    active_display: &str,
    visible: usize,
    total: usize,
) -> String {
    format!("Filter by {dimension}: {active_display} · {visible} of {total} shown")
}
pub fn no_matches_text() -> String {
    "No matches — Clear search".to_string()
}
pub fn empty_library_title() -> String {
    "No proxy configurations yet".to_string()
}
pub fn empty_library_text() -> String {
    "Add one proxy, or use Import for many at once.".to_string()
}
pub fn crash_notification_text(detail: &str) -> String {
    format!("Engine crashed ({detail}) — View logs")
}
pub fn save_listen_gate(name: &str, bind_raw: &str, port_raw: &str) -> Result<(), String> {
    let port: u16 = match port_raw.trim() {
        "" => return Ok(()),
        raw => match raw.parse() {
            Ok(port) => port,
            Err(_) => return Ok(()),
        },
    };
    crate::engine::validate_listen_for_save(name, bind_raw, port).map_err(|error| error.to_string())
}
pub fn enabled_flag_message(name: &str, enabled: bool) -> String {
    if enabled {
        format!("\"{name}\" will restore on launch (Start serves it now)")
    } else {
        format!("\"{name}\" will not restore on launch (Stop closes the endpoint now)")
    }
}
pub fn db_list_error_message(error: &StoreError) -> String {
    format!("Cannot load library: {error}")
}
#[allow(clippy::too_many_arguments)]
pub fn refresh_summary_text(
    label: &str,
    added: usize,
    updated: usize,
    removed: usize,
    skipped: usize,
    duplicates: usize,
    failures: usize,
    conflicts: usize,
    in_use: usize,
    added_names: &[String],
    updated_names: &[String],
    removed_names: &[String],
    kept_in_use_names: &[String],
) -> (bool, String) {
    let mut summary =
        format!("{label}: [+]Added {added} / [~]Updated {updated} / [-]Deleted {removed}");
    if in_use > 0 {
        summary.push_str(&format!(" / [=] Still in use {in_use}"));
    }
    if skipped > 0 {
        summary.push_str(&format!(" · {skipped} skipped"));
    }
    if duplicates > 0 {
        summary.push_str(&format!(" · {duplicates} duplicates"));
    }
    if failures > 0 {
        summary.push_str(&format!(" · {failures} failed"));
    }
    if conflicts > 0 {
        summary.push_str(&format!(" · {conflicts} conflict(s) — your edits kept"));
    }
    if let Some(report) =
        refresh_named_report_block(added_names, updated_names, removed_names, kept_in_use_names)
    {
        summary.push('\n');
        summary.push_str(&report);
    }
    (failures > 0 || conflicts > 0, summary)
}
/// Cap on names rendered per report section; overflow folds into a
/// "+ N more…" line. Data beyond the cap still exists in the payload.
pub const REFRESH_REPORT_NAME_CAP: usize = 12;
/// Named identity block under the count line: `Added N profiles: [+] name /
/// Updated N profiles: [~] name / Deleted N profile: [-] name / Still in use,
/// so kept: [=] name`. Sections with zero rows are omitted; `None` when every
/// list is empty (count-only summary).
pub fn refresh_named_report_block(
    added_names: &[String],
    updated_names: &[String],
    removed_names: &[String],
    kept_in_use_names: &[String],
) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    for (verb, marker, names) in [
        ("Added", "[+]", added_names),
        ("Updated", "[~]", updated_names),
        ("Deleted", "[-]", removed_names),
    ] {
        if names.is_empty() {
            continue;
        }
        let noun = if names.len() == 1 {
            "profile"
        } else {
            "profiles"
        };
        let mut section = format!("{verb} {} {noun}:", names.len());
        for name in names.iter().take(REFRESH_REPORT_NAME_CAP) {
            section.push_str(&format!("\n{marker} {name}"));
        }
        let overflow = names.len().saturating_sub(REFRESH_REPORT_NAME_CAP);
        if overflow > 0 {
            section.push_str(&format!("\n+ {overflow} more…"));
        }
        sections.push(section);
    }
    if !kept_in_use_names.is_empty() {
        let mut section = "Still in use, so kept:".to_string();
        for name in kept_in_use_names.iter().take(REFRESH_REPORT_NAME_CAP) {
            section.push_str(&format!("\n[=] {name}"));
        }
        let overflow = kept_in_use_names
            .len()
            .saturating_sub(REFRESH_REPORT_NAME_CAP);
        if overflow > 0 {
            section.push_str(&format!("\n+ {overflow} more…"));
        }
        sections.push(section);
    }
    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n"))
    }
}
pub fn restore_done_message(outcome: &RestoreOutcome) -> String {
    format!(
        "Restored {} config(s), {} subscription(s)",
        outcome.configs, outcome.subscriptions
    )
}
pub fn unreadable_drop_message(names: &[String]) -> String {
    format!("Skipped unreadable file(s): {}", names.join(", "))
}
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
pub fn traffic_summary_text(rx: u64, tx: u64) -> Option<String> {
    if rx == 0 && tx == 0 {
        return None;
    }
    Some(format!("↓{} ↑{}", format_bytes(rx), format_bytes(tx)))
}
pub fn traffic_status_text(total_rx: u64, total_tx: u64) -> Option<String> {
    traffic_summary_text(total_rx, total_tx).map(|summary| format!("traffic {summary}"))
}
pub fn export_overwrite_confirm_description(path: &str) -> String {
    format!("\"{path}\" already exists. Overwrite it with the new export?")
}
/// The level radios filter by exact match, so the tooltip must not promise a
/// cumulative "and above" set.
pub fn log_level_tooltip(label: &str) -> String {
    if label == "All" {
        "Show logs of every level".to_string()
    } else {
        format!("Show only {label} records")
    }
}
/// K9/A7-5: the Settings → Engine body no longer paints the `Timers: …` line
/// (it was jargon at rest). The formatter stays as the tested summary of the
/// tick constants, so the numbers still have one source under test.
#[cfg(any(test, feature = "test-support"))]
pub fn tick_interval_summary() -> String {
    format!(
        "engine poll {ENGINE_POLL_TICK_SECS}s · subscriptions {DUE_SUBSCRIPTION_TICK_SECS}s · \
         reprobe {REPROBE_TICK_SECS}s · traffic {TRAFFIC_TICK_SECS}s"
    )
}
pub fn lan_hint_for_bind(bind: &str) -> Option<String> {
    if bind.trim().is_empty() || !requires_lan_confirm(bind) {
        return None;
    }
    Some(format!(
        "Leaving loopback exposes this machine — starting on {} needs a LAN confirm, and anyone on the network could use it",
        bind.trim()
    ))
}
pub fn bind_port_collision_warning(
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
            format!(
                "bind {bind}:{port} is already used by \"{}\" — saving is still allowed",
                row.name
            )
        })
}
pub fn http_subscription_warning(url: &str) -> Option<String> {
    match store::subscription_fetch_policy(url) {
        store::SubscriptionFetchPolicy::Warn { reason } => Some(reason.to_string()),
        _ => None,
    }
}
pub fn already_subscribed_message(label: &str) -> String {
    format!("Already subscribed as \"{label}\" — no duplicate added")
}
pub fn subscription_interval_failure_message(action: &str, label: &str, error: &str) -> String {
    format!("Subscription \"{label}\" {action} — could not set refresh interval: {error}")
}
pub fn format_interval_secs(secs: i64) -> String {
    if secs <= 0 {
        return "always".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3_600 {
        return format!("{}m", secs / 60);
    }
    if secs < 604_800 {
        return format!("{}h", secs / 3_600);
    }
    format!("{}d", secs / 86_400)
}
pub fn format_last_refresh(last_refresh_at: Option<i64>, now: i64) -> String {
    match last_refresh_at {
        None => "never refreshed".to_string(),
        Some(last) => {
            let delta = now.saturating_sub(last);
            if delta < 60 {
                "just now".to_string()
            } else if delta < 3_600 {
                format!("{}m ago", delta / 60)
            } else if delta < 86_400 {
                format!("{}h ago", delta / 3_600)
            } else {
                format!("{}d ago", delta / 86_400)
            }
        }
    }
}
pub fn format_expire(expire_at: i64, now: i64) -> String {
    let delta = expire_at.saturating_sub(now);
    if delta <= 0 {
        "expired".to_string()
    } else if delta < 5_400 {
        format!("{}m left", (delta / 60).max(1))
    } else if delta < 172_800 {
        format!("{}h left", delta / 3_600)
    } else {
        format!("{}d left", delta / 86_400)
    }
}
pub fn update_all_confirm_text(included: usize, skipped: usize) -> String {
    if skipped == 0 {
        format!("Update all {included} subscription(s)?")
    } else {
        format!(
            "Update {included} subscription(s)? {skipped} skipped (auto-refresh off for their group)."
        )
    }
}
pub fn preset_error_message(error: &crate::preset::PresetError) -> String {
    format!("Invalid routing preset: {error}")
}
pub fn preset_imported_message(name: &str, count: usize) -> String {
    format!("Routing preset \"{name}\" imported ({count} rule(s))")
}
pub fn preset_row_summary(preset: &store::RoutePreset, attached: &[String]) -> String {
    let rules = match preset.rules() {
        Ok(rules) => format!("{} rule(s)", rules.len()),
        Err(_) => "invalid rules".to_string(),
    };
    let attached = if attached.is_empty() {
        "unattached".to_string()
    } else {
        format!("attached to {}", attached.join(", "))
    };
    format!("{} · {} · {rules} · {attached}", preset.name, preset.source)
}
pub fn preset_delete_confirm_description(name: &str, attached: &[String]) -> String {
    format!(
        "Delete routing preset \"{name}\"? It is still attached to {} row(s): {}. \
         Deleting detaches every listed row first; unattached rows are unaffected.",
        attached.len(),
        attached.join(", ")
    )
}
/// T-12: one staged rule set, listed under the presets manager's "Rule sets"
/// sub-list: `{name, url, bytes, downloaded_at, used_by presets}`.
pub fn rule_set_summary(row: &store::RuleSetRow, used_by: &[String]) -> String {
    let mut parts = vec![
        crate::import::redact_link(&row.url),
        format_bytes(row.bytes),
        format_last_refresh(Some(row.downloaded_at), now_unix()),
    ];
    if !used_by.is_empty() {
        parts.push(format!("used by {}", used_by.join(", ")));
    }
    parts.join(" · ")
}
pub fn traffic_other_line_text(count: usize, rx: u64, tx: u64) -> String {
    format!(
        "Other ({count} more · ↓{} ↑{})",
        format_bytes(rx),
        format_bytes(tx)
    )
}
pub fn format_uptime(total_secs: u64) -> String {
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;
    let secs = total_secs % 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}
pub fn traffic_stacked_bar(rx: u64, tx: u64, max_total: u64) -> String {
    const CELLS: usize = 12;
    let max = max_total.max(1);
    let rx_fill = ((rx as f64 / max as f64) * CELLS as f64).round() as usize;
    let tx_fill = ((tx as f64 / max as f64) * CELLS as f64).round() as usize;
    let rx_fill = rx_fill.min(CELLS);
    let tx_fill = tx_fill.min(CELLS.saturating_sub(rx_fill));
    format!(
        "{}{}{}",
        "█".repeat(rx_fill),
        "▓".repeat(tx_fill),
        "░".repeat(CELLS.saturating_sub(rx_fill + tx_fill))
    )
}
pub fn traffic_stats_summary_text(
    period: store::TrafficPeriod,
    exits: usize,
    rx: u64,
    tx: u64,
) -> String {
    format!(
        "{} (all-time totals) · {exits} exit(s) · ↓{} ↑{}",
        period.label(),
        format_bytes(rx),
        format_bytes(tx),
    )
}
