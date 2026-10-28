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
