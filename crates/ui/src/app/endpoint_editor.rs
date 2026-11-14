//! The endpoint editor: one draft on `AppView`, two hosts (the row popover and
//! the inspector inline form), and the apply/restart/rollback path.
//! Belongs: draft lifetime, live validation, store writes and engine restart.
//! Not here: row painting, the chip or the full config form.

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointHost {
    RowPopover,
    InspectorInline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointPhase {
    Editing,
    Restarting,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointIntent {
    Apply,
}

/// The key context the shared form carries. Its Escape and Enter bindings are
/// deeper than the kit popover's own, so the form decides what Escape means
/// before the popover dismisses.
pub const ENDPOINT_EDITOR_CONTEXT: &str = "endpoint-editor";

pub struct EndpointDraft {
    pub row_id: String,
    pub host: EndpointHost,
    pub bind: Entity<InputState>,
    pub port: Entity<InputState>,
    pub focus: FocusHandle,
    pub phase: EndpointPhase,
    pub error: Option<String>,
    pub intent: Option<EndpointIntent>,
    pub _subs: Vec<Subscription>,
}

/// The values and verdicts one draft frame needs. Live validation is derived,
/// never stored: every keystroke re-reads the two fields.
#[derive(Debug, Clone)]
pub struct EndpointCheck {
    pub row_id: String,
    pub parsed: Option<EndpointValue>,
    pub error: Option<String>,
    pub collision: Option<EndpointCollision>,
    pub suggestion: Option<u16>,
    pub dirty: bool,
    pub restarting: bool,
}

/// One frame of an open draft, copied out of `AppView` and the list so the
/// row host can paint without borrowing either entity mid-render.
#[derive(Clone)]
pub struct EndpointFrame {
    pub row_id: String,
    pub host: EndpointHost,
    pub bind: Entity<InputState>,
    pub port: Entity<InputState>,
    /// The form itself takes focus, so its Escape and Enter bindings outrank
    /// the popover's own Cancel/Confirm.
    pub focus: FocusHandle,
    pub check: EndpointCheck,
    pub name: String,
    /// The engine or store message from a failed apply.
    pub failure: Option<String>,
    /// The row sits too close to the bottom edge for a `TopLeft` popover.
    pub flip: bool,
}

impl EndpointCheck {
    pub fn can_apply(&self) -> bool {
        self.dirty
            && self.parsed.is_some()
            && self.error.is_none()
            && self.collision.is_none()
            && !self.restarting
    }
}

impl AppView {
    pub fn open_endpoint_draft(
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
        // A custom-config row's listens come from its JSON document; the
        // stored bind and port are display only, so editing them would claim
        // a change the engine never makes.
        if is_custom_config_protocol(&config.protocol) {
            return;
        }
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
                    // A failed apply clears as soon as the user changes the
                    // value that failed; the next Apply starts from the draft.
                    if matches!(event, InputEvent::Change) {
                        if let Some(draft) = view.endpoint_draft.as_mut() {
                            draft.phase = EndpointPhase::Editing;
                            draft.error = None;
                        }
                    }
                    cx.notify();
                },
            ));
        }
        // One editor at a time: opening another door discards the draft.
        let focus = cx.focus_handle();
        self.endpoint_draft = Some(EndpointDraft {
            row_id: config.id,
            host,
            bind,
            port,
            focus: focus.clone(),
            phase: EndpointPhase::Editing,
            error: None,
            intent: None,
            _subs: subs,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// `Ctrl+E`: the inspector's inline host when it is already showing, the
    /// row's popover when the row is on screen, and the inspector inline host
    /// for a row a filter hides (never invent an off-screen anchor).
    pub fn open_endpoint_for_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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

    pub fn cancel_endpoint_draft(&mut self, cx: &mut Context<Self>) {
        if self.endpoint_draft.take().is_some() {
            cx.notify();
        }
    }

    /// A draft for a row the current filter or store no longer has cannot
    /// paint a host, so it closes rather than outliving its row. The row
    /// popover also closes when the list virtualizes its row away: the
    /// popover has no anchor to follow back into view.
    pub fn prune_endpoint_draft(&mut self, cx: &App) {
        let Some(draft) = self.endpoint_draft.as_ref() else {
            return;
        };
        if self.row_config(&draft.row_id).is_none() {
            self.endpoint_draft = None;
            return;
        }
        if draft.host != EndpointHost::RowPopover {
            return;
        }
        let list = self.libraries.read(cx);
        let Some(index) = self.visible.iter().position(|&row_ix| {
            list.delegate()
                .rows
                .get(row_ix)
                .is_some_and(|row| row.id == draft.row_id)
        }) else {
            self.endpoint_draft = None;
            return;
        };
        let Some((top, pitch, viewport_h)) = self.endpoint_row_position(index, cx) else {
            return;
        };
        if top + pitch <= 0. || top >= viewport_h {
            self.endpoint_draft = None;
        }
    }

    /// `Esc`: a dirty draft goes back to the stored values and stays open; a
    /// clean one closes. The popover's own Escape never sees it.
    pub fn escape_endpoint_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.endpoint_draft.as_ref() else {
            return;
        };
        let Some(row) = self.row_config(&draft.row_id) else {
            self.cancel_endpoint_draft(cx);
            return;
        };
        let bind_text = draft.bind.read(cx).value().to_string();
        let port_text = draft.port.read(cx).value().to_string();
        if bind_text == row.bind_ip && port_text == row.port.to_string() {
            self.cancel_endpoint_draft(cx);
            return;
        }
        let bind = draft.bind.clone();
        let port = draft.port.clone();
        bind.update(cx, |state, cx| {
            state.set_value(row.bind_ip.clone(), window, cx)
        });
        port.update(cx, |state, cx| {
            state.set_value(row.port.to_string(), window, cx)
        });
        cx.notify();
    }

    /// The first free port for the typed bind, written into the port field.
    pub fn use_suggested_port(&mut self, port: u16, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.endpoint_draft.as_ref() else {
            return;
        };
        let input = draft.port.clone();
        input.update(cx, |state, cx| {
            state.set_value(port.to_string(), window, cx)
        });
        cx.notify();
    }

    /// Everything one paint of the shared form needs, copied out of `AppView`
    /// before the list borrows itself for the frame.
    pub fn endpoint_frame(&self, cx: &App) -> Option<EndpointFrame> {
        let draft = self.endpoint_draft.as_ref()?;
        let check = self.endpoint_check(cx)?;
        let row = self.row_config(&draft.row_id)?;
        let failure = match &draft.phase {
            EndpointPhase::Failed(reason) => Some(reason.clone()),
            _ => draft.error.clone(),
        };
        let flip =
            draft.host == EndpointHost::RowPopover && self.endpoint_popover_flip(&draft.row_id, cx);
        Some(EndpointFrame {
            row_id: draft.row_id.clone(),
            host: draft.host,
            bind: draft.bind.clone(),
            port: draft.port.clone(),
            focus: draft.focus.clone(),
            check,
            name: row.name,
            failure,
            flip,
        })
    }

    /// The row's top in the list viewport, from the uniform pitch and the
    /// scroll offset. The library has no section rows, so index × pitch is
    /// exact; the kit's scroll handle does not expose virtual item bounds for
    /// this list, so its `bounds_for_item` always answers `None`.
    pub fn endpoint_row_position(&self, index: usize, cx: &App) -> Option<(f32, f32, f32)> {
        let list = self.libraries.read(cx);
        let viewport = list.scroll_handle().bounds();
        if viewport.size.height <= px(0.) {
            return None;
        }
        let pitch = list.delegate().density.row_height();
        let top = pitch * index as f32 + list.scroll_handle().base_handle().offset().y.as_f32();
        Some((top, pitch, viewport.size.height.as_f32()))
    }

    /// The row popover opens below its chip unless the chip sits close enough
    /// to the bottom edge that the form would be clamped over the row.
    pub fn endpoint_popover_flip(&self, id: &str, cx: &App) -> bool {
        const FORM_HEIGHT: f32 = 260.;
        let list = self.libraries.read(cx);
        let Some(index) = self.visible.iter().position(|&row_ix| {
            list.delegate()
                .rows
                .get(row_ix)
                .is_some_and(|row| row.id == id)
        }) else {
            return false;
        };
        let Some((top, pitch, viewport_h)) = self.endpoint_row_position(index, cx) else {
            return false;
        };
        top + pitch + FORM_HEIGHT > viewport_h
    }

    /// Enter from either field, or from the form itself, only raises a flag;
    /// the next frame consumes it once with a window in hand.
    pub fn queue_endpoint_apply(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = self.endpoint_draft.as_mut() {
            draft.intent = Some(EndpointIntent::Apply);
            cx.notify();
        }
    }

    /// Consume an Enter intent on the frame that can push notifications and
    /// reload. The input subscription has no window, so it only raises a flag.
    pub fn consume_endpoint_intent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let intent = self
            .endpoint_draft
            .as_mut()
            .and_then(|draft| draft.intent.take());
        if intent == Some(EndpointIntent::Apply) {
            self.apply_endpoint_draft(window, cx);
        }
    }

    pub fn endpoint_check(&self, cx: &App) -> Option<EndpointCheck> {
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
    pub fn apply_endpoint_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
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

/// The row host: the one popover the owning row paints, anchored to its pencil.
/// `open` is controlled by the draft, so an outside click cancels it through
/// `on_open_change` instead of leaving a hidden draft behind.
pub fn render_endpoint_popover(app: Entity<AppView>, frame: EndpointFrame) -> AnyElement {
    let anchor = if frame.flip {
        Anchor::BottomLeft
    } else {
        Anchor::TopLeft
    };
    let name = frame.name.clone();
    let content_app = app.clone();
    let content_frame = frame.clone();
    let close_app = app;
    Popover::new(format!("endpoint-popover-{}", frame.row_id))
        .anchor(anchor)
        .open(true)
        .track_focus(&frame.focus)
        .trigger(
            Button::new(format!("row-endpoint-edit-{}", frame.row_id))
                .small()
                .compact()
                .label("✎")
                .accessibility_label(t!("endpoint-edit-aria", name = name))
                .tooltip(t!("endpoint-edit-tip")),
        )
        .content(move |_, _, cx| render_endpoint_form(content_app.clone(), &content_frame, cx))
        .on_open_change(move |open, _, cx| {
            if !*open {
                close_app.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
            }
        })
        .into_any_element()
}

/// The one form both hosts paint. It reads the live `InputState`s directly, so
/// every keystroke re-validates without a third copy of the text.
pub fn render_endpoint_form(app: Entity<AppView>, frame: &EndpointFrame, cx: &App) -> AnyElement {
    let inline = frame.host == EndpointHost::InspectorInline;
    let muted = cx.theme().muted_foreground;
    let danger = danger_text_color(cx.theme());
    let mono = cx.theme().mono_font_family.clone();
    let escaped_app = app.clone();
    let apply_key_app = app.clone();
    let applied_button_app = app.clone();
    let cancelled_app = app.clone();
    let used_app = app.clone();
    let suggestion = frame.check.suggestion;
    let can_apply = frame.check.can_apply();
    let restarting = frame.check.restarting;

    let mut form = div()
        .id("endpoint-form")
        .test_support()
        .flex()
        .flex_col()
        .gap_2()
        .track_focus(&frame.focus)
        .key_context(ENDPOINT_EDITOR_CONTEXT)
        .on_action(move |_: &EscapeEndpoint, window, cx| {
            cx.stop_propagation();
            escaped_app.update(cx, |view, cx| view.escape_endpoint_draft(window, cx));
        })
        .on_action(move |_: &ApplyEndpoint, _, cx| {
            cx.stop_propagation();
            // The field's own Enter already raised the same intent; raising it
            // again here must not apply twice in one keystroke.
            apply_key_app.update(cx, |view, cx| view.queue_endpoint_apply(cx));
        });
    form = if inline {
        form.w_full().max_w(px(420.))
    } else {
        form.w(px(296.))
    };

    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .w_full()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(t!("endpoint-editor-title")),
        )
        .child(div().flex_1().min_w_0());
    if restarting {
        header = header.child(
            div()
                .id("endpoint-restoring")
                .test_support()
                .truncate()
                .text_sm()
                .text_color(muted)
                .child(t!("endpoint-restoring", name = frame.name.as_str())),
        );
    }

    let bind_label = t!("endpoint-bind-label");
    let port_label = t!("endpoint-port-label");
    let fields = div()
        .flex()
        .flex_row()
        .items_end()
        .gap_2()
        .w_full()
        .min_w_0()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().text_color(muted).child(bind_label.clone()))
                .child(
                    div().font_family(mono.clone()).child(
                        Input::new(&frame.bind)
                            .id("endpoint-bind-input")
                            .small()
                            .bordered(true)
                            .disabled(restarting)
                            .aria_label(bind_label),
                    ),
                ),
        )
        .child(
            div()
                .w(px(84.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().text_color(muted).child(port_label.clone()))
                .child(
                    div().font_family(mono).child(
                        Input::new(&frame.port)
                            .id("endpoint-port-input")
                            .small()
                            .bordered(true)
                            .disabled(restarting)
                            .aria_label(port_label),
                    ),
                ),
        );

    form = form.child(header).child(fields);

    let error = frame.failure.clone().or_else(|| frame.check.error.clone());
    if let Some(error) = error {
        form = form.child(
            div()
                .id("endpoint-error")
                .test_support()
                .w_full()
                .text_sm()
                .text_color(danger)
                .child(error),
        );
    }
    if let Some(found) = frame.check.collision.as_ref() {
        let bind = frame
            .check
            .parsed
            .as_ref()
            .map(|value| value.bind.clone())
            .unwrap_or_else(|| found.bind.clone());
        let mut line = div()
            .id("endpoint-collision")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(danger)
                    .child(t!(
                        "endpoint-collision",
                        port = found.port,
                        bind = bind,
                        name = found.name.as_str()
                    )),
            );
        if let Some(port) = suggestion {
            let used_app = used_app.clone();
            line = line.child(
                Button::new("endpoint-use-port")
                    .small()
                    .label(t!("endpoint-use-port", port = port))
                    .on_click(move |_, window, cx| {
                        used_app.update(cx, |view, cx| {
                            view.use_suggested_port(port, window, cx);
                        });
                    }),
            );
        }
        form = form.child(line);
    }

    form.child(
        div()
            .flex()
            .flex_row()
            .justify_end()
            .items_center()
            .gap_2()
            .w_full()
            .child(
                Button::new("endpoint-cancel")
                    .small()
                    .label(t!("endpoint-cancel"))
                    .on_click(move |_, _, cx| {
                        cancelled_app.update(cx, |view, cx| view.cancel_endpoint_draft(cx));
                    }),
            )
            .child(
                Button::new("endpoint-apply")
                    .small()
                    .label(t!("endpoint-apply"))
                    .disabled(!can_apply)
                    .on_click(move |_, window, cx| {
                        applied_button_app.update(cx, |view, cx| {
                            view.apply_endpoint_draft(window, cx);
                        });
                    }),
            ),
    )
    .into_any_element()
}
