use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelChoice {
    id: String,
    title: String,
    display: String,
    pub(crate) value: String,
}

impl SearchableListItem for PanelChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone().into()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.display.clone().into_any_element())
    }

    fn value(&self) -> &String {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PanelChoiceList(pub(crate) Vec<PanelChoice>);

impl SearchableListDelegate for PanelChoiceList {
    type Item = PanelChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&PanelChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        PanelChoice: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.0
            .iter()
            .position(|choice| choice.value() == value)
            .map(IndexPath::new)
    }

    fn render_item(
        &self,
        _ix: IndexPath,
        item: &PanelChoice,
        _checked: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<AnyElement> {
        Some(
            div()
                .id(item.id.clone())
                .test_support()
                .child(item.title.clone())
                .into_any_element(),
        )
    }
}

pub(crate) fn panel_choices(prefix: &str, items: &[(&str, &str)]) -> Vec<PanelChoice> {
    items
        .iter()
        .map(|(value, title)| {
            let slug = if value.trim().is_empty() {
                "default".to_string()
            } else {
                value.to_string()
            };
            PanelChoice {
                id: format!("{prefix}-{slug}"),
                title: title.to_string(),
                display: title.to_string(),
                value: value.to_string(),
            }
        })
        .collect()
}
