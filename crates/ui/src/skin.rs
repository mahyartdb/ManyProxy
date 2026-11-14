//! The signed lab skin, ported from `crates/lab/src/skin.rs` (ruling R2).
//! The lab's file is the source of truth for every token value below; this
//! module is the only place in the UI crate that may name a color literal.
//!
//! Port notes: `SkinKind` keeps the lab's two skins, but production ships the
//! user's signed choice (Native) — `SIGNED_SKIN` is the only kind
//! `apply_appearance` stamps. The lab's `overlay: hex(0x000000)` is an opaque
//! black the dialog scrim would paint verbatim, and the signed lab never
//! opened a dialog, so the scrim keeps the app's tuned values (F17) instead.
use gpui_kit::component::{Theme, ThemeMode};
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

/// Stamps the whole kit theme after `Theme::change` rebuilt it from config.
/// Field-for-field the lab's `apply_theme`; the `overlay` token and the type
/// scale stay with the app (`apply_appearance`), which owns the F17 scrim and
/// the K12 reading scale the lab never carried.
pub fn apply_theme(kind: SkinKind, mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    Theme::change(mode, window, cx);
    let skin = Skin::new(kind, mode);
    let theme = Theme::global_mut(cx);
    theme.radius = px(kind.radius());
    theme.radius_lg = px(kind.radius_lg());

    theme.background = skin.bg;
    theme.table = skin.bg;
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

    theme.border = skin.border;
    theme.sidebar_border = skin.border;
    theme.title_bar_border = skin.border;
    theme.status_bar_border = skin.border;
    theme.table_row_border = skin.divider;

    theme.foreground = skin.text;
    theme.popover_foreground = skin.text;
    theme.sidebar_foreground = skin.text;
    theme.secondary_foreground = skin.text;
    theme.button_foreground = skin.text;
    theme.table_head_foreground = skin.muted;
    theme.muted_foreground = skin.muted;
    theme.muted = skin.muted;
    theme.description_list_label_foreground = skin.muted;
    theme.description_list_label = skin.muted;

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
    theme.button = skin.plate;
    theme.button_active = skin.selected;

    theme.ring = skin.focus;
    theme.caret = skin.focus;
    theme.success = skin.success;
    theme.success_foreground = skin.success_text;
    theme.warning = skin.warning;
    theme.warning_foreground = skin.warning_text;
    theme.danger = skin.danger;
    theme.danger_foreground = skin.danger_text;
    theme.info = skin.info;
    theme.tab_active = skin.selected;
    theme.tab_active_foreground = skin.text;
    theme.tab_bar = skin.panel;
    theme.tab_foreground = skin.muted;
    theme.scrollbar_thumb = skin.scrollbar_thumb;
    theme.scrollbar_thumb_hover = skin.muted;
    theme.window_border = skin.border;

    let tokens = &mut theme.tokens;
    tokens.primary.background = skin.accent.into();
    tokens.primary.color = skin.accent;
    tokens.primary_hover.background = skin.accent_hover.into();
    tokens.primary_hover.color = skin.accent_hover;
    tokens.primary_active.background = skin.accent_active.into();
    tokens.primary_active.color = skin.accent_active;
    tokens.primary_foreground.background = skin.on_accent.into();
    tokens.primary_foreground.color = skin.on_accent;
    tokens.button_primary.background = skin.accent.into();
    tokens.button_primary.color = skin.accent;
    tokens.button_primary_hover.background = skin.accent_hover.into();
    tokens.button_primary_hover.color = skin.accent_hover;
    tokens.button_primary_active.background = skin.accent_active.into();
    tokens.button_primary_active.color = skin.accent_active;
    tokens.button_primary_foreground.background = skin.on_accent.into();
    tokens.button_primary_foreground.color = skin.on_accent;
    tokens.ring.background = skin.focus.into();
    tokens.ring.color = skin.focus;
    tokens.selection.background = skin.accent.into();
    tokens.selection.color = skin.accent;
    tokens.list_active.background = skin.selected.into();
    tokens.list_active.color = skin.selected;
    tokens.list_active_border.background = skin.rim.into();
    tokens.list_active_border.color = skin.rim;
    tokens.table_active.background = skin.selected.into();
    tokens.table_active.color = skin.selected;
    tokens.table_active_border.background = skin.rim.into();
    tokens.table_active_border.color = skin.rim;
    tokens.accent.background = skin.hover.into();
    tokens.accent.color = skin.hover;

    cx.refresh_windows();
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
