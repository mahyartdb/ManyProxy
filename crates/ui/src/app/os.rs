//! AppView's side of the Windows shell integration. The raw calls live in
//! `crate::os`; this file only translates app state into the typed requests.

use crate::*;

impl AppView {
    /// Called once, after the window exists. A test harness has no Win32
    /// window, so `hwnd_from_window` returns `None` and every later OS call
    /// stays a no-op.
    pub(crate) fn install_os(&mut self, window: &mut Window) {
        let Some(hwnd) = crate::tray::hwnd_from_window(window) else {
            return;
        };
        self.os_taskbar = Some(crate::os::taskbar::Taskbar::install(hwnd));
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
}
