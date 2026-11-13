//! Bulk actions over the selection: bar rendering and start/copy/delete commits.
//! Belongs: multi-row actions and the bar that exposes them.
//! Not here: selection or single-row inspector actions.
use crate::*;

/// One menu row for the batch bar's overflow. The row carries its own stable
/// id so the menu contents are provable in the completed frame; Delete is the
/// only danger row.
fn batch_menu_item(
    id: &'static str,
    label: String,
    danger: bool,
    view: Entity<AppView>,
    act: impl Fn(&mut AppView, &mut Window, &mut Context<AppView>) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, cx| {
        let row = div()
            .id(id)
            .test_support()
            .role(Role::MenuItem)
            .aria_label(label.clone())
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(label.clone());
        if danger {
            row.text_color(danger_text_color(cx.theme()))
        } else {
            row
        }
    })
    .on_click(move |_, window, cx| {
        view.update(cx, |view, cx| act(view, window, cx));
    })
}

impl AppView {
    /// The batch bar. It lives in the filter slot, so the table never moves
    /// when it appears. It shows whenever the selection is not empty.
    ///
    /// Left to right: `×`, the count, the running count, the batch's own
    /// result, `Show failures`, the hidden count, then the verbs.
    pub(crate) fn render_batch_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selection_count();
        if selected == 0 {
            return div().id("batch-bar").test_support().into_any_element();
        }
        let view = cx.entity();
        let running = self
            .selection
            .iter()
            .filter(|id| self.engine.is_running(id))
            .count();
        let hidden = self.selection_hidden_count(cx);
        let batch = self.batch.clone();

