use crate::*;

const SING_BOX_RELEASES_URL: &str = "https://github.com/SagerNet/sing-box/releases";
const XRAY_RELEASES_URL: &str = "https://github.com/XTLS/Xray-core/releases";

impl AppView {
    pub(crate) fn open_alert_dialog_after_root<F>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: F,
    ) where
        F: Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static,
    {
        self.after_root(window, cx, move |_view, window, cx| {
            open_app_alert_dialog(window, cx, build);
        });
    }

    pub(crate) fn open_edit_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_edit_dialog(config, window, cx);
    }

    pub(crate) fn open_remove_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_remove_confirm(config, window, cx);
    }

    pub(crate) fn open_traffic_reset_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_traffic_reset_confirm(config, window, cx);
    }

    pub(crate) fn open_usage_reset_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_usage_reset_confirm(config, window, cx);
    }

    pub(crate) fn open_traffic_reset_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let totals = self
            .store
            .traffic_totals()
            .ok()
            .and_then(|map| map.get(&config.id).copied())
            .unwrap_or((0, 0));
        let description = format!(
            "Reset traffic counters for \"{}\"? Current totals: ↓{} ↑{}. \
             This zeroes the stored counters.",
            config.name,
            format_bytes(totals.0),
            format_bytes(totals.1)
        );
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Reset")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset traffic")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_reset_traffic(&id, window, cx));
                    true
                })
        });
    }

    pub(crate) fn open_clear_traffic_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let description =
            settings_clear_traffic_text(self.libraries.read(cx).delegate().counts().0);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Clear")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Clear traffic counters")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_clear_traffic(window, cx));
                    true
                })
        });
    }

    pub(crate) fn open_clear_health_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let description = settings_clear_health_text();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Clear")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Clear health snapshots")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_clear_health(window, cx));
                    true
                })
        });
    }

    pub(crate) fn open_clear_snapshot_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let description = settings_clear_snapshot_text();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Reset")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset running snapshot")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_clear_snapshot(window, cx));
                    true
                })
        });
    }

    pub(crate) fn open_share_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let link =
            crate::import::share_link_for_config(&config.protocol, &config.name, &config.params);
        let title = format!("Share \"{}\"", config.name);
        let commit: DialogCommit = Rc::new(move |window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
            window.push_notification(Notification::success("Copied share link"), cx);
            true
        });
        let commit_key = commit.clone();
        let dialog_link =
            crate::import::share_link_for_config(&config.protocol, &config.name, &config.params);
        let payloads = share_qr_payloads(&config);
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(title.clone())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div().id("share-warning").test_support().child(
                                Alert::error("share-warning-box", SHARE_WARNING.to_string())
                                    .title("Share links carry credentials"),
                            ),
                        )
                        .child(
                            div()
                                .id("share-link-text")
                                .test_support()
                                .text_sm()
                                .child(dialog_link.clone()),
                        )
                        // T-28: the credential-free contact strings, one QR per scheme,
                        // as a transcription of the same text (never of the share link).
                        .child(share_qr_region(&payloads, cx)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "share-dialog-ok",
                    "share-dialog-cancel",
                    footer_commit_label("share"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_lan_confirm_single(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let description = lan_confirm_text(&config.name, &config.bind_ip);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Start"),
                )
                .title(LAN_CONFIRM_TITLE)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
                    true
                })
        });
    }

    pub(crate) fn open_tray_exit_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_preset_chooser_for_selected(window, cx);
    }

    pub(crate) fn open_preset_chooser_for_selected(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.selected_config(cx) {
            Some(config) => self.open_preset_attach_dialog(config, window, cx),
            None => window.push_notification(Notification::info(no_selection_message()), cx),
        }
    }

    pub(crate) fn open_runtime_stats_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let uptime = format_uptime(self.started_at.elapsed().as_secs());
        let running = self.engine.running_count();
        let rows = self.store.list().unwrap_or_default();
        let total = rows.len();
        let running_binds: Vec<String> = rows
            .iter()
            .filter(|row| self.engine.is_running(&row.id))
            .map(|row| format!("{}:{}", row.bind_ip, row.port))
            .collect();
        let traffic_line =
            summed_traffic_text(&self.store).unwrap_or_else(|| "usage unavailable".to_string());
        let conns = self.conn_snapshot.len();
        let (delta_rx, delta_tx) = self
            .conn_delta
            .values()
            .fold((0u64, 0u64), |(rx, tx), (d_rx, d_tx)| {
                (rx.saturating_add(*d_rx), tx.saturating_add(*d_tx))
            });
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let now = now_unix();
        let due = subs
            .iter()
            .filter(|sub| due_for_refresh_for(sub, now))
            .count();
        let subs_line = if self.store.subs_auto_refresh_enabled() {
            format!("{} subscription(s), {due} due", subs.len())
        } else {
            format!(
                "{} subscription(s), {due} due (auto-refresh off)",
                subs.len()
            )
        };
        let db_line = self
            .store
            .db_path()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| format_bytes(meta.len()))
            .unwrap_or_else(|| "unknown".to_string());
        let selected_line = match self.selected_config(cx) {
            Some(config) => {
                let health = self.health_store.row_summary(&config.id);
                let health = health.trim();
                if health.is_empty() {
                    format!("{} (no probes this session)", config.name)
                } else {
                    format!("{} ({health})", config.name)
                }
            }
            None => "none selected".to_string(),
        };
        let shard_rows = shard_stat_rows(
            &self.engine.shard_processes(),
            &self.shard_rings,
            &self.shard_denied,
        );
        let mut lines = vec![
            format!("Uptime: {uptime}"),
            format!("Endpoints: {running} running / {total} stored"),
            format!(
                "Running: {}",
                if running_binds.is_empty() {
                    "—".to_string()
                } else {
                    running_binds.join(", ")
                }
            ),
            format!("Traffic: {traffic_line}"),
            format!("Connections: {conns} live"),
            format!(
                "Session Δ: ↓{} ↑{}",
                format_bytes(delta_rx),
                format_bytes(delta_tx)
            ),
            format!("Subscriptions: {subs_line}"),
            format!("Database: {db_line}"),
            format!("Selected: {selected_line}"),
        ];
        lines.push(format!("Processes: {}", shard_stats_summary(&shard_rows)));
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let mut body = div().flex().flex_col().gap_1();
            for (ix, line) in lines.iter().enumerate() {
                body = body.child(
                    div()
                        .id(format!("runtime-stat-{ix}"))
                        .test_support()
                        .text_sm()
                        .child(line.clone()),
                );
            }
            body =
                body.child(
                    div()
                        .id("runtime-shard-stats")
                        .test_support()
                        .mt_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_xs().child(
                            "Sidecar processes — CPU % of one core, RAM in MB, last 60 ticks",
                        )),
                );
            for row in shard_rows.iter() {
                body = body.child(shard_stat_row_view(row));
            }
            dialog
                .title("Runtime stats")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "runtime-stats-ok",
                    "runtime-stats-cancel",
                    footer_commit_label("runtime-stats"),
                    Rc::new(|_, _| true),
                ))
        });
    }

    pub(crate) fn open_traffic_stats_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let period = self.traffic_period;
        let rows = self.store.list().unwrap_or_default();
        let totals = self.store.traffic_totals().unwrap_or_default();
        let exits = aggregate_exit_traffic(&rows, &totals);
        let (exit_count, rx, tx) = store::Store::traffic_period_summary(&totals, period);
        let summary = traffic_stats_summary_text(period, exit_count, rx, tx);
        let split = traffic_top9_split(&exits);
        let max = traffic_bar_max(&exits);
        let cycle_view = view.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let cycle_view = cycle_view.clone();
            let mut body = div().flex().flex_col().gap_1();
            body = body.child(
                div()
                    .id("traffic-stats-summary")
                    .test_support()
                    .text_sm()
                    .child(summary.clone()),
            );
            body = body.child(
                Button::new("traffic-stats-period")
                    .small()
                    .label(format!("Period: {}", period.label()))
                    .tooltip("Cycle 24h → 7d → 30d → 90d (all-time totals throughout)")
                    .on_click(move |_, window, cx| {
                        cycle_view.update(cx, |view, cx| {
                            let next = match view.traffic_period {
                                store::TrafficPeriod::Day => store::TrafficPeriod::Week,
                                store::TrafficPeriod::Week => store::TrafficPeriod::Month,
                                store::TrafficPeriod::Month => store::TrafficPeriod::Quarter,
                                store::TrafficPeriod::Quarter => store::TrafficPeriod::Day,
                            };
                            view.traffic_period = next;
                            window.close_all_dialogs(cx);
                            reclaim_shell_focus(window, cx);
                            view.open_traffic_stats_dialog(window, cx);
                        });
                    }),
            );
            if exits.is_empty() {
                body = body.child(
                    div()
                        .id("traffic-stats-empty")
                        .test_support()
                        .text_sm()
                        .child("No traffic recorded yet — start an exit and pass data."),
                );
            }
            for exit in &split.top {
                body = body.child(
                    div()
                        .id(format!("traffic-stat-{}", exit.row_id))
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "{} {} · {} ↓{} ↑{}",
                            traffic_stacked_bar(exit.rx_bytes, exit.tx_bytes, max),
                            exit.name,
                            exit.bind,
                            format_bytes(exit.rx_bytes),
                            format_bytes(exit.tx_bytes),
                        )),
                );
            }
            if let Some(other) = split.other_line() {
                body = body.child(
                    div()
                        .id("traffic-stats-other")
                        .test_support()
                        .text_sm()
                        .child(other),
                );
            }
            dialog
                .title("Traffic stats")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "traffic-stats-ok",
                    "traffic-stats-cancel",
                    footer_commit_label("traffic-stats"),
                    Rc::new(|_, _| true),
                ))
        });
    }

    pub(crate) fn open_about_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sidecars = self.sidecar_presence.clone();
        let about_view = cx.entity();
        let body = about_body(
            APP_VERSION,
            GIT_HASH,
            &sidecars,
            &Store::default_path().display().to_string(),
            &Store::default_export_path().display().to_string(),
        );
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let check_view = about_view.clone();
            dialog
                .title(format!("About {APP_TITLE}"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .id("about-body")
                                .test_support()
                                .text_sm()
                                .child(body.clone()),
                        )
                        .child(
                            Button::new("about-check-for-updates")
                                .small()
                                .label("Check for updates")
                                .tooltip("Fetch the release feed once (JSON only — nothing downloads or runs)")
                                .on_click(move |_, window, cx| {
                                    window.close_all_dialogs(cx);
                                    reclaim_shell_focus(window, cx);
                                    check_view.update(cx, |view, cx| {
                                        view.check_for_updates(window, cx);
                                    });
                                }),
                        ),
                )
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "about-dialog-ok",
                    "about-dialog-cancel",
                    "Close",
                    Rc::new(|_, _| true),
                ))
        });
    }

    pub(crate) fn open_update_check_dialog(
        &mut self,
        outcome: UpdateCheckOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, message, release_name, release_url) =
            update_check_presentation(&outcome, APP_VERSION);
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let mut body = div().flex().flex_col().gap_1();
            body = body.child(
                div()
                    .id(id.to_string())
                    .test_support()
                    .text_sm()
                    .child(message.clone()),
            );
            if let Some(name) = release_name.clone() {
                body = body.child(
                    div()
                        .id("update-release-name")
                        .test_support()
                        .text_sm()
                        .child(name),
                );
            }
            if let Some(url) = release_url.clone() {
                body = body.child(
                    Button::new("update-open-releases")
                        .small()
                        .label("Open releases in browser")
                        .tooltip(format!(
                            "Opens {url} in your own browser — the app downloads nothing"
                        ))
                        .on_click(move |_, window, cx| {
                            if let Err(why) = open_release_in_browser(&url) {
                                window.push_notification(Notification::error(why), cx);
                            }
                        }),
                );
            }
            dialog
                .title("Check for updates")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "update-dialog-ok",
                    "update-dialog-cancel",
                    "Close",
                    Rc::new(|_, _| true),
                ))
        });
    }

    /// F4: the missing-core hint needs somewhere actionable after its toast
    /// hides. No markdown viewer exists in the kit, so this dialog renders
    /// `docs/user/cores.md`'s three facts in place, with the project pages one
    /// browser-open away — the same affordance the update dialog uses.
    pub(crate) fn open_cores_help_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let body = div()
                .id("cores-help-body")
                .test_support()
                .flex()
                .flex_col()
                .gap_2()
                .text_sm()
                .child(format!(
                    "ManyProxy runs executables you place beside ManyProxy.exe — {} (stock), {} (AmneziaWG fork, only AmneziaWG/Noise/XHTTP rows need it) and {} (only Xray-only rows).",
                    engine_binary_file_name(),
                    fork_binary_file_name(),
                    xray_binary_file_name(),
                ))
                .child(
                    Button::new("cores-help-open-sing-box")
                        .small()
                        .label("Open sing-box releases in browser")
                        .tooltip(format!(
                            "Opens {} in your own browser — the app downloads nothing",
                            SING_BOX_RELEASES_URL
                        ))
                        .on_click({
                            let url = SING_BOX_RELEASES_URL.to_string();
                            move |_, window, cx| {
                                if let Err(why) = open_release_in_browser(&url) {
                                    window.push_notification(Notification::error(why), cx);
                                }
                            }
                        }),
                )
                .child(
                    Button::new("cores-help-open-xray")
                        .small()
                        .label("Open Xray-core releases in browser")
                        .tooltip(format!(
                            "Opens {} in your own browser — the app downloads nothing",
                            XRAY_RELEASES_URL
                        ))
                        .on_click({
                            let url = XRAY_RELEASES_URL.to_string();
                            move |_, window, cx| {
                                if let Err(why) = open_release_in_browser(&url) {
                                    window.push_notification(Notification::error(why), cx);
                                }
                            }
                        }),
                )
                .child(
                    div()
                        .id("cores-help-note")
                        .test_support()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Engines are read from the app folder only; PATH is never searched. ManyProxy never downloads executables silently — you download and install them yourself.",
                        ),
                );
            dialog
                .title("Where do I get cores?")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "cores-help-dialog-close",
                    "cores-help-dialog-cancel",
                    "Close",
                    Rc::new(|_, _| true),
                ))
        });
    }

    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if overlay_open(window, cx) {
            return;
        }
        let state = cx.new(|cx| CommandState::new(window, cx));
        self.palette = Some(state.clone());
        let view = cx.entity();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            dialog.title("Command palette").child(
                Command::new(&state)
                    .placeholder("Type a command…")
                    .group(CommandGroup::new().label("Library").items(vec![
                            CommandItem::new()
                                .label("Add configuration")
                                .keywords(["new", "create"]),
                            CommandItem::new()
                                .label("Import configurations")
                                .keywords(["import", "link", "file"]),
                            CommandItem::new()
                                .label("Start selected")
                                .keywords(["start", "serve", "f5"]),
                            CommandItem::new().label("Stop selected").keywords(["stop"]),
                            CommandItem::new()
                                .label("Test selected")
                                .keywords(["test", "probe", "health"]),
                            CommandItem::new().label("Copy URLs").keywords([
                                "copy",
                                "clipboard",
                                "url",
                                "endpoint",
                            ]),
                            CommandItem::new()
                                .label("Edit selected")
                                .keywords(["edit", "rename"]),
                            CommandItem::new()
                                .label("Delete selected")
                                .keywords(["delete", "remove"]),
                            CommandItem::new()
                                .label("Focus search field")
                                .keywords(["search", "find", "filter"]),
                            CommandItem::new()
                                .label("Clear checked")
                                .keywords(["clear", "uncheck", "escape"]),
                            CommandItem::new()
                                .label("Show Details tab")
                                .keywords(["details", "tab"]),
                            CommandItem::new()
                                .label("Show Health tab")
                                .keywords(["health", "tab"]),
                            CommandItem::new()
                                .label("Import from clipboard")
                                .keywords(["clipboard", "paste", "import"]),
                            CommandItem::new()
                                .label("Export library")
                                .keywords(["export", "backup", "json"]),
                        ]))
                    .group(CommandGroup::new().label("Manage").items(vec![
                            CommandItem::new()
                                .label("Subscriptions")
                                .keywords(["subscription", "refresh"]),
                            CommandItem::new()
                                .label("Routing presets")
                                .keywords(["preset", "routing", "rules"]),
                            CommandItem::new()
                                .label("Groups")
                                .keywords(["group", "move"]),
                            CommandItem::new().label("Logs").keywords(["log", "output"]),
                            CommandItem::new()
                                .label("Settings")
                                .keywords(["settings", "options", "test url"]),
                            CommandItem::new().label("About").keywords([
                                "about",
                                "version",
                                "shortcuts",
                            ]),
                            CommandItem::new()
                                .label("Start all")
                                .keywords(["start", "all", "serve"]),
                            CommandItem::new()
                                .label("Stop all")
                                .keywords(["stop", "all"]),
                            CommandItem::new()
                                .label("Test all")
                                .keywords(["test", "all", "probe"]),

                            CommandItem::new()
                                .label("Runtime stats")
                                .keywords(["runtime", "stats", "health", "cpu", "memory"]),
                            CommandItem::new()
                                .label("Traffic stats")
                                .keywords(["traffic", "stats", "usage", "profile"]),
                            CommandItem::new()
                                .label("Row columns")
                                .keywords(["column", "columns", "rows", "display"]),
                        ]))
                    .on_confirm(move |index, window, cx| {
                        view.update(cx, |view, cx| view.run_palette_op(index, window, cx));
                    }),
            )
        });
    }

    pub(crate) fn open_prune_confirm(
        &mut self,
        kind: GroupPruneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            GroupPruneKind::ALL.contains(&kind),
            "prune kinds stay the pinned four"
        );
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let (title, description, ok) = match kind {
            GroupPruneKind::Duplicates => {
                let (total, preview) = self
                    .store
                    .duplicate_preview_in_group(&group)
                    .unwrap_or((0, Vec::new()));
                if total == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no duplicates")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Remove {total} duplicate(s) in group \"{group}\"?"),
                    prune_duplicates_confirm_text(&group, total, &preview),
                    "Remove",
                )
            }
            GroupPruneKind::Unavailable => {
                let count = self
                    .store
                    .unavailable_rows_in_group(&group)
                    .map(|rows| rows.len())
                    .unwrap_or(0);
                if count == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no unavailable rows")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Remove {count} unavailable row(s) in group \"{group}\"?"),
                    prune_unavailable_confirm_text(&group, count),
                    "Remove",
                )
            }
            GroupPruneKind::Invalid => {
                let count = self
                    .store
                    .invalid_rows_in_group(&group)
                    .map(|rows| rows.len())
                    .unwrap_or(0);
                if count == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no invalid rows")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Remove {count} invalid row(s) in group \"{group}\"?"),
                    prune_invalid_confirm_text(&group, count),
                    "Remove",
                )
            }
            GroupPruneKind::ClearTests => {
                let count = self.group_test_sample_count(&group);
                if count == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no test results")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Clear {count} test result(s) in group \"{group}\"?"),
                    clear_group_tests_confirm_text(&group, count),
                    "Clear",
                )
            }
        };
        let view = cx.entity();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let title = title.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text(ok)
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(title)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_prune(kind, window, cx));
                    true
                })
        });
    }

    /// T-19: toggling auto-clear on opens this note once per group; the sheet
    /// origin re-opens the sheet on every exit so the switch re-reads the
    /// store, the facet origin just lets the window redraw revert visuals.
    pub(crate) fn open_auto_clear_opt_in_confirm(
        &mut self,
        group: &str,
        from_groups_sheet: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let group = group.to_string();
        let title = format!("Enable auto-clear in group \"{group}\"?");
        let description = auto_clear_opt_in_note(&group);
        focus_shell_before_overlay(window, cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let group = group.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Enable"),
                )
                .title(title.clone())
                .description(description.clone())
                .on_ok({
                    let view = view.clone();
                    let group = group.clone();
                    move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.commit_auto_clear_opt_in(&group, window, cx);
                            if from_groups_sheet {
                                window.close_all_dialogs(cx);
                                reclaim_shell_focus(window, cx);
                                view.open_groups_dialog(window, cx);
                            }
                        });
                        !from_groups_sheet
                    }
                })
                .on_cancel({
                    let view = view.clone();
                    move |_, window, cx| {
                        if !from_groups_sheet {
                            return true;
                        }
                        view.update(cx, |exposed, cx| {
                            window.close_all_dialogs(cx);
                            reclaim_shell_focus(window, cx);
                            exposed.open_groups_dialog(window, cx);
                        });
                        false
                    }
                })
                .on_close({
                    let view = view.clone();
                    move |_, window, cx| {
                        if !from_groups_sheet {
                            return;
                        }
                        view.update(cx, |exposed, cx| {
                            window.close_all_dialogs(cx);
                            exposed.open_groups_dialog(window, cx);
                        });
                        reclaim_shell_focus(window, cx);
                    }
                })
        });
    }

    pub(crate) fn open_logs_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A control-driven swap hands over the live draft; otherwise seed from
        // the committed filter fields.
        let (current, current_exclude, current_pattern) =
            self.log_reopen_draft.take().unwrap_or_else(|| {
                (
                    self.log_filter.clone(),
                    self.log_exclude.clone(),
                    self.log_regex.clone(),
                )
            });
        let filter_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Filter logs")
                .default_value(current)
        });

        let exclude_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Exclude keyword")
                .default_value(current_exclude)
        });
        let pattern_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Regex (empty = off)")
                .default_value(current_pattern)
        });
        self.log_inputs = Some((
            filter_state.clone(),
            exclude_state.clone(),
            pattern_state.clone(),
        ));
        let view = cx.entity();
        let reader = filter_state.clone();
        let exclude_reader = exclude_state.clone();
        let pattern_reader = pattern_state.clone();
        let lines = self.log_lines(cx);

        self.log_list = Some(cx.new(|cx| {
            ListState::new(
                LogLinesDelegate {
                    lines: lines.clone(),
                },
                window,
                cx,
            )
            .selectable(false)
        }));
        let log_list_state = self.log_list.clone();
        let title = match self.selected_config(cx) {
            Some(config) => format!("Logs — {}", config.name),
            None => "Logs".to_string(),
        };

        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |_window, cx| {
            let filter = reader.read(cx).value().to_string();
            let exclude = exclude_reader.read(cx).value().to_string();
            let pattern = pattern_reader.read(cx).value().to_string();
            commit_view.update(cx, |view, cx| {
                view.apply_log_filters(filter, exclude, pattern, cx);
            });
            true
        });

        self.logs_open = true;
        self.logs_seen = self.logs.push_seq();

        let chrome_view = view.clone();
        let chrome_level = self.log_level;
        let chrome_tail = self.log_tail;

        let chrome_filter_active = !self.log_filter.trim().is_empty();
        focus_shell_before_overlay(window, cx);
        window.open_sheet(cx, move |sheet: Sheet, _, _| {
            let commit_click = commit.clone();

            let (current_level, tail_on) = (chrome_level, chrome_tail);

            const LOG_LEVEL_LABELS: [&str; 5] = ["All", "Debug", "Info", "Warn", "Error"];
            let selected_level = LOG_LEVEL_LABELS
                .iter()
                .position(|label| LogLevel::from_label(label) == current_level);
            let mut level_group = RadioGroup::horizontal("log-level-options")
                .selected_index(selected_level)
                .on_change({
                    let group_view = chrome_view.clone();
                    move |ix, window, cx| {
                        let level = LOG_LEVEL_LABELS
                            .get(*ix)
                            .map(|label| LogLevel::from_label(label))
                            .unwrap_or(current_level);
                        let level_view = group_view.clone();
                        level_view.update(cx, |view, cx| {
                            view.set_log_level(level, window, cx);
                        });
                    }
                });
            for label in LOG_LEVEL_LABELS {
                level_group = level_group.child(
                    Radio::new(label)
                        .label(label)
                        .tooltip(log_level_tooltip(label)),
                );
            }
            let level_row = div().id("log-level-row").test_support().child(level_group);
            let copy_view = chrome_view.clone();
            let tail_view = chrome_view.clone();
            let save_view = chrome_view.clone();
            let clear_view = chrome_view.clone();

            let mut action_row = div()
                .id("log-actions-row")
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    Button::new("log-copy-visible")
                        .small()
                        .label("Copy visible")
                        .tooltip("Copy the visible log lines (redacted)")
                        .on_click(move |_, window, cx| {
                            copy_view.update(cx, |view, cx| {
                                view.copy_visible_logs(window, cx);
                            });
                        }),
                );
            if tail_on {
                action_row = action_row.child(
                    div()
                        .id("log-tail-spinner")
                        .test_support()
                        .child(Spinner::new().small()),
                );
            }
            action_row = action_row
                .child(
                    Button::new("log-tail-toggle")
                        .small()
                        .min_w(px(120.))
                        .label(if tail_on { "Pause tail" } else { "Resume tail" })
                        .tooltip(if tail_on {
                            "Pause the live tail"
                        } else {
                            "Resume the live tail"
                        })
                        .on_click(move |_, window, cx| {
                            tail_view.update(cx, |view, cx| {
                                view.toggle_log_tail(window, cx);
                            });
                        }),
                )
                .child(
                    Button::new("logs-save")
                        .small()
                        .label("Save")
                        .tooltip("Save the visible redacted lines to the logs file")
                        .on_click(move |_, window, cx| {
                            save_view.update(cx, |view, cx| {
                                view.save_visible_logs(window, cx);
                            });
                        }),
                );
            let save_path_text = logs_save_path_text();
            let mut body =
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(level_row)
                    .child(action_row)

                    .child(div().id("log-redact-note").test_support().text_sm().child(
                        "Copy and Save carry redacted lines — secrets never leave this dialog.",
                    ))
                    .child(
                        div()
                            .id("log-save-path")
                            .test_support()
                            .text_sm()
                            .child(save_path_text),
                    )
                    .child(dialog_field(
                        "field-log-filter",
                        "Filter",
                        filter_state.clone(),
                    ))
                    .child(dialog_field(
                        "field-log-exclude",
                        "Exclude",
                        exclude_state.clone(),
                    ))
                    .child(dialog_field(
                        "field-log-regex",
                        "Regex",
                        pattern_state.clone(),
                    ));
            let mut list = div().id("log-lines").test_support().w_full();

            let log_list_viewport = log_list_state.as_ref();
            if lines.is_empty() {
                let clear_filter_view = clear_view.clone();
                list = list.child(
                    Empty::new()
                        .header(
                            EmptyHeader::new()
                                .title(EmptyTitle::new().child("No log lines to show"))
                                .description(EmptyDescription::new().child(
                                    if chrome_filter_active || current_level.is_some() {
                                        "No lines match the current filter — clear it to see everything."
                                    } else {
                                        "No log lines recorded yet."
                                    },
                                )),
                        )
                        .content(
                            EmptyContent::new().child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .child(
                                        Button::new("log-clear-filter")
                                            .small()
                                            .label("Clear filter")
                                            .tooltip("Clear the log text filter and level")
                                            .on_click(move |_, window, cx| {
                                                clear_filter_view.update(cx, |view, cx| {
                                                    view.clear_log_filter(window, cx);
                                                });
                                            }),
                                    ),
                            ),
                        ),
                );
            } else if let Some(state) = log_list_viewport {
                list = list.child(List::new(state).h(px(320.)).w_full());
            } else {
                for line in &lines {
                    list = list.child(line.clone());
                }
            }
            body = body.child(list);
            let close_view = chrome_view.clone();
            sheet
                .title(title.clone())
                .child(body)
                .size(px(520.))
                .on_close(move |_, window, cx| {
                    close_view.update(cx, |view, _| {
                        view.logs_open = false;
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(sheet_footer(
                    "logs-dialog-ok",
                    Some("logs-dialog-cancel"),

                    footer_commit_label("logs"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_export_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();

        let Some(rows) = self.list_or_notify(window, cx) else {
            return;
        };
        let default_path = Store::default_export_path().display().to_string();
        let path_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Export file path")
                .default_value(default_path)
        });
        let preview = export_preview_description(
            rows.len(),
            count_importable(&rows),
            &Store::default_export_path().display().to_string(),
            Store::default_export_path().exists(),
        );
        let warning = export_confirm_description().to_string();
        let reader = path_state.clone();
        let commit_view = view.clone();

        let commit: DialogCommit = Rc::new(move |window, cx| {
            let path = reader.read(cx).value().to_string();
            commit_view.update(cx, |view, cx| view.commit_export_dialog(path, window, cx))
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title("Export library")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div().id("export-warning").test_support().child(
                                Alert::error("export-warning-box", warning.clone())
                                    .title("Secrets leave in plaintext"),
                            ),
                        )
                        .child(div().id("export-safer-path").test_support().child(
                            "Safer: share one endpoint with Copy URLs — it never carries secrets.",
                        ))
                        .child(
                            div()
                                .id("export-preview")
                                .test_support()
                                .child(preview.clone()),
                        )
                        .child(dialog_field(
                            "field-export-path",
                            "Export file path",
                            path_state.clone(),
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "export-dialog-ok",
                    "export-dialog-cancel",
                    footer_commit_label("export"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_restore_confirm(
        &mut self,
        payload: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let payload = payload.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Restore")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Restore library")
                .description(format!(
                    "This replaces the current library with the exported backup \
                     (row ids, groups, enabled flags, subscriptions, and deletions \
                     are restored exactly). {}",
                    crate::store::RESTORE_REQUIRES_CLAUSE
                ))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_library_restore(&payload, window, cx)
                    });
                    true
                })
        });
    }

    pub(crate) fn open_row_columns_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live: Rc<RefCell<RowColumns>> = Rc::new(RefCell::new(self.row_columns));
        let view = cx.entity();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            let current = *live.borrow();
            let mut body = div().flex().flex_col().gap_2();
            for (id, label, tip, enabled) in [
                (
                    "columns-upstream",
                    "Upstream",
                    "Show the upstream host:port on each row",
                    current.upstream,
                ),
                (
                    "columns-health",
                    "Health",
                    "Show serving state and probe evidence on each row",
                    current.health,
                ),
                (
                    "columns-group",
                    "Group",
                    "Show the group tag on each row",
                    current.group,
                ),
                (
                    "columns-traffic",
                    "Traffic",
                    "Show data-plane totals on serving rows",
                    current.traffic,
                ),
            ] {
                let column = match id {
                    "columns-upstream" => "upstream",
                    "columns-health" => "health",
                    "columns-group" => "group",
                    _ => "traffic",
                };
                let live = live.clone();
                let switch_view = view.clone();
                body = body.child(
                    Switch::new(id)
                        .checked(enabled)
                        .label(label)
                        .accessibility_label(label)
                        .tooltip(tip)
                        .on_change(move |enabled: &bool, window, cx| {
                            let enabled = *enabled;
                            {
                                let mut current = live.borrow_mut();
                                match column {
                                    "upstream" => current.upstream = enabled,
                                    "health" => current.health = enabled,
                                    "group" => current.group = enabled,
                                    _ => current.traffic = enabled,
                                }
                            }
                            switch_view.update(cx, |view, cx| {
                                view.set_row_column(column, enabled, window, cx);
                            });
                        }),
                );
            }
            let close: DialogCommit = Rc::new(|_, _| true);
            dialog
                .title("Row columns")
                .child(body)
                .footer(dialog_footer(
                    "columns-dialog-close",
                    "columns-dialog-cancel",
                    "Close",
                    close,
                ))
        });
    }

    pub(crate) fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut form = ConfigForm::blank(window, cx);
        let view = cx.entity();

        let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let error_for_commit = form_error.clone();

        let rows = self.store.list().unwrap_or_default();

        let library: Vec<ChainLibraryEntry> = rows
            .iter()
            .map(|row| ChainLibraryEntry {
                id: row.id.clone(),
                name: row.name.clone(),
                protocol: row.protocol.clone(),
            })
            .collect();
        form.chain_library = Rc::new(RefCell::new(library.clone()));
        form.chain_pick = ConfigForm::panel_select(window, cx, chain_pick_choices(&library), "");
        let reader = form.clone();

        let budget = ConfigForm::port_budget_text(&rows);

        let element_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_with_focus(&reader, &error_for_commit, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, window, cx| {
            form.sync_protocol_template(window, cx);
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();

            let live = form.clone();
            let bind = live.bind.read(cx).value().to_string();
            let port = live.port.read(cx).value().to_string();
            let collision = bind_port_collision_warning(&rows, None, &bind, &port);
            let inline = form_error.borrow().clone();
            dialog
                .title("Add proxy configuration")
                // The guided two-column form needs more than the kit's 448px
                // default: at 448 the Server sub-field clipped even the short
                // `example.com` placeholder. 560 keeps both columns whole at
                // the 880px window minimum.
                .w(px(560.))
                .overlay_closable(false)
                .child(form.clone().element(
                    element_view.clone(),
                    cx,
                    collision,
                    inline,
                    false,
                    Some(budget.clone()),
                ))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "add-dialog-ok",
                    "add-dialog-cancel",
                    footer_commit_label("add"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_edit_dialog(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut form = ConfigForm::prefilled(
            window,
            cx,
            &config,
            self.store.row_test_url(&config.id).as_deref(),
        );
        let view = cx.entity();
        let id = config.id.clone();
        // T-19: protecting the row under edit from auto-clear starts here and
        // ends with one of the dialog's exits (commit success, footer Cancel,
        // Esc close) plus the reload guard for anything that slips past.
        self.edit_form_open = Some(id.clone());
        let title = format!("Edit {}", config.name);

        let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let error_for_commit = form_error.clone();

        let rows = self.store.list().unwrap_or_default();

        let library: Vec<ChainLibraryEntry> = rows
            .iter()
            .map(|row| ChainLibraryEntry {
                id: row.id.clone(),
                name: row.name.clone(),
                protocol: row.protocol.clone(),
            })
            .collect();
        form.chain_library = Rc::new(RefCell::new(library.clone()));
        form.chain_pick = ConfigForm::panel_select(window, cx, chain_pick_choices(&library), "");
        let reader = form.clone();

        let budget = ConfigForm::port_budget_text(&rows);
        let exclude = id.clone();

        let snapshot = config.clone();
        let element_view = view.clone();
        let track_source = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let patch = reader.as_patch_vs(cx, &snapshot);
            let url = reader.test_url.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_edit_with_test_url(
                    &id,
                    patch,
                    &url,
                    &reader,
                    &error_for_commit,
                    window,
                    cx,
                )
            })
        });
        let commit_key = commit.clone();
        focus_shell_before_overlay(window, cx);
        window.open_dialog(cx, move |dialog: Dialog, window, cx| {
            form.sync_protocol_template(window, cx);
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let live = form.clone();
            let bind = live.bind.read(cx).value().to_string();
            let port = live.port.read(cx).value().to_string();
            let collision = bind_port_collision_warning(&rows, Some(&exclude), &bind, &port);
            let inline = form_error.borrow().clone();
            dialog
                .title(title.clone())
                // Match Add: the guided form plus its hint/advanced row needs
                // more than the kit's 448px default, or the hint truncates to
                // an ellipsis and the two dialog buttons crowd its edge.
                .w(px(560.))
                .overlay_closable(false)
                .close_button(false)
                .on_close({
                    let track_view = track_source.clone();
                    move |_, window, cx| {
                        track_view.update(cx, |view, _| view.edit_form_open = None);
                        reclaim_shell_focus(window, cx);
                    }
                })
                .child(form.clone().element(
                    element_view.clone(),
                    cx,
                    collision,
                    inline,
                    true,
                    Some(budget.clone()),
                ))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(
                    // T-19: the shared footer closes with close_all_dialogs and
                    // would leave edit_form_open set; the Cancel click must
                    // release the row-under-edit protection itself.
                    DialogFooter::new()
                        .child(
                            Button::new("edit-dialog-cancel")
                                .small()
                                .label("Cancel")
                                .on_click({
                                    let track_view = track_source.clone();
                                    move |_, window, cx| {
                                        track_view.update(cx, |view, _| {
                                            view.edit_form_open = None;
                                        });
                                        window.close_all_dialogs(cx);
                                        reclaim_shell_focus(window, cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("edit-dialog-ok")
                                .small()
                                .label(footer_commit_label("edit"))
                                .on_click(move |_, window, cx| {
                                    if commit_click(window, cx) {
                                        window.close_all_dialogs(cx);
                                        reclaim_shell_focus(window, cx);
                                    }
                                }),
                        ),
                )
        });
    }

    pub(crate) fn open_remove_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // K5: deletion is immediate — no blocking confirm for a single row.
        // The undo toast (commit_remove) is the safety net; the rows stay in
        // memory for its lifetime and vanish with it. K17 ruling: the bulk
        // More ▾ path is immediate too (spec line 91) — same pipeline.
        self.commit_remove(&config.id, window, cx);
    }

    pub(crate) fn open_usage_reset_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let name = config.name.clone();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Reset")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset usage counters")
                .description(format!(
                    "Reset usage counters for \"{name}\" to zero? This cannot be undone."
                ))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_usage_reset(&id, window, cx));
                    true
                })
        });
    }

    pub(crate) fn open_import_dialog(
        &mut self,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_import_dialog_with_files(
            prefill_text,
            prefill_group,
            file_hint,
            None,
            window,
            cx,
        );
    }

    pub(crate) fn open_import_dialog_with_files(
        &mut self,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
        multi_files: Option<Vec<(String, String)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = match multi_files {
            Some(files) => {
                ImportForm::new(window, cx, prefill_text, prefill_group, None, Some(files))
            }
            None => ImportForm::new(window, cx, prefill_text, prefill_group, file_hint, None),
        };
        // F7: the open form is the drop target, and a stale marker must never
        // outlive its dialog — every close below clears it.
        self.import_form = Some(form.clone());
        let view = cx.entity();
        let reader = form.clone();

        let commit_view = view.clone();
        let element_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let text = reader.text.read(cx).value().to_string();
            let group = reader.group.read(cx).value().to_string();
            let hint = reader.file_hint.clone();
            let original = reader.original_text.clone();
            let multi = reader.multi_files.clone();
            commit_view.update(cx, |view, cx| {
                view.commit_import_dialog(text, group, hint, Some(original), multi, window, cx)
            })
        });
        let commit_key = commit.clone();
        let cancel_view = view.clone();
        let close_view = view.clone();
        let ok_view = view.clone();
        let drop_view = view.clone();
        focus_shell_before_overlay(window, cx);
        // Snapshot before the builder runs: reading `AppView` inside the
        // dialog-builder closure re-enters the entity mid-update and panics.
        let in_flight = self.import_parse_in_flight;
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let cancel_view = cancel_view.clone();
            let close_view = close_view.clone();
            let ok_view = ok_view.clone();
            let drop_view = drop_view.clone();
            let footer = DialogFooter::new()
                .child(
                    Button::new("import-dialog-cancel")
                        .small()
                        .label("Cancel")
                        .on_click(move |_, window, cx| {
                            cancel_view.update(cx, |view, cx| {
                                view.import_form = None;
                                view.cancel_import_parse(cx);
                            });
                            window.close_all_dialogs(cx);
                            reclaim_shell_focus(window, cx);
                        }),
                )
                .child(
                    Button::new("import-dialog-ok")
                        .small()
                        // K17/A7-10: the named commit is a primary action —
                        // accent-filled like the other dialog footers.
                        .primary()
                        .label(if in_flight {
                            "Importing…"
                        } else {
                            footer_commit_label("import")
                        })
                        .disabled(in_flight)
                        .on_click(move |_, window, cx| {
                            if commit_click(window, cx) {
                                drop_view.update(cx, |view, _| view.import_form = None);
                                window.close_all_dialogs(cx);
                                reclaim_shell_focus(window, cx);
                            }
                        }),
                );
            dialog
                .title("Import proxy configurations")
                .close_button(false)
                .child(form.clone().element(element_view.clone(), cx))
                .button_props(DialogButtonProps::default().show_cancel(true).on_ok(
                    move |_, window, cx| {
                        let committed = commit_key(window, cx);
                        if committed {
                            ok_view.update(cx, |view, _| view.import_form = None);
                        }
                        committed
                    },
                ))
                .on_close(move |_, window, cx| {
                    close_view.update(cx, |view, cx| {
                        view.import_form = None;
                        view.cancel_import_parse(cx);
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(footer)
        });
    }

    pub(crate) fn open_import_from_clipboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // K5: the clipboard paste imports directly into the current group;
        // ambiguous grammar keeps its dialog inside import_text_direct.
        self.import_from_clipboard_direct(window, cx);
    }

    pub(crate) fn open_bare_url_chooser(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let proxy_view = view.clone();
        let proxy_url = url.clone();
        let sub_view = view.clone();
        let sub_url = url.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let proxy_view = proxy_view.clone();
            let proxy_url = proxy_url.clone();
            let sub_view = sub_view.clone();
            let sub_url = sub_url.clone();
            dialog
                .title("Add HTTP link")
                .child(
                    div()
                        .id("bare-url-chooser")
                        .test_support()
                        .child(format!(
                            "\"{url}\" can be a subscription feed or a proxy — choose how to add it:"
                        ))
                        .child(
                            {
                                let view = proxy_view.clone();
                                let url = proxy_url.clone();
                                Button::new("bare-url-import-proxy")
                                    .label("Import as proxy here")
                                    .on_click(move |_, window, cx| {
                                        window.close_all_dialogs(cx);
                                        reclaim_shell_focus(window, cx);
                                        view.update(cx, |view, cx| {
                                            view.open_import_dialog(
                                                &url,
                                                DEFAULT_GROUP,
                                                None,
                                                window,
                                                cx,
                                            );
                                        });
                                    })
                            },
                        )
                        .child(
                            {
                                let view = sub_view.clone();
                                let url = sub_url.clone();
                                Button::new("bare-url-add-subscription")
                                    .label("Add as subscription")
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |view, cx| {
                                            view.commit_bare_url_subscription(
                                                url.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    })
                            },
                        ),
                )

                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "bare-url-close",
                    "bare-url-cancel",
                    footer_commit_label("bare-url"),
                    Rc::new(|_, _| true),
                ))
        });
    }

    pub(crate) fn begin_refresh_batch(&mut self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        if self.refresh_batch_members.is_empty() {
            self.refresh_generation += 1;
        }
        for id in ids {
            if self.refresh_batch_members.insert(id.clone()) {
                self.refresh_batch_total += 1;
            }
        }
    }

    pub(crate) fn reopen_subscriptions_if_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !window.has_active_dialog(cx) && !window.has_active_sheet(cx) {
            self.subs_form = None;
            return;
        }
        let Some(form) = &self.subs_form else {
            return;
        };
        let label = form.label.read(cx).value().to_string();
        let url = form.url.read(cx).value().to_string();
        let group = form.group.read(cx).value().to_string();
        let auto = *form.auto_refresh.borrow();
        let interval = *form.interval_secs.borrow();
        window.close_all_dialogs(cx);
        window.close_sheet(cx);
        self.open_subscriptions_dialog_restored(&label, &url, &group, auto, interval, window, cx);
        reclaim_shell_focus(window, cx);
    }

    pub(crate) fn open_preset_edit_dialog(
        &mut self,
        preset: store::RoutePreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rules_text = preset
            .rules()
            .map(|rules| serde_json::Value::Array(rules).to_string())
            .unwrap_or_default();
        let title = format!("Edit preset {}", preset.name);
        if preset.locked {
            // T-10: a locked preset renders a readonly editor — view-only
            // rows, no inputs, no save/copy-edit path.
            let id = preset.id.clone();
            let name = preset.name.clone();
            let json_text = rules_text;
            let advanced_view = cx.entity();
            open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
                dialog
                    .title(title.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id(format!("preset-locked-hint-{id}"))
                                    .test_support()
                                    .text_sm()
                                    .child("Locked — import a copy to modify"),
                            )
                            .child(
                                div()
                                    .id(format!("preset-locked-name-{id}"))
                                    .test_support()
                                    .text_sm()
                                    .child(format!("Name: {name}")),
                            )
                            .child(
                                div()
                                    .id(format!("preset-locked-json-{id}"))
                                    .test_support()
                                    .text_sm()
                                    .child(json_text.clone()),
                            )
                            .child(preset_advanced_section(
                                advanced_view.clone(),
                                &preset,
                                None,
                                &[],
                            )),
                    )
                    .button_props(DialogButtonProps::default())
                    .footer(dialog_footer(
                        "preset-locked-close",
                        "preset-locked-close",
                        "Close",
                        Rc::new(|_, _| true),
                    ))
            });
            return;
        }
        let exits: Vec<String> = self
            .store
            .preset_attachments(&preset.id)
            .unwrap_or_default()
            .iter()
            .map(|row| crate::engine::outbound_tag(&row.id))
            .collect();
        let form = PresetForm::restored(window, cx, &preset.name, &rules_text);
        let view = cx.entity();
        let reader = form.clone();
        let id = preset.id.clone();
        let builder_view = view.clone();
        let builder_preset = preset.clone();
        let builder_form = form.clone();
        let builder_exits = exits.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = reader.name.read(cx).value().to_string();
            let rules = reader.rules.read(cx).value().to_string();
            let boxes = SimpleRuleBoxes::from_form(&reader, cx);
            view.update(cx, |view, cx| {
                view.commit_preset_edit_with_boxes(&id, name, rules, boxes, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(title.clone())
                .child(form.clone().element(cx))
                .child(preset_advanced_section(
                    builder_view.clone(),
                    &builder_preset,
                    Some(builder_form.clone()),
                    builder_exits.as_slice(),
                ))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "preset-edit-ok",
                    "preset-edit-cancel",
                    footer_commit_label("preset-edit"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_preset_detach_delete_confirm(
        &mut self,
        id: &str,
        name: &str,
        attached: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let preset_id = id.to_string();
        let description = preset_delete_confirm_description(name, attached);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let preset_id = preset_id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Delete")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Delete routing preset")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_preset_detach_delete(&preset_id, window, cx)
                    });
                    true
                })
        });
    }

    pub(crate) fn open_rule_set_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Rule set name"));
        let url_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://host.example/set.srs"));
        let view = cx.entity();
        let name_reader = name_state.clone();
        let url_reader = url_state.clone();
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = name_reader.read(cx).value().to_string();
            let url = url_reader.read(cx).value().to_string();
            if name.trim().is_empty() && url.trim().is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.begin_rule_set_download(name, url, window, cx)
            })
        });
        let commit_key = commit.clone();
        let commit_click = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit_click.clone();
            dialog
                .title("Add rule set by URL")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(dialog_field(
                            "field-rule-set-name",
                            "Name",
                            name_state.clone(),
                        ))
                        .child(dialog_field(
                            "field-rule-set-url",
                            "Rule set URL",
                            url_state.clone(),
                        ))
                        .child(Label::new(
                            "The download runs only after you confirm it below — each download is one explicit confirmation",
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "rule-set-add-dialog-ok",
                    "rule-set-add-dialog-cancel",
                    footer_commit_label("rule-set-add-dialog"),
                    commit_click,
                ))
        });
    }

    /// The Add-by-URL commit: validate the name, run the same policy gate as
    /// subscriptions (internal hosts block, plaintext http warn at the
    /// confirm), allocate a collision-free staged name, then request the
    /// explicit confirm. Nothing is fetched here.
    pub(crate) fn begin_rule_set_download(
        &mut self,
        name: String,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = match crate::store::Store::validated_rule_set_name(&name) {
            Ok(name) => name,
            Err(error) => {
                self.report_save_error(error, window, cx);
                return false;
            }
        };
        match store::subscription_fetch_policy(url.trim()) {
            store::SubscriptionFetchPolicy::Block { reason } => {
                window.push_notification(
                    Notification::error(format!("Invalid rule set URL: {reason}")),
                    cx,
                );
                return false;
            }
            store::SubscriptionFetchPolicy::Warn { .. } | store::SubscriptionFetchPolicy::Allow => {
            }
        }
        let dir = crate::store::rule_sets_dir();
        let taken: HashSet<String> = match self.store.list_rule_sets() {
            Ok(rows) => rows.into_iter().map(|row| row.name).collect(),
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load rule sets: {error}")),
                    cx,
                );
                return false;
            }
        };
        let mut candidate = name.clone();
        let mut suffix = 1;
        loop {
            let file_name = crate::store::rule_set_file_name(&candidate);
            if !taken.contains(&candidate) && !dir.join(&file_name).exists() {
                let grant = RuleSetDownloadGrant {
                    name: candidate,
                    file_name,
                    url: url.trim().to_string(),
                    dir,
                };
                self.open_rule_set_download_confirm(grant, window, cx);
                return true;
            }
            suffix += 1;
            if suffix > 100 {
                break;
            }
            candidate = format!("{name}-{suffix}");
        }
        window.push_notification(
            Notification::error(format!(
                "no free staging name for \"{name}\" — remove stale rule sets first"
            )),
            cx,
        );
        false
    }

    /// The exact one-confirmation-per-download callout. Blocked URLs never get
    /// here; http warns through the same policy verdicts subscriptions use.
    pub(crate) fn open_rule_set_download_confirm(
        &mut self,
        grant: RuleSetDownloadGrant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match store::subscription_fetch_policy(grant.url.trim()) {
            store::SubscriptionFetchPolicy::Block { reason } => {
                window.push_notification(
                    Notification::error(format!("Invalid rule set URL: {reason}")),
                    cx,
                );
                return;
            }
            store::SubscriptionFetchPolicy::Warn { reason } => {
                window.push_notification(Notification::warning(reason.to_string()), cx);
            }
            store::SubscriptionFetchPolicy::Allow => {}
        }
        let view = cx.entity();
        let warn = match store::subscription_fetch_policy(grant.url.trim()) {
            store::SubscriptionFetchPolicy::Warn { reason } => format!("\n{reason}"),
            _ => String::new(),
        };
        let overwrites = grant.dir.join(&grant.file_name).exists();
        let description = format!(
            "Download this rule set? {}\n{}\
             Staged file: {}\\{}",
            grant.url,
            warn,
            grant.dir.display(),
            grant.file_name,
        );
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let grant = grant.clone();
            let view = view.clone();
            let description = description.clone();
            let mut props = DialogButtonProps::default()
                .show_cancel(true)
                .ok_text("Download");
            if overwrites {
                props = props.ok_variant(ButtonVariant::Danger);
            }
            alert
                .confirm()
                .button_props(props)
                .title("Download rule set?")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.start_rule_set_download(grant.clone(), window, cx);
                    });
                    true
                })
        });
    }

    pub(crate) fn open_presets_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_presets_dialog_restored("", "", &SimpleRuleBoxes::default(), window, cx);
    }

    pub(crate) fn open_presets_dialog_restored(
        &mut self,
        name: &str,
        rules: &str,
        boxes: &SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = PresetForm::restored_with_boxes(window, cx, name, rules, boxes);
        self.preset_form = Some(form.clone());
        let view = cx.entity();
        let reader = form.clone();
        let presets = match self.store.list_presets() {
            Ok(presets) => presets,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load routing presets: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let rows = match self.store.list() {
            Ok(rows) => rows,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load library: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let mut attached: HashMap<String, Vec<String>> = HashMap::new();
        for row in &rows {
            if let Some(preset_id) = row.preset_id.as_deref() {
                attached
                    .entry(preset_id.to_string())
                    .or_default()
                    .push(format!("\"{}\"", row.name));
            }
        }
        let rule_sets = match self.store.list_rule_sets() {
            Ok(rule_sets) => rule_sets,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load rule sets: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let mut rule_set_used: HashMap<String, Vec<String>> = HashMap::new();
        for preset in &presets {
            if let Ok(names) = self.store.rule_sets_in_use(&preset.id) {
                for name in names {
                    rule_set_used
                        .entry(name)
                        .or_default()
                        .push(preset.name.clone());
                }
            }
        }
        let rule_set_view = view.clone();
        let rule_set_add_view = view.clone();
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = reader.name.read(cx).value().to_string();
            let rules = reader.rules.read(cx).value().to_string();
            let boxes = SimpleRuleBoxes::from_form(&reader, cx);
            let remote_url = reader.remote_url.read(cx).value().trim().to_string();
            if name.trim().is_empty()
                && rules.trim().is_empty()
                && boxes.is_empty()
                && remote_url.is_empty()
            {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.commit_preset_create_with_boxes(name, rules, boxes, window, cx)
            })
        });
        let commit_key = commit.clone();
        let close_view = view.clone();
        focus_shell_before_overlay(window, cx);
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let view_for_rows = view.clone();
            let close_view = close_view.clone();
            let mut body = div().flex().flex_col().gap_2();
            // T-04: the preset inventory left Settings — it lives here, in the
            // manager the Routing menu and the inspector pane link to.
            body = body.child(
                div()
                    .id("presets-summary")
                    .test_support()
                    .text_sm()
                    .child(preset_summary_text(Some(presets.len()))),
            );
            if presets.is_empty() {
                body = body.child(
                    "No routing presets yet — paste a throne://route link in Import, or create one below",
                );
            }
            for preset in &presets {
                body = body.child(preset_row(
                    view_for_rows.clone(),
                    preset.clone(),
                    attached.get(&preset.id).cloned().unwrap_or_default(),
                ));
            }

            // T-12: the Rule sets sub-list — explicit confirm-per-download
            // staging lives here, never anywhere else.
            body = body.child(
                div()
                    .id("rule-sets-section-title")
                    .test_support()
                    .text_sm()
                    .child("Rule sets"),
            );
            if rule_sets.is_empty() {
                body = body.child(Label::new(
                    "No rule sets staged — Add by URL downloads an .srs file into data\\rule-sets after you confirm each download",
                ));
            }
            for rule_set in &rule_sets {
                body = body.child(rule_set_row(
                    rule_set_view.clone(),
                    rule_set.clone(),
                    rule_set_used
                        .get(&rule_set.name)
                        .cloned()
                        .unwrap_or_default(),
                ));
            }
            {
                let value = rule_set_add_view.clone();
                body = body.child(
                    div().flex().flex_row().items_center().gap_2().child(
                        Button::new("rule-set-add")
                            .label("Add by URL")
                            .tooltip("Download an .srs rule set from an https URL (you confirm each download)")
                            .on_click(move |_, window, cx| {
                                value.update(cx, |view, cx| {
                                    view.open_rule_set_add_dialog(window, cx);
                                });
                            }),
                    ),
                );
            }

            body = body.child(form.clone().element(cx));
            dialog
                .title("Routing presets")
                .close_button(false)
                .child(body)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .on_close(move |_, window, cx| {
                    close_view.update(cx, |view, _| {
                        view.preset_form = None;
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(dialog_footer(
                    "presets-dialog-ok",
                    "presets-dialog-cancel",

                    footer_commit_label("presets"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn reopen_presets_if_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.has_active_dialog(cx) {
            self.preset_form = None;
            return;
        }
        let Some(form) = &self.preset_form else {
            return;
        };
        let name = form.name.read(cx).value().to_string();
        let rules = form.rules.read(cx).value().to_string();
        let boxes = SimpleRuleBoxes::from_form(form, cx);
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        self.open_presets_dialog_restored(&name, &rules, &boxes, window, cx);
    }

    pub(crate) fn open_preset_attach_dialog(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let config_id = config.id.clone();
        let config_name = config.name.clone();
        let current = config.preset_id.clone();
        let presets = match self.store.list_presets() {
            Ok(presets) => presets,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load routing presets: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        // Read-only monitor, computed once per open: rows of the selected
        // row's group, ranked by their latest health samples. No probing, no
        // sampling, no persistence — purely the ordering view of what the
        // session already knows.
        let monitor = {
            let group_rows: Vec<ProxyConfig> = self
                .store
                .list()
                .unwrap_or_default()
                .into_iter()
                .filter(|row| row.group == config.group)
                .collect();
            crate::health::best_rows_monitor_lines(&config.group, &group_rows, &self.health_store)
        };
        let commit: DialogCommit = Rc::new(|_, _| true);
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let mut body = div().flex().flex_col().gap_2();
            if let Some((monitor_title, monitor_lines)) = &monitor {
                let mut section = div()
                    .id("preset-attach-monitor")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("preset-attach-monitor-title")
                            .test_support()
                            .text_sm()
                            .child(monitor_title.clone()),
                    );
                for (index, line) in monitor_lines.iter().enumerate() {
                    section = section.child(
                        div()
                            .id(format!("preset-attach-monitor-{index}"))
                            .test_support()
                            .text_sm()
                            .child(line.clone()),
                    );
                }
                body = body.child(section);
            }
            if !presets.is_empty() {
                body = body.child(
                    div()
                        .id("preset-attach-exits")
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "Route exits for \"{config_name}\": {}",
                            crate::preset::preset_exit_suggestions(&[crate::engine::outbound_tag(
                                &config_id
                            )])
                            .join(" · ")
                        )),
                );
            }
            if presets.is_empty() {
                body = body.child(
                    "No routing presets yet — import one first (throne://route link or preset JSON)",
                );
            }
            for preset in &presets {
                let attach_view = view.clone();
                let attach_config = config_id.clone();
                let attach_preset = preset.id.clone();
                let mut line = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Label::new(preset.name.clone()));
                if current.as_deref() == Some(preset.id.as_str()) {
                    line = line.child("attached");
                } else {
                    line = line.child(
                        Button::new(format!("preset-attach-{}", preset.id))
                            .label("Attach")
                            .on_click(move |_, window, cx| {
                                let config = attach_config.clone();
                                let preset = attach_preset.clone();
                                attach_view.update(cx, |view, cx| {
                                    view.commit_preset_attach(&config, Some(preset), window, cx);
                                });
                            }),
                    );
                }
                body = body.child(line);
            }
            if current.is_some() {
                let detach_view = view.clone();
                let detach_config = config_id.clone();
                body = body.child(Button::new("preset-detach").label("Detach").on_click(
                    move |_, window, cx| {
                        let config = detach_config.clone();
                        detach_view.update(cx, |view, cx| {
                            view.commit_preset_attach(&config, None, window, cx);
                        });
                    },
                ));
            }
            dialog
                .title(format!("Routing preset for \"{config_name}\""))
                .child(body)
                .button_props(
                    DialogButtonProps::default().on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "preset-attach-ok",
                    "preset-attach-cancel",
                    footer_commit_label("preset-attach"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_sub_full_url_confirm(
        &mut self,
        label: &str,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let full = url.to_string();
        let description = format!(
            "The full URL for \"{label}\" carries its token in plaintext — anyone holding it can \
             use the subscription. Prefer the redacted copy. Copy the full URL anyway?"
        );
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let full = full.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Copy URL")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Copy full subscription URL?")
                .description(description)
                .on_ok(move |_, window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(full.clone()));
                    window.push_notification(
                        Notification::warning("Copied FULL subscription URL (carries the token)"),
                        cx,
                    );
                    true
                })
        });
    }

    pub(crate) fn open_subscriptions_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_subscriptions_dialog_restored(
            "",
            "",
            DEFAULT_GROUP,
            true,
            store::DEFAULT_AUTO_REFRESH_SECS,
            window,
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_subscriptions_dialog_restored(
        &mut self,
        label: &str,
        url: &str,
        group: &str,
        auto: bool,
        interval_secs: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = SubForm::restored(window, cx, label, url, group, auto, interval_secs);
        self.subs_form = Some(form.clone());
        let view = cx.entity();
        let reader = form.clone();
        let subs = match self.store.list_subscriptions() {
            Ok(subs) => subs,

            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load subscriptions: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let rows = match self.store.list() {
            Ok(rows) => rows,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load library: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let mut counts: HashMap<String, usize> = HashMap::new();
        for row in &rows {
            if let Some(owner) = row.subscription_id.as_deref() {
                *counts.entry(owner.to_string()).or_insert(0) += 1;
            }
        }

        let expanded = self.expanded_subs.clone();
        let member_rows = rows.clone();

        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let label = reader.label.read(cx).value().to_string();
            let url = reader.url.read(cx).value().to_string();
            let group = reader.group.read(cx).value().to_string();
            let auto = *reader.auto_refresh.borrow();
            let interval = *reader.interval_secs.borrow();
            if label.trim().is_empty() && url.trim().is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.commit_subscription_add(label, url, group, auto, interval, window, cx)
            })
        });

        let auto_all = self.store.subs_auto_refresh_enabled();
        let now = now_unix();
        let due_count = subs
            .iter()
            .filter(|sub| due_for_refresh_for(sub, now))
            .count();

        let batch_progress = self.refresh_batch_progress();
        let refreshing_rows = self.inflight_refreshes.clone();
        focus_shell_before_overlay(window, cx);
        let kill_view = view.clone();
        let due_view = view.clone();

        let close_view = view.clone();

        window.open_sheet(cx, move |sheet: Sheet, _, cx| {
            let commit_click = commit.clone();
            let view_for_rows = view.clone();
            let close_subs = close_view.clone();
            let kill_buttons = kill_view.clone();
            let due_buttons = due_view.clone();
            let batch_progress = batch_progress;
            let refreshing_rows = refreshing_rows.clone();
            let mut body = div().flex().flex_col().gap_2();

            let kill_now = auto_all;
            body = body.child(
                div()
                    .id("subs-auto-header")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("subs-auto-all")
                            .checked(kill_now)
                            .label("Auto-refresh all")

                            .tooltip(
                                "Automatic background refresh of all subscriptions. Manual Refresh always works.",
                            )
                            .accessibility_label(
                                "Auto-refresh all subscriptions automatically. Manual Refresh always works.",
                            )
                            .on_change(move |next: &bool, window, cx| {
                                let next = *next;
                                kill_buttons.update(cx, |view, cx| {
                                    view.commit_subs_global_auto(next, window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("subs-refresh-all-due")
                            .label(format!("Refresh all due ({due_count})"))
                            .on_click(move |_, window, cx| {
                                due_buttons.update(cx, |view, cx| {
                                    view.refresh_all_due(window, cx);
                                });
                            }),
                    ),
            );
            if subs.is_empty() {
                body = body.child("No subscriptions yet — paste a subscription URL below");
            }

            if let Some((done, total)) = batch_progress {
                let percent =
                    (done as f32 / total.max(1) as f32 * 100.0).clamp(0.0, 100.0);
                body = body.child(
                    div()
                        .id("subs-refresh-progress")
                        .test_support()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Progress::new("subs-refresh-progress-bar")
                                .small()
                                .value(percent)
                                .accessibility_label(format!(
                                    "Refreshing subscriptions: {done} of {total}"
                                )),
                        )
                        .child(format!("Refreshing {done} of {total} subscriptions")),
                );
            }
            for sub in &subs {
                body = body.child(subscription_row(
                    view_for_rows.clone(),
                    sub.clone(),
                    counts.get(&sub.id).copied().unwrap_or(0),
                    expanded.contains(&sub.id),
                    subscription_member_names(&member_rows, &sub.id),
                    refreshing_rows.contains(&sub.id),
                ));
            }
            body = body.child(form.clone().element(view_for_rows.clone(), cx));
            sheet
                .title("Subscriptions")
                .child(body)
                .size(px(520.))
                .on_close(move |_, window, cx| {
                    close_subs.update(cx, |view, _| {
                        view.subs_form = None;
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(sheet_footer(
                    "subscriptions-dialog-ok",
                    Some("subscriptions-dialog-cancel"),

                    footer_commit_label("subscriptions"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_subscription_edit_dialog(
        &mut self,
        sub: ProxySubscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Label")
                .default_value(sub.label.clone())
        });
        let url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://…")
                .default_value(sub.url.clone())
        });
        let group_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Group")
                .default_value(sub.group.clone())
        });
        let view = cx.entity();
        let id = sub.id.clone();
        let title = format!("Edit {}", sub.label);
        let read_label = label_state.clone();
        let read_url = url_state.clone();
        let read_group = group_state.clone();

        let interval_cell: Rc<RefCell<i64>> = Rc::new(RefCell::new(sub.interval_secs));
        let read_interval = interval_cell.clone();
        let edit_interval = interval_cell.clone();

        let repaint_view = view.clone();

        let commit: DialogCommit = Rc::new(move |window, cx| {
            let label = read_label.read(cx).value().to_string();
            let url = read_url.read(cx).value().to_string();
            let group = read_group.read(cx).value().to_string();
            let interval = *read_interval.borrow();
            view.update(cx, |view, cx| {
                view.commit_subscription_edit(&id, label, url, group, interval, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();

            let repaint_view = repaint_view.clone();

            let url_now = url_state.read(cx).value().to_string();
            let edit_warning = http_subscription_warning(&url_now).map(|warning| {
                div()
                    .id("sub-edit-http-warning")
                    .test_support()
                    .child(warning)
            });

            let mut form = v_form()
                .small()
                .child(dialog_field(
                    "field-sub-edit-label",
                    "Label",
                    label_state.clone(),
                ))
                .child(dialog_field(
                    "field-sub-edit-url",
                    "Subscription URL",
                    url_state.clone(),
                ));
            if let Some(warning) = edit_warning {
                form = form.child(field().label_indent(false).child(warning));
            }
            form = form
                .child(dialog_field(
                    "field-sub-edit-group",
                    "Group",
                    group_state.clone(),
                ))
                .child(field().label_indent(false).child(
                    div().id("field-sub-edit-interval").test_support().child({
                        let repaint: DialogRepaint = Rc::new(move |_, cx| {
                            repaint_view.update(cx, |_, _| {});
                        });
                        interval_picker_row("sub-interval-edit", edit_interval.clone(), repaint)
                    }),
                ));
            dialog
                .title(title.clone())
                .child(form)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "subscription-edit-dialog-ok",
                    "subscription-edit-dialog-cancel",
                    footer_commit_label("subscription-edit"),
                    commit_click,
                ))
        });
    }

    pub(crate) fn open_subscription_remove_confirm(
        &mut self,
        sub: ProxySubscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = sub.id.clone();
        let label = sub.label.clone();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Remove")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Remove subscription")
                .description(format!(
                    "Remove \"{label}\"? Its configs stay in the library."
                ))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_subscription_remove(&id, window, cx);
                    });
                    true
                })
        });
    }

    pub(crate) fn open_update_all_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let flagged: HashSet<String> = self
            .store
            .group_skip_flags()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let (included, skipped) = update_all_targets(subs, &flagged);
        if included.is_empty() {
            window.push_notification(
                Notification::info(if skipped.is_empty() {
                    "No subscriptions to update".to_string()
                } else {
                    format!(
                        "No subscriptions to update — {} skipped (auto-refresh off for their group)",
                        skipped.len()
                    )
                }),
                cx,
            );
            return;
        }
        let view = cx.entity();
        let ids: Vec<String> = included.iter().map(|sub| sub.id.clone()).collect();
        let description = update_all_confirm_text(included.len(), skipped.len());
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let ids = ids.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Update all"),
                )
                .title("Update all subscriptions?")
                .description(description.clone())
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.begin_refresh_batch(&ids);

                        for id in &ids {
                            if let Ok(Some(sub)) = view.store.get_subscription(id) {
                                view.start_subscription_refresh(sub, true, cx);
                            }
                        }
                        view.reopen_subscriptions_if_open(window, cx);
                    });
                    true
                })
        });
    }
}

