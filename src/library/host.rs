use crate::*;

/// T-33: one row of the per-row bind picker. The id carries the row so two
/// rows never register the same option path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BindHostChoice {
    id: String,
    pub(crate) value: String,
}

impl SearchableListItem for BindHostChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.value.clone().into()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BindHostChoiceList(pub(crate) Vec<BindHostChoice>);

impl BindHostChoiceList {
    pub(crate) fn for_row(row_id: &str, hosts: &[String]) -> Self {
        Self(
            hosts
                .iter()
                .map(|host| BindHostChoice {
                    id: format!("row-bind-option-{row_id}-{host}"),
                    value: host.clone(),
                })
                .collect(),
        )
    }
}

impl SearchableListDelegate for BindHostChoiceList {
    type Item = BindHostChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&BindHostChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        BindHostChoice: SearchableListItem<Value = V>,
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
        item: &BindHostChoice,
        _checked: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<AnyElement> {
        Some(
            div()
                .id(item.id.clone())
                .test_support()
                .child(item.value.clone())
                .into_any_element(),
        )
    }
}

pub(crate) struct RowEndpointInputs {
    /// The draft holder for both control shapes: the free-text field on
    /// custom-config rows, and the value a pick writes on every other row.
    pub(crate) bind: Entity<InputState>,
    pub(crate) port: Entity<InputState>,
    /// T-33: the picker for rows whose listen comes from the generated config.
    /// Custom-config rows never draw it.
    pub(crate) picker: Entity<SelectState<BindHostChoiceList>>,
    /// The host list the picker was last built from.
    pub(crate) picker_hosts: Vec<String>,
    /// The value the picker was last told to show, so a stable row does not
    /// re-select on every paint.
    pub(crate) picker_selected: Option<String>,
    /// A confirmed pick waiting for the window-aware paint step to apply it.
    pub(crate) pick_pending: Option<String>,
    pub(crate) _subs: Vec<Subscription>,
    pub(crate) clean_bind: String,
    pub(crate) clean_port: String,
}
