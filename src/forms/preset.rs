use crate::*;

#[derive(Clone)]
/// K3/T-11 simple-rules boxes (write-once authoring sugar over the same
/// stored rules): the boxes are the whole authoring surface now — K3 removed
/// the `Rules JSON (optional)` textarea and the grammar explainer paragraph,
/// so the grammar lives in each box's placeholder and tooltip only. The
/// `rules` state stays in the form because the commit and reopen paths read
/// whatever a restored preset already carried.
pub(crate) struct PresetForm {
    pub(crate) name: Entity<InputState>,
    pub(crate) rules: Entity<TextareaState>,
    pub(crate) simple_direct: Entity<TextareaState>,
    pub(crate) simple_block: Entity<TextareaState>,
    pub(crate) simple_proxy: Entity<TextareaState>,
    /// K3: the single `Add remote ruleset…` URL row — the one creation path
    /// replaces the old `Add by URL` / `New Remote` doors.
    pub(crate) remote_url: Entity<InputState>,
}

impl PresetForm {
    pub(crate) fn restored(
        window: &mut Window,
        cx: &mut Context<AppView>,
        name: &str,
        rules: &str,
    ) -> Self {
        Self::restored_with_boxes(window, cx, name, rules, &SimpleRuleBoxes::default())
    }

    pub(crate) fn restored_with_boxes(
        window: &mut Window,
        cx: &mut Context<AppView>,
        name: &str,
        rules: &str,
        boxes: &SimpleRuleBoxes,
    ) -> Self {
        Self {
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Preset name")
                    .default_value(name.to_string())
            }),
            rules: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Optional rules JSON (preset object or rules array)")
                    .default_value(rules.to_string())
            }),
            simple_direct: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("domain:example.com, suffix:cn — one rule per line")
                    .default_value(boxes.direct.clone())
            }),
            simple_block: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("keyword:ads, processName:updater.exe")
                    .default_value(boxes.block.clone())
            }),
            simple_proxy: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("ruleset:streaming, domain:netflix.com")
                    .default_value(boxes.proxy.clone())
            }),
            remote_url: cx
                .new(|cx| InputState::new(window, cx).placeholder("https://host.example/set.srs")),
        }
    }

    pub(crate) fn element(self, cx: &App) -> impl IntoElement {
        use crate::preset::SimpleRulesBox;
        let example = |r#box: SimpleRulesBox| match r#box {
            SimpleRulesBox::Direct => "domain:example.com, suffix:cn",
            SimpleRulesBox::Block => "keyword:ads, processName:updater.exe",
            SimpleRulesBox::Proxy => "ruleset:streaming, domain:netflix.com",
        };
        let mut simple = div()
            .id("preset-simple-rules")
            .test_support()
            .flex()
            .flex_col()
            .gap_1();
        for (r#box, state) in [
            (SimpleRulesBox::Direct, &self.simple_direct),
            (SimpleRulesBox::Block, &self.simple_block),
            (SimpleRulesBox::Proxy, &self.simple_proxy),
        ] {
            let tip = format!("One rule per line — e.g. {}", example(r#box));
            let action = r#box.action();
            simple = simple
                .child(
                    div()
                        .id(format!("preset-simple-label-{action}"))
                        .test_support()
                        .text_sm()
                        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                        .child(r#box.label()),
                )
                .child(
                    div()
                        .id(format!("field-preset-simple-{action}"))
                        .test_support()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Textarea::new(state).h(px(56.))),
                );
        }
        let invalid_lines = simple_rules_invalid_rows(&self, cx);
        if !invalid_lines.is_empty() {
            let mut fold = div()
                .id("preset-simple-invalid-lines")
                .test_support()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .id("preset-simple-invalid-header")
                        .test_support()
                        .text_sm()
                        .child("Invalid lines"),
                );
            for (index, (r#box, line)) in invalid_lines.into_iter().enumerate() {
                fold = fold.child(
                    div()
                        .id(format!("preset-simple-invalid-{index}"))
                        .test_support()
                        .text_sm()
                        .child(format!("{} box: {}", r#box.label(), line)),
                );
            }
            simple = simple.child(fold);
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("preset-new-caption")
                    .test_support()
                    .text_sm()
                    .child("New preset"),
            )
            .child(dialog_field("field-preset-name", "Name", self.name))
            .child(simple)
            .child(dialog_field(
                "field-preset-remote-url",
                "Add remote ruleset (URL)",
                self.remote_url,
            ))
    }
}
