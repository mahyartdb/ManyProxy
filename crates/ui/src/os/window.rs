//! Windows 11 frame corners.
//!
//! `DwmSetWindowAttribute` with `DWMWA_WINDOW_CORNER_PREFERENCE` is best-effort:
//! Windows 10 does not know the attribute, and a remote or headless session can
//! refuse it. Every failure is a logged no-op; the window keeps its stock
//! corners and the rest of the UI is unaffected.

use crate::store::WindowCorners;

pub fn apply_corner_preference(window: &gpui_kit::Window, corners: WindowCorners) {
    #[cfg(windows)]
    {
        use std::ffi::c_void;

        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT,
            DWMWCP_DONOTROUND, DWMWCP_ROUND, DWMWCP_ROUNDSMALL, DWM_WINDOW_CORNER_PREFERENCE,
        };

        let Some(hwnd) = crate::tray::hwnd_from_window(window) else {
            return;
        };
        let preference: DWM_WINDOW_CORNER_PREFERENCE = match corners {
            WindowCorners::Auto => DWMWCP_DEFAULT,
            WindowCorners::Rounded => DWMWCP_ROUND,
            WindowCorners::Small => DWMWCP_ROUNDSMALL,
            WindowCorners::Square => DWMWCP_DONOTROUND,
        };
        let result = unsafe {
            DwmSetWindowAttribute(
                HWND(hwnd as *mut c_void),
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            )
        };
        if let Err(error) = result {
            eprintln!("window: corner preference unavailable ({error})");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, corners);
    }
}
