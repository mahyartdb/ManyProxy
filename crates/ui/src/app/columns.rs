use crate::*;

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::Disableable as _;

/// R17: fit measures the name style at its default 14px, independent of the
/// current text scale and of virtualization — it reads the delegate's rows,
/// not the painted viewport.
pub const FIT_TEXT_PX: f32 = 14.;

/// The breathing room a fitted column keeps beyond its widest text: cell
/// padding plus the header's sort affordance.
pub const FIT_CHROME: f32 = 24.;

impl AppView {
    pub fn begin_column_resize(
        &mut self,
        column: ColumnId,
        start_x: f32,
        start_width: f32,
        cx: &mut Context<Self>,
    ) {
        self.column_resize = Some(ColumnResize {
            column,
            start_x,
            start_width,
        });
        cx.notify();
    }

    /// Live drag: the app's layout updates every move so the row and the header
    /// reflow together; the store waits for mouse-up. A Name with no explicit
    /// width is handed its current effective width, which is the moment it
    /// stops flexing.
    pub fn drag_column_resize(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(resize) = self.column_resize else {
            return;
        };
        let width = resize.start_width + (x - resize.start_x);
        self.column_layout.set_width(resize.column, width);
        self.notify_table(cx);
    }

    pub fn finish_column_resize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.column_resize.take().is_none() {
            return;
        }
        let layout = self.column_layout;
        self.persist_column_layout(layout, window, cx);
    }

    pub fn reset_column_width(
        &mut self,
        column: ColumnId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut layout = self.column_layout;
        layout.reset_width(column);
        self.commit_column_layout(layout, window, cx);
        window.push_notification(
            Notification::info(t!("column-restored", column = library_column_label(column))),
            cx,
        );
    }

    /// The four reset verbs the columns sheet exposes. Each changes only the
    /// dimension it names; `reset_all_columns` restores every default.
    pub fn reset_column_widths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut layout = self.column_layout;
        layout.reset_widths();
        self.commit_column_layout(layout, window, cx);
        window.push_notification(Notification::info(t!("columns-reset")), cx);
    }

    pub fn reset_column_order(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut layout = self.column_layout;
        layout.reset_order();
        self.commit_column_layout(layout, window, cx);
        window.push_notification(Notification::info(t!("columns-reset")), cx);
    }

    pub fn reset_column_visibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut layout = self.column_layout;
        layout.reset_visibility();
        self.commit_column_layout(layout, window, cx);
        window.push_notification(Notification::info(t!("columns-reset")), cx);
    }

    pub fn reset_all_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut layout = self.column_layout;
        layout.reset();
        self.commit_column_layout(layout, window, cx);
        window.push_notification(Notification::info(t!("columns-reset")), cx);
    }

    pub fn commit_column_layout(
        &mut self,
        layout: ColumnLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.column_layout = layout;
        self.notify_table(cx);
        self.persist_column_layout(layout, window, cx);
        cx.notify();
    }

    /// The layout is the only state the table needs to repaint; the delegate
    /// reads it through the per-frame metrics.
    pub fn notify_table(&mut self, cx: &mut Context<Self>) {
        self.libraries.update(cx, |_, list_cx| list_cx.notify());
        cx.notify();
    }

    pub fn persist_column_layout(
        &mut self,
        layout: ColumnLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.store.set_library_column_layout(layout) {
            window.push_notification(
                Notification::error(t!("column-layout-save-failed", error = error.to_string())),
                cx,
            );
        }
    }

    /// R17: size one column to its widest row text (or its header, with the
    /// sort marker) plus the cell chrome. The explicit width stops Name
    /// flexing, which is the signed behaviour.
    pub fn fit_column_to_content(
        &mut self,
        column: ColumnId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let measured = self.measured_column_widths(window, cx)[column.index()];
        let mut layout = self.column_layout;
        layout.fit(column, measured);
        self.commit_column_layout(layout, window, cx);
    }

    pub fn fit_all_columns_to_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let measured = self.measured_column_widths(window, cx);
        let mut layout = self.column_layout;
        layout.fit_all(|column| measured[column.index()]);
        self.commit_column_layout(layout, window, cx);
    }

    /// One measurement pass for every column. Header labels include the
    /// longest direction marker so an active header never truncates its own
    /// name; the rows come from the delegate, so off-screen rows count too.
    pub fn measured_column_widths(&self, window: &mut Window, cx: &App) -> [f32; 7] {
        let libraries = self.libraries.read(cx);
        let delegate = libraries.delegate();
        let inputs = RowViewInputs {
            running: &delegate.running,
            crashed: &delegate.crashed,
            starting: self.start_in_flight,
            health: &delegate.health,
            health_errors: &delegate.health_errors,
            latency: &delegate.latency,
            traffic: &delegate.traffic,
            sub_labels: &delegate.sub_labels,
            preset_names: &delegate.preset_names,
        };
        let mut widest = [0f32; 7];
        for column in ColumnId::ALL {
            let label = library_column_label(column);
            let with_marker = format!("{label}{}", t!("sort-marker-desc"));
            widest[column.index()] = measure_fit_text(&with_marker, window);
        }
        for row in &delegate.rows {
            let view = RowView::of(row, &inputs);
            let upstream = delegate
                .upstream_for(&row.id)
                .map(str::to_string)
                .unwrap_or_else(|| t!("library-no-upstream"));
            let traffic = view
                .traffic
                .and_then(|(rx, tx)| traffic_summary_text(rx, tx))
                .unwrap_or_default();
            let texts = [
                view.name,
                endpoint_display(&view.bind, view.port),
                view.health_text,
                view.group,
                view.protocol,
                upstream,
                traffic,
            ];
            for column in ColumnId::ALL {
                widest[column.index()] =
                    widest[column.index()].max(measure_fit_text(&texts[column.index()], window));
            }
        }
        for width in &mut widest {
            *width += FIT_CHROME;
        }
        widest
    }

    /// Move one column one slot in the sheet's list. Locked columns move like
    /// every other column; only hiding is locked.
    pub fn move_column_step(
        &mut self,
        column: ColumnId,
        up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let order = *self.column_layout.order();
        let Some(index) = order.iter().position(|candidate| *candidate == column) else {
            return;
        };
        let mut layout = self.column_layout;
        let moved = if up {
            index > 0 && layout.move_column(column, order[index - 1], false)
        } else {
            order
                .get(index + 1)
                .is_some_and(|next| layout.move_column(column, *next, true))
        };
        if moved {
            self.commit_column_layout(layout, window, cx);
        }
    }
}

