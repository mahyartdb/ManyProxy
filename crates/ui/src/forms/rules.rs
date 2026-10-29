use crate::*;

/// T-11: a snapshot of the three simple-rules boxes read from a form at
/// commit time. Empty boxes contribute nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SimpleRuleBoxes {
    pub(crate) direct: String,
    pub(crate) block: String,
    pub(crate) proxy: String,
}

impl SimpleRuleBoxes {
    pub(crate) fn from_form(form: &PresetForm, cx: &App) -> Self {
        Self {
            direct: form.simple_direct.read(cx).value().to_string(),
            block: form.simple_block.read(cx).value().to_string(),
            proxy: form.simple_proxy.read(cx).value().to_string(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.direct.trim().is_empty()
            && self.block.trim().is_empty()
            && self.proxy.trim().is_empty()
    }
}

/// The invalid lines the current parse of the three boxes produces; the
/// fold always mirrors the text as typed and exists for this session only —
/// nothing is persisted. Cheap: the parser is pure and small inputs are the
/// norm.
pub(crate) fn simple_rules_invalid_rows(
    form: &PresetForm,
    cx: &App,
) -> Vec<(crate::preset::SimpleRulesBox, String)> {
    use crate::preset::SimpleRulesBox;
    let mut rows = Vec::new();
    for (r#box, state) in [
        (SimpleRulesBox::Direct, &form.simple_direct),
        (SimpleRulesBox::Block, &form.simple_block),
        (SimpleRulesBox::Proxy, &form.simple_proxy),
    ] {
        let text = state.read(cx).value().to_string();
        if text.trim().is_empty() {
            continue;
        }
        let parsed = crate::preset::parse_simple_rules(r#box, r#box.action(), &text);
        rows.extend(
            parsed
                .invalid
                .into_iter()
                .map(|line| (r#box, line.to_string())),
        );
    }
    rows
}
