use crate::*;

#[derive(Clone)]
pub(crate) struct ImportForm {
    pub(crate) text: Entity<TextareaState>,
    pub(crate) group: Entity<InputState>,
    pub(crate) file_hint: Option<String>,
    pub(crate) original_text: String,
    pub(crate) multi_files: Option<Vec<(String, String)>>,
}

impl ImportForm {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<AppView>,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
        multi: Option<Vec<(String, String)>>,
    ) -> Self {
        Self {
            text: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("One share link per line, JSON, or a WireGuard conf")
                    .default_value(prefill_text.to_string())
            }),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(prefill_group.to_string())
            }),
            file_hint,
            original_text: prefill_text.to_string(),
            multi_files: multi,
        }
    }

    pub(crate) fn element(self, view: Entity<AppView>, cx: &App) -> impl IntoElement {
        let text = self.text.read(cx).value().to_string();

        let preview = preview_batch_for_form(
            &text,
            self.file_hint.clone(),
            Some(self.original_text.clone()),
            self.multi_files.clone(),
        );
        let (valid, invalid) = (preview.configs.len(), preview.failures.len());
        let first_failure = preview.failures.first().map(|failure| {
            format!(
                "item {} ({}): {}",
                failure.index + 1,
                failure.label,
                failure.reason
            )
        });

        let mut form = v_form()
            .small()

            .child(
                field().label_indent(false).child(
                    div()
                        .id("import-guide")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Paste links — nothing is added until you press Import. vless · vmess · trojan · ss · hysteria · tuic · wg · socks · http · anytls · naive · clash · conf files. e.g. vless://uuid@host:443#name. Drops land anywhere on the window.",
                        ),
                ),
            )
            .child(
                field().label("Links, JSON, or conf").child(
                    div()
                        .id("field-import-text")
                        .test_support()
                        .child(Textarea::new(&self.text).h(px(160.))),
                ),
            )
            .child(dialog_field(
                "field-import-group",
                "Group",
                self.group.clone(),
            ));

        {
            let browse_view = view.clone();
            let browse_text = self.text.clone();
            let browse_group = self.group.clone();
            form = form.child(
                field().label_indent(false).child(
                    Button::new("import-browse")
                        .label("Browse files…")
                        .on_click(move |_, window, cx| {
                            let current_text = browse_text.read(cx).value().to_string();
                            let current_group = browse_group.read(cx).value().to_string();
                            browse_view.update(cx, |view, cx| {
                                view.browse_for_import_files(
                                    current_text,
                                    current_group,
                                    window,
                                    cx,
                                );
                            });
                        }),
                ),
            );
        }
        form = form.child(
            field().label_indent(false).child(
                div()
                    .id("import-preview")
                    .test_support()
                    .child(import_preview_message(valid, invalid)),
            ),
        );

        if let Some(reason) = first_failure {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("import-first-failure")
                        .test_support()
                        .child(format!("first problem: {reason}")),
                ),
            );
        }
        if let Some(files) =
            import_files_label(self.file_hint.as_deref(), self.multi_files.as_deref())
        {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("import-files").test_support().child(files)),
            );
        }
        form
    }
}
