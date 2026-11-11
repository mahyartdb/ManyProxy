use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FacetMenu {
    Group,
    /// T-05: the latency buckets live in their own picker so they compose with
    /// the group/status menu instead of replacing it.
    Latency,
}

/// K11/A2-1: the enabled/disabled axis rides the group dropdown (one `facet`
/// menu, not a second picker; picking one releases the other). The values are
/// encoded with a control character no group or protocol name can carry, so a
/// sentinel choice can never collide with a real group row.
const STATE_ENABLED_VALUE: &str = "\u{1}state:enabled";
const STATE_DISABLED_VALUE: &str = "\u{1}state:disabled";
const STATE_RUNNING_VALUE: &str = "\u{1}state:running";
const STATE_DEAD_VALUE: &str = "\u{1}state:dead";
const STATE_STOPPED_VALUE: &str = "\u{1}state:stopped";

/// K11: the lamp's three states are facet values too. The lamp click writes
/// `facet_status` directly, so the dropdown has to be able to name and release
/// that filter; otherwise its collapsed label says `Group: All` while the list
/// shows only running rows.
fn status_value(status: FacetStatus) -> String {
    match status {
        FacetStatus::Enabled => STATE_ENABLED_VALUE,
        FacetStatus::Disabled => STATE_DISABLED_VALUE,
        FacetStatus::Running => STATE_RUNNING_VALUE,
        FacetStatus::Dead => STATE_DEAD_VALUE,
        FacetStatus::Stopped => STATE_STOPPED_VALUE,
    }
    .to_string()
}

fn status_from_value(value: &str) -> Option<FacetStatus> {
    match value {
        STATE_ENABLED_VALUE => Some(FacetStatus::Enabled),
        STATE_DISABLED_VALUE => Some(FacetStatus::Disabled),
        STATE_RUNNING_VALUE => Some(FacetStatus::Running),
        STATE_DEAD_VALUE => Some(FacetStatus::Dead),
        STATE_STOPPED_VALUE => Some(FacetStatus::Stopped),
        _ => None,
    }
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
    /// K11/A2-1: the filter row is `Search · Group · N of M`. The dropdown's
    /// per-group count chips live inside the list (including the `All` header
    /// row), never on the collapsed control — the `N of M` readout beside the
    /// search box carries the visible count. Enabled/Disabled entries close
    /// the menu over the state axis so the row never grows a second picker.
    pub(crate) fn choices(&self, delegate: &LibraryDelegate) -> (Vec<FacetChoice>, Option<String>) {
        if *self == FacetMenu::Latency {
            let choices = LatencyBucket::ALL
                .into_iter()
                .map(|bucket| {
                    let label = latency_bucket_label(bucket);
                    FacetChoice {
                        id: format!("facet-latency-{}", bucket.token()),
                        display: t!("facet-latency-choice", bucket = label.clone()),
                        title: label,
                        value: Some(bucket.token().to_string()),
                    }
                })
                .collect();
            return (
                choices,
                Some(delegate.latency_facet.bucket.token().to_string()),
            );
        }
        let total = delegate.rows.len();
        let mut choices = vec![FacetChoice {
            id: "facet-group-all".to_string(),
            title: t!("facet-all-groups", total = total),
            display: t!("facet-group-all"),
            value: None,
        }];
        choices.extend(
            delegate
                .group_options()
                .into_iter()
                .map(|(group, count)| FacetChoice {
                    id: format!("facet-group-{}", slugify_group_id(&group)),
                    title: t!("facet-group-choice", group = group.clone(), count = count),
                    display: t!("facet-group-display", group = &group),
                    value: Some(group),
                }),
        );
        let enabled_count = delegate.rows.iter().filter(|row| row.enabled).count();
        let disabled_count = delegate.rows.len() - enabled_count;
        for (status, id, count) in [
            (FacetStatus::Enabled, "facet-status-enabled", enabled_count),
            (
                FacetStatus::Disabled,
                "facet-status-disabled",
                disabled_count,
            ),
        ] {
            let label = status.label();
            choices.push(FacetChoice {
                id: id.to_string(),
                title: t!("facet-status-choice", status = label.clone(), count = count),
                display: t!("facet-status-display", status = label),
                value: Some(status_value(status)),
            });
        }
        for (status, id) in [
            (FacetStatus::Running, "facet-status-running"),
            (FacetStatus::Dead, "facet-status-dead"),
            (FacetStatus::Stopped, "facet-status-stopped"),
        ] {
            let count = delegate
                .rows
                .iter()
                .filter(|row| {
                    status.matches(row, &delegate.running, &delegate.health, &delegate.crashed)
                })
                .count();
            let label = status.label();
            choices.push(FacetChoice {
                id: id.to_string(),
                title: t!("facet-status-choice", status = label.clone(), count = count),
                display: t!("facet-status-display", status = label),
                value: Some(status_value(status)),
            });
        }
        let current = match delegate.facet_status {
            Some(status) => Some(status_value(status)),
            None => delegate.facet_group.clone(),
        };
        (choices, current)
    }

    /// The retired facet band's display. Slice 3's filter row owns it.
    #[allow(dead_code)]
    pub(crate) fn current_display(&self, delegate: &LibraryDelegate) -> String {
        let (choices, current) = self.choices(delegate);
        choices
            .into_iter()
            .find(|choice| choice.value == current)
            .map(|choice| choice.display)
            .unwrap_or_else(|| t!("facet-all"))
    }

    pub(crate) fn apply(
        &self,
        view: &mut AppView,
        value: Option<String>,
        cx: &mut Context<AppView>,
    ) {
        if *self == FacetMenu::Latency {
            let Some(bucket) = value.as_deref().and_then(LatencyBucket::from_token) else {
                return;
            };
            let include_unprobed = view
                .libraries
                .read(cx)
                .delegate()
                .latency_facet
                .include_unprobed;
            view.set_latency_facet(
                LatencyFacet {
                    bucket,
                    include_unprobed,
                },
                cx,
            );
            return;
        }
        view.libraries.update(cx, |state, list_cx| {
            let delegate = state.delegate_mut();
            // K11/A2-1: the dropdown shows one selected item, so a pick writes
            // the two slots disjointly — a state sentinel releases the group
            // choice and any group pick releases the sentinel.
            let (group, status) = match value.as_deref().and_then(status_from_value) {
                Some(status) => (None, Some(status)),
                None => (value, None),
            };
            delegate.set_facet_menu(group, status);
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
