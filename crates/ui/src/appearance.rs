use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

use crate::skin::{apply_theme, SIGNED_SKIN};
use crate::store::{Accent, TextSize};

pub const APPEARANCE_CHOICES: &[(&str, &str)] =
    &[("dark", "Dark"), ("light", "Light"), ("system", "System")];
pub fn windows_apps_light_theme() -> Option<bool> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
            .ok()?
            .get_value::<u32, _>("AppsUseLightTheme")
            .ok()
            .map(|value| value != 0)
    }
    #[cfg(not(windows))]
    {
        None
    }
}
pub fn appearance_theme_mode(appearance: &str, system_light: bool) -> ThemeMode {
    match appearance {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        "system" if system_light => ThemeMode::Light,
        "system" => ThemeMode::Dark,
        _ => ThemeMode::Dark,
    }
}
pub fn resolve_appearance_mode(appearance: &str) -> ThemeMode {
    appearance_theme_mode(appearance, windows_apps_light_theme().unwrap_or(false))
}
/// F17: the kit's light scrim is `#0000000d` (~5%), which a modal over a
/// near-white page barely reads as a backdrop. Dark keeps the kit's 20%. The
/// dialog backdrop paints this exact token (`Theme::overlay`).
pub const LIGHT_MODAL_OVERLAY_ALPHA: f32 = 0.15;

/// K12: the kit's stock sizes (`Theme::default`). `Theme::change` rebuilds the
/// theme from its config, so every appearance swap resets these before the
/// reading scale is applied again.
pub const BASE_FONT_SIZE: f32 = 16.;
pub const BASE_MONO_FONT_SIZE: f32 = 13.;

/// K12: scale the rem the whole window resolves against. Text roles keep
/// their two steps; only the base moves. `Root` re-reads `theme.font_size` for
/// `window.set_rem_size` every frame, so refreshing windows is enough.
pub fn apply_text_size(size: TextSize, cx: &mut App) {
    let scale = size.scale();
    let theme = Theme::global_mut(cx);
    theme.font_size = px(BASE_FONT_SIZE * scale);
    theme.mono_font_size = px(BASE_MONO_FONT_SIZE * scale);
    cx.refresh_windows();
}

/// Task 11: the accent-bearing tokens a preset stamps over the kit theme. Blue
/// has no entry: it is the kit's stock selection and re-stamping it would make
/// the defaults differ from the palette the suite already pins.
pub struct AccentPalette {
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub primary_active: Hsla,
    pub primary_foreground: Hsla,
    pub ring: Hsla,
    pub selection: Hsla,
    pub wash: Hsla,
    pub rim: Hsla,
    pub solid: Hsla,
}

pub fn accent_hue(accent: Accent) -> Option<f32> {
    match accent {
        Accent::Blue => None,
        Accent::Indigo => Some(0.66),
        Accent::Violet => Some(0.75),
        Accent::Magenta => Some(0.86),
        Accent::Rose => Some(0.95),
        Accent::Teal => Some(0.47),
        Accent::Amber => Some(0.10),
    }
}

pub fn accent_palette(accent: Accent, mode: ThemeMode) -> Option<AccentPalette> {
    let hue = accent_hue(accent)?;
    Some(if mode.is_dark() {
        AccentPalette {
            primary: hsla(hue, 0.62, 0.62, 1.),
            primary_hover: hsla(hue, 0.62, 0.68, 1.),
            primary_active: hsla(hue, 0.62, 0.73, 1.),
            primary_foreground: hsla(0., 0., 0.10, 1.),
            ring: hsla(hue, 0.65, 0.60, 1.),
            selection: hsla(hue, 0.65, 0.45, 1.),
            wash: hsla(hue, 0.70, 0.35, 0.22),
            rim: hsla(hue, 0.70, 0.55, 1.),
            solid: hsla(hue, 0.62, 0.62, 1.),
        }
    } else {
        AccentPalette {
            primary: hsla(hue, 0.72, 0.48, 1.),
            primary_hover: hsla(hue, 0.72, 0.42, 1.),
            primary_active: hsla(hue, 0.72, 0.37, 1.),
            primary_foreground: hsla(0., 0., 1., 1.),
            ring: hsla(hue, 0.72, 0.52, 1.),
            selection: hsla(hue, 0.75, 0.60, 1.),
            wash: hsla(hue, 0.80, 0.85, 0.22),
            rim: hsla(hue, 0.75, 0.55, 1.),
            solid: hsla(hue, 0.72, 0.48, 1.),
        }
    })
}

/// Task 11: the color a Settings swatch paints. Blue borrows the kit's stock
/// selection rim so the default swatch matches what the window actually shows.
pub fn accent_swatch(accent: Accent, mode: ThemeMode) -> Hsla {
    match accent_palette(accent, mode) {
        Some(palette) => palette.solid,
        None => {
            if mode.is_dark() {
                hsla(0.61, 0.72, 0.48, 1.)
            } else {
                hsla(0.60, 0.83, 0.60, 1.)
            }
        }
    }
}

