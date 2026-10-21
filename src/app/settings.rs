use crate::*;

impl AppView {
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

    pub(crate) fn commit_skip_delete_confirm(
        &mut self,
        skip: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.set_skip_delete_confirm(skip) {
            Ok(()) => {
                let message = if skip {
                    "Delete confirms off — single and bulk deletes run at once (preset detach-deletes still ask)"
                } else {
                    "Delete confirms on"
                };
                window.push_notification(Notification::success(message), cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Delete-confirm setting failed: {error}")),
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

    /// T-33: adds one address to the list the row pickers draw from. The store
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

    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let autostart_state = Rc::new(RefCell::new(crate::tray::autostart_enabled()));
        let restore_state = Rc::new(RefCell::new(self.store.restore_running()));
        let auto_restart_state = Rc::new(RefCell::new(self.engine.auto_restart().enabled));

        let skip_delete_state = Rc::new(RefCell::new(self.store.skip_delete_confirm()));

        let appearance_state = Rc::new(RefCell::new(self.store.appearance()));
        let appearance_view = view.clone();
        apply_appearance(&self.store.appearance(), Some(window), cx);

        let preset_total = self.store.list_presets().ok().map(|presets| presets.len());
        let attached_total = self
            .store
            .list()
            .ok()
            .map(|rows| rows.iter().filter(|row| row.preset_id.is_some()).count());
        let routing_summary = routing_summary_text(preset_total, attached_total);
        let preset_summary = preset_summary_text(preset_total);
        let presets_view = view.clone();

        let current_test_url = self.store.test_url().unwrap_or_default();
        let test_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("Default: {DEFAULT_TEST_URL}"))
                .default_value(current_test_url)
        });
        let test_url_reader = test_url_state.clone();
        let test_url_view = view.clone();
        let current_feed_url = self.store.update_feed_url().unwrap_or_default();
        let feed_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("Default: {UPDATE_FEED_URL_DEFAULT}"))
                .default_value(current_feed_url)
        });
        let feed_url_reader = feed_url_state.clone();
        let feed_url_view = view.clone();
        let cap_text = self.logs.cap().to_string();
        let cap_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("500–50,000")
                .default_value(cap_text)
        });
        let cap_reader = cap_state.clone();
        self.log_cap_input = Some(cap_state.clone());
        let db_path = Store::default_path().display().to_string();
        let export_path = Store::default_export_path().display().to_string();
        // T-33: the Settings address list is the single authority the row
        // pickers draw from. The dialog edits these cells; every commit goes
        // through the store, then the delegate's copied list is refreshed.
        let bind_hosts_cell = Rc::new(RefCell::new(self.store.bind_hosts()));
        let bind_host_error_cell: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let bind_host_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 192.168.1.20 or fd00::7"));
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let autostart_view = view.clone();
            let restore_view = view.clone();
            let autostart_cell = autostart_state.clone();
            let restore_cell = restore_state.clone();
            let auto_restart_view = view.clone();
            let auto_restart_cell = auto_restart_state.clone();

            let skip_delete_view = view.clone();
            let skip_delete_cell = skip_delete_state.clone();

            let appearance_render_view = appearance_view.clone();
            let appearance_render_cell = appearance_state.clone();
            let presets_render_view = presets_view.clone();
            let routing_summary_render = routing_summary.clone();
            let preset_summary_render = preset_summary.clone();
            let autostart_on = *autostart_state.borrow();
            let restore_on = *restore_state.borrow();
            let auto_restart_on = *auto_restart_state.borrow();
            let skip_delete_on = *skip_delete_state.borrow();
            let save_reader = test_url_reader.clone();
            let save_view = test_url_view.clone();
            let feed_reader = feed_url_reader.clone();
            let feed_view = feed_url_view.clone();
            let cap_save_view = view.clone();
            let cap_save_reader = cap_reader.clone();
            let copy_db_view = test_url_view.clone();
            let db_path_for_copy = db_path.clone();
            let copy_export_view = test_url_view.clone();
            let export_path_for_copy = export_path.clone();
            let clear_traffic_view = test_url_view.clone();
            let clear_health_view = test_url_view.clone();
            let clear_snapshot_view = test_url_view.clone();
            let bind_hosts_render_cell = bind_hosts_cell.clone();
            let bind_host_error_render_cell = bind_host_error_cell.clone();
            dialog
                .title("Settings")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()

                        .pb_4()

                        .child(
                            div()
                                .id("settings-section-basic")
                                .test_support()
                                .text_sm()
                                .child("Basic"),
                        )
                        .child(
                            div()
                                .id("settings-section-startup")
                                .test_support()
                                .text_sm()
                                .child("Startup"),
                        )
                        .child(
                            div()
                                .id("settings-autostart")
                                .test_support()
                                .child(
                                    Switch::new("settings-autostart-switch")
                                        .checked(autostart_on)
                                        .label("Start with Windows")
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
                                ),
                        )
                        .child(
                            div()
                                .id("settings-autostart-note")
                                .test_support()
                                .child(
                                    "On writes HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run \
                                     for this exe; off removes it. Default off; nothing writes silently.",
                                ),
                        )
                        .child(
                            div()
                                .id("settings-restore")
                                .test_support()
                                .child(
                                    Switch::new("settings-restore-switch")
                                        .checked(restore_on)
                                        .label("Restore running endpoints on launch")
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
                                ),
                        )
                        .child(
                            div()
                                .id("settings-restore-note")
                                .test_support()
                                .child(
                                    "On relaunches the previously running set. Loopback rows start \
                                     directly; LAN-bound rows still need their confirm. Default off.",
                                ),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-probe")
                                .test_support()
                                .text_sm()
                                .child("Probe"),
                        )
                        .child(dialog_field(
                            "field-settings-test-url",
                            "Test URL (blank = default)",
                            test_url_reader.clone(),
                        ))
                        .child(
                            div()
                                .id("settings-test-url-note")
                                .test_support()
                                .child(format!(
                                    "Probes GET this URL through each endpoint. \
                                     Default: {DEFAULT_TEST_URL}"
                                )),
                        )
                        .child(
                            Button::new("settings-save-test-url")
                                .label("Save test URL")
                                .on_click(move |_, window, cx| {
                                    let url = save_reader.read(cx).value().to_string();
                                    save_view.update(cx, |view, cx| {
                                        view.commit_test_url(&url, window, cx);
                                    });
                                }),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-update")
                                .test_support()
                                .text_sm()
                                .child("Update check"),
                        )
                        .child(dialog_field(
                            "field-settings-update-feed",
                            "Update feed URL (blank = default)",
                            feed_url_reader.clone(),
                        ))
                        .child(
                            div()
                                .id("settings-update-feed-note")
                                .test_support()
                                .child(format!(
                                    "Checking GETs this JSON feed (10s timeout, ≤5 redirects, \
                                     1 MiB cap) and compares tags; nothing downloads or runs. \
                                     Default: {UPDATE_FEED_URL_DEFAULT}"
                                )),
                        )
                        .child(
                            Button::new("settings-save-update-feed")
                                .label("Save feed URL")
                                .on_click(move |_, window, cx| {
                                    let url = feed_reader.read(cx).value().to_string();
                                    feed_view.update(cx, |view, cx| {
                                        view.commit_update_feed_url(&url, window, cx);
                                    });
                                }),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-engine")
                                .test_support()
                                .text_sm()
                                .child("Engine"),
                        )
                        .child(
                            div()
                                .id("settings-auto-restart")
                                .test_support()
                                .child(
                                    Switch::new("settings-auto-restart-switch")
                                        .checked(auto_restart_on)
                                        .label("Automatically restart crashed cores")
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
                                ),
                        )
                        .child(
                            div()
                                .id("settings-auto-restart-note")
                                .test_support()
                                .child(
                                    "On restarts a crashed core with backoff (up to 3 tries) and \
                                     tells you when it recovers. Default on.",
                                ),
                        )
                        .child(dialog_field(
                            "field-log-cap",
                            "Maximum kept log lines",
                            cap_reader.clone(),
                        ))
                        .child(
                            div()
                                .id("settings-log-cap-note")
                                .test_support()
                                .child(
                                    "Range 500–50,000 (default 5,000). Saving rebuilds the \
                                     log ring: lines already recorded are dropped.",
                                ),
                        )
                        .child(
                            Button::new("settings-save-log-cap")
                                .label("Save log cap")
                                .on_click(move |_, window, cx| {
                                    let raw = cap_save_reader.read(cx).value().to_string();
                                    cap_save_view.update(cx, |view, cx| {
                                        view.commit_log_cap(window, &raw, cx);
                                    });
                                }),
                        )

                        .child(
                            div()
                                .id("settings-ticks")
                                .test_support()
                                .child(format!("Timers: {}", tick_interval_summary())),
                        )

                        .child(
                            div()
                                .id("settings-skip-delete")
                                .test_support()
                                .child(
                                    Switch::new("settings-skip-delete-switch")
                                        .checked(skip_delete_on)
                                        .label("Skip delete confirms")
                                        .on_change(move |next: &bool, window, cx| {
                                            let next = *next;
                                            let cell = skip_delete_cell.clone();
                                            skip_delete_view.update(cx, |view, cx| {
                                                if view.commit_skip_delete_confirm(next, window, cx) {
                                                    *cell.borrow_mut() = next;
                                                    cx.notify();
                                                }
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("settings-skip-delete-note")
                                .test_support()
                                .child(
                                    "On deletes a row (or the checked set) at once. \
                                     Preset detach-deletes still ask. Default off.",
                                ),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-paths")
                                .test_support()
                                .text_sm()
                                .child("Paths"),
                        )
                        .child(
                            div()
                                .id("settings-db-path")
                                .test_support()
                                .child(format!("Data folder: {db_path}")),
                        )
                        .child(
                            Button::new("settings-copy-db-path")
                                .label("Copy data path")
                                .on_click(move |_, window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        db_path_for_copy.clone(),
                                    ));
                                    copy_db_view.update(cx, |_, cx| {
                                        window.push_notification(
                                            Notification::success("Copied data path"),
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .child(
                            div()
                                .id("settings-export-path")
                                .test_support()
                                .child(format!("Export default: {export_path}")),
                        )
                        .child(
                            Button::new("settings-copy-export-path")
                                .label("Copy export path")
                                .on_click(move |_, window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        export_path_for_copy.clone(),
                                    ));
                                    copy_export_view.update(cx, |_, cx| {
                                        window.push_notification(
                                            Notification::success("Copied export path"),
                                            cx,
                                        );
                                    });
                                }),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-maintenance")
                                .test_support()
                                .text_sm()
                                .child("Maintenance"),
                        )
                        .child(
                            Button::new("settings-clear-traffic")
                                .danger()
                                .label("Clear traffic counters")
                                .on_click(move |_, window, cx| {
                                    clear_traffic_view.update(cx, |view, cx| {
                                        view.open_clear_traffic_confirm(window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("settings-clear-health")
                                .danger()
                                .label("Clear health snapshots")
                                .on_click(move |_, window, cx| {
                                    clear_health_view.update(cx, |view, cx| {
                                        view.open_clear_health_confirm(window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("settings-clear-snapshot")
                                .danger()
                                .label("Reset running snapshot")
                                .on_click(move |_, window, cx| {
                                    clear_snapshot_view.update(cx, |view, cx| {
                                        view.open_clear_snapshot_confirm(window, cx);
                                    });
                                }),
                        )

                        .child(
                            div()
                                .id("settings-appearance-caption")
                                .test_support()
                                .child(Label::new("Appearance")),
                        )
                        .child(
                            div()
                                .id("settings-appearance-group")
                                .test_support()
                                .child({
                                    let current = appearance_render_cell.borrow().clone();
                                    let selected = APPEARANCE_CHOICES
                                        .iter()
                                        .position(|(value, _)| *value == current);
                                    let mut group = RadioGroup::horizontal(
                                        "settings-appearance-options",
                                    )
                                    .selected_index(selected)
                                    .on_change(move |ix, window, cx| {
                                        let Some((value, _)) = APPEARANCE_CHOICES.get(*ix)
                                        else {
                                            return;
                                        };
                                        let value = value.to_string();
                                        let cell = appearance_render_cell.clone();
                                        appearance_render_view.update(cx, |view, cx| {
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
                                        group = group.child(
                                            Radio::new(*label)
                                                .label(label.to_string())
                                                .tooltip(tip),
                                        );
                                    }
                                    group
                                }),
                        )
                        .child(
                            div()
                                .id("settings-appearance-note")
                                .test_support()
                                .child(
                                    "System follows the Windows app theme flag \
                                     (AppsUseLightTheme). A change paints at once — no restart.",
                                ),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-routing")
                                .test_support()
                                .text_sm()
                                .child("Routing"),
                        )
                        .child(
                            div()
                                .id("settings-routing-summary")
                                .test_support()
                                .child(routing_summary_render.clone()),
                        )
                        .child(
                            Button::new("settings-open-routing-presets")
                                .label("Manage routing presets")
                                .on_click(move |_, window, cx| {
                                    presets_render_view.update(cx, |view, cx| {
                                        view.open_presets_dialog(window, cx);
                                    });
                                }),
                        )
                        .child(
                            div()
                                .id("settings-routing-note")
                                .test_support()
                                .child(
                                    "Routing rules edit only in the presets manager \
                                     (proxy | direct | block — no system paths).",
                                ),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-preset")
                                .test_support()
                                .text_sm()
                                .child("Preset"),
                        )
                        .child(
                            div()
                                .id("settings-preset-summary")
                                .test_support()
                                .child(preset_summary_render.clone()),
                        )
                        .child(
                            div()
                                .id("settings-preset-note")
                                .test_support()
                                .child(
                                    "Attach a preset to a row from the inspector Presets pane.",
                                ),
                        )

                        .child(Separator::horizontal())
                        .child(
                            div()
                                .id("settings-section-hotkey")
                                .test_support()
                                .text_sm()
                                .child("Hotkey"),
                        )
                        .child(
                            div().id("settings-hotkey-list").test_support().children(
                                SHORTCUT_LINES
                                    .iter()
                                    .map(|(action, keys)| {
                                        div().child(format!("{action}: {keys}"))
                                    })
                                    .collect::<Vec<_>>(),
                            ),
                        )
                        .child(
                            div()
                                .id("settings-section-addresses")
                                .test_support()
                                .text_sm()
                                .child("Addresses"),
                        )
                        .child(
                            div()
                                .id("settings-addresses-note")
                                .test_support()
                                .child(
                                    "Rows pick their serve address from this list; ports stay \
                                     free-text. 127.0.0.1 and 0.0.0.0 are built in and cannot be \
                                     removed. Add IPv4/IPv6 literals only — no hostnames. A host a \
                                     row still uses cannot be removed.",
                                ),
                        )
                        .child({
                            let mut list = div().flex().flex_col().gap_1();
                            for host in bind_hosts_render_cell.borrow().clone() {
                                let constant =
                                    Store::BIND_HOST_CONSTANTS.contains(&host.as_str());
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
                                            .child("built in"),
                                    );
                                } else {
                                    let remove_view = view.clone();
                                    let remove_host = host.clone();
                                    let remove_cell = bind_hosts_cell.clone();
                                    let remove_error = bind_host_error_cell.clone();
                                    host_row = host_row.child(
                                        Button::new(format!(
                                            "settings-bind-host-remove-{host}"
                                        ))
                                        .small()
                                        .label("Remove")
                                        .accessibility_label(format!("Remove {host}"))
                                        .on_click(move |_, window, cx| {
                                            let remove_host = remove_host.clone();
                                            let remove_cell = remove_cell.clone();
                                            let remove_error = remove_error.clone();
                                            remove_view.update(cx, |view, cx| {
                                                match view.commit_remove_bind_host(
                                                    &remove_host,
                                                    window,
                                                    cx,
                                                ) {
                                                    Ok(hosts) => {
                                                        *remove_error.borrow_mut() = None;
                                                        *remove_cell.borrow_mut() = hosts;
                                                    }
                                                    Err(message) => {
                                                        *remove_error.borrow_mut() =
                                                            Some(message);
                                                    }
                                                }
                                                cx.notify();
                                            });
                                        }),
                                    );
                                }
                                list = list.child(host_row);
                            }
                            list
                        })
                        .child(dialog_field(
                            "field-settings-bind-host",
                            "Add address (IPv4 or IPv6 literal)",
                            bind_host_input.clone(),
                        ))
                        .child({
                            let add_view = view.clone();
                            let add_input = bind_host_input.clone();
                            let add_cell = bind_hosts_cell.clone();
                            let add_error = bind_host_error_cell.clone();
                            Button::new("settings-bind-host-add")
                                .label("Add address")
                                .on_click(move |_, window, cx| {
                                    let raw = add_input.read(cx).value().to_string();
                                    let add_input = add_input.clone();
                                    let add_cell = add_cell.clone();
                                    let add_error = add_error.clone();
                                    add_view.update(cx, |view, cx| {
                                        match view.commit_add_bind_host(&raw, window, cx) {
                                            Ok(hosts) => {
                                                add_input.update(cx, |state, cx| {
                                                    state.set_value(String::new(), window, cx)
                                                });
                                                *add_error.borrow_mut() = None;
                                                *add_cell.borrow_mut() = hosts;
                                            }
                                            Err(message) => {
                                                *add_error.borrow_mut() = Some(message);
                                            }
                                        }
                                        cx.notify();
                                    });
                                })
                        })
                        .child({
                            let error_text = bind_host_error_render_cell
                                .borrow()
                                .clone()
                                .unwrap_or_default();
                            div()
                                .id("settings-bind-host-error")
                                .test_support()
                                .child(error_text)
                        })
                        .child(
                            div().id("settings-tray-note").test_support().child(
                                "Closing the window hides to the tray while endpoints run; \
                                 Quit in the tray menu stops everything and exits.",
                            ),
                        ),
                )

                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "settings-dialog-ok",
                    "settings-dialog-cancel",

                    footer_commit_label("settings"),
                    Rc::new(|_, _| true),
                ))
        });
    }
}