        let mut bar = div()
            .id("batch-bar")
            .test_support()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_2()
            .w_full()
            .h_full()
            .min_w_0()
            .px_2()
            .child(
                Button::new("batch-clear")
                    .small()
                    .compact()
                    .ghost()
                    .label("×")
                    .accessibility_label(t!("selection-clear"))
                    .tooltip(t!("selection-clear"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.clear_selection(window, cx);
                    })),
            )
            .child(
                div()
                    .id("batch-count")
                    .test_support()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .aria_label(t!("selection-count", count = selected))
                    .child(t!("selection-count", count = selected)),
            );
        if running > 0 {
            bar = bar.child(
                div()
                    .id("batch-running")
                    .test_support()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("selection-running", count = running)),
            );
        }
        if let Some(batch) = &batch {
            let status = if batch.done < batch.total() {
                match batch.kind {
                    BatchKind::Start => {
                        t!("batch-starting", done = batch.done, total = batch.total())
                    }
                    BatchKind::Stop => {
                        t!("batch-stopping", done = batch.done, total = batch.total())
                    }
                    BatchKind::Test => {
                        t!("batch-testing", done = batch.done, total = batch.total())
                    }
                }
            } else {
                t!("batch-result", ok = batch.ok, failed = batch.failed.len())
            };
            bar = bar.child(
                div()
                    .id("batch-status")
                    .test_support()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            );
            if !batch.failed.is_empty() {
                let failures = batch.failed_ids();
                let fail_view = view.clone();
                bar = bar.child(
                    Button::new("batch-show-failures")
                        .small()
                        .compact()
                        .label(t!("batch-show-failures"))
                        .tooltip(t!("batch-show-failures-tip"))
                        .on_click(move |_, window, cx| {
                            fail_view.update(cx, |view, cx| {
                                view.show_batch_failures(&failures, window, cx);
                            });
                        }),
                );
            }
        }
        if hidden > 0 {
            let hide_view = view.clone();
            bar = bar.child(
                Button::new("batch-hidden")
                    .small()
                    .compact()
                    .label(t!("selection-hidden", count = hidden))
                    .tooltip(t!("selection-hidden-tip"))
                    .on_click(move |_, window, cx| {
                        hide_view.update(cx, |view, cx| {
                            view.reveal_hidden_selection(window, cx);
                        });
                    }),
            );
        }
        bar = bar
            .child(div().flex_1())
            .child(
                Button::new("batch-start")
                    .small()
                    .compact()
                    .label(t!("action-label-start"))
                    .accessibility_label(t!("action-label-start"))
                    .disabled(self.batch_running())
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.start_selected_batch(window, cx);
                    })),
            )
            .child(
                Button::new("batch-stop")
                    .small()
                    .compact()
                    .label(t!("action-label-stop"))
                    .accessibility_label(t!("action-label-stop"))
                    .disabled(self.batch_running())
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.stop_selected_batch(window, cx);
                    })),
            )
            .child(
                Button::new("batch-test")
                    .small()
                    .compact()
                    .label(t!("action-label-test"))
                    .accessibility_label(t!("action-label-test"))
                    .disabled(self.batch_running())
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.test_selected_batch(window, cx);
                    })),
            )
            .child(
                div()
                    .id("batch-divider")
                    .test_support()
                    .w(px(1.))
                    .h(px(16.))
                    .bg(cx.theme().border),
            )
            .child(
                Button::new("batch-delete")
                    .small()
                    .compact()
                    .label(t!("action-label-delete"))
                    .accessibility_label(t!("action-label-delete"))
                    .disabled(self.batch_running())
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.delete_selected_batch(window, cx);
                    })),
            )
            .child(self.render_batch_more(view));
        bar.into_any_element()
    }

    /// The batch bar's overflow. It is a popover, never a second bar.
    fn render_batch_more(&self, view: Entity<AppView>) -> AnyElement {
        let enabled = self
            .selection
            .iter()
            .all(|id| self.row_config(id).is_some_and(|row| row.enabled));
        let copy_view = view.clone();
        let move_view = view.clone();
        let preset_view = view.clone();
        let reset_view = view.clone();
        let delete_view = view.clone();
        let toggle_view = view.clone();
        Button::new("batch-more")
            .small()
            .compact()
            .label("⋯")
            .dropdown_caret(true)
            .accessibility_label(t!("batch-more-aria"))
            .tooltip(t!("batch-more-tip"))
            .dropdown_menu(move |menu, _, _| {
                menu.item(batch_menu_item(
                    "batch-more-copy",
                    t!("batch-more-copy"),
                    false,
                    copy_view.clone(),
                    |view, window, cx| {
                        let ids = view.selected_ids_in_visible_order(cx);
                        view.copy_selected_urls(&ids, window, cx);
                    },
                ))
                .item(batch_menu_item(
                    "batch-more-toggle",
                    if enabled {
                        t!("batch-more-dont-start")
                    } else {
                        t!("batch-more-start")
                    },
                    false,
                    toggle_view.clone(),
                    |view, window, cx| view.toggle_selection_enabled(window, cx),
                ))
                .item(batch_menu_item(
                    "batch-more-move",
                    t!("batch-more-move"),
                    false,
                    move_view.clone(),
                    |view, window, cx| view.open_bulk_move_dialog(window, cx),
                ))
                .item(batch_menu_item(
                    "batch-more-preset",
                    t!("batch-more-preset"),
                    false,
                    preset_view.clone(),
                    |view, window, cx| view.open_preset_attach_for_selection(window, cx),
                ))
                .separator()
                .item(batch_menu_item(
                    "batch-more-reset-traffic",
                    t!("action-label-reset-traffic"),
                    false,
                    reset_view.clone(),
                    |view, window, cx| view.reset_traffic_for_selection(window, cx),
                ))
                .item(batch_menu_item(
                    "batch-more-reset-usage",
                    t!("action-label-reset-usage"),
                    false,
                    reset_view.clone(),
                    |view, window, cx| view.reset_usage_for_selection(window, cx),
                ))
                .separator()
                .item(batch_menu_item(
                    "batch-more-delete",
                    t!("action-label-delete"),
                    true,
                    delete_view.clone(),
                    |view, window, cx| view.delete_selected_batch(window, cx),
                ))
            })
            .into_any_element()
    }

    /// Whether a batch is still running. The bar's own verbs wait for it.
    pub(crate) fn batch_running(&self) -> bool {
        self.batch
            .as_ref()
            .is_some_and(|batch| batch.done < batch.total())
    }

    /// Settle a Start batch from the engine's applied set. An id the snapshot
    /// holds but the engine did not apply counts as failed — dirty, unreadable,
    /// refused or skipped, the row is not serving.
    pub(crate) fn settle_batch_start(&mut self, applied: &[String]) {
        let Some(batch) = self.batch.as_mut() else {
            return;
        };
        if batch.kind != BatchKind::Start || batch.is_finished() {
            return;
        }
        batch.failed = batch
            .ids
            .iter()
            .filter(|id| !applied.contains(id))
            .cloned()
            .collect();
        batch.ok = batch.total() - batch.failed.len();
        batch.done = batch.total();
    }

    /// Settle a Stop batch from the engine: every snapshotted row that is no
    /// longer serving counts as stopped, whether it was running or not.
    pub(crate) fn settle_batch_stop(&mut self) {
        let Some(batch) = self.batch.as_ref() else {
            return;
        };
        if batch.kind != BatchKind::Stop || batch.is_finished() {
            return;
        }
        let failed: Vec<String> = batch
            .ids
            .iter()
            .filter(|id| self.engine.is_running(id))
            .cloned()
            .collect();
        if let Some(batch) = self.batch.as_mut() {
            batch.done = batch.total();
            batch.ok = batch.total() - failed.len();
            batch.failed = failed;
        }
    }

    /// Settle a Test batch from the probe's outcomes. A snapshotted row the
    /// probe never sampled counts as failed: it was not tested.
    pub(crate) fn settle_batch_test(&mut self, outcomes: &[TestOutcome]) {
        let Some(batch) = self.batch.as_ref() else {
            return;
        };
        if batch.kind != BatchKind::Test || batch.is_finished() {
            return;
        }
        let failed: Vec<String> = batch
            .ids
            .iter()
            .filter(|id| {
                !outcomes.iter().any(|outcome| {
                    &outcome.sample.config_id == *id && outcome.sample.latency_ms.is_some()
                })
            })
            .cloned()
            .collect();
        if let Some(batch) = self.batch.as_mut() {
            batch.done = batch.total();
            batch.ok = batch.total() - failed.len();
            batch.failed = failed;
        }
    }

    /// The batch bar's `Start with ManyProxy` / `Don't start with ManyProxy`:
    /// the enabled flag flips for every selected config.
    pub(crate) fn toggle_selection_enabled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids_in_visible_order(cx);
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let enable = !ids
            .iter()
            .all(|id| self.row_config(id).is_some_and(|row| row.enabled));
        for id in &ids {
            self.commit_enabled_flag(id, enable, window, cx);
        }
    }

    /// The batch bar's `Reset → Traffic`: every selected config's counters.
    pub(crate) fn reset_traffic_for_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.selected_ids_in_visible_order(cx);
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        for id in &ids {
            self.commit_reset_traffic(id, window, cx);
        }
    }

    /// The batch bar's `Reset → Usage`: every selected config's counters.
    pub(crate) fn reset_usage_for_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.selected_ids_in_visible_order(cx);
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        for id in &ids {
            self.commit_usage_reset(id, window, cx);
        }
    }

    /// R29: the table narrows to the ids the last batch failed on. The
    /// ordinary filter controls take it away again.
    pub(crate) fn show_batch_failures(
        &mut self,
        failed: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let failed: HashSet<String> = failed.iter().cloned().collect();
        self.failure_filter = Some(failed);
        self.reload(window, cx);
    }

    /// The hidden count's button: the filter goes away and the first hidden
    /// selected row comes into view. The selection does not change.
    pub(crate) fn reveal_hidden_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible_row_ids(cx);
        let hidden = {
            let delegate = self.libraries.read(cx);
            delegate
                .delegate()
                .row_ids()
                .into_iter()
                .find(|id| self.selection.contains(id) && !visible.contains(id))
        };
        let Some(id) = hidden else {
            return;
        };
        self.clear_query(window, cx);
        let index = self.visible_row_ids(cx).iter().position(|row| row == &id);
        if let Some(index) = index {
            self.libraries.update(cx, |state, list_cx| {
                state.scroll_to_item(
                    IndexPath::new(index),
                    ScrollStrategy::Nearest,
                    window,
                    list_cx,
                );
            });
        }
    }

    /// Escape: the top layer goes first, then the selection.
    pub(crate) fn clear_selection_unless_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if overlay_open(window, cx) {
            return false;
        }
        // T-03: in the narrow drawer the inspector is the top layer — Esc
        // closes it and hands focus back to the list, and the selection
        // survives (one overlay layer per Esc).
        if self.close_inspector(window, cx) {
            return false;
        }
        // Slice 3: field first, then the selection, then the facets.
        let console_filter_focused = self
            .console_filter_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        if console_filter_focused && !self.console.filter.is_empty() {
            self.clear_console_filter(cx);
            return true;
        }
        if !self.query.search.is_empty() {
            self.clear_search(window, cx);
            return true;
        }
        if self.selection_count() > 0 {
            self.clear_selection(window, cx);
            cx.notify();
            return true;
        }
        if self.query.is_active() {
            self.clear_filters(cx);
            return true;
        }
        false
    }

    /// Shift+Up/Down: the focus moves one row and the range from the anchor
    /// to the new focus replaces the set.
    pub(crate) fn extend_selection_by(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.extend_focus(delta, window, cx);
    }

    /// Ctrl+A: select every row the filters keep, without moving the focus.
    pub(crate) fn select_all_visible(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_all_visible_rows(cx);
    }
    /// The batch bar's Start: the selection is snapshotted, and the snapshot
    /// is what runs. Changing the selection mid-flight never changes the
    /// batch.
    pub(crate) fn start_selected_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids_in_visible_order(cx);
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        self.batch = Some(BatchRun::new(BatchKind::Start, ids.clone()));
        cx.notify();
        self.start_selected_rows(&ids, window, cx);
    }

    /// The batch bar's Stop: the same snapshot rule.
    pub(crate) fn stop_selected_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids_in_visible_order(cx);
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        self.batch = Some(BatchRun::new(BatchKind::Stop, ids.clone()));
        cx.notify();
        self.stop_selected_rows(&ids, window, cx);
    }

    /// The batch bar's Test: the same snapshot rule.
    pub(crate) fn test_selected_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids_in_visible_order(cx);
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        self.batch = Some(BatchRun::new(BatchKind::Test, ids.clone()));
        cx.notify();
        self.test_selected_rows(&ids, window, cx);
    }

    pub(crate) fn start_rows(
        &mut self,
        rows: Vec<ProxyConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if rows.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            self.settle_batch_start(&[]);
            return;
        }
        let lan: Vec<(String, String)> = rows
            .iter()
            .filter(|row| requires_lan_confirm(&row.bind_ip))
            .map(|row| (row.name.clone(), row.bind_ip.clone()))
            .collect();
        if !lan.is_empty() {
            let view = cx.entity();
            let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
            let description = lan_confirm_description(&lan);
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let ids = ids.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text(t!("confirm-start")),
                    )
                    .title(lan_confirm_title())
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        let ids = ids.clone();
                        view.update(cx, |view, cx| view.commit_bulk_start_ids(&ids, window, cx));
                        true
                    })
            });
            return;
        }
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        if cfg!(test) {
            self.commit_bulk_start_ids(&ids, window, cx);
        } else {
            self.spawn_commit_bulk_start_ids(ids, window, cx);
        }
    }

    /// The Start action for a row menu and for the selection shortcuts. One
    /// row keeps the single-row path (and its own toasts); several rows share
    /// the batch apply the bulk bar already uses.
    pub(crate) fn start_selected_rows(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let [id] = ids {
            self.start_row(id, window, cx);
            return;
        }
        let rows: Vec<ProxyConfig> = ids.iter().filter_map(|id| self.row_config(id)).collect();
        self.start_rows(rows, window, cx);
    }

    pub(crate) fn commit_bulk_start_ids(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            self.settle_batch_start(&[]);
            return;
        }
        self.start_in_flight = true;
        if let Err(reason) = self.sync_presets_to_engine() {
            self.start_in_flight = false;
            window.push_notification(Notification::error(t!("cannot-start", reason = reason)), cx);
            self.settle_batch_start(&[]);
            return;
        }
        let mut unreadable: Vec<String> = Vec::new();
        let mut configs: Vec<ProxyConfig> = Vec::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for id in ids {
            let config = match self.store.get(id) {
                Ok(Some(config)) => config,
                Ok(None) => continue,
                Err(_) => {
                    unreadable.push(id.clone());
                    continue;
                }
            };
            if let Some(warning) = lan_exposure_warning(&config.name, &config.bind_ip) {
                window.push_notification(Notification::warning(warning), cx);
            }
            names.insert(config.id.clone(), config.name.clone());
            configs.push(config);
        }

        // T-05: ONE additive start for the whole batch. The engine unions
        // the selected rows with what already serves and applies once, so a
        // twenty-row batch neither flaps the engine twenty times nor drops
        // the rows that were already serving.
        let requested = configs.len();
        let outcome = self.engine.start_many_additive(&configs, true);
        let mut flag_failed: Vec<String> = Vec::new();
        for id in &outcome.applied {
            if self.store.set_enabled(id, true).is_err() {
                if let Some(name) = names.get(id) {
                    flag_failed.push(name.clone());
                }
            }
        }
        self.start_in_flight = false;
        self.reload(window, cx);
        self.settle_batch_start(&outcome.applied);
        if !unreadable.is_empty() {
            window.push_notification(
                Notification::warning(bulk_start_unreadable_message(&unreadable)),
                cx,
            );
        }
        if !flag_failed.is_empty() {
            window.push_notification(
                Notification::warning(bulk_start_enable_flag_failed_message(&flag_failed)),
                cx,
            );
        }
        let started = outcome.applied.len();
        let failed: Vec<(String, String)> = outcome
            .failed
            .iter()
            .map(|(name, reason)| (name.clone(), redact_secrets(reason).to_string()))
            .collect();
        match outcome.error.as_deref() {
            // The one apply refused the batch: nothing new started, and the
            // refusal names the row (or the shared port) that caused it.
            Some(reason) => {
                window.push_notification(
                    Notification::error(bulk_start_refused_message(
                        requested,
                        &redact_secrets(reason),
                        &failed,
                    )),
                    cx,
                );
            }
            None if failed.is_empty() => {
                window.push_notification(
                    Notification::success(t!("start-bulk-started", count = started)),
                    cx,
                );
            }
            None => {
                window.push_notification(
                    Notification::error(bulk_start_failed_message(started, &failed)),
                    cx,
                );
            }
        }
    }

    pub(crate) fn spawn_commit_start_selected(
        &mut self,
        id: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let handle = window.window_handle();
        let view = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.commit_start_selected(&id, window, cx);
                });
            });
        })
        .detach();
    }

    pub(crate) fn spawn_commit_start_all(&mut self, window: &Window, cx: &mut Context<Self>) {
        let handle = window.window_handle();
        let view = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| view.commit_start_all(window, cx));
            });
        })
        .detach();
    }

    pub(crate) fn spawn_commit_bulk_start_ids(
        &mut self,
        ids: Vec<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let handle = window.window_handle();
        let view = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.commit_bulk_start_ids(&ids, window, cx);
                });
            });
        })
        .detach();
    }

    pub(crate) fn stop_selected_rows(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let [id] = ids {
            self.stop_row(id, window, cx);
            return;
        }
        self.stop_ids(ids, window, cx);
    }

    pub(crate) fn stop_ids(&mut self, ids: &[String], window: &mut Window, cx: &mut Context<Self>) {
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let mut stopped: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for id in ids {
            let name = self
                .row_config(id)
                .map(|row| row.name)
                .unwrap_or_else(|| id.clone());
            let (was_active, stop_error) = self.engine.stop_one_result(id);
            if !was_active {
                continue;
            }
            if let Some(error) = stop_error {
                failed.push(format!("{name}: {}", redact_secrets(&error.to_string())));
                continue;
            }
            match self.store.set_enabled(id, false) {
                Ok(_) => stopped.push(name),
                Err(error) => failed.push(t!(
                    "stopped-flag-not-saved",
                    name = name,
                    error = error.to_string()
                )),
            }
        }
        self.reload(window, cx);
        self.settle_batch_stop();
        if failed.is_empty() {
            if stopped.is_empty() {
                window.push_notification(Notification::info(t!("stop-bulk-empty")), cx);
            } else {
                window.push_notification(
                    Notification::success(t!("stop-bulk-stopped", count = stopped.len())),
                    cx,
                );
            }
            return;
        }
        if stopped.is_empty() {
            window.push_notification(
                Notification::error(t!(
                    "stop-bulk-failed",
                    count = failed.len(),
                    names = failed.join("; ")
                )),
                cx,
            );
        } else {
            window.push_notification(
                Notification::warning(t!(
                    "stop-bulk-partial",
                    stopped = stopped.len(),
                    failed = failed.len(),
                    names = failed.join("; ")
                )),
                cx,
            );
        }
    }

    /// The signed delete split: one row goes at once, two or more confirm
    /// first. A row a filter hides still counts — the selection is not a
    /// visible-row set.
    pub(crate) fn delete_selected_rows(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ids {
            [] => {
                window.push_notification(Notification::info(no_selection_message()), cx);
            }
            [id] => {
                self.delete_row_immediately(id, window, cx);
            }
            _ => {
                self.open_bulk_delete_confirm(ids, window, cx);
            }
        }
    }

    /// The batch bar's Delete: the same split, on the selection.
    pub(crate) fn delete_selected_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids_in_visible_order(cx);
        self.delete_selected_rows(&ids, window, cx);
    }

    /// One row deletes at once; the Undo toast is the safety net.
    pub(crate) fn delete_row_immediately(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_remove(id, window, cx);
        self.clear_selection(window, cx);
    }

    /// Two or more: one confirm that names the rows, the running count and
    /// the subscription count.
    pub(crate) fn open_bulk_delete_confirm(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| self.row_config(id))
            .map(|row| row.name)
            .collect();
        let running = ids.iter().filter(|id| self.engine.is_running(id)).count();
        let subscribed = ids
            .iter()
            .filter(|id| {
                self.row_config(id)
                    .is_some_and(|row| row.subscription_id.is_some())
            })
            .count();
        let first = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        let extra = names.len().saturating_sub(3);
        let more = if extra > 0 {
            t!("delete-more", count = extra)
        } else {
            String::new()
        };
        let title = t!("delete-title", count = ids.len());
        let description = t!(
            "delete-body",
            names = first,
            more = more,
            running = running,
            subs = subscribed
        );
        let ids_for_commit = ids.to_vec();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let ids = ids_for_commit.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text(t!("action-label-delete"))
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(title.clone())
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_bulk_delete_ids(&ids, window, cx);
                    });
                    true
                })
        });
    }

    /// The confirmed delete: every row goes, and one Undo toast covers them.
    pub(crate) fn commit_bulk_delete_ids(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets: Vec<(String, String)> = ids
            .iter()
            .filter_map(|id| self.row_config(id))
            .map(|row| (row.id, row.name))
            .collect();
        self.commit_bulk_delete(&targets, window, cx);
    }

    pub(crate) fn commit_bulk_delete(
        &mut self,
        targets: &[(String, String)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut removed: Vec<String> = Vec::new();
        let mut undo_rows: Vec<ProxyConfig> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for (id, name) in targets {
            let (was_active, stop_error) = self.engine.stop_one_result(id);
            if was_active && stop_error.is_some() {
                failed.push(name.clone());
                continue;
            }
            // K17: the whole row is captured BEFORE removal — one batch key
            // covers the whole selection through the same stash the
            // single-row delete uses, so expiry stays permanent and Undo
            // restores names + groups at once.
            let captured = self.store.get(id).ok().flatten();
            match self.store.remove(id) {
                Ok(true) => {
                    removed.push(name.clone());
                    if let Some(row) = captured {
                        undo_rows.push(row);
                    }
                }
                _ => failed.push(name.clone()),
            }
        }
        self.clear_selection(window, cx);
        self.reload(window, cx);

        if targets.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let note = bulk_delete_note(&removed, &failed);
        if removed.is_empty() {
            // Nothing was removed: an honest outcome, nothing to undo.
            self.push_toast(note, window, cx);
        } else {
            let batch = next_undo_batch_id();
            stash_undo_rows(undo_rows, batch);
            self.push_delete_undo_toast(batch, note, window, cx);
        }
    }

    pub(crate) fn open_bulk_move_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids_in_visible_order(cx);
        self.open_move_ids_dialog(ids, window, cx);
    }

    pub(crate) fn move_selected_rows(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let [id] = ids {
            self.open_row_move_dialog(id, window, cx);
            return;
        }
        self.open_move_ids_dialog(ids.to_vec(), window, cx);
    }

    fn open_move_ids_dialog(
        &mut self,
        ids: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let group_state = cx.new(|cx| InputState::new(window, cx));
        let view = cx.entity();
        let reader = group_state.clone();
        let ids_for_commit = ids.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let group = reader.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_bulk_move(&ids_for_commit, &group, window, cx)
            })
        });
        let commit_key = commit.clone();
        let count = ids.len();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(t!("bulk-move-title", count = count))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Label::new(t!("field-group")))
                        .child(labelled_input(t!("field-group"), &group_state)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "bulk-move-ok",
                    "bulk-move-cancel",
                    &footer_commit_label("bulk-move"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_row_move_dialog(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning(t!("already-removed")), cx);
            return;
        };
        let ids_for_commit = vec![config.id.clone()];
        let name = config.name.clone();
        let group_state = cx.new(|cx| InputState::new(window, cx));
        let view = cx.entity();
        let reader = group_state.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let group = reader.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_bulk_move(&ids_for_commit, &group, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(t!("bulk-move-one-title", name = name.as_str()))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Label::new(t!("field-group")))
                        .child(labelled_input(t!("field-group"), &group_state)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "row-move-ok",
                    "row-move-cancel",
                    &footer_commit_label("bulk-move"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn commit_bulk_move(
        &mut self,
        ids: &[String],
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.move_to_group(ids, group) {
            Ok(moved) => {
                self.reload(window, cx);

                window.push_notification(
                    Notification::success(t!(
                        "moved-to-group",
                        count = moved,
                        group = group.trim()
                    )),
                    cx,
                );
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("cannot-move", error = error.to_string())),
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn open_groups_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let counts = self.store.group_counts().unwrap_or_default();
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let flagged: HashSet<String> = self
            .store
            .group_skip_flags()
            .unwrap_or_default()
            .into_iter()
            .collect();
        // T-19: auto-clear opt-ins snapshot at sheet open; every toggle
        // re-opens this sheet so the switch always reads the store.
        let auto_flagged: HashSet<String> =
            self.store.auto_clear_enabled_groups().into_iter().collect();

        let rows = group_row_views(
            &counts,
            &subs,
            &|group| flagged.contains(group),
            &|group| auto_flagged.contains(group),
            now_unix(),
        );
        let commit: DialogCommit = Rc::new(|_, _| true);

        open_app_sheet(window, cx, move |sheet: Sheet, _, _| {
            let commit_click = commit.clone();

            let mut form = v_form().small();

            {
                let current_view = view.clone();
                let all_view = view.clone();
                form = form.child(
                    field().label_indent(false).child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("groups-update-current")
                                    .label(t!("group-update-current"))
                                    .tooltip(t!("group-update-current-tip"))
                                    .on_click(move |_, window, cx| {
                                        current_view.update(cx, |view, cx| {
                                            view.update_current_group(window, cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("groups-update-all")
                                    .label(t!("group-update-all"))
                                    .tooltip(t!("group-update-all-tip"))
                                    .on_click(move |_, window, cx| {
                                        all_view.update(cx, |view, cx| {
                                            view.open_update_all_confirm(window, cx);
                                        });
                                    }),
                            ),
                    ),
                );
            }
            if rows.is_empty() {
                form = form.child(field().label_indent(false).child(t!("groups-empty")));
            }
            for row in &rows {
                let rename_view = view.clone();
                let edit_view = view.clone();
                let update_view = view.clone();
                let delete_view = view.clone();
                let rename_group = row.name.clone();
                let edit_group = row.name.clone();
                let update_group = row.name.clone();
                let delete_group = row.name.clone();
                let slug = slugify_group_id(&row.name);
                let mut buttons = div().flex().flex_row().items_center().gap_2().child(
                    Button::new(format!("group-rename-{slug}"))
                        .label(t!("group-rename"))
                        .on_click(move |_, window, cx| {
                            let group = rename_group.clone();
                            rename_view.update(cx, |view, cx| {
                                view.open_group_rename_dialog(&group, window, cx);
                            });
                        }),
                );
                buttons = buttons.child(
                    Button::new(format!("group-edit-{slug}"))
                        .label(t!("group-edit"))
                        .tooltip(t!("group-edit-tip"))
                        .on_click(move |_, window, cx| {
                            let group = edit_group.clone();
                            edit_view.update(cx, |view, cx| {
                                view.open_group_edit_dialog(&group, window, cx);
                            });
                        }),
                );

                if !row.sub_ids.is_empty() {
                    buttons = buttons.child(
                        Button::new(format!("group-update-{slug}"))
                            .label(t!("group-update"))
                            .tooltip(t!("group-update-tip"))
                            .on_click(move |_, window, cx| {
                                let group = update_group.clone();
                                update_view.update(cx, |view, cx| {
                                    view.update_group_subs(&group, window, cx);
                                });
                            }),
                    );
                }
                buttons = buttons.child(
                    Button::new(format!("group-delete-{slug}"))
                        .label(t!("group-delete"))
                        .on_click(move |_, window, cx| {
                            let group = delete_group.clone();
                            delete_view.update(cx, |view, cx| {
                                view.open_group_delete_confirm(&group, window, cx);
                            });
                        }),
                );

                // T-19: the sheet render is a snapshot; the toggle never writes
                // silently — enable opens this switch's own confirm note, and
                // both handlers re-open the sheet to re-read the store.
                let auto_view = view.clone();
                let auto_group = row.name.clone();
                let auto_switch = Switch::new(format!("group-auto-clear-{slug}"))
                    .checked(row.auto_clear)
                    .label(t!("group-auto-clear"))
                    .tooltip(t!("group-auto-clear-tip"))
                    .accessibility_label(t!("group-auto-clear-aria", group = row.name.as_str()))
                    .on_change(move |next: &bool, window, cx| {
                        let group = auto_group.clone();
                        let view = auto_view.clone();
                        if *next {
                            view.update(cx, |view, cx| {
                                view.open_auto_clear_opt_in_confirm(&group, true, window, cx)
                            });
                        } else {
                            view.update(cx, |view, cx| {
                                view.commit_auto_clear_off(&group, window, cx)
                            });
                        }
                    });

                let mut column = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(buttons)
                    .child(
                        div()
                            .id(format!("group-auto-clear-{slug}"))
                            .test_support()
                            .child(auto_switch),
                    )
                    .child(
                        div()
                            .id(format!("group-kind-{slug}"))
                            .test_support()
                            .child(row.kind.clone()),
                    );
                if let Some(quota) = row.quota.clone() {
                    column = column.child(
                        div()
                            .id(format!("group-quota-{slug}"))
                            .test_support()
                            .child(quota),
                    );
                }
                form = form.child(
                    field()
                        .label(format!("{} ({})", row.name, row.count))
                        .child(column),
                );
            }
            sheet
                .title(t!("groups-title"))
                .child(form)
                .size(px(520.))
                .footer(sheet_footer(
                    "groups-dialog-ok",
                    None,
                    &footer_commit_label("groups"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_group_rename_dialog(
        &mut self,
        old: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_name = old.to_string();
        let name_state = cx.new(|cx| InputState::new(window, cx).default_value(old_name.clone()));
        let view = cx.entity();
        let reader = name_state.clone();
        let old_for_commit = old_name.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let new_name = reader.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_rename_group(&old_for_commit, &new_name, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(t!("group-rename-title", name = old_name.as_str()))
                .child(v_form().small().child(dialog_field(
                    "field-group-rename-name",
                    t!("group-new-name"),
                    name_state.clone(),
                )))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "group-rename-ok",
                    "group-rename-cancel",
                    &footer_commit_label("group-rename"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_group_edit_dialog(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_name = group.to_string();
        let name_state = cx.new(|cx| InputState::new(window, cx).default_value(old_name.clone()));
        let skip = Rc::new(RefCell::new(self.store.group_skip_refresh(group)));
        let skip_for_change = skip.clone();
        let skip_for_commit = skip.clone();
        let skip_checked = *skip.borrow();
        let view = cx.entity();
        let reader = name_state.clone();
        let old_for_commit = old_name.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let new_name = reader.read(cx).value().to_string();
            let skip = *skip_for_commit.borrow();
            view.update(cx, |view, cx| {
                view.commit_edit_group(&old_for_commit, &new_name, skip, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(t!("group-edit-title", name = old_name.as_str()))
                .child(
                    v_form()
                        .small()
                        .child(dialog_field(
                            "field-group-edit-name",
                            t!("group-name"),
                            name_state.clone(),
                        ))
                        .child(
                            field().label_indent(false).child(
                                div().id("field-group-edit-skip").test_support().child(
                                    Switch::new("group-edit-skip")
                                        .checked(skip_checked)
                                        .label(t!("group-skip-refresh"))
                                        .tooltip(t!("group-skip-refresh-tip"))
                                        .accessibility_label(t!("group-skip-refresh-aria"))
                                        .on_change({
                                            let skip_for_change = skip_for_change.clone();
                                            move |next: &bool, _, _| {
                                                *skip_for_change.borrow_mut() = *next;
                                            }
                                        }),
                                ),
                            ),
                        ),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "group-edit-ok",
                    "group-edit-cancel",
                    &footer_commit_label("group-edit"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn commit_edit_group(
        &mut self,
        old: &str,
        new: &str,
        skip: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let renamed = new.trim().to_string();
        if renamed != old {
            match self.store.rename_group(old, &renamed) {
                Ok(_) => {}
                Err(error) => {
                    window.push_notification(
                        Notification::error(t!("cannot-rename-group", error = error.to_string())),
                        cx,
                    );
                    return false;
                }
            }
        }
        if let Err(error) = self.store.set_group_skip_refresh(&renamed, skip) {
            window.push_notification(
                Notification::error(t!(
                    "cannot-change-auto-refresh-skip",
                    error = error.to_string()
                )),
                cx,
            );
            return false;
        }
        self.reload(window, cx);

        window.push_notification(
            Notification::success(t!("group-updated", name = renamed)),
            cx,
        );
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        self.open_groups_dialog(window, cx);
        true
    }

    pub(crate) fn commit_rename_group(
        &mut self,
        old: &str,
        new: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.rename_group(old, new) {
            Ok(outcome) if outcome.rows == 0 && outcome.subscriptions == 0 => {
                window.push_notification(
                    Notification::warning(t!("group-holds-no-rows", name = old)),
                    cx,
                );
                true
            }
            Ok(outcome) => {
                let renamed = new.trim().to_string();
                self.reload(window, cx);

                let mut moved = Vec::new();
                if outcome.rows > 0 {
                    moved.push(t!("group-rows-count", count = outcome.rows));
                }
                if outcome.subscriptions > 0 {
                    moved.push(t!("group-subs-count", count = outcome.subscriptions));
                }
                window.push_notification(
                    Notification::success(t!(
                        "group-renamed",
                        old = old,
                        new = renamed,
                        moved = moved.join(", ")
                    )),
                    cx,
                );
                window.close_all_dialogs(cx);
                reclaim_shell_focus(window, cx);
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("cannot-rename-group", error = error.to_string())),
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn commit_delete_group(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.delete_group(name) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(t!("group-deleted-empty", name = name)),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("cannot-delete-group", error = error.to_string())),
                    cx,
                );
            }
        }
    }

    pub(crate) fn open_group_delete_confirm(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let group = name.to_string();
        let count = self
            .store
            .group_counts()
            .unwrap_or_default()
            .iter()
            .find(|(label, _)| label == name)
            .map(|(_, count)| *count)
            .unwrap_or(0);
        let subscriptions = self
            .store
            .list_subscriptions()
            .unwrap_or_default()
            .iter()
            .filter(|sub| sub.group == name)
            .count();
        let description = group_delete_confirm_description(name, count, subscriptions);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let group = group.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text(t!("group-delete"))
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(t!("group-delete-title"))
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_delete_group(&group, window, cx);

                        window.close_dialog(cx);
                        window.close_dialog(cx);
                        view.open_groups_dialog(window, cx);
                        reclaim_shell_focus(window, cx);
                    });
                    false
                })
        });
    }
}
