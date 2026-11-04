use crate::*;

use gpui_kit::component::notification::Notification;

/// One keyboard/menu resize step. 16px is one comfortable character pair at the
/// default text scale: big enough to feel like a move, small enough to land on
/// a useful width without a drag.
const WIDTH_STEP: f32 = 16.;

impl AppView {
    pub(crate) fn begin_column_resize(
        &mut self,
        column: LibraryColumn,
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

    /// Live drag: the delegate sees the new width every move so the row and the
    /// header reflow together; the store waits for mouse-up.
    pub(crate) fn drag_column_resize(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(resize) = self.column_resize else {
            return;
        };
        let width = resize.start_width + (x - resize.start_x);
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_column_width(resize.column, width);
            list_cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn finish_column_resize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.column_resize.take().is_none() {
            return;
        }
        let layout = self.libraries.read(cx).delegate().layout;
        self.persist_column_layout(layout, window, cx);
    }

    pub(crate) fn nudge_column_width(
        &mut self,
        column: LibraryColumn,
        delta: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut layout = self.libraries.read(cx).delegate().layout;
        layout.set_width(column, layout.width(column) + delta);
        self.commit_column_layout(layout, window, cx);
    }

    pub(crate) fn reset_column_width(
        &mut self,
        column: LibraryColumn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut layout = self.libraries.read(cx).delegate().layout;
        layout.reset_width(column);
        self.commit_column_layout(layout, window, cx);
        window.push_notification(
            Notification::info(format!("{} column restored to default", column.label())),
            cx,
        );
    }

    /// The Row-columns dialog action. Clears every width and the reorder in one
    /// step: the dialog promises "Reset columns", not "reset the widths I
    /// happen to remember".
    pub(crate) fn reset_all_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut layout = self.libraries.read(cx).delegate().layout;
        layout.reset();
        self.commit_column_layout(layout, window, cx);
        window.push_notification(Notification::info("Columns reset to defaults"), cx);
    }

    fn commit_column_layout(
        &mut self,
        layout: LibraryColumns,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_layout(layout);
            list_cx.notify();
        });
        self.persist_column_layout(layout, window, cx);
        cx.notify();
    }

    fn persist_column_layout(
        &mut self,
        layout: LibraryColumns,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.store.set_library_column_layout(layout) {
            window.push_notification(
                Notification::error(format!("Could not save column layout: {error}")),
                cx,
            );
        }
    }
}

/// The keyboard/menu half of resizing: each header edge is draggable, and the
/// same three verbs are reachable from the header's context menu.
#[derive(Clone, Copy)]
pub(crate) enum WidthAction {
    Wider,
    Narrower,
    Reset,
}

impl WidthAction {
    pub(crate) const ALL: [WidthAction; 3] = [
        WidthAction::Wider,
        WidthAction::Narrower,
        WidthAction::Reset,
    ];

    pub(crate) fn id_suffix(self) -> &'static str {
        match self {
            WidthAction::Wider => "wider",
            WidthAction::Narrower => "narrower",
            WidthAction::Reset => "reset",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            WidthAction::Wider => "Wider",
            WidthAction::Narrower => "Narrower",
            WidthAction::Reset => "Reset",
        }
    }

    pub(crate) fn apply(
        self,
        view: &Entity<AppView>,
        column: LibraryColumn,
        window: &mut Window,
        cx: &mut App,
    ) {
        view.update(cx, |view, cx| match self {
            WidthAction::Wider => view.nudge_column_width(column, WIDTH_STEP, window, cx),
            WidthAction::Narrower => view.nudge_column_width(column, -WIDTH_STEP, window, cx),
            WidthAction::Reset => view.reset_column_width(column, window, cx),
        });
    }
}
