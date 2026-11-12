use crate::*;

impl AppView {
    pub(crate) fn focus_library_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.focus_handle(cx).focus(window, cx);
    }

    /// R7/R11: the library list's column header strip. Every visible column in
    /// the layout's order paints one sortable header; the lead and the trailing
    /// reserve are explicit spacers, never columns.
    ///
    /// T-03: the strip reads the same `RowMetrics` the row reads, so every
    /// label sits at its column's first character.
    pub(crate) fn render_library_column_headers(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let libraries = self.libraries.read(cx);
        let delegate = libraries.delegate();
        let metrics = delegate.row_metrics;
        let sort = delegate.sort;
        let has_rows = !delegate.rows.is_empty();
        let view = cx.entity();
        let column_drop = self.column_drop;
        let clear_view = view.clone();
        if !has_rows {
            // An empty library keeps today's empty-state clean: a header strip
            // over nothing is noise.
            return div()
                .id("library-column-headers")
                .test_support()
                .into_any_element();
        }
        let mut headers: Vec<AnyElement> = Vec::new();
        for column in *metrics.layout.order() {
            if !metrics.column_visible(column) {
                continue;
            }
            let drop_side =
                column_drop.and_then(|(target, side)| (target == column).then_some(side));
            headers.push(column_header_cell(
                column,
                sort,
                metrics,
                drop_side,
                view.clone(),
                window,
                cx,
            ));
        }
        let strip = div()
            .id("library-column-headers")
            .test_support()
            .relative()
            .w_full()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            // A drag move outside any header must clear the indicator; parent
            // listeners register before child listeners, so the headers below
            // can put it back when the pointer is over one of them.
            .on_drag_move(
                cx.listener(move |view, _: &DragMoveEvent<ColumnDrag>, _, cx| {
                    view.clear_column_drop(cx);
                }),
            )
            // A plain click on the header is a click outside the rows: the
            // selection goes. Header controls that act (sort, menus, resize)
            // keep their own handlers; this only adds the clearing.
            .on_click(move |_, window, cx| {
                clear_view.update(cx, |view, cx| {
                    if view.selection_count() > 0 {
                        view.clear_selection(window, cx);
                    }
                });
            })
            .h(px(self.density.header_height()))
            .border_b_1()
            .border_color(cx.theme().border)
            // The lead gutter the row spends on its state glyph.
            .child(
                div()
                    .id("header-lead")
                    .test_support()
                    .w(px(metrics.lead))
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .id("header-columns")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(RowMetrics::GAP))
                    .flex_shrink_0()
                    .children(headers),
            )
            // The row's trailing reserve holds the hover actions; the strip
            // keeps the same width so the columns line up.
            .child(
                div()
                    .id("header-actions")
                    .test_support()
                    .w(px(metrics.actions))
                    .flex_shrink_0(),
            )
            .child(column_drag_canvas(view));
        strip.into_any_element()
    }

    /// R11: the one Explorer-style menu every header opens. The same items and
    /// the same ids on every column; only the fit target differs.
    pub(crate) fn column_header_menu(
        mut menu: PopupMenu,
        column: ColumnId,
        layout: ColumnLayout,
        view: &Entity<AppView>,
    ) -> PopupMenu {
        menu = menu.item(Self::column_menu_action(
            "columns-menu-size-this",
            t!("columns-size-this"),
            view.clone(),
            move |view, window, cx| view.fit_column_to_content(column, window, cx),
        ));
        menu = menu.item(Self::column_menu_action(
            "columns-menu-size-all",
            t!("columns-size-all"),
            view.clone(),
            |view, window, cx| view.fit_all_columns_to_content(window, cx),
        ));
        menu = menu.separator();
        // The menu lists the signed default order; only the sheet follows the
        // live order, so the two surfaces stay distinguishable.
        for column in ColumnId::ALL {
            menu = menu.item(Self::column_visibility_item(column, layout, view));
        }
        menu = menu.separator();
        menu.item(Self::column_menu_action(
            "columns-menu-more",
            t!("columns-more"),
            view.clone(),
            |view, window, cx| view.open_columns_sheet(window, cx),
        ))
    }

    fn column_menu_action(
        id: &'static str,
        label: String,
        view: Entity<AppView>,
        act: impl Fn(&mut AppView, &mut Window, &mut Context<AppView>) + 'static,
    ) -> PopupMenuItem {
        PopupMenuItem::element(move |_, _| {
            div()
                .id(id)
                .test_support()
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .child(label.clone())
        })
        .on_click(move |_, window, cx| {
            view.update(cx, |view, cx| act(view, window, cx));
        })
    }

    fn column_visibility_item(
        column: ColumnId,
        layout: ColumnLayout,
        view: &Entity<AppView>,
    ) -> PopupMenuItem {
        let label = library_column_label(column);
        let checked = layout.is_visible(column);
        let locked = column.locked();
        let hint = locked.then(|| t!("columns-always-visible"));
        let aria = hint
            .as_ref()
            .map_or_else(|| label.clone(), |hint| format!("{label}, {hint}"));
        let item_label = label.clone();
        let view = view.clone();
        PopupMenuItem::element(move |_, cx| {
            let mut row = div()
                .id(format!("columns-menu-{}", column.id()))
                .test_support()
                .role(Role::MenuItem)
                .aria_label(aria.clone())
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(item_label.clone());
            if let Some(hint) = hint.clone() {
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                );
            }
            row
        })
        .checked(checked)
        .disabled(locked)
        .on_click(move |_, window, cx| {
            view.update(cx, |view, cx| {
                view.set_column_visible(column, !checked, window, cx);
            });
        })
    }
}

/// T-03: window-level pointer tracking for a width drag. A header cell's own
/// `on_mouse_move` stops at the cell edge, but the user may drag higher or
/// lower than the 30px strip; the window listener follows until mouse-up. It
/// has to be registered during paint, hence the zero-visual canvas.
fn column_drag_canvas(view: Entity<AppView>) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_mouse_event({
                let view = view.clone();
                move |event: &MouseMoveEvent, _phase, _window, cx| {
                    if view.read(cx).column_resize.is_some() {
                        view.update(cx, |view, cx| {
                            view.drag_column_resize(event.position.x.as_f32(), cx)
                        });
                    }
                }
            });
            window.on_mouse_event({
                let view = view.clone();
                move |event: &MouseUpEvent, _phase, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    if view.read(cx).column_resize.is_some() {
                        view.update(cx, |view, cx| view.finish_column_resize(window, cx));
                    }
                    // A drag released outside a header has no on_drop to clear
                    // its indicator; defer so a real drop still sees the
                    // indicator state during its own dispatch.
                    if view.read(cx).column_drop.is_some() {
                        let view = view.clone();
                        window.defer(cx, move |_, cx| {
                            view.update(cx, |view, cx| view.clear_column_drop(cx));
                        });
                    }
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h_full()
    .into_any_element()
}