/// T-06: every app dialog opens through here, so Esc, a backdrop press and the
/// close button all end in `reclaim_shell_focus`. The kit fires `on_close`
/// after `Root::close_dialog` has already popped the layer, which is what makes
/// the end-of-path predicate correct.
pub(crate) fn open_app_dialog(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
) {
    focus_shell_before_overlay(window, cx);
    // K19/D3: the kit's header Close renders as a blank 24x24 icon (no asset
    // source registers kit glyphs here), so it was an invisible interactive
    // target. Every app dialog carries a named commit footer and Esc, so the
    // redundant header X is disabled at both open routes.
    window.open_dialog(cx, move |dialog, window, cx| {
        build(dialog, window, cx)
            .close_button(false)
            .on_close(|_, window, cx| reclaim_shell_focus(window, cx))
    });
}

pub(crate) fn open_app_alert_dialog(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static,
) {
    focus_shell_before_overlay(window, cx);
    window.open_alert_dialog(cx, move |alert, window, cx| {
        build(alert, window, cx).on_close(|_, window, cx| reclaim_shell_focus(window, cx))
    });
}

pub(crate) fn open_app_sheet(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(Sheet, &mut Window, &mut App) -> Sheet + 'static,
) {
    focus_shell_before_overlay(window, cx);
    window.open_sheet(cx, move |sheet, window, cx| {
        build(sheet, window, cx).on_close(|_, window, cx| reclaim_shell_focus(window, cx))
    });
}
