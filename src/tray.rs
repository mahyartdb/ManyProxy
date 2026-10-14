include!(concat!(env!("OUT_DIR"), "/tray_icon_rgba.rs"));

use crate::store::ProxyConfig;

pub fn tray_tooltip(running: usize) -> String {
    format!("ManyProxy · {running} running")
}

pub mod menu_id {
    pub const SHOW: &str = "tray-show";
    pub const SELECT_EXIT: &str = "tray-select-exit";
    pub const SELECT_PRESET: &str = "tray-select-preset";
    pub const START_ALL: &str = "tray-start-all";
    pub const STOP_ALL: &str = "tray-stop-all";
    pub const OPEN_CONFIG: &str = "tray-open-config";
    pub const QUIT: &str = "tray-quit";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Show,

    SelectExit,

    SelectPreset,
    StartAll,
    StopAll,

    OpenConfig,
    Quit,
}

pub fn action_for_menu_id(id: &str) -> Option<TrayAction> {
    match id {
        menu_id::SHOW => Some(TrayAction::Show),
        menu_id::SELECT_EXIT => Some(TrayAction::SelectExit),
        menu_id::SELECT_PRESET => Some(TrayAction::SelectPreset),
        menu_id::START_ALL => Some(TrayAction::StartAll),
        menu_id::STOP_ALL => Some(TrayAction::StopAll),
        menu_id::OPEN_CONFIG => Some(TrayAction::OpenConfig),
        menu_id::QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

pub fn menu_id_for_action(action: TrayAction) -> &'static str {
    match action {
        TrayAction::Show => menu_id::SHOW,
        TrayAction::SelectExit => menu_id::SELECT_EXIT,
        TrayAction::SelectPreset => menu_id::SELECT_PRESET,
        TrayAction::StartAll => menu_id::START_ALL,
        TrayAction::StopAll => menu_id::STOP_ALL,
        TrayAction::OpenConfig => menu_id::OPEN_CONFIG,
        TrayAction::Quit => menu_id::QUIT,
    }
}

pub fn config_folder() -> String {
    let path = crate::store::Store::default_path();
    path.parent()
        .map(|parent| parent.display().to_string())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

pub const AUTOSTART_VALUE_NAME: &str = "ManyProxy";

pub const AUTOSTART_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

pub const AUTOSTART_MINIMIZED_FLAG: &str = "--tray";

pub fn autostart_command() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| format!("\"{}\" {AUTOSTART_MINIMIZED_FLAG}", path.display()))
}

fn autostart_command_legacy() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| format!("\"{}\"", path.display()))
}

pub fn launched_minimized() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_MINIMIZED_FLAG)
}

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

#[cfg(windows)]
pub fn clear_autostart_at(key_path: &str) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;
    let root = RegKey::predef(HKEY_CURRENT_USER);

    let key = match root.open_subkey_with_flags(key_path, KEY_READ | KEY_WRITE) {
        Ok(key) => key,

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot open autostart key ({error})")),
    };
    match key.get_value::<String, _>(AUTOSTART_VALUE_NAME) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => key
            .delete_value(AUTOSTART_VALUE_NAME)
            .map_err(|error| format!("cannot clear autostart value ({error})")),

        Err(_) => key
            .delete_value(AUTOSTART_VALUE_NAME)
            .map_err(|error| format!("cannot clear autostart value ({error})")),
    }
}

#[cfg(not(windows))]
pub fn clear_autostart_at(_key_path: &str) -> Result<(), String> {
    Err("autostart is Windows-only".to_string())
}

pub fn autostart_enabled() -> bool {
    autostart_value_is_on(read_autostart_at(AUTOSTART_RUN_KEY).as_deref())
}

fn autostart_value_is_on(stored: Option<&str>) -> bool {
    match (stored, autostart_command()) {
        (Some(stored), Some(command)) => {
            stored == command || autostart_command_legacy().is_some_and(|legacy| stored == legacy)
        }
        _ => false,
    }
}

fn autostart_target_exe(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let exe = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next().unwrap_or("").to_string()
    } else {
        trimmed.split_whitespace().next().unwrap_or("").to_string()
    };
    if exe.is_empty() {
        None
    } else {
        Some(exe)
    }
}

#[cfg(windows)]
pub fn clear_dead_autostart_target_at(key_path: &str) -> bool {
    let Some(stored) = read_autostart_at(key_path) else {
        return false;
    };
    let live = autostart_target_exe(&stored).is_some_and(|exe| std::path::Path::new(&exe).exists());
    if live {
        return false;
    }
    clear_autostart_at(key_path).is_ok()
}

#[cfg(not(windows))]
pub fn clear_dead_autostart_target_at(_key_path: &str) -> bool {
    false
}

pub fn clear_dead_autostart_target() -> bool {
    clear_dead_autostart_target_at(AUTOSTART_RUN_KEY)
}

pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let command = autostart_command()
            .ok_or_else(|| "cannot determine the app path for autostart".to_string())?;
        set_autostart_at(AUTOSTART_RUN_KEY, &command)?;

        if read_autostart_at(AUTOSTART_RUN_KEY).as_deref() == Some(command.as_str()) {
            Ok(())
        } else {
            Err("autostart write did not stick".to_string())
        }
    } else {
        clear_autostart_at(AUTOSTART_RUN_KEY)
    }
}

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

