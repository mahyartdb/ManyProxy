#[cfg(windows)]
use std::ffi::c_void;

#[cfg(windows)]
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

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
    use zeroize::Zeroize as _;
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
        // DPAPI hands back its own copy of the plaintext; wipe it before the
        // freed page can sit in the process with a readable secret in it.
        let raw = std::slice::from_raw_parts_mut(output.pb_data, output.cb_data as usize);
        let bytes = raw.to_vec();
        raw.zeroize();
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

    #[test]
    fn t48_fail_seam_aborts_writes_loudly() {
        let _guard = FailGuard::on();
        let error = protect(b"x").expect_err("protect aborts");
        assert!(error.contains("dpapi"), "loud dpapi cause, got {error}");
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

    #[cfg(not(windows))]
    #[test]
    fn t48_non_windows_dpapi_is_a_loud_stub() {
        for error in [
            protect(b"x").expect_err("protect is a stub"),
            unprotect(b"blob").expect_err("unprotect is a stub"),
        ] {
            assert!(error.contains("Windows"), "Windows-only cause, got {error}");
        }
    }
}