pub fn measure_fit_text(text: &str, window: &mut Window) -> f32 {
    if text.is_empty() {
        return 0.;
    }
    let style = window.text_style();
    let font = style.font();
    let color = style.color;
    let runs = [TextRun {
        len: text.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    window
        .text_system()
        .shape_line(text.to_string().into(), px(FIT_TEXT_PX), &runs, None)
        .width
        .as_f32()
}

/// T-04: the chip under the pointer while a header is dragged. It names the
/// column being moved so the drop result is obvious before release.
pub struct ColumnDragPreview {
    pub label: String,
}

impl Render for ColumnDragPreview {
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

impl AppView {
    pub fn hover_column_drop(
        &mut self,
        column: ColumnId,
        side: DropSide,
        cx: &mut Context<Self>,
    ) {
        if self.column_drop != Some((column, side)) {
            self.column_drop = Some((column, side));
            cx.notify();
        }
    }

    pub fn clear_column_drop(&mut self, cx: &mut Context<Self>) {
        if self.column_drop.take().is_some() {
            cx.notify();
        }
    }

    pub fn drop_column(
        &mut self,
        drag: ColumnDrag,
        target: ColumnId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = self
            .column_drop
            .take()
            .filter(|(hovered, _)| *hovered == target)
            .map(|(_, side)| side)
            .unwrap_or(DropSide::After);
        let mut layout = self.column_layout;
        if layout.move_column(drag.column, target, side == DropSide::After) {
            self.commit_column_layout(layout, window, cx);
        } else {
            cx.notify();
        }
    }
}

/// R11: the Columns sheet's live body. It observes the app so a visibility,
/// order or width change repaints here without closing and reopening the
/// sheet; the sheet's chrome is built once by the root.
pub struct ColumnsSheetBody {
    pub app: Entity<AppView>,
    pub _app_subscription: Subscription,
}

impl ColumnsSheetBody {
    pub fn new(app: Entity<AppView>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            _app_subscription: subscription,
        }
    }
}

impl Render for ColumnsSheetBody {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let layout = app.column_layout;
        let order = *layout.order();
        let mut rows = div().flex().flex_col().gap_1();
        for (index, column) in order.iter().copied().enumerate() {
            let label = library_column_label(column);
            let locked = column.locked();
            let aria = if locked {
                format!("{label}, {}", t!("columns-always-visible"))
            } else {
                label.clone()
            };
            let visible_label = if locked {
                format!("{label} · {}", t!("columns-always-visible"))
            } else {
                label.clone()
            };
            let toggle_app = self.app.clone();
            let up_app = self.app.clone();
            let down_app = self.app.clone();
            let up_aria = t!("columns-move-up-aria", column = &label);
            let down_aria = t!("columns-move-down-aria", column = &label);
            rows = rows.child(
                div()
                    .id(format!("columns-sheet-row-{}", column.id()))
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new(format!("columns-sheet-toggle-{}", column.id()))
                            .checked(layout.is_visible(column))
                            .disabled(locked)
                            .label(visible_label)
                            .accessibility_label(aria)
                            .on_change(move |checked, window, cx| {
                                toggle_app.update(cx, |view, cx| {
                                    view.set_column_visible(column, *checked, window, cx);
                                });
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(format!("columns-sheet-up-{}", column.id()))
                            .small()
                            .compact()
                            .label(t!("columns-move-up"))
                            .accessibility_label(up_aria)
                            .disabled(index == 0)
                            .on_click(move |_, window, cx| {
                                up_app.update(cx, |view, cx| {
                                    view.move_column_step(column, true, window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("columns-sheet-down-{}", column.id()))
                            .small()
                            .compact()
                            .label(t!("columns-move-down"))
                            .accessibility_label(down_aria)
                            .disabled(index + 1 == order.len())
                            .on_click(move |_, window, cx| {
                                down_app.update(cx, |view, cx| {
                                    view.move_column_step(column, false, window, cx);
                                });
                            }),
                    ),
            );
        }

        let width_app = self.app.clone();
        let order_app = self.app.clone();
        let visibility_app = self.app.clone();
        let all_app = self.app.clone();
        div()
            .id("columns-sheet-body")
            .test_support()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("columns-sheet-hint")),
            )
            .child(rows)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("columns-sheet-reset-widths")
                            .small()
                            .label(t!("columns-reset-widths"))
                            .on_click(move |_, window, cx| {
                                width_app.update(cx, |view, cx| {
                                    view.reset_column_widths(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("columns-sheet-reset-order")
                            .small()
                            .label(t!("columns-reset-order"))
                            .on_click(move |_, window, cx| {
                                order_app.update(cx, |view, cx| {
                                    view.reset_column_order(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("columns-sheet-reset-visibility")
                            .small()
                            .label(t!("columns-reset-visibility"))
                            .on_click(move |_, window, cx| {
                                visibility_app.update(cx, |view, cx| {
                                    view.reset_column_visibility(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("columns-sheet-reset-all")
                            .small()
                            .label(t!("columns-reset-all"))
                            .on_click(move |_, window, cx| {
                                all_app.update(cx, |view, cx| {
                                    view.reset_all_columns(window, cx);
                                });
                            }),
                    ),
            )
    }
}
