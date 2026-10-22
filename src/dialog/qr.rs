use crate::*;

pub(crate) const SHARE_WARNING: &str = "This link carries the endpoint's credentials — anyone holding it can use it. Prefer Copy URLs for credential-free sharing.";

// ---- T-28: share-dialog QR of the two listener URLs ---------------------

/// Modules of quiet zone baked into every grid — the QR spec's minimum.
pub(crate) const SHARE_QR_QUIET_ZONE: usize = 4;
/// Side of one QR plate in the Share dialog. The plate never shrinks; the module
/// grid scales inside it, so the quiet zone cannot be clipped by a resize.
pub(crate) const SHARE_QR_PLATE_PX: f32 = 132.;

/// A monochrome QR module grid with its quiet zone baked in, row-major and square.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QrGrid {
    pub(crate) side: usize,
    pub(crate) dark: Vec<bool>,
}

impl QrGrid {
    pub(crate) fn is_dark(&self, col: usize, row: usize) -> bool {
        col < self.side && row < self.side && self.dark[row * self.side + col]
    }

    /// FNV-1a over the row-major module bits: the regression fingerprint for a fixed
    /// payload, since decoding a rendered QR is out of scope by decision.
    #[cfg(test)]
    pub(crate) fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for &dark in &self.dark {
            hash ^= u64::from(dark);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

/// Encode one listener URL as a byte-mode QR at least error-correction level M (the ticket
/// allows L or M; M already fits these URLs in a 33-module grid, quiet zone included,
/// and tolerates a smudge on a phone screen). The encoder picks the smallest version
/// that fits, well past the ticket's 255-byte ceiling, and we add the quiet zone.
pub(crate) fn qr_grid(payload: &str) -> Result<QrGrid, String> {
    let code = QrCode::encode_binary(payload.as_bytes(), QrCodeEcc::Medium)
        .map_err(|_| format!("QR payload too long ({} bytes)", payload.len()))?;
    let inner = code.size() as usize;
    let side = inner + SHARE_QR_QUIET_ZONE * 2;
    let mut dark = vec![false; side * side];
    for row in 0..inner {
        for col in 0..inner {
            if code.get_module(col as i32, row as i32) {
                dark[(row + SHARE_QR_QUIET_ZONE) * side + col + SHARE_QR_QUIET_ZONE] = true;
            }
        }
    }
    Ok(QrGrid { side, dark })
}

/// The row's two listener URLs, byte-identical to `proxy_urls`, with their labels.
pub(crate) fn share_qr_payloads(config: &ProxyConfig) -> [(&'static str, String); 2] {
    let (socks, http) = proxy_urls(&config.bind_ip, config.port);
    [("socks", socks), ("http", http)]
}

/// Where the module grid lands inside a plate box: cell edge plus the grid origin. The
/// whole grid — quiet zone included — is centered and never exceeds the box, so a
/// resize cannot clip the quiet zone; whole-pixel cells keep modules square and crisp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct QrPlate {
    pub(crate) cell: f32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) side: usize,
}

impl QrPlate {
    pub(crate) fn fit(width: f32, height: f32, side: usize) -> Self {
        let box_side = width.min(height).max(0.);
        let raw = box_side / side as f32;
        let cell = if raw >= 1. { raw.floor() } else { raw };
        let plate = cell * side as f32;
        Self {
            cell,
            x: ((width - plate) / 2.).max(0.),
            y: ((height - plate) / 2.).max(0.),
            side,
        }
    }

    pub(crate) fn plate_side(&self) -> f32 {
        self.cell * self.side as f32
    }

    pub(crate) fn module_origin(&self, col: usize, row: usize) -> (f32, f32) {
        (
            self.x + col as f32 * self.cell,
            self.y + row as f32 * self.cell,
        )
    }
}

/// Paint the white plate (quiet zone included) and the dark modules on top of it.
/// Scanners read black-on-white, so the plate keeps that pair in both themes while the
/// surrounding dialog and the labels stay theme-colored.
pub(crate) fn paint_share_qr(grid: &QrGrid, plate: QrPlate, window: &mut Window) {
    let plate_side = px(plate.plate_side());
    window.paint_quad(fill(
        Bounds::new(
            point(px(plate.x), px(plate.y)),
            size(plate_side, plate_side),
        ),
        rgb(0xFFFFFF),
    ));
    for row in 0..grid.side {
        for col in 0..grid.side {
            if !grid.is_dark(col, row) {
                continue;
            }
            let (x, y) = plate.module_origin(col, row);
            window.paint_quad(fill(
                Bounds::new(point(px(x), px(y)), size(px(plate.cell), px(plate.cell))),
                rgb(0x000000),
            ));
        }
    }
}

/// One labeled QR plate. The payload that cannot encode (far beyond any listener URL)
/// degrades to a caption rather than an empty square.
pub(crate) fn share_qr_block(
    label: &'static str,
    url: &str,
    id: &'static str,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let content: AnyElement = match qr_grid(url) {
        Ok(grid) => {
            let grid = Rc::new(grid);
            let side = grid.side;
            canvas(
                move |bounds, _, _| {
                    QrPlate::fit(
                        bounds.size.width.as_f32(),
                        bounds.size.height.as_f32(),
                        side,
                    )
                },
                move |_, plate, window, _| paint_share_qr(&grid, plate, window),
            )
            .size_full()
            .into_any_element()
        }
        Err(reason) => div()
            .text_xs()
            .text_color(muted)
            .child(reason)
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .child(
            div()
                .id(id)
                .test_support()
                .w(px(SHARE_QR_PLATE_PX))
                .h(px(SHARE_QR_PLATE_PX))
                .flex_shrink_0()
                .child(content),
        )
        .child(
            div()
                .id(format!("{id}-label"))
                .test_support()
                .text_xs()
                .text_color(muted)
                .child(label),
        )
}

/// The dialog's QR region: both schemes side by side, one scannable code each.
pub(crate) fn share_qr_region(
    payloads: &[(&'static str, String); 2],
    cx: &App,
) -> impl IntoElement {
    div()
        .id("share-qr-region")
        .test_support()
        .flex()
        .flex_wrap()
        .gap_3()
        .child(share_qr_block(
            payloads[0].0,
            &payloads[0].1,
            "share-qr-socks",
            cx,
        ))
        .child(share_qr_block(
            payloads[1].0,
            &payloads[1].1,
            "share-qr-http",
            cx,
        ))
}
