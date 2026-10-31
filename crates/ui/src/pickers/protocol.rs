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
