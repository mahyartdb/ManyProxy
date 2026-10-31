//! A secret's plaintext, in memory and at rest.
//!
//! What this protects: the plaintext buffer is zeroed when the secret is
//! dropped, so a freed heap page does not keep handing out subscription tokens
//! or proxy passwords; and `Debug`/`Display` print `***`, so a log line, a
//! panic, a toast or a failing assertion cannot carry the value by accident.
//! Sealing at rest is DPAPI, scoped to the Windows user account — a database
//! file copied to another account or machine does not open.
//!
//! What this does NOT protect: a memory dump, a debugger attached to the
//! process, or another process running as the same user (DPAPI's own boundary
//! is that account). Nor the explicit export: the Export action writes
//! plaintext because the user asked for a portable file, and the UI says so
//! before it writes.

use std::fmt;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

use crate::dpapi;

/// The stored form's prefix. `dpapi1:` is what every existing database row
/// carries; the sealing here keeps that format byte for byte, so an upgrade
/// opens old rows and a downgrade still reads new ones.
pub const MARKER: &str = "dpapi1:";

/// A right-looking secret that could not be sealed or opened. The message is
/// DPAPI's own (a Win32 error code or a storage-shape complaint); it never
/// carries the plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// DPAPI refused to protect the plaintext.
    Seal(String),
    /// A stored blob could not be opened.
    Open(String),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretError::Seal(message) | SecretError::Open(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SecretError {}

/// A secret value: redacted when printed, zeroed when dropped.
///
/// `Deref` keeps ordinary string code (parse, search, compare) working without
/// naming the value; anything that has to reach a place where the plaintext
/// genuinely lives — a sealed column, an engine config file, an HTTP header —
/// asks for it with [`Secret::expose`] and reads like it means it.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Secret(String);

impl Secret {
    pub fn new(plaintext: impl Into<String>) -> Self {
        Secret(plaintext.into())
    }

    /// The plaintext, for the few call sites that must hand it on.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn expose_mut(&mut self) -> &mut String {
        &mut self.0
    }

    /// Hands the plaintext back to the caller without the copy a `Clone`
    /// would leave in the allocator's hands.
    pub fn into_plaintext(mut self) -> String {
        std::mem::take(&mut self.0)
    }

    /// Seals for storage: `dpapi1:` followed by the base64 DPAPI blob.
    pub fn seal(&self) -> Result<String, SecretError> {
        use base64::Engine as _;
        let blob = dpapi::protect(self.0.as_bytes()).map_err(SecretError::Seal)?;
        Ok(format!(
            "{MARKER}{}",
            base64::engine::general_purpose::STANDARD.encode(blob)
        ))
    }

    /// Opens a stored value. A row written before sealing existed is a plain
    /// JSON object and passes through untouched; anything else has to be a
    /// readable sealed blob, because guessing is how a broken database turns
    /// into a silently empty one.
    pub fn open(stored: &str) -> Result<Secret, SecretError> {
        use base64::Engine as _;

        let trimmed = stored.trim();
        if let Some(rest) = trimmed.strip_prefix(MARKER) {
            let mut blob = base64::engine::general_purpose::STANDARD
                .decode(rest.trim())
                .map_err(|_| SecretError::Open("dpapi params are not valid base64".to_string()))?;
            let mut plain = dpapi::unprotect(&blob).map_err(SecretError::Open)?;
            blob.zeroize();
            let text = match String::from_utf8(std::mem::take(&mut plain)) {
                Ok(text) => text,
                Err(error) => {
                    // The rejected bytes are still the plaintext; wipe them
                    // before the error escapes.
                    let mut bytes = error.into_bytes();
                    bytes.zeroize();
                    return Err(SecretError::Open(
                        "dpapi params are not valid utf-8".to_string(),
                    ));
                }
            };
            return Ok(Secret(text));
        }

        if trimmed.starts_with('{') {
            let is_object = serde_json::from_str::<serde_json::Value>(trimmed)
                .map(|value| value.is_object())
                .unwrap_or(false);
            if is_object {
                return Ok(Secret(stored.to_string()));
            }
            return Err(SecretError::Open(
                "legacy params are not valid JSON (refusing to guess)".to_string(),
            ));
        }
        Err(SecretError::Open(
            "unknown params storage marker (refusing to guess)".to_string(),
        ))
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl Deref for Secret {
    type Target = String;

    fn deref(&self) -> &String {
        &self.0
    }
}

impl DerefMut for Secret {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Secret(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Secret(value.to_string())
    }
}

impl AsRef<str> for Secret {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for Secret {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Secret {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Secret> for &str {
    fn eq(&self, other: &Secret) -> bool {
        *self == other.0
    }
}

impl PartialEq<String> for Secret {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

/// A `Secret` serializes as its plaintext string, which is what keeps the
/// export JSON byte-identical; the export path is the one place that plaintext
/// is the point.
impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Secret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_are_redacted() {
        let secret = Secret::new("hunter2");
        assert_eq!(format!("{secret}"), "***");
        assert_eq!(format!("{secret:?}"), "***");
        let outer = format!("{:?}", (Some(secret.clone()), 1));
        assert!(!outer.contains("hunter2"), "no plaintext in {outer}");
        assert!(outer.contains("***"), "redaction is visible: {outer}");
    }

    #[test]
    fn ordinary_string_work_still_reads_as_a_string() {
        let secret = Secret::new(r#"{"password":"hunter2"}"#);
        assert!(secret.contains("hunter2"));
        assert_eq!(secret.len(), 22);
        assert_eq!(secret.expose(), r#"{"password":"hunter2"}"#);
        assert!(secret.starts_with('{'));
        assert_eq!(secret, r#"{"password":"hunter2"}"#);
        assert_eq!(secret, r#"{"password":"hunter2"}"#.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&secret).expect("deref to &str");
        assert_eq!(parsed["password"], "hunter2");
    }

    #[test]
    fn serde_round_trip_is_the_plaintext_string() {
        let secret = Secret::new(r#"{"a":1}"#);
        assert_eq!(
            serde_json::to_string(&secret).expect("serializes"),
            r#""{\"a\":1}""#
        );
        let back: Secret = serde_json::from_str(r#""{\"a\":1}""#).expect("deserializes");
        assert_eq!(back.expose(), r#"{"a":1}"#);
    }

    #[cfg(windows)]
    #[test]
    fn seal_round_trip_keeps_the_stored_marker() {
        let sealed = Secret::new(r#"{"a":1}"#).seal().expect("seal works");
        assert!(sealed.starts_with(MARKER), "marker shape, got {sealed}");
        assert!(!sealed.contains(r#"{"a":1}"#), "ciphertext only");
        let opened = Secret::open(&sealed).expect("opens");
        assert_eq!(opened.expose(), r#"{"a":1}"#);
    }

    #[cfg(windows)]
    #[test]
    fn a_flipped_blob_never_opens_to_a_different_plaintext() {
        use base64::Engine as _;
        let plaintext = r#"{"password":"hunter2"}"#;
        let sealed = Secret::new(plaintext).seal().expect("seal works");
        let mut blob = base64::engine::general_purpose::STANDARD
            .decode(&sealed[MARKER.len()..])
            .expect("sealed payload is base64");
        // Flipping base64 characters would not prove this: two characters can
        // decode to the same bytes. Flip ciphertext bytes instead.
        for at in 0..blob.len() {
            let original = blob[at];
            blob[at] ^= 0x01;
            let tampered = format!(
                "{MARKER}{}",
                base64::engine::general_purpose::STANDARD.encode(&blob)
            );
            let opened = Secret::open(&tampered);
            blob[at] = original;
            match opened {
                Err(_) => {}
                Ok(secret) => assert_eq!(
                    secret.expose(),
                    plaintext,
                    "byte {at} flipped opened to different bytes: corruption would go unnoticed"
                ),
            }
        }
        // DPAPI's opaque header has bytes it does not authenticate (they cannot
        // change the plaintext, as the loop above proves); the payload it does.
        for at in blob.len() - 32..blob.len() {
            let original = blob[at];
            blob[at] ^= 0x01;
            let tampered = format!(
                "{MARKER}{}",
                base64::engine::general_purpose::STANDARD.encode(&blob)
            );
            let opened = Secret::open(&tampered);
            blob[at] = original;
            assert!(
                opened.is_err(),
                "payload byte {at} flipped still opened a secret"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn sealed_blob_is_not_readable_by_a_foreign_shape() {
        let sealed = Secret::new("hunter2").seal().expect("seal works");
        let truncated = &sealed[..sealed.len() - 4];
        assert!(Secret::open(truncated).is_err(), "truncated blob refuses");
    }

    #[test]
    fn storage_rejection_table_names_the_cause() {
        for (stored, needle) in [
            (format!("{MARKER}!!!"), "base64"),
            (MARKER.to_string(), "dpapi"),
            ("v2:abc".to_string(), "unknown params storage marker"),
            ("plaintext".to_string(), "unknown params storage marker"),
            ("".to_string(), "unknown params storage marker"),
            ("{garbage".to_string(), "JSON"),
            ("[1,2]".to_string(), "unknown params storage marker"),
        ] {
            let error = Secret::open(&stored).expect_err(&format!("{stored:?} must fail"));
            assert!(
                error.to_string().contains(needle),
                "named cause ({needle}), got {error}"
            );
        }
    }

    #[test]
    fn legacy_plaintext_passes_through_byte_for_byte() {
        for legacy in ["{\"a\":1}", "   {\"a\":1}  "] {
            assert_eq!(
                Secret::open(legacy)
                    .expect("legacy passes through")
                    .expose(),
                legacy
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn stored_blob_tolerates_trailing_whitespace() {
        let sealed = Secret::new(r#"{"a":1}"#).seal().expect("seal works");
        for padded in [
            format!("{sealed} "),
            format!("{sealed}\n"),
            format!(
                "{sealed}  
"
            ),
        ] {
            assert_eq!(
                Secret::open(&padded)
                    .expect("whitespace tolerated")
                    .expose(),
                r#"{"a":1}"#
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn seal_refuses_when_dpapi_refuses() {
        crate::dpapi::set_fail_for_test(true);
        let error = Secret::new("hunter2").seal().expect_err("seal aborts");
        crate::dpapi::set_fail_for_test(false);
        assert!(error.to_string().contains("dpapi"), "loud cause: {error}");
        assert!(!format!("{error:?}").contains("hunter2"));
    }

    #[cfg(not(windows))]
    #[test]
    fn sealing_is_windows_only_and_says_so() {
        let error = Secret::new("hunter2").seal().expect_err("seal is a stub");
        assert!(error.to_string().contains("Windows"), "cause: {error}");
        let error = Secret::open(&format!("{MARKER}QUJD")).expect_err("open is a stub");
        assert!(error.to_string().contains("Windows"), "cause: {error}");
    }

    #[test]
    fn error_messages_never_carry_the_plaintext() {
        let error = Secret::open(&format!("{MARKER}not-base64")).expect_err("must fail");
        assert!(!error.to_string().contains("hunter2"));
        assert!(!format!("{error:?}").contains(MARKER));
    }
}
