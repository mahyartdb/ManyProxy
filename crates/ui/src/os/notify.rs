//! Balloon notifications for events the user would otherwise miss while the
//! window is hidden.
//!
//! The notification area icon is deliberately invisible (`NIS_HIDDEN`); it
//! exists only to own the balloon. The app's real tray icon belongs to the
//! `tray-icon` crate and owns a different window, so this module keeps its own
//! hidden message window and its own icon id and never touches that one.
//!
//! Which events are shown, how often, and whether the window is hidden at all
//! is decided by [`NoticePolicy`] — pure data so the rules are testable without
//! a shell. Every Win32 call is a logged no-op on failure.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The same event repeating inside this window is one event, not two.
pub const DEDUP_COOLDOWN: Duration = Duration::from_secs(60);
/// Hard ceiling on balloons per rolling hour, regardless of event kind.
pub const FLOOD_WINDOW: Duration = Duration::from_secs(60 * 60);
pub const MAX_PER_WINDOW: usize = 5;

/// Where a click on the balloon should land once the window is back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeTarget {
    Logs,
    Subscriptions,
    Update,
}

/// The four events worth a balloon. The kind is also the dedup key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    EngineCrash,
    SubscriptionFailure,
    PortConflict,
    UpdateFound,
}

impl NoticeKind {
    pub fn target(self) -> NoticeTarget {
        match self {
            Self::EngineCrash | Self::PortConflict => NoticeTarget::Logs,
            Self::SubscriptionFailure => NoticeTarget::Subscriptions,
            Self::UpdateFound => NoticeTarget::Update,
        }
    }

    pub fn slot(self) -> usize {
        match self {
            Self::EngineCrash => 0,
            Self::SubscriptionFailure => 1,
            Self::PortConflict => 2,
            Self::UpdateFound => 3,
        }
    }
}

/// The passive update balloon has three gates before the policy's own dedup
/// and flood rules: the weekly check is on, the window is not in front of the
/// user, and the tray-notification setting is on.
pub fn update_notice_allowed(
    passive_enabled: bool,
    window_hidden: bool,
    notices_enabled: bool,
) -> bool {
    passive_enabled && window_hidden && notices_enabled
}

/// The shell's notification policy. `allow` is the shared gate: it rejects
/// while the window is visible (the in-app toast already covers it), when the
/// user turned the feature off, when the same kind fired recently, and when the
/// rolling hour is full. Only an allowed notice consumes budget. The passive
/// update notice adds [`update_notice_allowed`] before this gate.
#[derive(Debug, Default)]
pub struct NoticePolicy {
    pub last_sent: [Option<Instant>; 4],
    pub recent: VecDeque<Instant>,
}

impl NoticePolicy {
    pub fn allow(
        &mut self,
        kind: NoticeKind,
        now: Instant,
        window_hidden: bool,
        enabled: bool,
    ) -> bool {
        if !enabled || !window_hidden {
            return false;
        }
        if let Some(last) = self.last_sent[kind.slot()] {
            if now.saturating_duration_since(last) < DEDUP_COOLDOWN {
                return false;
            }
        }
        while let Some(front) = self.recent.front() {
            if now.saturating_duration_since(*front) < FLOOD_WINDOW {
                break;
            }
            self.recent.pop_front();
        }
        if self.recent.len() >= MAX_PER_WINDOW {
            return false;
        }
        self.last_sent[kind.slot()] = Some(now);
        self.recent.push_back(now);
        true
    }
}

#[cfg(not(windows))]
pub struct Notifier;

#[cfg(not(windows))]
impl Notifier {
    pub fn install(_main_hwnd: isize) -> Self {
        Self
    }

    pub fn window_hidden(&self) -> bool {
        false
    }

    pub fn show(&mut self, _kind: NoticeKind, _title: &str, _body: &str) {}
}

