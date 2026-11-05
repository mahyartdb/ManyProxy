//! The `manyproxy://` protocol handler: one HKCU registration under
//! `Software\Classes\manyproxy` pointing at this exe.
//!
//! Windows has no per-user API for this, so the registry is the only record —
//! and the only source of truth for whether the toggle is on. That keeps the
//! setting idempotent: the boot check repairs a stale exe path, and turning the
//! toggle off removes the key tree instead of writing a second flag.

use std::path::Path;

pub const SCHEME_KEY: &str = r"Software\Classes\manyproxy";
pub const COMMAND_KEY: &str = r"Software\Classes\manyproxy\shell\open\command";
pub const URL_PROTOCOL_VALUE: &str = "URL Protocol";
pub const PROTOCOL_DISPLAY_NAME: &str = "URL:ManyProxy Protocol";

#[derive(Debug)]
pub enum SchemeError {
    CurrentExe(String),
    Registry(String),
    Verify,
}

impl std::fmt::Display for SchemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemeError::CurrentExe(error) => {
                write!(f, "cannot resolve the app path ({error})")
            }
            SchemeError::Registry(error) => write!(f, "registry access failed ({error})"),
            SchemeError::Verify => write!(f, "the registration did not stick"),
        }
    }
}

impl std::error::Error for SchemeError {}

/// The command Windows runs for a link. The quoted exe path survives spaces,
/// and `%1` receives the whole URL as one argument for the existing argv intake.
pub fn command_for_exe(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

#[cfg(windows)]
mod registry {
    use super::{
        command_for_exe, SchemeError, COMMAND_KEY, PROTOCOL_DISPLAY_NAME, SCHEME_KEY,
        URL_PROTOCOL_VALUE,
    };
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    fn root() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    fn registry_error(context: &str, error: std::io::Error) -> SchemeError {
        SchemeError::Registry(format!("{context} ({error})"))
    }

    fn current_command() -> Option<String> {
        std::env::current_exe()
            .ok()
            .map(|exe| command_for_exe(&exe))
    }

    fn registered_command() -> Option<String> {
        root()
            .open_subkey_with_flags(COMMAND_KEY, KEY_READ)
            .ok()?
            .get_value::<String, _>("")
            .ok()
    }

    fn scheme_key_exists() -> Result<bool, SchemeError> {
        match root().open_subkey_with_flags(SCHEME_KEY, KEY_READ) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(registry_error("cannot read the link handler key", error)),
        }
    }

    pub fn is_registered() -> bool {
        match (registered_command(), current_command()) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        }
    }

    pub fn register() -> Result<(), SchemeError> {
        let exe =
            std::env::current_exe().map_err(|error| SchemeError::CurrentExe(error.to_string()))?;
        let command = command_for_exe(&exe);

        let (scheme, _) = root()
            .create_subkey(SCHEME_KEY)
            .map_err(|error| registry_error("cannot open the link handler key", error))?;
        scheme
            .set_value("", &PROTOCOL_DISPLAY_NAME.to_string())
            .map_err(|error| registry_error("cannot write the protocol name", error))?;
        scheme
            .set_value(URL_PROTOCOL_VALUE, &String::new())
            .map_err(|error| registry_error("cannot mark the protocol", error))?;

        let (open, _) = root()
            .create_subkey(COMMAND_KEY)
            .map_err(|error| registry_error("cannot open the link command key", error))?;
        open.set_value("", &command)
            .map_err(|error| registry_error("cannot write the link command", error))?;

        if registered_command().as_deref() != Some(command.as_str()) {
            return Err(SchemeError::Verify);
        }
        Ok(())
    }

    pub fn unregister() -> Result<(), SchemeError> {
        let shared = root();
        match shared.delete_subkey_all(SCHEME_KEY) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(registry_error("cannot remove the link handler key", error)),
        }
    }

    /// Called once at startup. An absent key means the user never opted in, so
    /// nothing is written; an existing key with an old exe path is repaired.
    /// Returns whether a repair happened.
    pub fn repair_if_registered() -> Result<bool, SchemeError> {
        if !scheme_key_exists()? || is_registered() {
            return Ok(false);
        }
        register()?;
        Ok(true)
    }
}

#[cfg(not(windows))]
mod registry {
    use super::SchemeError;

    pub fn is_registered() -> bool {
        false
    }

    pub fn register() -> Result<(), SchemeError> {
        Err(SchemeError::Registry(
            "the link handler is Windows-only".to_string(),
        ))
    }

    pub fn unregister() -> Result<(), SchemeError> {
        Err(SchemeError::Registry(
            "the link handler is Windows-only".to_string(),
        ))
    }

    pub fn repair_if_registered() -> Result<bool, SchemeError> {
        Ok(false)
    }
}

pub use registry::{is_registered, register, repair_if_registered, unregister};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_the_exe_and_passes_one_url() {
        assert_eq!(
            command_for_exe(Path::new(r"C:\Program Files\ManyProxy\manyproxy.exe")),
            r#""C:\Program Files\ManyProxy\manyproxy.exe" "%1""#
        );
    }
}
