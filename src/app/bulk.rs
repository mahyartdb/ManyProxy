use crate::*;

impl AppView {
    pub(crate) fn render_bulk_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.libraries.read(cx).delegate().bulk_label();
        if label.is_empty() {
            return div().id("bulk-bar").test_support().into_any_element();
        }

        Self::motion_enter(
            div()
                .id("bulk-bar")
                .test_support()
                .flex()
                .flex_row()

                .flex_wrap()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .bg(cx.theme().group_box)
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("bulk-label")
                        .test_support()
                        .tooltip(|_, cx| {
                            cx.new(|_| {
                                Tooltip::new(
                                    "Blue wash = selected (F5). Blue rim + number = checked (bulk).",
                                )
                            })
                            .into()
                        })
                        .child(label),
                )

                .child(
                    Button::new("bulk-select-visible")
                        .small()
                        .label("Check all visible")
                        .tooltip("Check every visible row")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.select_visible_checked(window, cx);
                        })),
                )
                .child(
                    Button::new("bulk-clear")
                        .small()
                        .label("Uncheck all")
                        .tooltip("Uncheck all (Esc)")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.clear_checked(window, cx);
                        })),
                )
                .child(Separator::vertical())
                .child(
                    Button::new("bulk-start")
                        .small()
                        .label("Start checked")
                        .tooltip("Start every checked endpoint")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.commit_bulk_start(window, cx);
                        })),
                )
                .child(
                    Button::new("bulk-stop")
                        .small()
                        .label("Stop checked")
                        .tooltip("Stop every checked endpoint")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.commit_bulk_stop(window, cx);
                        })),
                )
                .child(
                    Button::new("bulk-enable")
                        .small()
                        .label("Enable")
                        .tooltip("Enable every checked row")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.commit_bulk_enable(true, window, cx);
                        })),
                )
                .child(
                    Button::new("bulk-disable")
                        .small()
                        .label("Disable")
                        .tooltip("Disable every checked row")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.commit_bulk_enable(false, window, cx);
                        })),
                )
                .child(
                    Button::new("bulk-move")
                        .small()
                        .label("Move to group")
                        .tooltip("Move every checked row")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.open_bulk_move_dialog(window, cx);
                        })),
                )

                .child(Separator::vertical())
                .child(
                    Button::new("bulk-delete")
                        .danger()
                        .small()
                        .label("Delete")
                        .tooltip("Delete every checked row")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.open_bulk_delete_confirm(window, cx);
                        })),
                )
                .into_any_element(),
            "bulk-bar-enter",
            cx,
        )
        .into_any_element()
    }

    pub(crate) fn clear_facets(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().clear_facets();
            list_cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn clear_checked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().clear_checked();
            list_cx.notify();
        });

        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn clear_checked_unless_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if overlay_open(window, cx) {
            return false;
        }
        // T-03: in the narrow drawer the inspector is the top layer — Esc
        // closes it and hands focus back to the list, and the checked set
        // survives (one overlay layer per Esc).
        if self.close_inspector_drawer(window, cx) {
            return false;
        }
        self.clear_checked(window, cx);
        true
    }

    pub(crate) fn commit_bulk_enable(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<String> = self
            .libraries
            .read(cx)
            .delegate()
            .checked_rows()
            .iter()
            .map(|row| row.id.clone())
            .collect();
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let mut ok = 0usize;
        let mut failed = 0usize;
        for id in &ids {
            match self.store.set_enabled(id, enabled) {
                Ok(_) => ok += 1,
                Err(_) => failed += 1,
            }
        }
        self.reload(window, cx);
        let verb = if enabled { "Enabled" } else { "Disabled" };
        if failed == 0 {
            window.push_notification(
                Notification::success(format!("{verb} {ok} configuration(s)")),
                cx,
            );
        } else {
            window.push_notification(
                Notification::warning(format!("{verb} {ok}, {failed} failed")),
                cx,
            );
        }
    }

    pub(crate) fn commit_bulk_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.libraries.read(cx).delegate().checked_rows();
        if rows.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
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
            window.open_alert_dialog(cx, move |alert, _, _| {
                let view = view.clone();
                let ids = ids.clone();
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

    pub(crate) fn commit_bulk_start_ids(
        &mut self,
        ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        self.start_in_flight = true;
        if let Err(reason) = self.sync_presets_to_engine() {
            self.start_in_flight = false;
            window.push_notification(Notification::error(format!("Cannot start: {reason}")), cx);
            return;
        }
        let mut started = 0usize;
        let mut failed: Vec<String> = Vec::new();
        let mut skipped_dirty: Vec<String> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        let mut flag_failed: Vec<String> = Vec::new();
        for id in ids {
            if self.libraries.read(cx).delegate().endpoint_is_dirty(id, cx) {
                if let Ok(Some(config)) = self.store.get(id) {
                    skipped_dirty.push(config.name.clone());
                }
                continue;
            }
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
            match self.engine.start_one_confirmed(&config) {
                Ok(()) => {
                    started += 1;
                    if self.store.set_enabled(id, true).is_err() {
                        flag_failed.push(config.name.clone());
                    }
                }
                Err(error) => {
                    failed.push(format!(
                        "{}: {}",
                        config.name,
                        redact_secrets(&error.to_string())
                    ));
                }
            }
        }
        self.start_in_flight = false;
        self.reload(window, cx);
        if !skipped_dirty.is_empty() {
            window.push_notification(
                Notification::info(skipped_dirty_message(&skipped_dirty)),
                cx,
            );
        }
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
        if failed.is_empty() {
            window.push_notification(
                Notification::success(format!("Started {started} configuration(s)")),
                cx,
            );
        } else {
            window.push_notification(
                Notification::error(format!(
                    "Started {started}, {} failed: {}",
                    failed.len(),
                    failed.join("; ")
                )),
                cx,
            );
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

    pub(crate) fn commit_bulk_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .libraries
            .read(cx)
            .delegate()
            .checked_rows()
            .iter()
            .map(|row| row.id.clone())
            .collect();
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let mut stopped: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for id in &ids {
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
                Err(error) => failed.push(format!("{name}: stopped but flag not saved ({error})")),
            }
        }
        self.reload(window, cx);
        if failed.is_empty() {
            if stopped.is_empty() {
                window.push_notification(
                    Notification::info("Nothing to stop — no checked endpoints are running"),
                    cx,
                );
            } else {
                window.push_notification(
                    Notification::success(format!("Stopped {} configuration(s)", stopped.len())),
                    cx,
                );
            }
            return;
        }
        if stopped.is_empty() {
            window.push_notification(
                Notification::error(format!(
                    "Stop failed for {} configuration(s): {}",
                    failed.len(),
                    failed.join("; ")
                )),
                cx,
            );
        } else {
            window.push_notification(
                Notification::warning(format!(
                    "Stopped {}, {} failed: {}",
                    stopped.len(),
                    failed.len(),
                    failed.join("; ")
                )),
                cx,
            );
        }
    }

    pub(crate) fn open_bulk_delete_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.libraries.read(cx).delegate().checked_rows();
        if rows.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let view = cx.entity();
        let targets: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.id.clone(), row.name.clone()))
            .collect();

        if self.store.skip_delete_confirm() {
            view.update(cx, |view, cx| view.commit_bulk_delete(&targets, window, cx));
            return;
        }
        let names: Vec<String> = targets.iter().map(|(_, name)| name.clone()).collect();
        let count = targets.len();
        let description = bulk_delete_confirm_description(count, &names);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let targets = targets.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Delete")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(format!("Delete {count} configurations?"))
                .description(description)
                .on_ok(move |_, window, cx| {
                    let targets = targets.clone();
                    view.update(cx, |view, cx| view.commit_bulk_delete(&targets, window, cx));
                    true
                })
        });
    }

    pub(crate) fn commit_bulk_delete(
        &mut self,
        targets: &[(String, String)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut removed: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for (id, name) in targets {
            let (was_active, stop_error) = self.engine.stop_one_result(id);
            if was_active && stop_error.is_some() {
                failed.push(name.clone());
                continue;
            }
            match self.store.remove(id) {
                Ok(true) => removed.push(name.clone()),
                _ => failed.push(name.clone()),
            }
        }
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().clear_checked();
            list_cx.notify();
        });
        self.reload(window, cx);

        window.focus(&self.focus, cx);
        if targets.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let text = bulk_delete_result_message(&removed, &failed);
        if failed.is_empty() {
            window.push_notification(Notification::success(text), cx);
        } else if removed.is_empty() {
            window.push_notification(Notification::error(text), cx);
        } else {
            window.push_notification(Notification::warning(text), cx);
        }
    }

    pub(crate) fn open_bulk_move_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .libraries
            .read(cx)
            .delegate()
            .checked_rows()
            .iter()
            .map(|row| row.id.clone())
            .collect();
        if ids.is_empty() {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        }
        let group_state = cx.new(|cx| InputState::new(window, cx).placeholder("Group"));
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
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(format!("Move {count} configuration(s) to group"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Label::new("Group"))
                        .child(Input::new(&group_state)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "bulk-move-ok",
                    "bulk-move-cancel",
                    footer_commit_label("bulk-move"),
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
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let ids_for_commit = vec![config.id.clone()];
        let name = config.name.clone();
        let group_state = cx.new(|cx| InputState::new(window, cx).placeholder("Group"));
        let view = cx.entity();
        let reader = group_state.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let group = reader.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_bulk_move(&ids_for_commit, &group, window, cx)
            })
        });
        let commit_key = commit.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(format!("Move \"{name}\" to group"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Label::new("Group"))
                        .child(Input::new(&group_state)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "row-move-ok",
                    "row-move-cancel",
                    footer_commit_label("bulk-move"),
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

                self.sync_facet_pickers(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Moved {moved} configuration(s) to \"{}\"",
                        group.trim()
                    )),
                    cx,
                );
                true
            }
            Err(error) => {
                window.push_notification(Notification::error(format!("Cannot move: {error}")), cx);
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

        window.open_sheet(cx, move |sheet: Sheet, _, _| {
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
                                .label("Update current")
                                .tooltip(
                                    "Refresh the subscriptions feeding the focused group \
                                     (facet filter, else selected row) — Ctrl-R",
                                )
                                .on_click(move |_, window, cx| {
                                    current_view.update(cx, |view, cx| {
                                        view.update_current_group(window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("groups-update-all")
                                .label("Update all")
                                .tooltip(
                                    "Refresh every subscription outside auto-skipped groups (asks first)",
                                )
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
                form = form.child(
                    field()
                        .label_indent(false)
                        .child("No groups yet — groups appear when rows use them"),
                );
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
                        .label("Rename")
                        .on_click(move |_, window, cx| {
                            let group = rename_group.clone();
                            rename_view.update(cx, |view, cx| {
                                view.open_group_rename_dialog(&group, window, cx);
                            });
                        }),
                );
                buttons = buttons.child(
                    Button::new(format!("group-edit-{slug}"))
                        .label("Edit")
                        .tooltip("Rename + auto-refresh skip for this group")
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
                            .label("Update")
                            .tooltip(
                                "Refresh this group's subscription(s) now (manual — runs even when auto-skipped)"
                            )
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
                        .label("Delete")
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
                    .label("Auto-clear unavailable")
                    .tooltip(
                        "Opt-in per group: after this group's URL test ends, its unavailable \
                         rows are removed automatically. Running rows and the row being edited \
                         are never removed; enabling asks its own confirmation.",
                    )
                    .accessibility_label(format!(
                        "Auto-clear unavailable rows in group \"{}\" after group tests",
                        row.name
                    ))
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
                .title("Groups")
                .child(form)
                .size(px(520.))
                .footer(sheet_footer(
                    "groups-dialog-ok",
                    "groups-dialog-cancel",

                    footer_commit_label("groups"),
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
        let name_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("New group name")
                .default_value(old_name.clone())
        });
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
        window.open_dialog(cx, move |dialog: Dialog, _, _cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(format!("Rename group \"{old_name}\""))
                .child(v_form().small().child(dialog_field(
                    "field-group-rename-name",
                    "New group name",
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
                    footer_commit_label("group-rename"),
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
        let name_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Group name")
                .default_value(old_name.clone())
        });
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
        window.open_dialog(cx, move |dialog: Dialog, _, _cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(format!("Edit group \"{old_name}\""))
                .child(
                    v_form()
                        .small()
                        .child(dialog_field(
                            "field-group-edit-name",
                            "Group name",
                            name_state.clone(),
                        ))
                        .child(field().label_indent(false).child(
                            div().id("field-group-edit-skip").test_support().child(
                                Switch::new("group-edit-skip")
                                    .checked(skip_checked)
                                    .label("Skip automatic refresh")
                                    .tooltip(
                                        "Skip this group's subscriptions on ticks and Update-all. \
                                         Manual Update still works.",
                                    )
                                    .accessibility_label(
                                        "Skip automatic refresh of this group's subscriptions. \
                                         Manual Update still works.",
                                    )
                                    .on_change({
                                        let skip_for_change = skip_for_change.clone();
                                        move |next: &bool, _, _| {
                                            *skip_for_change.borrow_mut() = *next;
                                        }
                                    }),
                            ),
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "group-edit-ok",
                    "group-edit-cancel",

                    footer_commit_label("group-edit"),
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
                        Notification::error(format!("Cannot rename group: {error}")),
                        cx,
                    );
                    return false;
                }
            }
        }
        if let Err(error) = self.store.set_group_skip_refresh(&renamed, skip) {
            window.push_notification(
                Notification::error(format!("Cannot change auto-refresh skip: {error}")),
                cx,
            );
            return false;
        }
        self.reload(window, cx);

        self.libraries.update(cx, |state, list_cx| {
            let delegate = state.delegate_mut();
            if delegate.facet_group.as_deref() == Some(old) {
                delegate.set_facet_group(Some(renamed.clone()));
            }
            list_cx.notify();
        });
        self.sync_facet_pickers(window, cx);
        window.push_notification(
            Notification::success(format!("Updated group \"{renamed}\"")),
            cx,
        );
        window.close_all_dialogs(cx);
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
                    Notification::warning(format!("Group \"{old}\" holds no rows")),
                    cx,
                );
                true
            }
            Ok(outcome) => {
                let renamed = new.trim().to_string();
                self.reload(window, cx);

                self.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    if delegate.facet_group.as_deref() == Some(old) {
                        delegate.set_facet_group(Some(renamed.clone()));
                    }
                    list_cx.notify();
                });
                self.sync_facet_pickers(window, cx);
                let mut moved = Vec::new();
                if outcome.rows > 0 {
                    moved.push(format!("{} row(s)", outcome.rows));
                }
                if outcome.subscriptions > 0 {
                    moved.push(format!("{} subscription(s)", outcome.subscriptions));
                }
                window.push_notification(
                    Notification::success(format!(
                        "Renamed \"{old}\" to \"{renamed}\" ({})",
                        moved.join(", ")
                    )),
                    cx,
                );
                window.close_all_dialogs(cx);
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot rename group: {error}")),
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
                self.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    if delegate.facet_group.as_deref() == Some(name) {
                        delegate.set_facet_group(None);
                    }
                    list_cx.notify();
                });
                self.sync_facet_pickers(window, cx);
                window.push_notification(
                    Notification::success(format!("Deleted empty group \"{name}\"")),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot delete group: {error}")),
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
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let group = group.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Delete")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Delete group")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_delete_group(&group, window, cx);

                        window.close_dialog(cx);
                        window.close_dialog(cx);
                        view.open_groups_dialog(window, cx);
                    });
                    false
                })
        });
    }
}
