use crate::*;

pub(crate) fn preset_row(
    view: Entity<AppView>,
    preset: store::RoutePreset,
    attached: Vec<String>,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(preset.name.clone()))
        .child(preset_row_summary(&preset, &attached));
    if preset.locked {
        row = row.child(Label::new("Locked"));
    }
    if preset.source == "url" && preset.source_url.is_some() {
        let refresh_view = view.clone();
        let refresh_id = preset.id.clone();
        row = row.child(
            Button::new(format!("preset-refresh-{}", preset.id))
                .label("Refresh")
                .on_click(move |_, window, cx| {
                    let id = refresh_id.clone();
                    refresh_view.update(cx, |view, cx| {
                        view.refresh_preset_by_id(&id, window, cx);
                    });
                }),
        );
    }
    let remove_view = view.clone();
    let remove_id = preset.id.clone();
    let edit_view = view.clone();
    let edit_preset = preset.clone();
    let lock_view = view.clone();
    let lock_id = preset.id.clone();
    let lock_next = !preset.locked;
    let export_view = view.clone();
    let export_preset = preset.clone();
    row = row.child(
        Button::new(format!("preset-lock-{}", preset.id))
            .label(if preset.locked { "Unlock" } else { "Lock" })
            .tooltip(if preset.locked {
                "Allow modifications again (the rules gain an editor)"
            } else {
                "Prevent modifications: the rules stay byte-stable; attach/detach unaffected"
            })
            .on_click(move |_, window, cx| {
                let id = lock_id.clone();
                let locked = lock_next;
                lock_view.update(cx, |view, cx| {
                    view.commit_preset_lock(&id, locked, window, cx);
                });
            }),
    );
    row = row.child(
        Button::new(format!("preset-export-{}", preset.id))
            .label("Export link")
            .tooltip("Copy this preset's throne://route share link")
            .on_click(move |_, window, cx| {
                let preset = export_preset.clone();
                export_view.update(cx, |view, cx| {
                    view.commit_preset_export_link(&preset, window, cx);
                });
            }),
    );
    row = row.child(
        Button::new(format!("preset-edit-{}", preset.id))
            .label("Edit")
            .on_click(move |_, window, cx| {
                let preset = edit_preset.clone();
                edit_view.update(cx, |view, cx| {
                    view.open_preset_edit_dialog(preset, window, cx);
                });
            }),
    );
    let duplicate_view = view.clone();
    let duplicate_id = preset.id.clone();
    row = row.child(
        Button::new(format!("preset-duplicate-{}", preset.id))
            .label("Duplicate")
            .on_click(move |_, window, cx| {
                let id = duplicate_id.clone();
                duplicate_view.update(cx, |view, cx| {
                    view.commit_preset_duplicate(&id, window, cx);
                });
            }),
    );
    row.child(
        Button::new(format!("preset-remove-{}", preset.id))
            .label("Remove")
            .on_click(move |_, window, cx| {
                let id = remove_id.clone();
                remove_view.update(cx, |view, cx| {
                    view.commit_preset_remove(&id, window, cx);
                });
            }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresetRulesOp {
    CycleAction(usize),
    Remove(usize),
    Add,
}

/// T-09 attribute fold keys, rendered per rule as JSON-shape fields. The
/// rules JSON box stays the single editing surface; no per-key input state.
pub(crate) const PRESET_FOLD_KEYS: &[(&str, &str)] = &[
    ("source_ip_cidr", "Source CIDR"),
    ("source_ip_is_private", "Source is private"),
    ("source_port", "Source port"),
    ("source_port_range", "Source port range"),
    ("ip_version", "IP version"),
    ("invert", "Invert"),
    ("process_path_regex", "Process path regex"),
];

pub(crate) fn preset_rule_attribute_folds(
    preset_id: &str,
    index: usize,
    rule: &serde_json::Value,
) -> Vec<AnyElement> {
    PRESET_FOLD_KEYS
        .iter()
        .filter_map(|(key, label)| {
            let value = rule.get(*key)?;
            Some(
                div()
                    .id(format!("preset-rule-fold-{preset_id}-{index}-{key}"))
                    .test_support()
                    .text_sm()
                    .child(format!(
                        "{label}: {}",
                        serde_json::to_string(value).unwrap_or_default()
                    ))
                    .into_any_element(),
            )
        })
        .collect()
}

pub(crate) fn preset_advanced_section(
    view: Entity<AppView>,
    preset: &store::RoutePreset,
    form: Option<PresetForm>,
    exits: &[String],
) -> impl IntoElement {
    let mut section = div().flex().flex_col().gap_1().child(
        div()
            .id(format!("preset-advanced-{}", preset.id))
            .test_support()
            .text_sm()
            .child("Advanced"),
    );
    let rules = match preset.rules() {
        Ok(rules) => rules,
        Err(message) => {
            return section.child(
                div()
                    .id(format!("preset-advanced-invalid-{}", preset.id))
                    .test_support()
                    .text_sm()
                    .child(message),
            );
        }
    };
    let locked = preset.locked;
    for (index, rule) in rules.iter().enumerate() {
        let action = rule
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let tabs = crate::preset::rule_attribute_tabs(rule).join(" · ");
        let name = rule
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.trim().is_empty());
        let label = match name {
            Some(name) => format!("Rule {index} — \"{name}\" — {action} · {tabs}"),
            None => format!("Rule {index} — {action} · {tabs}"),
        };
        let mut row = div().flex().flex_row().items_center().gap_2().child(
            div()
                .id(format!("preset-rule-name-{}-{index}", preset.id))
                .test_support()
                .text_sm()
                .child(label),
        );
        if crate::preset::action_carries_outbound(action) {
            let outbound_text = match rule.get("outbound").and_then(serde_json::Value::as_str) {
                Some(tag) => format!("Outbound: {tag}"),
                None => "Outbound: serving row".to_string(),
            };
            row = row.child(
                div()
                    .id(format!("preset-rule-outbound-{}-{index}", preset.id))
                    .test_support()
                    .text_sm()
                    .child(outbound_text),
            );
            if !locked {
                row = row.child(
                    div()
                        .id(format!("preset-rule-exits-{}-{index}", preset.id))
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "Exits: {}",
                            crate::preset::preset_exit_suggestions(exits).join(" · ")
                        )),
                );
            }
        }
        if let Some(form) = &form {
            let cycle_view = view.clone();
            let cycle_form = form.clone();
            let remove_view = view.clone();
            let remove_form = form.clone();
            row = row
                .child(
                    Button::new(format!("preset-rule-action-{}-{index}", preset.id))
                        .small()
                        .label(format!("Action: {action}"))
                        .tooltip("Cycle this rule's action (direct → proxy → block)")
                        .on_click(move |_, window, cx| {
                            cycle_view.update(cx, |_, cx| {
                                AppView::rewrite_preset_rules_box(
                                    &cycle_form,
                                    &PresetRulesOp::CycleAction(index),
                                    window,
                                    cx,
                                );
                            });
                        }),
                )
                .child(
                    Button::new(format!("preset-rule-delete-{}-{index}", preset.id))
                        .small()
                        .label("Delete rule")
                        .tooltip("Remove this rule from the rules box")
                        .on_click(move |_, window, cx| {
                            remove_view.update(cx, |_, cx| {
                                AppView::rewrite_preset_rules_box(
                                    &remove_form,
                                    &PresetRulesOp::Remove(index),
                                    window,
                                    cx,
                                );
                            });
                        }),
                );
        }
        section = section.child(row);
        for fold in preset_rule_attribute_folds(&preset.id, index, rule) {
            section = section.child(fold);
        }
    }
    match form {
        Some(form) => {
            let add_view = view.clone();
            section.child(
                Button::new(format!("preset-rule-add-{}", preset.id))
                    .small()
                    .label("Add rule")
                    .tooltip("Append a fresh direct rule to the rules box")
                    .on_click(move |_, window, cx| {
                        add_view.update(cx, |_, cx| {
                            AppView::rewrite_preset_rules_box(
                                &form,
                                &PresetRulesOp::Add,
                                window,
                                cx,
                            );
                        });
                    }),
            )
        }
        None => section,
    }
}
