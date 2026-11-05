//! UI copy helpers. The core crate cannot reach the Fluent bundle, so any
//! shared string that names UI state (title, counts, columns) lives here; the
//! rest of the pure builders stay in `manyproxy_core::text`.

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
