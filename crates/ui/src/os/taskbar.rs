//! Taskbar progress for probe sweeps.
//!
//! `ITaskbarList3` is a best-effort shell feature: Explorer can restart, a
//! remote session has no taskbar, and the object can be unavailable for
//! reasons the app cannot fix. Every call therefore degrades to a logged
//! no-op; the UI keeps working with no taskbar at all.

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
}

#[cfg(windows)]
mod win {
    use super::{progress_state, Progress, ProgressState};

    use std::ffi::c_void;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        ITaskbarList3, TaskbarList, TBPF_ERROR, TBPF_NOPROGRESS, TBPF_NORMAL,
    };

    pub struct Taskbar {
        hwnd: HWND,
        list: Option<ITaskbarList3>,
        last_progress: Option<Progress>,
    }

    impl Taskbar {
        pub fn install(hwnd: isize) -> Self {
            let hwnd = HWND(hwnd as *mut c_void);
            if hwnd.0.is_null() {
                return Self {
                    hwnd,
                    list: None,
                    last_progress: None,
                };
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

            Self {
                hwnd,
                list,
                last_progress: None,
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
}

#[cfg(windows)]
pub use win::Taskbar;

#[cfg(test)]
mod tests {
    use super::*;

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
            Progress::from_counts(9, 4, 6),
            Some(Progress {
                completed: 4,
                total: 4,
                failed: 4
            }),
            "counters never exceed total"
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
    fn headless_install_is_a_no_op() {
        let mut taskbar = Taskbar::install(0);
        taskbar.set_progress(Progress::from_counts(1, 2, 0));
        taskbar.set_progress(None);
    }
}
