//! AppView's side of the Windows shell integration. The raw calls live in
//! `crate::os`; this file only translates app state into the typed requests and
//! routes shell events back through the same actions as the toolbar.

use crate::*;

impl AppView {
    /// Called once, after the window exists. A test harness has no Win32
    /// window, so `hwnd_from_window` returns `None` and every later OS call
    /// stays a no-op.
    pub(crate) fn install_os(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hwnd) = crate::tray::hwnd_from_window(window) else {
            return;
        };
        self.os_taskbar = Some(crate::os::taskbar::Taskbar::install(hwnd));
        self.sync_taskbar_buttons(cx);
    }

    /// Runs on the existing tray pump. A taskbar button click is a user action
    /// with no entity callback, so the pump drains the queue and dispatches it
    /// through the same methods the toolbar calls.
    pub(crate) fn poll_os_once(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    crate::os::taskbar::ThumbAction::StartAll => {
                        self.start_all_engines(window, cx);
                    }
                    crate::os::taskbar::ThumbAction::StopAll => {
                        self.stop_all_engines(window, cx);
                    }
                    crate::os::taskbar::ThumbAction::TestAll => {
                        self.test_all_health(window, cx);
                    }
                },
            }
        }
    }

    pub(crate) fn set_taskbar_progress(&mut self, done: usize, total: usize, failed: usize) {
        let progress = crate::os::taskbar::Progress::from_counts(done, total, failed);
        if let Some(taskbar) = &mut self.os_taskbar {
            taskbar.set_progress(progress);
        }
    }

    pub(crate) fn clear_taskbar_progress(&mut self) {
        if let Some(taskbar) = &mut self.os_taskbar {
            taskbar.set_progress(None);
        }
    }

    fn taskbar_button_states(&self, cx: &App) -> crate::os::taskbar::ButtonStates {
        crate::os::taskbar::button_states(
            self.engine.running_count(),
            self.config_count(cx),
            self.start_in_flight,
            self.probe_url_in_flight || self.group_test_in_flight,
        )
    }

    fn sync_taskbar_buttons(&mut self, cx: &App) {
        let states = self.taskbar_button_states(cx);
        if let Some(taskbar) = &mut self.os_taskbar {
            taskbar.sync_buttons(states);
        }
    }
}
