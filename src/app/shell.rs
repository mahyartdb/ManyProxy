use crate::*;

impl AppView {
    pub(crate) fn render_shell(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let drop_view = cx.entity();
        div()
            .id("manyproxy-shell")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context(APP_KEY_CONTEXT)
            .on_action(cx.listener(|view, _: &AddConfig, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.open_add_dialog(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ImportConfigs, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
            }))
            .on_action(cx.listener(|view, _: &PasteImport, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.open_import_from_clipboard(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StartSelected, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.start_selected_engine(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StopSelected, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.stop_selected_engine(window, cx);
            }))
            .on_action(cx.listener(|view, _: &TestSelected, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.test_selected_health(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StopTesting, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.stop_group_tests(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ClearChecked, window, cx| {
                view.clear_checked_unless_dialog(window, cx);
            }))
            .on_action(cx.listener(|view, _: &OpenPalette, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.open_palette(window, cx);
            }))
            .on_action(cx.listener(|view, _: &CopyUrls, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.copy_selected_urls(window, cx);
            }))
            .on_action(cx.listener(|view, _: &UpdateCurrentGroup, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.update_current_group(window, cx);
            }))
            .on_action(cx.listener(|view, _: &EditSelected, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                if let Some(config) = view.selected_config(cx) {
                    view.open_edit_dialog(config, window, cx);
                } else {
                    window.push_notification(Notification::info(no_selection_message()), cx);
                }
            }))
            .on_action(cx.listener(|view, _: &DeleteSelected, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                if let Some(config) = view.selected_config(cx) {
                    view.open_remove_confirm(config, window, cx);
                } else {
                    window.push_notification(Notification::info(no_selection_message()), cx);
                }
            }))
            .on_action(cx.listener(|view, _: &FocusSearch, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.focus_library_search(window, cx);
            }))
            .on_action(cx.listener(|view, _: &InspectorDetails, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                // F19: Ctrl-1 is the one key that owns presence — it toggles
                // the panel shut when it is already showing Details.
                view.toggle_inspector(InspectorTab::Details);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorHealth, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.set_inspector_tab(InspectorTab::Health);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorLogs, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.set_inspector_tab(InspectorTab::Logs);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorLinks, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.set_inspector_tab(InspectorTab::Links);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorTraffic, window, cx| {
                if overlay_open(window, cx) {
                    refuse_shortcut_under_overlay(window, cx);
                    return;
                }
                view.set_inspector_tab(InspectorTab::Traffic);
                cx.notify();
            }))
            .on_drop(move |paths: &ExternalPaths, window, cx| {
                drop_view.update(cx, |view, cx| {
                    view.import_dropped_files(paths, window, cx);
                });
            })
    }

    /// T-08: the folder the standing banner names. A file-backed store knows
    /// its own folder; a session that fell back to memory carries the folder
    /// the failed open was using; a deliberate in-memory store has none, and
    /// never raises the banner anyway.
    pub(crate) fn writable_folder(&self) -> String {
        self.write_blocked_folder
            .clone()
            .or_else(|| self.store.data_dir())
            .unwrap_or_else(crate::store::app_data_dir)
            .display()
            .to_string()
    }

    /// T-08: the pinned banner sentence, or `None` when the last probe passed.
    /// A failed OPEN names its own cause; the probe path keeps the write
    /// refusal. Both name the folder and both promise the same outcome: this
    /// session does not save.
    pub(crate) fn write_banner_text(&self) -> Option<String> {
        let reason = self.write_blocked.as_ref()?;
        Some(write_banner_sentence(
            &self.writable_folder(),
            self.write_blocked_pinned.then_some(reason.as_str()),
        ))
    }

    /// T-08: the standing not-writable strip. It is a child of the shell's
    /// column — under the toolbar, above the table — so it takes its own height
    /// and the table shrinks. It carries no focus handle and no click-absorbing
    /// layer: the app keeps working, only saving is affected.
    pub(crate) fn render_writable_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let reason = self.write_blocked.as_ref()?;
        let text = self.write_banner_text()?;
        let tip = reason.clone();
        let view = cx.entity();
        Some(
            div()
                .id("write-banner")
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .w_full()
                .px_2()
                .py_1()
                .bg(cx.theme().group_box)
                .border_b_1()
                .border_color(cx.theme().danger)
                .child(
                    div()
                        .id("write-banner-text")
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(cx.theme().danger)
                        .aria_label(text.clone())
                        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                        .child(text),
                )
                .child(
                    Button::new("write-banner-open-folder")
                        .small()
                        .label("Open data folder")
                        .tooltip("Open the folder that holds the library")
                        .on_click(move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                view.open_data_folder(window, cx);
                            });
                        }),
                )
                .into_any_element(),
        )
    }
}

/// T-08: one sentence, pinned by tests. The folder is named because the app can
/// run from any folder; the outcome is named because a silent save is the bug.
/// `open_failure` carries the real reason a store could not open — a corrupt or
/// locked database is not a write refusal — while the probe path keeps the
/// write sentence.
fn write_banner_sentence(folder: &str, open_failure: Option<&str>) -> String {
    // The consequence leads. The strip is one truncated line, and a folder
    // path can be longer than the window: with the promise last, a long path
    // silently cut off the only part that matters. The OS reason rides the
    // tooltip (`write_blocked`), so it is never lost either.
    match open_failure {
        Some(_) => format!("Changes will not be saved — cannot use the data folder ({folder})."),
        None => format!("Changes will not be saved — cannot write to the data folder ({folder})."),
    }
}
