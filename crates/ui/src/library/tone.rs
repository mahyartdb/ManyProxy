use crate::*;

/// T-31: the kit's signal tokens (green, amber, red) are *fills* paired with a
/// light foreground — a button ground, not text. On the light theme's paper
/// every one of them is far under WCAG AA as text, so in light mode each backs
/// off toward black along its own hue until it reads on the quiet surface
/// (#f5f5f5, the most-tinted ground a label sits on in light mode). Dark keeps
/// the raw tokens, which already read on near-black. Hue still identifies the
/// signal; only lightness moves.
pub fn signal_text_color(signal: Hsla, darken: f32, theme: &Theme) -> Hsla {
    match theme.mode {
        ThemeMode::Light => signal.blend(hsla(0., 0., 0., darken)),
        ThemeMode::Dark => signal,
    }
}

pub fn warning_text_color(theme: &Theme) -> Hsla {
    signal_text_color(theme.warning, 0.45, theme)
}

pub fn danger_text_color(theme: &Theme) -> Hsla {
    signal_text_color(theme.danger, 0.35, theme)
}

pub fn success_text_color(theme: &Theme) -> Hsla {
    signal_text_color(theme.success, 0.40, theme)
}

/// One step *away* from the ground: darker on light paper, brighter on
/// near-black. Used where a state must outrank another in the same hue, so the
/// step reads as emphasis in both palettes instead of as a dimmer red.
pub fn emphasized_text_color(color: Hsla, theme: &Theme) -> Hsla {
    let toward = match theme.mode {
        ThemeMode::Light => hsla(0., 0., 0., 0.35),
        ThemeMode::Dark => hsla(0., 0., 1., 0.35),
    };
    color.blend(toward)
}

/// T-31: the tone each level paints in, from the active theme. `Info` keeps the
/// default text color, `Debug` recedes to muted.
pub fn log_level_text_color(level: LogLevel, theme: &Theme) -> Hsla {
    match level {
        LogLevel::Error => danger_text_color(theme),
        LogLevel::Warn => warning_text_color(theme),
        LogLevel::Info => theme.foreground,
        LogLevel::Debug => theme.muted_foreground,
    }
}

/// T-31: a visible line already carries its parsed level as `[level]`.
pub fn log_line_level(line: &str) -> LogLevel {
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(label, _)| LogLevel::from_label(label))
        .unwrap_or(LogLevel::Info)
}

/// T-31: the Settings "Maximum kept log lines" text to a clamped cap. `None`
/// means the text is not a whole number.
pub fn parse_log_cap(raw: &str) -> Option<usize> {
    let digits = raw.trim().replace(',', "");
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok().map(clamp_log_max_lines)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingTone {
    Failed,
    Serving,
    Stopped,
}

/// Theme color for a probe's LatencyTone: Throne §F.1 bands map green /
/// yellow / red with the 100/300 boundaries, a failed probe reads one step
/// stronger than a plain poor latency, and an untested/no-sample row keeps the
/// neutral muted tone. Colors only — no probe, classification, or cutoff
/// change rides along.
pub fn health_tone_color(tone: LatencyTone, theme: &Theme) -> Hsla {
    match tone {
        LatencyTone::Fast => success_text_color(theme),
        LatencyTone::Slow => warning_text_color(theme),
        LatencyTone::Poor => danger_text_color(theme),
        // Failed keeps its old relation to poor — one visible step stronger —
        // but the step follows the ground instead of always darkening, which
        // on near-black turned the alarm red into 3.4:1 mud.
        LatencyTone::Failed => emphasized_text_color(danger_text_color(theme), theme),
        LatencyTone::Unknown => theme.muted_foreground,
    }
}

/// One verdict, one color: the row's occupancy lamp, the inspector badge dot,
/// and the badge text all read this. A stopped badge used the kit's
/// `secondary` fill, which is a *surface* — on light paper it painted #e5e5e5
/// on #ffffff, so a stopped row had no badge at all.
pub fn serving_tone_color(tone: ServingTone, theme: &Theme) -> Hsla {
    match tone {
        ServingTone::Serving => success_text_color(theme),
        ServingTone::Failed => danger_text_color(theme),
        ServingTone::Stopped => theme.muted_foreground,
    }
}

impl From<BadgeState> for ServingTone {
    fn from(state: BadgeState) -> Self {
        match state {
            BadgeState::Running => Self::Serving,
            BadgeState::Failed => Self::Failed,
            BadgeState::Stopped => Self::Stopped,
        }
    }
}

/// The lead's state glyph: one 14px mark that says what the row is doing.
/// It is status, not a control — it carries no button role, no hit target
/// and no hover state.
pub fn state_glyph(state: RowPhase, theme: &Theme) -> AnyElement {
    let glyph: AnyElement = match state {
        RowPhase::Running => div()
            .size(px(8.))
            .rounded_full()
            .bg(success_text_color(theme))
            .into_any_element(),
        RowPhase::Stopped => div()
            .size(px(8.))
            .rounded_full()
            .border_1()
            .border_color(theme.muted_foreground)
            .into_any_element(),
        RowPhase::Starting => div()
            .size(px(8.))
            .rounded_full()
            .border_1()
            .border_color(theme.primary)
            .child(div().w(px(4.)).h(px(8.)).bg(theme.primary))
            .into_any_element(),
        RowPhase::Failed => div()
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(danger_text_color(theme))
            .child("!")
            .into_any_element(),
    };
    div()
        .size(px(RowMetrics::GLYPH))
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
        .into_any_element()
}

#[cfg(any(test, feature = "test-support"))]
pub fn log_level_matches(level: LogLevel, filter: Option<LogLevel>) -> bool {
    match filter {
        None => true,
        Some(wanted) => level == wanted,
    }
}
