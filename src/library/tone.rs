use crate::*;

/// T-31: the tone each level paints in, from the active theme. `Info` keeps the
/// default text color, `Debug` recedes to muted. `Warn`/`Error` are theme
/// *background* tokens (amber, danger red): legible behind white text on a dark
/// surface, but well under WCAG AA as text on the light theme's white — so in
/// light mode both are backed off toward black along their own hue (the T-30
/// `danger.blend(...)` precedent). Dark mode keeps the raw tokens.
pub(crate) fn log_level_text_color(level: LogLevel, theme: &Theme) -> Hsla {
    let text_safe = |color: Hsla, darken: f32| match theme.mode {
        ThemeMode::Light => color.blend(hsla(0., 0., 0., darken)),
        ThemeMode::Dark => color,
    };
    match level {
        LogLevel::Error => text_safe(theme.danger, 0.35),
        LogLevel::Warn => text_safe(theme.warning, 0.45),
        LogLevel::Info => theme.foreground,
        LogLevel::Debug => theme.muted_foreground,
    }
}

/// T-31: a visible line already carries its parsed level as `[level]`.
pub(crate) fn log_line_level(line: &str) -> LogLevel {
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(label, _)| LogLevel::from_label(label))
        .unwrap_or(LogLevel::Info)
}

/// T-31: the Settings "Maximum kept log lines" text to a clamped cap. `None`
/// means the text is not a whole number.
pub(crate) fn parse_log_cap(raw: &str) -> Option<usize> {
    let digits = raw.trim().replace(',', "");
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok().map(clamp_log_max_lines)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServingTone {
    Failed,
    Serving,
    Stopped,
}

/// Theme color for a probe's LatencyTone: Throne §F.1 bands map green /
/// yellow / red with the 100/300 boundaries, a failed probe reads a darker
/// red than a plain poor latency, and an untested/no-sample row keeps the
/// neutral muted tone. Colors only — no probe, classification, or cutoff
/// change rides along.
pub(crate) fn health_tone_color(tone: LatencyTone, cx: &App) -> Hsla {
    match tone {
        LatencyTone::Fast => cx.theme().success,
        LatencyTone::Slow => cx.theme().warning,
        LatencyTone::Poor => cx.theme().danger,
        LatencyTone::Failed => cx.theme().danger.blend(hsla(0., 0., 0., 0.35)),
        LatencyTone::Unknown => cx.theme().muted_foreground,
    }
}

#[cfg(test)]
pub(crate) fn log_level_matches(level: LogLevel, filter: Option<LogLevel>) -> bool {
    match filter {
        None => true,
        Some(wanted) => level == wanted,
    }
}
