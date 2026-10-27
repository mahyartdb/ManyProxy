use crate::*;

impl AppView {
    pub(crate) fn finish_update_check(
        &mut self,
        fetched: Result<Option<FetchedRelease>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_check_in_flight = false;
        // T-04: the check happened — remember when, so the Settings → Updates
        // section can say so.
        self.record_update_check();
        cx.notify();
        match update_check_outcome(fetched) {
            Ok(outcome) => self.open_update_check_dialog(outcome, window, cx),
            Err(_why) => {
                // every remaining failure is a plain error notification; K5:
                // the copy says only what failed and never the internals
                window.push_notification(Notification::error(update_feed_failure_message()), cx);
            }
        }
    }

    pub(crate) fn finish_update_check_no_window(
        &mut self,
        _fetched: Result<Option<FetchedRelease>, String>,
        cx: &mut Context<Self>,
    ) {
        self.update_check_in_flight = false;
        cx.notify();
    }

    pub(crate) fn finish_ssh_key_pick(
        &mut self,
        path_entity: Entity<InputState>,
        key_entity: Entity<TextareaState>,
        path: String,
        content: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(content) = content else {
            window.push_notification(Notification::warning(ssh_key_pick_error(&path)), cx);
            return;
        };
        path_entity.update(cx, |state, cx| {
            state.set_value(path.clone(), window, cx);
        });
        key_entity.update(cx, |state, cx| {
            state.set_value(content, window, cx);
        });
        window.push_notification(
            Notification::success(format!("Loaded SSH key from {path}")),
            cx,
        );
    }