#[cfg(not(windows))]
pub fn drain_clicks() -> Vec<NoticeTarget> {
    Vec::new()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::sync::Mutex;

    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_STATE, NIF_TIP,
        NIIF_ERROR, NIIF_INFO, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
        NIN_BALLOONUSERCLICK, NIS_HIDDEN, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, IsIconic, IsWindowVisible,
        RegisterClassW, HMENU, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
    };

    use crate::os::icon::hicon_from_rgba;

    const CLASS_NAME: PCWSTR = w!("ManyProxyOsNotify");
    const WINDOW_TITLE: PCWSTR = w!("ManyProxy notifications");
    const ICON_ID: u32 = 1;
    /// `WM_APP + 1`: private to the hidden window, never broadcast.
    const CALLBACK_MESSAGE: u32 = 0x8000 + 1;

    static LAST_TARGET: Mutex<Option<NoticeTarget>> = Mutex::new(None);
    static CLICKS: Mutex<Vec<NoticeTarget>> = Mutex::new(Vec::new());

    pub fn drain_clicks() -> Vec<NoticeTarget> {
        CLICKS
            .lock()
            .map(|mut clicks| clicks.drain(..).collect())
            .unwrap_or_default()
    }

    fn queue_click() {
        let target = LAST_TARGET.lock().ok().and_then(|target| *target);
        if let Some(target) = target {
            if let Ok(mut clicks) = CLICKS.lock() {
                clicks.push(target);
            }
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == CALLBACK_MESSAGE {
            let event = (lparam.0 as u32) & 0xffff;
            if event == NIN_BALLOONUSERCLICK {
                queue_click();
            }
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn write_wide(dest: &mut [u16], text: &str) {
        let limit = dest.len().saturating_sub(1);
        for (slot, unit) in dest.iter_mut().zip(text.encode_utf16().take(limit)) {
            *slot = unit;
        }
    }

    fn info_flags(kind: NoticeKind) -> windows::Win32::UI::Shell::NOTIFY_ICON_INFOTIP_FLAGS {
        match kind {
            NoticeKind::EngineCrash | NoticeKind::PortConflict => NIIF_ERROR,
            NoticeKind::SubscriptionFailure => NIIF_WARNING,
            NoticeKind::UpdateFound => NIIF_INFO,
        }
    }

    pub struct Notifier {
        main: HWND,
        window: HWND,
        icon: Option<windows::Win32::UI::WindowsAndMessaging::HICON>,
        added: bool,
    }

    impl Notifier {
        pub fn install(main_hwnd: isize) -> Self {
            let main = HWND(main_hwnd as *mut c_void);
            if main.0.is_null() {
                return Self::headless();
            }
            unsafe {
                let Ok(module) = GetModuleHandleW(None) else {
                    eprintln!("notify: cannot get this module's handle");
                    return Self::headless();
                };
                let instance = HINSTANCE(module.0);
                let class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: CLASS_NAME,
                    ..Default::default()
                };
                // A second AppView in one process would find the class already
                // registered; either result is fine as long as CreateWindow
                // can find it.
                RegisterClassW(&class);
                let window = match CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    CLASS_NAME,
                    WINDOW_TITLE,
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    HWND::default(),
                    HMENU::default(),
                    instance,
                    None,
                ) {
                    Ok(window) => window,
                    Err(error) => {
                        eprintln!("notify: cannot create the hidden window ({error})");
                        return Self::headless();
                    }
                };
                let icon = hicon_from_rgba(
                    crate::tray::TRAY_ICON_RGBA,
                    crate::tray::TRAY_ICON_WIDTH as usize,
                    crate::tray::TRAY_ICON_HEIGHT as usize,
                );
                let mut notifier = Self {
                    main,
                    window,
                    icon,
                    added: false,
                };
                notifier.add_icon();
                notifier
            }
        }

        fn headless() -> Self {
            Self {
                main: HWND(std::ptr::null_mut()),
                window: HWND(std::ptr::null_mut()),
                icon: None,
                added: false,
            }
        }

        fn icon_data(&self) -> NOTIFYICONDATAW {
            NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.window,
                uID: ICON_ID,
                ..Default::default()
            }
        }

        fn add_icon(&mut self) {
            let Some(icon) = self.icon else {
                return;
            };
            let mut data = self.icon_data();
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP | NIF_STATE;
            data.uCallbackMessage = CALLBACK_MESSAGE;
            data.hIcon = icon;
            write_wide(&mut data.szTip, "ManyProxy notifications");
            data.dwState = NIS_HIDDEN;
            data.dwStateMask = NIS_HIDDEN;
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            self.added = unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() };
            if !self.added {
                eprintln!("notify: cannot add the hidden notification icon");
                return;
            }
            let mut version = self.icon_data();
            version.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &version).as_bool() } {
                // Version 4 only affects how a click identifies itself; the
                // balloon still works, so this is not fatal.
                eprintln!("notify: cannot select notification version 4");
            }
        }

        /// Visible or minimized counts as hidden for notification purposes:
        /// the user is not looking at the list, so the balloon is the only
        /// surface that can reach them.
        pub fn window_hidden(&self) -> bool {
            unsafe {
                let visible = IsWindowVisible(self.main).as_bool();
                let iconic = IsIconic(self.main).as_bool();
                !visible || iconic
            }
        }

        pub fn show(&mut self, kind: NoticeKind, title: &str, body: &str) {
            if !self.added {
                return;
            }
            if let Ok(mut target) = LAST_TARGET.lock() {
                *target = Some(kind.target());
            }
            let mut data = self.icon_data();
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = info_flags(kind);
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            write_wide(&mut data.szInfoTitle, title);
            write_wide(&mut data.szInfo, body);
            if !unsafe { Shell_NotifyIconW(NIM_MODIFY, &data).as_bool() } {
                eprintln!("notify: balloon not shown");
            }
        }
    }

    impl Drop for Notifier {
        fn drop(&mut self) {
            unsafe {
                if self.added {
                    let data = NOTIFYICONDATAW {
                        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                        hWnd: self.window,
                        uID: ICON_ID,
                        ..Default::default()
                    };
                    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                }
                if let Some(icon) = self.icon {
                    let _ = DestroyIcon(icon);
                }
                if !self.window.0.is_null() {
                    let _ = DestroyWindow(self.window);
                }
            }
        }
    }
}

