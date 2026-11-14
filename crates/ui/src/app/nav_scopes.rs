//! The nav's scope sections and the source strip.
//!
//! The nav owns the scope index: every group and source entry is built from
//! the one snapshot `refresh_scope_index` takes on the row-load path. The
//! render path only reads that snapshot.

use crate::*;
use std::collections::BTreeSet;

/// The payload a row drag carries. Only the dragged row moves; a selection is
/// never dragged along.
#[derive(Clone)]
pub struct RowDrag {
    pub id: String,
}

/// The chip under the pointer while a row is dragged. It names the row so the
/// drop result is obvious before release.
pub struct RowDragPreview {
    pub label: String,
}

impl Render for RowDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(self.label.clone())
    }
}

/// What a drag can land on. `All` has no group to move a row into; it refuses
/// with copy instead of inventing a "no group" state.
#[derive(Clone)]
pub enum RowDrop {
    All,
    Group(String),
}

pub struct ScopeEntrySpec {
    pub id: String,
    pub scope: LibraryScope,
    pub letter: String,
    pub label: String,
    pub tip: String,
    pub count: Option<usize>,
    pub dot: Option<SourceStatus>,
    pub drop: RowDrop,
    pub context_group: Option<String>,
}

impl AppView {
    /// Rebuild the group, source and scope-count snapshot. This is the only
    /// writer of the nav's counts; a repaint never touches the store or walks
    /// the library. A scope whose bucket disappeared falls back to `All`.
    pub fn refresh_scope_index(&mut self, cx: &mut Context<Self>) {
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let now = now_unix();
        let (counts, groups, sources) = {
            let libraries = self.libraries.read(cx);
            let delegate = libraries.delegate();
            let mut group_totals: HashMap<String, usize> = HashMap::new();
            let mut group_subs: HashMap<String, usize> = HashMap::new();
            let mut source_totals: HashMap<String, usize> = HashMap::new();
            let mut source_running: HashMap<String, usize> = HashMap::new();
            let mut source_traffic: HashMap<String, (u64, u64)> = HashMap::new();
            for sub in &subs {
                *group_subs.entry(sub.group.clone()).or_default() += 1;
            }
            for row in &delegate.rows {
                *group_totals.entry(row.group.clone()).or_default() += 1;
                let Some(sub_id) = row.subscription_id.as_deref() else {
                    continue;
                };
                *source_totals.entry(sub_id.to_string()).or_default() += 1;
                if delegate.running.contains(&row.id) {
                    *source_running.entry(sub_id.to_string()).or_default() += 1;
                }
                if let Some((rx, tx)) = delegate.traffic.get(&row.id) {
                    let slot = source_traffic.entry(sub_id.to_string()).or_insert((0, 0));
                    slot.0 = slot.0.saturating_add(*rx);
                    slot.1 = slot.1.saturating_add(*tx);
                }
            }

            let mut names: BTreeSet<String> = group_totals.keys().cloned().collect();
            names.extend(group_subs.keys().cloned());
            names.extend(self.created_groups.iter().cloned());
            names.remove(DEFAULT_GROUP);
            let mut groups: Vec<GroupEntry> = names
                .into_iter()
                .map(|name| GroupEntry {
                    count: group_totals.get(&name).copied().unwrap_or(0),
                    sub_count: group_subs.get(&name).copied().unwrap_or(0),
                    scope: LibraryScope::Group(name.clone()),
                    name,
                    ungrouped: false,
                })
                .collect();
            let default_count = group_totals.get(DEFAULT_GROUP).copied().unwrap_or(0);
            if default_count > 0 {
                groups.push(GroupEntry {
                    name: t!("nav-ungrouped"),
                    scope: LibraryScope::Ungrouped,
                    count: default_count,
                    sub_count: group_subs.get(DEFAULT_GROUP).copied().unwrap_or(0),
                    ungrouped: true,
                });
            }

            let sources: Vec<SourceEntry> = subs
                .iter()
                .map(|sub| SourceEntry {
                    id: sub.id.clone(),
                    label: sub.label.clone(),
                    group: sub.group.clone(),
                    count: source_totals.get(&sub.id).copied().unwrap_or(0),
                    running: source_running.get(&sub.id).copied().unwrap_or(0),
                    health: source_health(sub, now),
                    quota: source_quota_line(sub),
                    traffic: source_traffic
                        .get(&sub.id)
                        .copied()
                        .filter(|(rx, tx)| *rx > 0 || *tx > 0),
                })
                .collect();

            let scopes: Vec<LibraryScope> = std::iter::once(LibraryScope::All)
                .chain(groups.iter().map(|group| group.scope.clone()))
                .chain(
                    sources
                        .iter()
                        .map(|source| LibraryScope::Source(source.id.clone())),
                )
                .collect();
            let counts = scope_counts(&delegate.rows, &scopes);
            (counts, groups, sources)
        };

        let scope_lives = match &self.scope {
            LibraryScope::All => true,
            LibraryScope::Group(name) => groups.iter().any(
                |group| matches!(&group.scope, LibraryScope::Group(candidate) if candidate == name),
            ),
            LibraryScope::Ungrouped => groups.iter().any(|group| group.ungrouped),
            LibraryScope::Source(id) => sources.iter().any(|source| &source.id == id),
        };
        if !scope_lives {
            self.scope = LibraryScope::All;
            self.reset_selection(cx);
        }
        let enabled = {
            let delegate = self.libraries.read(cx).delegate();
            delegate
                .rows
                .iter()
                .filter(|row| self.scope.contains(row) && row.enabled)
                .count()
        };
        self.scope_counts = counts;
        self.group_list = groups;
        self.source_list = sources;
        self.scope_enabled = enabled;
    }

