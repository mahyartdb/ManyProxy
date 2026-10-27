use crate::*;

impl AppView {
    #[cfg(test)]
    pub(crate) fn commit_add(
        &mut self,
        input: NewConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Err(reason) = save_listen_gate(&input.name, &input.bind_ip, &input.port) {
            window.push_notification(Notification::error(reason), cx);
            return false;
        }
        match self.store.add(input) {
            Ok(created) => {
                let message = row_action_message("Added", &created.name);
                self.reload(window, cx);
                window.push_notification(Notification::success(message), cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    pub(crate) fn commit_add_with_focus(
        &mut self,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let input = form.as_new(cx);
        if let Err(reason) = save_listen_gate(&input.name, &input.bind_ip, &input.port) {
            *form_error.borrow_mut() = Some(("port".to_string(), reason.clone()));
            form.focus_field("port", window, cx);
            window.push_notification(Notification::error(reason), cx);
            cx.notify();
            return false;
        }

        // F5: name the first blank row in reading order before `store.add`
        // can only blame the folded params JSON.
        let field_gate =
            blank_name(&input.name).or_else(|| guided_blank_field(&input.params, &input.protocol));
        if let Some((field, message)) = field_gate {
            *form_error.borrow_mut() = Some((field.to_string(), message.clone()));
            form.focus_field(field, window, cx);
            window.push_notification(Notification::error(message), cx);
            cx.notify();
            return false;
        }

        if input.protocol == "extra-core" {
            if let Err(reason) = validate_extra_params(&input.params) {
                *form_error.borrow_mut() = Some(("params".to_string(), reason.clone()));
                *form.advanced.borrow_mut() = true;
                form.focus_field("params", window, cx);
                window.push_notification(Notification::error(reason), cx);
                cx.notify();
                return false;
            }
        }
        match self.store.add(input) {
            Ok(created) => {
                *form_error.borrow_mut() = None;
                let message = row_action_message("Added", &created.name);
                let created_id = created.id.clone();
                self.reload(window, cx);
                // F12: the new row is the confirmation. A filter that hides
                // it leaves the previous selection alone.
                self.libraries.update(cx, |list_state, list_cx| {
                    select_first_visible(list_state, &[created_id], window, list_cx);
                    list_cx.notify();
                });
                window.push_notification(Notification::success(message), cx);
                true
            }
            Err(error) => {
                if let Some((field, message)) = field_error_target(&error) {
                    *form_error.borrow_mut() = Some((field.to_string(), message));

                    if field == "params" {
                        *form.advanced.borrow_mut() = true;
                    }
                    form.focus_field(field, window, cx);
                }
                self.report_save_error(error, window, cx);
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn commit_edit(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.get(id) {
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
            Ok(Some(existing)) => {
                let name = patch.name.as_deref().unwrap_or(&existing.name);
                let bind = patch.bind_ip.as_deref().unwrap_or(&existing.bind_ip);
                let port_gate = match patch.port.as_deref() {
                    Some(port_raw) => save_listen_gate(name, bind, port_raw),
                    None if patch.bind_ip.is_some() || patch.name.is_some() => {
                        save_listen_gate(name, bind, &existing.port.to_string())
                    }
                    None => Ok(()),
                };
                if let Err(reason) = port_gate {
                    window.push_notification(Notification::error(reason), cx);
                    return false;
                }
                match self.store.update(id, patch) {
                    Ok(updated) => {
                        let message = if self.engine.is_running(id) {
                            restart_to_apply_message(&updated.name)
                        } else {
                            row_action_message("Updated", &updated.name)
                        };
                        self.reload(window, cx);
                        window.push_notification(Notification::success(message), cx);
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                }
            }
        }
    }

    pub(crate) fn commit_row_endpoint(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (bind_raw, port_raw, stored) = self.libraries.read_with(cx, |list, cx| {
            let delegate = list.delegate();
            let drafts = delegate.endpoint_draft_texts(id, cx).unwrap_or_default();
            let stored = delegate
                .rows
                .iter()
                .find(|row| row.id == id)
                .map(|row| (row.name.clone(), row.bind_ip.clone(), row.port.to_string()));
            (drafts.0, drafts.1, stored)
        });
        let Some((name, bind_stored, port_stored)) = stored else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return true;
        };
        if bind_raw.trim() == bind_stored && port_raw.trim() == port_stored {
            self.libraries.update(cx, |list, _| {
                if let Some(inputs) = list.delegate_mut().endpoint_inputs.get_mut(id) {
                    inputs.clean_bind = bind_raw.clone();
                    inputs.clean_port = port_raw.clone();
                }
            });
            return true;
        }
        if let Err(reason) = save_listen_gate(&name, &bind_raw, &port_raw) {
            window.push_notification(Notification::error(reason), cx);
            return false;
        }
        let patch = ConfigPatch {
            bind_ip: Some(bind_raw),
            port: Some(port_raw),
            ..Default::default()
        };
        let ok = self.commit_edit(id, patch, window, cx);
        if ok {
            if let Ok(Some(fresh)) = self.store.get(id) {
                let bind_fresh = fresh.bind_ip.clone();
                let port_fresh = fresh.port.to_string();
                self.libraries.update(cx, |list, cx| {
                    let delegate = list.delegate_mut();
                    delegate
                        .pending_endpoint_commits
                        .retain(|pending| pending != id);
                    if let Some(inputs) = delegate.endpoint_inputs.get_mut(id) {
                        inputs.bind.update(cx, |state, cx| {
                            state.set_value(bind_fresh.clone(), window, cx);
                        });
                        inputs.port.update(cx, |state, cx| {
                            state.set_value(port_fresh.clone(), window, cx);
                        });
                        inputs.clean_bind = bind_fresh;
                        inputs.clean_port = port_fresh;
                    }
                });
            }
        }
        ok
    }

    pub(crate) fn commit_edit_with_focus(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.get(id) {
            Ok(None) => {
                *form_error.borrow_mut() = None;
                window.push_notification(Notification::warning("Already removed"), cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
            Ok(Some(existing)) => {
                let name = patch.name.as_deref().unwrap_or(&existing.name);
                let bind = patch.bind_ip.as_deref().unwrap_or(&existing.bind_ip);
                let port_gate = match patch.port.as_deref() {
                    Some(port_raw) => save_listen_gate(name, bind, port_raw),
                    None if patch.bind_ip.is_some() || patch.name.is_some() => {
                        save_listen_gate(name, bind, &existing.port.to_string())
                    }
                    None => Ok(()),
                };
                if let Err(reason) = port_gate {
                    *form_error.borrow_mut() = Some(("port".to_string(), reason.clone()));
                    form.focus_field("port", window, cx);
                    window.push_notification(Notification::error(reason), cx);
                    cx.notify();
                    return false;
                }

                if patch.params.is_some() {
                    let params = patch.params.as_deref().unwrap_or(&existing.params);
                    let protocol = patch.protocol.as_deref().unwrap_or(&existing.protocol);
                    if let Some((field, message)) = guided_blank_field(params, protocol) {
                        *form_error.borrow_mut() = Some((field.to_string(), message.clone()));
                        form.focus_field(field, window, cx);
                        window.push_notification(Notification::error(message), cx);
                        cx.notify();
                        return false;
                    }
                }

                if patch.params.is_some() && existing.protocol == "extra-core" {
                    let params = patch.params.as_deref().unwrap_or(&existing.params);
                    if let Err(reason) = validate_extra_params(params) {
                        *form_error.borrow_mut() = Some(("params".to_string(), reason.clone()));
                        *form.advanced.borrow_mut() = true;
                        form.focus_field("params", window, cx);
                        window.push_notification(Notification::error(reason), cx);
                        cx.notify();
                        return false;
                    }
                }
                match self.store.update(id, patch) {
                    Ok(updated) => {
                        *form_error.borrow_mut() = None;

                        let message = if self.engine.is_running(id) {
                            restart_to_apply_message(&updated.name)
                        } else {
                            row_action_message("Updated", &updated.name)
                        };
                        self.reload(window, cx);
                        window.push_notification(Notification::success(message), cx);
                        true
                    }
                    Err(error) => {
                        if let Some((field, message)) = field_error_target(&error) {
                            *form_error.borrow_mut() = Some((field.to_string(), message));

                            if field == "params" {
                                *form.advanced.borrow_mut() = true;
                            }
                            form.focus_field(field, window, cx);
                        }
                        self.report_save_error(error, window, cx);
                        cx.notify();
                        false
                    }
                }
            }
        }
    }

    pub(crate) fn commit_remove(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // K5: the whole row is captured BEFORE removal — the undo toast keeps
        // it in memory for its lifetime, so expiry means the row is simply
        // gone, and Undo re-adds it with its group assignment.
        let removed = self.store.get(id).ok().flatten();
        let was_running = self.engine.stop_one(id);
        match self.store.remove(id) {
            Ok(true) => {
                if self.selected_config(cx).map(|row| row.id).as_deref() == Some(id) {
                    self.libraries.update(cx, |state, list_cx| {
                        // T-06: clear through the list so the kit's own index
                        // (and the UIA selection it reports) drops the row too.
                        state.set_selected_index(None, window, list_cx);

                        list_cx.notify();
                    });
                }
                self.reload(window, cx);

                window.focus(&self.focus, cx);
                let name = removed
                    .as_ref()
                    .map(|row| row.name.clone())
                    .unwrap_or_else(|| id.to_string());
                if let Some(row) = removed {
                    let batch = next_undo_batch_id();
                    stash_undo_rows(vec![row], batch);
                    self.push_delete_undo_toast(
                        batch,
                        Notification::success(removed_message(&name, was_running)),
                        window,
                        cx,
                    );
                } else {
                    window.push_notification(
                        Notification::success(removed_message(&name, was_running)),
                        cx,
                    );
                }
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Cannot remove: {error}")), cx);
            }
        }
    }

    pub(crate) fn commit_duplicate(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = match self.store.get(id) {
            Ok(Some(row)) => row,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return;
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                return;
            }
        };
        let taken: Vec<String> = self
            .store
            .list()
            .map(|rows| rows.into_iter().map(|row| row.name).collect())
            .unwrap_or_default();
        let name = duplicate_name(&source.name, &taken);
        let input = NewConfig {
            name: name.clone(),
            protocol: source.protocol.clone(),
            params: source.params.clone(),
            group: source.group.clone(),
            bind_ip: source.bind_ip.clone(),
            port: String::new(),
        };
        match self.store.add(input) {
            Ok(created) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Duplicated \"{}\" as \"{}\" (disabled, port :{})",
                        source.name, created.name, created.port
                    )),
                    cx,
                );
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
    }

    pub(crate) fn commit_test_url(
        &mut self,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            match self.store.clear_test_url() {
                Ok(()) => {
                    window.push_notification(
                        Notification::success(format!(
                            "Test URL cleared — probes use the default ({DEFAULT_TEST_URL})"
                        )),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            }
        } else {
            match normalize_test_url(trimmed) {
                Ok(url) => match self.store.set_test_url(&url) {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(format!("Test URL set — probes use {url}")),
                            cx,
                        );
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                },
                Err(reason) => {
                    window.push_notification(
                        Notification::error(format!("Invalid test URL: {reason}")),
                        cx,
                    );
                    false
                }
            }
        }
    }

    pub(crate) fn commit_update_feed_url(
        &mut self,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            match self.store.clear_update_feed_url() {
                Ok(()) => {
                    window.push_notification(
                        Notification::success(format!(
                            "Feed URL cleared — updates check {UPDATE_FEED_URL_DEFAULT}"
                        )),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            }
        } else {
            match normalize_update_feed_url(trimmed) {
                Ok(url) => {
                    let shown = redact_subscription_url(&url);
                    match self.store.set_update_feed_url(&url) {
                        Ok(()) => {
                            if matches!(
                                store::subscription_fetch_policy(trimmed),
                                store::SubscriptionFetchPolicy::Warn { .. }
                            ) {
                                window.push_notification(
                                    Notification::warning(format!(
                                        "Feed URL set over plaintext http — prefer https: {shown}"
                                    )),
                                    cx,
                                );
                            } else {
                                window.push_notification(
                                    Notification::success(format!(
                                        "Feed URL set — updates check {shown}"
                                    )),
                                    cx,
                                );
                            }
                            true
                        }
                        Err(error) => {
                            self.report_save_error(error, window, cx);
                            false
                        }
                    }
                }
                Err(reason) => {
                    window.push_notification(
                        Notification::error(format!("Invalid feed URL: {reason}")),
                        cx,
                    );
                    false
                }
            }
        }
    }

    pub(crate) fn commit_row_test_url(
        &mut self,
        id: &str,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = self.store.row_test_url(id).unwrap_or_default();
        let trimmed = raw.trim();
        if trimmed == current.trim() {
            return true;
        }
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());
        if trimmed.is_empty() {
            match self.store.clear_row_test_url(id) {
                Ok(()) => {
                    window.push_notification(
                        Notification::success(format!(
                            "Test URL for \"{name}\" cleared — uses the global"
                        )),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            }
        } else {
            match normalize_test_url(trimmed) {
                Ok(url) => match self.store.set_row_test_url(id, &url) {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(format!(
                                "Test URL for \"{name}\" set — probes use {url}"
                            )),
                            cx,
                        );
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                },
                Err(reason) => {
                    window.push_notification(
                        Notification::error(format!("Invalid test URL: {reason}")),
                        cx,
                    );
                    false
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_edit_with_test_url(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        raw_url: &str,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.commit_edit_with_focus(id, patch, form, form_error, window, cx) {
            return false;
        }
        if self.commit_row_test_url(id, raw_url, window, cx) {
            // T-19: the edit committed — release the row-under-edit protection.
            self.edit_form_open = None;
            return true;
        }
        *form_error.borrow_mut() = Some((
            "test_url".to_string(),
            "Test URL rejected — fix it or clear the field".to_string(),
        ));
        form.focus_field("test_url", window, cx);
        cx.notify();
        false
    }

    pub(crate) fn commit_enabled_flag(
        &mut self,
        id: &str,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());

        match self.store.set_enabled(id, enabled) {
            Ok(_) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::info(enabled_flag_message(&name, enabled)),
                    cx,
                );
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Cannot toggle: {error}")), cx);
            }
        }
    }

    pub(crate) fn commit_prune(
        &mut self,
        kind: GroupPruneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };

        let targets: Vec<ProxyConfig> = match kind {
            GroupPruneKind::Duplicates => self
                .store
                .duplicate_rows_in_group(&group)
                .unwrap_or_default(),
            GroupPruneKind::Unavailable => self
                .store
                .unavailable_rows_in_group(&group)
                .unwrap_or_default(),
            GroupPruneKind::Invalid => self.store.invalid_rows_in_group(&group).unwrap_or_default(),
            GroupPruneKind::ClearTests => match self.store.clear_tests_in_group(&group) {
                Ok(cleared) => {
                    self.health_store_prune_group(&group);
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(format!(
                            "Cleared {cleared} test result(s) in group \"{group}\""
                        )),
                        cx,
                    );
                    return;
                }
                Err(error) => {
                    window.push_notification(Notification::error(error.to_string()), cx);
                    return;
                }
            },
        };
        // Stop every candidate first: a row that refuses to stop must stay in
        // the library so the engine never orphans a live listener (BUG-07).
        let mut skip: HashSet<String> = HashSet::new();
        let mut skipped: Vec<String> = Vec::new();
        for row in &targets {
            let (was_active, stop_error) = self.engine.stop_one_result(&row.id);
            if was_active && stop_error.is_some() {
                skip.insert(row.id.clone());
                skipped.push(row.name.clone());
            }
        }
        let pruned = match kind {
            GroupPruneKind::Duplicates => self.store.remove_duplicates_in_group(&group, &skip),
            GroupPruneKind::Unavailable => self.store.remove_unavailable_in_group(&group, &skip),
            GroupPruneKind::Invalid => self.store.remove_invalid_in_group(&group, &skip),
            GroupPruneKind::ClearTests => Ok(Vec::new()),
        };
        let removed: Vec<String> = match pruned {
            Ok(rows) => rows.into_iter().map(|row| row.name).collect(),
            Err(error) => {
                window.push_notification(Notification::error(error.to_string()), cx);
                return;
            }
        };
        self.reload(window, cx);
        if removed.is_empty() && skipped.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "{} in group \"{group}\": nothing removed — rows changed",
                    kind.label()
                )),
                cx,
            );
            return;
        }
        if removed.is_empty() {
            window.push_notification(
                Notification::error(format!(
                    "{} in group \"{group}\": nothing removed — {} row(s) could not be stopped ({})",
                    kind.label(),
                    skipped.len(),
                    skipped.join(", ")
                )),
                cx,
            );
            return;
        }
        let shown: Vec<&str> = removed.iter().take(3).map(String::as_str).collect();
        let mut text = format!(
            "{} in group \"{group}\": {} removed ({})",
            kind.label(),
            removed.len(),
            shown.join(", ")
        );
        if removed.len() > shown.len() {
            text.push_str(&format!(" (+{} more)", removed.len() - shown.len()));
        }
        if skipped.is_empty() {
            window.push_notification(Notification::success(text), cx);
        } else {
            window.push_notification(
                Notification::warning(format!(
                    "{text} — {} could not be stopped ({})",
                    skipped.len(),
                    skipped.join(", ")
                )),
                cx,
            );
        }
    }

    /// T-19: only this confirm's ACK enables the flag — the note is
    /// auto-clear's own, and the store rejects any enable without note_seen.
    pub(crate) fn commit_auto_clear_opt_in(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = AutoClearGroupConfig {
            enabled: true,
            on_date: now_unix(),
            note_seen: true,
        };
        match self.store.set_auto_clear_group(group, config) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Auto-clear unavailable enabled for \"{group}\""
                    )),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot enable auto-clear: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_auto_clear_off(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (on_date, note_seen) = match self.store.auto_clear_group(group) {
            Some(config) => (config.on_date, config.note_seen),
            None => (0, false),
        };
        let config = AutoClearGroupConfig {
            enabled: false,
            on_date,
            note_seen,
        };
        match self.store.set_auto_clear_group(group, config) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Auto-clear unavailable disabled for \"{group}\""
                    )),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot disable auto-clear: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_library_sort(
        &mut self,
        sort: LibrarySort,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_sort(sort);
            list_cx.notify();
        });
        if let Err(error) = self.store.set_library_sort(sort) {
            window.push_notification(
                Notification::error(format!("Could not save sort: {error}")),
                cx,
            );
        }
        window.push_notification(
            Notification::info(if sort.is_none() {
                "Sort: library order".to_string()
            } else {
                format!("Sort: {} · {}", sort.key.label(), sort.direction.label())
            }),
            cx,
        );
        cx.notify();
    }

    pub(crate) fn commit_save_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = Store::default_logs_path();
        match self.write_visible_logs_to(&path, cx) {
            Ok(()) => window.push_notification(
                Notification::success(format!("Logs saved to {}", path.display())),
                cx,
            ),
            Err(error) => window.push_notification(Notification::error(error), cx),
        }
    }

    /// T-31: persist `log_max_lines` and rebuild the ring at the clamped cap.
    /// The rebuild preserves nothing: changing the cap mid-session truncates
    /// every present record for the fresh ring (no partial-growth state), which
    /// the Settings note beside the field states.
    pub(crate) fn commit_log_cap(
        &mut self,
        window: &mut Window,
        raw: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(cap) = parse_log_cap(raw) else {
            window.push_notification(
                Notification::warning(format!(
                    "Maximum kept log lines must be a whole number (500–50,000) — keeping {}",
                    self.logs.cap()
                )),
                cx,
            );
            return;
        };
        if let Err(error) = self.store.set_log_max_lines(cap) {
            window.push_notification(
                Notification::error(format!("Cannot save the log cap: {error}")),
                cx,
            );
            return;
        }
        if let Some(input) = self.log_cap_input.clone() {
            input.update(cx, |state, cx| state.set_value(cap.to_string(), window, cx));
        }
        if cap == self.logs.cap() {
            return;
        }
        self.logs = LogStore::with_capacity(cap);
        self.logs_seen = 0;
        if let Some(list) = self.log_list.clone() {
            let lines = self.log_lines(cx);
            list.update(cx, |state, cx| {
                state.delegate_mut().lines = lines;
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(crate) fn commit_export_dialog(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            window.push_notification(Notification::error("Export path must not be empty"), cx);
            return false;
        }
        if std::path::Path::new(trimmed).exists() {
            let view = cx.entity();
            let description = export_overwrite_confirm_description(trimmed);
            let target = trimmed.to_string();
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let target = target.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text("Overwrite")
                            .ok_variant(ButtonVariant::Danger),
                    )
                    .title("Overwrite export?")
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.commit_export_write_to(&target, window, cx)
                        });
                        true
                    })
            });
            return false;
        }
        self.commit_export_write_to(trimmed, window, cx);
        true
    }

    pub(crate) fn commit_export_write_to(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = std::path::PathBuf::from(path.trim());
        match self.store.commit_export(&target, true) {
            Ok(Some(path)) => {
                let rendered = path.display().to_string();

                match self.store.list() {
                    Ok(rows) => window.push_notification(
                        Notification::success(export_count_message(
                            rows.len(),
                            count_importable(&rows),
                            &rendered,
                        )),
                        cx,
                    ),
                    Err(_) => window.push_notification(
                        Notification::success(format!("Exported to {rendered}")),
                        cx,
                    ),
                }
            }
            Ok(None) => {}
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Export failed: {error}")), cx);
            }
        }
    }

    pub(crate) fn commit_library_restore(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match Store::open_in_memory()
            .and_then(|mut dry| dry.restore_library_json(payload).map(|_| ()))
        {
            Ok(()) => {}
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Restore failed: {error}")), cx);
                return;
            }
        }

        if let Some(live_path) = self.store.db_path() {
            match Store::restore_library_file(&live_path, payload, now_unix()) {
                Ok((outcome, backup)) => {
                    let backup_text = backup.as_deref().map(|path| path.display().to_string());
                    let backup_text = backup_text.as_deref().unwrap_or("(no prior library file)");

                    match Store::open_file(&live_path) {
                        Ok(fresh) => {
                            self.store = fresh;
                            self.reload(window, cx);
                            window.push_notification(
                                Notification::success(format!(
                                    "{} (backup {})",
                                    restore_done_message(&outcome),
                                    backup_text
                                )),
                                cx,
                            );
                        }
                        Err(error) => {
                            self.reload(window, cx);
                            window.push_notification(
                                Notification::error(format!(
                                    "Restore swapped but reopen failed ({error}); backup at {backup_text}"
                                )),
                                cx,
                            );
                        }
                    }
                    return;
                }
                Err(error) => {
                    window.push_notification(
                        Notification::error(format!("Restore failed: {error}")),
                        cx,
                    );
                    return;
                }
            }
        }
        if let Err(error) = self.wipe_library_for_restore() {
            window.push_notification(
                Notification::error(format!(
                    "Restore failed: cannot clear the library ({error})"
                )),
                cx,
            );
            return;
        }
        match self.store.restore_library_json(payload) {
            Ok(outcome) => {
                self.reload(window, cx);
                window.push_notification(Notification::success(restore_done_message(&outcome)), cx);
            }
            Err(error) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::error(format!(
                        "Restore failed after clearing the library: {error}"
                    )),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_usage_reset(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.reset_traffic_counters(id) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(Notification::success("Usage counters reset to zero"), cx);
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot reset usage: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_bare_url_subscription(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        let label = subscription_label_from_url(&url);
        self.commit_subscription_add(
            label,
            url,
            DEFAULT_GROUP.to_string(),
            true,
            store::DEFAULT_AUTO_REFRESH_SECS,
            window,
            cx,
        );
    }

    #[cfg(test)]
    pub(crate) fn commit_import(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.commit_import_with_context(text, group, file_hint, None, None, window, cx)
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_import_with_context(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        original: Option<String>,
        multi: Option<Vec<(String, String)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_library_export(&text) {
            self.open_restore_confirm(text, window, cx);
            return false;
        }
        let resolved = resolve_import_inputs(text, group, file_hint, original, multi);
        if let Some(done) = self.try_addsub_commit(&resolved, window, cx) {
            return done;
        }
        if let Some(done) = self.try_preset_commit(&resolved, window, cx) {
            return done;
        }
        let batch = parse_import_inputs(&resolved);
        self.finish_import_batch(batch, &resolved.group, window, cx)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_import_dialog(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        original: Option<String>,
        multi: Option<Vec<(String, String)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_library_export(&text) {
            self.open_restore_confirm(text, window, cx);
            return false;
        }
        let resolved = resolve_import_inputs(text, group, file_hint, original, multi);
        if let Some(done) = self.try_addsub_commit(&resolved, window, cx) {
            return done;
        }
        if let Some(done) = self.try_preset_commit(&resolved, window, cx) {
            return done;
        }
        if self.import_parse_in_flight {
            return false;
        }
        self.import_parse_in_flight = true;
        self.import_parse_epoch = self.import_parse_epoch.wrapping_add(1);
        let epoch = self.import_parse_epoch;
        self.import_cancel_epoch = None;
        cx.notify();
        let weak = cx.entity().downgrade();

        let handle = window.window_handle();
        cx.spawn(async move |_entity, cx| {
            let group = resolved.group.clone();
            let batch = cx
                .background_executor()
                .spawn(async move { parse_import_inputs(&resolved) })
                .await;
            cx.update(|cx| {
                let mut pending = Some((batch, group));
                let delivered = handle
                    .update(cx, |_, window, cx| {
                        if let Some((batch, group)) = pending.take() {
                            let _ = weak.update(cx, |view, cx| {
                                view.finish_import_parse(epoch, batch, group, window, cx);
                            });
                        }
                    })
                    .is_ok();
                if !delivered {
                    if let Some((batch, group)) = pending.take() {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_import_parse_no_window(epoch, batch, group, cx);
                        });
                    }
                }
            });
        })
        .detach();
        false
    }

    pub(crate) fn commit_addsub_request_with_group(
        &mut self,
        request: AddSubRequest,
        dialog_group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let label = subscription_label_from_url(&request.url);
        self.commit_subscription_add(
            label,
            request.url,
            dialog_group.to_string(),
            true,
            store::DEFAULT_AUTO_REFRESH_SECS,
            window,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_subscription_add(
        &mut self,
        label: String,
        url: String,
        group: String,
        auto_refresh: bool,
        interval_secs: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Ok(Some(existing)) = self.store.find_subscription_by_url(&url) {
            window.push_notification(
                Notification::warning(already_subscribed_message(&existing.label)),
                cx,
            );
            return false;
        }
        match self
            .store
            .add_subscription(&label, &url, &group, auto_refresh)
        {
            Ok(sub) => {
                let interval_error = self
                    .store
                    .set_subscription_interval(&sub.id, interval_secs)
                    .err();
                self.reload(window, cx);
                match interval_error {
                    Some(error) => window.push_notification(
                        Notification::warning(subscription_interval_failure_message(
                            "added",
                            &sub.label,
                            &error.to_string(),
                        )),
                        cx,
                    ),
                    None => window.push_notification(
                        Notification::success(format!(
                            "Subscription \"{}\" added — refreshing",
                            sub.label
                        )),
                        cx,
                    ),
                }
                self.start_subscription_refresh(sub, true, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_subscription_edit(
        &mut self,
        id: &str,
        label: String,
        url: String,
        group: String,
        interval_secs: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.update_subscription(id, &label, &url, &group) {
            Ok(updated) => {
                let interval_error = self
                    .store
                    .set_subscription_interval(id, interval_secs)
                    .err();
                self.reload(window, cx);
                match interval_error {
                    Some(error) => window.push_notification(
                        Notification::warning(subscription_interval_failure_message(
                            "updated",
                            &updated.label,
                            &error.to_string(),
                        )),
                        cx,
                    ),
                    None => window.push_notification(
                        Notification::success(format!("Updated \"{}\"", updated.label)),
                        cx,
                    ),
                }
                self.reopen_subscriptions_if_open(window, cx);
                true
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                self.reopen_subscriptions_if_open(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    pub(crate) fn commit_subscription_auto(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = match self.store.get_subscription(sub_id) {
            Ok(Some(sub)) => sub.auto_refresh,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot change auto-refresh: {error}")),
                    cx,
                );
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
        };
        match self.store.set_subscription_auto_refresh(sub_id, !current) {
            Ok(_) => self.reload(window, cx),
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot change auto-refresh: {error}")),
                    cx,
                );
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    pub(crate) fn commit_preset_create_with_boxes(
        &mut self,
        name: String,
        rules_text: String,
        boxes: SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // K3: the form's `Add remote ruleset…` URL is content — a URL-only
        // form still commits (it stages the download below).
        let remote_url = self
            .preset_form
            .as_ref()
            .map(|form| form.remote_url.read(cx).value().trim().to_string())
            .unwrap_or_default();
        if name.trim().is_empty()
            && rules_text.trim().is_empty()
            && boxes.is_empty()
            && remote_url.is_empty()
        {
            return true;
        }
        let mut rules = if rules_text.trim().is_empty() {
            Vec::new()
        } else {
            match crate::preset::parse_preset_json(&rules_text, &name) {
                Ok(parsed) => parsed.rules,
                Err(error) => {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    return false;
                }
            }
        };
        match AppView::expand_simple_rule_boxes(&boxes, window, cx) {
            Ok(mut expanded) => rules.append(&mut expanded),
            Err(()) => return false,
        }
        let create_name = if name.trim().is_empty() {
            window.push_notification(
                Notification::error("Invalid routing preset: preset needs a name"),
                cx,
            );
            return false;
        } else {
            name.trim().to_string()
        };
        match self
            .store
            .create_preset(&create_name, "manual", None, &rules)
        {
            Ok(preset) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(preset_imported_message(&preset.name, rules.len())),
                    cx,
                );
                if !remote_url.is_empty() {
                    // K3: one creation path — the URL row stages the same
                    // explicit-confirm-per-download rule-set staging the old
                    // `Add by URL` dialog did. The staging request is deferred:
                    // the footer closes every dialog after a successful commit,
                    // and opening the Download confirm synchronously would be
                    // swept away by that same close.
                    let view = cx.entity();
                    let set_name = rule_set_name_from_url(&remote_url);
                    window.defer(cx, move |window, cx| {
                        view.update(cx, |view, cx| {
                            view.begin_rule_set_download(set_name, remote_url, window, cx);
                        });
                    });
                }
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    /// Compat entry for contract tests that only exercise the rules JSON
    /// layer (empty boxes expand to nothing).
    #[cfg(test)]
    pub(crate) fn commit_preset_edit(
        &mut self,
        id: &str,
        name: String,
        rules_text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.commit_preset_edit_with_boxes(
            id,
            name,
            rules_text,
            SimpleRuleBoxes::default(),
            window,
            cx,
        )
    }

    pub(crate) fn commit_preset_edit_with_boxes(
        &mut self,
        id: &str,
        name: String,
        rules_text: String,
        boxes: SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut rules = if rules_text.trim().is_empty() {
            Vec::new()
        } else {
            match crate::preset::parse_preset_json(&rules_text, &name) {
                Ok(parsed) => parsed.rules,
                Err(error) => {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    return false;
                }
            }
        };
        match AppView::expand_simple_rule_boxes(&boxes, window, cx) {
            Ok(mut expanded) => rules.append(&mut expanded),
            Err(()) => return false,
        }
        let renamed = match self.store.rename_preset(id, &name) {
            Ok(preset) => preset,
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return true;
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                return false;
            }
        };
        match self.store.set_preset_rules(id, &rules) {
            Ok(_) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Preset \"{}\" updated ({} rule(s))",
                        renamed.name,
                        rules.len()
                    )),
                    cx,
                );
                self.reopen_presets_if_open(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    pub(crate) fn commit_preset_export_link(
        &mut self,
        preset: &store::RoutePreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fresh = match self.store.get_preset(&preset.id) {
            Ok(Some(fresh)) => fresh,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot export routing preset: {error}")),
                    cx,
                );
                return;
            }
        };
        match fresh.rules() {
            Ok(rules) => {
                let link = crate::preset::preset_share_link(&fresh.name, &rules);
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                window.push_notification(
                    Notification::success(format!("Copied preset link for \"{}\"", fresh.name)),
                    cx,
                );
            }
            Err(message) => {
                window.push_notification(Notification::error(message), cx);
            }
        }
    }

    pub(crate) fn commit_preset_duplicate(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preset = match self.store.get_preset(id) {
            Ok(Some(preset)) => preset,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot duplicate routing preset: {error}")),
                    cx,
                );
                return;
            }
        };
        let taken: Vec<String> = self
            .store
            .list_presets()
            .map(|presets| presets.into_iter().map(|preset| preset.name).collect())
            .unwrap_or_default();
        let name = duplicate_name(&preset.name, &taken);
        let rules = preset.rules().unwrap_or_default();
        match self
            .store
            .create_preset(&name, &preset.source, preset.source_url.as_deref(), &rules)
        {
            Ok(created) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(preset_imported_message(&created.name, rules.len())),
                    cx,
                );
                self.reopen_presets_if_open(window, cx);
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
    }

    /// T-10 prevent-modifications lock. The toggle is the owner's positive
    /// action (no confirm dialog); it protects the rules, never the attach
    /// relationship.
    pub(crate) fn commit_preset_lock(
        &mut self,
        id: &str,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.set_preset_locked(id, locked) {
            Ok(_) => {
                self.reload(window, cx);
                if locked {
                    window.push_notification(
                        Notification::success("Routing preset locked — import a copy to modify"),
                        cx,
                    );
                } else {
                    window.push_notification(Notification::success("Routing preset unlocked"), cx);
                }
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    pub(crate) fn commit_preset_remove(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.delete_preset(id) {
            Ok(true) => {
                self.reload(window, cx);
                window.push_notification(Notification::success("Routing preset removed"), cx);
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(StoreError::Invalid(_)) => {
                let (name, attached) = match self.store.get_preset(id) {
                    Ok(Some(preset)) => {
                        let rows = self
                            .store
                            .preset_attachments(id)
                            .unwrap_or_default()
                            .iter()
                            .map(|row| format!("\"{}\"", row.name))
                            .collect::<Vec<_>>();
                        (preset.name, rows)
                    }
                    _ => ("preset".to_string(), Vec::new()),
                };
                let preset_id = id.to_string();
                self.open_preset_detach_delete_confirm(&preset_id, &name, &attached, window, cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot remove routing preset: {error}")),
                    cx,
                );
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    pub(crate) fn commit_preset_detach_delete(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.delete_preset_detaching(id) {
            Ok(true) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success("Routing preset removed (rows detached)"),
                    cx,
                );
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot remove routing preset: {error}")),
                    cx,
                );
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    pub(crate) fn commit_preset_attach(
        &mut self,
        config_id: &str,
        preset_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .store
            .set_config_preset(config_id, preset_id.as_deref())
        {
            Ok(row) => {
                self.reload(window, cx);
                let message = match preset_id {
                    Some(_) => {
                        let name = self
                            .preset_name_for(row.preset_id.as_deref())
                            .unwrap_or_else(|| "preset".to_string());
                        format!("\"{}\" now routes with \"{name}\"", row.name)
                    }
                    None => format!("\"{}\" detached from its routing preset", row.name),
                };
                window.push_notification(Notification::success(message), cx);
                window.close_all_dialogs(cx);
                reclaim_shell_focus(window, cx);
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
    }

    /// Remove: blocked while any preset references the set (loud inline), the
    /// file delete lands right after the row so no orphan cache bytes survive.
    pub(crate) fn commit_rule_set_remove(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = self.store.get_rule_set(id).ok().flatten();
        let Some(row) = existing else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        match self.store.delete_rule_set(id) {
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Ok(Some(file_name)) => {
                let path = crate::store::staged_rule_set_path(&file_name);
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(format!(
                                "Rule set \"{}\" removed (staged file deleted)",
                                row.name
                            )),
                            cx,
                        );
                    }
                    Err(error) => {
                        window.push_notification(
                            Notification::error(format!(
                                "Rule set \"{}\" removed, but the staged file could not be deleted: {} ({error}) — delete it manually",
                                row.name,
                                path.display()
                            )),
                            cx,
                        );
                    }
                }
                let _ = self.sync_presets_to_engine();
                self.reload(window, cx);
                self.reopen_presets_if_open(window, cx);
            }
        }
    }

    pub(crate) fn commit_subscription_remove(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.remove_subscription(id) {
            Ok(true) => {
                self.reload(window, cx);
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot remove subscription: {error}")),
                    cx,
                );
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    pub(crate) fn commit_subs_global_auto(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.store.set_subs_auto_refresh_enabled(on) {
            window.push_notification(
                Notification::error(format!("Cannot change auto-refresh: {error}")),
                cx,
            );
        }
        self.reopen_subscriptions_if_open(window, cx);
    }
}

/// K3: the `Add remote ruleset…` row only carries a URL, so the staging name
/// falls back to the URL's last path segment with the extension stripped. The
/// store still validates the final name and reports why it refused.
fn rule_set_name_from_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    let segment = trimmed
        .split(['/', '?', '#'])
        .next_back()
        .unwrap_or_default();
    let stem = segment.split('.').next().unwrap_or_default();
    let slug: String = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "rule-set".to_string()
    } else {
        slug
    }
}
