//! The left nav: the scopes on top, the utilities at the bottom.
//!
//! Belongs: the nav column and the entries it holds.
//! Not here: the scope filtering itself — slice 7 owns groups and sources.
use crate::*;

/// The nav's expanded width. A scope entry needs its label.
pub(crate) const NAV_WIDTH: f32 = 220.;
/// The nav's collapsed width: one letter per entry, with the tooltip naming it.
pub(crate) const NAV_COLLAPSED_WIDTH: f32 = 56.;
/// Below this viewport width the nav collapses to its letters.
pub(crate) const NAV_BREAKPOINT: f32 = 1200.;

/// One nav row's height.
const NAV_ENTRY_HEIGHT: f32 = 32.;

/// One nav entry's content. The letter is what the collapsed nav shows; the
/// label is what the tooltip and the assistive tree name.
struct NavEntry {
    id: &'static str,
    letter: &'static str,
    label: String,
    count: Option<usize>,
    active: bool,
    action: Option<fn(&mut AppView, &mut Window, &mut Context<AppView>)>,
}

impl NavEntry {
    fn scope(id: &'static str, letter: &'static str, label: String, count: usize) -> Self {
        Self {
            id,
            letter,
            label,
            count: Some(count),
            active: true,
            action: None,
        }
    }

    fn utility(
        id: &'static str,
        letter: &'static str,
        label: String,
        action: fn(&mut AppView, &mut Window, &mut Context<AppView>),
    ) -> Self {
        Self {
            id,
            letter,
            label,
            count: None,
            active: false,
            action: Some(action),
        }
    }
}

impl AppView {
    /// The nav's width for this frame. The viewport is read every frame, so a
    /// resize or a DPI change can never leave a stale width behind.
    pub(crate) fn nav_width(&self, window: &Window) -> f32 {
        if window.viewport_size().width.as_f32() >= NAV_BREAKPOINT {
            NAV_WIDTH
        } else {
            NAV_COLLAPSED_WIDTH
        }
    }

    /// The left nav. `All configs` is the only scope entry in this slice and
    /// carries the library's row count; the bottom utilities open the dialogs
    /// they always opened. Groups and Sources arrive in slice 7 — this slice
    /// wires no temporary facet for them.
    pub(crate) fn render_nav(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let width = self.nav_width(window);
        let collapsed = width < NAV_WIDTH;
        let total = self.config_count(cx);

        div()
            .id("nav")
            .test_support()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(nav_entry(
                NavEntry::scope("nav-all-configs", "A", t!("nav-all-configs"), total),
                collapsed,
                cx,
            ))
            .child(div().flex_1())
            .child(nav_entry(
                NavEntry::utility(
                    "nav-routing-presets",
                    "R",
                    t!("nav-routing-presets"),
                    AppView::open_presets_dialog,
                ),
                collapsed,
                cx,
            ))
            .child(nav_entry(
                NavEntry::utility(
                    "nav-settings",
                    "S",
                    t!("action-settings"),
                    AppView::open_settings_dialog,
                ),
                collapsed,
                cx,
            ))
            .into_any_element()
    }
}

/// One nav row: the label, or its letter when the nav is collapsed, plus the
/// trailing count. The active entry keeps a 2px left rim, so the scope the
/// user is in is visible even when the nav carries nothing but letters.
fn nav_entry(entry: NavEntry, collapsed: bool, cx: &mut Context<AppView>) -> AnyElement {
    let NavEntry {
        id,
        letter,
        label,
        count,
        active,
        action,
    } = entry;
    let accent = cx.theme().accent;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().list_hover;
    let rim = if active {
        accent
    } else {
        cx.theme().transparent
    };
    let tip = label.clone();

    let mut row = div()
        .id(id)
        .test_support()
        .aria_label(label.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(NAV_ENTRY_HEIGHT))
        .px_2()
        .border_l_2()
        .border_color(rim)
        .text_color(if active { foreground } else { muted })
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into());

    if collapsed {
        row = row.child(
            div()
                .id(format!("{id}-letter"))
                .test_support()
                .flex_1()
                .text_center()
                .child(letter),
        );
    } else {
        row = row.child(
            div()
                .id(format!("{id}-label"))
                .test_support()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(label.clone()),
        );
    }

    if let Some(count) = count {
        row = row.child(
            div()
                .id(format!("{id}-count"))
                .test_support()
                .flex_shrink_0()
                .text_sm()
                .text_color(muted)
                .child(count.to_string()),
        );
    }

    if let Some(action) = action {
        row = row.role(Role::Button);
        let view = cx.entity();
        row = row
            .on_click(move |_, window, cx| {
                view.update(cx, |view, cx| action(view, window, cx));
            })
            .hover(move |style| style.bg(hover).text_color(foreground));
    }

    row.into_any_element()
}
