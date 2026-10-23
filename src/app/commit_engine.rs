use crate::*;

impl AppView {
    pub(crate) fn commit_reset_traffic(
        &mut self,
        id: &str,
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
        match self.store.reset_traffic(id) {
            Ok(()) => {
                self.health_store.set_data_traffic(id, 0, 0);
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Reset traffic for \"{name}\"")),
                    cx,
                );
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot reset traffic: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_clear_traffic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.clear_all_traffic() {
            Ok(count) => {
                self.traffic_last_seen.clear();
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Cleared traffic for {count} row(s)")),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot clear traffic: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_clear_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.clear_health_snapshots() {
            Ok(count) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Cleared {count} health snapshot(s)")),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot clear health: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_clear_snapshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.clear_running_snapshot() {
            Ok(()) => {
                self.last_snapshot = Vec::new();
                window.push_notification(
                    Notification::success("Running snapshot cleared — next launch starts clean"),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot clear snapshot: {error}")),
                    cx,
                );
            }
        }
    }

    pub(crate) fn commit_start_selected(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        self.start_in_flight = true;
        let Ok(Some(config)) = self.store.get(id) else {
            self.start_in_flight = false;
            return;
        };

        if let Err(reason) = self.sync_presets_to_engine() {
            self.start_in_flight = false;
            window.push_notification(Notification::error(format!("Cannot start: {reason}")), cx);
            return;
        };
        let was_running = self.engine.is_running(id);
        if let Some(warning) = lan_exposure_warning(&config.name, &config.bind_ip) {
            window.push_notification(Notification::warning(warning), cx);
        }
        match self.engine.start_one_confirmed(&config) {
            Ok(()) => match self.store.set_enabled(id, true) {
                Ok(_) => {
                    self.reload(window, cx);
                    let message = if was_running {
                        already_running_message(&config.name)
                    } else {
                        format!("Started \"{}\"", config.name)
                    };
                    window.push_notification(Notification::success(message), cx);
                }
                Err(error) => {
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::error(format!(
                            "Started \"{}\" but could not save its flag: {error}",
                            config.name
                        )),
                        cx,
                    );
                }
            },
            Err(error) => {
                window
                    .push_notification(Notification::error(redact_secrets(&error.to_string())), cx);
            }
        }
        self.start_in_flight = false;
    }

    pub(crate) fn commit_start_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        self.start_in_flight = true;

        let Some(rows) = self.list_or_notify(window, cx) else {
            self.start_in_flight = false;
            return;
        };
        if rows.is_empty() {
            window.push_notification(Notification::warning(start_all_message(0)), cx);
            self.start_in_flight = false;
            return;
        }
        if rows.iter().all(|row| self.engine.is_running(&row.id)) {
            self.start_in_flight = false;
            window.push_notification(
                Notification::info(format!("All {} endpoint(s) already running", rows.len())),
                cx,
            );
            return;
        }

        if let Err(reason) = self.sync_presets_to_engine() {
            self.start_in_flight = false;
            window.push_notification(Notification::error(format!("Cannot start: {reason}")), cx);
            return;
        }
        match self.engine.start_all_confirmed(&rows) {
            Ok(()) => {
                let mut flag_errors = 0;
                let mut first_error = String::new();
                for row in &rows {
                    if let Err(error) = self.store.set_enabled(&row.id, true) {
                        if flag_errors == 0 {
                            first_error = error.to_string();
                        }
                        flag_errors += 1;
                    }
                }
                self.reload(window, cx);
                self.warn_for_lan_binds(window, cx);
                self.start_in_flight = false;
                if flag_errors == 0 {
                    window.push_notification(
                        Notification::success(start_all_message(rows.len())),
                        cx,
                    );
                } else {
                    window.push_notification(
                        Notification::error(format!(
                            "Started {} endpoint(s) but {flag_errors} flag(s) failed to save: {first_error}",
                            rows.len()
                        )),
                        cx,
                    );
                }
            }
            Err(fast_error) => {
                if !is_row_local_start_error(&fast_error) {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    window.push_notification(
                        Notification::error(redact_secrets(&fast_error.to_string())),
                        cx,
                    );
                    return;
                }

                let outcome = self.engine.start_all_subset(&rows, true);
                if let Some(error) = outcome.error.as_deref() {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    window.push_notification(Notification::error(redact_secrets(error)), cx);
                    return;
                }
                let started: Vec<ProxyConfig> = rows
                    .iter()
                    .filter(|row| outcome.applied.contains(&row.id))
                    .cloned()
                    .collect();
                let failed_names: Vec<String> = outcome
                    .failed
                    .iter()
                    .map(|(name, _)| format!("\"{name}\""))
                    .collect();
                if started.is_empty() {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    let reason = outcome
                        .failed
                        .first()
                        .map(|(_, cause)| cause.clone())
                        .unwrap_or_else(|| fast_error.to_string());
                    window.push_notification(Notification::error(redact_secrets(&reason)), cx);
                    return;
                }

                let mut flag_errors = 0;
                let mut first_error = String::new();
                for row in &started {
                    if let Err(error) = self.store.set_enabled(&row.id, true) {
                        if flag_errors == 0 {
                            first_error = error.to_string();
                        }
                        flag_errors += 1;
                    }
                }
                self.reload(window, cx);
                self.warn_for_lan_binds(window, cx);
                self.start_in_flight = false;
                if flag_errors > 0 {
                    window.push_notification(
                        Notification::error(format!(
                            "Started {} endpoint(s) but {flag_errors} flag(s) failed to save: {first_error}",
                            started.len()
                        )),
                        cx,
                    );
                    return;
                }
                window.push_notification(
                    Notification::warning(start_all_partial_message(started.len(), &failed_names)),
                    cx,
                );

                if !outcome.warnings.is_empty() {
                    window
                        .push_notification(Notification::warning(outcome.warnings.join("; ")), cx);
                }
            }
        }
    }

    pub(crate) fn commit_stop_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let running = self.engine.running_count();
        let stop_error = self.engine.stop_all_result().map(|error| error.to_string());
        if let Some(error) = stop_error {
            self.reload(window, cx);
            window.push_notification(
                Notification::error(format!("Stop-all failed: {}", redact_secrets(&error))),
                cx,
            );
            return;
        }

        match self.store.list() {
            Ok(rows) => {
                let mut flag_errors = 0;
                let mut first_error = String::new();
                for row in &rows {
                    if let Err(error) = self.store.set_enabled(&row.id, false) {
                        if flag_errors == 0 {
                            first_error = error.to_string();
                        }
                        flag_errors += 1;
                    }
                }
                self.reload(window, cx);
                if flag_errors > 0 {
                    window.push_notification(
                        Notification::error(format!(
                            "Stopped all endpoints but {flag_errors} flag(s) failed to save: {first_error}"
                        )),
                        cx,
                    );
                    return;
                }
            }
            Err(error) => {
                window.push_notification(Notification::error(db_list_error_message(&error)), cx);
                self.reload_quiet(cx);
                return;
            }
        }
        let message = stop_all_message(running);
        if running == 0 {
            window.push_notification(Notification::warning(message), cx);
        } else {
            window.push_notification(Notification::success(message), cx);
        }
    }
}