#[cfg(windows)]
pub fn hide_window_handle(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }

    unsafe {
        if native::IsWindow(hwnd) == 0 {
            return false;
        }

        let was_visible = native::ShowWindow(hwnd, native::SW_HIDE);
        was_visible != 0 || native::IsWindowVisible(hwnd) == 0
    }
}

#[cfg(windows)]
pub fn show_window_handle(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }

    unsafe {
        if native::IsWindow(hwnd) == 0 {
            return false;
        }
        let was_visible = native::ShowWindow(hwnd, native::SW_SHOW);
        was_visible != 0 || native::IsWindowVisible(hwnd) != 0
    }
}

#[cfg(windows)]
mod native {
    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOW: i32 = 5;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        pub fn IsWindow(hwnd: isize) -> i32;
        pub fn IsWindowVisible(hwnd: isize) -> i32;
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

#[cfg(windows)]
pub fn hwnd_from_window(window: &gpui_kit::Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

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

pub struct TrayHandle {
    icon: tray_icon::TrayIcon,
}

impl TrayHandle {
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
            (TrayAction::SelectExit, "Select exit"),
            (TrayAction::SelectPreset, "Select preset"),
            (TrayAction::StartAll, "Start all"),
            (TrayAction::StopAll, "Stop all"),
            (TrayAction::OpenConfig, "Open config folder"),
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

    #[test]
    fn tray_icon_rgba_is_32x32() {
        assert_eq!((TRAY_ICON_WIDTH, TRAY_ICON_HEIGHT), (32, 32));
        assert_eq!(TRAY_ICON_RGBA.len(), 32 * 32 * 4);

        assert!(TRAY_ICON_RGBA.iter().any(|byte| *byte != 0));
        assert!(TRAY_ICON_RGBA.iter().any(|byte| *byte != 255));
    }

    #[test]
    fn tooltip_carries_running_count() {
        assert_eq!(tray_tooltip(0), "ManyProxy · 0 running");
        assert_eq!(tray_tooltip(3), "ManyProxy · 3 running");
    }

    #[test]
    fn menu_ids_round_trip() {
        for (id, action) in [
            (menu_id::SHOW, TrayAction::Show),
            (menu_id::SELECT_EXIT, TrayAction::SelectExit),
            (menu_id::SELECT_PRESET, TrayAction::SelectPreset),
            (menu_id::START_ALL, TrayAction::StartAll),
            (menu_id::STOP_ALL, TrayAction::StopAll),
            (menu_id::OPEN_CONFIG, TrayAction::OpenConfig),
            (menu_id::QUIT, TrayAction::Quit),
        ] {
            assert_eq!(action_for_menu_id(id), Some(action));
            assert_eq!(menu_id_for_action(action), id);
        }
        assert_eq!(action_for_menu_id("foreign"), None);
    }

    #[test]
    fn config_folder_is_parent_of_db_path() {
        let folder = config_folder();
        assert!(!folder.trim().is_empty(), "folder never empty");
        let db = crate::store::Store::default_path().display().to_string();
        assert!(
            db.starts_with(&folder),
            "folder contains the db path ({db} vs {folder})"
        );
    }

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

        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&key);
        }
    }

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

    #[test]
    fn null_handles_report_not_applied() {
        assert!(!hide_window_handle(0));
        assert!(!show_window_handle(0));
    }

    #[test]
    fn autostart_command_is_quoted_exe_path() {
        let command = autostart_command().expect("current exe is known");
        let exe = std::env::current_exe().expect("exe");
        assert_eq!(
            command,
            format!("\"{}\" {AUTOSTART_MINIMIZED_FLAG}", exe.display())
        );

        assert!(!launched_minimized(), "test harness did not pass --tray");
    }

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

    #[test]
    fn autostart_on_predicate_accepts_current_and_legacy() {
        let command = autostart_command().expect("exe known");
        assert!(autostart_value_is_on(Some(&command)));
        let legacy = autostart_command_legacy().expect("exe known");
        assert!(autostart_value_is_on(Some(&legacy)));
        assert!(!autostart_value_is_on(Some(
            "\"C:\\other\\manyproxy.exe\" --tray"
        )));
        assert!(!autostart_value_is_on(None));
    }

    #[cfg(windows)]
    #[test]
    fn dead_autostart_target_cleans_up() {
        let key = format!(r"Software\ManyProxyTrayTest\dead-{}", std::process::id());
        assert!(!clear_dead_autostart_target_at(&key), "absent stays absent");
        set_autostart_at(&key, "\"C:\\definitely\\not\\here\\manyproxy.exe\" --tray")
            .expect("dead value writes");
        assert!(clear_dead_autostart_target_at(&key), "dead entry removed");
        assert_eq!(read_autostart_at(&key), None);
        set_autostart_at(&key, &autostart_command().expect("exe known")).expect("live writes");
        assert!(!clear_dead_autostart_target_at(&key), "live target kept");
        clear_autostart_at(&key).expect("cleanup");
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&key);
    }

    #[cfg(windows)]
    #[test]
    fn corrupt_autostart_type_still_clears() {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let key_path = format!(r"Software\ManyProxyTrayTest\corrupt-{}", std::process::id());
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(&key_path)
            .expect("temp key");
        key.set_value(AUTOSTART_VALUE_NAME, &42u32)
            .expect("dword writes");
        drop(key);
        clear_autostart_at(&key_path).expect("corrupt value still clears");
        assert_eq!(read_autostart_at(&key_path), None);
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&key_path);
    }
}
