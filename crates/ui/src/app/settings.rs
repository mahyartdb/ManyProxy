//! The Settings surface: sections, rows, data-folder and shortcut text.
//! Belongs: settings layout and the view state it edits.
//! Not here: dialog chrome or unrelated feature panels.
use crate::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::prelude::FluentBuilder as _;
use std::cell::Cell;

// ---- T-04/B17: the Settings dialog --------------------------------------
//
// One column, six sections in fixed order, each under a caps header with a
// hairline under it. Every setting is one row: the label on the left, the
// control on the right, and at most one muted help line beneath. A filter
// field at the top narrows the column by row label, so a setting is one
// scroll or one query away. Toggles and radios save on change; text fields
// save on Enter or on the dialog's one footer `Save` — never on blur. K9: the
// footer is the only commit row: `Cancel` (secondary) beside `Save`
// (primary), bottom-right, with the Esc note on its left.

/// T-04/B17: the six named sections, in spine order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SettingsSection {
    General,
    Probing,
    Updates,
    Engine,
    Maintenance,
    Shortcuts,
}

impl SettingsSection {
    pub(crate) const ORDER: [SettingsSection; 6] = [
        SettingsSection::General,
        SettingsSection::Probing,
        SettingsSection::Updates,
        SettingsSection::Engine,
        SettingsSection::Maintenance,
        SettingsSection::Shortcuts,
    ];

    /// The section's name — the column header. Test-only: the live header
    /// reads `settings_section_label`, which can reach the Fluent bundle.
    #[cfg(test)]
    pub(crate) fn title(self) -> &'static str {
        match self {
            SettingsSection::General => "General",
            SettingsSection::Probing => "Probing",
            SettingsSection::Updates => "Updates",
            SettingsSection::Engine => "Engine",
            SettingsSection::Maintenance => "Maintenance",
            SettingsSection::Shortcuts => "Shortcuts",
        }
    }

    /// The column header's element id.
    pub(crate) fn header_id(self) -> &'static str {
        match self {
            SettingsSection::General => "settings-section-general",
            SettingsSection::Probing => "settings-section-probing",
            SettingsSection::Updates => "settings-section-updates",
            SettingsSection::Engine => "settings-section-engine",
            SettingsSection::Maintenance => "settings-section-maintenance",
            SettingsSection::Shortcuts => "settings-section-shortcuts",
        }
    }

    /// The section body's element id.
    pub(crate) fn body_id(self) -> &'static str {
        match self {
            SettingsSection::General => "settings-body-general",
            SettingsSection::Probing => "settings-body-probing",
            SettingsSection::Updates => "settings-body-updates",
            SettingsSection::Engine => "settings-body-engine",
            SettingsSection::Maintenance => "settings-body-maintenance",
            SettingsSection::Shortcuts => "settings-body-shortcuts",
        }
    }
}

/// T-04: the dialog's width. Measured against the 1000×700 window: the
/// update-feed URL shapes to 520.8 px at the input's 14 px text size, and the
/// input adds 10 px of padding per side, so the field needs 540.8 px of room.
/// 820 px leaves the 168 px rail, a 16 px gap, a 604 px body (584 px of text
/// room after the padding) and the kit's 16 px padding on each side.
const SETTINGS_DIALOG_WIDTH: f32 = 820.;

/// T-04/B17: the label column's width. Every row's label starts here, so the
/// labels read as one column and the controls as another — and it is wide
/// enough for the longest label to stay on one line, because a label that wraps
/// reads as two columns of label text beside the controls.
pub(crate) const SETTINGS_LABEL_WIDTH: f32 = 300.;

/// T-04: the width a text field's control column takes.
const SETTINGS_CONTROL_WIDTH: f32 = 320.;

/// T-04: the chrome the kit's dialog adds around the body — 16 px of padding
/// top and bottom, the title line, two 16 px gaps and the 24 px footer button.
const SETTINGS_CHROME_HEIGHT: f32 = 100.;

/// T-04/K9: the tallest the section body may grow before it scrolls. The kit
/// clamps a dialog to `viewport - viewport/10 - 16` px (its own top margin plus
/// the theme's large spacing), so the body takes what is left after the chrome.
/// Short sections stay shorter than this — the body hugs its content, so a
/// three-control section never paints a 500 px black void under itself.
fn settings_body_max_height(window: &Window) -> f32 {
    let viewport = window.viewport_size().height.as_f32();
    let max_height = viewport - viewport / 10.0 - 16.0;
    (max_height - SETTINGS_CHROME_HEIGHT).max(0.0)
}

/// T-04/B17: one setting row — the label on the left, the control on the
/// right, and at most one muted help line beneath. A row the filter misses is
/// hidden but still counted, so the badge can say how many of how many showed.
fn settings_row(
    state: &SettingsState,
    id: &'static str,
    label: impl Into<String>,
    help: Option<String>,
    muted: Hsla,
    control: impl IntoElement,
) -> impl IntoElement {
    let label = label.into();
    let shown = state.note_row(id, &label);
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .min_w_0()
                .child(
                    div()
                        .w(px(SETTINGS_LABEL_WIDTH))
                        .flex_shrink_0()
                        .child(label),
                )
                .child(control),
        )
        .when_some(help, |this, text| {
            this.child(
                div()
                    .id(format!("{id}-help"))
                    .test_support()
                    .text_sm()
                    .text_color(muted)
                    .child(text),
            )
        })
        .when(!shown, |this| this.hidden())
        .test_support()
}

/// T-04: the one deliberate full-width field. The label sits above the
/// control and the control takes the whole column, because the update-feed URL
/// is longer than the label column plus a right-hand control column can hold.
fn settings_field_row(
    state: &SettingsState,
    id: &'static str,
    label: impl Into<String>,
    help: Option<String>,
    muted: Hsla,
    control: impl IntoElement,
) -> impl IntoElement {
    let label = label.into();
    let shown = state.note_row(id, &label);
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w_0()
                .child(div().id(format!("{id}-label")).test_support().child(label))
                .child(div().w_full().min_w_0().child(control)),
        )
        .when_some(help, |this, text| {
            this.child(
                div()
                    .id(format!("{id}-help"))
                    .test_support()
                    .text_sm()
                    .text_color(muted)
                    .child(text),
            )
        })
        .when(!shown, |this| this.hidden())
        .test_support()
}

/// T-04: the state the dialog body reads on every frame. The body is rebuilt
/// each frame and cannot read the view it belongs to, so every value it shows
/// travels through one of these cells.
struct SettingsState {
    view: Entity<AppView>,
    appearance: Rc<RefCell<String>>,
    accent: Rc<RefCell<Accent>>,
    text_size: Rc<RefCell<TextSize>>,
    /// R22/R30: the row density. It owns the row pitch and every chrome
    /// height, and it is persisted under `row_density`.
    density: Rc<RefCell<Density>>,
    text_size_slider: Entity<SliderState>,
    autostart: Rc<RefCell<bool>>,
    link_handler: Rc<RefCell<bool>>,
    restore: Rc<RefCell<bool>>,
    auto_restart: Rc<RefCell<bool>>,
    notify_from_tray: Rc<RefCell<bool>>,
    passive_update_check: Rc<RefCell<bool>>,
    close_action: Rc<RefCell<CloseAction>>,
    show_tray: Rc<RefCell<bool>>,
    window_corners: Rc<RefCell<WindowCorners>>,
    test_all_scope: Rc<RefCell<TestAllScope>>,
    bind_hosts: Rc<RefCell<Vec<String>>>,
    bind_host_error: Rc<RefCell<Option<String>>>,
    last_check: Rc<RefCell<Option<i64>>>,
    test_url: Entity<InputState>,
    feed_url: Entity<InputState>,
    timeout_input: Entity<InputState>,
    bind_host_input: Entity<InputState>,
    log_cap_input: Entity<InputState>,
    filter: Entity<InputState>,
    /// `(shown, total)` — written while the column is built, read by the
    /// filter badge. The column is rebuilt every frame, so it resets there.
    counted: Rc<Cell<(usize, usize)>>,
    /// B17: this frame's filter query, trimmed and lowercased. Written once at
    /// the top of `render_body`; every row reads it instead of re-reading the
    /// field, because the column is rebuilt every frame.
    query: Rc<RefCell<String>>,
    /// K9/A7-5: the three cores as `(row id, name, exe)`, resolved once when the
    /// dialog opens. The dialog is rebuilt every frame and the binaries cannot
    /// change under it, so resolving them per frame was pure I/O in a paint.
    engines: Vec<(&'static str, &'static str, Option<std::path::PathBuf>)>,
    /// T-04: the two folder paths Maintenance shows, resolved at open for the
    /// same reason: `app_data_dir` asks the OS for the exe path.
    data_folder: String,
    logs_path: String,
}

