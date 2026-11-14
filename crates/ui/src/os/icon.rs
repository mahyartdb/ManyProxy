//! Shared RGBA → `HICON` conversion. The taskbar thumbnails and the hidden
//! notification icon both start as pixels in this crate, so the bottom-up
//! mask/XOR packing lives in one place.

#[cfg(windows)]
pub fn hicon_from_rgba(
    rgba: &[u8],
    width: usize,
    height: usize,
) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::Foundation::HINSTANCE;
    use windows::Win32::UI::WindowsAndMessaging::CreateIcon;

    if width == 0 || height == 0 || rgba.len() != width * height * 4 {
        return None;
    }
    let mask_stride = width.div_ceil(32) * 4;
    let mut and_mask = vec![0u8; mask_stride * height];
    let mut xor_bits = vec![0u8; width * height * 4];
    for y in 0..height {
        let source_row = height - 1 - y;
        for x in 0..width {
            let source = (source_row * width + x) * 4;
            let alpha = rgba[source + 3];
            if alpha < 128 {
                and_mask[y * mask_stride + x / 8] |= 1 << (7 - (x % 8));
            }
            let target = (y * width + x) * 4;
            xor_bits[target] = rgba[source + 2];
            xor_bits[target + 1] = rgba[source + 1];
            xor_bits[target + 2] = rgba[source];
            xor_bits[target + 3] = alpha;
        }
    }
    unsafe {
        CreateIcon(
            HINSTANCE(std::ptr::null_mut()),
            width as i32,
            height as i32,
            1,
            32,
            and_mask.as_ptr(),
            xor_bits.as_ptr(),
        )
        .ok()
    }
}
