//! Windows shell integration. Everything that has to speak Win32 directly
//! lives here so the rest of the UI never imports the `windows` crate and a
//! shell/com failure can stay a logged no-op instead of a panic.

mod icon;
pub mod notify;
pub mod scheme;
pub mod taskbar;
