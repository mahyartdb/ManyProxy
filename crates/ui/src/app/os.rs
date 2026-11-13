//! AppView's side of the Windows shell integration. The raw calls live in
//! `crate::os`; this file only translates app state into the typed requests and
//! routes shell events back through the same actions as the toolbar.

use crate::*;

impl AppView {
    /// Called once, after the window exists. A test harness has no Win32
    /// window, so `hwnd_from_window` returns `None` and every later OS call
    /// stays a no-op.
    pub fn install_os(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hwnd) = crate::tray::hwnd_from_window(window) else {
            return;
        };
        self.os_taskbar = Some(crate::os::taskbar::Taskbar::install(hwnd));
        self.os_notifier = Some(crate::os::notify::Notifier::install(hwnd));
        self.sync_taskbar_buttons(cx);
    }

    /// Runs on the existing tray pump. A taskbar button click is a user action
    /// with no entity callback, so the pump drains the queue and dispatches it
    /// through the same methods the toolbar calls.
    pub fn poll_os_once(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_taskbar_buttons(cx);
        for event in crate::os::taskbar::drain_events() {
            match event {
                crate::os::taskbar::TaskbarEvent::ButtonCreated => {
                    let states = self.taskbar_button_states(cx);
                    if let Some(taskbar) = &mut self.os_taskbar {
                        taskbar.taskbar_button_created(states);
                    }
                }
                crate::os::taskbar::TaskbarEvent::Action(action) => match action {
                    crate::os::taskbar::ThumbAction::Start => {
                        self.start_all_engines(window, cx);
                    }
                    crate::os::taskbar::ThumbAction::Stop => {
                        self.stop_all_engines(window, cx);
                    }
                    crate::os::taskbar::ThumbAction::Test => {
                        self.test_all_health(window, cx);
                    }
                },
            }
        }
        for target in crate::os::notify::drain_clicks() {
            self.show_window_from_tray(window, cx);
            match target {
                crate::os::notify::NoticeTarget::Logs => {
                    self.open_console_scoped_to_selection(false, cx);
                }
                crate::os::notify::NoticeTarget::Subscriptions => {
                    self.open_subscriptions_dialog(window, cx);
                }
                crate::os::notify::NoticeTarget::Update => {
                    // The stored release is the one the balloon announced; if
                    // a manual check already cleared it, run the manual check
                    // the click asked for.
                    if !self.open_available_update(window, cx) {
                        self.check_for_updates(window, cx);
                    }
                }
            }
        }
    }

    /// One balloon, subject to the same visibility/setting/dedup rules the
    /// policy unit tests pin. The caller supplies the event text the in-app
    /// toast would have shown.
    pub fn os_notify(
        &mut self,
        kind: crate::os::notify::NoticeKind,
        title: &str,
        body: &str,
    ) {
        let enabled = self.notify_from_tray_enabled();
        let Some(notifier) = self.os_notifier.as_ref() else {
            return;
        };
        let hidden = notifier.window_hidden();
        if !self
            .os_notice_policy
            .allow(kind, std::time::Instant::now(), hidden, enabled)
        {
            return;
        }
        if let Some(notifier) = self.os_notifier.as_mut() {
            notifier.show(kind, title, body);
        }
    }

    /// The deferred Phase 3 balloon: a passive check found a newer release
    /// while the window is out of sight. A visible window gets the status-bar
    /// hint alone, and the policy still owns dedup and flood.
    pub fn os_notify_update(&mut self, release: &FetchedRelease) {
        let Some(notifier) = self.os_notifier.as_ref() else {
            return;
        };
        let hidden = notifier.window_hidden();
        if !crate::os::notify::update_notice_allowed(
            self.passive_update_check_enabled(),
            hidden,
            self.notify_from_tray_enabled(),
        ) {
            return;
        }
        let title = t!("update-notice-title");
        let body = t!("update-notice-body", version = release.tag_name.trim());
        self.os_notify(crate::os::notify::NoticeKind::UpdateFound, &title, &body);
    }

    pub fn set_taskbar_progress(&mut self, done: usize, total: usize, failed: usize) {
        let progress = crate::os::taskbar::Progress::from_counts(done, total, failed);
        if let Some(taskbar) = &mut self.os_taskbar {
            taskbar.set_progress(progress);
        }
    }

    pub fn clear_taskbar_progress(&mut self) {
        if let Some(taskbar) = &mut self.os_taskbar {
            taskbar.set_progress(None);
        }
    }

    pub fn taskbar_button_states(&self, cx: &App) -> crate::os::taskbar::ButtonStates {
        crate::os::taskbar::button_states(
            self.engine.running_count(),
            self.config_count(cx),
            self.start_in_flight,
            self.probe_url_in_flight || self.group_test_in_flight,
        )
    }

    pub fn sync_taskbar_buttons(&mut self, cx: &App) {
        let states = self.taskbar_button_states(cx);
        if let Some(taskbar) = &mut self.os_taskbar {
            taskbar.sync_buttons(states);
        }
    }
}
