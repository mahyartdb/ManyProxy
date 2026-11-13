use crate::*;

pub fn redact_subscription_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);

    let stripped = match without_query.split_once("://") {
        Some((scheme, rest)) => {
            let path_at = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(path_at);
            let host = authority.rsplit('@').next().unwrap_or(authority);
            format!("{scheme}://{host}{path}")
        }
        None => {
            let path_at = without_query.find('/').unwrap_or(without_query.len());
            let (authority, path) = without_query.split_at(path_at);
            let host = authority.rsplit('@').next().unwrap_or(authority);
            format!("{host}{path}")
        }
    };

    truncate(&stripped, 96)
}

pub fn subscription_label_from_url(url: &str) -> String {
    let after = url.split("://").nth(1).unwrap_or(url);
    let host = after.split('/').next().unwrap_or(after);
    let host = host.rsplit('@').next().unwrap_or(host);
    if host.trim().is_empty() {
        t!("sub-label-fallback")
    } else {
        host.to_string()
    }
}

pub fn subscription_member_names(rows: &[ProxyConfig], sub_id: &str) -> Vec<String> {
    rows.iter()
        .filter(|row| row.subscription_id.as_deref() == Some(sub_id))
        .map(|row| row.name.clone())
        .collect()
}

pub fn subscription_row(
    view: Entity<AppView>,
    sub: ProxySubscription,
    count: usize,
    expanded: bool,
    members: Vec<String>,
    refreshing: bool,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(Label::new(sub.label.clone()))
        .child(t!(
            "sub-row-summary",
            url = redact_subscription_url(&sub.url),
            group = sub.group.as_str(),
            count = count,
            state = if sub.auto_refresh {
                t!("sub-auto-state-on")
            } else {
                t!("sub-auto-state-off")
            }
        ))
        .child(
            div()
                .id(format!("sub-meta-{}", sub.id))
                .test_support()
                .child(t!(
                    "sub-meta",
                    last = format_last_refresh_label(sub.last_refresh_at, now_unix()),
                    interval = format_interval_label(sub.interval_secs)
                )),
        );
    if let Some(error) = sub.last_error.as_deref() {
        row = row.child(t!("sub-last-error", error = error));
    }

    let failures = subscription_failures_text(&sub.failures);
    if !failures.is_empty() {
        row = row.child(
            div()
                .id(format!("sub-failures-{}", sub.id))
                .test_support()
                .child(failures),
        );
    }

    let expand_view = view.clone();
    let expand_id = sub.id.clone();
    row = row.child(
        Button::new(format!("sub-expand-{}", sub.id))
            .label(if expanded {
                t!("sub-collapse")
            } else {
                t!("sub-expand")
            })
            .on_click(move |_, window, cx| {
                let sub_id = expand_id.clone();
                expand_view.update(cx, |view, cx| {
                    view.toggle_sub_expanded(&sub_id, window, cx);
                });
            }),
    );
    if expanded {
        let names = if members.is_empty() {
            t!("sub-no-members")
        } else {
            members.join(", ")
        };
        row = row.child(
            div()
                .id(format!("sub-members-{}", sub.id))
                .test_support()
                .child(t!("sub-members", names = names)),
        );
        let copy_view = view.clone();
        let copy_url = sub.url.clone();
        row = row.child(
            Button::new(format!("sub-copy-url-{}", sub.id))
                .label(t!("sub-copy-url"))
                .on_click(move |_, window, cx| {
                    let redacted = redact_subscription_url(&copy_url);
                    cx.write_to_clipboard(ClipboardItem::new_string(redacted));
                    copy_view.update(cx, |_, cx| {
                        window.push_notification(
                            Notification::success(t!("copied-sub-url-redacted")),
                            cx,
                        );
                    });
                }),
        );

        let full_view = view.clone();
        let full_url = sub.url.clone();
        let full_label = sub.label.clone();
        row = row.child(
            Button::new(format!("sub-copy-full-url-{}", sub.id))
                .label(t!("sub-copy-full-url"))
                .on_click(move |_, window, cx| {
                    let url = full_url.clone();
                    let label = full_label.clone();
                    full_view.update(cx, |view, cx| {
                        view.open_sub_full_url_confirm(&label, &url, window, cx);
                    });
                }),
        );
    }
    let refresh_view = view.clone();
    let refresh_id = sub.id.clone();

    let mut refresh_button = Button::new(format!("sub-refresh-{}", sub.id))
        .label(t!("sub-refresh"))
        .loading(refreshing)
        .on_click(move |_, window, cx| {
            let sub_id = refresh_id.clone();
            refresh_view.update(cx, |view, cx| {
                view.refresh_subscription_by_id(&sub_id, window, cx);
            });
        });
    if refreshing {
        refresh_button = refresh_button.child(
            div()
                .id(format!("sub-refresh-spinner-{}", sub.id))
                .test_support()
                .child(Spinner::new().small()),
        );
    }
    row = row.child(refresh_button);

    if refreshing {
        row = row.child(
            div()
                .id(format!("sub-refreshing-{}", sub.id))
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(Skeleton::new().max_w(px(120.)))
                .child(
                    ShimmerText::new(t!("sub-refreshing"))
                        .id(format!("sub-refreshing-text-{}", sub.id)),
                ),
        );
    }
    let auto_view = view.clone();
    let auto_id = sub.id.clone();
    let auto_now = sub.auto_refresh;
    let auto_label = sub.label.clone();
    row = row.child(
        Button::new(format!("sub-auto-{}", sub.id))
            .label(if auto_now {
                t!("sub-auto-button-on")
            } else {
                t!("sub-auto-button-off")
            })
            .tooltip(t!("sub-auto-row-tip", label = auto_label.as_str()))
            .accessibility_label(t!("sub-auto-row-aria", label = auto_label.as_str()))
            .on_click(move |_, window, cx| {
                let sub_id = auto_id.clone();
                auto_view.update(cx, |view, cx| {
                    view.commit_subscription_auto(&sub_id, window, cx);
                });
            }),
    );
    let remove_view = view.clone();
    let edit_view = view.clone();
    let edit_sub = sub.clone();
    row = row.child(
        Button::new(format!("sub-edit-{}", sub.id))
            .label(t!("sub-edit"))
            .on_click(move |_, window, cx| {
                let sub = edit_sub.clone();
                edit_view.update(cx, |view, cx| {
                    view.open_subscription_edit_dialog(sub, window, cx);
                });
            }),
    );
    row.child(
        Button::new(format!("sub-remove-{}", sub.id))
            .label(t!("sub-remove"))
            .on_click(move |_, window, cx| {
                let sub = sub.clone();
                remove_view.update(cx, |view, cx| {
                    view.open_subscription_remove_confirm(sub, window, cx);
                });
            }),
    )
}

