//! The Settings surface: sections, rows, data-folder and shortcut text.
//! Belongs: settings layout and the view state it edits.
//! Not here: dialog chrome or unrelated feature panels.
use crate::*;
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

    /// The section's name — the column header.
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
    cx: &App,
    id: &'static str,
    label: &'static str,
    help: Option<String>,
    muted: Hsla,
    control: impl IntoElement,
) -> impl IntoElement {
    let shown = state.note_row(cx, id, label);
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
                .gap_4()
                .min_w_0()
                .child(
                    div()
                        .id(format!("{id}-label"))
                        .test_support()
                        .w(px(SETTINGS_LABEL_WIDTH))
                        .flex_shrink_0()
                        .child(label),
                )
                .child(div().flex_1().min_w_0().justify_end().child(control)),
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
    cx: &App,
    id: &'static str,
    label: &'static str,
    help: Option<String>,
    muted: Hsla,
    control: impl IntoElement,
) -> impl IntoElement {
    let shown = state.note_row(cx, id, label);
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
    autostart: Rc<RefCell<bool>>,
    restore: Rc<RefCell<bool>>,
    auto_restart: Rc<RefCell<bool>>,
    close_action: Rc<RefCell<CloseAction>>,
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
}

impl SettingsState {
    /// B17: the filter query, trimmed and lowercased.
    fn query(&self, cx: &App) -> String {
        self.filter.read(cx).value().trim().to_lowercase()
    }

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
    fn note_row(&self, cx: &App, id: &str, label: &str) -> bool {
        let shown = Self::row_matches(&self.query(cx), id, label);
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
    fn filtered(&self, cx: &App, id: &str, label: &str, element: impl IntoElement) -> AnyElement {
        let shown = self.note_row(cx, id, label);
        div()
            .id(format!("{id}-slot"))
            .when(!shown, |this| this.hidden())
            .child(element)
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
                    .child(section.title().to_uppercase()),
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
                        .cleanable(true),
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
                    .child("No setting matches that — clear the filter to see them all."),
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
        for (value, label) in APPEARANCE_CHOICES {
            let tip = match *value {
                "system" => "Follow the Windows app theme",
                "light" => "Light theme",
                _ => "Dark theme",
            };
            group = group.child(Radio::new(*label).label(label.to_string()).tooltip(tip));
        }
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-appearance",
            "Appearance",
            Some(
                "System follows the Windows app theme; a change paints at once — no restart."
                    .to_string(),
            ),
            muted,
            div()
                .id("settings-appearance-group")
                .test_support()
                .child(group),
        ));

        // Launch at login.
        let autostart_cell = self.autostart.clone();
        let autostart_view = self.view.clone();
        let autostart_on = *autostart_cell.borrow();
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-autostart",
            "Launch at login",
            Some(
                "Writes an HKCU Run entry for this exe; turning it off removes the entry."
                    .to_string(),
            ),
            muted,
            Switch::new("settings-autostart-switch")
                .checked(autostart_on)
                .accessibility_label("Launch at login")
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

        // Restore on launch.
        let restore_cell = self.restore.clone();
        let restore_view = self.view.clone();
        let restore_on = *restore_cell.borrow();
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-restore",
            "Restore on launch",
            Some("Relauch the rows that were running when ManyProxy last exited.".to_string()),
            muted,
            Switch::new("settings-restore-switch")
                .checked(restore_on)
                .accessibility_label("Restore on launch")
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

