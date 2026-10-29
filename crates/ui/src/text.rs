//! UI-tinted copy helpers: toasts need the kit's `Notification`, lamps need
//! `ServingTone`, shard summaries need the shard rows. The pure string builders
//! live in `manyproxy_core::text`.

use crate::*;

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
pub(crate) fn bulk_delete_note(removed: &[String], failed: &[String]) -> Notification {
    let text = bulk_delete_result_message(removed, failed);
    if failed.is_empty() {
        Notification::success(text)
    } else if removed.is_empty() {
        Notification::error(text)
    } else {
        Notification::warning(text)
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
        ServingTone::Failed => format!("Failed — {name} (dead while serving)"),
        ServingTone::Serving => format!("Serving — {name}"),
        ServingTone::Stopped => format!("Stopped — {name}"),
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
