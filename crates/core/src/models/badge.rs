//! K1/A4-3: the inspector's serving chip — one truthful state word, one lamp,
//! no glyph dots.
//!
//! `text::serving_badge` feeds the library row (and pins its own tests); this
//! module is the inspector header's badge string. The word follows the row's
//! ENDPOINT truth: `Failed` only while the endpoint itself is the failure
//! source (serving with a dead verdict, or a crashed shard). A stopped row is
//! `Stopped` even when its last probe died — there is nothing serving that
//! could be dead.

/// The badge's state word, decided from engine + health evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeState {
    Stopped,
    Running,
    Failed,
}

/// `<state> · <bind:port> · <latency>` (DESIGN "Serving badge string"). The
/// latency segment appears only once a probe has run AND the endpoint still
/// serves; a failing or stopped endpoint never wears a stale latency. No
/// `●`/`○` prefix — the lamp div carries the color, so there is no second dot
/// to clip.
pub fn inspector_badge_text(
    state: BadgeState,
    bind: &str,
    port: u16,
    latency_ms: Option<u64>,
) -> String {
    let bind = bind.trim();
    let endpoint = if bind.is_empty() {
        format!(":{port}")
    } else {
        format!("{bind}:{port}")
    };
    let head = match state {
        BadgeState::Stopped => format!("Stopped · {endpoint}"),
        BadgeState::Running => format!("Running · {endpoint}"),
        BadgeState::Failed => format!("Failed · {endpoint}"),
    };
    match (state, latency_ms) {
        (BadgeState::Running, Some(ms)) => format!("{head} · {ms}ms"),
        _ => head,
    }
}
