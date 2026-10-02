//! Ticket 38: tray presence + opt-in autostart + session restore.
//!
//! The tray icon comes from `assets/icon.ico` (no new art): `build.rs`
//! decodes the preferred PNG entry to RGBA and emits
//! `OUT_DIR/tray_icon_rgba.rs`, included below for `tray-icon`'s
//! `Icon::from_rgba` (its only icon constructor).
//!
//! Window hide/restore goes through the native `HWND` (`ShowWindow`
//! `SW_HIDE`/`SW_SHOW`) because GPUI exposes only minimize/activate —
//! minimizing leaves a taskbar button, which is not a hide. The `HWND`
//! comes from the GPUI `Window` via `raw-window-handle` on the main
//! thread, so the engine (and its endpoints) is never touched: hiding is
//! purely windowing, closing never kills proxies.
//!
//! Autostart writes `HKCU\...\Run` ONLY from the explicit Settings
//! checkbox ([`set_autostart_enabled`]); every helper takes an explicit
//! key path so tests use a temp key, never the real Run key.

include!(concat!(env!("OUT_DIR"), "/tray_icon_rgba.rs"));

use crate::store::ProxyConfig;

/// Tray tooltip: the app name plus the live running count.
pub fn tray_tooltip(running: usize) -> String {
    format!("ManyProxy · {running} running")
}

/// Menu item ids for the tray menu (Show / Start all / Stop all / Quit).
pub mod menu_id {
    pub const SHOW: &str = "tray-show";
    pub const START_ALL: &str = "tray-start-all";
    pub const STOP_ALL: &str = "tray-stop-all";
    pub const QUIT: &str = "tray-quit";
}

/// Actions behind the tray menu (and the left-click toggle, which is Show).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    StartAll,
    StopAll,
    Quit,
}

/// Map a menu item id to its action (`None` for foreign ids).
pub fn action_for_menu_id(id: &str) -> Option<TrayAction> {
    match id {
        menu_id::SHOW => Some(TrayAction::Show),
        menu_id::START_ALL => Some(TrayAction::StartAll),
        menu_id::STOP_ALL => Some(TrayAction::StopAll),
        menu_id::QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

/// The menu item id for an action (inverse of [`action_for_menu_id`]).
pub fn menu_id_for_action(action: TrayAction) -> &'static str {
    match action {
        TrayAction::Show => menu_id::SHOW,
        TrayAction::StartAll => menu_id::START_ALL,
        TrayAction::StopAll => menu_id::STOP_ALL,
        TrayAction::Quit => menu_id::QUIT,
    }
}

/// Value name of the autostart entry.
pub const AUTOSTART_VALUE_NAME: &str = "ManyProxy";

/// Real Run key path (never used by tests).
pub const AUTOSTART_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Quoted current-exe command for the Run value (`None` when the exe path
/// is unknown — the checkbox then reports the failure instead of writing
/// a broken value).
pub fn autostart_command() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| format!("\"{}\"", path.display()))
}

/// Read the autostart command stored under `key_path` (`None` when absent
/// or unreadable — absent and broken both read as off).
#[cfg(windows)]
pub fn read_autostart_at(key_path: &str) -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(key_path)
        .ok()?
        .get_value::<String, _>(AUTOSTART_VALUE_NAME)
        .ok()
}

#[cfg(not(windows))]
pub fn read_autostart_at(_key_path: &str) -> Option<String> {
    None
}

/// Write the autostart command under `key_path`.
#[cfg(windows)]
pub fn set_autostart_at(key_path: &str, command: &str) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(key_path)
        .map_err(|error| format!("cannot open autostart key ({error})"))?;
    key.set_value(AUTOSTART_VALUE_NAME, &command.to_string())
        .map_err(|error| format!("cannot write autostart value ({error})"))
}

#[cfg(not(windows))]
pub fn set_autostart_at(_key_path: &str, _command: &str) -> Result<(), String> {
    Err("autostart is Windows-only".to_string())
}