    pub fn scope_count(&self, scope: &LibraryScope) -> usize {
        self.scope_counts
            .iter()
            .find(|(candidate, _)| candidate == scope)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    }

    /// The action bar's name for the current scope.
    pub fn scope_label(&self) -> String {
        match &self.scope {
            LibraryScope::All => t!("nav-all-configs"),
            LibraryScope::Group(name) => name.clone(),
            LibraryScope::Ungrouped => t!("nav-ungrouped"),
            LibraryScope::Source(id) => self
                .source_list
                .iter()
                .find(|source| &source.id == id)
                .map(|source| source.label.clone())
                .unwrap_or_else(|| t!("nav-sources")),
        }
    }

    /// A scope change is a navigation move. It clears the selection, the
    /// anchor and the focus, then recomputes the table inside the new scope;
    /// the query (search, status, protocol) stays exactly as it was.
    pub fn set_scope(&mut self, scope: LibraryScope, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.row_drop_target = None;
        self.clear_selection_for_scope_change(window, cx);
        self.apply_query(cx);
        self.refresh_scope_index(cx);
        cx.notify();
    }

    pub fn hover_row_drop(&mut self, group: &str, cx: &mut Context<Self>) {
        if self.row_drop_target.as_deref() != Some(group) {
            self.row_drop_target = Some(group.to_string());
            cx.notify();
        }
    }

    pub fn clear_row_drop(&mut self, cx: &mut Context<Self>) {
        if self.row_drop_target.take().is_some() {
            cx.notify();
        }
    }

    pub fn group_drop_label(group: &str) -> String {
        if group == DEFAULT_GROUP {
            t!("nav-ungrouped")
        } else {
            group.to_string()
        }
    }

    pub fn source_tone(&self, status: SourceStatus, cx: &App) -> Hsla {
        match status {
            SourceStatus::Fresh => cx.theme().success,
            SourceStatus::Stale | SourceStatus::Never => cx.theme().warning,
            SourceStatus::Failed => cx.theme().danger,
        }
    }

    pub fn source_status_label(status: SourceStatus) -> String {
        match status {
            SourceStatus::Fresh => t!("source-strip-fresh"),
            SourceStatus::Stale => t!("source-strip-stale"),
            SourceStatus::Never => t!("source-strip-never"),
            SourceStatus::Failed => t!("source-strip-failed"),
        }
    }

    pub fn nav_dot(&self, status: SourceStatus, cx: &App) -> AnyElement {
        div()
            .flex_shrink_0()
            .size(px(6.))
            .rounded_full()
            .bg(self.source_tone(status, cx))
            .into_any_element()
    }

    /// `All configs`, pinned above the scrolling sections. It is the one
    /// scope that is also a drop target that refuses: there is no ungrouped
    /// state to move a row into.
    pub fn render_all_configs_entry(
        &self,
        total: usize,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_scope_entry(
            ScopeEntrySpec {
                id: "nav-all-configs".to_string(),
                scope: LibraryScope::All,
                letter: "A".to_string(),
                label: t!("nav-all-configs"),
                tip: t!("nav-all-tip", count = total),
                count: Some(total),
                dot: None,
                drop: RowDrop::All,
                context_group: None,
            },
            collapsed,
            cx,
        )
    }

