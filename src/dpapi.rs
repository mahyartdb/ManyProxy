//! Ticket 35: Windows DPAPI (user scope) for secrets at rest.
//!
//! Hand-rolled `crypt32.dll` FFI — two calls, no `windows`-crate weight.
//! `protect`/`unprotect` are bytes in/bytes out; the `dpapi1:` storage
//! marker lives in [`seal_params`]/[`unseal_params`]. Errors name the
//! operation only (never secret bytes, never lengths that leak shape).

use std::ffi::c_void;

const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
/// Storage marker: `dpapi1:<base64(ciphertext)>`.
pub const MARKER: &str = "dpapi1:";

#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[repr(C)]
struct ProtectPrompt {
    cb_size: u32,
    dw_prompt_flags: u32,
    hwnd_app: isize,
    sz_prompt: *const u16,
}

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

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
    fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
}

/// Encrypt `plaintext` for the current Windows user. Fails loudly when
/// DPAPI is unavailable (never silently returns plaintext).
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
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

/// Decrypt a blob from [`protect`]. Tampered or foreign blobs fail loudly.
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
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

/// Seal a params string for storage: `dpapi1:<base64>`.
pub fn seal_params(plaintext: &str) -> Result<String, String> {
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
    if let Some(rest) = stored.strip_prefix(MARKER) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(rest.trim())
            .map_err(|_| "dpapi params are not valid base64".to_string())?;
        let plain = unprotect(&bytes)?;
        return String::from_utf8(plain)
            .map_err(|_| "dpapi params are not valid utf-8".to_string());
    }
    if stored.trim_start().starts_with('{') {
        return Ok(stored.to_string());
    }
    Err("unknown params storage marker (refusing to guess)".to_string())
}
