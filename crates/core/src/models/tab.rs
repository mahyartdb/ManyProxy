#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Details,
    Health,
    Logs,
    Links,
    Traffic,
}

impl InspectorTab {
    /// K1: the switcher holds five panes in one row (Details · Health · Logs ·
    /// Links · Traffic).
    pub fn index(self) -> usize {
        match self {
            InspectorTab::Details => 0,
            InspectorTab::Health => 1,
            InspectorTab::Logs => 2,
            InspectorTab::Links => 3,
            InspectorTab::Traffic => 4,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            1 => InspectorTab::Health,
            2 => InspectorTab::Logs,
            3 => InspectorTab::Links,
            4 => InspectorTab::Traffic,
            _ => InspectorTab::Details,
        }
    }

    pub fn a11y_label(self) -> &'static str {
        match self {
            InspectorTab::Details => "Details",
            InspectorTab::Health => "Health",
            InspectorTab::Logs => "Logs",
            InspectorTab::Links => "Links",
            InspectorTab::Traffic => "Traffic",
        }
    }
}
