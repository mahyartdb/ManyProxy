//! The left nav: the scopes on top, the utilities at the bottom.
//!
//! Belongs: the nav column and the entries it holds.
//! Not here: the scope sections and the strip — `nav_scopes` owns those.
use crate::*;

/// The nav's expanded width. A scope entry needs its label.
pub(crate) const NAV_WIDTH: f32 = 220.;
/// The nav's collapsed width: one letter per entry, with the tooltip naming it.
pub(crate) const NAV_COLLAPSED_WIDTH: f32 = 56.;
/// Below this viewport width the nav collapses to its letters.
pub(crate) const NAV_BREAKPOINT: f32 = 1200.;

/// One nav row's height.
pub(crate) const NAV_ENTRY_HEIGHT: f32 = 32.;

/// One utility row: the label, or its letter when the nav is collapsed. The
/// two utilities are the only fixed entries below the scroll area.
struct NavEntry {
    id: &'static str,
    letter: &'static str,
    label: String,
    action: fn(&mut AppView, &mut Window, &mut Context<AppView>),
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

    /// The left nav. `All configs` stays pinned; the group and source scopes
    /// scroll between it and the two utilities, so a long group list never
    /// pushes Routing presets or Settings out of reach.
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
            .child(self.render_all_configs_entry(total, collapsed, cx))
            .child(
                div()
                    .id("nav-scopes")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.nav_scroll)
                    .vertical_scrollbar(&self.nav_scroll)
                    .child(self.render_group_section(collapsed, cx))
                    .child(self.render_source_section(collapsed, cx)),
            )
            .child(
                div()
                    .id("nav-utilities-hairline")
                    .test_support()
                    .h(px(1.))
                    .w_full()
                    .flex_shrink_0()
                    .bg(cx.theme().sidebar_border),
            )
            .child(nav_entry(
                NavEntry {
                    id: "nav-routing-presets",
                    letter: "R",
                    label: t!("nav-routing-presets"),
                    action: AppView::open_presets_dialog,
                },
                collapsed,
                cx,
            ))
            .child(nav_entry(
                NavEntry {
                    id: "nav-settings",
                    letter: "S",
                    label: t!("action-settings"),
                    action: AppView::open_settings_dialog,
                },
                collapsed,
                cx,
            ))
            .into_any_element()
    }
}

/// One utility row: the label, or its letter when the nav is collapsed. The
/// tooltip names the dialog it opens.
fn nav_entry(entry: NavEntry, collapsed: bool, cx: &mut Context<AppView>) -> AnyElement {
    let NavEntry {
        id,
        letter,
        label,
        action,
    } = entry;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().list_hover;
    let tip = label.clone();
    let view = cx.entity();

    let mut row = div()
        .id(id)
        .test_support()
        .aria_label(label.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(NAV_ENTRY_HEIGHT))
        .flex_shrink_0()
        .px_2()
        .border_l_2()
        .border_color(cx.theme().transparent)
        .text_color(muted)
        .role(Role::Button)
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
        .on_click(move |_, window, cx| {
            view.update(cx, |view, cx| action(view, window, cx));
        })
        .hover(move |style| style.bg(hover).text_color(foreground));

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

    row.into_any_element()
}
