//! Ticket 21: embed the Windows exe icon (`assets/icon.ico`, wired through
//! `manyproxy.rc`). Windows-only; other targets skip it (the `compile` call
//! itself is a no-op there).

fn main() {
    println!("cargo:rerun-if-changed=manyproxy.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    build_tray_icon_rgba();
    // Ticket 33: stamp the git hash for About (`option_env!` in main.rs).
    // Best-effort only: no git binary / no repo / any failure falls back to
    // "unknown" and never fails the build.
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Ticket 34: `.git/HEAD` only names the ref (its bytes never change on
    // commit), so follow it to the ref file — otherwise the About hash goes
    // stale after the first build. Best-effort; never fails the build.
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(target) = head.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=.git/{target}");
        }
    }
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MANYPROXY_GIT_HASH={hash}");
    if let Err(error) =
        embed_resource::compile("manyproxy.rc", embed_resource::NONE).manifest_optional()
    {
        panic!("ticket 21: cannot embed manyproxy.rc ({error})");
    }
}

/// Ticket 38: decode the tray icon from `assets/icon.ico` (no new art) into
/// an RGBA byte array for `tray-icon`'s `Icon::from_rgba` (its only icon
/// constructor). The ICO holds PNG-compressed entries, so this parses the
/// ICO directory and PNG-decodes the preferred entry (32x32, else 48, 16,
/// 256, else the largest). Emits `OUT_DIR/tray_icon_rgba.rs` with
/// `TRAY_ICON_WIDTH/HEIGHT/RGBA`. Loud failure (panic) on unreadable or
/// undecodable assets: a missing tray icon must break the build, never ship
/// a tray without an icon.
fn build_tray_icon_rgba() {
    let data =
        std::fs::read("assets/icon.ico").expect("ticket 38: assets/icon.ico must be readable");
    let entry = pick_ico_entry(&data).expect("ticket 38: assets/icon.ico has no usable entry");
    let png_bytes = &data[entry.offset..entry.offset + entry.len];
    let (width, height, rgba) =
        decode_png_rgba8(png_bytes).expect("ticket 38: tray icon PNG entry must decode");
    assert_eq!(
        rgba.len(),
        width as usize * height as usize * 4,
        "ticket 38: RGBA length matches dimensions"
    );
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set for build scripts");
    let mut out = String::with_capacity(rgba.len() * 5 + 128);
    out.push_str(&format!(
        "pub const TRAY_ICON_WIDTH: u32 = {width};\n\
         pub const TRAY_ICON_HEIGHT: u32 = {height};\n\
         pub const TRAY_ICON_RGBA: &[u8] = &["
    ));
    for (index, byte) in rgba.iter().enumerate() {
        if index % 12 == 0 {
            out.push('\n');
        }
        out.push_str(&format!("{byte},"));
    }
    out.push_str("\n];\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("tray_icon_rgba.rs"),
        out,
    )
    .expect("ticket 38: OUT_DIR must be writable");
}

struct IcoEntry {
    width: u32,
    height: u32,
    offset: usize,
    len: usize,
}

fn ico_dimensions(raw_width: u8, raw_height: u8) -> (u32, u32) {
    // A zero byte means 256 per the ICO format.
    let width = if raw_width == 0 {
        256
    } else {
        raw_width as u32
    };
    let height = if raw_height == 0 {
        256
    } else {
        raw_height as u32
    };
    (width, height)
}

/// Parse the ICO directory; returns the preferred entry's location.
/// Preference is tray-sized entries first (32, 48, 16, 256), else largest.
fn pick_ico_entry(data: &[u8]) -> Option<IcoEntry> {
    if data.len() < 6 || data[0..4] != [0, 0, 1, 0] {
        return None;
    }
    let count = u16::from_le_bytes([data[4], data[5]]) as usize;
    if data.len() < 6 + count * 16 {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let base = 6 + index * 16;
        let (width, height) = ico_dimensions(data[base], data[base + 1]);
        let len = u32::from_le_bytes([
            data[base + 8],
            data[base + 9],
            data[base + 10],
            data[base + 11],
        ]) as usize;
        let offset = u32::from_le_bytes([
            data[base + 12],
            data[base + 13],
            data[base + 14],
            data[base + 15],
        ]) as usize;
        if len == 0 || offset.checked_add(len)? > data.len() {
            continue;
        }
        // PNG-compressed entries start with the PNG magic; BMP/DIB entries
        // need a rasterizer this build script deliberately avoids. The magic
        // check stays inside the entry bounds (never past `len` into a
        // neighboring entry).
        if len >= 8
            && data[offset..offset + len]
                .starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
        {
            entries.push(IcoEntry {
                width,
                height,
                offset,
                len,
            });
        }
    }
    if entries.is_empty() {
        return None;
    }
    for preferred in [32u32, 48, 16, 256] {
        if let Some(entry) = entries.iter().find(|entry| entry.width == preferred) {
            return Some(IcoEntry {
                width: entry.width,
                height: entry.height,
                offset: entry.offset,
                len: entry.len,
            });
        }
    }
    entries.into_iter().max_by_key(|entry| entry.width)
}

/// PNG-decode one entry to straight RGBA8. Rejects anything that is not
/// 8-bit RGBA so a mode change in the asset fails loudly instead of
/// shipping a mis-decoded icon.
fn decode_png_rgba8(png_bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use png::ColorType;
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    if info.color_type != ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let expected = info.width as usize * info.height as usize * 4;
    if rgba.len() != expected {
        rgba.truncate(expected);
    }
    Some((info.width, info.height, rgba))
}
