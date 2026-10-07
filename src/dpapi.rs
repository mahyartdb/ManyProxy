//! Ticket 35: Windows DPAPI (user scope) for secrets at rest.
//!
//! Hand-rolled `crypt32.dll` FFI — two calls, no `windows`-crate weight.
//! `protect`/`unprotect` are bytes in/bytes out; the `dpapi1:` storage
//! marker lives in [`seal_params`]/[`unseal_params`]. Errors name the
//! operation only (never secret bytes, never lengths that leak shape).

#[cfg(windows)]
use std::ffi::c_void;

#[cfg(windows)]
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
/// Storage marker: `dpapi1:<base64(ciphertext)>`.
pub const MARKER: &str = "dpapi1:";

#[cfg(windows)]
#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[cfg(windows)]
#[repr(C)]
struct ProtectPrompt {
    cb_size: u32,
    dw_prompt_flags: u32,
    hwnd_app: isize,
    sz_prompt: *const u16,
}

#[cfg(windows)]
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        p_data_in: *const DataBlob,
        sz_data_descr: *const u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *mut c_void,
        p_prompt_struct: *const ProtectPrompt,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;
    fn CryptUnprotectData(
        p_data_in: *const DataBlob,
        ppsz_data_descr: *mut *mut u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *mut c_void,
        p_prompt_struct: *const ProtectPrompt,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
    fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
}

/// Encrypt `plaintext` for the current Windows user. Fails loudly when
/// DPAPI is unavailable (never silently returns plaintext).
/// Ticket 48: off Windows this is a loud stub — secrets at rest need
/// Windows DPAPI, so the app is documented Windows-only (see README).
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    if fail_for_test() {
        return Err("dpapi protect failed (injected for test)".to_string());
    }
    protect_platform(plaintext)
}

#[cfg(windows)]
fn protect_platform(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    // SAFETY: in-blob borrows the caller's slice for the duration of the
    // call; all pointers are null except the input; UI is forbidden so the
    // call never blocks on a dialog. The out-blob is copied then freed.
    unsafe {
        let input = DataBlob {
            cb_data: plaintext.len() as u32,
            pb_data: plaintext.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            return Err(format!("dpapi protect failed (win32 {})", GetLastError()));
        }
        let bytes = std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
        LocalFree(output.pb_data as *mut c_void);
        Ok(bytes)
    }
}

/// Ticket 48: non-Windows loud stub (documented Windows-only). Never
/// returns plaintext, never succeeds without the OS key.
#[cfg(not(windows))]
fn protect_platform(_plaintext: &[u8]) -> Result<Vec<u8>, String> {
    Err(
        "dpapi protect unavailable: secrets at rest need Windows DPAPI (Windows-only build)"
            .to_string(),
    )
}

/// Decrypt a blob from [`protect`]. Tampered or foreign blobs fail loudly.
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    unprotect_platform(blob)
}

