use crate::*;

impl AppView {
    /// T-03: the one SOCKS copy. It reads the row by id and uses the STORED
    /// bind host, so the row's own `Copy SOCKS` and the inspector's SOCKS
    /// control always copy the row they belong to — never the selection.
    pub(crate) fn copy_row_socks(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let (socks, _) = proxy_urls(&config.bind_ip, config.port);
        cx.write_to_clipboard(ClipboardItem::new_string(socks));
        window.push_notification(Notification::success("Copied SOCKS URL"), cx);
    }

    pub(crate) fn copy_row_urls(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let (socks, http) = proxy_urls(&config.bind_ip, config.port);
        cx.write_to_clipboard(ClipboardItem::new_string(format!("{socks}\n{http}")));
        window.push_notification(Notification::success("Copied proxy URLs"), cx);
    }

    /// T-32: mirrors the window rectangle into meta on the same orderly-exit
    /// paths `snapshot_running_set` already uses. A killed process keeps the
    /// last clean-exit save — the snapshot's own tolerance. Maximized
    /// rectangles are not persisted (out of scope): the pre-maximize windowed
    /// rect from the previous clean exit stays current.
    ///
    /// The value must come from `window.window_bounds()`, not
    /// `window.bounds()`: the former is the exact inverse of the
    /// `WindowBounds::Windowed` transform startup uses, while the live client
    /// rect skips the frame split and walked the window up 4px per relaunch.
    pub(crate) fn save_window_geometry(&mut self, window: &Window) {
        let WindowBounds::Windowed(bounds) = window.window_bounds() else {
            return;
        };
        let geometry = WindowGeometry {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };
        if let Err(error) = self.store.set_window_geometry(geometry) {
            eprintln!("window geometry save failed: {error}");
        }
    }

    pub(crate) fn snapshot_running_set(&mut self, running: &[String]) {
        if self.last_snapshot.as_slice() == running {
            return;
        }
        self.last_snapshot = running.to_vec();
        if let Err(error) = self.store.save_running_snapshot(running) {
            eprintln!("running snapshot failed: {error}");
        }
    }

