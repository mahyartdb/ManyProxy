use crate::*;
use gpui_kit::component::{Icon, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;

/// K12/A1-1: the custom chrome's height, matching the kit title bar the app
/// replaced so the toolbar and status strip keep their vertical budget.
pub const TITLEBAR_HEIGHT: f32 = 34.;
/// K12/A1-1: one flat control cell. Square-ish, like the platform's own
/// caption buttons, with a hairline between cells.
pub const TITLEBAR_CONTROL_WIDTH: f32 = 46.;

/// K12/A1-1: the three caption controls drawn in the custom chrome. The ids
/// are pinned for the UI test; the label/action tables keep the visible
/// affordance and the platform call from drifting apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarControl {
    Minimize,
    Maximize,
    Close,
}

/// K12/A1-1: what a caption control (or a title-area double click) asks the
/// window to do. Kept as data so the mapping is testable without a platform
/// window; `apply_titlebar_action` is the only place that touches the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarAction {
    Minimize,
    ToggleMaximize,
    Close,
}

impl TitlebarControl {
    pub fn id(self) -> &'static str {
        match self {
            TitlebarControl::Minimize => "titlebar-minimize",
            TitlebarControl::Maximize => "titlebar-maximize",
            TitlebarControl::Close => "titlebar-close",
        }
    }

    /// The maximize cell turns into a restore affordance while maximized, but
    /// keeps its stable id so tests and the platform hit-test never chase it.
    pub fn label(self, maximized: bool) -> String {
        match self {
            TitlebarControl::Minimize => t!("titlebar-minimize"),
            TitlebarControl::Maximize if maximized => t!("titlebar-restore"),
            TitlebarControl::Maximize => t!("titlebar-maximize"),
            TitlebarControl::Close => t!("titlebar-close"),
        }
    }

    pub fn area(self) -> WindowControlArea {
        match self {
            TitlebarControl::Minimize => WindowControlArea::Min,
            TitlebarControl::Maximize => WindowControlArea::Max,
            TitlebarControl::Close => WindowControlArea::Close,
        }
    }
}

pub fn titlebar_control_action(control: TitlebarControl) -> TitlebarAction {
    match control {
        TitlebarControl::Minimize => TitlebarAction::Minimize,
        TitlebarControl::Maximize => TitlebarAction::ToggleMaximize,
        TitlebarControl::Close => TitlebarAction::Close,
    }
}

/// K12/A1-1: double-clicking the title area toggles maximize (restore when
/// already maximized).
pub fn titlebar_double_click_action() -> TitlebarAction {
    TitlebarAction::ToggleMaximize
}