impl SettingsState {
    /// B17: does this row match? Every whitespace-separated token must appear
    /// in the row's label or its id, so two tokens narrow instead of widen, and
    /// a row's id (`settings-row-autostart`) is searchable when the label is not
    /// the word the user reached for.
    fn row_matches(query: &str, id: &str, label: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let haystack = format!("{label} {id}").to_lowercase();
        query
            .split_whitespace()
            .all(|token| haystack.contains(token))
    }

    /// B17: count one row and report whether it shows.
    fn note_row(&self, id: &str, label: &str) -> bool {
        let shown = Self::row_matches(&self.query.borrow(), id, label);
        let (mut shown_count, mut total) = self.counted.get();
        total += 1;
        if shown {
            shown_count += 1;
        }
        self.counted.set((shown_count, total));
        shown
    }

    /// B17: a row that is not built by `settings_row` still has to be counted
    /// and hidden, or its section would vanish under a filter that matches it.
    fn filtered(&self, id: &str, label: &str, element: impl Styled + IntoElement) -> AnyElement {
        let shown = self.note_row(id, label);
        // Hiding the row itself, not a wrapper around it: the wrapper was one
        // more element in a column GPUI lays out again on every frame.
        element
            .when(!shown, |this| this.hidden())
            .into_any_element()
    }

    /// B17: one caps section header over a hairline.
    fn section_header(&self, section: SettingsSection, cx: &App) -> AnyElement {
        div()
            .id(section.header_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(settings_section_label(section).to_uppercase()),
            )
            .child(Separator::horizontal())
            .into_any_element()
    }