    pub(crate) fn copy_config_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = crate::tray::config_folder();
        cx.write_to_clipboard(ClipboardItem::new_string(folder.clone()));
        window.push_notification(
            Notification::success(format!("Copied config folder path ({folder})")),
            cx,
        );
    }

    pub(crate) fn write_visible_logs_to(
        &self,
        path: &std::path::Path,
        cx: &App,
    ) -> Result<(), String> {
        let body = self.visible_log_lines(cx).join("\n");
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, body).map_err(|error| format!("Cannot save logs: {error}"))?;
        std::fs::rename(&tmp, path).map_err(|error| format!("Cannot save logs: {error}"))?;
        Ok(())
    }

    pub(crate) fn save_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = Store::default_logs_path();
        if path.exists() {
            let view = cx.entity();
            let description = logs_overwrite_text(&path);
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text("Overwrite")
                            .ok_variant(ButtonVariant::Danger),
                    )
                    .title("Overwrite saved logs?")
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| view.commit_save_visible_logs(window, cx));
                        true
                    })
            });
            return;
        }
        self.commit_save_visible_logs(window, cx);
    }

    pub(crate) fn copy_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lines = self.visible_log_lines(cx);
        if lines.is_empty() {
            window.push_notification(Notification::warning("No log lines to copy"), cx);
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
        window.push_notification(
            Notification::success(format!("Copied {} log line(s)", lines.len())),
            cx,
        );
    }

    pub(crate) fn wipe_library_for_restore(&mut self) -> Result<(), StoreError> {
        let subs = self.store.list_subscriptions()?;
        for sub in &subs {
            self.store.remove_subscription(&sub.id)?;
        }
        let rows = self.store.list()?;
        for row in &rows {
            self.store.remove(&row.id)?;
        }
        Ok(())
    }

    pub(crate) fn paste_intake_refusal(text: &str) -> Option<&'static str> {
        use crate::preset::{is_remote_route_attempt, is_route_attempt, looks_like_preset_json};
        if text.trim().is_empty() {
            return Some("Clipboard has no text to import");
        }
        if is_library_export(text) {
            return None;
        }
        let trimmed = text.trim();
        let single_line = !trimmed.contains('\n');
        if single_line && is_addsub_attempt(trimmed) {
            return None;
        }
        if single_line && (is_route_attempt(trimmed) || is_remote_route_attempt(trimmed)) {
            return None;
        }
        if looks_like_preset_json(trimmed) {
            return None;
        }
        if import_text(text).configs.is_empty() {
            return Some("Nothing importable in clipboard");
        }
        None
    }

    /// K5: `Ctrl+V` / `From clipboard` / clipboard paste imports straight
    /// into the current group — no blocking preview dialog when the content
    /// parses cleanly. The ambiguous branches of the import grammar keep
    /// their dialog (library exports keep their explicit-confirm restore —
    /// hard rule; the bare-HTTP chooser and preset shells keep asking).
    pub(crate) fn import_text_direct(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(reason) = Self::paste_intake_refusal(&text) {
            window.push_notification(Notification::warning(reason), cx);
            return;
        }
        let group = self
            .current_group(cx)
            .unwrap_or_else(|| DEFAULT_GROUP.to_string());
        self.commit_import_dialog(text, group, None, None, None, window, cx);
    }

    pub(crate) fn import_from_clipboard_direct(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        self.import_text_direct(text, window, cx);
    }

    /// F7: every dropped file lands in the Import dialog when it is open —
    /// the same merge the Browse button runs — instead of being refused. Any
    /// other overlay still refuses; only the Import form absorbs drops.
    pub(crate) fn import_dropped_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.import_form.clone() {
            if window.has_active_dialog(cx) {
                let text = form.text.read(cx).value().to_string();
                let group = form.group.read(cx).value().to_string();
                let (files, unreadable) = Self::read_dropped_import_files(paths);
                if files.iter().all(|(_, content)| content.trim().is_empty()) {
                    window.push_notification(
                        Notification::warning(Self::drop_refusal_text(&unreadable)),
                        cx,
                    );
                    return;
                }
                self.finish_browse_pick(text, group, files, unreadable, window, cx);
                return;
            }
            // The dialog is gone without its close path running (another
            // surface cleared the layer); never route into a dead form.
            self.import_form = None;
        }
        if overlay_open(window, cx) {
            window.push_notification(
                Notification::warning(
                    "Close the open dialog first — dropped files were not imported",
                ),
                cx,
            );
            return;
        }
        let (files, unreadable) = Self::read_dropped_import_files(paths);
        let merged = files
            .iter()
            .map(|(_, content)| content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if merged.trim().is_empty() {
            window.push_notification(
                Notification::warning(Self::drop_refusal_text(&unreadable)),
                cx,
            );
            return;
        }
        if !unreadable.is_empty() {
            window.push_notification(
                Notification::warning(unreadable_drop_message(&unreadable)),
                cx,
            );
        }
        if files.len() == 1 {
            let file_hint = files.into_iter().next().map(|(name, _)| name);
            self.open_import_dialog(&merged, DEFAULT_GROUP, file_hint, window, cx);
        } else {
            self.open_import_dialog_with_files(
                &merged,
                DEFAULT_GROUP,
                None,
                Some(files),
                window,
                cx,
            );
        }
    }

    fn read_dropped_import_files(paths: &ExternalPaths) -> (Vec<(String, String)>, Vec<String>) {
        let mut files: Vec<(String, String)> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        for path in paths.0.iter() {
            let display = path.to_string_lossy().into_owned();
            match read_import_file(path) {
                ImportFileRead::Text(content) => files.push((display, content)),
                ImportFileRead::Unreadable => unreadable.push(file_basename(&display)),
                ImportFileRead::Refused(reason) => {
                    unreadable.push(format!("{} ({reason})", file_basename(&display)))
                }
            }
        }
        (files, unreadable)
    }

    fn drop_refusal_text(unreadable: &[String]) -> String {
        if unreadable.is_empty() {
            "No readable files dropped".to_string()
        } else {
            unreadable_drop_message(unreadable)
        }
    }

    pub(crate) fn import_parse_cancelled(&mut self, epoch: u64) -> bool {
        if self.import_cancel_epoch == Some(epoch) {
            self.import_cancel_epoch = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn take_batch_name_lists(
        &mut self,
    ) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
        (
            std::mem::take(&mut self.refresh.added_names),
            std::mem::take(&mut self.refresh.updated_names),
            std::mem::take(&mut self.refresh.removed_names),
            std::mem::take(&mut self.refresh.in_use_names),
        )
    }

    pub(crate) fn take_batch_counts(
        &mut self,
    ) -> (usize, usize, usize, usize, usize, usize, usize) {
        self.refresh.members.clear();
        let total = std::mem::take(&mut self.refresh.total);
        let added = std::mem::take(&mut self.refresh.added);
        let updated = std::mem::take(&mut self.refresh.updated);
        let removed = std::mem::take(&mut self.refresh.removed);
        let failed = std::mem::take(&mut self.refresh.failed);
        let conflicts = std::mem::take(&mut self.refresh.conflicts);
        let in_use = std::mem::take(&mut self.refresh.in_use);
        self.refresh.added_names.clear();
        self.refresh.updated_names.clear();
        self.refresh.removed_names.clear();
        self.refresh.in_use_names.clear();
        (total, added, updated, removed, failed, conflicts, in_use)
    }

    pub(crate) fn flush_refresh_batch(&mut self, window: &mut Window, cx: &mut App) {
        let (added_names, updated_names, removed_names, kept_in_use_names) =
            self.take_batch_name_lists();
        let (total, added, updated, removed, failed, conflicts, in_use) = self.take_batch_counts();
        let (warn, mut text) =
            coalesced_refresh_message(total, added, updated, removed, failed, conflicts, in_use)
                .unwrap_or((false, String::new()));
        if text.is_empty() {
            return;
        }
        if let Some(report) = refresh_named_report_block(
            &added_names,
            &updated_names,
            &removed_names,
            &kept_in_use_names,
        ) {
            text.push('\n');
            text.push_str(&report);
        }
        window.push_notification(
            if warn {
                Notification::warning(text)
            } else {
                Notification::success(text)
            },
            cx,
        );
    }

    /// Post-download staging shared by both finish variants: write the staged
    /// bytes, then record the row (or refresh the identical row on re-download).
    pub(crate) fn stage_rule_set_bytes(
        &mut self,
        grant: &RuleSetDownloadGrant,
        bytes: Vec<u8>,
    ) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&grant.dir)
            .map_err(|error| format!("cannot create {}: {error}", grant.dir.display()))?;
        let path = grant.dir.join(&grant.file_name);
        std::fs::write(&path, &bytes)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        let existing = self
            .store
            .list_rule_sets()
            .ok()
            .and_then(|rows| rows.into_iter().find(|row| row.name == grant.name));
        let recorded = match existing {
            Some(row) => self
                .store
                .touch_rule_set(&row.id, &grant.url, bytes.len() as u64),
            None => self
                .store
                .create_rule_set(&grant.name, &grant.url, bytes.len() as u64),
        };
        if let Err(error) = recorded {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_string());
        }
        self.sync_presets_to_engine()?;
        Ok(path)
    }

    /// Writes the cell value to the clipboard (bind:port-safe display strings
    /// only). Returns `false` without touching the clipboard for an empty value.
    pub(crate) fn copy_connection_dest(
        &mut self,
        dest: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if dest.is_empty() {
            window.push_notification(Notification::warning("No destination to copy"), cx);
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(dest.to_string()));
        true
    }

    /// Click-to-copy for a Connections cell: clipboard write plus a transient
    /// inline `Copied!` marker anchored at the clicked cell. Repeat clicks reuse
    /// the generation guard so the newest marker survives its full window.
    pub(crate) fn copy_conn_cell(
        &mut self,
        target: ConnCopyTarget,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.copy_connection_dest(value, window, cx) {
            return;
        }
        self.conn.copied = Some(target);
        self.conn.copied_generation = self.conn.copied_generation.wrapping_add(1);
        let generation = self.conn.copied_generation;
        cx.notify();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(Self::CONN_COPIED_TOOLTIP_MS))
                .await;
            cx.update(|cx| {
                weak.update(cx, |view, cx| {
                    if view.conn.copied_generation == generation {
                        view.conn.copied = None;
                        cx.notify();
                    }
                })
                .ok();
            });
        })
        .detach();
    }
}
