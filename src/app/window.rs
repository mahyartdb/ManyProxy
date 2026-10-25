use crate::*;
use gpui_kit::prelude::FluentBuilder as _;

#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// K12/A1-1: the custom chrome's height, matching the kit title bar the app
/// replaced so the toolbar and status strip keep their vertical budget.
pub(crate) const TITLEBAR_HEIGHT: f32 = 34.;
/// K12/A1-1: one flat control cell. Square-ish, like the platform's own
/// caption buttons, with a hairline between cells.
pub(crate) const TITLEBAR_CONTROL_WIDTH: f32 = 46.;

/// K12/A1-1: the three caption controls drawn in the custom chrome. The ids
/// are pinned for the UI test; the label/action tables keep the visible
/// affordance and the platform call from drifting apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitlebarControl {
    Minimize,
    Maximize,
    Close,
}

/// K12/A1-1: what a caption control (or a title-area double click) asks the
/// window to do. Kept as data so the mapping is testable without a platform
/// window; `apply_titlebar_action` is the only place that touches the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitlebarAction {
    Minimize,
    ToggleMaximize,
    Close,
}

impl TitlebarControl {
    pub(crate) fn id(self) -> &'static str {
        match self {
            TitlebarControl::Minimize => "titlebar-minimize",
            TitlebarControl::Maximize => "titlebar-maximize",
            TitlebarControl::Close => "titlebar-close",
        }
    }

    /// The maximize cell turns into a restore affordance while maximized, but
    /// keeps its stable id so tests and the platform hit-test never chase it.
    pub(crate) fn label(self, maximized: bool) -> &'static str {
        match self {
            TitlebarControl::Minimize => "Minimize the window",
            TitlebarControl::Maximize if maximized => "Restore the window",
            TitlebarControl::Maximize => "Maximize the window",
            TitlebarControl::Close => "Close the window",
        }
    }

    pub(crate) fn area(self) -> WindowControlArea {
        match self {
            TitlebarControl::Minimize => WindowControlArea::Min,
            TitlebarControl::Maximize => WindowControlArea::Max,
            TitlebarControl::Close => WindowControlArea::Close,
        }
    }
}

pub(crate) fn titlebar_control_action(control: TitlebarControl) -> TitlebarAction {
    match control {
        TitlebarControl::Minimize => TitlebarAction::Minimize,
        TitlebarControl::Maximize => TitlebarAction::ToggleMaximize,
        TitlebarControl::Close => TitlebarAction::Close,
    }
}

/// K12/A1-1: double-clicking the title area toggles maximize (restore when
/// already maximized).
pub(crate) fn titlebar_double_click_action() -> TitlebarAction {
    TitlebarAction::ToggleMaximize
}

