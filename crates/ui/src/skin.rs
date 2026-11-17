//! The signed lab skin, ported from `crates/lab/src/skin.rs` (ruling R2).
//! The lab's file is the source of truth for every token value below; this
//! module is the only place in the UI crate that may name a color literal.
//! The kit 0.9.1 colors the lab never named (chart ramps, state hovers, tinted
//! button variants) derive from those values with the kit's own fallback
//! formulas, so every kit component reads the signed palette.
//!
//! Port notes: `SkinKind` keeps the lab's two skins, but production ships the
//! user's signed choice (Native) — `SIGNED_SKIN` is the only kind
//! `apply_appearance` stamps. The lab's `overlay: hex(0x000000)` is an opaque
//! black the dialog scrim would paint verbatim, and the signed lab never
//! opened a dialog, so the scrim keeps the app's tuned values (F17) instead.
use gpui_kit::component::{Colorize as _, Theme, ThemeMode};
use gpui_kit::*;

/// The skin the user signed. `Signal` stays in the enum so the port matches
/// the lab file, but nothing selects it yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinKind {
    Native,
    #[allow(dead_code)]
    Signal,
}

pub const SIGNED_SKIN: SkinKind = SkinKind::Native;

/// QR plates stay literal black-on-white in both themes: scanners read that
/// pair, and a theme-tinted code can refuse to scan. Named here so no feature
/// code carries a color literal.
pub fn qr_plate() -> Hsla {
    rgb(0xFFFFFF).into()
}
pub fn qr_module() -> Hsla {
    rgb(0x000000).into()
}

fn hex(value: u32) -> Hsla {
    let color: Hsla = rgb(value).into();
    color
}

/// Every color the app paints. Nothing outside this module names a hex value.
#[derive(Clone, Debug)]
pub struct Skin {
    pub bg: Hsla,
    pub panel: Hsla,
    pub plate: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub rim: Hsla,
    pub border: Hsla,
    pub divider: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub dim: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_active: Hsla,
    pub on_accent: Hsla,
    pub accent_surface: Hsla,
    pub success: Hsla,
    pub success_text: Hsla,
    pub warning: Hsla,
    pub warning_text: Hsla,
    pub danger: Hsla,
    pub danger_text: Hsla,
    pub info: Hsla,
    pub focus: Hsla,
    pub scrollbar_thumb: Hsla,
}

