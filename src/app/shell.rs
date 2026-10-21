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
                    return;
                }
                view.open_add_dialog(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ImportConfigs, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
            }))
            .on_action(cx.listener(|view, _: &PasteImport, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.open_import_from_clipboard(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StartSelected, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.start_selected_engine(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StopSelected, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.stop_selected_engine(window, cx);
            }))
            .on_action(cx.listener(|view, _: &TestSelected, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.test_selected_health(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StopTesting, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.stop_group_tests(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ClearChecked, window, cx| {
                view.clear_checked_unless_dialog(window, cx);
            }))
            .on_action(cx.listener(|view, _: &OpenPalette, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.open_palette(window, cx);
            }))
            .on_action(cx.listener(|view, _: &CopyUrls, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.copy_selected_urls(window, cx);
            }))
            .on_action(cx.listener(|view, _: &UpdateCurrentGroup, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.update_current_group(window, cx);
            }))
            .on_action(cx.listener(|view, _: &EditSelected, window, cx| {
                if overlay_open(window, cx) {
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
                    return;
                }
                view.focus_library_search(window, cx);
            }))
            .on_action(cx.listener(|view, _: &InspectorDetails, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.set_inspector_tab(InspectorTab::Details);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorHealth, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.set_inspector_tab(InspectorTab::Health);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorPresets, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.set_inspector_tab(InspectorTab::Presets);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorLogs, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.set_inspector_tab(InspectorTab::Logs);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorConnections, window, cx| {
                if overlay_open(window, cx) {
                    return;
                }
                view.set_inspector_tab(InspectorTab::Connections);
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &InspectorTraffic, window, cx| {
                if overlay_open(window, cx) {
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
}