/// K12/A1-1: hermetic tests exercise the wiring without a platform window —
/// the test platform's `minimize`/`zoom` are unimplemented. The counter is the
/// observable proof that an id reached its action.
#[cfg(test)]
pub(crate) static TITLEBAR_ACTION_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(crate) fn apply_titlebar_action(action: TitlebarAction, window: &mut Window) {
    #[cfg(test)]
    {
        let _ = (action, window);
        TITLEBAR_ACTION_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    #[cfg(not(test))]
    match action {
        TitlebarAction::Minimize => window.minimize_window(),
        TitlebarAction::ToggleMaximize => window.zoom_window(),
        TitlebarAction::Close => window.remove_window(),
    }
}

/// K12/A1-1: flat hairline caption buttons per DESIGN — no shadows, no icon
/// font (the kit's icon set is not initialized here and rendered blank), a
/// quiet hover wash for minimize/maximize and the alarm wash for close.
fn titlebar_control(control: TitlebarControl, maximized: bool, cx: &App) -> AnyElement {
    let close = control == TitlebarControl::Close;
    let theme = cx.theme();
    let hover_bg = if close {
        theme.danger
    } else {
        theme.list_hover
    };
    let hover_fg = if close {
        theme.danger_foreground
    } else {
        theme.foreground
    };
    let active_bg = if close {
        theme.danger_active
    } else {
        theme.secondary_active
    };
    let glyph_color = theme.foreground;
    let glyph = titlebar_control_glyph(control, maximized, glyph_color, theme.title_bar);
    let label = control.label(maximized);
    let tip = label.to_string();
    let action = titlebar_control_action(control);
    // On Windows the NC hit-test area performs the native action; a click
    // listener on top of it would act twice. Tests (and non-Windows builds)
    // take the explicit path so the id → action wiring stays observable.
    let clickable = cfg!(test) || !cfg!(windows);
    div()
        .id(control.id())
        .test_support()
        .role(Role::Button)
        .aria_label(label)
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
        .flex()
        .items_center()
        .justify_center()
        .h_full()
        .w(px(TITLEBAR_CONTROL_WIDTH))
        .flex_shrink_0()
        .border_l_1()
        .border_color(theme.border)
        .text_color(glyph_color)
        .when(cfg!(windows), |this| {
            this.window_control_area(control.area())
        })
        .when(clickable, |this| {
            this.on_click(move |_, window, _| apply_titlebar_action(action, window))
        })
        .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
        .active(move |style| style.bg(active_bg).text_color(hover_fg))
        .child(glyph)
        .into_any_element()
}

/// Drawn glyphs, not icon-font codepoints: the small-glyph family (A5-1) is
/// exactly what fails when a font falls back. A line, a square outline, two
/// offset squares, and a multiplication sign read at any scale.
fn titlebar_control_glyph(
    control: TitlebarControl,
    maximized: bool,
    color: Hsla,
    band: Hsla,
) -> AnyElement {
    match control {
        TitlebarControl::Minimize => div().w(px(10.)).h(px(1.)).bg(color).into_any_element(),
        TitlebarControl::Maximize if maximized => div()
            .relative()
            .w(px(12.))
            .h(px(12.))
            .child(
                div()
                    .absolute()
                    .top(px(0.))
                    .right(px(0.))
                    .w(px(8.))
                    .h(px(8.))
                    .border_1()
                    .border_color(color),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .left(px(0.))
                    .w(px(8.))
                    .h(px(8.))
                    .border_1()
                    .border_color(color)
                    .bg(band),
            )
            .into_any_element(),
        TitlebarControl::Maximize => div()
            .w(px(10.))
            .h(px(10.))
            .border_1()
            .border_color(color)
            .into_any_element(),
        TitlebarControl::Close => div().text_sm().child("✕").into_any_element(),
    }
}

impl AppView {
    /// K12/A1-1: the app's own title bar. The kit title bar is not rendered
    /// because its caption glyphs come from an icon font this app never loads
    /// (they painted blank). The drag region is the whole title area; the
    /// three cells at the trailing end are real window controls.
    pub(crate) fn render_titlebar(&self, title: String, window: &Window, cx: &App) -> AnyElement {
        let maximized = window.is_maximized();
        let drag = div()
            .id("titlebar-drag-region")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .flex_1()
            .min_w_0()
            .h_full()
            .pl_3()
            .window_control_area(WindowControlArea::Drag)
            .on_click(|event, window, _| {
                // Windows handles HTCAPTION double clicks natively; the
                // explicit path covers non-Windows and the hermetic wiring
                // test. `apply_titlebar_action` is inert on the test platform.
                if event.click_count() == 2 && (cfg!(test) || !cfg!(windows)) {
                    apply_titlebar_action(titlebar_double_click_action(), window);
                }
            })
            .child(
                div()
                    .id("titlebar-title")
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .child(Label::new(title)),
            );
        div()
            .id("title-bar")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(TITLEBAR_HEIGHT))
            .flex_shrink_0()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().title_bar_border)
            .child(drag)
            .child(
                div()
                    .id("titlebar-controls")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .flex_shrink_0()
                    .child(titlebar_control(TitlebarControl::Minimize, maximized, cx))
                    .child(titlebar_control(TitlebarControl::Maximize, maximized, cx))
                    .child(titlebar_control(TitlebarControl::Close, maximized, cx)),
            )
            .into_any_element()
    }
}

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

/// The startup rectangle: a stored placement wins; otherwise the window opens
/// at 1000×700 centered in the primary display's work area. The old upper-left
/// default (origin 0,0) is not a placement — it treated "never moved" as a
/// deliberate corner, which is exactly the first-run corner nobody chose. A
/// headless caller (tests, no display) keeps the old (0,0) fallback.
pub(crate) fn startup_window_bounds(
    geometry: Option<WindowGeometry>,
    visible_bounds: Option<Bounds<Pixels>>,
) -> WindowBounds {
    let stored = geometry.and_then(WindowGeometry::clamped);
    let desired = stored.map_or_else(
        || size(px(1000.), px(700.)),
        |geometry| size(px(geometry.width), px(geometry.height)),
    );
    if let Some(geometry) = stored.filter(|geometry| geometry.x != 0. || geometry.y != 0.) {
        return WindowBounds::Windowed(Bounds::new(
            point(px(geometry.x), px(geometry.y)),
            size(px(geometry.width), px(geometry.height)),
        ));
    }
    let Some(visible) = visible_bounds else {
        return WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), desired));
    };
    WindowBounds::Windowed(Bounds::centered_at(
        visible.center(),
        desired.min(&visible.size),
    ))
}

pub(crate) fn app_window_options(bounds: WindowBounds) -> WindowOptions {
    let mut options = TitleBar::window_options();
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(APP_TITLE.into());
    }
    options.window_bounds = Some(bounds);
    options.window_min_size = Some(size(
        px(WindowGeometry::MIN_WIDTH),
        px(WindowGeometry::MIN_HEIGHT),
    ));
    options
}

/// T-32: the persisted rectangle, read before the window exists — the only
/// point the startup bounds can seed from. A missing file, an unreadable
/// store, or an absent/corrupt key all read as `None`, so the first run
/// centers instead of landing in the old upper-left corner.
pub(crate) fn stored_window_geometry() -> Option<WindowGeometry> {
    Store::peek_meta_readonly(&Store::default_path(), Store::META_GEOMETRY)
        .as_deref()
        .and_then(WindowGeometry::from_json)
}
