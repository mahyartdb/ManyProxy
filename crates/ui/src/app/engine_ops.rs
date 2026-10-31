use crate::*;

impl AppView {
    pub(crate) fn reload_quiet(&mut self, cx: &mut Context<Self>) {
        let _ = self.reload_inner(cx);
        cx.notify();
    }

    pub(crate) fn reload_quiet_no_poll(&mut self, cx: &mut Context<Self>) {
        let _ = self.reload_inner_no_poll(cx);
        cx.notify();
    }

    pub(crate) fn reload_inner(&mut self, cx: &mut Context<Self>) -> Result<(), StoreError> {
        self.reload_inner_impl(cx, true)
    }

    pub(crate) fn reload_inner_no_poll(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), StoreError> {
        self.reload_inner_impl(cx, false)
    }

    pub(crate) fn reload_inner_impl(
        &mut self,
        cx: &mut Context<Self>,
        poll: bool,
    ) -> Result<(), StoreError> {
        let rows = self.store.list()?;
        let bind_hosts = self.store.bind_hosts();
        self.drain_engine_logs(&rows);

        if poll {
            self.poll_engine();
        }
        let health = self.health_map();
        let health_tones = self.health_tones();
        // T-27: sort mirrors — pulled from state the ticks already maintain.
        let latency = self.latency_map(&rows);
        let data_traffic = self.data_traffic_map(&rows);
        let out_ips = self.out_ip_by_row.clone();
        let running = self.engine.running_ids();

        self.snapshot_running_set(&running);
        self.sync_running_title(running.len(), cx);
        if let Some(tray) = &self.tray {
            tray.set_running(running.len());
        }

        let traffic = self.store.traffic_totals().unwrap_or_default();
        let mut sub_labels = HashMap::new();
        if let Ok(subs) = self.store.list_subscriptions() {
            for sub in &subs {
                sub_labels.insert(sub.id.clone(), sub.label.clone());
            }
        }
        let mut preset_names = HashMap::new();
        if let Ok(presets) = self.store.list_presets() {
            for preset in &presets {
                preset_names.insert(preset.id.clone(), preset.name.clone());
            }
        }
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_rows(rows);
            state.delegate_mut().set_bind_hosts(bind_hosts);
            state.delegate_mut().set_health(health);

            state.delegate_mut().set_health_tone(health_tones);

            state.delegate_mut().set_running(running);

            state
                .delegate_mut()
                .set_crashed(self.engine.ids_on_crashed_shards());
            state.delegate_mut().set_traffic(traffic);
            state.delegate_mut().set_latency(latency);
            state.delegate_mut().set_data_traffic(data_traffic);
            state.delegate_mut().set_out_ips(out_ips);
            state
                .delegate_mut()
                .set_search_context(sub_labels, preset_names);

            list_cx.notify();
        });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn toggle_selected_enabled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        let id = config.id.clone();
        let enabled = !config.enabled;
        self.commit_enabled_flag(&id, enabled, window, cx);
    }

    pub(crate) fn poll_engine(&mut self) -> Option<String> {
        self.engine.poll()
    }

    #[cfg(test)]
    pub(crate) fn restart_crashed_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let crashed = self.engine.crashed_shards();
        if crashed.is_empty() {
            window.push_notification(Notification::info("No crashed endpoints to restart"), cx);
            return;
        }
        match self.engine.restart_crashed() {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Restarted {}", crashed.join(", "))),
                    cx,
                );
            }
            Err(error) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::error(format!(
                        "Restart failed: {}",
                        redact_secrets(&error.to_string())
                    )),
                    cx,
                );
            }
        }
    }

    pub(crate) fn start_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.start_row(&config.id, window, cx);
    }

    pub(crate) fn start_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };

        if self.libraries.read(cx).delegate().endpoint_is_dirty(id, cx) {
            window.push_notification(Notification::info(dirty_endpoint_message(&config.name)), cx);
            return;
        }
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        if requires_lan_confirm(&config.bind_ip) {
            self.open_lan_confirm_single(config, window, cx);
            return;
        }

        if cfg!(test) {
            self.commit_start_selected(&config.id, window, cx);
        } else {
            self.spawn_commit_start_selected(config.id.clone(), window, cx);
        }
    }

    pub(crate) fn stop_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.stop_row(&config.id, window, cx);
    }

    pub(crate) fn stop_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let id = config.id.clone();
        let (removed, stop_error) = self.engine.stop_one_result(&id);
        if removed {
            if let Some(error) = stop_error {
                self.reload(window, cx);
                window.push_notification(
                    Notification::error(format!(
                        "Stop of \"{}\" failed: {}",
                        config.name,
                        redact_secrets(&error.to_string())
                    )),
                    cx,
                );
                return;
            }

            match self.store.set_enabled(&id, false) {
                Ok(_) => {
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(format!(
                            "Stopped \"{}\" — flag cleared",
                            config.name
                        )),
                        cx,
                    );
                }
                Err(error) => {
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::error(format!(
                            "Stopped \"{}\" but could not save its flag: {error}",
                            config.name
                        )),
                        cx,
                    );
                }
            }
        } else {
            window.push_notification(
                Notification::warning(already_stopped_message(&config.name)),
                cx,
            );
        }
    }

    pub(crate) fn toggle_global_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        if self.engine.running_count() == 0 {
            self.start_all_engines(window, cx);
        } else {
            self.stop_all_engines(window, cx);
        }
    }

    pub(crate) fn start_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }

        let Some(rows) = self.list_or_notify(window, cx) else {
            return;
        };
        let lan: Vec<(String, String)> = rows
            .iter()
            .filter(|row| requires_lan_confirm(&row.bind_ip))
            .map(|row| (row.name.clone(), row.bind_ip.clone()))
            .collect();
        if !lan.is_empty() {
            let view = cx.entity();
            let description = lan_confirm_description(&lan);
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text("Start all"),
                    )
                    .title(LAN_CONFIRM_TITLE)
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| view.commit_start_all(window, cx));
                        true
                    })
            });
            return;
        }
        if cfg!(test) {
            self.commit_start_all(window, cx);
        } else {
            self.spawn_commit_start_all(window, cx);
        }
    }

    pub(crate) fn stop_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.engine.running_count() == 0 {
            let message = stop_all_message(0);
            window.push_notification(Notification::warning(message), cx);
            return;
        }
        let names = self.running_names_for_close();
        let view = cx.entity();
        let description = stop_all_confirm_description(&names);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Stop all"),
                )
                .title("Stop all endpoints")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_stop_all(window, cx));
                    true
                })
        });
    }

    pub(crate) fn health_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for id in self.engine.running_ids() {
            let summary = self.health_store.row_summary(&id);
            if !summary.is_empty() {
                map.insert(id, summary);
            }
        }
        map
    }

    pub(crate) fn health_tones(&self) -> HashMap<String, LatencyTone> {
        self.engine
            .running_ids()
            .iter()
            .filter(|id| !self.health_store.row_summary(id).trim().is_empty())
            .map(|id| (id.clone(), self.health_store.health_tone(id)))
            .collect()
    }

    pub(crate) fn health_counts(&self) -> (usize, usize) {
        self.health_store.counts(&self.engine.running_ids())
    }

    pub(crate) fn health_status_text(&self) -> String {
        let (live, dead) = self.health_counts();
        // K12/A5-1: the center segment is `engine <state> · health X live ·
        // Y dead`; the label carries no colon so the `·` grammar stays the
        // only separator the strip spends.
        if live == 0 && dead == 0 {
            "health never tested".to_string()
        } else {
            format!("health {live} live · {dead} dead")
        }
    }

    pub(crate) fn probe_kind_in_flight(&self, ping: bool) -> bool {
        if ping {
            self.probe_ping_in_flight
        } else {
            self.probe_url_in_flight
        }
    }

    pub(crate) fn test_all_health(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let Some(rows) = self.running_rows_or_notify(window, cx) else {
            return 0;
        };
        // T-04: the scope decides which running rows the run covers.
        let rows = if self.store.test_all_scope() == TestAllScope::Checked {
            let checked: HashSet<String> = self
                .libraries
                .read(cx)
                .delegate()
                .checked_rows()
                .into_iter()
                .map(|row| row.id)
                .collect();
            rows.into_iter()
                .filter(|row| checked.contains(&row.id))
                .collect()
        } else {
            rows
        };
        if rows.is_empty() {
            window.push_notification(Notification::warning("No running endpoints to test"), cx);
            return 0;
        }
        if self.probe_kind_in_flight(false) {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return 0;
        }
        self.spawn_probe(rows, false, true, Some(window.window_handle()), cx)
    }

    pub(crate) fn test_selected_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.test_row(&config.id, window, cx);
    }

    pub(crate) fn test_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        if !self.engine.running_ids().contains(&config.id) {
            window.push_notification(
                Notification::warning(format!("\"{}\" is not running", config.name)),
                cx,
            );
            return;
        }
        if self.probe_kind_in_flight(false) {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return;
        }
        self.spawn_probe(vec![config], false, true, Some(window.window_handle()), cx);
    }

    /// K8 removed the Tools "Test group" row; the group sweep stays wired for
    /// the T-19 auto-clear path and its tests, so the entry point is retained
    /// until a group-test entry gets a new UI home.
    #[allow(dead_code)]
    pub(crate) fn test_group_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let rows = self.group_running_rows(&group);
        if rows.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "Group \"{group}\" has no running endpoints to test"
                )),
                cx,
            );
            return;
        }
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.spawn_group_probe(rows, group, Some(window.window_handle()), cx);
    }

    pub(crate) fn stop_group_tests(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        if !self.group_test_in_flight {
            window.push_notification(Notification::info("No group test is running"), cx);
            return;
        }
        self.probe_stop.store(true, Ordering::SeqCst);
        window.push_notification(Notification::info("Stopping the group test…"), cx);
    }

    pub(crate) fn health_store_prune_group(&mut self, group: &str) {
        let ids: HashSet<String> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == group)
            .map(|row| row.id)
            .collect();
        self.health_store.prune_configs(&ids);
    }

    pub(crate) fn toggle_show_security(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // A6-4/K8: the toggle renders in place (rows re-render with the
        // security marks) — no toast while the menu is open.
        let show = !self.libraries.read(cx).delegate().show_security;
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_show_security(show);
            list_cx.notify();
        });
    }

    pub(crate) fn toggle_library_sort_direction(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.libraries.read(cx).delegate().sort;
        self.commit_library_sort(
            LibrarySort::new(current.key, current.direction.toggled()),
            window,
            cx,
        );
    }

    pub(crate) fn drain_engine_logs(&mut self, active: &[ProxyConfig]) {
        let lines = self.engine.drain_log_lines();
        if lines.is_empty() {
            return;
        }

        let tags = LogTagIndex::build(active);
        for line in &lines {
            self.logs.store.push_raw(line, &tags);
        }
    }

    pub(crate) fn toggle_log_tail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.logs.tail = !self.logs.tail;
        window.push_notification(
            Notification::info(if self.logs.tail {
                "Log tail resumed"
            } else {
                "Log tail paused"
            }),
            cx,
        );
        if self.logs.open && window.has_active_sheet(cx) {
            self.logs.reopen_draft = self.log_input_draft(cx);
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs.open = false;
            self.open_logs_dialog(window, cx);
        } else {
            cx.notify();
        }
    }

    #[cfg(test)]
    pub(crate) fn apply_log_filter(&mut self, filter: String, cx: &mut Context<Self>) {
        self.logs.filter = filter;
        cx.notify();
    }

    pub(crate) fn apply_log_filters(
        &mut self,
        include: String,
        exclude: String,
        pattern: String,
        cx: &mut Context<Self>,
    ) {
        self.logs.filter = include;
        self.logs.exclude = exclude;
        self.logs.regex = pattern;
        cx.notify();
    }

    pub(crate) fn start_subscription_refresh(
        &mut self,
        sub: ProxySubscription,
        named: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.note_refresh_started(&sub.id) {
            return false;
        }
        self.mark_library_loading(cx);
        if named {
            self.refresh.inflight_named.insert(sub.id.clone());
        }
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        let sub_id = sub.id.clone();
        let label = sub.label.clone();
        let url = sub.url.clone();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_subscription_body(client, &url, resolve_subscription_host).await;
            let now = now_unix();
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_subscription_refresh(
                                &sub_id, &label, fetched, now, window, cx,
                            );
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_subscription_refresh_no_window(&sub_id, fetched, now, cx);
                    });
                }
            });
        })
        .detach();
        true
    }

    pub(crate) fn toggle_sub_expanded(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.expanded_subs.remove(id) {
            self.expanded_subs.insert(id.to_string());
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    pub(crate) fn start_preset_refresh(
        &mut self,
        preset: store::RoutePreset,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(url) = preset.source_url.clone() else {
            return false;
        };
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        let preset_id = preset.id.clone();
        let preset_name = preset.name.clone();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_subscription_body(client, &url, resolve_subscription_host).await;
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_preset_refresh(
                                &preset_id,
                                &preset_name,
                                fetched,
                                window,
                                cx,
                            );
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_preset_refresh_no_window(&preset_id, fetched, cx);
                    });
                }
            });
        })
        .detach();
        true
    }

    /// Reached only from the confirm dialog's Download click. Fetches through
    /// the shared subscription pipeline, then stages bytes + row.
    pub(crate) fn start_rule_set_download(
        &mut self,
        grant: RuleSetDownloadGrant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rule_set_download_in_flight {
            window.push_notification(
                Notification::warning("A rule set download is already in flight — wait for it"),
                cx,
            );
            return;
        }
        self.rule_set_download_in_flight = true;
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_rule_set_body(client, &grant.url, resolve_subscription_host).await;
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_rule_set_download(&grant, fetched, window, cx);
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_rule_set_download_no_window(&grant, fetched, cx);
                    });
                }
            });
        })
        .detach();
    }

    pub(crate) fn start_explicit_refresh(
        &mut self,
        subs: Vec<ProxySubscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        if subs.len() == 1 {
            let sub = subs.into_iter().next().expect("one sub");
            if self.refresh_subscription_by_id(&sub.id, window, cx) {
                return 1;
            }
            return 0;
        }
        let ids: Vec<String> = subs.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in subs {
            if self.start_subscription_refresh(sub, true, cx) {
                started += 1;
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
        started
    }

    #[cfg(test)]
    pub(crate) fn stop_background_ticks(&mut self) {
        self.ticks_enabled = false;
    }

    pub(crate) fn tick_engine_poll(&mut self, cx: &mut Context<Self>) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }

        // T-08: the tick is the seam that lets a fixed folder clear the banner
        // without a restart.
        self.refresh_write_blocked(cx);
        let cause = self.poll_engine();
        self.tick_shard_samples();
        self.reload_quiet_no_poll(cx);
        if let Some(notification) = self.service_engine_crash() {
            self.push_engine_notification(notification, cx);
        }
        cause
    }

    #[cfg(test)]
    pub(crate) fn tick_engine_poll_notify(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }
        self.refresh_write_blocked(cx);
        let detail = self.poll_engine();
        self.tick_shard_samples();
        self.reload_quiet_no_poll(cx);
        if let Some(notification) = self.service_engine_crash() {
            window.push_notification(notification, cx);
        }
        detail
    }

    /// T-30: one sampling pass, riding the existing engine tick (no new timer).
    /// The pid cohort comes from the controller, so only this app's own
    /// sidecars can ever be opened.
    pub(crate) fn tick_shard_samples(&mut self) {
        let live: Vec<(String, u32)> = self
            .engine
            .shard_processes()
            .into_iter()
            .filter_map(|(label, pid)| pid.map(|pid| (label, pid)))
            .collect();
        self.shard_denied = fold_shard_samples(
            &mut self.shard_rings,
            &mut self.shard_baselines,
            &live,
            self.process_sampler.as_ref(),
            Instant::now(),
        );
    }

    pub(crate) fn tick_due_subscriptions(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let due = self.due_subscriptions();
        let ids: Vec<String> = due.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in due {
            if self.start_subscription_refresh(sub, false, cx) {
                started += 1;
            }
        }
        started
    }

    pub(crate) fn tick_reprobe_running(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let running = self.engine.running_ids();
        let crashed: HashSet<String> = self.engine.ids_on_crashed_shards().into_iter().collect();
        let Ok(rows) = self.store.list() else {
            return 0;
        };
        let rows: Vec<ProxyConfig> = rows
            .into_iter()
            .filter(|row| {
                running.contains(&row.id)
                    && self.engine.is_running(&row.id)
                    && !crashed.contains(&row.id)
            })
            .collect();
        if rows.is_empty() {
            return 0;
        }
        self.spawn_probe(rows, false, false, None, cx)
    }

    pub(crate) fn ensure_traffic_mirror(&mut self, cx: &mut Context<Self>) {
        if self.traffic_mirror_hydrated {
            return;
        }
        self.traffic_mirror_hydrated = true;
        let Ok(totals) = self.store.traffic_totals() else {
            return;
        };
        let known: HashSet<String> = self
            .libraries
            .read(cx)
            .delegate()
            .rows
            .iter()
            .map(|row| row.id.clone())
            .collect();
        for (id, (rx, tx)) in &totals {
            if (*rx, *tx) != (0, 0) && known.contains(id) {
                self.health_store.set_data_traffic(id, *rx, *tx);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn tick_traffic_poll(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        self.ensure_traffic_mirror(cx);
        let targets = self.engine.live_clash_targets();
        if targets.is_empty() {
            return 0;
        }
        let results = crate::health::poll_clash_targets(
            &targets,
            crate::health::CLASH_PER_CORE_TIMEOUT,
            crate::health::CLASH_TOTAL_DEADLINE,
        );
        let mut exclusions = self.traffic_probe_exclusions();
        exclusions.resolve_ips();
        self.apply_traffic_results(results, &exclusions, cx)
    }

    pub(crate) fn apply_traffic_results(
        &mut self,
        results: Vec<(
            crate::engine::ClashTarget,
            Result<Vec<crate::health::ClashConn>, String>,
        )>,
        exclusions: &ProbeExclusions,
        cx: &mut Context<Self>,
    ) -> usize {
        let now = now_unix();
        let mut deltas: HashMap<String, (u64, u64)> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut ok_cores: HashSet<String> = HashSet::new();

        let mut fresh_rows: HashSet<String> = HashSet::new();

        let mut snapshot: Vec<(String, crate::health::ClashConn)> = Vec::new();
        let mut conn_deltas: HashMap<String, (u64, u64)> = HashMap::new();
        for (target, result) in &results {
            let Ok(entries) = result else {
                continue;
            };
            ok_cores.insert(target.core.clone());
            for entry in entries {
                if exclusions.excludes(&entry.dest) {
                    continue;
                }
                let key = format!("{}/{}", target.core, entry.id);
                let (rx, tx) = (entry.rx_bytes, entry.tx_bytes);
                let (last_rx, last_tx) = self.traffic_last_seen.get(&key).unwrap_or((0, 0));
                let (delta_rx, delta_tx) = if rx >= last_rx && tx >= last_tx {
                    (rx - last_rx, tx - last_tx)
                } else {
                    (rx, tx)
                };
                self.traffic_last_seen.insert(key.clone(), rx, tx, now);
                seen.insert(key.clone());
                snapshot.push((target.core.clone(), entry.clone()));
                if delta_rx > 0 || delta_tx > 0 {
                    conn_deltas.insert(key, (delta_rx, delta_tx));
                }
                let Some(row) = entry.row_id.as_deref() else {
                    continue;
                };
                fresh_rows.insert(row.to_string());
                if delta_rx > 0 || delta_tx > 0 {
                    let slot = deltas.entry(row.to_string()).or_insert((0, 0));
                    slot.0 = slot.0.saturating_add(delta_rx);
                    slot.1 = slot.1.saturating_add(delta_tx);
                }
            }
        }

        let live_cores: HashSet<String> = results
            .iter()
            .map(|(target, _)| target.core.clone())
            .collect();
        self.traffic_last_seen
            .sweep(now, &ok_cores, &seen, &live_cores);

        self.conn.snapshot = snapshot;
        self.conn.counts =
            crate::health::ConnCounts::from_conns(self.conn.snapshot.iter().map(|(_, conn)| conn));
        self.conn.delta = conn_deltas;
        self.conn.last_poll_at = now;
        let mut updated = 0;

        if !deltas.is_empty() {
            let batch: Vec<(String, u64, u64)> = deltas
                .iter()
                .map(|(row, (rx, tx))| (row.clone(), *rx, *tx))
                .collect();
            match self.store.add_traffic_batch(&batch) {
                Ok(count) => {
                    updated = count;

                    for (row, (rx, tx)) in &deltas {
                        let (prev_rx, prev_tx) =
                            self.health_store.data_traffic(row).unwrap_or((0, 0));
                        self.health_store.set_data_traffic(
                            row,
                            prev_rx.saturating_add(*rx),
                            prev_tx.saturating_add(*tx),
                        );
                    }
                }
                Err(error) => eprintln!("traffic tick failed: {error}"),
            }
        }

        self.health_store
            .mark_stale_all(&self.engine.ids_on_crashed_shards());
        for row in &fresh_rows {
            self.health_store.clear_stale(row);
        }
        if updated > 0 {
            self.reload_quiet(cx);
        }
        updated
    }

    pub(crate) fn start_background_ticks(&mut self, cx: &mut Context<Self>) {
        if self.ticks_started {
            return;
        }
        self.ticks_started = true;
        spawn_tick(cx, ENGINE_POLL_TICK_SECS, |view, cx| {
            view.tick_engine_poll(cx);
        });
        spawn_tick(cx, DUE_SUBSCRIPTION_TICK_SECS, |view, cx| {
            view.tick_due_subscriptions(cx);
        });
        spawn_tick(cx, REPROBE_TICK_SECS, |view, cx| {
            view.tick_reprobe_running(cx);
        });
        spawn_tick(cx, TRAFFIC_FAST_TICK_SECS, |view, cx| {
            view.spawn_traffic_poll(cx);
        });
    }
}