    /// B17: the filter field. The kit's own cleanable affordance clears it — no
    /// hand-rolled Clear button — and the count reads `N of M` exactly like the
    /// library's search count.
    fn render_filter_band(&self, cx: &mut App) -> AnyElement {
        let (shown, total) = self.counted.get();
        div()
            .id("settings-filter-band")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.filter)
                        .id("settings-filter")
                        .appearance(false)
                        .cleanable(true)
                        .aria_label(t!("settings-filter-placeholder")),
                ),
            )
            .child(
                div()
                    .id("settings-filter-count")
                    .test_support()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(search_count_text(shown, total)),
            )
            .into_any_element()
    }

    /// T-04/B17: every section in one column, in fixed order. A section whose
    /// rows all miss the filter takes its header with it, so the column reads as
    /// the filter's result and not as a mostly empty spine.
    fn render_body(&self, _window: &mut Window, cx: &mut App) -> AnyElement {
        self.counted.set((0, 0));
        *self.query.borrow_mut() = self.filter.read(cx).value().trim().to_lowercase();
        let mut column = div()
            .id("settings-body")
            .test_support()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_6();
        for section in SettingsSection::ORDER {
            let before = self.counted.get().0;
            let body = match section {
                SettingsSection::General => self.render_general(cx),
                SettingsSection::Probing => self.render_probing(cx),
                SettingsSection::Updates => self.render_updates(cx),
                SettingsSection::Engine => self.render_engine(cx),
                SettingsSection::Maintenance => self.render_maintenance(cx),
                SettingsSection::Shortcuts => self.render_shortcuts(cx),
            };
            if self.counted.get().0 == before {
                continue;
            }
            column = column.child(self.section_header(section, cx)).child(body);
        }
        if self.counted.get().0 == 0 {
            column = column.child(
                div()
                    .id("settings-filter-empty")
                    .test_support()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("settings-filter-empty")),
            );
        }
        column.into_any_element()
    }

    /// T-04: General — appearance, startup, window behaviour.
    fn render_general(&self, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .id(SettingsSection::General.body_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_4();

        // Appearance — a radio pair that saves on change.
        let appearance_cell = self.appearance.clone();
        let appearance_view = self.view.clone();
        let current = appearance_cell.borrow().clone();
        let selected = APPEARANCE_CHOICES
            .iter()
            .position(|(value, _)| *value == current);
        let mut group = RadioGroup::horizontal("settings-appearance-options")
            .selected_index(selected)
            .on_change(move |ix, window, cx| {
                let Some((value, _)) = APPEARANCE_CHOICES.get(*ix) else {
                    return;
                };
                let value = value.to_string();
                let cell = appearance_cell.clone();
                appearance_view.update(cx, |view, cx| {
                    if view.commit_appearance(&value, window, cx) {
                        *cell.borrow_mut() = value;
                        cx.notify();
                    }
                });
            });
        for (value, _) in APPEARANCE_CHOICES {
            let label = appearance_choice_label(value);
            group = group.child(
                Radio::new(label.clone())
                    .label(label)
                    .tooltip(appearance_choice_tip(value)),
            );
        }
        body = body.child(settings_row(
            self,
            "settings-row-appearance",
            t!("settings-appearance"),
            Some(t!("settings-appearance-help")),
            muted,
            div()
                .id("settings-appearance-group")
                .test_support()
                .child(group),
        ));

        // Task 11: one swatch per preset. A pick saves on click; the selected
        // swatch wears the live focus ring so the current accent reads without
        // a checkmark glyph. Health signal colors never follow this.
        let accent_cell = self.accent.clone();
        let accent_view = self.view.clone();
        let current_accent = *accent_cell.borrow();
        let mode = cx.theme().mode;
        let mut swatches = div()
            .id("settings-accent-swatches")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2();
        for accent in Accent::ALL {
            let selected = accent == current_accent;
            let cell = accent_cell.clone();
            let view = accent_view.clone();
            swatches = swatches.child(
                div()
                    .id(format!("settings-accent-{}", accent.stored()))
                    .test_support()
                    .role(Role::RadioButton)
                    .aria_label(t!("settings-accent-aria", accent = accent_label(accent)))
                    .aria_selected(selected)
                    .tab_stop(true)
                    .size(px(22.))
                    .rounded_full()
                    .bg(crate::appearance::accent_swatch(accent, mode))
                    .border_2()
                    .border_color(if selected { cx.theme().ring } else { muted })
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        let cell = cell.clone();
                        view.update(cx, |view, cx| {
                            if view.commit_accent(accent, window, cx) {
                                *cell.borrow_mut() = accent;
                                cx.notify();
                            }
                        });
                    }),
            );
        }
        body = body.child(settings_row(
            self,
            "settings-row-accent",
            t!("settings-accent"),
            Some(t!("settings-accent-help")),
            muted,
            swatches,
        ));

        // Task 13: one slider from 100 to 150 in 5% steps. The drag previews
        // live; the release persists and toasts once — a drag across the whole
        // band writes one value, not eleven.
        let current_size = *self.text_size.borrow();
        let text_size_control = div()
            .id("settings-text-size-group")
            .test_support()
            .w(px(SETTINGS_CONTROL_WIDTH))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("settings-text-size-slider")
                            .test_support()
                            .flex_1()
                            .child(Slider::new(&self.text_size_slider)),
                    )
                    .child(
                        div()
                            .id("settings-text-size-value")
                            .test_support()
                            .role(Role::Label)
                            .aria_label(text_size_label(current_size))
                            .w(px(42.))
                            .flex_shrink_0()
                            .text_right()
                            .text_sm()
                            .child(text_size_label(current_size)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child(text_size_label(TextSize::DEFAULT))
                    .child(text_size_label(TextSize::LARGE))
                    .child(text_size_label(TextSize::from_percent(TextSize::MAX))),
            );
        body = body.child(settings_row(
            self,
            "settings-row-text-size",
            t!("settings-text-size"),
            Some(t!("settings-text-size-help")),
            muted,
            text_size_control,
        ));

        // R22/R30: the density. One radio pair: comfortable rows are 36px,
        // compact rows are 30px, and the chrome follows.
        let density_cell = self.density.clone();
        let density_view = self.view.clone();
        let current_density = *density_cell.borrow();
        let mut density_group = RadioGroup::horizontal("settings-density-options")
            .selected_index(match current_density {
                Density::Comfortable => Some(0),
                Density::Compact => Some(1),
            })
            .on_change(move |ix, window, cx| {
                let density = match ix {
                    0 => Density::Comfortable,
                    _ => Density::Compact,
                };
                let cell = density_cell.clone();
                density_view.update(cx, |view, cx| {
                    view.set_density(density, window, cx);
                    *cell.borrow_mut() = density;
                    cx.notify();
                });
            });
        for density in [Density::Comfortable, Density::Compact] {
            let label = density_label(density);
            density_group = density_group.child(
                Radio::new(label.clone())
                    .label(label)
                    .tooltip(density_label(density)),
            );
        }
        body = body.child(settings_row(
            self,
            "settings-row-density",
            t!("settings-density"),
            Some(t!("settings-density-help")),
            muted,
            div()
                .id("settings-density-group")
                .test_support()
                .child(density_group),
        ));

        // Launch at login.
        let autostart_cell = self.autostart.clone();
        let autostart_view = self.view.clone();
        let autostart_on = *autostart_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-autostart",
            t!("settings-launch-at-login"),
            Some(t!("settings-launch-at-login-help")),
            muted,
            Switch::new("settings-autostart-switch")
                .checked(autostart_on)
                .accessibility_label(t!("settings-launch-at-login"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = autostart_cell.clone();
                    autostart_view.update(cx, |view, cx| {
                        if view.commit_autostart(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        // Task 9: the manyproxy:// handler is one HKCU key, so the toggle's
        // state is the registration itself — not a second stored flag that
        // could drift from it.
        let link_cell = self.link_handler.clone();
        let link_view = self.view.clone();
        let link_on = *link_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-link-handler",
            t!("settings-handle-links"),
            Some(if link_on {
                t!("settings-handle-links-on")
            } else {
                t!("settings-handle-links-off")
            }),
            muted,
            Switch::new("settings-link-handler-switch")
                .checked(link_on)
                .accessibility_label(t!("settings-handle-links"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = link_cell.clone();
                    link_view.update(cx, |view, cx| {
                        if view.commit_link_handler(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        // Restore on launch.
        let restore_cell = self.restore.clone();
        let restore_view = self.view.clone();
        let restore_on = *restore_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-restore",
            t!("settings-restore"),
            Some(t!("settings-restore-help")),
            muted,
            Switch::new("settings-restore-switch")
                .checked(restore_on)
                .accessibility_label(t!("settings-restore"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = restore_cell.clone();
                    restore_view.update(cx, |view, cx| {
                        if view.commit_restore_opt_in(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        // Window behaviour — the close button's two meanings. With the tray
        // icon off there is no "close to tray" destination, so the row shows
        // the effective Quit and explains where the saved choice went.
        let close_cell = self.close_action.clone();
        let close_view = self.view.clone();
        let close_action = *close_cell.borrow();
        let tray_on = *self.show_tray.borrow();
        let effective_close = if tray_on {
            close_action
        } else {
            CloseAction::Quit
        };
        let mut group = RadioGroup::horizontal("settings-close-action-options")
            .selected_index(match effective_close {
                CloseAction::Tray => Some(0),
                CloseAction::Quit => Some(1),
            })
            .disabled(!tray_on)
            .on_change(move |ix, window, cx| {
                let action = match ix {
                    0 => CloseAction::Tray,
                    1 => CloseAction::Quit,
                    _ => return,
                };
                let cell = close_cell.clone();
                close_view.update(cx, |view, cx| {
                    if view.commit_close_action(action, window, cx) {
                        *cell.borrow_mut() = action;
                        cx.notify();
                    }
                });
            });
        for action in [CloseAction::Tray, CloseAction::Quit] {
            let label = close_action_label(action);
            group = group.child(
                Radio::new(label.clone())
                    .label(label)
                    .tooltip(close_action_explanation(action)),
            );
        }
        body = body.child(settings_row(
            self,
            "settings-row-window-behaviour",
            t!("settings-window-behaviour"),
            Some(if tray_on {
                close_action_explanation(close_action)
            } else {
                t!("settings-window-behaviour-no-tray")
            }),
            muted,
            group,
        ));

        // Task 14: the icon is the one way back from a hidden window, so its
        // switch also owns what closing means — and says so when it goes off.
        let show_tray_cell = self.show_tray.clone();
        let show_tray_view = self.view.clone();
        let show_tray = *show_tray_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-show-tray",
            t!("settings-show-tray"),
            Some(if show_tray {
                t!("settings-show-tray-help-on")
            } else {
                t!("settings-show-tray-help-off")
            }),
            muted,
            Switch::new("settings-show-tray-switch")
                .checked(show_tray)
                .accessibility_label(t!("settings-show-tray"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = show_tray_cell.clone();
                    show_tray_view.update(cx, |view, cx| {
                        if view.commit_show_tray_icon(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        // Task 12: the frame's outer corners are an OS preference, not app
        // paint, so the radio saves and asks DWM; Windows 10 ignores it.
        let corners_cell = self.window_corners.clone();
        let corners_view = self.view.clone();
        let corners = *corners_cell.borrow();
        let mut corner_group = RadioGroup::horizontal("settings-window-corners-options")
            .selected_index(
                WindowCorners::ALL
                    .iter()
                    .position(|choice| *choice == corners),
            )
            .on_change(move |ix, window, cx| {
                let Some(corners) = WindowCorners::ALL.get(*ix).copied() else {
                    return;
                };
                let cell = corners_cell.clone();
                corners_view.update(cx, |view, cx| {
                    if view.commit_window_corners(corners, window, cx) {
                        *cell.borrow_mut() = corners;
                        cx.notify();
                    }
                });
            });
        for choice in WindowCorners::ALL {
            let label = window_corners_label(choice);
            corner_group = corner_group.child(
                Radio::new(label.clone())
                    .label(label)
                    .tooltip(window_corners_explanation(choice)),
            );
        }
        body = body.child(settings_row(
            self,
            "settings-row-window-corners",
            t!("settings-window-corners"),
            Some(window_corners_explanation(corners)),
            muted,
            corner_group,
        ));

        // Task 10: failures that happen while the window is hidden reach the
        // user through a balloon; visible windows already have the toast.
        let notify_cell = self.notify_from_tray.clone();
        let notify_view = self.view.clone();
        let notify_on = *notify_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-notify-from-tray",
            t!("settings-notify-from-tray"),
            Some(t!("settings-notify-from-tray-help")),
            muted,
            Switch::new("settings-notify-from-tray-switch")
                .checked(notify_on)
                .accessibility_label(t!("settings-notify-from-tray"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = notify_cell.clone();
                    notify_view.update(cx, |view, cx| {
                        if view.commit_notify_from_tray(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        // K17 ruling (spec line 91): no blocking delete confirms — the undo
        // toast is the safety net — so this section lost the "Confirm before
        // deleting" toggle; nothing read its state any more.
        body.into_any_element()
    }

    /// T-04: Probing — the URL a probe fetches, how long it may take, and the
    /// rows a "Test all" run covers.
    fn render_probing(&self, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .id(SettingsSection::Probing.body_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_4();

        body = body.child(settings_row(
            self,
            "settings-row-test-url",
            t!("settings-test-url"),
            Some(t!("settings-test-url-help", default = DEFAULT_TEST_URL)),
            muted,
            div()
                .w(px(SETTINGS_CONTROL_WIDTH))
                .id("field-settings-test-url")
                .test_support()
                .child(labelled_input(t!("settings-test-url"), &self.test_url)),
        ));

        body = body.child(settings_row(
            self,
            "settings-row-probe-timeout",
            t!("settings-timeout"),
            Some(t!(
                "settings-timeout-help",
                default = DEFAULT_PROBE_TIMEOUT_SECS
            )),
            muted,
            div()
                .w(px(SETTINGS_CONTROL_WIDTH))
                .id("field-settings-probe-timeout")
                .test_support()
                .child(labelled_input(t!("settings-timeout"), &self.timeout_input)),
        ));

        let scope_cell = self.test_all_scope.clone();
        let scope_view = self.view.clone();
        let scope = *scope_cell.borrow();
        let mut group = RadioGroup::horizontal("settings-test-all-scope-options")
            .selected_index(match scope {
                TestAllScope::Running => Some(0),
                TestAllScope::Checked => Some(1),
            })
            .on_change(move |ix, window, cx| {
                let next = match ix {
                    0 => TestAllScope::Running,
                    1 => TestAllScope::Checked,
                    _ => return,
                };
                let cell = scope_cell.clone();
                scope_view.update(cx, |view, cx| {
                    if view.commit_test_all_scope(next, window, cx) {
                        *cell.borrow_mut() = next;
                        cx.notify();
                    }
                });
            });
        for option in [TestAllScope::Running, TestAllScope::Checked] {
            let label = test_all_scope_label(option);
            group = group.child(
                Radio::new(label.clone())
                    .label(label.clone())
                    .tooltip(label),
            );
        }
        body = body.child(settings_row(
            self,
            "settings-row-test-all-scope",
            t!("settings-test-all-scope"),
            Some(t!("settings-test-all-scope-help")),
            muted,
            group,
        ));

        body.into_any_element()
    }

    /// T-04: Updates — the feed the release check reads, and when it last ran.
    fn render_updates(&self, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .id(SettingsSection::Updates.body_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_4();

        body = body.child(settings_field_row(
            self,
            "settings-row-feed-url",
            t!("settings-update-feed"),
            Some(t!(
                "settings-update-feed-help",
                default = UPDATE_FEED_URL_DEFAULT
            )),
            muted,
            div()
                .w_full()
                .min_w_0()
                .id("field-settings-update-feed")
                .test_support()
                .child(labelled_input(t!("settings-update-feed"), &self.feed_url)),
        ));

        // The passive check is the one automatic feed read; the switch is its
        // off ramp and takes effect on the spot.
        let weekly_cell = self.passive_update_check.clone();
        let weekly_view = self.view.clone();
        let weekly_on = *weekly_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-passive-update-check",
            t!("settings-passive-update-check"),
            Some(t!("settings-passive-update-check-help")),
            muted,
            Switch::new("settings-passive-update-check-switch")
                .checked(weekly_on)
                .accessibility_label(t!("settings-passive-update-check"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = weekly_cell.clone();
                    weekly_view.update(cx, |view, cx| {
                        if view.commit_passive_update_check(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        let last = match *self.last_check.borrow() {
            Some(at) => t!("settings-last-checked-at", time = format_check_time(at)),
            None => t!("settings-never-checked"),
        };
        // K9: `Check now` is a row action, not a commit. It rides beside the
        // timestamp so the section keeps one commit row: the dialog footer.
        let check_view = self.view.clone();
        body = body.child(settings_row(
            self,
            "settings-row-last-check",
            t!("settings-last-checked"),
            None,
            muted,
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().text_sm().text_color(muted).child(last))
                .child(
                    Button::new("settings-check-updates")
                        .small()
                        .ghost()
                        .label(t!("settings-check-now"))
                        .tooltip(t!("settings-check-now-tip"))
                        .on_click(move |_, window, cx| {
                            check_view.update(cx, |view, cx| view.check_for_updates(window, cx));
                        }),
                ),
        ));

        body.into_any_element()
    }

    /// T-04: Engine — the cores beside the app, the crash restart, and the
    /// address list every row's picker reads.
    fn render_engine(&self, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .id(SettingsSection::Engine.body_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_4();

        for (id, name, path) in &self.engines {
            // K9/A7-5: the row is the summary (`sing-box · ready`) and the
            // full path lives behind ONE `Details` tooltip. A path is noise at
            // rest and truncates mid-directory in the row.
            let ready = path.is_some();
            let status_color = if ready {
                muted
            } else {
                danger_text_color(cx.theme())
            };
            body = body.child(
                self.filtered(
                    id,
                    name,
                    div()
                        .id(*id)
                        .test_support()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .min_w_0()
                        .child(
                            div()
                                .id(format!("settings-engine-summary-{name}"))
                                .test_support()
                                .text_sm()
                                .text_color(status_color)
                                .child(engine_summary(name, ready)),
                        )
                        .child(
                            Button::new(format!("settings-engine-details-{name}"))
                                .small()
                                .ghost()
                                .label(t!("settings-engine-details"))
                                .tooltip(engine_details_tooltip(name, path.as_deref())),
                        ),
                ),
            );
        }

        // F4: the durable "where do I get cores" entry point — the status line
        // names the missing core, this names the source.
        let cores_view = self.view.clone();
        body = body.child(
            self.filtered(
                "settings-engine-cores-help-row",
                &t!("settings-cores-help"),
                div()
                    .id("settings-engine-cores-help-row")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("settings-engine-cores-help")
                            .small()
                            .label(t!("settings-cores-help"))
                            .tooltip(t!("settings-cores-help-tip"))
                            .on_click(move |_, window, cx| {
                                cores_view
                                    .update(cx, |view, cx| view.open_cores_help_dialog(window, cx));
                            }),
                    ),
            ),
        );

        let auto_restart_cell = self.auto_restart.clone();
        let auto_restart_view = self.view.clone();
        let auto_restart_on = *auto_restart_cell.borrow();
        body = body.child(settings_row(
            self,
            "settings-row-auto-restart",
            t!("settings-auto-restart"),
            Some(t!("settings-auto-restart-help")),
            muted,
            Switch::new("settings-auto-restart-switch")
                .checked(auto_restart_on)
                .accessibility_label(t!("settings-auto-restart"))
                .on_change(move |next: &bool, window, cx| {
                    let next = *next;
                    let cell = auto_restart_cell.clone();
                    auto_restart_view.update(cx, |view, cx| {
                        if view.commit_auto_restart(next, window, cx) {
                            *cell.borrow_mut() = next;
                            cx.notify();
                        }
                    });
                }),
        ));

        // Bind addresses — the list the row pickers read.
        body = body.child(
            div()
                .id("settings-section-addresses")
                .test_support()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().child(t!("settings-bind-addresses")))
                .child(
                    div()
                        .id("settings-addresses-note")
                        .test_support()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("settings-bind-addresses-note")),
                )
                .child(self.render_bind_hosts(cx))
                .child(
                    div()
                        .id("settings-bind-host-input-row")
                        .test_support()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("field-settings-bind-host")
                                .test_support()
                                .w(px(SETTINGS_CONTROL_WIDTH))
                                .child(labelled_input(
                                    t!("settings-bind-host"),
                                    &self.bind_host_input,
                                )),
                        )
                        .child(self.render_add_bind_host()),
                )
                .child({
                    let error_text = self.bind_host_error.borrow().clone().unwrap_or_default();
                    div()
                        .id("settings-bind-host-error")
                        .test_support()
                        .text_sm()
                        .text_color(danger_text_color(cx.theme()))
                        .child(error_text)
                }),
        );

        body.into_any_element()
    }

    /// T-04: the bind-address list, one row per address.
    fn render_bind_hosts(&self, cx: &mut App) -> AnyElement {
        let mut list = div()
            .id("settings-bind-host-list")
            .test_support()
            .flex()
            .flex_col()
            .gap_1();
        for host in self.bind_hosts.borrow().clone() {
            let constant = Store::BIND_HOST_CONSTANTS.contains(&host.as_str());
            let row_id = format!("settings-bind-host-{host}");
            let mut host_row = div()
                .id(row_id.clone())
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id(format!("settings-bind-host-value-{host}"))
                        .test_support()
                        .child(host.clone()),
                );
            if constant {
                host_row = host_row.child(
                    div()
                        .id(format!("settings-bind-host-builtin-{host}"))
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("settings-built-in")),
                );
            } else {
                let remove_view = self.view.clone();
                let remove_host = host.clone();
                let remove_cell = self.bind_hosts.clone();
                let remove_error = self.bind_host_error.clone();
                host_row = host_row.child(
                    Button::new(format!("settings-bind-host-remove-{host}"))
                        .small()
                        .label(t!("settings-remove"))
                        .accessibility_label(t!("settings-remove-host", host = host.as_str()))
                        .on_click(move |_, window, cx| {
                            let remove_host = remove_host.clone();
                            let remove_cell = remove_cell.clone();
                            let remove_error = remove_error.clone();
                            remove_view.update(cx, |view, cx| {
                                match view.commit_remove_bind_host(&remove_host, window, cx) {
                                    Ok(hosts) => {
                                        *remove_error.borrow_mut() = None;
                                        *remove_cell.borrow_mut() = hosts;
                                    }
                                    Err(message) => {
                                        *remove_error.borrow_mut() = Some(message);
                                    }
                                }
                                cx.notify();
                            });
                        }),
                );
            }
            list = list.child(self.filtered(
                &row_id,
                &t!("settings-bind-address-filter", host = host.as_str()),
                host_row,
            ));
        }
        list.into_any_element()
    }

    /// T-04: the "Add address" button, right-aligned against the field.
    fn render_add_bind_host(&self) -> AnyElement {
        let view = self.view.clone();
        let input = self.bind_host_input.clone();
        let cell = self.bind_hosts.clone();
        let error = self.bind_host_error.clone();
        Button::new("settings-bind-host-add")
            .small()
            .label(t!("settings-add-address"))
            .on_click(move |_, window, cx| {
                let raw = input.read(cx).value().to_string();
                let input = input.clone();
                let cell = cell.clone();
                let error = error.clone();
                view.update(cx, |view, cx| {
                    match view.commit_add_bind_host(&raw, window, cx) {
                        Ok(hosts) => {
                            input
                                .update(cx, |state, cx| state.set_value(String::new(), window, cx));
                            *error.borrow_mut() = None;
                            *cell.borrow_mut() = hosts;
                        }
                        Err(message) => {
                            *error.borrow_mut() = Some(message);
                        }
                    }
                    cx.notify();
                });
            })
            .into_any_element()
    }

    /// T-04: Maintenance — the upkeep actions, and the destructive ones.
    fn render_maintenance(&self, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .id(SettingsSection::Maintenance.body_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_4();

        let view = self.view.clone();
        let folder = self.data_folder.clone();
        body = body.child(settings_row(
            self,
            "settings-row-data-folder",
            t!("settings-data-folder"),
            Some(t!("settings-data-folder-help")),
            muted,
            Button::new("settings-open-data-folder")
                .small()
                .label(t!("settings-open"))
                .tooltip(t!("settings-open-folder", folder = folder))
                .on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.open_data_folder(window, cx));
                }),
        ));

        let view = self.view.clone();
        let logs_path = self.logs_path.clone();
        body = body.child(settings_row(
            self,
            "settings-row-save-logs",
            t!("settings-visible-logs"),
            Some(t!("settings-visible-logs-help")),
            muted,
            Button::new("settings-save-logs")
                .small()
                .label(t!("settings-save-logs"))
                .tooltip(logs_path)
                .on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.save_visible_logs(window, cx));
                }),
        ));

        body = body.child(settings_row(
            self,
            "settings-row-log-cap",
            t!("settings-log-cap"),
            Some(t!("settings-log-cap-help")),
            muted,
            div()
                .w(px(SETTINGS_CONTROL_WIDTH))
                .id("field-log-cap")
                .test_support()
                .child(labelled_input(t!("settings-log-cap"), &self.log_cap_input)),
        ));

        // K9/A7-4: four upkeep ops as one neutral 2×2 grid. The buttons carry
        // their own labels (no orphan `Library` section label), and nothing is
        // painted red at rest — each click opens the naming confirm, whose
        // affirmative carries the danger variant.
        body = body.child(
            div()
                .id("settings-maintenance-note")
                .test_support()
                .text_sm()
                .text_color(muted)
                .child(t!("settings-maintenance-note")),
        );
        let mut grid = div()
            .id("settings-maintenance-grid")
            .test_support()
            .grid()
            .grid_cols(2)
            .gap_2();
        for (id, label, action) in [
            (
                "settings-clear-traffic",
                t!("settings-clear-traffic"),
                MaintenanceAction::Traffic,
            ),
            (
                "settings-clear-health",
                t!("settings-clear-health"),
                MaintenanceAction::Health,
            ),
            (
                "settings-clear-snapshot",
                t!("settings-clear-snapshot"),
                MaintenanceAction::Snapshot,
            ),
        ] {
            let view = self.view.clone();
            grid = grid.child(Button::new(id).small().label(label).w_full().on_click(
                move |_, window, cx| {
                    view.update(cx, |view, cx| match action {
                        MaintenanceAction::Traffic => view.open_clear_traffic_confirm(window, cx),
                        MaintenanceAction::Health => view.open_clear_health_confirm(window, cx),
                        MaintenanceAction::Snapshot => view.open_clear_snapshot_confirm(window, cx),
                    });
                },
            ));
        }
        let view = self.view.clone();
        grid = grid.child(
            Button::new("settings-reset-library")
                .small()
                .label(t!("settings-reset-library"))
                .w_full()
                .on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.open_reset_library_confirm(window, cx));
                }),
        );
        body = body.child(self.filtered(
            "settings-maintenance-grid",
            &t!("settings-maintenance-filter"),
            grid,
        ));

        body.into_any_element()
    }

    /// T-04: Shortcuts — every binding, read from the one list the reference
    /// below the app uses, so the two can never drift apart.
    fn render_shortcuts(&self, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .id(SettingsSection::Shortcuts.body_id())
            .test_support()
            .flex()
            .flex_col()
            .gap_2();
        body = body.child(
            div()
                .id("settings-shortcuts-note")
                .test_support()
                .text_sm()
                .text_color(muted)
                .child(t!("settings-shortcuts-note")),
        );
        let mut list = div()
            .id("settings-shortcuts-list")
            .test_support()
            .flex()
            .flex_col()
            .gap_1();
        for (action, keys) in SHORTCUT_LINES.iter() {
            let action = shortcut_action_label(action);
            let id = format!("settings-shortcut-{}", keys.to_lowercase().replace('.', ""));
            let row = div()
                .id(id.clone())
                .test_support()
                .flex()
                .flex_row()
                .justify_between()
                .gap_4()
                .child(action.clone())
                .child(div().text_sm().text_color(muted).child(*keys));
            list = list.child(self.filtered(&id, &format!("{action} {keys}"), row));
        }
        body.child(list).into_any_element()
    }
}

/// T-04: what the close button does, given the stored choice and whether a
/// tray exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CloseRoute {
    /// Hide the window to the tray; the proxies keep running.
    HideToTray,
    /// Take the quit path: stop every profile and exit.
    Quit,
}

/// T-04: the destructive Maintenance actions, named so the row builder can
/// pick one without a closure per row.
#[derive(Clone, Copy)]
enum MaintenanceAction {
    Traffic,
    Health,
    Snapshot,
}

/// T-04: the data folder the app writes into, as shown in Maintenance.
fn data_folder_text() -> String {
    crate::store::app_data_dir().display().to_string()
}

/// K9/A7-5: the Engine section's whole resting summary — name and resolved
/// state, no path. The path rides behind `Details` so a long install path is
/// never truncated mid-directory in the row.
pub(crate) fn engine_summary(name: &str, ready: bool) -> String {
    if ready {
        t!("settings-engine-ready", name = name)
    } else {
        t!("settings-engine-missing", name = name)
    }
}

/// K9/A7-5: the one `Details` tooltip. A resolved engine names its exe; a
/// missing one states the lookup rule instead of inventing a path.
pub(crate) fn engine_details_tooltip(name: &str, path: Option<&std::path::Path>) -> String {
    match path {
        Some(path) => t!(
            "settings-engine-details-path",
            name = name,
            path = path.display().to_string()
        ),
        None => t!("settings-engine-details-missing", name = name),
    }
}

/// K9: the text fields the one footer `Save` owns. Toggles and radios keep
/// their save-on-change contract; these four commit on Enter or on `Save`.
pub(crate) struct SettingsTextDraft {
    pub(crate) test_url: String,
    pub(crate) probe_timeout: String,
    pub(crate) feed_url: String,
    pub(crate) log_cap: String,
}

/// T-04: a check timestamp, rendered as plain local text. The app never
/// claims a timezone it does not know.
fn format_check_time(at: i64) -> String {
    let seconds = at.rem_euclid(86_400);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    t!(
        "settings-check-time",
        time = format!("{hours:02}:{minutes:02}")
    )
}

impl AppView {
    /// K9: the footer `Save` commits every text field whose value differs from
    /// the stored one and skips the rest, so one Save never fires a toast per
    /// untouched field. A refused value keeps the dialog open.
    pub(crate) fn commit_settings_text_draft(
        &mut self,
        draft: &SettingsTextDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut saved = true;

        let test_url = draft.test_url.trim();
        let stored_test_url = self.store.test_url().unwrap_or_default();
        if test_url != stored_test_url.trim() && !self.commit_test_url(test_url, window, cx) {
            saved = false;
        }

        let timeout = draft.probe_timeout.trim();
        if timeout != self.store.probe_timeout_secs().to_string()
            && !self.commit_probe_timeout(timeout, window, cx)
        {
            saved = false;
        }

        let feed_url = draft.feed_url.trim();
        let stored_feed_url = self.store.update_feed_url().unwrap_or_default();
        if feed_url != stored_feed_url.trim() && !self.commit_update_feed_url(feed_url, window, cx)
        {
            saved = false;
        }

        let log_cap = draft.log_cap.trim();
        if log_cap != self.logs.store.cap().to_string() {
            // `commit_log_cap` toasts its own refusal and returns nothing; the
            // parse is the one failure that must keep the dialog open.
            self.commit_log_cap(window, log_cap, cx);
            if parse_log_cap(log_cap).is_none() {
                saved = false;
            }
        }

        saved
    }

    pub(crate) fn commit_autostart(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match crate::tray::set_autostart_enabled(enabled) {
            Ok(()) => {
                let message = if enabled {
                    t!("autostart-on-toast")
                } else {
                    t!("autostart-off-toast")
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("autostart-failed", error = error)),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn commit_link_handler(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let outcome = if enabled {
            crate::os::scheme::register()
        } else {
            crate::os::scheme::unregister()
        };
        match outcome {
            Ok(()) => {
                let message = if enabled {
                    t!("link-handler-on-toast")
                } else {
                    t!("link-handler-off-toast")
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("link-handler-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn commit_restore_opt_in(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_restore_running(enabled) {
            Ok(()) => {
                let message = if enabled {
                    t!("session-restore-on-toast")
                } else {
                    t!("session-restore-off-toast")
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("restore-setting-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// "Automatically restart crashed cores" setting; a missing or corrupt value
    /// reads as enabled, matching `AutoRestartConfig::default()`.
    pub(crate) fn auto_restart_enabled(&self) -> bool {
        self.store
            .get_meta(META_AUTO_RESTART)
            .map(|value| value.as_deref() != Some("0"))
            .unwrap_or(true)
    }

    /// Pushes the persisted auto-restart choice into the live engine. Called once
    /// at startup so a saved opt-out takes effect before the first crash tick.
    pub(crate) fn apply_engine_auto_restart(&mut self) {
        self.engine.set_auto_restart(AutoRestartConfig {
            enabled: self.auto_restart_enabled(),
            ..AutoRestartConfig::default()
        });
    }

    /// "Notify from the tray"; missing or corrupt reads as on.
    pub(crate) fn notify_from_tray_enabled(&self) -> bool {
        self.store
            .get_meta(META_NOTIFY_FROM_TRAY)
            .map(|value| value.as_deref() != Some("0"))
            .unwrap_or(true)
    }

    pub(crate) fn commit_notify_from_tray(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self
            .store
            .set_meta(META_NOTIFY_FROM_TRAY, if enabled { "1" } else { "0" })
        {
            Ok(()) => {
                let message = if enabled {
                    t!("tray-notify-on-toast")
                } else {
                    t!("tray-notify-off-toast")
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!(
                        "tray-notification-setting-failed",
                        error = error.to_string()
                    )),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// "Check for updates weekly"; missing or corrupt reads as on.
    pub(crate) fn passive_update_check_enabled(&self) -> bool {
        self.store
            .get_meta(META_PASSIVE_UPDATE_CHECK)
            .map(|value| value.as_deref() != Some("0"))
            .unwrap_or(true)
    }

    pub(crate) fn commit_passive_update_check(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self
            .store
            .set_meta(META_PASSIVE_UPDATE_CHECK, if enabled { "1" } else { "0" })
        {
            Ok(()) => {
                if !enabled {
                    // Turning it off is an instruction about the result too:
                    // drop the hint and discard any fetch already in flight.
                    self.passive_update_epoch = self.passive_update_epoch.wrapping_add(1);
                    self.update_available = None;
                }
                let message = if enabled {
                    t!("settings-passive-update-check-on-toast")
                } else {
                    t!("settings-passive-update-check-off-toast")
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!(
                        "settings-passive-update-check-failed",
                        error = error.to_string()
                    )),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn commit_auto_restart(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self
            .store
            .set_meta(META_AUTO_RESTART, if enabled { "1" } else { "0" })
        {
            Ok(()) => {
                self.apply_engine_auto_restart();
                let message = if enabled {
                    t!("auto-restart-on-toast")
                } else {
                    t!("auto-restart-off-toast")
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!(
                        "auto-restart-setting-failed",
                        error = error.to_string()
                    )),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn commit_appearance(
        &mut self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_appearance(value) {
            Ok(()) => {
                apply_appearance(value, self.store.accent(), Some(window), cx);
                apply_text_size(self.store.text_size(), cx);
                window.push_notification(
                    Notification::success(t!("appearance-toast", value = value)),
                    cx,
                );
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("appearance-change-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// Task 11: an accent pick saves and repaints at once. The full appearance
    /// pass re-stamps the accent after `Theme::change` rebuilt the theme, then
    /// restores the reading scale that same rebuild reset.
    pub(crate) fn commit_accent(
        &mut self,
        accent: Accent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_accent(accent) {
            Ok(()) => {
                apply_appearance(&self.store.appearance(), accent, Some(window), cx);
                apply_text_size(self.store.text_size(), cx);
                window.push_notification(
                    Notification::success(t!("accent-toast", label = accent_label(accent))),
                    cx,
                );
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("accent-change-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn commit_text_size(
        &mut self,
        size: TextSize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_text_size(size) {
            Ok(()) => {
                apply_text_size(size, cx);
                window.push_notification(
                    Notification::success(t!("text-size-toast", label = text_size_label(size))),
                    cx,
                );
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("text-size-change-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// T-04: the close button's behaviour. `Tray` keeps the proxies running;
    /// `Quit` stops them and exits.
    pub(crate) fn commit_close_action(
        &mut self,
        action: CloseAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_close_action(action) {
            Ok(()) => {
                let message = match action {
                    CloseAction::Tray => t!("close-behaviour-tray-toast"),
                    CloseAction::Quit => t!("close-behaviour-quit-toast"),
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("close-behaviour-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// Task 12: save the frame style and hand it to DWM at once. The attribute
    /// is unknown below Windows 11, so a failure there leaves stock corners and
    /// is not a user error.
    pub(crate) fn commit_window_corners(
        &mut self,
        corners: WindowCorners,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_window_corners(corners) {
            Ok(()) => {
                crate::os::window::apply_corner_preference(window, corners);
                window.push_notification(
                    Notification::success(t!(
                        "window-corners-toast",
                        label = window_corners_label(corners)
                    )),
                    cx,
                );
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("window-corners-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// Task 14: own the tray icon at runtime. Off drops the icon and, by
    /// removing the only way back from a hidden window, makes close mean Quit;
    /// on rebuilds the icon. The stored close choice is untouched, so it
    /// returns with the icon.
    pub(crate) fn commit_show_tray_icon(
        &mut self,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_show_tray_icon(show) {
            Ok(()) => {
                if show {
                    if self.create_tray(window, cx) {
                        window.push_notification(Notification::success(t!("tray-icon-shown")), cx);
                    }
                } else {
                    self.tray = None;
                    if self.store.seen_tray_disable_hint() {
                        window
                            .push_notification(Notification::info(t!("tray-icon-hidden-info")), cx);
                    } else {
                        if let Err(error) = self.store.set_seen_tray_disable_hint(true) {
                            eprintln!("tray disable hint flag failed: {error}");
                        }
                        window.push_notification(
                            Notification::warning(t!("tray-icon-hidden-warning")),
                            cx,
                        );
                    }
                }
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("tray-icon-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// T-04: how long one URL probe may take.
    pub(crate) fn commit_probe_timeout(
        &mut self,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = raw.trim();
        let Ok(secs) = trimmed.parse::<u64>() else {
            window.push_notification(
                Notification::warning(t!(
                    "probe-timeout-invalid",
                    min = MIN_PROBE_TIMEOUT_SECS,
                    max = MAX_PROBE_TIMEOUT_SECS
                )),
                cx,
            );
            return false;
        };
        match self.store.set_probe_timeout_secs(secs) {
            Ok(()) => {
                // T-04: the live prober takes the new budget right away.
                self.prober = Box::new(RealHealthProber::new(std::time::Duration::from_secs(secs)));
                window.push_notification(
                    Notification::success(t!("probe-timeout-set", secs = secs)),
                    cx,
                );
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("probe-timeout-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// T-04: which rows a "Test all" run covers.
    pub(crate) fn commit_test_all_scope(
        &mut self,
        scope: TestAllScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_test_all_scope(scope) {
            Ok(()) => {
                let message = match scope {
                    TestAllScope::Running => t!("test-all-scope-running-toast"),
                    TestAllScope::Checked => t!("test-all-scope-checked-toast"),
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("test-all-scope-failed", error = error.to_string())),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }

    /// T-04: adds one address to the list the row pickers draw from. The store
    /// owns validation (IP literals only, no duplicates); the message it returns
    /// is exactly what the Settings field shows inline.
    pub(crate) fn commit_add_bind_host(
        &mut self,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<String>, String> {
        match self.store.add_bind_host(raw) {
            Ok(hosts) => {
                self.refresh_bind_hosts(cx);
                window.push_notification(
                    Notification::success(t!("bind-host-added", host = raw.trim())),
                    cx,
                );
                Ok(hosts)
            }
            Err(error) => {
                let message = store_error_text(&error);
                window.push_notification(Notification::error(message.clone()), cx);
                Err(message)
            }
        }
    }

    /// T-33: removes one extra. The store refuses constants and any bind a row
    /// still uses; that refusal names the blocking row(s) and lands inline.
    pub(crate) fn commit_remove_bind_host(
        &mut self,
        host: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<String>, String> {
        match self.store.remove_bind_host(host) {
            Ok(hosts) => {
                self.refresh_bind_hosts(cx);
                window.push_notification(
                    Notification::success(t!("bind-host-removed", host = host)),
                    cx,
                );
                Ok(hosts)
            }
            Err(error) => {
                let message = store_error_text(&error);
                window.push_notification(Notification::error(message.clone()), cx);
                Err(message)
            }
        }
    }

    /// T-33: pushes the current list into every picker at once — a Settings edit
    /// is visible in all rows on the next paint.
    pub(crate) fn refresh_bind_hosts(&mut self, cx: &mut Context<Self>) {
        let hosts = self.store.bind_hosts();
        self.libraries.update(cx, |list, cx| {
            list.delegate_mut().set_bind_hosts(hosts);
            cx.notify();
        });
        cx.notify();
    }

    /// T-04: opens the app's data folder in the shell. The path is the app's
    /// own folder — nothing outside it is ever opened.
    pub(crate) fn open_data_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = crate::store::app_data_dir();
        match open_folder_in_shell(&folder) {
            Ok(()) => {
                window.push_notification(
                    Notification::success(t!(
                        "data-folder-opened",
                        path = folder.display().to_string()
                    )),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("data-folder-failed", error = error.to_string())),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// T-04: the naming confirm for the library reset. The dialog names the
    /// library and the number of configurations it would delete.
    pub(crate) fn open_reset_library_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.store.list().map(|rows| rows.len()).unwrap_or(0);
        if count == 0 {
            window.push_notification(Notification::info(t!("library-already-empty")), cx);
            return;
        }
        let view = cx.entity();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text(t!("settings-reset-ok"))
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(t!("settings-reset-title"))
                .description(t!("settings-reset-description", count = count))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_reset_library(window, cx));
                    true
                })
        });
    }

    /// T-04: empties the library. Running profiles are stopped first, so no
    /// sidecar is left behind.
    pub(crate) fn commit_reset_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.store.list().unwrap_or_default();
        let mut removed = 0usize;
        for row in &rows {
            self.engine.stop_one(&row.id);
            match self.store.remove(&row.id) {
                Ok(true) => removed += 1,
                Ok(false) => {}
                Err(error) => {
                    window.push_notification(
                        Notification::error(t!(
                            "reset-remove-failed",
                            name = row.name.as_str(),
                            error = error.to_string()
                        )),
                        cx,
                    );
                    return;
                }
            }
        }
        self.libraries.update(cx, |state, cx| {
            // T-06: the reset clears the list's own selection too, so UIA
            // stops naming a row that no longer exists.
            state.set_selected_index(None, window, cx);
            cx.notify();
        });
        self.clear_selection(window, cx);
        self.reload(window, cx);
        window.push_notification(
            Notification::success(t!("library-reset-done", count = removed)),
            cx,
        );
        cx.notify();
    }

    /// T-04/Task 14: the close button's route. `Tray` hides the window only
    /// when the icon setting is on and a tray actually exists to hide to;
    /// otherwise there is nothing to hide to, so the close takes the quit path.
    pub(crate) fn close_route(&self, has_tray: bool) -> CloseRoute {
        match self.store.close_action() {
            CloseAction::Tray if has_tray && self.store.show_tray_icon() => CloseRoute::HideToTray,
            _ => CloseRoute::Quit,
        }
    }

    /// T-04: the one-time "still running in the tray" hint. It shows exactly
    /// once per library: the flag is written before the toast goes out.
    pub(crate) fn show_tray_hint_once(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.store.seen_tray_hint() {
            return false;
        }
        if let Err(error) = self.store.set_seen_tray_hint(true) {
            eprintln!("tray hint flag failed: {error}");
        }
        window.push_notification(Notification::info(t!("tray-hint")), cx);
        cx.notify();
        true
    }

    /// R22: the density owns every chrome height and the row pitch. The
    /// choice is written on every change, so the next launch reads it back.
    pub(crate) fn set_density(
        &mut self,
        density: Density,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.density = density;
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_density(density);
            list_cx.notify();
        });
        if let Err(error) = self.store.set_row_density(density) {
            window.push_notification(
                Notification::error(t!("density-save-failed", error = error.to_string())),
                cx,
            );
        }
        cx.notify();
    }

    /// The palette's `Density: Comfortable`.
    pub(crate) fn set_comfortable_density(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_density(Density::Comfortable, window, cx);
    }

    /// The palette's `Density: Compact`.
    pub(crate) fn set_compact_density(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_density(Density::Compact, window, cx);
    }

    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();

        let autostart_state = Rc::new(RefCell::new(crate::tray::autostart_enabled()));
        let link_handler_state = Rc::new(RefCell::new(crate::os::scheme::is_registered()));
        let restore_state = Rc::new(RefCell::new(self.store.restore_running()));
        let auto_restart_state = Rc::new(RefCell::new(self.engine.auto_restart().enabled));
        let notify_from_tray_state = Rc::new(RefCell::new(self.notify_from_tray_enabled()));
        let passive_update_check_state = Rc::new(RefCell::new(self.passive_update_check_enabled()));
        let appearance_state = Rc::new(RefCell::new(self.store.appearance()));
        let accent_state = Rc::new(RefCell::new(self.store.accent()));
        let text_size_state = Rc::new(RefCell::new(self.store.text_size()));
        let density_state = Rc::new(RefCell::new(self.store.row_density()));
        let text_size_slider = cx.new(|_| {
            SliderState::new()
                .min(TextSize::MIN as f32)
                .max(TextSize::MAX as f32)
                .step(TextSize::STEP as f32)
                .default_value(self.store.text_size().percent() as f32)
        });
        let close_action_state = Rc::new(RefCell::new(self.store.close_action()));
        let show_tray_state = Rc::new(RefCell::new(self.store.show_tray_icon()));
        let window_corners_state = Rc::new(RefCell::new(self.store.window_corners()));
        let test_all_scope_state = Rc::new(RefCell::new(self.store.test_all_scope()));
        let last_check_state = Rc::new(RefCell::new(self.store.last_update_check()));

        // K9/A7-5 + T-04: the three cores and the two folder paths are read
        // once, here. The dialog rebuilds its whole column on every frame, and
        // these are the answers that cannot change while it is open — resolving
        // them in the paint meant a dozen `current_exe`/`PATH`/`stat` syscalls
        // per frame.
        let engines = vec![
            (
                "settings-row-engine-sing-box",
                "sing-box",
                resolve_engine_binary().ok(),
            ),
            (
                "settings-row-engine-sing-box-awg",
                "sing-box-awg",
                resolve_fork_binary().ok(),
            ),
            (
                "settings-row-engine-xray",
                "xray",
                resolve_xray_binary().ok(),
            ),
        ];

        let current_test_url = self.store.test_url().unwrap_or_default();
        let test_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("settings-url-placeholder"))
                .default_value(current_test_url)
        });
        let current_feed_url = self.store.update_feed_url().unwrap_or_default();
        let feed_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("settings-url-placeholder"))
                .default_value(current_feed_url)
        });
        let timeout_text = self.store.probe_timeout_secs().to_string();
        let timeout_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("settings-timeout-placeholder"))
                .default_value(timeout_text)
        });
        let cap_text = self.logs.store.cap().to_string();
        let cap_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("settings-log-cap-placeholder"))
                .default_value(cap_text)
        });
        self.logs.cap_input = Some(cap_state.clone());
        let bind_host_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("settings-bind-host-placeholder"))
        });
        // B17: the spine's filter. It commits nothing — it narrows the column
        // while it is typed and leaves every stored value alone.
        let filter_state =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("settings-filter-placeholder")));

        // T-04/K9: Enter commits a text field; the dialog's one footer `Save`
        // commits the same value. A field never commits on blur.
        let weak = cx.entity().downgrade();
        let mut subscriptions = Vec::new();
        {
            let emitter = test_url_state.clone();
            let weak = weak.clone();
            subscriptions.push(window.subscribe(
                &test_url_state,
                cx,
                move |emitter, event, window, cx| {
                    if !matches!(event, InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let url = emitter.read(cx).value().to_string();
                    let _ = weak.update(cx, |view, cx| view.commit_test_url(&url, window, cx));
                },
            ));
            let _ = emitter;
        }
        {
            let weak = weak.clone();
            subscriptions.push(window.subscribe(
                &feed_url_state,
                cx,
                move |emitter, event, window, cx| {
                    if !matches!(event, InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let url = emitter.read(cx).value().to_string();
                    let _ =
                        weak.update(cx, |view, cx| view.commit_update_feed_url(&url, window, cx));
                },
            ));
        }
        {
            let weak = weak.clone();
            subscriptions.push(window.subscribe(
                &timeout_state,
                cx,
                move |emitter, event, window, cx| {
                    if !matches!(event, InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let raw = emitter.read(cx).value().to_string();
                    let _ = weak.update(cx, |view, cx| view.commit_probe_timeout(&raw, window, cx));
                },
            ));
        }
        {
            let weak = weak.clone();
            subscriptions.push(window.subscribe(
                &cap_state,
                cx,
                move |emitter, event, window, cx| {
                    if !matches!(event, InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let raw = emitter.read(cx).value().to_string();
                    let _ = weak.update(cx, |view, cx| view.commit_log_cap(window, &raw, cx));
                },
            ));
        }
        {
            let weak = weak.clone();
            subscriptions.push(window.subscribe(
                &bind_host_input,
                cx,
                move |emitter, event, window, cx| {
                    if !matches!(event, InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let raw = emitter.read(cx).value().to_string();
                    let _ = weak.update(cx, |view, cx| {
                        let _ = view.commit_add_bind_host(&raw, window, cx);
                    });
                },
            ));
        }
        {
            // B17: the column narrows as the filter is typed, so the dialog
            // has to rebuild on a text change — the input repaints itself, the
            // column does not.
            let weak = weak.clone();
            subscriptions.push(window.subscribe(&filter_state, cx, move |_, event, _, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let _ = weak.update(cx, |_, cx| cx.notify());
            }));
        }
        {
            // Task 13: the slider previews on every stop while it moves and
            // commits on release. The cell carries the preview into the next
            // frame so the value readout and the scale labels agree.
            let weak = weak.clone();
            let cell = text_size_state.clone();
            subscriptions.push(window.subscribe(
                &text_size_slider,
                cx,
                move |_, event, window, cx| {
                    let (value, committed) = match event {
                        SliderEvent::Change(value) => (*value, false),
                        SliderEvent::Release(value) => (*value, true),
                    };
                    let size = TextSize::from_percent(value.start().round() as u32);
                    let cell = cell.clone();
                    let _ = weak.update(cx, |view, cx| {
                        let current = *cell.borrow();
                        if committed {
                            if view.commit_text_size(size, window, cx) {
                                *cell.borrow_mut() = size;
                            }
                        } else if size != current {
                            apply_text_size(size, cx);
                            *cell.borrow_mut() = size;
                        }
                        cx.notify();
                    });
                },
            ));
        }
        self.settings_subscriptions = subscriptions;
        self.settings_last_check = Some(last_check_state.clone());

        let bind_hosts_cell = Rc::new(RefCell::new(self.store.bind_hosts()));
        let bind_host_error_cell: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        // K9: the one footer `Save` reads every text field the dialog owns.
        let save_view = view.clone();
        let save_test_url = test_url_state.clone();
        let save_feed_url = feed_url_state.clone();
        let save_timeout = timeout_state.clone();
        let save_log_cap = cap_state.clone();

        let state = SettingsState {
            view: view.clone(),
            appearance: appearance_state.clone(),
            accent: accent_state.clone(),
            text_size: text_size_state.clone(),
            density: density_state.clone(),
            text_size_slider: text_size_slider.clone(),
            autostart: autostart_state.clone(),
            link_handler: link_handler_state.clone(),
            restore: restore_state.clone(),
            auto_restart: auto_restart_state.clone(),
            notify_from_tray: notify_from_tray_state.clone(),
            passive_update_check: passive_update_check_state.clone(),
            close_action: close_action_state.clone(),
            show_tray: show_tray_state.clone(),
            window_corners: window_corners_state.clone(),
            test_all_scope: test_all_scope_state.clone(),
            bind_hosts: bind_hosts_cell.clone(),
            bind_host_error: bind_host_error_cell.clone(),
            last_check: last_check_state.clone(),
            test_url: test_url_state.clone(),
            feed_url: feed_url_state.clone(),
            timeout_input: timeout_state.clone(),
            bind_host_input: bind_host_input.clone(),
            log_cap_input: cap_state.clone(),
            filter: filter_state.clone(),
            counted: Rc::new(Cell::new((0, 0))),
            query: Rc::new(RefCell::new(String::new())),
            engines,
            data_folder: data_folder_text(),
            logs_path: logs_save_path_text(),
        };

        open_app_dialog(window, cx, move |dialog: Dialog, window, cx| {
            // B17: the column is built first — it counts what the filter kept —
            // and the band reads that count to paint `N of M`.
            let body = state.render_body(window, cx);
            let band = state.render_filter_band(cx);
            let body_max_height = settings_body_max_height(window);
            dialog
                .title(t!("settings-dialog-title"))
                .w(px(SETTINGS_DIALOG_WIDTH))
                .child(
                    div()
                        .id("settings-shell")
                        .test_support()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .max_h(px(body_max_height))
                        .child(band)
                        .child(body),
                )
                // K9/A7-2: ONE footer. `Cancel` is secondary, `Save` primary,
                // both bottom-right; the Esc note keeps the keyboard route
                // visible. There is no mid-body Save and no `Done`.
                .footer({
                    let save_view = save_view.clone();
                    let save_test_url = save_test_url.clone();
                    let save_feed_url = save_feed_url.clone();
                    let save_timeout = save_timeout.clone();
                    let save_log_cap = save_log_cap.clone();
                    DialogFooter::new()
                        .child(
                            div()
                                .id("settings-footer-esc-note")
                                .test_support()
                                .flex_1()
                                .flex()
                                .items_center()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("settings-footer-esc")),
                        )
                        .child(
                            Button::new("settings-dialog-cancel")
                                .small()
                                .label(t!("dialog-cancel"))
                                .tooltip(t!("settings-cancel-tooltip"))
                                .on_click(|_, window, cx| {
                                    window.close_all_dialogs(cx);
                                    reclaim_shell_focus(window, cx);
                                }),
                        )
                        .child(
                            Button::new("settings-dialog-save")
                                .small()
                                .primary()
                                .label(t!("dialog-save"))
                                .tooltip(t!("settings-save-tooltip"))
                                .on_click(move |_, window, cx| {
                                    let draft = SettingsTextDraft {
                                        test_url: save_test_url.read(cx).value().to_string(),
                                        probe_timeout: save_timeout.read(cx).value().to_string(),
                                        feed_url: save_feed_url.read(cx).value().to_string(),
                                        log_cap: save_log_cap.read(cx).value().to_string(),
                                    };
                                    let saved = save_view.update(cx, |view, cx| {
                                        view.commit_settings_text_draft(&draft, window, cx)
                                    });
                                    if saved {
                                        window.close_all_dialogs(cx);
                                    }
                                    reclaim_shell_focus(window, cx);
                                }),
                        )
                })
        });
    }
}

/// T-04: opens a folder in the shell on the user's explicit click. Only the
/// app's own data folder is ever opened; the app never navigates anywhere on
/// its own.
#[cfg(windows)]
fn open_folder_in_shell(folder: &std::path::Path) -> Result<(), String> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt as _;
    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(once(0))
            .collect()
    }
    let operation = wide("open");
    let file = wide(&folder.to_string_lossy());
    let instance = unsafe {
        crate::ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    };
    if instance > 32 {
        Ok(())
    } else {
        Err(t!("shell-refused-folder", instance = instance))
    }
}

#[cfg(not(windows))]
fn open_folder_in_shell(_folder: &std::path::Path) -> Result<(), String> {
    Err(t!("folder-open-unsupported"))
}
