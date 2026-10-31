use crate::*;

#[derive(Clone)]
pub(crate) struct SubForm {
    pub(crate) label: Entity<InputState>,
    pub(crate) url: Entity<InputState>,
    pub(crate) group: Entity<InputState>,
    pub(crate) auto_refresh: Rc<RefCell<bool>>,

    pub(crate) interval_secs: Rc<RefCell<i64>>,
}

impl SubForm {
    pub(crate) fn restored(
        window: &mut Window,
        cx: &mut Context<AppView>,
        label: &str,
        url: &str,
        group: &str,
        auto_refresh: bool,
        interval_secs: i64,
    ) -> Self {
        Self {
            label: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Label")
                    .default_value(label.to_string())
            }),
            url: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://…")
                    .default_value(url.to_string())
            }),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(group.to_string())
            }),
            auto_refresh: Rc::new(RefCell::new(auto_refresh)),
            interval_secs: Rc::new(RefCell::new(interval_secs)),
        }
    }

    pub(crate) fn element(self, view: Entity<AppView>, cx: &App) -> impl IntoElement {
        let auto = self.auto_refresh.clone();
        let checked = *auto.borrow();
        let url = self.url.read(cx).value().to_string();
        let repaint_view = view.clone();
        let repaint: DialogRepaint = Rc::new(move |_, cx| {
            repaint_view.update(cx, |_, _| {});
        });
        let mut form = v_form()
            .small()
            .child(dialog_field("field-sub-label", "Label", self.label))
            .child(dialog_field("field-sub-url", "Subscription URL", self.url));

        if let Some(warning) = http_subscription_warning(&url) {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("sub-add-http-warning")
                        .test_support()
                        .child(warning),
                ),
            );
        }
        form = form
            .child(dialog_field("field-sub-group", "Group", self.group))
            .child(field().label_indent(false).child(
                div().id("field-sub-auto").test_support().child(
                    Switch::new("sub-auto-add")
                        .checked(checked)
                        .label("Auto-refresh")

                        .tooltip(
                            "Automatic refresh of this subscription. Manual Refresh always works.",
                        )
                        .accessibility_label(
                            "Auto-refresh this subscription automatically. Manual Refresh always works.",
                        )
                        .on_change(move |next: &bool, _, _| {
                            *auto.borrow_mut() = *next;
                        }),
                ),
            ))

            .child(field().label_indent(false).child(
                div()
                    .id("field-sub-interval")
                    .test_support()
                    .child(interval_picker_row(
                        "sub-interval-add",
                        self.interval_secs,
                        repaint,
                    )),
            ));
        form
    }
}
