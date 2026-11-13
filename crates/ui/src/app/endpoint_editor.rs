//! The endpoint editor: one draft on `AppView`, two hosts (the row popover and
//! the inspector inline form), and the apply/restart/rollback path.
//! Belongs: draft lifetime, live validation, store writes and engine restart.
//! Not here: row painting, the chip or the full config form.

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointHost {
    RowPopover,
    InspectorInline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EndpointPhase {
    Editing,
    Restarting,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointIntent {
    Apply,
}

pub(crate) struct EndpointDraft {
    pub(crate) row_id: String,
    /// Read by the form; the state commit lands before its renderer.
    #[allow(dead_code)]
    pub(crate) host: EndpointHost,
    pub(crate) bind: Entity<InputState>,
    pub(crate) port: Entity<InputState>,
    pub(crate) phase: EndpointPhase,
    /// Read by the form; the state commit lands before its renderer.
    #[allow(dead_code)]
    pub(crate) error: Option<String>,
    intent: Option<EndpointIntent>,
    _subs: Vec<Subscription>,
}

/// The values and verdicts one draft frame needs. Live validation is derived,
/// never stored: every keystroke re-reads the two fields.
pub(crate) struct EndpointCheck {
    pub(crate) row_id: String,
    pub(crate) parsed: Option<EndpointValue>,
    pub(crate) error: Option<String>,
    pub(crate) collision: Option<EndpointCollision>,
    /// Read by the form's collision line; the renderer lands next.
    #[allow(dead_code)]
    pub(crate) suggestion: Option<u16>,
    pub(crate) dirty: bool,
    pub(crate) restarting: bool,
}

impl EndpointCheck {
    pub(crate) fn can_apply(&self) -> bool {
        self.dirty
            && self.parsed.is_some()
            && self.error.is_none()
            && self.collision.is_none()
            && !self.restarting
    }
}

impl AppView {
    pub(crate) fn open_endpoint_draft(
        &mut self,
        id: &str,
        host: EndpointHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning(t!("already-removed")), cx);
            return;
        };
        let bind = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(DEFAULT_BIND)
                .default_value(config.bind_ip.clone())
        });
        let port = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("10801")
                .default_value(config.port.to_string())
        });
        let mut subs = Vec::with_capacity(2);
        for input in [&bind, &port] {
            subs.push(cx.subscribe(
                input,
                |view: &mut AppView,
                 _input: Entity<InputState>,
                 event: &InputEvent,
                 cx: &mut Context<AppView>| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        if let Some(draft) = view.endpoint_draft.as_mut() {
                            draft.intent = Some(EndpointIntent::Apply);
                        }
                    }
                    cx.notify();
                },
            ));
        }
        // One editor at a time: opening another door discards the draft.
        self.endpoint_draft = Some(EndpointDraft {
            row_id: config.id,
            host,
            bind,
            port,
            phase: EndpointPhase::Editing,
            error: None,
            intent: None,
            _subs: subs,
        });
        cx.notify();
    }

    /// `Ctrl+E`: the inspector's inline host when it is already showing, the
    /// row's popover when the row is on screen, and the inspector inline host
    /// for a row a filter hides (never invent an off-screen anchor).
    pub(crate) fn open_endpoint_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inspector_visible {
            self.open_endpoint_draft(id, EndpointHost::InspectorInline, window, cx);
            return;
        }
        if self.visible_row_ids(cx).iter().any(|row| row == id) {
            self.scroll_to_row(id, window, cx);
            self.open_endpoint_draft(id, EndpointHost::RowPopover, window, cx);
        } else {
            self.open_inspector();
            self.open_endpoint_draft(id, EndpointHost::InspectorInline, window, cx);
        }
    }

    pub(crate) fn cancel_endpoint_draft(&mut self, cx: &mut Context<Self>) {
        if self.endpoint_draft.take().is_some() {
            cx.notify();
        }
    }

    /// Consume an Enter intent on the frame that can push notifications and
    /// reload. The input subscription has no window, so it only raises a flag.
    pub(crate) fn consume_endpoint_intent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let intent = self
            .endpoint_draft
            .as_mut()
            .and_then(|draft| draft.intent.take());
        if intent == Some(EndpointIntent::Apply) {
            self.apply_endpoint_draft(window, cx);
        }
    }

    pub(crate) fn endpoint_check(&self, cx: &App) -> Option<EndpointCheck> {
        let draft = self.endpoint_draft.as_ref()?;
        let row = self.row_config(&draft.row_id)?;
        let bind_text = draft.bind.read(cx).value().to_string();
        let port_text = draft.port.read(cx).value().to_string();
        let mut error = None;
        let parsed = match parse_endpoint(&bind_text, &port_text) {
            Ok(value) => {
                if let Err(gate) = validate_listen_for_save(&row.name, &value.bind, value.port) {
                    error = Some(if value.port < 1024 {
                        t!("endpoint-privileged", port = value.port)
                    } else {
                        gate.to_string()
                    });
                }
                Some(value)
            }
            Err(failure) => {
                error = Some(if failure.field == "bind" {
                    t!("endpoint-invalid-bind")
                } else {
                    t!("endpoint-invalid-port")
                });
                None
            }
        };
        let dirty = match &parsed {
            Some(value) => value.bind != row.bind_ip || value.port != row.port,
            None => bind_text.trim() != row.bind_ip || port_text.trim() != row.port.to_string(),
        };
        let (collision, suggestion) = match &parsed {
            Some(value) => {
                let rows = &self.libraries.read(cx).delegate().rows;
                match collision(rows, &row.id, value) {
                    Some(found) => (
                        Some(found),
                        next_free_port(rows, &row.id, &value.bind, value.port.saturating_add(1)),
                    ),
                    None => (None, None),
                }
            }
            None => (None, None),
        };
        Some(EndpointCheck {
            row_id: row.id,
            parsed,
            error,
            collision,
            suggestion,
            dirty,
            restarting: matches!(draft.phase, EndpointPhase::Restarting),
        })
    }

    /// Apply the draft. A stopped row is a store write; a running row also
    /// restarts, and an engine refusal rolls the store back and restores the
    /// old serving endpoint.
    pub(crate) fn apply_endpoint_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(check) = self.endpoint_check(cx) else {
            return false;
        };
        if !check.can_apply() {
            return false;
        }
        let Some(value) = check.parsed.clone() else {
            return false;
        };
        let Some(previous) = self.row_config(&check.row_id) else {
            window.push_notification(Notification::warning(t!("already-removed")), cx);
            self.cancel_endpoint_draft(cx);
            return false;
        };
        let patch = ConfigPatch {
            bind_ip: Some(value.bind.clone()),
            port: Some(value.port.to_string()),
            ..Default::default()
        };

        if !self.engine.is_running(&previous.id) {
            return match self.store.update(&previous.id, patch) {
                Ok(_) => {
                    self.cancel_endpoint_draft(cx);
                    self.reload(window, cx);
                    window.push_notification(Notification::success(t!("endpoint-updated")), cx);
                    true
                }
                Err(error) => {
                    let reason = error.to_string();
                    if let Some(draft) = self.endpoint_draft.as_mut() {
                        draft.phase = EndpointPhase::Failed(reason);
                    }
                    self.report_save_error(error, window, cx);
                    cx.notify();
                    false
                }
            };
        }

        if let Some(draft) = self.endpoint_draft.as_mut() {
            draft.phase = EndpointPhase::Restarting;
            draft.error = None;
        }
        // The synchronous engine call below must not hide the restart from
        // the next frame: paint it before the work starts.
        cx.notify();

        if let Err(reason) = self.sync_presets_to_engine() {
            if let Some(draft) = self.endpoint_draft.as_mut() {
                draft.phase = EndpointPhase::Editing;
                draft.error = Some(reason.clone());
            }
            window.push_notification(Notification::error(t!("cannot-start", reason = reason)), cx);
            cx.notify();
            return false;
        }
        if let Some(warning) = lan_exposure_warning(&previous.name, &value.bind) {
            window.push_notification(Notification::warning(warning), cx);
        }

        let updated = match self.store.update(&previous.id, patch) {
            Ok(updated) => updated,
            Err(error) => {
                let reason = error.to_string();
                if let Some(draft) = self.endpoint_draft.as_mut() {
                    draft.phase = EndpointPhase::Failed(reason);
                }
                self.report_save_error(error, window, cx);
                cx.notify();
                return false;
            }
        };

        match self.engine.start_one_confirmed(&updated) {
            Ok(()) => {
                self.cancel_endpoint_draft(cx);
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(t!("endpoint-restarted", name = updated.name.as_str())),
                    cx,
                );
                true
            }
            Err(error) => {
                let reason = redact_secrets(&error.to_string());
                // Restore the store first: the row and inspector must agree
                // with the old endpoint even if the old restart also fails.
                let mut detail = reason.clone();
                match self.store.update(
                    &previous.id,
                    ConfigPatch {
                        bind_ip: Some(previous.bind_ip.clone()),
                        port: Some(previous.port.to_string()),
                        ..Default::default()
                    },
                ) {
                    Ok(_) => {}
                    Err(rollback) => {
                        detail.push_str(&format!("; restore failed: {rollback}"));
                    }
                }
                if let Err(restore) = self.engine.start_one_confirmed(&previous) {
                    detail.push_str(&format!(
                        "; old endpoint did not restart: {}",
                        redact_secrets(&restore.to_string())
                    ));
                }
                self.reload(window, cx);
                if let Some(draft) = self.endpoint_draft.as_mut() {
                    draft.phase = EndpointPhase::Failed(reason);
                }
                window.push_notification(
                    Notification::error(t!("endpoint-not-changed", reason = detail)),
                    cx,
                );
                cx.notify();
                false
            }
        }
    }
}
