#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum InspectorTab {
    #[default]
    Details,
    Health,
    Presets,
    Logs,
    Connections,
    Traffic,
}

impl InspectorTab {
    pub(crate) fn index(self) -> usize {
        match self {
            InspectorTab::Details => 0,
            InspectorTab::Health => 1,
            InspectorTab::Presets => 2,
            InspectorTab::Logs => 3,
            InspectorTab::Connections => 4,
            InspectorTab::Traffic => 5,
        }
    }

    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            1 => InspectorTab::Health,
            2 => InspectorTab::Presets,
            3 => InspectorTab::Logs,
            4 => InspectorTab::Connections,
            5 => InspectorTab::Traffic,
            _ => InspectorTab::Details,
        }
    }

    pub(crate) fn a11y_label(self) -> &'static str {
        match self {
            InspectorTab::Details => "Details",
            InspectorTab::Health => "Health",
            InspectorTab::Presets => "Presets",
            InspectorTab::Logs => "Logs",
            InspectorTab::Connections => "Connections",
            InspectorTab::Traffic => "Traffic",
        }
    }
}
