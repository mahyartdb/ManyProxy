use crate::*;

#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Spawn a console executable with no visible console window. `creation_flags`
/// REPLACES the whole flag word (std does `self.flags = flags`), so a future
/// site that needs other flags must OR them in:
/// `cmd.creation_flags(cmd.get_creation_flags() | CREATE_NO_WINDOW)`.
pub(crate) fn spawn_hidden(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

pub(crate) fn app_window_options(geometry: Option<WindowGeometry>) -> WindowOptions {
    let mut options = TitleBar::window_options();
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(APP_TITLE.into());
    }

    // T-32: a sane stored rectangle seeds the window; absent, corrupt, or
    // non-finite meta keeps today's fixed 1000×700 at (0, 0) — first run.
    let bounds = geometry.and_then(WindowGeometry::clamped).map_or_else(
        || Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(700.))),
        |geometry| {
            Bounds::new(
                point(px(geometry.x), px(geometry.y)),
                size(px(geometry.width), px(geometry.height)),
            )
        },
    );
    options.window_bounds = Some(WindowBounds::Windowed(bounds));
    options.window_min_size = Some(size(
        px(WindowGeometry::MIN_WIDTH),
        px(WindowGeometry::MIN_HEIGHT),
    ));
    options
}

/// T-32: the persisted rectangle, read before the window exists — the only
/// point `app_window_options` can seed `WindowBounds` from. A missing file, an
/// unreadable store, or an absent/corrupt key all read as `None`, which is
/// exactly today's startup rectangle.
pub(crate) fn stored_window_geometry() -> Option<WindowGeometry> {
    Store::peek_meta_readonly(&Store::default_path(), Store::META_GEOMETRY)
        .as_deref()
        .and_then(WindowGeometry::from_json)
}
