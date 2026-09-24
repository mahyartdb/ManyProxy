#![recursion_limit = "512"]

use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::list::{List, ListDelegate, ListItem, ListState};
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::TitleBar;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::*;

/// Delegate for the configuration library list.
///
/// Ticket 01 owns the empty shell only: zero sections of real data, one
/// section reporting zero rows, and an empty-state message. Ticket 02 fills
/// this with stored configurations.
struct LibraryDelegate;

impl ListDelegate for LibraryDelegate {
    type Item = ListItem;

    fn sections_count(&self, _cx: &App) -> usize {
        1
    }

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        0
    }

    fn set_selected_index(
        &mut self,
        _index: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
    }

    fn render_item(
        &mut self,
        _index: IndexPath,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        None
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("No proxy configurations yet")
    }
}

/// Root view of the ManyProxy window: title bar, toolbar, virtualized
/// configuration list, status bar, plus the overlay layers owned by `Root`.
struct AppView {
    focus: FocusHandle,
    libraries: Entity<ListState<LibraryDelegate>>,
}

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let libraries = cx.new(|cx| {
            ListState::new(LibraryDelegate, window, cx)
                .searchable(true)
                .selectable(true)
        });
        Self {
            focus: cx.focus_handle(),
            libraries,
        }
    }

    /// Number of stored configurations. Always 0 until ticket 02.
    fn config_count(&self, cx: &App) -> usize {
        self.libraries.read(cx).delegate().items_count(0, cx)
    }

    fn open_add_placeholder(&self, window: &mut Window, cx: &mut App) {
        window.open_dialog(cx, |dialog: Dialog, _, _| {
            dialog
                .title("Add proxy configuration")
                .child("The configuration form arrives with ticket 02.")
        });
    }
    fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let count = self.config_count(cx);
        StatusBar::new().left(
            div()
                .id("status-count")
                .test_support()
                .child(format!("{count} configurations")),
        )
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("manyproxy-shell")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context("manyproxy-library")
            .child(TitleBar::new())
            .child(
                div()
                    .id("manyproxy-toolbar")
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_2()
                    .child(Button::new("add-config").label("Add").on_click(cx.listener(
                        |view, _, window, cx| {
                            view.open_add_placeholder(window, cx);
                        },
                    ))),
            )
            .child(
                div().flex_1().child(
                    div().id("config-list").test_support().child(
                        List::new(&self.libraries).search_placeholder("Search configurations"),
                    ),
                ),
            )
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(TitleBar::window_options(), |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::AppView;
    use gpui_kit::component::dialog::Dialog;
    use gpui_kit::component::{Root, WindowExt};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{px, size, AppContext, Entity, TestAppContext};

    /// Ticket 01, scenario 1: cold start shows an empty library.
    /// Ticket 01, scenario 2: the mounted dialog layer renders a dialog.
    ///
    /// NOTE: compiling this test needs a large rustc stack on Windows
    /// (`RUST_MIN_STACK=67108864`) plus the crate `#![recursion_limit]`
    /// above; both come from the `gpui_kit::test` harness expansion.
    #[gpui_kit::test]
    fn shell_boots_with_empty_library(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::new(window, cx));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);

        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        let list_found = cx.update(|window, _| window.try_find("config-list").is_some());
        assert!(
            list_found,
            "config list renders: missing element, see registered paths in panic context"
        );
        assert!(cx.update(|window, _| window.try_find("status-count").is_some()));

        cx.update(|window, cx| {
            window.open_dialog(cx, |dialog: Dialog, _, _| {
                dialog.title("Add proxy configuration")
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(
            cx.debug_bounds("dialog-0").is_some(),
            "dialog layer paints the dialog"
        );
    }
}