        // Window behaviour — the close button's two meanings.
        let close_cell = self.close_action.clone();
        let close_view = self.view.clone();
        let close_action = *close_cell.borrow();
        let mut group = RadioGroup::horizontal("settings-close-action-options")
            .selected_index(match close_action {
                CloseAction::Tray => Some(0),
                CloseAction::Quit => Some(1),
            })
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
            group = group.child(
                Radio::new(action.label())
                    .label(action.label().to_string())
                    .tooltip(action.explanation()),
            );
        }
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-window-behaviour",
            "Window behaviour",
            Some(close_action.explanation().to_string()),
            muted,
            group,
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
            cx,
            "settings-row-test-url",
            "Test URL",
            Some(format!(
                "Probes GET this URL through each endpoint. Default: {DEFAULT_TEST_URL}"
            )),
            muted,
            div()
                .w(px(SETTINGS_CONTROL_WIDTH))
                .id("field-settings-test-url")
                .test_support()
                .child(Input::new(&self.test_url)),
        ));

        body = body.child(settings_row(
            self,
            cx,
            "settings-row-probe-timeout",
            "Timeout",
            Some(format!(
                "Seconds a single probe may take. Default: {DEFAULT_PROBE_TIMEOUT_SECS}."
            )),
            muted,
            div()
                .w(px(SETTINGS_CONTROL_WIDTH))
                .id("field-settings-probe-timeout")
                .test_support()
                .child(Input::new(&self.timeout_input)),
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
            group = group.child(
                Radio::new(option.label())
                    .label(option.label().to_string())
                    .tooltip(option.label()),
            );
        }
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-test-all-scope",
            "Test all scope",
            Some("The rows a Test all run covers.".to_string()),
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

        body = body.child(settings_field_row(self, cx,
            "settings-row-feed-url",
            "Update feed URL",
            Some(format!(
                "A JSON feed of releases; checking only reads it. Default: {UPDATE_FEED_URL_DEFAULT}"
            )),
            muted,
            div()
                .w_full()
                .min_w_0()
                .id("field-settings-update-feed")
                .test_support()
                .child(Input::new(&self.feed_url)),
        ));

        let last = match *self.last_check.borrow() {
            Some(at) => format!("Last checked at {}.", format_check_time(at)),
            None => "Never checked.".to_string(),
        };
        // K9: `Check now` is a row action, not a commit. It rides beside the
        // timestamp so the section keeps one commit row: the dialog footer.
        let check_view = self.view.clone();
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-last-check",
            "Last checked",
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
                        .label("Check now")
                        .tooltip("Read the update feed now")
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

        for (id, name, resolved) in [
            (
                "settings-row-engine-sing-box",
                "sing-box",
                resolve_engine_binary(),
            ),
            (
                "settings-row-engine-sing-box-awg",
                "sing-box-awg",
                resolve_fork_binary(),
            ),
            ("settings-row-engine-xray", "xray", resolve_xray_binary()),
        ] {
            // K9/A7-5: the row is the summary (`sing-box · ready`) and the
            // full path lives behind ONE `Details` tooltip. A path is noise at
            // rest and truncates mid-directory in the row.
            let (ready, path) = match resolved {
                Ok(path) => (true, Some(path)),
                Err(_) => (false, None),
            };
            let status_color = if ready { muted } else { cx.theme().danger };
            body = body.child(
                self.filtered(
                    cx,
                    id,
                    name,
                    div()
                        .id(id)
                        .test_support()
                        .flex()
                        .flex_row()
                        .items_center()
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
                        .child(div().flex_1())
                        .child(
                            Button::new(format!("settings-engine-details-{name}"))
                                .small()
                                .ghost()
                                .label("Details")
                                .tooltip(engine_details_tooltip(name, path.as_deref())),
                        ),
                ),
            );
        }

        // F4: the durable "where do I get cores" entry point — the status line
        // names the missing core, this names the source.
        let cores_view = self.view.clone();
        body = body.child(self.filtered(cx, "settings-engine-cores-help-row", "Where do I get cores?",
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
                        .label("Where do I get cores?")
                        .tooltip(
                            "Opens the guide to installing sing-box, sing-box-awg and xray yourself",
                        )
                        .on_click(move |_, window, cx| {
                            cores_view
                                .update(cx, |view, cx| view.open_cores_help_dialog(window, cx));
                        }),
                ),
        ));

        let auto_restart_cell = self.auto_restart.clone();
        let auto_restart_view = self.view.clone();
        let auto_restart_on = *auto_restart_cell.borrow();
        body = body.child(settings_row(self, cx,
            "settings-row-auto-restart",
            "Restart crashed engines",
            Some(
                "On restarts a crashed core with backoff (up to 3 tries) and tells you when it recovers."
                    .to_string(),
            ),
            muted,
            Switch::new("settings-auto-restart-switch")
                .checked(auto_restart_on)
                .accessibility_label("Restart crashed engines")
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
                .child(div().child("Bind addresses"))
                .child(
                    div()
                        .id("settings-addresses-note")
                        .test_support()
                        .text_sm()
                        .text_color(muted)
                        .child(
                            "Rows pick their serve address from this list. 127.0.0.1 and 0.0.0.0 are built in.",
                        ),
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
                                .child(Input::new(&self.bind_host_input)),
                        )
                        .child(self.render_add_bind_host()),
                )
                .child({
                    let error_text = self
                        .bind_host_error
                        .borrow()
                        .clone()
                        .unwrap_or_default();
                    div()
                        .id("settings-bind-host-error")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().danger)
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
            let mut host_row = div()
                .id(format!("settings-bind-host-{host}"))
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
                        .child("built in"),
                );
            } else {
                let remove_view = self.view.clone();
                let remove_host = host.clone();
                let remove_cell = self.bind_hosts.clone();
                let remove_error = self.bind_host_error.clone();
                host_row = host_row.child(
                    Button::new(format!("settings-bind-host-remove-{host}"))
                        .small()
                        .label("Remove")
                        .accessibility_label(format!("Remove {host}"))
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
                cx,
                &format!("settings-bind-host-{host}"),
                &format!("Bind address {host}"),
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
            .label("Add address")
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
        let folder = data_folder_text();
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-data-folder",
            "Data folder",
            Some("Where the library and the logs live.".to_string()),
            muted,
            Button::new("settings-open-data-folder")
                .small()
                .label("Open")
                .tooltip(format!("Open {folder}"))
                .on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.open_data_folder(window, cx));
                }),
        ));

        let view = self.view.clone();
        let logs_path = logs_save_path_text();
        body = body.child(settings_row(
            self,
            cx,
            "settings-row-save-logs",
            "Visible logs",
            Some("Write the session log to a file.".to_string()),
            muted,
            Button::new("settings-save-logs")
                .small()
                .label("Save logs")
                .tooltip(logs_path)
                .on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.save_visible_logs(window, cx));
                }),
        ));

        body = body.child(settings_row(
            self,
            cx,
            "settings-row-log-cap",
            "Maximum kept log lines",
            Some("500–50,000 lines; saving rebuilds the ring.".to_string()),
            muted,
            div()
                .w(px(SETTINGS_CONTROL_WIDTH))
                .id("field-log-cap")
                .test_support()
                .child(Input::new(&self.log_cap_input)),
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
                .child("Each action asks before it runs; nothing here is undone by Cancel."),
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
                "Clear traffic counters",
                MaintenanceAction::Traffic,
            ),
            (
                "settings-clear-health",
                "Clear health snapshots",
                MaintenanceAction::Health,
            ),
            (
                "settings-clear-snapshot",
                "Reset running snapshot",
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
                .label("Reset library")
                .w_full()
                .on_click(move |_, window, cx| {
                    view.update(cx, |view, cx| view.open_reset_library_confirm(window, cx));
                }),
        );
        body = body.child(self.filtered(
            cx,
            "settings-maintenance-grid",
            "Clear traffic counters, clear health snapshots, clear sessions, reset library",
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
                .child("Every key bound in ManyProxy."),
        );
        let mut list = div()
            .id("settings-shortcuts-list")
            .test_support()
            .flex()
            .flex_col()
            .gap_1();
        for (action, keys) in SHORTCUT_LINES.iter() {
            let id = format!("settings-shortcut-{}", keys.to_lowercase().replace('.', ""));
            let row = div()
                .id(id.clone())
                .test_support()
                .flex()
                .flex_row()
                .justify_between()
                .gap_4()
                .child(div().child(*action))
                .child(div().text_sm().text_color(muted).child(*keys));
            list = list.child(self.filtered(cx, &id, &format!("{action} {keys}"), row));
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
    format!("{name} · {}", if ready { "ready" } else { "not found" })
}

/// K9/A7-5: the one `Details` tooltip. A resolved engine names its exe; a
/// missing one states the lookup rule instead of inventing a path.
pub(crate) fn engine_details_tooltip(name: &str, path: Option<&std::path::Path>) -> String {
    match path {
        Some(path) => format!("{name}: {}", path.display()),
        None => format!(
            "{name} is not beside ManyProxy.exe — engines are read from the app folder only; PATH is never searched."
        ),
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
    format!("{hours:02}:{minutes:02} UTC")
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
                    "Autostart on — ManyProxy starts with Windows (HKCU Run entry written)"
                } else {
                    "Autostart off — Run entry removed"
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Autostart change failed: {error}")),
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
                    "Session restore on — the running set returns on next launch"
                } else {
                    "Session restore off"
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Restore setting failed: {error}")),
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
                    "Auto-restart on — crashed cores restart with backoff (up to 3 tries)"
                } else {
                    "Auto-restart off — crashed cores stay down until you restart them"
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Auto-restart setting failed: {error}")),
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
                apply_appearance(value, Some(window), cx);
                window.push_notification(Notification::success(format!("Appearance: {value}")), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Appearance change failed: {error}")),
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
                    CloseAction::Tray => {
                        "Close behaviour: the window closes and the proxies keep running"
                    }
                    CloseAction::Quit => "Close behaviour: quitting stops every profile and exits",
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Close behaviour change failed: {error}")),
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
                Notification::warning(format!(
                    "The probe timeout must be a whole number of seconds ({MIN_PROBE_TIMEOUT_SECS}–{MAX_PROBE_TIMEOUT_SECS})"
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
                    Notification::success(format!("Probe timeout set to {secs} seconds")),
                    cx,
                );
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Probe timeout change failed: {error}")),
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
                    TestAllScope::Running => "Test all covers every running endpoint",
                    TestAllScope::Checked => "Test all covers the checked endpoints only",
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Test-all scope change failed: {error}")),
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
                window
                    .push_notification(Notification::success(format!("Added {}", raw.trim())), cx);
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
                window.push_notification(Notification::success(format!("Removed {host}")), cx);
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
                    Notification::success(format!("Opened {}", folder.display())),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot open the data folder: {error}")),
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
            window.push_notification(
                Notification::info("The library is already empty — nothing to reset"),
                cx,
            );
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
                        .ok_text("Reset library")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset the library?")
                .description(format!(
                    "This deletes all {count} configuration(s) from the ManyProxy library. It cannot be undone."
                ))
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
                        Notification::error(format!("Cannot remove \"{}\": {error}", row.name)),
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
            state.delegate_mut().clear_checked();
            cx.notify();
        });
        self.reload(window, cx);
        window.push_notification(
            Notification::success(format!(
                "Library reset — {removed} configuration(s) removed"
            )),
            cx,
        );
        cx.notify();
    }

    /// T-04: the close button's route. `Tray` hides the window only when a
    /// tray is there to hide to; without one there is nothing to hide to, so
    /// the close takes the quit path. `Quit` always takes it.
    pub(crate) fn close_route(&self, has_tray: bool) -> CloseRoute {
        match self.store.close_action() {
            CloseAction::Tray if has_tray => CloseRoute::HideToTray,
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
        window.push_notification(
            Notification::info("Still running in the tray — right-click the icon to quit."),
            cx,
        );
        cx.notify();
        true
    }

    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();

        let autostart_state = Rc::new(RefCell::new(crate::tray::autostart_enabled()));
        let restore_state = Rc::new(RefCell::new(self.store.restore_running()));
        let auto_restart_state = Rc::new(RefCell::new(self.engine.auto_restart().enabled));
        let appearance_state = Rc::new(RefCell::new(self.store.appearance()));
        let close_action_state = Rc::new(RefCell::new(self.store.close_action()));
        let test_all_scope_state = Rc::new(RefCell::new(self.store.test_all_scope()));
        let last_check_state = Rc::new(RefCell::new(self.store.last_update_check()));

        apply_appearance(&self.store.appearance(), Some(window), cx);

        let current_test_url = self.store.test_url().unwrap_or_default();
        let test_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://…")
                .default_value(current_test_url)
        });
        let current_feed_url = self.store.update_feed_url().unwrap_or_default();
        let feed_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://…")
                .default_value(current_feed_url)
        });
        let timeout_text = self.store.probe_timeout_secs().to_string();
        let timeout_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("3")
                .default_value(timeout_text)
        });
        let cap_text = self.logs.store.cap().to_string();
        let cap_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("500–50,000")
                .default_value(cap_text)
        });
        self.logs.cap_input = Some(cap_state.clone());
        let bind_host_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("192.168.1.20 or fd00::7"));
        // B17: the spine's filter. It commits nothing — it narrows the column
        // while it is typed and leaves every stored value alone.
        let filter_state = cx.new(|cx| InputState::new(window, cx).placeholder("Filter settings"));

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
            autostart: autostart_state.clone(),
            restore: restore_state.clone(),
            auto_restart: auto_restart_state.clone(),
            close_action: close_action_state.clone(),
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
        };

        open_app_dialog(window, cx, move |dialog: Dialog, window, cx| {
            // B17: the column is built first — it counts what the filter kept —
            // and the band reads that count to paint `N of M`.
            let body = state.render_body(window, cx);
            let band = state.render_filter_band(cx);
            let body_max_height = settings_body_max_height(window);
            dialog
                .title("Settings")
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
                                .child("Esc cancels without saving"),
                        )
                        .child(
                            Button::new("settings-dialog-cancel")
                                .small()
                                .label("Cancel")
                                .tooltip("Close without saving (Esc)")
                                .on_click(|_, window, cx| {
                                    window.close_all_dialogs(cx);
                                    reclaim_shell_focus(window, cx);
                                }),
                        )
                        .child(
                            Button::new("settings-dialog-save")
                                .small()
                                .primary()
                                .label("Save")
                                .tooltip("Save the text fields and close")
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
        Err(format!("the shell refused to open the folder ({instance})"))
    }
}

#[cfg(not(windows))]
fn open_folder_in_shell(_folder: &std::path::Path) -> Result<(), String> {
    Err("opening the data folder is not supported on this platform".to_string())
}
