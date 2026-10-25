use crate::*;

pub(crate) fn dialog_field(id: &'static str, label: &str, state: Entity<InputState>) -> Field {
    field()
        .label(label.to_string())
        .child(div().id(id).test_support().child(Input::new(&state)))
}

pub(crate) type DialogCommit = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

pub(crate) type DialogRepaint = Rc<dyn Fn(&mut Window, &mut App)>;

pub(crate) fn dialog_footer(
    ok_id: &'static str,
    cancel_id: &'static str,
    ok_label: &'static str,
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
                .label("Cancel")
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

pub(crate) fn sheet_footer(
    ok_id: &'static str,
    cancel_id: &'static str,
    ok_label: &'static str,
    commit: DialogCommit,
) -> DialogFooter {
    debug_assert!(
        !ok_label.eq_ignore_ascii_case("ok") && !ok_label.eq_ignore_ascii_case("okay"),
        "Ticket 05: honest footers never use a bare OK label"
    );
    let commit_click = commit.clone();
    DialogFooter::new()
        .child(
            Button::new(cancel_id)
                .small()
                .label("Cancel")
                .on_click(|_, window, cx| {
                    window.close_sheet(cx);
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
                        window.close_sheet(cx);
                    }
                    reclaim_shell_focus(window, cx);
                }),
        )
}

pub(crate) fn footer_commit_label(dialog: &str) -> &'static str {
    match dialog {
        // K9 handed the dead Settings mapping over to K5: the settings modal
        // owns its own Cancel/Save footer, so "settings" falls through.
        "groups" | "preset-attach" => "Close",
        "logs" => "Apply filter",
        "presets" | "add" => "OK",
        "edit" => "OK",
        "subscriptions" => "Add",
        "preset-edit" | "subscription-edit" => "Save",
        "import" => "Import",
        "export" => "Export",
        "bulk-move" => "Move",
        "group-rename" => "Rename",
        "rule-set-add-dialog" => "Add",
        "share" => "Copy link",
        _ => "Close",
    }
}

pub(crate) fn secret_state_caption(value: &str) -> &'static str {
    if value.trim().is_empty() {
        "Not Set"
    } else {
        "Already Set"
    }
}

pub(crate) fn field_error_target(error: &StoreError) -> Option<(&'static str, String)> {
    match error {
        StoreError::Invalid(errors) => errors
            .first()
            .map(|first| (first.field, first.message.clone())),
        _ => None,
    }
}