#[cfg(windows)]
fn unprotect_platform(blob: &[u8]) -> Result<Vec<u8>, String> {
    // SAFETY: same contract as [`protect`].
    unsafe {
        let input = DataBlob {
            cb_data: blob.len() as u32,
            pb_data: blob.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let mut _descr: *mut u16 = std::ptr::null_mut();
        let ok = CryptUnprotectData(
            &input,
            &mut _descr,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            return Err(format!("dpapi unprotect failed (win32 {})", GetLastError()));
        }
        if !_descr.is_null() {
            LocalFree(_descr as *mut c_void);
        }
        let bytes = std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
        LocalFree(output.pb_data as *mut c_void);
        Ok(bytes)
    }
}

/// Ticket 48: non-Windows loud stub (documented Windows-only).
#[cfg(not(windows))]
fn unprotect_platform(_blob: &[u8]) -> Result<Vec<u8>, String> {
    Err(
        "dpapi unprotect unavailable: secrets at rest need Windows DPAPI (Windows-only build)"
            .to_string(),
    )
}

/// Seal a params string for storage: `dpapi1:<base64>`.
pub fn seal_params(plaintext: &str) -> Result<String, String> {
    if fail_for_test() {
        return Err("dpapi protect failed (injected for test)".to_string());
    }
    use base64::Engine as _;
    protect(plaintext.as_bytes()).map(|bytes| {
        format!(
            "{MARKER}{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    })
}

/// Open a stored params string. Sealed values decrypt; legacy plaintext
/// passes through (migration window only — migration seals everything).
/// Unknown markers fail loudly (never guessed).
pub fn unseal_params(stored: &str) -> Result<String, String> {
    use base64::Engine as _;
    // Trim both sides consistently: detection and decoding agree whether
    // the caller pasted leading whitespace, trailing newlines, or neither.
    let trimmed = stored.trim();
    if let Some(rest) = trimmed.strip_prefix(MARKER) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(rest.trim())
            .map_err(|_| "dpapi params are not valid base64".to_string())?;
        let plain = unprotect(&bytes)?;
        return String::from_utf8(plain)
            .map_err(|_| "dpapi params are not valid utf-8".to_string());
    }
    // Legacy plaintext passes through ONLY as a JSON object: `"{garbage"`
    // (or any other non-JSON) fails loudly instead of smuggling corrupt
    // params into the library as a trusted row.
    if trimmed.starts_with('{') {
        let is_object = serde_json::from_str::<serde_json::Value>(trimmed)
            .map(|value| value.is_object())
            .unwrap_or(false);
        if is_object {
            return Ok(stored.to_string());
        }
        return Err("legacy params are not valid JSON (refusing to guess)".to_string());
    }
    Err("unknown params storage marker (refusing to guess)".to_string())
}

thread_local! {
    static FAIL_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test seam: force [`protect`]/[`seal_params`] to fail loudly on this thread.
#[allow(dead_code)]
pub fn set_fail_for_test(fail: bool) {
    FAIL_FOR_TEST.with(|flag| flag.set(fail));
}

fn fail_for_test() -> bool {
    FAIL_FOR_TEST.with(|flag| flag.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reset the fail seam on drop so a panicking assertion never poisons
    /// a later test sharing this thread.
    struct FailGuard;

    impl FailGuard {
        fn on() -> Self {
            set_fail_for_test(true);
            Self
        }
    }

    impl Drop for FailGuard {
        fn drop(&mut self) {
            set_fail_for_test(false);
        }
    }

    /// Ticket 48 (US-60): bytes round-trip, including empty and unicode.
    #[cfg(windows)]
    #[test]
    fn t48_protect_roundtrip() {
        for plaintext in [
            b"".as_slice(),
            b"{}".as_slice(),
            b"{\"password\":\"hunter2\"}".as_slice(),
            "ünïcodé-☃".as_bytes(),
        ] {
            let blob = protect(plaintext).expect("protect works");
            assert!(!blob.is_empty() || plaintext.is_empty());
            assert_ne!(blob, plaintext, "ciphertext differs from plaintext");
            assert_eq!(unprotect(&blob).expect("unprotect works"), plaintext);
        }
    }

    /// Ticket 48 (US-60): sealed storage round-trips with the marker shape.
    #[cfg(windows)]
    #[test]
    fn t48_seal_roundtrip() {
        let sealed = seal_params("{\"a\":1}").expect("seal works");
        assert!(sealed.starts_with(MARKER), "marker shape, got {sealed}");
        assert_eq!(unseal_params(&sealed).expect("unseal works"), "{\"a\":1}");
    }

    /// Ticket 48 (US-60): rejection table — every bad shape fails loudly
    /// with a named cause, never silent plaintext, never a guess.
    #[cfg(windows)]
    #[test]
    fn t48_unseal_rejection_table() {
        // Tampered blob: still valid base64, but DPAPI integrity fails.
        // The flip lands near the tail inside the encrypted payload:
        // DPAPI MACs the payload, not the header, so a header flip (or
        // a last-char flip that only appends a tolerated zero byte when
        // it replaces padding) still decrypts.
        let sealed = seal_params("{\"a\":1}").expect("seal works");
        let mut tampered = sealed.clone();
        let at = tampered.len() - 8;
        tampered.replace_range(
            at..at + 1,
            if &tampered[at..at + 1] == "A" {
                "B"
            } else {
                "A"
            },
        );
        assert!(
            unseal_params(&tampered).is_err(),
            "tampered blob fails loudly"
        );
        // Random bytes are no DPAPI blob either (foreign-blob class).
        {
            use base64::Engine as _;
            let foreign = format!(
                "{MARKER}{}",
                base64::engine::general_purpose::STANDARD.encode([0xabu8; 64])
            );
            assert!(
                unseal_params(&foreign).is_err(),
                "foreign blob fails loudly"
            );
        }
        // NOTE: the non-UTF8 arm needs DPAPI itself to decrypt to
        // non-UTF8 bytes, which no same-profile ciphertext can produce;
        // the foreign-blob arms above pin loud failure for that class.
        for (stored, needle) in [
            (format!("{MARKER}!!!"), "base64"),
            (MARKER.to_string(), "dpapi"),
            ("v2:abc".to_string(), "unknown params storage marker"),
            ("plaintext".to_string(), "unknown params storage marker"),
            ("".to_string(), "unknown params storage marker"),
        ] {
            let error = unseal_params(&stored).expect_err(&format!("{stored:?} must fail"));
            assert!(
                error.contains(needle),
                "named cause ({needle}), got {error}"
            );
        }
        // Legacy migration-window plaintext passes through verbatim.
        for legacy in ["{\"a\":1}", "   {\"a\":1}  "] {
            assert_eq!(
                unseal_params(legacy).expect("legacy passes through"),
                legacy
            );
        }
    }

    /// Ticket 48 (US-60): the injected DPAPI outage fails writes loudly.
    #[test]
    fn t48_fail_seam_aborts_writes_loudly() {
        let _guard = FailGuard::on();
        for error in [
            protect(b"x").expect_err("protect aborts"),
            seal_params("{\"a\":1}").expect_err("seal aborts"),
        ] {
            assert!(error.contains("dpapi"), "loud dpapi cause, got {error}");
        }
    }

    /// Sealed values tolerate trailing whitespace (copy/paste with a
    /// newline still opens to the same params).
    #[cfg(windows)]
    #[test]
    fn unseal_tolerates_trailing_whitespace() {
        let sealed = seal_params("{\"a\":1}").expect("seal works");
        assert!(sealed.starts_with(MARKER));
        for padded in [
            format!("{sealed} "),
            format!("{sealed}\n"),
            format!("{sealed}  \r\n"),
        ] {
            assert_eq!(
                unseal_params(&padded).expect("trailing whitespace tolerated"),
                "{\"a\":1}"
            );
        }
    }

    /// An empty blob is no DPAPI ciphertext: it fails loudly, never
    /// silent plaintext, never an empty success.
    #[test]
    fn unprotect_empty_blob_fails_loud() {
        let error = unprotect(&[]).expect_err("empty blob must fail");
        assert!(!error.is_empty(), "loud failure");
        assert!(
            error.contains("dpapi") || error.contains("Windows"),
            "loud dpapi cause, got {error}"
        );
    }

    /// Brace-led but not a JSON object (`"{garbage"`) is refused, never
    /// passed through as legacy plaintext; sealed detection trims both
    /// sides like the trailing-whitespace arm.
    #[test]
    fn unseal_rejects_brace_led_non_json() {
        for bad in ["{garbage", "{", "[1,2]", "\"str\"", "   {oops"] {
            let error = unseal_params(bad).expect_err(&format!("{bad:?} must fail"));
            assert!(
                error.contains("JSON") || error.contains("unknown params storage marker"),
                "named cause, got {error}"
            );
        }
        assert_eq!(
            unseal_params("  {\"a\":1}").expect("legacy object passes"),
            "  {\"a\":1}"
        );
    }

    /// Ticket 48 (US-66): off Windows every DPAPI op is a loud stub —
    /// never silent plaintext, never a success without the OS key. Legacy
    /// plaintext still passes through (there is nothing to seal with).
    #[cfg(not(windows))]
    #[test]
    fn t48_non_windows_dpapi_is_a_loud_stub() {
        for error in [
            protect(b"x").expect_err("protect is a stub"),
            seal_params("{\"a\":1}").expect_err("seal is a stub"),
            unprotect(b"blob").expect_err("unprotect is a stub"),
            unseal_params(&format!("{MARKER}QUJD")).expect_err("sealed open is a stub"),
        ] {
            assert!(error.contains("Windows"), "Windows-only cause, got {error}");
        }
        assert_eq!(
            unseal_params("{\"a\":1}").expect("legacy passes through"),
            "{\"a\":1}"
        );
        assert!(
            unseal_params("v2:abc").is_err(),
            "unknown markers still refused"
        );
    }
}
