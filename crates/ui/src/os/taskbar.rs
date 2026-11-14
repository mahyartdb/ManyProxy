//! Taskbar progress and the thumbnail toolbar.
//!
//! `ITaskbarList3` and the thumbnail buttons are best-effort shell features:
//! Explorer can restart, a remote session has no taskbar, and the object can be
//! unavailable for reasons the app cannot fix. Every call therefore degrades to
//! a logged no-op; the UI keeps working with no taskbar at all.
//!
//! The window procedure is shared by the process (there is one main window), so
//! button clicks land in a small queue that the UI tick drains. That keeps the
//! raw callback out of the entity and makes clicks testable by id alone.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbAction {
    Start,
    Stop,
    Test,
}

pub mod button_id {
    pub const START_ALL: u32 = 1;
    pub const STOP_ALL: u32 = 2;
    pub const TEST_ALL: u32 = 3;
}

pub const THUMB_BUTTONS: [(u32, ThumbAction); 3] = [
    (button_id::START_ALL, ThumbAction::Start),
    (button_id::STOP_ALL, ThumbAction::Stop),
    (button_id::TEST_ALL, ThumbAction::Test),
];

/// Thumbnail-button tooltip copy. The button table stays ids and actions;
/// only the tooltip needs a locale.
pub fn thumb_label(action: ThumbAction) -> String {
    match action {
        ThumbAction::Start => crate::t!("thumb-start-all"),
        ThumbAction::Stop => crate::t!("thumb-stop-all"),
        ThumbAction::Test => crate::t!("thumb-test-all"),
    }
}

pub fn action_for_button_id(id: u32) -> Option<ThumbAction> {
    THUMB_BUTTONS
        .iter()
        .find(|(button, _)| *button == id)
        .map(|(_, action)| *action)
}

#[cfg(any(test, feature = "test-support"))]
pub fn button_id_for_action(action: ThumbAction) -> u32 {
    THUMB_BUTTONS
        .iter()
        .find(|(_, candidate)| *candidate == action)
        .map(|(button, _)| *button)
        .unwrap_or(button_id::START_ALL)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarEvent {
    /// Explorer (re)created the taskbar button; the buttons must be added again.
    ButtonCreated,
    Action(ThumbAction),
}

/// The enable rules the thumbnail buttons share with the toolbar: Start all
/// while anything is still stopped, Stop all while anything runs, Test only
/// while a running row can be probed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonStates {
    pub start_all: bool,
    pub stop_all: bool,
    pub test_all: bool,
}

impl ButtonStates {
    pub fn is_enabled(self, action: ThumbAction) -> bool {
        match action {
            ThumbAction::Start => self.start_all,
            ThumbAction::Stop => self.stop_all,
            ThumbAction::Test => self.test_all,
        }
    }
}

pub fn button_states(
    running: usize,
    total: usize,
    start_busy: bool,
    test_busy: bool,
) -> ButtonStates {
    ButtonStates {
        start_all: total > 0 && !start_busy && running < total,
        stop_all: running > 0,
        test_all: running > 0 && !test_busy,
    }
}

/// One probe sweep, as the taskbar needs it: completed rows, the sweep's total,
/// and how many of the completed rows failed. The toast reads the same counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub completed: u64,
    pub total: u64,
    pub failed: u32,
}

