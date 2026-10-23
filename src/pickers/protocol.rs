use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolChoice {
    id: String,
    title: String,
    display: String,
    pub(crate) value: String,
}

impl SearchableListItem for ProtocolChoice {
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
pub(crate) struct ProtocolChoiceList(pub(crate) Vec<ProtocolChoice>);

impl SearchableListDelegate for ProtocolChoiceList {
    type Item = ProtocolChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&ProtocolChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        ProtocolChoice: SearchableListItem<Value = V>,
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
        item: &ProtocolChoice,
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

pub(crate) fn protocol_choices() -> Vec<ProtocolChoice> {
    VALID_PROTOCOLS
        .iter()
        .map(|protocol| ProtocolChoice {
            id: format!("protocol-option-{protocol}"),
            title: protocol.to_string(),
            display: format!("Protocol: {protocol}"),
            value: protocol.to_string(),
        })
        .collect()
}
