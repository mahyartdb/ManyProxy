use crate::*;

#[derive(Clone)]
/// T-11 simple-rules boxes (write-once authoring sugar over the same stored
/// rules): the rules JSON view stays the single source of truth — box text
/// expands into the structured rules on save and round-trips nowhere.
pub(crate) struct PresetForm {
    pub(crate) name: Entity<InputState>,
    pub(crate) rules: Entity<TextareaState>,
    pub(crate) simple_direct: Entity<TextareaState>,
    pub(crate) simple_block: Entity<TextareaState>,
    pub(crate) simple_proxy: Entity<TextareaState>,
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
        }
    }

    pub(crate) fn element(self, cx: &App) -> impl IntoElement {
        use crate::preset::SimpleRulesBox;
        let mut simple = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id("preset-simple-rules")
                    .test_support()
                    .text_sm()
                    .child("Simple rules"),
            )
            .child(
                div()
                    .id("preset-simple-help")
                    .test_support()
                    .text_sm()
                    .child(
                        "One rule per line: tokens domain:/suffix:/keyword:/regex:/ruleset:/ip:/processName:/processPath: separated by commas or spaces; blank and # lines are skipped. ruleset: names require a staged rule set (Rule sets section) — unstaged sets fail loud when the preset attaches and at config generation.",
                    ),
            );
        for (r#box, state) in [
            (SimpleRulesBox::Direct, &self.simple_direct),
            (SimpleRulesBox::Block, &self.simple_block),
            (SimpleRulesBox::Proxy, &self.simple_proxy),
        ] {
            simple = simple
                .child(
                    div()
                        .id(format!("preset-simple-label-{}", r#box.action()))
                        .test_support()
                        .text_sm()
                        .child(r#box.label()),
                )
                .child(
                    div()
                        .id(format!("field-preset-simple-{}", r#box.action()))
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
            .child(dialog_field("field-preset-name", "Name", self.name))
            .child(
                div()
                    .id("field-preset-rules")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new("Rules JSON (optional)"))
                    .child(Textarea::new(&self.rules).h(px(120.))),
            )
            .child(simple)
    }
}
