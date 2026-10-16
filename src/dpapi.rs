#[cfg(windows)]
use std::ffi::c_void;

#[cfg(windows)]
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

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

pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    if fail_for_test() {
        return Err("dpapi protect failed (injected for test)".to_string());
    }
    protect_platform(plaintext)
}

#[cfg(windows)]
fn protect_platform(plaintext: &[u8]) -> Result<Vec<u8>, String> {
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

#[cfg(not(windows))]
fn protect_platform(_plaintext: &[u8]) -> Result<Vec<u8>, String> {
    Err(
        "dpapi protect unavailable: secrets at rest need Windows DPAPI (Windows-only build)"
            .to_string(),
    )
}

pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    unprotect_platform(blob)
}

#[cfg(windows)]
fn unprotect_platform(blob: &[u8]) -> Result<Vec<u8>, String> {
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

#[cfg(not(windows))]
fn unprotect_platform(_blob: &[u8]) -> Result<Vec<u8>, String> {
    Err(
        "dpapi unprotect unavailable: secrets at rest need Windows DPAPI (Windows-only build)"
            .to_string(),
    )
}

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

pub fn unseal_params(stored: &str) -> Result<String, String> {
    use base64::Engine as _;

    let trimmed = stored.trim();
    if let Some(rest) = trimmed.strip_prefix(MARKER) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(rest.trim())
            .map_err(|_| "dpapi params are not valid base64".to_string())?;
        let plain = unprotect(&bytes)?;
        return String::from_utf8(plain)
            .map_err(|_| "dpapi params are not valid utf-8".to_string());
    }

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

#[cfg(test)]
pub fn set_fail_for_test(fail: bool) {
    FAIL_FOR_TEST.with(|flag| flag.set(fail));
}

fn fail_for_test() -> bool {
    FAIL_FOR_TEST.with(|flag| flag.get())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[cfg(windows)]
    #[test]
    fn t48_seal_roundtrip() {
        let sealed = seal_params("{\"a\":1}").expect("seal works");
        assert!(sealed.starts_with(MARKER), "marker shape, got {sealed}");
        assert_eq!(unseal_params(&sealed).expect("unseal works"), "{\"a\":1}");
    }

    #[cfg(windows)]
    #[test]
    fn t48_unseal_rejection_table() {
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

        for legacy in ["{\"a\":1}", "   {\"a\":1}  "] {
            assert_eq!(
                unseal_params(legacy).expect("legacy passes through"),
                legacy
            );
        }
    }

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

    #[test]
    fn unprotect_empty_blob_fails_loud() {
        let error = unprotect(&[]).expect_err("empty blob must fail");
        assert!(!error.is_empty(), "loud failure");
        assert!(
            error.contains("dpapi") || error.contains("Windows"),
            "loud dpapi cause, got {error}"
        );
    }

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
