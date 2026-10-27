#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum InspectorTab {
    #[default]
    Details,
    Health,
    Logs,
    Connections,
    Traffic,
}

impl InspectorTab {
    /// K1: the switcher holds five panes in one row (Details · Health · Logs ·
    /// Connections · Traffic).
    pub(crate) fn index(self) -> usize {
        match self {
            InspectorTab::Details => 0,
            InspectorTab::Health => 1,
            InspectorTab::Logs => 2,
            InspectorTab::Connections => 3,
            InspectorTab::Traffic => 4,
        }
    }

    pub(crate) fn from_index(index: usize) -> Self {
        match index {
            1 => InspectorTab::Health,
            2 => InspectorTab::Logs,
            3 => InspectorTab::Connections,
            4 => InspectorTab::Traffic,
            _ => InspectorTab::Details,
        }
    }

    pub(crate) fn a11y_label(self) -> &'static str {
        match self {
            InspectorTab::Details => "Details",
            InspectorTab::Health => "Health",
            InspectorTab::Logs => "Logs",
            InspectorTab::Connections => "Connections",
            InspectorTab::Traffic => "Traffic",
        }
    }
}