    /// The `Groups` section. The header carries the `+`; the collapsed nav
    /// keeps the same action as its own hairline row.
    pub fn render_group_section(&self, collapsed: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut section = div().id("nav-groups-section").flex().flex_col();
        if collapsed {
            section = section
                .child(section_hairline("nav-groups-hairline", cx))
                .child(self.new_group_row("+", true, cx));
        } else {
            section = section.child(
                div()
                    .id("nav-groups-header")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h(px(24.))
                    .px_2()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!("nav-groups")),
                    )
                    .child(self.new_group_row("+", false, cx)),
            );
        }

        for group in &self.group_list {
            let tip = if group.ungrouped {
                t!("nav-ungrouped-tip", group = DEFAULT_GROUP)
            } else {
                t!(
                    "nav-group-tip",
                    count = group.count,
                    name = group.name.as_str()
                )
            };
            section = section.child(self.render_scope_entry(
                ScopeEntrySpec {
                    id: if group.ungrouped {
                        "nav-ungrouped".to_string()
                    } else {
                        format!("nav-group-{}", slugify_group_id(&group.name))
                    },
                    scope: group.scope.clone(),
                    letter: group_letter(&group.name),
                    label: group.name.clone(),
                    tip,
                    count: Some(group.count),
                    dot: None,
                    drop: RowDrop::Group(if group.ungrouped {
                        DEFAULT_GROUP.to_string()
                    } else {
                        group.name.clone()
                    }),
                    context_group: if group.ungrouped {
                        // `Ungrouped` is the store's fallback bucket, not a
                        // user group: renaming it would invent a named group
                        // and deleting it can only refuse while it is visible.
                        None
                    } else {
                        Some(group.name.clone())
                    },
                },
                collapsed,
                cx,
            ));
        }
        if !collapsed && self.group_list.is_empty() {
            section = section.child(
                div()
                    .id("nav-groups-empty")
                    .test_support()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("groups-empty")),
            );
        }
        section.into_any_element()
    }

    pub fn new_group_row(
        &self,
        label: &'static str,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut button = Button::new("nav-new-group")
            .label(label)
            .accessibility_label(t!("nav-new-group"))
            .tooltip(t!("nav-new-group-tip"))
            .on_click(move |_, window, cx| {
                view.update(cx, |view, cx| view.open_new_group_dialog(window, cx));
            });
        if compact {
            button = button.compact().ghost();
        }
        div()
            .id("nav-new-group-row")
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .h(px(24.))
            .child(button)
            .into_any_element()
    }

    /// The `Sources` section. `Manage…` opens the existing sources sheet; each
    /// entry carries its freshness dot, count and one-source refresh.
    pub fn render_source_section(&self, collapsed: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut section = div().id("nav-sources-section").flex().flex_col();
        if collapsed {
            section = section
                .child(section_hairline("nav-sources-hairline", cx))
                .child(self.manage_sources_row("⋯", true, cx));
        } else {
            section = section.child(
                div()
                    .id("nav-sources-header")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h(px(24.))
                    .px_2()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!("nav-sources")),
                    )
                    .child(self.manage_sources_row(t!("nav-manage"), false, cx)),
            );
        }

        for source in &self.source_list {
            section = section.child(self.render_source_entry(source, collapsed, cx));
        }
        section.into_any_element()
    }

    pub fn manage_sources_row(
        &self,
        label: impl Into<SharedString>,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut button = Button::new("nav-manage-sources")
            .label(label)
            .accessibility_label(t!("nav-manage-sources"))
            .tooltip(t!("nav-manage-sources"))
            .on_click(move |_, window, cx| {
                view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
            });
        if compact {
            button = button.compact().ghost();
        }
        div()
            .id("nav-manage-sources-row")
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .h(px(24.))
            .child(button)
            .into_any_element()
    }

    pub fn render_source_entry(
        &self,
        source: &SourceEntry,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry_id = format!("nav-source-{}", source.id);
        let active = self.scope == LibraryScope::Source(source.id.clone());
        let tip = t!(
            "nav-source-tip",
            count = source.count,
            name = source.label.as_str(),
            group = Self::group_drop_label(&source.group)
        );
        let accent = cx.theme().accent;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().list_hover;
        let dot = self.nav_dot(source.health.status, cx);
        let scope = LibraryScope::Source(source.id.clone());
        let view = cx.entity();
        let refresh_view = cx.entity();
        let refresh_id = source.id.clone();

        let mut row = div()
            .id(entry_id.clone())
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(super::nav::NAV_ENTRY_HEIGHT))
            .px_2()
            .border_l_2()
            .border_color(if active {
                accent
            } else {
                cx.theme().transparent
            })
            .bg(if active {
                cx.theme().list_active
            } else {
                cx.theme().transparent
            })
            .text_color(if active { foreground } else { muted })
            .role(Role::Button)
            .aria_label(source.label.clone())
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
            .on_click(move |_, window, cx| {
                let scope = scope.clone();
                view.update(cx, |view, cx| view.set_scope(scope, window, cx));
            })
            .hover(move |style| style.bg(hover).text_color(foreground));

        if collapsed {
            row = row.child(
                div()
                    .id(format!("{entry_id}-dot"))
                    .test_support()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .child(dot),
            );
        } else {
            row = row
                .child(dot)
                .child(
                    div()
                        .id(format!("{entry_id}-label"))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(source.label.clone()),
                )
                .child(
                    div()
                        .id(format!("{entry_id}-count"))
                        .test_support()
                        .flex_shrink_0()
                        .text_sm()
                        .text_color(muted)
                        .aria_label(source.count.to_string())
                        .child(source.count.to_string()),
                )
                .child(
                    Button::new(format!("nav-source-refresh-{}", source.id))
                        .compact()
                        .ghost()
                        .label("↻")
                        .accessibility_label(t!("nav-refresh-source", name = source.label.as_str()))
                        .tooltip(t!("nav-refresh-source", name = source.label.as_str()))
                        .on_click(move |_, window, cx| {
                            let id = refresh_id.clone();
                            refresh_view.update(cx, |view, cx| {
                                view.refresh_subscription_by_id(&id, window, cx);
                            });
                        }),
                );
        }
        row.into_any_element()
    }

    /// One nav scope entry: the label or its letter, the count, the active rim
    /// and fill, the tooltip, and the row drop target for a group.
    pub fn render_scope_entry(
        &self,
        spec: ScopeEntrySpec,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ScopeEntrySpec {
            id,
            scope,
            letter,
            label,
            tip,
            count,
            dot,
            drop,
            context_group,
        } = spec;
        let active = self.scope == scope;
        let drop_active = match &drop {
            RowDrop::Group(group) => self.row_drop_target.as_deref() == Some(group.as_str()),
            _ => false,
        };
        let accent = cx.theme().accent;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().list_hover;
        let view = cx.entity();
        let click_scope = scope.clone();

        let mut row = div()
            .id(id.clone())
            .test_support()
            .aria_label(label.clone())
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(super::nav::NAV_ENTRY_HEIGHT))
            .px_2()
            .border_l_2()
            .border_color(if active || drop_active {
                accent
            } else {
                cx.theme().transparent
            })
            .bg(if active {
                cx.theme().list_active
            } else if drop_active {
                cx.theme().list_active_border
            } else {
                cx.theme().transparent
            })
            .text_color(if active { foreground } else { muted })
            .role(Role::Button)
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
            .on_click(move |_, window, cx| {
                let scope = click_scope.clone();
                view.update(cx, |view, cx| view.set_scope(scope, window, cx));
            })
            .hover(move |style| style.bg(hover).text_color(foreground));

        if collapsed {
            row = row.child(
                div()
                    .id(format!("{id}-letter"))
                    .test_support()
                    .flex_1()
                    .text_center()
                    .child(letter),
            );
        } else {
            if let Some(status) = dot {
                row = row.child(self.nav_dot(status, cx));
            }
            row = row.child(
                div()
                    .id(format!("{id}-label"))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(label.clone()),
            );
        }

        if !collapsed {
            if let Some(count) = count {
                row = row.child(
                    div()
                        .id(format!("{id}-count"))
                        .test_support()
                        .flex_shrink_0()
                        .text_sm()
                        .text_color(muted)
                        .aria_label(count.to_string())
                        .child(count.to_string()),
                );
            }
        }

        match drop {
            RowDrop::Group(group) => {
                let hover_group = group.clone();
                let drop_group = group.clone();
                row = row
                    .on_drag_move(cx.listener(
                        move |view, event: &DragMoveEvent<RowDrag>, _, cx| {
                            if event.bounds.contains(&event.event.position) {
                                view.hover_row_drop(&hover_group, cx);
                            } else if view.row_drop_target.as_deref() == Some(hover_group.as_str())
                            {
                                view.clear_row_drop(cx);
                            }
                        },
                    ))
                    .on_drop(cx.listener(move |view, drag: &RowDrag, window, cx| {
                        view.drop_row_on_group(&drag.id, Some(&drop_group), window, cx);
                    }));
            }
            RowDrop::All => {
                row = row.on_drop(cx.listener(|view, drag: &RowDrag, window, cx| {
                    view.drop_row_on_group(&drag.id, None, window, cx);
                }));
            }
        }

        if let Some(group) = context_group {
            let menu_view = cx.entity();
            let menu_group = group.clone();
            return row
                .context_menu(move |menu, _, _| {
                    AppView::group_context_menu(menu, &menu_group, &menu_view)
                })
                .into_any_element();
        }

        row.into_any_element()
    }

    pub fn group_context_menu(menu: PopupMenu, group: &str, view: &Entity<AppView>) -> PopupMenu {
        let slug = slugify_group_id(group);
        let manage_slug = slug.clone();
        let manage_view = view.clone();
        let manage_group = group.to_string();
        let delete_view = view.clone();
        let delete_group = group.to_string();
        menu.item(
            PopupMenuItem::element(move |_, _| {
                div()
                    .id(format!("nav-group-manage-{manage_slug}"))
                    .test_support()
                    .role(Role::MenuItem)
                    .child(t!("group-manage-action"))
            })
            .on_click(move |_, window, cx| {
                let group = manage_group.clone();
                manage_view.update(cx, |view, cx| {
                    view.open_group_manage_sheet(&group, window, cx)
                });
            }),
        )
        .item(
            PopupMenuItem::element(move |_, _| {
                div()
                    .id(format!("nav-group-delete-{slug}"))
                    .test_support()
                    .role(Role::MenuItem)
                    .child(t!("group-delete-action"))
            })
            .on_click(move |_, window, cx| {
                let group = delete_group.clone();
                delete_view.update(cx, |view, cx| {
                    view.open_group_delete_confirm(&group, window, cx)
                });
            }),
        )
    }

    /// The source strip, inside the main column above the filter row. `None`
    /// outside a source scope.
    pub fn render_scope_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let LibraryScope::Source(id) = &self.scope else {
            return None;
        };
        let source = self.source_list.iter().find(|source| &source.id == id)?;
        let status = Self::source_status_label(source.health.status);
        let status_color = self.source_tone(source.health.status, cx);
        let now = now_unix();
        let updated = source
            .health
            .last_refresh_at
            .map(|last| {
                t!(
                    "source-strip-updated",
                    when = format_last_refresh_label(Some(last), now)
                )
            })
            .unwrap_or_default();
        let configs = t!("source-strip-configs", count = source.count);
        let running = t!("source-strip-running", count = source.running);
        let quota = source
            .quota
            .and_then(|quota| source_quota_text(quota, now))
            .unwrap_or_default();
        let traffic = source
            .traffic
            .and_then(|(rx, tx)| traffic_summary_text(rx, tx))
            .unwrap_or_default();
        let view = cx.entity();
        let all_view = cx.entity();
        let manage_view = cx.entity();
        let refresh_id = source.id.clone();
        let muted = cx.theme().muted_foreground;

        Some(
            div()
                .id("scope-strip")
                .test_support()
                .flex()
                .flex_row()
                .flex_nowrap()
                .items_center()
                .gap_2()
                .w_full()
                .h(px(32.))
                .flex_shrink_0()
                .px_3()
                .border_b_1()
                .border_color(cx.theme().border)
                .text_sm()
                .child(
                    div()
                        .id("scope-strip-name")
                        .test_support()
                        .flex_shrink_0()
                        .max_w(px(220.))
                        .truncate()
                        .text_color(cx.theme().foreground)
                        .aria_label(source.label.clone())
                        .child(source.label.clone()),
                )
                .child(
                    div()
                        .id("scope-strip-status")
                        .test_support()
                        .flex_shrink_0()
                        .text_color(status_color)
                        .aria_label(status.clone())
                        .child(status),
                )
                .child(
                    div()
                        .id("scope-strip-configs")
                        .test_support()
                        .flex_shrink_0()
                        .text_color(muted)
                        .aria_label(configs.clone())
                        .child(configs),
                )
                .child(
                    div()
                        .id("scope-strip-running")
                        .test_support()
                        .flex_shrink_0()
                        .text_color(muted)
                        .aria_label(running.clone())
                        .child(running),
                )
                .children((!updated.is_empty()).then(|| {
                    div()
                        .id("scope-strip-updated")
                        .test_support()
                        .min_w_0()
                        .flex_shrink(1.)
                        .truncate()
                        .text_color(muted)
                        .aria_label(updated.clone())
                        .child(updated)
                }))
                .children((!quota.is_empty()).then(|| {
                    div()
                        .id("scope-strip-quota")
                        .test_support()
                        .min_w_0()
                        .flex_shrink(6.)
                        .truncate()
                        .text_color(muted)
                        .aria_label(quota.clone())
                        .child(quota)
                }))
                .children((!traffic.is_empty()).then(|| {
                    div()
                        .id("scope-strip-traffic")
                        .test_support()
                        .min_w_0()
                        .flex_shrink(3.)
                        .truncate()
                        .text_color(muted)
                        .aria_label(traffic.clone())
                        .child(traffic)
                }))
                .child(div().flex_1())
                .child(
                    Button::new("scope-strip-refresh")
                        .small()
                        .label(t!("source-strip-refresh"))
                        .on_click(move |_, window, cx| {
                            let id = refresh_id.clone();
                            view.update(cx, |view, cx| {
                                view.refresh_subscription_by_id(&id, window, cx);
                            });
                        }),
                )
                .child(
                    Button::new("scope-strip-manage")
                        .small()
                        .label(t!("source-strip-manage"))
                        .accessibility_label(t!("nav-manage-sources"))
                        .on_click(move |_, window, cx| {
                            manage_view.update(cx, |view, cx| {
                                view.open_subscriptions_dialog(window, cx);
                            });
                        }),
                )
                .child(
                    Button::new("scope-strip-all")
                        .small()
                        .ghost()
                        .label(t!("source-strip-all"))
                        .on_click(move |_, window, cx| {
                            all_view.update(cx, |view, cx| {
                                view.set_scope(LibraryScope::All, window, cx)
                            });
                        }),
                )
                .into_any_element(),
        )
    }

    /// A row dropped on a group entry moves only that row. `None` is the
    /// `All configs` entry: the store has no "no group" state to move into.
    pub fn drop_row_on_group(
        &mut self,
        row_id: &str,
        group: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_row_drop(cx);
        let Some(config) = self.row_config(row_id) else {
            window.push_notification(Notification::warning(t!("already-removed")), cx);
            return;
        };
        let Some(group) = group else {
            window.push_notification(Notification::info(t!("group-drop-all")), cx);
            return;
        };
        let target = Self::group_drop_label(group);
        if config.group == group {
            window.push_notification(
                Notification::info(t!(
                    "group-already-in",
                    name = config.name.as_str(),
                    group = target
                )),
                cx,
            );
            return;
        }
        match self
            .store
            .move_to_group(std::slice::from_ref(&config.id), group)
        {
            Ok(_) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(t!(
                        "group-moved",
                        name = config.name.as_str(),
                        group = target
                    )),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("cannot-move", error = error.to_string())),
                    cx,
                );
            }
        }
    }
}

pub fn group_letter(name: &str) -> String {
    name.chars()
        .next()
        .map(|letter| letter.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

pub fn section_hairline(id: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .h(px(1.))
        .w_full()
        .bg(cx.theme().sidebar_border)
        .into_any_element()
}

pub fn source_quota_text(quota: SourceQuotaLine, now: i64) -> Option<String> {
    let mut segments = Vec::new();
    match (quota.used, quota.total) {
        (Some(used), Some(total)) => segments.push(t!(
            "source-strip-quota",
            used = format_bytes(used.max(0) as u64),
            total = format_bytes(total.max(0) as u64)
        )),
        (Some(used), None) => segments.push(t!(
            "source-strip-quota-used",
            used = format_bytes(used.max(0) as u64)
        )),
        (None, Some(total)) => segments.push(t!(
            "source-strip-quota-total",
            total = format_bytes(total.max(0) as u64)
        )),
        (None, None) => {}
    }
    if let Some(expire_at) = quota.expire_at {
        segments.push(t!(
            "source-strip-expires",
            when = format_expire_label(expire_at, now)
        ));
    }
    (!segments.is_empty()).then(|| segments.join(" · "))
}