impl Skin {
    pub fn new(kind: SkinKind, mode: ThemeMode) -> Self {
        match (kind, mode.is_dark()) {
            (SkinKind::Native, true) => Self {
                bg: hex(0x0E1014),
                panel: hex(0x14171C),
                plate: hex(0x181C22),
                hover: hex(0x1F242C),
                selected: hex(0x1B2A44),
                rim: hex(0x4A7FE0),
                border: hex(0x262B33),
                divider: hex(0x1C2026),
                text: hex(0xE9EBEF),
                muted: hex(0x9BA3AF),
                dim: hex(0x6B7380),
                accent: hex(0x5B8CFF),
                accent_hover: hex(0x6E9AFF),
                accent_active: hex(0x82AAFF),
                on_accent: hex(0x0B1020),
                accent_surface: hex(0x1F242C),
                success: hex(0x3FB950),
                success_text: hex(0x56D364),
                warning: hex(0xD29922),
                warning_text: hex(0xE3B341),
                danger: hex(0xF85149),
                danger_text: hex(0xFF7B72),
                info: hex(0x58A6FF),
                focus: hex(0x7AA2FF),
                scrollbar_thumb: hex(0x3A404A),
            },
            (SkinKind::Native, false) => Self {
                bg: hex(0xF4F6F9),
                panel: hex(0xFFFFFF),
                plate: hex(0xFFFFFF),
                hover: hex(0xEEF1F5),
                selected: hex(0xE7EFFD),
                rim: hex(0x2F63CC),
                border: hex(0xDCE1E8),
                divider: hex(0xE8ECF1),
                text: hex(0x171A1F),
                muted: hex(0x5A6472),
                dim: hex(0x7A8492),
                accent: hex(0x2F63CC),
                accent_hover: hex(0x2857B8),
                accent_active: hex(0x234CA3),
                on_accent: hex(0xFFFFFF),
                accent_surface: hex(0xEEF1F5),
                success: hex(0x1A7F37),
                success_text: hex(0x1A7F37),
                warning: hex(0x9A6700),
                warning_text: hex(0x9A6700),
                danger: hex(0xCF222E),
                danger_text: hex(0xCF222E),
                info: hex(0x0969DA),
                focus: hex(0x2F63CC),
                scrollbar_thumb: hex(0xC5CCD6),
            },
            (SkinKind::Signal, true) => Self {
                bg: hex(0x08090A),
                panel: hex(0x0E1012),
                plate: hex(0x121416),
                hover: hex(0x181B1E),
                selected: hex(0x15202D),
                rim: hex(0x4A90E2),
                border: hex(0x24282C),
                divider: hex(0x1A1D20),
                text: hex(0xE8E8E6),
                muted: hex(0x9A9C98),
                dim: hex(0x6C6F6B),
                accent: hex(0xE8E8E6),
                accent_hover: hex(0xF5F5F2),
                accent_active: hex(0xFFFFFF),
                on_accent: hex(0x08090A),
                accent_surface: hex(0x1A1D20),
                success: hex(0x3DDC97),
                success_text: hex(0x3DDC97),
                warning: hex(0xF5B942),
                warning_text: hex(0xF5B942),
                danger: hex(0xFF5A5F),
                danger_text: hex(0xFF7B80),
                info: hex(0x4A90E2),
                focus: hex(0x9CC7FF),
                scrollbar_thumb: hex(0x2E3236),
            },
            (SkinKind::Signal, false) => Self {
                bg: hex(0xF2F2EE),
                panel: hex(0xFAFAF7),
                plate: hex(0xFFFFFF),
                hover: hex(0xECECE7),
                selected: hex(0xE4EEFA),
                rim: hex(0x2F6FBF),
                border: hex(0xD9D9D2),
                divider: hex(0xE5E5DE),
                text: hex(0x17181A),
                muted: hex(0x5F625D),
                dim: hex(0x8A8D86),
                accent: hex(0x17181A),
                accent_hover: hex(0x2A2C2E),
                accent_active: hex(0x3A3D40),
                on_accent: hex(0xF2F2EE),
                accent_surface: hex(0xE8E8E3),
                success: hex(0x0E7A4A),
                success_text: hex(0x0E7A4A),
                warning: hex(0x9A6200),
                warning_text: hex(0x9A6200),
                danger: hex(0xB3261E),
                danger_text: hex(0xB3261E),
                info: hex(0x2F6FBF),
                focus: hex(0x1D5BD6),
                scrollbar_thumb: hex(0xC9C9C2),
            },
        }
    }
}

/// The kit's fallback formulas for one state family, seeded from a signed hue.
/// 0.9.1 grew hover/active tones and tinted button variants the lab skin never
/// named; deriving them here keeps a kit component's states in the family of
/// its base color instead of falling back to the kit's stock blue palette.
struct Tone {
    hover: Hsla,
    active: Hsla,
    button: Hsla,
    button_hover: Hsla,
    button_active: Hsla,
}

impl Tone {
    fn new(background: Hsla, hue: Hsla, mode: ThemeMode) -> Self {
        let transparent = transparent_black();
        let active_darken = if mode.is_dark() { 0.2 } else { 0.1 };
        Self {
            hover: background.blend(hue.opacity(0.9)),
            active: hue.darken(active_darken),
            button: hue.mix_oklab(transparent, 0.2),
            button_hover: hue.mix_oklab(transparent, 0.3),
            button_active: hue.mix_oklab(transparent, 0.4),
        }
    }
}