/// K12/A1-1: hermetic tests exercise the wiring without a platform window —
/// the test platform's `minimize`/`zoom` are unimplemented. The counter is the
/// observable proof that an id reached its action.
#[cfg(any(test, feature = "test-support"))]
pub static TITLEBAR_ACTION_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub fn apply_titlebar_action(action: TitlebarAction, window: &mut Window) {
    #[cfg(any(test, feature = "test-support"))]
    {
        // Borrow, don't move: the production match below still compiles under
        // the test feature, and `window` must survive for it.
        let _ = &action;
        let _ = &*window;
        TITLEBAR_ACTION_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    #[cfg(not(any(test, feature = "test-support")))]
    match action {
        TitlebarAction::Minimize => window.minimize_window(),
        TitlebarAction::ToggleMaximize => window.zoom_window(),
        TitlebarAction::Close => window.remove_window(),
    }
}

/// K12/A1-1: flat hairline caption buttons per DESIGN — the kit's bundled
/// `window-*.svg` glyphs, a quiet hover wash for minimize/maximize and the
/// alarm wash for close.
pub fn titlebar_control(control: TitlebarControl, maximized: bool, cx: &App) -> AnyElement {
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
    let glyph = titlebar_control_icon(control, maximized);
    let label = control.label(maximized);
    let tip = label.to_string();
    let action = titlebar_control_action(control);
    // On Windows the NC hit-test area performs the native action; a click
    // listener on top of it would act twice. Tests (and non-Windows builds)
    // take the explicit path so the id → action wiring stays observable.
    let clickable = cfg!(any(test, feature = "test-support")) || !cfg!(windows);
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

/// The caption glyph resolves its own color from the ambient text style at
/// build time, so the cell's hover/active washes recolor the cell but not
/// the glyph — the kit `Icon` stamps its color eagerly. Same visuals as the
/// drawn glyph it replaced.
fn titlebar_control_icon(control: TitlebarControl, maximized: bool) -> Icon {
    let name = match control {
        TitlebarControl::Minimize => IconName::WindowMinimize,
        TitlebarControl::Maximize if maximized => IconName::WindowRestore,
        TitlebarControl::Maximize => IconName::WindowMaximize,
        TitlebarControl::Close => IconName::WindowClose,
    };
    Icon::new(name).small()
}

impl AppView {
    /// K12/A1-1: the app's own title bar. The kit `TitleBar` was evaluated and
    /// the app keeps its own chrome: the caption ids are pinned by the UI test
    /// and the Windows NC hit-test needs one drag region with three real
    /// window controls, not a reparented widget tree.
    pub fn render_titlebar(&self, title: String, window: &Window, cx: &App) -> AnyElement {
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
                if event.click_count() == 2
                    && (cfg!(any(test, feature = "test-support")) || !cfg!(windows))
                {
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

/// The startup rectangle: a stored placement wins; otherwise the window opens
/// at 1200×700 centered in the primary display's work area. The old upper-left
/// default (origin 0,0) is not a placement — it treated "never moved" as a
/// deliberate corner, which is exactly the first-run corner nobody chose. A
/// headless caller (tests, no display) keeps the old (0,0) fallback.
///
/// The width is the nav's own breakpoint: at 1200 the nav opens expanded, so
/// the first frame shows the shell the user is meant to learn.
pub fn startup_window_bounds(
    geometry: Option<WindowGeometry>,
    visible_bounds: Option<Bounds<Pixels>>,
) -> WindowBounds {
    let stored = geometry.and_then(WindowGeometry::clamped);
    let desired = stored.map_or_else(
        || size(px(1200.), px(700.)),
        |geometry| size(px(geometry.width), px(geometry.height)),
    );
    // A stored rectangle can outgrow the display it opens on (a laptop panel
    // at high scaling is the common case): keep the chosen position, but
    // never open wider or taller than the work area.
    let desired = visible_bounds.map_or(desired, |visible| desired.min(&visible.size));
    if let Some(geometry) = stored.filter(|geometry| geometry.x != 0. || geometry.y != 0.) {
        return WindowBounds::Windowed(Bounds::new(point(px(geometry.x), px(geometry.y)), desired));
    }
    let Some(visible) = visible_bounds else {
        return WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), desired));
    };
    WindowBounds::Windowed(Bounds::centered_at(visible.center(), desired))
}

/// The window's design minimum, capped by the display's work area. Logical
/// points shrink as Windows display scaling rises: a 1080p panel at 200%
/// leaves roughly 540 points of usable height, well under the 620-point
/// design floor. A minimum taller than the screen pins the status strip below
/// the desktop and leaves no drag edge to recover it, so the display wins.
pub fn window_min_size(visible_bounds: Option<Bounds<Pixels>>) -> Size<Pixels> {
    let design = size(
        px(WindowGeometry::MIN_WIDTH),
        px(WindowGeometry::MIN_HEIGHT),
    );
    visible_bounds.map_or(design, |visible| design.min(&visible.size))
}

pub fn app_window_options(
    bounds: WindowBounds,
    visible_bounds: Option<Bounds<Pixels>>,
) -> WindowOptions {
    let mut options = TitleBar::window_options();
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(t!("app-title").into());
    }
    options.window_bounds = Some(bounds);
    options.window_min_size = Some(window_min_size(visible_bounds));
    options
}

/// T-32: the persisted rectangle, read before the window exists — the only
/// point the startup bounds can seed from. A missing file, an unreadable
/// store, or an absent/corrupt key all read as `None`, so the first run
/// centers instead of landing in the old upper-left corner.
pub fn stored_window_geometry() -> Option<WindowGeometry> {
    Store::peek_meta_readonly(&Store::default_path(), Store::META_GEOMETRY)
        .as_deref()
        .and_then(WindowGeometry::from_json)
}