/// Remove the autostart value under `key_path` (absent counts as removed).
#[cfg(windows)]
pub fn clear_autostart_at(key_path: &str) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;
    let root = RegKey::predef(HKEY_CURRENT_USER);
    // Read + write access: the existence check reads, the delete writes.
    // A plain `open_subkey` (read-only) denies the delete (os error 5).
    let key = match root.open_subkey_with_flags(key_path, KEY_READ | KEY_WRITE) {
        Ok(key) => key,
        // No key means no value — already off.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot open autostart key ({error})")),
    };
    match key.get_value::<String, _>(AUTOSTART_VALUE_NAME) {
        // No value means already off (checked first so a missing value is
        // never an error, whatever the OS reports on delete).
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot read autostart value ({error})")),
        Ok(_) => key
            .delete_value(AUTOSTART_VALUE_NAME)
            .map_err(|error| format!("cannot clear autostart value ({error})")),
    }
}

#[cfg(not(windows))]
pub fn clear_autostart_at(_key_path: &str) -> Result<(), String> {
    Err("autostart is Windows-only".to_string())
}

/// True when the real Run value equals this exe's command (a stale value
/// from a moved exe reads as off, never as on).
pub fn autostart_enabled() -> bool {
    match (read_autostart_at(AUTOSTART_RUN_KEY), autostart_command()) {
        (Some(stored), Some(command)) => stored == command,
        _ => false,
    }
}

/// The explicit opt-in behind the Settings checkbox: `true` writes the
/// real Run value, `false` removes it. The ONLY writer of the real key.
pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let command = autostart_command()
            .ok_or_else(|| "cannot determine the app path for autostart".to_string())?;
        set_autostart_at(AUTOSTART_RUN_KEY, &command)?;
        // Read back: a write that does not stick is a loud failure, never
        // a checkbox that lies.
        if read_autostart_at(AUTOSTART_RUN_KEY).as_deref() == Some(command.as_str()) {
            Ok(())
        } else {
            Err("autostart write did not stick".to_string())
        }
    } else {
        clear_autostart_at(AUTOSTART_RUN_KEY)
    }
}

/// Split a restore snapshot into rows that start immediately (loopback
/// binds) and rows that stay behind the LAN confirm gate. Unknown ids
/// (deleted rows) are dropped. Library order is preserved in both arms.
pub fn partition_restore(
    rows: &[ProxyConfig],
    snapshot: &[String],
) -> (Vec<ProxyConfig>, Vec<ProxyConfig>) {
    let wanted: std::collections::HashSet<&str> = snapshot.iter().map(String::as_str).collect();
    let mut now = Vec::new();
    let mut gated = Vec::new();
    for row in rows {
        if !wanted.contains(row.id.as_str()) {
            continue;
        }
        if crate::engine::requires_lan_confirm(&row.bind_ip) {
            gated.push(row.clone());
        } else {
            now.push(row.clone());
        }
    }
    (now, gated)
}

/// Native window hide/show through the `HWND` (see module docs).
/// `false` when the handle is null or the call cannot apply.
#[cfg(windows)]
pub fn hide_window_handle(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    // SAFETY: `ShowWindow` on a non-null `HWND` is synchronous and never
    // dereferences our memory; a stale handle fails inside the OS without
    // touching us. Called on the main thread only.
    unsafe {
        native::ShowWindow(hwnd, native::SW_HIDE);
    }
    true
}

/// Restore a hidden window through the `HWND` (see module docs).
#[cfg(windows)]
pub fn show_window_handle(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    // SAFETY: same contract as [`hide_window_handle`].
    unsafe {
        native::ShowWindow(hwnd, native::SW_SHOW);
    }
    true
}

/// Hand-rolled `user32` hide/show (two constants, one call — the same
/// minimal-FFI shape as `dpapi.rs`, no `windows`-crate weight).
#[cfg(windows)]
mod native {
    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOW: i32 = 5;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }
}

#[cfg(not(windows))]
pub fn hide_window_handle(_hwnd: isize) -> bool {
    false
}

#[cfg(not(windows))]
pub fn show_window_handle(_hwnd: isize) -> bool {
    false
}