/// Stamps the whole kit theme after `Theme::change` rebuilt it from config.
/// Field-for-field the lab's `apply_theme`, extended with the colors 0.9.1
/// added since the skin was signed. The `overlay` token and the type scale
/// stay with the app (`apply_appearance`), which owns the F17 scrim and the
/// K12 reading scale the lab never carried.
pub fn apply_theme(kind: SkinKind, mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    Theme::change(mode, window, cx);
    let skin = Skin::new(kind, mode);
    // `update` reconciles every token with the colors edited here, rebuilds the
    // Base projection (scrollbars, resize handles, TextView defaults) and
    // refreshes the windows; a `global_mut` edit would leave all three behind.
    Theme::update(cx, |theme| stamp(theme, kind, mode, &skin));
}

fn stamp(theme: &mut Theme, kind: SkinKind, mode: ThemeMode, skin: &Skin) {
    theme.radius = px(kind.radius());
    theme.radius_lg = px(kind.radius_lg());

    let danger = Tone::new(skin.bg, skin.danger, mode);
    let success = Tone::new(skin.bg, skin.success, mode);
    let warning = Tone::new(skin.bg, skin.warning, mode);
    let info = Tone::new(skin.bg, skin.info, mode);
    // Signal-box stripes stay whisper-quiet: a barely-stepped alternate of the
    // paper, never an alternating band.
    let stripe = skin.bg.blend(skin.border.opacity(0.25));
    let ramp = skin.info;
    let ramp_light = |base: Hsla| skin.bg.blend(base.opacity(0.8));

    theme.background = skin.bg;
    theme.table = skin.bg;
    // `theme.list` is the list settings; the color lives on `colors`.
    theme.colors.list = skin.bg;
    theme.accordion = skin.bg;
    theme.sidebar = skin.panel;
    theme.title_bar = skin.panel;
    theme.status_bar = skin.panel;
    theme.group_box = skin.plate;
    theme.popover = skin.plate;
    theme.secondary = skin.panel;
    theme.input = skin.plate;
    theme.list_head = skin.panel;
    theme.table_head = skin.panel;
    theme.table_foot = skin.panel;
    theme.tab = skin.panel;
    theme.tab_bar = skin.panel;
    theme.tab_bar_segmented = skin.hover;
    theme.scrollbar = transparent_black();
    theme.skeleton = skin.hover;
    theme.switch = skin.hover;
    theme.switch_thumb = skin.panel;

    theme.list_hover = skin.hover;
    theme.table_hover = skin.hover;
    theme.button_hover = skin.hover;
    theme.secondary_hover = skin.hover;
    theme.accent = skin.hover;
    theme.accent_foreground = skin.text;

    theme.list_active = skin.selected;
    theme.table_active = skin.selected;
    theme.list_active_border = skin.rim;
    theme.table_active_border = skin.rim;
    theme.selection = skin.accent;
    theme.tab_active = skin.selected;
    theme.tab_active_foreground = skin.text;

    theme.list_even = stripe;
    theme.table_even = stripe;

    theme.border = skin.border;
    theme.sidebar_border = skin.border;
    theme.title_bar_border = skin.border;
    theme.status_bar_border = skin.border;
    theme.table_row_border = skin.divider;
    theme.window_border = skin.border;
    theme.drag_border = skin.accent.opacity(0.65);
    theme.drop_target = skin.accent.opacity(0.2);

    theme.foreground = skin.text;
    theme.popover_foreground = skin.text;
    theme.sidebar_foreground = skin.text;
    theme.secondary_foreground = skin.text;
    theme.button_foreground = skin.text;
    theme.group_box_foreground = skin.text;
    theme.table_head_foreground = skin.muted;
    theme.table_foot_foreground = skin.muted;
    theme.muted_foreground = skin.muted;
    theme.muted = skin.muted;
    theme.tab_foreground = skin.muted;
    theme.description_list_label = skin.muted;
    theme.description_list_label_foreground = skin.muted;
    // Links keep the signal blue even when the accent is monochrome (Signal),
    // and their states step toward the ink so both grounds gain contrast.
    theme.link = skin.rim;
    theme.link_hover = skin.rim.blend(skin.text.opacity(0.25));
    theme.link_active = skin.rim.blend(skin.text.opacity(0.4));

    theme.primary = skin.accent;
    theme.primary_hover = skin.accent_hover;
    theme.primary_active = skin.accent_active;
    theme.primary_foreground = skin.on_accent;
    theme.button_primary = skin.accent;
    theme.button_primary_hover = skin.accent_hover;
    theme.button_primary_active = skin.accent_active;
    theme.button_primary_foreground = skin.on_accent;
    theme.button_secondary = skin.panel;
    theme.button_secondary_hover = skin.hover;
    theme.button_secondary_active = skin.selected;
    theme.button_secondary_foreground = skin.text;
    theme.secondary_active = skin.selected;
    theme.button = skin.plate;
    theme.button_active = skin.selected;
    theme.sidebar_primary = skin.accent;
    theme.sidebar_primary_foreground = skin.on_accent;
    theme.sidebar_accent = skin.selected;
    theme.sidebar_accent_foreground = skin.text;
    theme.progress_bar = skin.accent;
    theme.slider_bar = skin.accent;
    theme.slider_thumb = skin.on_accent;

    theme.ring = skin.focus;
    theme.caret = skin.focus;
    theme.success = skin.success;
    theme.success_foreground = skin.success_text;
    theme.success_hover = success.hover;
    theme.success_active = success.active;
    theme.button_success = success.button;
    theme.button_success_hover = success.button_hover;
    theme.button_success_active = success.button_active;
    theme.button_success_foreground = skin.success;
    theme.warning = skin.warning;
    theme.warning_foreground = skin.warning_text;
    theme.warning_hover = warning.hover;
    theme.warning_active = warning.active;
    theme.button_warning = warning.button;
    theme.button_warning_hover = warning.button_hover;
    theme.button_warning_active = warning.button_active;
    theme.button_warning_foreground = skin.warning;
    theme.danger = skin.danger;
    theme.danger_foreground = skin.danger_text;
    theme.danger_hover = danger.hover;
    theme.danger_active = danger.active;
    theme.button_danger = danger.button;
    theme.button_danger_hover = danger.button_hover;
    theme.button_danger_active = danger.button_active;
    theme.button_danger_foreground = skin.danger;
    theme.info = skin.info;
    theme.info_foreground = skin.on_accent;
    theme.info_hover = info.hover;
    theme.info_active = info.active;
    theme.button_info = info.button;
    theme.button_info_hover = info.button_hover;
    theme.button_info_active = info.button_active;
    theme.button_info_foreground = skin.info;

    theme.chart_1 = ramp.lighten(0.4);
    theme.chart_2 = ramp.lighten(0.2);
    theme.chart_3 = ramp;
    theme.chart_4 = ramp.darken(0.2);
    theme.chart_5 = ramp.darken(0.4);
    theme.chart_bullish = skin.success;
    theme.chart_bearish = skin.danger;
    theme.chart_grid = skin.border.opacity(0.6);

    theme.red = skin.danger;
    theme.red_light = ramp_light(skin.danger);
    theme.green = skin.success;
    theme.green_light = ramp_light(skin.success);
    theme.blue = skin.info;
    theme.blue_light = ramp_light(skin.info);
    theme.yellow = skin.warning;
    theme.yellow_light = ramp_light(skin.warning);
    // Magenta and cyan are chart complements only; they keep the info tone and
    // drop some saturation so the ramp reads as one family.
    let magenta = skin.info.hue(0.88).saturation(0.62);
    let cyan = skin.info.hue(0.52).saturation(0.55);
    theme.magenta = magenta;
    theme.magenta_light = ramp_light(magenta);
    theme.cyan = cyan;
    theme.cyan_light = ramp_light(cyan);

    theme.scrollbar_thumb = skin.scrollbar_thumb;
    theme.scrollbar_thumb_hover = skin.muted;
}

impl SkinKind {
    pub fn radius(self) -> f32 {
        match self {
            Self::Native => 6.,
            Self::Signal => 3.,
        }
    }

    pub fn radius_lg(self) -> f32 {
        match self {
            Self::Native => 8.,
            Self::Signal => 4.,
        }
    }
}