impl Progress {
    pub fn from_counts(done: usize, total: usize, failed: usize) -> Option<Self> {
        if total == 0 {
            return None;
        }
        Some(Self {
            completed: done.min(total) as u64,
            total: total as u64,
            // A mux warm-up can fail more probes than there are rows; the bar
            // only needs to know "some failed", so clamp to keep the counters
            // describing one sweep.
            failed: failed.min(total) as u32,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    Hidden,
    Normal,
    Error,
}

pub fn progress_state(progress: Option<Progress>) -> ProgressState {
    match progress {
        None => ProgressState::Hidden,
        Some(progress) if progress.total == 0 => ProgressState::Hidden,
        Some(progress) if progress.failed > 0 => ProgressState::Error,
        Some(_) => ProgressState::Normal,
    }
}

#[cfg(not(windows))]
pub struct Taskbar;

#[cfg(not(windows))]
impl Taskbar {
    pub fn install(_hwnd: isize) -> Self {
        Self
    }

    pub fn set_progress(&mut self, _progress: Option<Progress>) {}

    pub fn sync_buttons(&mut self, _states: ButtonStates) {}

    pub fn taskbar_button_created(&mut self, _states: ButtonStates) {}
}

#[cfg(not(windows))]
pub fn drain_events() -> Vec<TaskbarEvent> {
    Vec::new()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;

    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, TBPF_ERROR,
        TBPF_NOPROGRESS, TBPF_NORMAL, THBF_DISABLED, THBF_ENABLED, THBN_CLICKED, THB_FLAGS,
        THB_ICON, THB_TOOLTIP, THUMBBUTTON, THUMBBUTTONFLAGS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        ChangeWindowMessageFilterEx, DestroyIcon, RegisterWindowMessageW, HICON, MSGFLT_ALLOW,
        WM_COMMAND,
    };

    use crate::os::icon::hicon_from_rgba;

    const SUBCLASS_ID: usize = 0x4D50_5854;
    const ICON_SIZE: usize = 32;
    const GLYPH_SIZE: usize = 16;

    static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
    static PENDING: Mutex<Vec<TaskbarEvent>> = Mutex::new(Vec::new());

    fn push_event(event: TaskbarEvent) {
        if let Ok(mut pending) = PENDING.lock() {
            pending.push(event);
        }
    }

    pub fn drain_events() -> Vec<TaskbarEvent> {
        PENDING
            .lock()
            .map(|mut pending| pending.drain(..).collect())
            .unwrap_or_default()
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        let created = TASKBAR_CREATED.load(Ordering::Relaxed);
        if created != 0 && msg == created {
            push_event(TaskbarEvent::ButtonCreated);
        } else if msg == WM_COMMAND {
            let code = ((wparam.0 >> 16) & 0xffff) as u32;
            if code == THBN_CLICKED {
                let id = (wparam.0 & 0xffff) as u32;
                if let Some(action) = action_for_button_id(id) {
                    push_event(TaskbarEvent::Action(action));
                }
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum Glyph {
        Start,
        Stop,
        Test,
    }

    impl Glyph {
        /// A 16×16 stencil. The renderer adds a dark outline around every filled
        /// cell so the same glyph stays readable on a light or dark taskbar.
        fn rows(self) -> [&'static str; GLYPH_SIZE] {
            match self {
                Glyph::Start => [
                    "................",
                    "................",
                    "...####.........",
                    "...######.......",
                    "...########.....",
                    "...##########...",
                    "...############.",
                    "...##############",
                    "...##############",
                    "...############.",
                    "...##########...",
                    "...########.....",
                    "...######.......",
                    "...####.........",
                    "................",
                    "................",
                ],
                Glyph::Stop => [
                    "................",
                    "................",
                    "................",
                    "....########....",
                    "....########....",
                    "....########....",
                    "....########....",
                    "....########....",
                    "....########....",
                    "....########....",
                    "....########....",
                    "....########....",
                    "................",
                    "................",
                    "................",
                    "................",
                ],
                Glyph::Test => [
                    "................",
                    "................",
                    ".......####.....",
                    "......####......",
                    ".....####.......",
                    "....####........",
                    "....####........",
                    "....########....",
                    ".......#####....",
                    "......####......",
                    ".....####.......",
                    "....####........",
                    "...####.........",
                    "...###..........",
                    "................",
                    "................",
                ],
            }
        }
    }

    fn filled(rows: &[&str; GLYPH_SIZE], x: usize, y: usize) -> bool {
        rows.get(y)
            .and_then(|row| row.as_bytes().get(x))
            .map(|cell| *cell == b'#')
            .unwrap_or(false)
    }

    fn edge(rows: &[&str; GLYPH_SIZE], x: usize, y: usize) -> bool {
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0
                    || ny < 0
                    || nx >= GLYPH_SIZE as i32
                    || ny >= GLYPH_SIZE as i32
                    || !filled(rows, nx as usize, ny as usize)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn glyph_rgba(glyph: Glyph) -> Vec<u8> {
        let rows = glyph.rows();
        let mut rgba = vec![0u8; ICON_SIZE * ICON_SIZE * 4];
        let scale = ICON_SIZE / GLYPH_SIZE;
        for y in 0..GLYPH_SIZE {
            for x in 0..GLYPH_SIZE {
                if !filled(&rows, x, y) {
                    continue;
                }
                // The outline is dark enough for a light taskbar thumbnail; the
                // fill is light enough for the dark one.
                let color = if edge(&rows, x, y) {
                    [18u8, 27, 46, 255]
                } else {
                    [245u8, 247, 250, 255]
                };
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = x * scale + dx;
                        let py = y * scale + dy;
                        let index = (py * ICON_SIZE + px) * 4;
                        rgba[index..index + 4].copy_from_slice(&color);
                    }
                }
            }
        }
        rgba
    }

    fn tip(text: &str) -> [u16; 260] {
        let mut wide = [0u16; 260];
        for (slot, unit) in wide.iter_mut().zip(text.encode_utf16().take(259)) {
            *slot = unit;
        }
        wide
    }

    fn flags(enabled: bool) -> THUMBBUTTONFLAGS {
        if enabled {
            THBF_ENABLED
        } else {
            THBF_DISABLED
        }
    }

    pub struct Taskbar {
        hwnd: HWND,
        list: Option<ITaskbarList3>,
        subclassed: bool,
        // `Some` only after Explorer accepted the first add. Until then every
        // state sync is a no-op and `ButtonCreated` retries the add.
        buttons_added: bool,
        last_progress: Option<Progress>,
        last_states: Option<ButtonStates>,
        icons: [Option<HICON>; 3],
    }

    impl Taskbar {
        pub fn install(hwnd: isize) -> Self {
            let hwnd = HWND(hwnd as *mut c_void);
            if hwnd.0.is_null() {
                return Self::headless();
            }
            let subclassed =
                unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0).as_bool() };
            if !subclassed {
                eprintln!("taskbar: cannot subclass the main window");
            }

            let list = unsafe {
                // COM may already be initialized on this thread by GPUI or a
                // shell component. S_FALSE and RPC_E_CHANGED_MODE both leave
                // COM usable, so only a failed CoCreateInstance disables us.
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let created: windows::core::Result<ITaskbarList3> =
                    CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER);
                match created {
                    Ok(list) => match list.HrInit() {
                        Ok(()) => Some(list),
                        Err(error) => {
                            eprintln!("taskbar: HrInit failed ({error})");
                            None
                        }
                    },
                    Err(error) => {
                        eprintln!("taskbar: taskbar object unavailable ({error})");
                        None
                    }
                }
            };

            unsafe {
                let message = RegisterWindowMessageW(w!("TaskbarButtonCreated"));
                if message != 0 {
                    TASKBAR_CREATED.store(message, Ordering::Relaxed);
                    // An elevated app only receives this message when it is
                    // explicitly allowed through UIPI.
                    let _ = ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, None);
                }
            }

            let icons = [
                hicon_from_rgba(&glyph_rgba(Glyph::Start), ICON_SIZE, ICON_SIZE),
                hicon_from_rgba(&glyph_rgba(Glyph::Stop), ICON_SIZE, ICON_SIZE),
                hicon_from_rgba(&glyph_rgba(Glyph::Test), ICON_SIZE, ICON_SIZE),
            ];
            let mut taskbar = Self {
                hwnd,
                list,
                subclassed,
                buttons_added: false,
                last_progress: None,
                last_states: None,
                icons,
            };
            taskbar.add_buttons(ButtonStates {
                start_all: true,
                stop_all: false,
                test_all: false,
            });
            taskbar
        }

        fn headless() -> Self {
            Self {
                hwnd: HWND(std::ptr::null_mut()),
                list: None,
                subclassed: false,
                buttons_added: false,
                last_progress: None,
                last_states: None,
                icons: [None, None, None],
            }
        }

        fn buttons(&self, states: ButtonStates) -> Vec<THUMBBUTTON> {
            THUMB_BUTTONS
                .iter()
                .zip(self.icons.iter())
                .map(|((id, action), icon)| THUMBBUTTON {
                    dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
                    iId: *id,
                    iBitmap: 0,
                    hIcon: (*icon).unwrap_or(HICON(std::ptr::null_mut())),
                    szTip: tip(&thumb_label(*action)),
                    dwFlags: flags(states.is_enabled(*action)),
                })
                .collect()
        }

        fn add_buttons(&mut self, states: ButtonStates) {
            let Some(list) = self.list.as_ref() else {
                return;
            };
            let buttons = self.buttons(states);
            match unsafe { list.ThumbBarAddButtons(self.hwnd, &buttons) } {
                Ok(()) => {
                    self.buttons_added = true;
                    self.last_states = Some(states);
                }
                Err(error) => {
                    // Normal before Explorer sends TaskbarButtonCreated; the
                    // pump retries when that message arrives.
                    self.buttons_added = false;
                    eprintln!("taskbar: thumbnail buttons not ready ({error})");
                }
            }
        }

        pub fn taskbar_button_created(&mut self, states: ButtonStates) {
            self.buttons_added = false;
            self.add_buttons(states);
            // Explorer may have re-created the button after a crash; the
            // progress overlay did not survive that, so re-send it.
            let progress = self.last_progress;
            self.last_progress = None;
            self.set_progress(progress);
        }

        pub fn sync_buttons(&mut self, states: ButtonStates) {
            if !self.buttons_added {
                return;
            }
            if self.last_states == Some(states) {
                return;
            }
            let Some(list) = self.list.as_ref() else {
                return;
            };
            let buttons: Vec<THUMBBUTTON> = self
                .buttons(states)
                .into_iter()
                .map(|button| THUMBBUTTON {
                    dwMask: THB_FLAGS,
                    dwFlags: button.dwFlags,
                    ..button
                })
                .collect();
            match unsafe { list.ThumbBarUpdateButtons(self.hwnd, &buttons) } {
                Ok(()) => self.last_states = Some(states),
                Err(error) => {
                    self.buttons_added = false;
                    eprintln!("taskbar: thumbnail button update failed ({error})");
                }
            }
        }

        pub fn set_progress(&mut self, progress: Option<Progress>) {
            let Some(list) = self.list.as_ref() else {
                return;
            };
            if self.last_progress == progress {
                return;
            }
            let flag = match progress_state(progress) {
                ProgressState::Hidden => TBPF_NOPROGRESS,
                ProgressState::Normal => TBPF_NORMAL,
                ProgressState::Error => TBPF_ERROR,
            };
            if let Err(error) = unsafe { list.SetProgressState(self.hwnd, flag) } {
                eprintln!("taskbar: progress state unavailable ({error})");
                return;
            }
            if let Some(progress) = progress {
                if let Err(error) =
                    unsafe { list.SetProgressValue(self.hwnd, progress.completed, progress.total) }
                {
                    eprintln!("taskbar: progress value unavailable ({error})");
                    return;
                }
            }
            self.last_progress = progress;
        }
    }

