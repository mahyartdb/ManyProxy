use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolChoice {
    pub id: String,
    pub title: String,
    pub display: String,
    pub value: String,
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

pub fn protocol_choices() -> Vec<ProtocolChoice> {
    VALID_PROTOCOLS
        .iter()
        .map(|protocol| ProtocolChoice {
            id: format!("protocol-option-{protocol}"),
            title: protocol.to_string(),
            display: t!("protocol-choice-display", protocol = *protocol),
            value: protocol.to_string(),
        })
        .collect()
}
