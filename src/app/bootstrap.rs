use crate::*;

impl AppView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        match Store::open_file(&Store::default_path()) {
            Ok(store) => Self::with_store(window, cx, store),
            Err(error) => Self::fell_back_to_memory(window, cx, error, &Store::default_path()),
        }
    }

    /// T-08: the primary case, not an edge: a read-only data folder usually
    /// kills the file store at OPEN, because SQLite must create `-wal`/`-shm`.
    /// The session then runs in memory and the probe can never see the folder,
    /// so the open failure itself is the banner's cause. The one-time warning
    /// still names the reason; the banner stands for as long as the session
    /// cannot save.
    pub(crate) fn fell_back_to_memory(
        window: &mut Window,
        cx: &mut Context<Self>,
        error: StoreError,
        path: &Path,
    ) -> Self {
        let fallback = WriteFallback {
            folder: store_folder(path),
            reason: format!("cannot open {}: {error}", path.display()),
        };
        let mut view = Self::with_store_inner(
            window,
            cx,
            Store::open_in_memory().expect("in-memory store opens"),
            Some(fallback),
        );
        view.notify_after_root(
            Notification::warning(format!("Running without persistence: {error}")),
            window,
            cx,
        );
        view
    }

    /// T-08: a deliberate in-memory store (tests, any explicit no-persist
    /// path) passes no fallback and stays silent: only an open failure can
    /// claim the folder refused this session.
    pub(crate) fn with_store(window: &mut Window, cx: &mut Context<Self>, store: Store) -> Self {
        Self::with_store_inner(window, cx, store, None)
    }

    pub(crate) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notify_engine_crash(window, cx);
        // `has_active_dialog` / `has_active_sheet` / `close_sheet` all read the
        // Root layer, which the window only adopts after the boot closure
        // returns — ask only once a Root is actually installed.
        let root_ready = Self::root_installed(window);
        // T-19: an edit dialog that left no dialog active anywhere is gone —
        // stop protecting its row from auto-clear.
        if self.edit_form_open.is_some() && root_ready && !window.has_active_dialog(cx) {
            self.edit_form_open = None;
        }
        if let Err(error) = self.reload_inner_no_poll(cx) {
            // Reached during boot (restore_on_launch -> commit_restore_subset)
            // as well as from ordinary clicks: `notify_now_or_after_root`
            // keeps the post-Root timing and defers only before Root exists.
            self.notify_now_or_after_root(
                Notification::error(db_list_error_message(&error)),
                window,
                cx,
            );
        }

        if self.logs_open
            && self.log_tail
            && self.logs.push_seq() != self.logs_seen
            && root_ready
            && window.has_active_sheet(cx)
        {
            self.log_reopen_draft = self.log_input_draft(cx);
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs_open = false;
            self.open_logs_dialog(window, cx);
            return;
        }
        cx.notify();
    }

    pub(crate) fn create_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match crate::tray::TrayHandle::build() {
            Ok(tray) => {
                tray.set_running(self.engine.running_count());
                self.tray = Some(tray);
                cx.notify();
            }
            Err(error) => {
                self.notify_after_root(
                    Notification::warning(format!(
                        "Tray unavailable ({error}) — closing still confirms"
                    )),
                    window,
                    cx,
                );
            }
        }
    }

    pub(crate) fn restore_on_launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.store.restore_running() {
            return;
        }
        let snapshot = self.store.load_running_snapshot();
        if snapshot.is_empty() {
            return;
        }
        let rows = match self.store.list() {
            Ok(rows) => rows,
            Err(error) => {
                self.notify_after_root(
                    Notification::error(db_list_error_message(&error)),
                    window,
                    cx,
                );
                return;
            }
        };
        let (now, gated) = crate::tray::partition_restore(&rows, &snapshot);
        if !now.is_empty() {
            self.commit_restore_subset(&now, window, cx);
        }
        if !gated.is_empty() {
            self.open_restore_lan_confirm(gated, window, cx);
        }
    }

    pub(crate) fn commit_restore_subset(
        &mut self,
        rows: &[ProxyConfig],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(reason) = self.sync_presets_to_engine() {
            self.notify_after_root(
                Notification::error(format!("Cannot restore: {reason}")),
                window,
                cx,
            );
            return;
        }
        let mut started = 0usize;
        let mut failed: Vec<String> = Vec::new();
        for row in rows {
            match self.engine.start_one_confirmed(row) {
                Ok(()) => {
                    if self.store.set_enabled(&row.id, true).is_ok() {
                        started += 1;
                    } else {
                        failed.push(format!("\"{}\"", row.name));
                    }
                }
                Err(_) => failed.push(format!("\"{}\"", row.name)),
            }
        }
        self.reload(window, cx);
        if failed.is_empty() {
            self.notify_after_root(
                Notification::success(format!("Restored {started} endpoint(s)")),
                window,
                cx,
            );
        } else {
            self.notify_after_root(
                Notification::warning(start_all_partial_message(started, &failed)),
                window,
                cx,
            );
        }
    }

    pub(crate) fn open_restore_lan_confirm(
        &mut self,
        rows: Vec<ProxyConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let lan: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.name.clone(), row.bind_ip.clone()))
            .collect();
        let description = format!(
            "{}\n\nThese were running before; confirm to restart them.",
            lan_confirm_description(&lan)
        );
        self.open_alert_dialog_after_root(window, cx, move |alert, _, _| {
            let view = view.clone();
            let rows = rows.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Restart"),
                )
                .title(LAN_CONFIRM_TITLE)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_restore_subset(&rows, window, cx));
                    true
                })
        });
    }

    pub(crate) fn handle_launch_intake(
        &mut self,
        intake: OpenWithKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Boot calls this inside `cx.open_window`'s closure, before the window
        // adopts its `Root`; the intake dialog would panic in `Root::update`.
        // The intake decision is taken now, the dialog opens on the next frame.
        self.after_root(window, cx, move |view, window, cx| match intake {
            OpenWithKind::BareHttp(url) => view.open_bare_url_chooser(url, window, cx),
            OpenWithKind::Batch(text) => {
                view.open_import_dialog(&text, DEFAULT_GROUP, None, window, cx)
            }
        });
    }
}