    impl Drop for Taskbar {
        fn drop(&mut self) {
            unsafe {
                if self.subclassed {
                    let _ = windows::Win32::UI::Shell::RemoveWindowSubclass(
                        self.hwnd,
                        Some(subclass_proc),
                        SUBCLASS_ID,
                    );
                }
                for icon in self.icons.iter().flatten() {
                    let _ = DestroyIcon(*icon);
                }
            }
        }
    }
}

#[cfg(windows)]
pub use win::{drain_events, Taskbar};
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_ids_round_trip_like_tray_menu_ids() {
        for (id, action) in THUMB_BUTTONS {
            assert_eq!(action_for_button_id(id), Some(action));
            assert_eq!(button_id_for_action(action), id);
        }
        assert_eq!(action_for_button_id(99), None);
    }

    #[test]
    fn thumb_states_follow_engine_state() {
        let idle = button_states(0, 3, false, false);
        assert!(idle.start_all);
        assert!(!idle.stop_all);
        assert!(!idle.test_all);

        let partial = button_states(1, 3, false, false);
        assert!(partial.start_all, "some rows still stopped");
        assert!(partial.stop_all);
        assert!(partial.test_all);

        let all = button_states(3, 3, false, false);
        assert!(!all.start_all, "nothing left to start");
        assert!(all.stop_all);
        assert!(all.test_all);

        let starting = button_states(0, 3, true, false);
        assert!(!starting.start_all, "start already in flight");

        let testing = button_states(2, 3, false, true);
        assert!(!testing.test_all, "a probe is already in flight");

        let empty = button_states(0, 0, false, false);
        assert!(!empty.start_all, "no rows to start");
    }

    #[test]
    fn progress_maps_done_total_and_failures() {
        assert_eq!(
            Progress::from_counts(3, 4, 0),
            Some(Progress {
                completed: 3,
                total: 4,
                failed: 0
            })
        );
        assert_eq!(
            Progress::from_counts(9, 4, 1),
            Some(Progress {
                completed: 4,
                total: 4,
                failed: 1
            }),
            "done never exceeds total"
        );
        assert_eq!(Progress::from_counts(0, 0, 0), None);
        assert_eq!(progress_state(None), ProgressState::Hidden);
        assert_eq!(
            progress_state(Progress::from_counts(1, 2, 0)),
            ProgressState::Normal
        );
        assert_eq!(
            progress_state(Progress::from_counts(1, 2, 1)),
            ProgressState::Error
        );
        assert_eq!(
            progress_state(Progress::from_counts(0, 0, 0)),
            ProgressState::Hidden
        );
    }

    #[test]
    fn every_glyph_is_visible_and_outlined() {
        #[cfg(windows)]
        for glyph in [win::Glyph::Start, win::Glyph::Stop, win::Glyph::Test] {
            let rgba = win::glyph_rgba(glyph);
            assert_eq!(rgba.len(), 32 * 32 * 4);
            let (pixels, _) = rgba.as_chunks::<4>();
            let opaque = pixels.iter().filter(|px| px[3] == 255).count();
            assert!(opaque > 100, "glyph covers enough pixels");
            let dark = pixels
                .iter()
                .filter(|px| px[3] == 255 && px[0] < 60)
                .count();
            let light = pixels
                .iter()
                .filter(|px| px[3] == 255 && px[0] > 200)
                .count();
            assert!(dark > 0, "outline is dark");
            assert!(light > 0, "fill is light");
        }
    }

    #[test]
    fn headless_install_is_a_no_op() {
        let mut taskbar = Taskbar::install(0);
        taskbar.set_progress(Progress::from_counts(1, 2, 0));
        taskbar.sync_buttons(button_states(1, 2, false, false));
        taskbar.taskbar_button_created(button_states(1, 2, false, false));
        let _ = drain_events();
    }
}