/// Task 11: re-stamp every accent-bearing token after `Theme::change` rebuilt
/// the theme from its config. Health, danger, and warning tokens are never
/// touched: those hues carry meaning, not preference.
pub fn apply_accent(accent: Accent, mode: ThemeMode, cx: &mut App) {
    let Some(palette) = accent_palette(accent, mode) else {
        return;
    };
    let theme = Theme::global_mut(cx);
    theme.primary = palette.primary;
    theme.primary_hover = palette.primary_hover;
    theme.primary_active = palette.primary_active;
    theme.primary_foreground = palette.primary_foreground;
    theme.ring = palette.ring;
    theme.selection = palette.selection;
    theme.list_active = palette.wash;
    theme.list_active_border = palette.rim;
    theme.table_active = palette.wash;
    theme.table_active_border = palette.rim;
    theme.accent = palette.rim;

    let tokens = &mut theme.tokens;
    tokens.primary.background = palette.primary.into();
    tokens.primary.color = palette.primary;
    tokens.primary_hover.background = palette.primary_hover.into();
    tokens.primary_hover.color = palette.primary_hover;
    tokens.primary_active.background = palette.primary_active.into();
    tokens.primary_active.color = palette.primary_active;
    tokens.primary_foreground.background = palette.primary_foreground.into();
    tokens.primary_foreground.color = palette.primary_foreground;
    tokens.button_primary.background = palette.primary.into();
    tokens.button_primary.color = palette.primary;
    tokens.button_primary_hover.background = palette.primary_hover.into();
    tokens.button_primary_hover.color = palette.primary_hover;
    tokens.button_primary_active.background = palette.primary_active.into();
    tokens.button_primary_active.color = palette.primary_active;
    tokens.button_primary_foreground.background = palette.primary_foreground.into();
    tokens.button_primary_foreground.color = palette.primary_foreground;
    tokens.ring.background = palette.ring.into();
    tokens.ring.color = palette.ring;
    tokens.selection.background = palette.selection.into();
    tokens.selection.color = palette.selection;
    tokens.list_active.background = palette.wash.into();
    tokens.list_active.color = palette.wash;
    tokens.list_active_border.background = palette.rim.into();
    tokens.list_active_border.color = palette.rim;
    tokens.table_active.background = palette.wash.into();
    tokens.table_active.color = palette.wash;
    tokens.table_active_border.background = palette.rim.into();
    tokens.table_active_border.color = palette.rim;
    tokens.accent.background = palette.rim.into();
    tokens.accent.color = palette.rim;
    cx.refresh_windows();
}

pub fn apply_appearance(
    appearance: &str,
    accent: Accent,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let mode = resolve_appearance_mode(appearance);
    // R2: the signed lab skin owns every color token. `apply_theme` runs
    // `Theme::change` itself, then stamps the skin; the overrides below ride
    // on top of it.
    apply_theme(SIGNED_SKIN, mode, window, cx);
    if !mode.is_dark() {
        let theme = Theme::global_mut(cx);
        // F17 keeps the app's tuned scrim (the lab's `hex(0x000000)` overlay
        // is opaque black; see skin.rs). The muted-ink darken tuned the KIT's
        // palette; the skin's light muted ink already clears AA, so it goes.
        theme.overlay = hsla(0., 0., 0., LIGHT_MODAL_OVERLAY_ALPHA);
    }
    // Task 11: the rebuild above reset every stamped accent token, so the
    // accent rides every re-appearance exactly like the light-mode overrides.
    apply_accent(accent, mode, cx);
    // Theme::change already refreshed the window; the overrides landed after.
    cx.refresh_windows();
    // F5: the kit has no per-scrollbar mode switch, only the theme's. Always
    // keeps the dialog body's scrollbar visible while it overflows instead of
    // fading out after two seconds.
    Theme::set_scrollbar_mode(ScrollbarMode::Always, cx);
    // K5: `Theme::change` rebuilds the notification settings with the theme,
    // so the toast park rides along on every re-appearance.
    crate::configure_notification_toasts(cx);
}

#[cfg(test)]
mod tokens {
    use crate::store::Accent;
    use gpui_kit::component::{Theme, ThemeMode};

    /// Read the theme the app actually paints: `apply_appearance` owns the
    /// light-mode token overrides, so a bare `Theme::change` would test a
    /// palette no window ever shows.
    fn theme_for(mode: ThemeMode, cx: &mut gpui_kit::TestAppContext) -> Theme {
        theme_for_accent(mode, Accent::Blue, cx)
    }

    fn theme_for_accent(
        mode: ThemeMode,
        accent: Accent,
        cx: &mut gpui_kit::TestAppContext,
    ) -> Theme {
        cx.update(|cx| {
            crate::apply_appearance(
                if mode.is_dark() { "dark" } else { "light" },
                accent,
                None,
                cx,
            );
            Theme::global(cx).clone()
        })
    }