#[cfg(windows)]
pub use win::{drain_clicks, Notifier};

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Instant {
        Instant::now()
    }

    #[test]
    fn visible_window_never_notifies_and_keeps_the_budget() {
        let mut policy = NoticePolicy::default();
        let base = base();
        for _ in 0..20 {
            assert!(!policy.allow(NoticeKind::EngineCrash, base, false, true));
        }
        assert!(policy.allow(NoticeKind::EngineCrash, base, true, true));
    }

    #[test]
    fn the_setting_off_wins_over_every_other_rule() {
        let mut policy = NoticePolicy::default();
        assert!(!policy.allow(NoticeKind::PortConflict, base(), true, false));
    }

    #[test]
    fn the_same_event_dedups_for_a_minute() {
        let mut policy = NoticePolicy::default();
        let base = base();
        assert!(policy.allow(NoticeKind::EngineCrash, base, true, true));
        assert!(!policy.allow(
            NoticeKind::EngineCrash,
            base + Duration::from_secs(59),
            true,
            true
        ));
        assert!(policy.allow(NoticeKind::EngineCrash, base + DEDUP_COOLDOWN, true, true));
    }

    #[test]
    fn different_events_do_not_dedup_each_other() {
        let mut policy = NoticePolicy::default();
        let base = base();
        assert!(policy.allow(NoticeKind::EngineCrash, base, true, true));
        assert!(policy.allow(
            NoticeKind::SubscriptionFailure,
            base + Duration::from_secs(1),
            true,
            true
        ));
        assert!(policy.allow(
            NoticeKind::PortConflict,
            base + Duration::from_secs(2),
            true,
            true
        ));
    }

    #[test]
    fn the_flood_guard_caps_at_five_per_rolling_hour() {
        let mut policy = NoticePolicy::default();
        let base = base();
        let kinds = [
            NoticeKind::EngineCrash,
            NoticeKind::SubscriptionFailure,
            NoticeKind::PortConflict,
        ];
        for index in 0..MAX_PER_WINDOW as u64 {
            assert!(policy.allow(
                kinds[index as usize % kinds.len()],
                base + Duration::from_secs(60 * index),
                true,
                true
            ));
        }
        assert!(!policy.allow(
            NoticeKind::EngineCrash,
            base + Duration::from_secs(300),
            true,
            true
        ));
        // The oldest send has left the rolling hour; one slot is free again.
        assert!(policy.allow(NoticeKind::EngineCrash, base + FLOOD_WINDOW, true, true));
    }

    #[test]
    fn clicks_route_to_the_surface_the_event_belongs_to() {
        assert_eq!(NoticeKind::EngineCrash.target(), NoticeTarget::Logs);
        assert_eq!(NoticeKind::PortConflict.target(), NoticeTarget::Logs);
        assert_eq!(
            NoticeKind::SubscriptionFailure.target(),
            NoticeTarget::Subscriptions
        );
        assert_eq!(NoticeKind::UpdateFound.target(), NoticeTarget::Update);
    }

    #[test]
    fn the_update_balloon_needs_hidden_and_both_switches() {
        assert!(update_notice_allowed(true, true, true));
        assert!(!update_notice_allowed(false, true, true));
        assert!(!update_notice_allowed(true, false, true));
        assert!(!update_notice_allowed(true, true, false));
    }

    #[test]
    fn an_update_notice_obeys_visibility_and_dedup() {
        let mut policy = NoticePolicy::default();
        let base = base();
        assert!(!policy.allow(NoticeKind::UpdateFound, base, false, true));
        assert!(policy.allow(NoticeKind::UpdateFound, base, true, true));
        assert!(!policy.allow(
            NoticeKind::UpdateFound,
            base + Duration::from_secs(59),
            true,
            true
        ));
        assert!(policy.allow(NoticeKind::UpdateFound, base + DEDUP_COOLDOWN, true, true));
    }
}
