//! Creating a group and managing one group.
//!
//! The all-groups sheet had no selected group, no order and no counts. The nav
//! owns the ordered list now; this sheet edits exactly the group the nav
//! handed it, through the same store methods and confirms the old sheet used.

use crate::*;

impl AppView {
    pub fn open_new_group_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_state = cx.new(|cx| InputState::new(window, cx));
        let view = cx.entity();
        let reader = name_state.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = reader.read(cx).value().to_string();
            view.update(cx, |view, cx| view.create_group(&name, window, cx))
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(t!("group-new-title"))
                .child(v_form().small().child(dialog_field(
                    "field-group-new-field",
                    t!("group-new-field"),
                    name_state.clone(),
                )))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "group-new-ok",
                    "group-new-cancel",
                    &footer_commit_label("group-edit"),
                    commit_click,
                ))
        });
    }

    /// A created group is session-only; it needs no store row to exist. The
    /// reserved default name belongs to the `Ungrouped` bucket, and an existing
    /// group is never duplicated in the nav.
    pub fn create_group(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() {
            window.push_notification(Notification::warning(t!("group-new-empty")), cx);
            return false;
        }
        if name == DEFAULT_GROUP {
            window.push_notification(Notification::warning(t!("group-new-reserved")), cx);
            return false;
        }
        let exists = self
            .group_list
            .iter()
            .any(|group| !group.ungrouped && group.name == name);
        if exists {
            window.push_notification(
                Notification::warning(t!("group-new-exists", name = name)),
                cx,
            );
            return false;
        }
        self.created_groups.push(name.to_string());
        self.refresh_scope_index(cx);
        self.set_scope(LibraryScope::Group(name.to_string()), window, cx);
        true
    }

    pub fn rename_group_scope(
        &mut self,
        old: &str,
        new: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.commit_rename_group(old, new, window, cx)
    }

    pub fn delete_group_scope(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_delete_group(name, window, cx);
    }

    pub fn open_group_manage_sheet(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let group = group.to_string();
        let slug = slugify_group_id(&group);
        let counts = self.store.group_counts().unwrap_or_default();
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let flagged: HashSet<String> = self
            .store
            .group_skip_flags()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let auto_flagged: HashSet<String> =
            self.store.auto_clear_enabled_groups().into_iter().collect();
        let views = group_row_views(
            &counts,
            &subs,
            &|name| flagged.contains(name),
            &|name| auto_flagged.contains(name),
            now_unix(),
        );
        let view = views.iter().find(|row| row.name == group);
        let count = view.map(|row| row.count).unwrap_or(0);
        let kind = view
            .map(|row| row.kind.clone())
            .unwrap_or_else(|| t!("group-kind-basic"));
        let quota = view.and_then(|row| row.quota.clone());
        let sub_ids = view.map(|row| row.sub_ids.clone()).unwrap_or_default();
        let auto_clear = view.map(|row| row.auto_clear).unwrap_or(false);
        let has_subs = !sub_ids.is_empty();
        let muted = cx.theme().muted_foreground;
        let commit: DialogCommit = Rc::new(|_, _| true);
        let sheet_view = cx.entity();

        open_app_sheet(window, cx, move |sheet: Sheet, _, _| {
            let commit_click = commit.clone();
            let rename_view = sheet_view.clone();
            let edit_view = sheet_view.clone();
            let refresh_view = sheet_view.clone();
            let delete_view = sheet_view.clone();
            let auto_view = sheet_view.clone();
            let rename_group = group.clone();
            let edit_group = group.clone();
            let refresh_group = group.clone();
            let delete_group = group.clone();
            let auto_group = group.clone();

            let detail = div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .id(format!("group-manage-count-{slug}"))
                        .test_support()
                        .child(t!("group-manage-count", count = count)),
                )
                .child(
                    div()
                        .id(format!("group-kind-{slug}"))
                        .test_support()
                        .text_color(muted)
                        .child(kind.clone()),
                )
                .children(quota.clone().map(|quota| {
                    div()
                        .id(format!("group-quota-{slug}"))
                        .test_support()
                        .child(quota)
                }));
            let actions = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    Button::new("group-manage-rename")
                        .label(t!("group-rename"))
                        .on_click(move |_, window, cx| {
                            let group = rename_group.clone();
                            rename_view.update(cx, |view, cx| {
                                view.open_group_rename_dialog(&group, window, cx)
                            });
                        }),
                )
                .child(
                    Button::new("group-manage-edit")
                        .label(t!("group-edit"))
                        .tooltip(t!("group-edit-tip"))
                        .on_click(move |_, window, cx| {
                            let group = edit_group.clone();
                            edit_view.update(cx, |view, cx| {
                                view.open_group_edit_dialog(&group, window, cx)
                            });
                        }),
                )
                .child({
                    let mut refresh = Button::new("group-manage-refresh")
                        .label(t!("group-manage-refresh"))
                        .tooltip(if has_subs {
                            t!("group-manage-refresh-tip")
                        } else {
                            t!("group-manage-refresh-disabled")
                        });
                    if !has_subs {
                        refresh = refresh.disabled(true);
                    }
                    refresh.on_click(move |_, window, cx| {
                        let group = refresh_group.clone();
                        refresh_view
                            .update(cx, |view, cx| view.update_group_subs(&group, window, cx));
                    })
                });
            let auto_clear_switch = Switch::new("group-manage-auto-clear-switch")
                .checked(auto_clear)
                .label(t!("group-auto-clear"))
                .tooltip(t!("group-auto-clear-tip"))
                .accessibility_label(t!("group-auto-clear-aria", group = group.as_str()))
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
            let mut prunes = div().flex().flex_row().flex_wrap().items_center().gap_2();
            for (index, kind) in GroupPruneKind::ALL.into_iter().enumerate() {
                let prune_view = sheet_view.clone();
                let prune_group = group.clone();
                prunes = prunes.child(
                    Button::new(format!("group-manage-prune-{index}"))
                        .label(prune_label(kind))
                        .on_click(move |_, window, cx| {
                            let group = prune_group.clone();
                            prune_view.update(cx, |view, cx| {
                                view.open_prune_confirm(kind, &group, window, cx)
                            });
                        }),
                );
            }

            let form = v_form()
                .small()
                .child(field().label(group.clone()).child(detail))
                .child(field().label_indent(false).child(actions))
                .child(
                    field().label_indent(false).child(
                        div()
                            .id("group-manage-auto-clear")
                            .test_support()
                            .child(auto_clear_switch),
                    ),
                )
                .child(field().label_indent(false).child(prunes))
                .child(
                    field().label_indent(false).child(
                        Button::new("group-manage-delete")
                            .label(t!("group-delete-action"))
                            .on_click(move |_, window, cx| {
                                let group = delete_group.clone();
                                delete_view.update(cx, |view, cx| {
                                    view.open_group_delete_confirm(&group, window, cx)
                                });
                            }),
                    ),
                );
            sheet
                .title(t!("group-manage-title", group = group.as_str()))
                .child(form)
                .size(px(520.))
                .footer(sheet_footer(
                    "group-manage-ok",
                    None,
                    &footer_commit_label("groups"),
                    commit_click,
                ))
        });
    }
}

pub fn prune_label(kind: GroupPruneKind) -> String {
    match kind {
        GroupPruneKind::Duplicates => t!("prune-duplicates"),
        GroupPruneKind::Unavailable => t!("prune-unavailable"),
        GroupPruneKind::Invalid => t!("prune-invalid"),
        GroupPruneKind::ClearTests => t!("prune-clear-tests"),
    }
}