pub fn update_target_subs(
    subs: &[ProxySubscription],
    group: &str,
) -> Vec<ProxySubscription> {
    let mut out: Vec<ProxySubscription> = subs
        .iter()
        .filter(|sub| sub.group == group)
        .cloned()
        .collect();
    out.sort_by_key(|sub| sub.id.parse::<i64>().unwrap_or(i64::MAX));
    out
}

pub fn update_all_targets(
    subs: Vec<ProxySubscription>,
    flagged: &HashSet<String>,
) -> (Vec<ProxySubscription>, Vec<ProxySubscription>) {
    let mut included = Vec::new();
    let mut skipped = Vec::new();
    for sub in subs {
        if flagged.contains(sub.group.as_str()) {
            skipped.push(sub);
        } else {
            included.push(sub);
        }
    }
    (included, skipped)
}

pub fn rule_set_row(
    view: Entity<AppView>,
    row: store::RuleSetRow,
    used_by: Vec<String>,
) -> impl IntoElement {
    let redownload_view = view.clone();
    let redownload_row = row.clone();
    let remove_view = view.clone();
    let remove_id = row.id.clone();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(format!("{} · {}", row.name, row.file_name)))
        .child(Label::new(rule_set_summary(&row, &used_by)))
        .child(
            Button::new(format!("rule-set-redownload-{}", row.id))
                .label(t!("rule-set-redownload"))
                .tooltip(t!("rule-set-redownload-tip"))
                .on_click(move |_, window, cx| {
                    let row = redownload_row.clone();
                    redownload_view.update(cx, |view, cx| {
                        view.open_rule_set_download_confirm(
                            RuleSetDownloadGrant {
                                name: row.name.clone(),
                                file_name: row.file_name.clone(),
                                url: row.url.clone(),
                                dir: crate::store::rule_sets_dir(),
                            },
                            window,
                            cx,
                        );
                    });
                }),
        )
        .child(
            Button::new(format!("rule-set-remove-{}", row.id))
                .label(t!("sub-remove"))
                .tooltip(t!("rule-set-remove-tip"))
                .on_click(move |_, window, cx| {
                    let id = remove_id.clone();
                    remove_view.update(cx, |view, cx| {
                        view.commit_rule_set_remove(&id, window, cx);
                    });
                }),
        )
}