/// Extract the Win32 `HWND` behind a GPUI window (`None` off-Windows or
/// when the handle is not a Win32 one). Main thread only, like every
/// GPUI window call.
#[cfg(windows)]
pub fn hwnd_from_window(window: &gpui_kit::Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    // Fully qualified: `Window` also has an inherent `window_handle`
    // returning GPUI's `AnyWindowHandle`; the trait gives the OS handle.
    match HasWindowHandle::window_handle(window) {
        Ok(handle) => match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        },
        Err(_) => None,
    }
}

#[cfg(not(windows))]
pub fn hwnd_from_window(_window: &gpui_kit::Window) -> Option<isize> {
    None
}

/// Live tray presence. Owns the `TrayIcon` (dropping it removes the icon
/// via `NIM_DELETE`) and the menu behind it. Created and polled on the
/// GPUI main thread only.
pub struct TrayHandle {
    icon: tray_icon::TrayIcon,
}

impl TrayHandle {
    /// Build the icon + Show/Start all/Stop all/Quit menu on the calling
    /// (main) thread. Fails loudly when the shell refuses the icon — the
    /// caller then keeps the pre-tray close-confirm instead of a dishonest
    /// tray-less hide.
    pub fn build() -> Result<Self, String> {
        use tray_icon::{
            menu::{Menu, MenuItem},
            Icon, TrayIconBuilder,
        };
        let icon = Icon::from_rgba(TRAY_ICON_RGBA.to_vec(), TRAY_ICON_WIDTH, TRAY_ICON_HEIGHT)
            .map_err(|error| format!("tray icon invalid ({error:?})"))?;
        let menu = Menu::new();
        for (action, title) in [
            (TrayAction::Show, "Show"),
            (TrayAction::StartAll, "Start all"),
            (TrayAction::StopAll, "Stop all"),
            (TrayAction::Quit, "Quit"),
        ] {
            menu.append(&MenuItem::with_id(
                menu_id_for_action(action),
                title,
                true,
                None,
            ))
            .map_err(|error| format!("tray menu invalid ({error:?})"))?;
        }
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip(tray_tooltip(0))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| format!("cannot create tray icon ({error:?})"))?;
        Ok(Self { icon })
    }

    /// Refresh the `ManyProxy · N running` tooltip (best-effort: a tooltip
    /// that does not stick never breaks serving).
    pub fn set_running(&self, running: usize) {
        let _ = self.icon.set_tooltip(Some(tray_tooltip(running)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy_row(id: &str, bind: &str) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: format!("row-{id}"),
            protocol: "vless".to_string(),
            params: "{}".to_string(),
            group: "default".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    /// Ticket 38: the build-decode icon is the 32x32 entry with a full
    /// RGBA buffer (same asset as the exe icon, no new art).
    #[test]
    fn tray_icon_rgba_is_32x32() {
        assert_eq!((TRAY_ICON_WIDTH, TRAY_ICON_HEIGHT), (32, 32));
        assert_eq!(TRAY_ICON_RGBA.len(), 32 * 32 * 4);
        // Non-trivial pixels: not all zeros, not all opaque white.
        assert!(TRAY_ICON_RGBA.iter().any(|byte| *byte != 0));
        assert!(TRAY_ICON_RGBA.iter().any(|byte| *byte != 255));
    }

    /// Ticket 38: the tooltip names the app plus the live count.
    #[test]
    fn tooltip_carries_running_count() {
        assert_eq!(tray_tooltip(0), "ManyProxy · 0 running");
        assert_eq!(tray_tooltip(3), "ManyProxy · 3 running");
    }

    /// Ticket 38: every menu id maps to exactly one action and back.
    #[test]
    fn menu_ids_round_trip() {
        for (id, action) in [
            (menu_id::SHOW, TrayAction::Show),
            (menu_id::START_ALL, TrayAction::StartAll),
            (menu_id::STOP_ALL, TrayAction::StopAll),
            (menu_id::QUIT, TrayAction::Quit),
        ] {
            assert_eq!(action_for_menu_id(id), Some(action));
            assert_eq!(menu_id_for_action(action), id);
        }
        assert_eq!(action_for_menu_id("foreign"), None);
    }

    /// Ticket 38: autostart round-trips on a temp key (never the real Run
    /// key); absent reads as off; clearing an absent value still succeeds.
    #[test]
    fn autostart_round_trips_on_temp_key() {
        let key = format!(r"Software\ManyProxyTrayTest\{}", std::process::id());
        assert_eq!(read_autostart_at(&key), None);
        assert!(clear_autostart_at(&key).is_ok());
        set_autostart_at(&key, "\"C:\\test\\manyproxy.exe\"").expect("temp key writes");
        assert_eq!(
            read_autostart_at(&key),
            Some("\"C:\\test\\manyproxy.exe\"".to_string())
        );
        clear_autostart_at(&key).expect("temp key clears");
        assert_eq!(read_autostart_at(&key), None);
        // Best-effort cleanup of the temp key itself.
        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&key);
        }
    }

    /// Ticket 38: restore partition keeps loopback immediate, gates LAN,
    /// drops deleted rows, and preserves library order.
    #[test]
    fn restore_partition_gates_lan() {
        let rows = vec![
            proxy_row("1", "127.0.0.1"),
            proxy_row("2", "0.0.0.0"),
            proxy_row("3", "::1"),
        ];
        let snapshot = vec!["2".to_string(), "9".to_string(), "1".to_string()];
        let (now, gated) = partition_restore(&rows, &snapshot);
        assert_eq!(
            now.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["1"]
        );
        assert_eq!(
            gated.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["2"]
        );
    }

    /// Ticket 38: null handles never crash (headless-safe); there is no
    /// window behind them so both report not-applied.
    #[test]
    fn null_handles_report_not_applied() {
        assert!(!hide_window_handle(0));
        assert!(!show_window_handle(0));
    }

    /// Autostart command is the quoted current-exe path (the exact value
    /// the Settings checkbox writes to the Run key).
    #[test]
    fn autostart_command_is_quoted_exe_path() {
        let command = autostart_command().expect("current exe is known");
        assert!(
            command.starts_with('"') && command.ends_with('"'),
            "quoted command, got {command}"
        );
        let inner = &command[1..command.len() - 1];
        assert!(!inner.is_empty(), "exe path inside quotes");
        assert_eq!(
            command,
            format!(
                "\"{}\"",
                std::env::current_exe().expect("exe").display()
            )
        );
    }

    /// A stale Run value (moved exe) never reads as on: it differs from
    /// this exe's command, so the equality behind `autostart_enabled`
    /// reports off.
    #[cfg(windows)]
    #[test]
    fn autostart_stale_value_reads_as_off() {
        let key = format!(r"Software\ManyProxyTrayTest\stale-{}", std::process::id());
        let stale = "\"C:\\old\\location\\manyproxy.exe\"";
        set_autostart_at(&key, stale).expect("temp key writes");
        let stored = read_autostart_at(&key).expect("temp key reads");
        assert_eq!(stored, "\"C:\\old\\location\\manyproxy.exe\"");
        let command = autostart_command().expect("current exe is known");
        assert_ne!(stored, command, "stale differs from this exe");
        assert_ne!(
            read_autostart_at(&key),
            autostart_command(),
            "stale mismatch reads as off"
        );
        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&key);
        }
    }

    /// Null HWND plus tray build is headless-safe: null never applies,
    /// build either yields an icon whose tooltip refresh never panics or
    /// fails loudly with a tray cause.
    #[test]
    fn tray_build_and_null_hwnd_are_headless_safe() {
        assert!(!hide_window_handle(0));
        assert!(!show_window_handle(0));
        match TrayHandle::build() {
            Ok(handle) => {
                handle.set_running(0);
                handle.set_running(3);
                assert_eq!(tray_tooltip(3), "ManyProxy · 3 running");
            }
            Err(error) => {
                assert!(!error.is_empty(), "loud failure");
                assert!(
                    error.to_lowercase().contains("tray"),
                    "tray cause, got {error}"
                );
            }
        }
    }
}
