//! The action bar: the view title and count, the intake actions, the palette
//! trigger and the overflow.
//!
//! Belongs: the bar above the table and the menu behind its `⋯`.
//! Not here: the filter row — slice 3 owns it — or the transport controls,
//! which live in the engine bar.
use crate::*;

/// The action bar's height. The title, the count and four controls share it.
pub(crate) const ACTION_BAR_HEIGHT: f32 = 44.;

impl AppView {
    /// The empty canvas and the action bar share these two handlers.
    pub(crate) fn open_add_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_add_dialog(window, cx);
    }

    pub(crate) fn open_import_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
    }

    /// The action bar. Left: the view's name and its count. Right: `Import`,
    /// `Add`, the palette trigger and the overflow.
    ///
    /// The count truncates before the buttons do: at the minimum width the
    /// numbers give up their room first, never a control.
    pub(crate) fn render_action_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let libraries = self.libraries.read(cx);
        let delegate = libraries.delegate();
        let (total, enabled) = delegate.counts();
        let count_text = status_count_text(total, enabled);
        let view = cx.entity();
        let update_available = self.update_available.is_some();

        let count_tip = count_text.clone();
        let title = div()
            .id("action-bar-title")
            .test_support()
            .flex_shrink_0()
            .child(t!("nav-all-configs"));
        let count = div()
            .id("action-bar-count")
            .test_support()
            .min_w_0()
            .truncate()
            .text_color(cx.theme().muted_foreground)
            .aria_label(count_text.clone())
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(count_tip.clone())).into())
            .child(count_text);

        div()
            .id("action-bar")
            .test_support()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_2()
            .w_full()
            .h(px(ACTION_BAR_HEIGHT))
            .flex_shrink_0()
            .min_w_0()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(title)
            .child(count)
            .child(div().flex_1())
            .child(
                Button::new("action-bar-import")
                    .small()
                    .label(t!("action-import"))
                    .accessibility_label(t!("action-import"))
                    .tooltip(t!("toolbar-import-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_import_config(window, cx);
                    })),
            )
            .child(
                Button::new("action-bar-add")
                    .small()
                    .primary()
                    .label(t!("action-add-config"))
                    .accessibility_label(t!("action-add-config-accessible"))
                    .tooltip(t!("action-add-config-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_add_config(window, cx);
                    })),
            )
            .child(
                Button::new("action-bar-palette")
                    .small()
                    .label(t!("action-command-palette"))
                    .accessibility_label(t!("action-command-palette"))
                    .tooltip(t!("toolbar-palette-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_palette(window, cx);
                    })),
            )
            .child(action_overflow_dot(update_available, cx))
            .child(self.render_action_overflow(view))
            .into_any_element()
    }

    /// The overflow behind the action bar's `⋯`: the verbs that are neither
    /// intake nor transport.
    pub(crate) fn render_action_overflow(&self, view: Entity<AppView>) -> AnyElement {
        Button::new("action-bar-overflow")
            .small()
            .ghost()
            .label("⋯")
            .accessibility_label(t!("action-overflow"))
            .tooltip(t!("action-overflow"))
            .dropdown_menu(move |menu, _, _| {
                let export_view = view.clone();
                let update_view = view.clone();
                let about_view = view.clone();
                menu.item(
                    PopupMenuItem::element(|_, _| {
                        topbar_menu_row(
                            "action-overflow-export",
                            t!("toolbar-export"),
                            t!("toolbar-export"),
                            t!("toolbar-export-tip"),
                            None,
                        )
                    })
                    .on_click(move |_, window, cx| {
                        export_view.update(cx, |view, cx| {
                            view.open_export_dialog(window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        topbar_menu_row(
                            "action-overflow-updates",
                            t!("toolbar-check-updates"),
                            t!("toolbar-check-updates"),
                            t!("toolbar-check-updates-tip"),
                            None,
                        )
                    })
                    .on_click(move |_, window, cx| {
                        update_view.update(cx, |view, cx| {
                            // A release the passive check already found opens
                            // straight away; otherwise the menu item runs the
                            // same manual check the retired Tools item ran.
                            if !view.open_available_update(window, cx) {
                                view.check_for_updates(window, cx);
                            }
                        });
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, _| {
                        topbar_menu_row(
                            "action-overflow-about",
                            t!("toolbar-about"),
                            t!("toolbar-about"),
                            t!("toolbar-about-tip"),
                            None,
                        )
                    })
                    .on_click(move |_, window, cx| {
                        about_view.update(cx, |view, cx| {
                            view.open_about_dialog(window, cx);
                        });
                    }),
                )
            })
            .into_any_element()
    }
}

/// The passive check's marker on the overflow. The retired status strip
/// carried this as a text hint; a dot says the same thing in one pixel.
fn action_overflow_dot(update_available: bool, cx: &mut Context<AppView>) -> AnyElement {
    if !update_available {
        return div().into_any_element();
    }
    div()
        .id("action-bar-update-dot")
        .test_support()
        .w(px(6.))
        .h(px(6.))
        .flex_shrink_0()
        .rounded_full()
        .bg(cx.theme().accent)
        .aria_label(t!("status-update-hint"))
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(t!("status-update-hint"))).into())
        .into_any_element()
}
