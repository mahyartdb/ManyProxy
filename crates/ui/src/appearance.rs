use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

pub(crate) const APPEARANCE_CHOICES: &[(&str, &str)] =
    &[("dark", "Dark"), ("light", "Light"), ("system", "System")];
pub(crate) fn windows_apps_light_theme() -> Option<bool> {
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
pub(crate) fn appearance_theme_mode(appearance: &str, system_light: bool) -> ThemeMode {
    match appearance {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        "system" if system_light => ThemeMode::Light,
        "system" => ThemeMode::Dark,
        _ => ThemeMode::Dark,
    }
}
pub(crate) fn resolve_appearance_mode(appearance: &str) -> ThemeMode {
    appearance_theme_mode(appearance, windows_apps_light_theme().unwrap_or(false))
}
/// F17: the kit's light scrim is `#0000000d` (~5%), which a modal over a
/// near-white page barely reads as a backdrop. Dark keeps the kit's 20%. The
/// dialog backdrop paints this exact token (`Theme::overlay`).
pub(crate) const LIGHT_MODAL_OVERLAY_ALPHA: f32 = 0.15;

pub(crate) fn apply_appearance(appearance: &str, window: Option<&mut Window>, cx: &mut App) {
    let mode = resolve_appearance_mode(appearance);
    Theme::change(mode, window, cx);
    if !mode.is_dark() {
        Theme::global_mut(cx).overlay = hsla(0., 0., 0., LIGHT_MODAL_OVERLAY_ALPHA);
        // Theme::change already refreshed the window; the override landed after.
        cx.refresh_windows();
    }
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
    use gpui_kit::component::{Theme, ThemeMode};

    fn theme_for(mode: ThemeMode, cx: &mut gpui_kit::TestAppContext) -> Theme {
        cx.update(|cx| {
            Theme::change(mode, None, cx);
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
            assert!(
                (dark_color.h - light_color.h).abs() < 0.02,
                "{name} keeps its hue across modes ({} vs {})",
                dark_color.h,
                light_color.h
            );
            assert!(
                (dark_color.l - light_color.l).abs() > 0.05,
                "{name} retunes its lightness for the other ground"
            );
        }

        // Selection is the kit's blue list tokens, not `accent`: the kit's
        // accent is a neutral surface (saturation 0) in both modes.
        for (name, wash, rim) in [
            ("dark", dark.list_active, dark.list_active_border),
            ("light", light.list_active, light.list_active_border),
        ] {
            assert!(
                (0.55..0.70).contains(&wash.h) && wash.s > 0.5,
                "{name} selection wash is blue, got {wash:?}"
            );
            assert!(
                (0.55..0.70).contains(&rim.h) && rim.s > 0.5,
                "{name} selection rim is blue, got {rim:?}"
            );
            assert!(wash.a < 1.0, "{name} selection is a wash, not a fill");
        }
        for (name, accent) in [("dark", dark.accent), ("light", light.accent)] {
            assert!(
                accent.s < 0.01,
                "{name} accent is a neutral surface, not a signal color: {accent:?}"
            );
        }

        assert_eq!(dark.radius, light.radius, "one radius across modes");
    }
}