    pub(crate) fn finish_extra_exe_pick(
        &mut self,
        path_entity: Entity<InputState>,
        display: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stored = std::path::Path::new(&display)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| display.clone());
        path_entity.update(cx, |state, cx| {
            state.set_value(stored.clone(), window, cx);
        });
        window.push_notification(
            Notification::success(format!("Picked extra-core executable {stored}")),
            cx,
        );
    }

    pub(crate) fn finish_browse_pick(
        &mut self,
        current_text: String,
        current_group: String,
        files: Vec<(String, String)>,
        unreadable: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !unreadable.is_empty() {
            window.push_notification(
                Notification::warning(unreadable_drop_message(&unreadable)),
                cx,
            );
        }
        if files.is_empty() {
            return;
        }
        let (merged, hint, multi) = merge_browse_results(&current_text, &files);
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        self.open_import_dialog_with_files(&merged, &current_group, hint, multi, window, cx);
        // K17/A7-10: the dialog's `Files:` row is the one representation of
        // the picked files — a success toast repeating the same names was the
        // second render. Unreadable files keep their warning toast (they
        // cannot be named by a row that lists only readable ones).
    }

    pub(crate) fn finish_import_parse(
        &mut self,
        epoch: u64,
        batch: crate::import::BatchResult,
        group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.import_parse_in_flight = false;
        if self.import_parse_cancelled(epoch) {
            return;
        }
        if self.finish_import_batch(batch, &group, window, cx) {
            window.close_all_dialogs(cx);
            reclaim_shell_focus(window, cx);
        }
    }

    pub(crate) fn finish_import_parse_no_window(
        &mut self,
        epoch: u64,
        batch: crate::import::BatchResult,
        group: String,
        cx: &mut Context<Self>,
    ) {
        self.import_parse_in_flight = false;
        if self.import_parse_cancelled(epoch) {
            return;
        }
        if commit_batch(&mut self.store, batch, &group).is_ok() {
            self.reload_quiet(cx);
        }
    }

    pub(crate) fn finish_import_batch(
        &mut self,
        batch: crate::import::BatchResult,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match commit_batch(&mut self.store, batch, group) {
            Ok(outcome) => {
                self.reload(window, cx);

                if outcome.imported == 0 && !outcome.failures.is_empty() {
                    window.push_notification(
                        Notification::error(format!(
                            "Import failed: {}",
                            format_import_failures(&outcome.failures)
                        )),
                        cx,
                    );
                    return false;
                }

                let summary =
                    import_success_summary(outcome.imported, outcome.duplicates, &outcome.failures);
                if outcome.failures.is_empty() {
                    window.push_notification(Notification::success(summary), cx);
                } else {
                    window.push_notification(Notification::warning(summary), cx);
                }

                if !outcome.imported_ids.is_empty() {
                    let first = outcome.imported_ids.clone();
                    self.libraries.update(cx, |list_state, list_cx| {
                        list_state.delegate_mut().check_ids(&first);
                        // T-06: selection belongs to the list, not to a
                        // delegate field the kit never sees. Routing it
                        // through the list's own setter is what makes
                        // `ListItem::selected` and `aria_selected` name the
                        // imported row.
                        select_first_visible(list_state, &first, window, list_cx);
                        list_cx.notify();
                    });
                }
                true
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Import failed: {error}")), cx);
                false
            }
        }
    }

    pub(crate) fn finish_subscription_refresh(
        &mut self,
        sub_id: &str,
        label: &str,
        fetched: Result<FetchedSubscription, String>,
        now: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inflight_refreshes.remove(sub_id);
        let named = self.inflight_named_refreshes.remove(sub_id);

        match self.store.get_subscription(sub_id) {
            Ok(None) => {
                self.reload(window, cx);

                self.close_batch_slot(sub_id, window, cx);
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
            Err(error) => {
                self.push_refresh_log(&format!(
                    "error cannot refresh subscription \"{label}\": {error}"
                ));
                self.route_refresh_completion(
                    sub_id,
                    Notification::error(format!("Cannot refresh subscription: {error}")),
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    &[],
                    &[],
                    &[],
                    &[],
                    window,
                    cx,
                );
                return;
            }
            Ok(Some(_)) => {}
        }

        let running: HashSet<String> = self.engine.running_ids().into_iter().collect();
        match fetched {
            Ok(fetched) => {
                match self
                    .store
                    .refresh_with_body_running(sub_id, &fetched.body, now, &running)
                {
                    Ok(outcome) => {
                        self.reload(window, cx);
                        if let Some(error) = outcome.error {
                            self.push_refresh_log(&format!(
                                "error subscription refresh \"{label}\": refresh failed ({error})"
                            ));
                            self.route_refresh_completion(
                                sub_id,
                                Notification::error(format!("{label}: refresh failed ({error})")),
                                0,
                                0,
                                0,
                                1,
                                0,
                                0,
                                &[],
                                &[],
                                &[],
                                &[],
                                window,
                                cx,
                            );
                        } else {
                            let _ = self.store.set_subscription_quota(sub_id, fetched.userinfo);
                            let failed = outcome.failures.len();
                            let (added_names, updated_names, removed_names, kept_in_use_names) =
                                if named {
                                    (
                                        outcome.added_names.as_slice(),
                                        outcome.updated_names.as_slice(),
                                        outcome.removed_names.as_slice(),
                                        outcome.kept_in_use_names.as_slice(),
                                    )
                                } else {
                                    let empty: &[String] = &[];
                                    (empty, empty, empty, empty)
                                };
                            let (warn, summary) = refresh_summary_text(
                                label,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                outcome.skipped,
                                outcome.duplicates,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                            );

                            if warn {
                                self.push_refresh_log(&format!(
                                    "warning subscription refresh {summary}"
                                ));
                            } else {
                                self.push_refresh_log(&format!("subscription refresh {summary}"));
                            }
                            let notice = if warn {
                                Notification::warning(summary)
                            } else {
                                Notification::success(summary)
                            };
                            self.route_refresh_completion(
                                sub_id,
                                notice,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                                window,
                                cx,
                            );
                        }
                    }
                    Err(error) => {
                        if matches!(error, StoreError::NotFound(_)) {
                            self.reload(window, cx);
                            self.close_batch_slot(sub_id, window, cx);
                        } else {
                            self.push_refresh_log(&format!(
                                "error subscription refresh \"{label}\": refresh failed ({error})"
                            ));
                            self.route_refresh_completion(
                                sub_id,
                                Notification::error(format!("{label}: refresh failed ({error})")),
                                0,
                                0,
                                0,
                                1,
                                0,
                                0,
                                &[],
                                &[],
                                &[],
                                &[],
                                window,
                                cx,
                            );
                        }
                    }
                }
            }
            Err(error) => match self.store.record_subscription_error(sub_id, &error) {
                Ok(_) => {
                    self.reload(window, cx);
                    self.push_refresh_log(&format!(
                        "error subscription refresh \"{label}\": {error}"
                    ));
                    self.route_refresh_completion(
                        sub_id,
                        Notification::error(format!("{label}: {error}")),
                        0,
                        0,
                        0,
                        1,
                        0,
                        0,
                        &[],
                        &[],
                        &[],
                        &[],
                        window,
                        cx,
                    );
                }
                Err(StoreError::NotFound(_)) => {
                    self.reload(window, cx);
                    self.close_batch_slot(sub_id, window, cx);
                }
                Err(db_error) => {
                    self.push_refresh_log(&format!(
                        "error cannot refresh subscription \"{label}\": {db_error}"
                    ));
                    self.route_refresh_completion(
                        sub_id,
                        Notification::error(format!("Cannot refresh subscription: {db_error}")),
                        0,
                        0,
                        0,
                        1,
                        0,
                        0,
                        &[],
                        &[],
                        &[],
                        &[],
                        window,
                        cx,
                    );
                }
            },
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    pub(crate) fn finish_subscription_refresh_no_window(
        &mut self,
        sub_id: &str,
        fetched: Result<FetchedSubscription, String>,
        now: i64,
        cx: &mut Context<Self>,
    ) {
        self.inflight_refreshes.remove(sub_id);
        let named = self.inflight_named_refreshes.remove(sub_id);

        let running: HashSet<String> = self.engine.running_ids().into_iter().collect();
        match fetched {
            Ok(fetched) => {
                match self
                    .store
                    .refresh_with_body_running(sub_id, &fetched.body, now, &running)
                {
                    Ok(outcome) => {
                        if outcome.error.is_none() {
                            let _ = self.store.set_subscription_quota(sub_id, fetched.userinfo);
                        }
                        let label = self
                            .store
                            .get_subscription(sub_id)
                            .ok()
                            .flatten()
                            .map(|sub| sub.label)
                            .unwrap_or_else(|| sub_id.to_string());
                        let failed = outcome.failures.len();
                        if let Some(error) = outcome.error {
                            self.push_refresh_log(&format!(
                                "error subscription refresh \"{label}\": refresh failed ({error})"
                            ));
                            self.accumulate_batch_member(
                                sub_id,
                                0,
                                0,
                                0,
                                1,
                                0,
                                0,
                                &[],
                                &[],
                                &[],
                                &[],
                            );
                        } else {
                            let (added_names, updated_names, removed_names, kept_in_use_names) =
                                if named {
                                    (
                                        outcome.added_names.as_slice(),
                                        outcome.updated_names.as_slice(),
                                        outcome.removed_names.as_slice(),
                                        outcome.kept_in_use_names.as_slice(),
                                    )
                                } else {
                                    let empty: &[String] = &[];
                                    (empty, empty, empty, empty)
                                };
                            let (warn, summary) = refresh_summary_text(
                                &label,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                outcome.skipped,
                                outcome.duplicates,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                            );
                            if warn {
                                self.push_refresh_log(&format!(
                                    "warning subscription refresh {summary}"
                                ));
                            } else {
                                self.push_refresh_log(&format!("subscription refresh {summary}"));
                            }
                            self.accumulate_batch_member(
                                sub_id,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                            );
                        }
                    }
                    Err(_) => {
                        self.accumulate_batch_member(sub_id, 0, 0, 0, 1, 0, 0, &[], &[], &[], &[]);
                    }
                }
            }
            Err(error) => {
                let _ = self.store.record_subscription_error(sub_id, &error);
                self.accumulate_batch_member(sub_id, 0, 0, 0, 1, 0, 0, &[], &[], &[], &[]);
            }
        }
        if self.refresh_batch_members.is_empty() {
            self.take_batch_counts();
        }
        self.reload_quiet(cx);
    }

    pub(crate) fn finish_preset_refresh(
        &mut self,
        id: &str,
        name: &str,
        fetched: Result<FetchedSubscription, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let body = match fetched {
            Ok(fetched) => fetched.body,
            Err(reason) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh routing preset: {reason}")),
                    cx,
                );
                return;
            }
        };
        let rules = match crate::preset::parse_preset_json(&body, name) {
            Ok(parsed) => parsed.rules,
            Err(error) => {
                window.push_notification(Notification::error(preset_error_message(&error)), cx);
                return;
            }
        };
        match self.store.set_preset_rules(id, &rules) {
            Ok(preset) => {
                let count = preset.rules().map(|rules| rules.len()).unwrap_or(0);
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Routing preset \"{}\" refreshed ({count} rule(s))",
                        preset.name
                    )),
                    cx,
                );
            }
            Err(StoreError::NotFound(_)) => {
                self.reload(window, cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh routing preset: {error}")),
                    cx,
                );
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    pub(crate) fn finish_preset_refresh_no_window(
        &mut self,
        id: &str,
        fetched: Result<FetchedSubscription, String>,
        cx: &mut Context<Self>,
    ) {
        let Ok(fetched) = fetched else {
            return;
        };
        let body = fetched.body;
        let name = self
            .store
            .get_preset(id)
            .ok()
            .flatten()
            .map(|preset| preset.name)
            .unwrap_or_default();
        let Ok(parsed) = crate::preset::parse_preset_json(&body, &name) else {
            return;
        };
        if self.store.set_preset_rules(id, &parsed.rules).is_ok() {
            self.reload_quiet(cx);
        }
    }

    pub(crate) fn finish_rule_set_download(
        &mut self,
        grant: &RuleSetDownloadGrant,
        fetched: Result<Vec<u8>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.rule_set_download_in_flight = false;
        let bytes = match fetched {
            Ok(bytes) => bytes,
            Err(reason) => {
                window.push_notification(
                    Notification::error(format!(
                        "Cannot download rule set \"{}\" ({}): {reason}",
                        grant.name,
                        crate::import::redact_link(&grant.url),
                    )),
                    cx,
                );
                return false;
            }
        };
        let bytes_len = bytes.len();
        match self.stage_rule_set_bytes(grant, bytes) {
            Ok(path) => {
                window.push_notification(
                    Notification::success(format!(
                        "Rule set \"{}\" staged ({} bytes) — {}",
                        grant.name,
                        bytes_len,
                        crate::health::truncate(&path.display().to_string(), 160),
                    )),
                    cx,
                );
                self.reload(window, cx);
                self.reopen_presets_if_open(window, cx);
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!(
                        "Cannot stage rule set \"{}\" ({}): {error}",
                        grant.name,
                        crate::import::redact_link(&grant.url),
                    )),
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn finish_rule_set_download_no_window(
        &mut self,
        grant: &RuleSetDownloadGrant,
        fetched: Result<Vec<u8>, String>,
        cx: &mut Context<Self>,
    ) {
        self.rule_set_download_in_flight = false;
        if let Ok(bytes) = fetched {
            if self.stage_rule_set_bytes(grant, bytes).is_ok() {
                self.reload_quiet(cx);
            }
        }
    }
}
