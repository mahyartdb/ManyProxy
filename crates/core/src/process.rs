//! Console-less child processes: the engine, health tools and DPAPI helpers all
//! spawn CLIs whose windows must never flash.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Spawn a console executable with no visible console window. `creation_flags`
/// REPLACES the whole flag word (std does `self.flags = flags`), so a future
/// site that needs other flags must OR them in:
/// `cmd.creation_flags(cmd.get_creation_flags() | CREATE_NO_WINDOW)`.
pub fn spawn_hidden(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}