    /// The palette claim DESIGN.md makes: the monochrome ramps invert between
    /// modes while the signal hues stay put. A theme swap that flips a signal
    /// color would make green mean nothing.
    #[gpui_kit::test]
    fn ramps_invert_and_signal_hues_hold(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let dark = theme_for(ThemeMode::Dark, cx);
        let light = theme_for(ThemeMode::Light, cx);

        for (name, dark_l, light_l) in [
            ("ink", dark.foreground.l, light.foreground.l),
            (
                "muted ink",
                dark.muted_foreground.l,
                light.muted_foreground.l,
            ),
        ] {
            assert!(
                dark_l > light_l,
                "{name} must be lighter on dark than on light ({dark_l} vs {light_l})"
            );
        }
        for (name, dark_l, light_l) in [
            ("paper", dark.background.l, light.background.l),
            ("quiet surface", dark.group_box.l, light.group_box.l),
            ("hairline", dark.border.l, light.border.l),
        ] {
            assert!(
                dark_l < light_l,
                "{name} must be darker on dark than on light ({dark_l} vs {light_l})"
            );
        }

        for (name, dark_color, light_color) in [
            ("serving green", dark.success, light.success),
            ("alarm red", dark.danger, light.danger),
            ("warning amber", dark.warning, light.warning),
        ] {
            // The lab's signal hues sit in the same family per mode pair
            // (GitHub-style greens/ambers/reds), but dark and light variants
            // are independently tuned, so the pin is the family, not the
            // value — measured around the hue circle, because red wraps.
            let hue_gap = (dark_color.h - light_color.h).abs();
            let hue_gap = hue_gap.min(1. - hue_gap);
            assert!(
                hue_gap < 0.06,
                "{name} keeps its hue family across modes ({} vs {})",
                dark_color.h,
                light_color.h
            );
            assert!(
                (dark_color.l - light_color.l).abs() > 0.05,
                "{name} retunes its lightness for the other ground"
            );
        }

        // Selection is the skin's blue list tokens, not `accent`: the skin's
        // accent surface is the quiet hover tone, not a signal color.
        for (name, wash, rim) in [
            ("dark", dark.list_active, dark.list_active_border),
            ("light", light.list_active, light.list_active_border),
        ] {
            assert!(
                (0.55..0.70).contains(&rim.h) && rim.s > 0.5,
                "{name} selection rim is blue, got {rim:?}"
            );
            assert!(wash != rim, "{name} selection is distinct from its rim");
        }
        for (name, theme) in [("dark", &dark), ("light", &light)] {
            assert_eq!(
                theme.accent, theme.list_hover,
                "{name} accent surface is the skin's hover tone: {:?}",
                theme.accent
            );
        }

        assert_eq!(dark.radius, light.radius, "one radius across modes");
    }

    /// Task 11, re-pointed at the ported skin (R2): blue is the signed skin
    /// itself, so applying it re-stamps the skin's own values rather than
    /// leaving the kit's stock palette underneath.
    #[gpui_kit::test]
    fn default_blue_re_stamps_the_signed_skin(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            Theme::change(ThemeMode::Dark, None, cx);
            let stock = Theme::global(cx).clone();
            crate::apply_appearance("dark", Accent::Blue, None, cx);
            let painted = Theme::global(cx).clone();
            for (name, stock, painted) in [
                ("page", stock.background, painted.background),
                ("selection rim", stock.list_active_border, painted.list_active_border),
                ("focus ring", stock.ring, painted.ring),
                ("primary fill", stock.primary, painted.primary),
            ] {
                assert_ne!(painted, stock, "blue re-stamps {name} off the kit default");
            }
        });
    }

    /// Task 11: a preset stamps the tokens the brief names — selection wash,
    /// checked/primary fills, and the focus ring — and the stamps survive the
    /// `Theme::change` a later appearance swap performs.
    #[gpui_kit::test]
    fn accent_stamps_its_surfaces_and_survives_a_theme_swap(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let stock_dark = theme_for(ThemeMode::Dark, cx);
        let dark = theme_for_accent(ThemeMode::Dark, Accent::Violet, cx);

        for (name, color) in [
            ("selection wash", dark.list_active),
            ("selection rim", dark.list_active_border),
            ("focus ring", dark.ring),
            ("checked fill", dark.primary),
            ("accent strip", dark.accent),
        ] {
            assert!(
                (color.h - 0.75).abs() < 0.03,
                "{name} takes the violet hue, got {color:?}"
            );
        }
        assert!(dark.list_active.a < 0.3, "selection stays a wash");
        assert_eq!(
            (dark.success, dark.danger, dark.warning),
            (stock_dark.success, stock_dark.danger, stock_dark.warning),
            "signal hues never follow the accent"
        );

        // A real appearance swap is `apply_appearance` again; the stamp has to
        // land after `Theme::change` rebuilt the config colors.
        let light = theme_for_accent(ThemeMode::Light, Accent::Violet, cx);
        assert!(
            (light.list_active.h - 0.75).abs() < 0.03
                && (light.list_active_border.h - 0.75).abs() < 0.03,
            "light mode re-stamps the same accent: {light:?}"
        );
        assert!(
            light.muted_foreground.l < 0.5,
            "the light ink override still composes with the accent"
        );
    }
}
