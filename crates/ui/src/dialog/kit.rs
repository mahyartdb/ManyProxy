use crate::*;

/// K12: the visible `Field` label is not an accessible name, so a labelled
/// input carries the same words through `aria_label`. Required-field stars are
/// visual, not spoken.
pub fn accessible_label(label: &str) -> String {
    label.trim_end_matches('*').trim_end().to_string()
}

pub fn labelled_input(label: impl Into<String>, state: &Entity<InputState>) -> Input {
    let label = label.into();
    Input::new(state).aria_label(accessible_label(&label))
}

pub fn labelled_masked_input(label: impl Into<String>, state: &Entity<InputState>) -> Input {
    let label = label.into();
    Input::new(state)
        .mask_toggle()
        .aria_label(accessible_label(&label))
}

pub fn labelled_textarea(
    label: impl Into<String>,
    state: &Entity<TextareaState>,
) -> Textarea {
    let label = label.into();
    Textarea::new(state).aria_label(accessible_label(&label))
}

pub fn dialog_field(
    id: &'static str,
    label: impl Into<String>,
    state: Entity<InputState>,
) -> Field {
    let label = label.into();
    field()
        .label(label.clone())
        .child(labelled_input(label, &state).id(id))
}

pub type DialogCommit = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

pub type DialogRepaint = Rc<dyn Fn(&mut Window, &mut App)>;

pub fn dialog_footer(
    ok_id: &'static str,
    cancel_id: &'static str,
    ok_label: &str,
    commit: DialogCommit,
) -> DialogFooter {
    // K2/K3: the Throne-grammar dialogs (add / edit / routing presets) carry a
    // literal `OK` by ticket decision — they are the only exempt footers.
    let throne_grammar = matches!(
        ok_id,
        "add-dialog-ok" | "edit-dialog-ok" | "presets-dialog-ok"
    );
    debug_assert!(
        throne_grammar
            || (!ok_label.eq_ignore_ascii_case("ok") && !ok_label.eq_ignore_ascii_case("okay")),
        "Ticket 05: honest footers never use a bare OK label"
    );
    if ok_label.eq_ignore_ascii_case("close") {
        return DialogFooter::new().child(Button::new(ok_id).small().label(ok_label).on_click(
            move |_, window, cx| {
                if commit(window, cx) {
                    window.close_all_dialogs(cx);
                }
                reclaim_shell_focus(window, cx);
            },
        ));
    }
    let commit_click = commit.clone();
    DialogFooter::new()
        .child(
            Button::new(cancel_id)
                .small()
                .label(t!("dialog-cancel"))
                .on_click(|_, window, cx| {
                    window.close_all_dialogs(cx);
                    reclaim_shell_focus(window, cx);
                }),
        )
        .child(
            Button::new(ok_id)
                .small()
                .primary()
                .label(ok_label)
                .on_click(move |_, window, cx| {
                    if commit_click(window, cx) {
                        window.close_all_dialogs(cx);
                    }
                    reclaim_shell_focus(window, cx);
                }),
        )
}

/// F13: `cancel_id` is None where the sheet's primary button already is the
/// only exit — Groups had Cancel and Close beside the header close.
pub fn sheet_footer(
    ok_id: &'static str,
    cancel_id: Option<&'static str>,
    ok_label: &str,
    commit: DialogCommit,
) -> DialogFooter {
    debug_assert!(
        !ok_label.eq_ignore_ascii_case("ok") && !ok_label.eq_ignore_ascii_case("okay"),
        "Ticket 05: honest footers never use a bare OK label"
    );
    let commit_click = commit.clone();
    let mut footer = DialogFooter::new();
    if let Some(cancel_id) = cancel_id {
        footer = footer.child(
            Button::new(cancel_id)
                .small()
                .label(t!("dialog-cancel"))
                .on_click(|_, window, cx| {
                    window.close_sheet(cx);
                    reclaim_shell_focus(window, cx);
                }),
        );
    }
    footer.child(
        Button::new(ok_id)
            .small()
            .primary()
            .label(ok_label)
            .on_click(move |_, window, cx| {
                if commit_click(window, cx) {
                    window.close_sheet(cx);
                }
                reclaim_shell_focus(window, cx);
            }),
    )
}

pub fn footer_commit_label(dialog: &str) -> String {
    match dialog {
        // K9 handed the dead Settings mapping over to K5: the settings modal
        // owns its own Cancel/Save footer, so "settings" falls through.
        "groups" | "preset-attach" => t!("dialog-close"),
        "logs" => t!("dialog-apply-filter"),
        "presets" | "add" => t!("dialog-ok"),
        "edit" => t!("dialog-ok"),
        "subscriptions" => t!("dialog-add"),
        "preset-edit" | "subscription-edit" | "group-edit" => t!("dialog-save"),
        "import" => t!("dialog-import"),
        "export" => t!("dialog-export"),
        "bulk-move" => t!("dialog-move"),
        "group-rename" => t!("dialog-rename"),
        "rule-set-add-dialog" => t!("dialog-add"),
        "share" => t!("dialog-copy-link"),
        _ => t!("dialog-close"),
    }
}

pub fn secret_state_caption(value: &str) -> String {
    if value.trim().is_empty() {
        t!("secret-not-set")
    } else {
        t!("secret-already-set")
    }
}

pub fn field_error_target(error: &StoreError) -> Option<(&'static str, String)> {
    match error {
        StoreError::Invalid(errors) => errors
            .first()
            .map(|first| (first.field, first.message.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::accessible_label;

    #[test]
    fn required_stars_are_visual_only() {
        assert_eq!(accessible_label("Password *"), "Password");
        assert_eq!(accessible_label("Name*"), "Name");
        assert_eq!(
            accessible_label("Test URL (blank = global)"),
            "Test URL (blank = global)"
        );
    }
}
