use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FacetMenu {
    Group,
    Protocol,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FacetChoice {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) display: String,
    pub(crate) value: Option<String>,
}

impl SearchableListItem for FacetChoice {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.title.clone().into()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.display.clone().into_any_element())
    }

    fn value(&self) -> &Option<String> {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FacetChoiceList(pub(crate) Vec<FacetChoice>);

impl SearchableListDelegate for FacetChoiceList {
    type Item = FacetChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&FacetChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        FacetChoice: SearchableListItem<Value = V>,
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
        item: &FacetChoice,
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

pub(crate) fn slugify_group_id(group: &str) -> String {
    let mut slug = String::with_capacity(group.len());
    for ch in group.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "group".to_string()
    } else {
        slug
    }
}

impl FacetMenu {
    pub(crate) const ALL: [FacetMenu; 3] =
        [FacetMenu::Group, FacetMenu::Protocol, FacetMenu::Status];

    pub(crate) fn choices(&self, delegate: &LibraryDelegate) -> (Vec<FacetChoice>, Option<String>) {
        match self {
            FacetMenu::Group => (
                std::iter::once(FacetChoice {
                    id: "facet-group-all".to_string(),
                    title: "All groups".to_string(),
                    display: "Group: All".to_string(),
                    value: None,
                })
                .chain(
                    delegate
                        .group_options()
                        .into_iter()
                        .map(|(group, count)| FacetChoice {
                            id: format!("facet-group-{}", slugify_group_id(&group)),
                            title: format!("{group} ({count})"),
                            display: format!("Group: {group} ({count})"),
                            value: Some(group),
                        }),
                )
                .collect(),
                delegate.facet_group.clone(),
            ),
            FacetMenu::Protocol => (
                std::iter::once(FacetChoice {
                    id: "facet-protocol-all".to_string(),
                    title: "All protocols".to_string(),
                    display: "Protocol: All".to_string(),
                    value: None,
                })
                .chain(
                    delegate
                        .protocol_options()
                        .into_iter()
                        .map(|(protocol, count)| FacetChoice {
                            id: format!("facet-protocol-{protocol}"),
                            title: format!("{protocol} ({count})"),
                            display: format!("Protocol: {protocol} ({count})"),
                            value: Some(protocol),
                        }),
                )
                .collect(),
                delegate.facet_protocol.clone(),
            ),
            FacetMenu::Status => (
                std::iter::once(FacetChoice {
                    id: "facet-status-all".to_string(),
                    title: "All statuses".to_string(),
                    display: "Status: All".to_string(),
                    value: None,
                })
                .chain(
                    delegate
                        .status_options()
                        .into_iter()
                        .map(|(status, count)| {
                            let label = status.label().to_string();
                            FacetChoice {
                                id: format!("facet-status-{label}"),
                                title: format!("{label} ({count})"),
                                display: format!("Status: {label} ({count})"),
                                value: Some(label),
                            }
                        }),
                )
                .collect(),
                delegate
                    .facet_status
                    .map(|status| status.label().to_string()),
            ),
        }
    }

    pub(crate) fn current_display(&self, delegate: &LibraryDelegate) -> String {
        let (choices, current) = self.choices(delegate);
        choices
            .into_iter()
            .find(|choice| choice.value == current)
            .map(|choice| choice.display)
            .unwrap_or_else(|| "All".to_string())
    }

    pub(crate) fn apply(
        &self,
        view: &mut AppView,
        value: Option<String>,
        cx: &mut Context<AppView>,
    ) {
        view.libraries.update(cx, |state, list_cx| {
            let delegate = state.delegate_mut();
            match self {
                FacetMenu::Group => delegate.set_facet_group(value),
                FacetMenu::Protocol => delegate.set_facet_protocol(value),
                FacetMenu::Status => {
                    delegate
                        .set_facet_status(value.and_then(|label| FacetStatus::from_label(&label)));
                }
            }
            list_cx.notify();
        });
        cx.notify();
    }
}

pub(crate) struct FacetPicker {
    pub(crate) state: Entity<SelectState<FacetChoiceList>>,
    pub(crate) synced: Vec<FacetChoice>,
    _subscription: Subscription,
}

impl FacetPicker {
    pub(crate) fn new(
        menu: FacetMenu,
        choices: Vec<FacetChoice>,
        current: Option<String>,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        let selected = choices
            .iter()
            .position(|choice| choice.value == current)
            .map(IndexPath::new);
        let state =
            cx.new(|cx| SelectState::new(FacetChoiceList(choices.clone()), selected, window, cx));
        let subscription = cx.subscribe(
            &state,
            move |view, _state, event: &SelectEvent<FacetChoiceList>, cx| {
                let SelectEvent::Confirm(choice) = event;
                if let Some(value) = choice {
                    menu.apply(view, value.clone(), cx);
                }
            },
        );
        Self {
            state,
            synced: choices,
            _subscription: subscription,
        }
    }
}
