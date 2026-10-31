//! The library list delegate: rows, visibility, filtering, selection, row render.
//! Belongs: list state and the per-row element; it reads the store, never writes it.
//! Not here: dialogs or engine actions — those call back into `AppView`.
use crate::*;

use gpui_kit::component::list::ListState;
use gpui_kit::component::ThemeStyled as _;

mod columns;
mod host;
mod shard;
mod tone;

pub(crate) use columns::*;
pub(crate) use host::*;
pub(crate) use shard::*;
pub(crate) use tone::*;

/// B16: what the list paints when it has no rows to show. A fetch that has not
/// landed is never "empty" — the list holds skeleton rows at the row pitch so
/// the first real row does not appear from nowhere — and a failed one names the
/// reason and carries the way back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum LibraryLoad {
    #[default]
    Ready,
    Loading(String),
    Failed(String),
}

pub(crate) struct LibraryDelegate {
    /// B16: pushed by every refresh origin; the list never reads the app's
    /// in-flight set itself, because a frame is already borrowing that entity.
    pub(crate) load: LibraryLoad,
    pub(crate) rows: Vec<ProxyConfig>,
    pub(crate) search_index: Vec<RowSearchIndex>,
    pub(crate) upstream_cache: HashMap<String, String>,
    pub(crate) visible: Vec<usize>,
    pub(crate) query: String,
    pub(crate) selected: Option<IndexPath>,

    /// T-06: the list's own focus handle, handed in by the frame that owns the
    /// `ListState`. The selected row wears the kit focus ring while this handle
    /// is focused - the row card is the app's custom control, but the keyboard
    /// lands on the list.
    pub(crate) list_focus: Option<FocusHandle>,

    pub(crate) health: HashMap<String, String>,

    pub(crate) health_tone: HashMap<String, LatencyTone>,

    pub(crate) running: HashSet<String>,

    pub(crate) crashed: HashSet<String>,

    pub(crate) traffic: HashMap<String, (u64, u64)>,

    pub(crate) sub_labels: HashMap<String, String>,
    pub(crate) preset_names: HashMap<String, String>,

    pub(crate) checked: HashSet<String>,

    pub(crate) facet_group: Option<String>,
    pub(crate) facet_status: Option<FacetStatus>,

    pub(crate) show_security: bool,

    /// T-27: the one sort concept — a key plus a direction, persisted in the
    /// `library_sort` meta key. Replaces the retired `type_security_sort` bool.
    pub(crate) sort: LibrarySort,

    /// T-27 mirrors the sort keys read on render. All three are pushed from
    /// state the app already holds (health store samples, session traffic
    /// mirror, out-IP resolutions) — the sort never polls.
    pub(crate) latency: HashMap<String, u64>,

    pub(crate) data_traffic: HashMap<String, (u64, u64)>,

    pub(crate) out_ips: HashMap<String, String>,

    pub(crate) row_columns: RowColumns,

    /// T-03: the geometry the header strip and the row both read. The owner
    /// decides it once per frame (the table's width decides which optional
    /// columns fit) and hands it to both readers.
    pub(crate) row_metrics: RowMetrics,

    pub(crate) app_view: Option<Entity<AppView>>,

    pub(crate) endpoint_inputs: HashMap<String, RowEndpointInputs>,

    pub(crate) pending_endpoint_commits: Vec<String>,

    /// T-33: the bind-address menu every per-row picker draws from — constants
    /// first, then the persisted extras. Pushed from the store on reload, so a
    /// Settings edit reaches every picker on the next paint.
    pub(crate) bind_hosts: Vec<String>,

    /// B18: the one row whose engine state just settled, with a nonce that
    /// makes a second settle run the accent again instead of reusing the
    /// finished one. Only a single-id change sets it — bulk ops and the boot
    /// restore move several rows at once and stay still.
    pub(crate) settled: Option<(String, u64)>,
    settle_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FacetStatus {
    Enabled,
    Disabled,
    /// K11/A2-1: the lamp's three tones are the endpoint-state facet. Matches
    /// reads the same `serving_tone(..)` the lamp paints, so a lamp click
    /// always shows the rows that carry that lamp.
    Running,
    Dead,
    Stopped,
}

impl FacetStatus {
    // K11: the menus no longer round-trip status labels; these survive for the
    // tests that pin the label vocabulary.
    #[cfg(test)]
    pub(crate) const ALL: [FacetStatus; 5] = [
        FacetStatus::Enabled,
        FacetStatus::Disabled,
        FacetStatus::Running,
        FacetStatus::Dead,
        FacetStatus::Stopped,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            FacetStatus::Enabled => "Enabled",
            FacetStatus::Disabled => "Disabled",
            FacetStatus::Running => "Running",
            FacetStatus::Dead => "Dead",
            FacetStatus::Stopped => "Stopped",
        }
    }

    #[cfg(test)]
    pub(crate) fn from_label(label: &str) -> Option<FacetStatus> {
        Self::ALL.into_iter().find(|status| status.label() == label)
    }

    pub(crate) fn matches(
        self,
        row: &ProxyConfig,
        running: &HashSet<String>,
        health: &HashMap<String, String>,
        crashed: &HashSet<String>,
    ) -> bool {
        let tone = || {
            serving_tone(
                running.contains(&row.id),
                health.get(&row.id).map(String::as_str),
                crashed.contains(&row.id),
            )
        };
        match self {
            FacetStatus::Enabled => row.enabled,
            FacetStatus::Disabled => !row.enabled,
            FacetStatus::Running => tone() == ServingTone::Serving,
            FacetStatus::Dead => tone() == ServingTone::Failed,
            FacetStatus::Stopped => tone() == ServingTone::Stopped,
        }
    }
}

impl LibraryDelegate {
    pub(crate) fn new() -> Self {
        Self {
            load: LibraryLoad::Ready,
            rows: Vec::new(),
            search_index: Vec::new(),
            upstream_cache: HashMap::new(),
            visible: Vec::new(),
            query: String::new(),
            selected: None,
            list_focus: None,
            health: HashMap::new(),
            health_tone: HashMap::new(),
            running: HashSet::new(),
            crashed: HashSet::new(),
            traffic: HashMap::new(),
            sub_labels: HashMap::new(),
            preset_names: HashMap::new(),
            checked: HashSet::new(),
            facet_group: None,
            facet_status: None,
            show_security: false,
            sort: LibrarySort::default(),
            latency: HashMap::new(),
            data_traffic: HashMap::new(),
            out_ips: HashMap::new(),
            row_columns: RowColumns::default(),
            row_metrics: RowMetrics::widths(false, false, RowColumns::default()),
            app_view: None,
            endpoint_inputs: HashMap::new(),
            pending_endpoint_commits: Vec::new(),
            bind_hosts: Store::BIND_HOST_CONSTANTS
                .iter()
                .map(|host| host.to_string())
                .collect(),
            settled: None,
            settle_seq: 0,
        }
    }

    pub(crate) fn set_bind_hosts(&mut self, hosts: Vec<String>) {
        self.bind_hosts = hosts;
    }

    /// B16: returns whether the state actually changed, so a caller can skip a
    /// notify when nothing on screen moves.
    pub(crate) fn set_load(&mut self, load: LibraryLoad) -> bool {
        if self.load == load {
            return false;
        }
        self.load = load;
        true
    }

    /// B16: `0 of 0` reads as "your library is empty" while the rows are still
    /// on the wire, so the count line names the wait instead.
    pub(crate) fn count_label(&self) -> String {
        match &self.load {
            LibraryLoad::Loading(_) if self.rows.is_empty() => "Loading…".to_string(),
            _ => search_count_text(self.visible.len(), self.rows.len()),
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Vec<ProxyConfig>) {
        let selected_id = self.selected_row().map(|row| row.id.clone());
        self.search_index = rows.iter().map(RowSearchIndex::of).collect();
        self.upstream_cache = rows
            .iter()
            .map(|row| {
                (
                    row.id.clone(),
                    upstream_host_port(row).unwrap_or_else(|| "no upstream".to_string()),
                )
            })
            .collect();
        self.rows = rows;

        let live: HashSet<&str> = self.rows.iter().map(|row| row.id.as_str()).collect();
        self.checked.retain(|id| live.contains(id.as_str()));

        self.endpoint_inputs
            .retain(|id, _| live.contains(id.as_str()));
        self.pending_endpoint_commits
            .retain(|id| live.contains(id.as_str()));
        self.refresh_search_words();
        self.apply_filter();
        self.selected = selected_id.and_then(|id| {
            self.visible
                .iter()
                .position(|&index| self.rows[index].id == id)
                .map(IndexPath::new)
        });
    }

    pub(crate) fn upstream_for(&self, id: &str) -> Option<&str> {
        self.upstream_cache.get(id).map(String::as_str)
    }

    pub(crate) fn ensure_row_inputs(
        &mut self,
        row_id: &str,
        bind_default: &str,
        port_default: &str,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if self.endpoint_inputs.contains_key(row_id) {
            return;
        }
        let bind = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("127.0.0.1")
                .default_value(bind_default.to_string())
        });
        let port = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("auto")
                .default_value(port_default.to_string())
        });
        let hosts = self.bind_hosts.clone();
        let picker = cx.new(|cx| {
            let choices = BindHostChoiceList::for_row(row_id, &hosts);
            let selected = choices
                .0
                .iter()
                .position(|choice| choice.value == bind_default)
                .map(IndexPath::new);
            SelectState::new(choices, selected, window, cx)
        });

        let mut subs = Vec::with_capacity(3);
        for state in [&bind, &port] {
            let id = row_id.to_string();
            subs.push(cx.subscribe(
                state,
                move |list: &mut ListState<Self>,
                      _emitter: Entity<InputState>,
                      event: &InputEvent,
                      cx: &mut Context<ListState<Self>>| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        let delegate = list.delegate_mut();
                        if !delegate.pending_endpoint_commits.contains(&id) {
                            delegate.pending_endpoint_commits.push(id.clone());
                        }
                        cx.notify();
                    }
                },
            ));
        }
        let pick_id = row_id.to_string();
        subs.push(cx.subscribe(
            &picker,
            move |list: &mut ListState<Self>,
                  _emitter: Entity<SelectState<BindHostChoiceList>>,
                  event: &SelectEvent<BindHostChoiceList>,
                  cx: &mut Context<ListState<Self>>| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                list.delegate_mut().queue_bind_pick(&pick_id, value);
                cx.notify();
            },
        ));
        self.endpoint_inputs.insert(
            row_id.to_string(),
            RowEndpointInputs {
                bind,
                port,
                picker,
                picker_hosts: hosts,
                picker_selected: Some(bind_default.to_string()),
                pick_pending: None,
                _subs: subs,
                clean_bind: bind_default.to_string(),
                clean_port: port_default.to_string(),
            },
        );
    }

    /// T-33: one confirmed pick — from the picker's Confirm event, or a test
    /// driving the same seam. The host lands in the bind draft, and the row is
    /// queued for the ordinary endpoint commit so `save_listen_gate` runs on it.
    pub(crate) fn queue_bind_pick(&mut self, row_id: &str, host: &str) {
        if let Some(inputs) = self.endpoint_inputs.get_mut(row_id) {
            inputs.pick_pending = Some(host.to_string());
        }
        if !self.pending_endpoint_commits.iter().any(|id| id == row_id) {
            self.pending_endpoint_commits.push(row_id.to_string());
        }
    }

    pub(crate) fn sync_row_inputs(
        &mut self,
        row_id: &str,
        bind_stored: &str,
        port_stored: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let hosts = self.bind_hosts.clone();
        let Some(inputs) = self.endpoint_inputs.get_mut(row_id) else {
            return;
        };
        if let Some(pick) = inputs.pick_pending.take() {
            if inputs.bind.read(cx).value().as_str() != pick {
                inputs
                    .bind
                    .update(cx, |state, cx| state.set_value(pick.clone(), window, cx));
            }
        }
        let bind_current = inputs.bind.read(cx).value().to_string();
        let port_current = inputs.port.read(cx).value().to_string();
        if !inputs.bind.focus_handle(cx).is_focused(window)
            && bind_current != bind_stored
            && bind_current == inputs.clean_bind
        {
            inputs.bind.update(cx, |state, cx| {
                state.set_value(bind_stored.to_string(), window, cx)
            });
            inputs.clean_bind = bind_stored.to_string();
        }
        if !inputs.port.focus_handle(cx).is_focused(window)
            && port_current != port_stored
            && port_current == inputs.clean_port
        {
            inputs.port.update(cx, |state, cx| {
                state.set_value(port_stored.to_string(), window, cx)
            });
            inputs.clean_port = port_stored.to_string();
        }
        if inputs.picker_hosts != hosts {
            inputs.picker_hosts = hosts.clone();
            inputs.picker_selected = None;
            let choices = BindHostChoiceList::for_row(row_id, &hosts);
            inputs
                .picker
                .update(cx, |state, cx| state.set_items(choices, window, cx));
        }
        // A bind the list no longer holds (a hand-edited row) leaves the picker
        // empty rather than inventing an entry — the list stays the authority.
        let current = inputs.bind.read(cx).value().to_string();
        if inputs.picker_selected.as_deref() != Some(current.as_str())
            && hosts.iter().any(|host| host == &current)
        {
            inputs.picker.update(cx, |state, cx| {
                state.set_selected_value(&current, window, cx)
            });
            inputs.picker_selected = Some(current);
        } else if !hosts.iter().any(|host| host == &current) && inputs.picker_selected.is_some() {
            // The selected value left the list: drop the selection so the
            // picker shows its placeholder instead of a vanished host.
            inputs
                .picker
                .update(cx, |state, cx| state.set_selected_index(None, window, cx));
            inputs.picker_selected = None;
        }
    }

    pub(crate) fn row_inputs_focused(&self, row_id: &str, window: &Window, cx: &App) -> bool {
        let Some(inputs) = self.endpoint_inputs.get(row_id) else {
            return false;
        };
        inputs.bind.focus_handle(cx).is_focused(window)
            || inputs.port.focus_handle(cx).is_focused(window)
    }

    pub(crate) fn endpoint_draft_texts(&self, row_id: &str, cx: &App) -> Option<(String, String)> {
        let inputs = self.endpoint_inputs.get(row_id)?;
        Some((
            inputs.bind.read(cx).value().to_string(),
            inputs.port.read(cx).value().to_string(),
        ))
    }

    pub(crate) fn endpoint_is_dirty(&self, row_id: &str, cx: &App) -> bool {
        let Some(row) = self.rows.iter().find(|row| row.id == row_id) else {
            return false;
        };
        let Some(inputs) = self.endpoint_inputs.get(row_id) else {
            return false;
        };
        inputs.bind.read(cx).value().as_str() != row.bind_ip.as_str()
            || inputs.port.read(cx).value().as_str() != row.port.to_string().as_str()
    }

    pub(crate) fn set_row_columns(&mut self, columns: RowColumns) {
        self.row_columns = columns;
    }

    /// T-03: the owner hands both readers the same geometry.
    pub(crate) fn set_row_metrics(&mut self, metrics: RowMetrics) {
        self.row_metrics = metrics;
    }

    pub(crate) fn revert_row_inputs(
        &mut self,
        row_id: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(row) = self.rows.iter().find(|row| row.id == row_id).cloned() else {
            return false;
        };
        let port_stored = row.port.to_string();
        let Some(inputs) = self.endpoint_inputs.get_mut(row_id) else {
            return false;
        };
        let dirty = inputs.bind.read(cx).value().as_str() != row.bind_ip.as_str()
            || inputs.port.read(cx).value().as_str() != port_stored.as_str();
        if dirty {
            inputs.bind.update(cx, |state, cx| {
                state.set_value(row.bind_ip.clone(), window, cx)
            });
            inputs.port.update(cx, |state, cx| {
                state.set_value(port_stored.clone(), window, cx)
            });
            inputs.clean_bind = row.bind_ip;
            inputs.clean_port = port_stored;
            inputs.pick_pending = None;
            self.pending_endpoint_commits.retain(|id| id != row_id);
        }
        dirty
    }

    pub(crate) fn apply_filter(&mut self) {
        let selected_id = self.selected_row().map(|row| row.id.clone());
        let (port, text) = split_port_query(self.query.trim());
        let lowered = text.to_lowercase();
        let mut visible = Vec::new();
        for (index, blob) in self.search_index.iter().enumerate() {
            if !blob.matches_lowered(&lowered) {
                continue;
            }
            if let Some(predicate) = port {
                let in_range = self
                    .rows
                    .get(index)
                    .is_some_and(|row| predicate.matches(row.port));
                if !in_range {
                    continue;
                }
            }
            if !self.row_matches_facets(index) {
                continue;
            }
            visible.push(index);
        }

        if !self.sort.is_none() {
            if self.sort.key == LibrarySortKey::TypeSecurity {
                // T-27: the retired toggle's ordering, unchanged — protocol
                // ascending, then transport security descending, then name.
                visible.sort_by(|&a, &b| compare_type_security(&self.rows[a], &self.rows[b]));
            } else {
                let context = SortContext {
                    latency: &self.latency,
                    data_traffic: &self.data_traffic,
                    totals: &self.traffic,
                    out_ips: &self.out_ips,
                };
                visible.sort_by(|&a, &b| {
                    compare_library_rows(&context, self.sort, &self.rows[a], &self.rows[b])
                });
            }
        }
        self.visible = visible;
        self.selected = selected_id.and_then(|id| {
            self.visible
                .iter()
                .position(|&index| self.rows[index].id == id)
                .map(IndexPath::new)
        });
    }

    pub(crate) fn row_matches_facets(&self, index: usize) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        if let Some(group) = &self.facet_group {
            if &row.group != group {
                return false;
            }
        }
        if let Some(status) = &self.facet_status {
            if !status.matches(row, &self.running, &self.health, &self.crashed) {
                return false;
            }
        }
        true
    }

    pub(crate) fn facets_active(&self) -> bool {
        self.facet_group.is_some() || self.facet_status.is_some()
    }

    pub(crate) fn active_filter_text(&self) -> String {
        let mut parts = Vec::new();
        if let Some(group) = &self.facet_group {
            parts.push(format!("group: {group}"));
        }
        if let Some(status) = &self.facet_status {
            parts.push(format!("status: {}", status.label()));
        }
        let query = self.query.trim();
        if !query.is_empty() {
            let (port, text) = split_port_query(query);
            if let Some(predicate) = port {
                parts.push(predicate.label());
            }
            let text = text.trim();
            if !text.is_empty() {
                parts.push(format!("search: \"{text}\""));
            }
        }
        parts.join(" · ")
    }

    pub(crate) fn bulk_label(&self) -> String {
        if self.checked.is_empty() {
            return String::new();
        }
        // F3: M is the visible row count, so it must read as the current
        // scope of the bar, never as the library total.
        format!(
            "{} checked · {} shown",
            self.checked.len(),
            self.visible.len()
        )
    }

    pub(crate) fn checked_rows(&self) -> Vec<ProxyConfig> {
        self.rows
            .iter()
            .filter(|row| self.checked.contains(&row.id))
            .cloned()
            .collect()
    }

    pub(crate) fn group_options(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for row in &self.rows {
            *counts.entry(row.group.as_str()).or_default() += 1;
        }
        let mut options: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(group, count)| (group.to_string(), count))
            .collect();
        options.sort();
        options
    }

    pub(crate) fn set_facet_group(&mut self, group: Option<String>) {
        self.facet_group = group;
        self.apply_filter();
    }

    /// K11/A2-1: the group dropdown owns both slots — one pick writes group
    /// and status disjointly in a single re-rank, so the menu can never show
    /// two active choices at once.
    pub(crate) fn set_facet_menu(&mut self, group: Option<String>, status: Option<FacetStatus>) {
        self.facet_group = group;
        self.facet_status = status;
        self.apply_filter();
    }

    pub(crate) fn set_facet_status(&mut self, status: Option<FacetStatus>) {
        self.facet_status = status;
        self.apply_filter();
    }

    pub(crate) fn set_show_security(&mut self, show: bool) {
        self.show_security = show;
    }

    /// T-27: swap the whole sort model (key + direction) and re-rank what the
    /// filters kept. The setter never persists — the owner (AppView) writes the
    /// meta row, matching how the column toggles work.
    pub(crate) fn set_sort(&mut self, sort: LibrarySort) {
        self.sort = sort;
        self.apply_filter();
    }

    pub(crate) fn set_latency(&mut self, latency: HashMap<String, u64>) {
        self.latency = latency;
        self.apply_filter();
    }

    pub(crate) fn set_data_traffic(&mut self, data_traffic: HashMap<String, (u64, u64)>) {
        self.data_traffic = data_traffic;
        self.apply_filter();
    }

    pub(crate) fn set_out_ips(&mut self, out_ips: HashMap<String, String>) {
        self.out_ips = out_ips;
        self.apply_filter();
    }

    pub(crate) fn clear_facets(&mut self) {
        self.facet_group = None;
        self.facet_status = None;
        self.apply_filter();
    }

    pub(crate) fn toggle_checked(&mut self, id: &str) {
        if self.checked.contains(id) {
            self.checked.remove(id);
            return;
        }
        if self.rows.iter().any(|row| row.id == id) {
            self.checked.insert(id.to_string());
        }
    }

    pub(crate) fn check_ids(&mut self, ids: &[String]) {
        for id in ids {
            if self.rows.iter().any(|row| &row.id == id) {
                self.checked.insert(id.clone());
            }
        }
    }

    pub(crate) fn clear_checked(&mut self) {
        self.checked.clear();
    }

    /// K4: rows no longer paint numeral chips, but the checked set keeps its
    /// insertion order and the tests pin it through this helper.
    #[cfg(test)]
    pub(crate) fn checked_position(&self, id: &str) -> Option<usize> {
        self.rows
            .iter()
            .filter(|row| self.checked.contains(&row.id))
            .position(|row| row.id == id)
            .map(|position| position + 1)
    }

    pub(crate) fn visible_rows(&self) -> Vec<ProxyConfig> {
        self.visible
            .iter()
            .filter_map(|&index| self.rows.get(index).cloned())
            .collect()
    }

    /// Borrowed row for per-frame rendering: `render_item` copies only the
    /// fields it needs instead of cloning a whole config every frame.
    pub(crate) fn row_ref(&self, index: IndexPath) -> Option<&ProxyConfig> {
        self.visible
            .get(index.row)
            .and_then(|&row| self.rows.get(row))
    }

    pub(crate) fn row_at(&self, index: IndexPath) -> Option<ProxyConfig> {
        self.row_ref(index).cloned()
    }

    pub(crate) fn selected_row(&self) -> Option<ProxyConfig> {
        self.selected.and_then(|index| self.row_at(index))
    }

    pub(crate) fn set_health(&mut self, health: HashMap<String, String>) {
        self.health = health;
        self.refresh_search_words();
        self.apply_filter();
    }

    pub(crate) fn set_health_tone(&mut self, health_tone: HashMap<String, LatencyTone>) {
        self.health_tone = health_tone;
    }

    pub(crate) fn set_running(&mut self, ids: Vec<String>) {
        let next: HashSet<String> = ids.into_iter().collect();
        // B18: one id gained or lost is a single-row start or stop — the row
        // the user just acted on, and the only row the accent marks. Any other
        // shape (a tick that changes nothing, or a bulk op that moves several)
        // retires the marker instead of re-running it.
        let changed: Vec<String> = next.symmetric_difference(&self.running).cloned().collect();
        self.settled = match changed.as_slice() {
            [id] => {
                self.settle_seq = self.settle_seq.wrapping_add(1);
                Some((id.clone(), self.settle_seq))
            }
            _ => None,
        };
        self.running = next;
        self.refresh_search_words();
        self.apply_filter();
    }

    /// B18: the accent strip for the row the engine just started or stopped —
    /// the same 3px leading edge the checked set uses, so it costs the row no
    /// geometry at all. `None` for every other row, and for every row under
    /// reduced motion.
    fn row_accent(&self, row_id: &str, cx: &App) -> Option<impl IntoElement> {
        let (_, seq) = self.settled.as_ref().filter(|(id, _)| id == row_id)?;
        AppView::motion_accent(
            div()
                .id(format!("row-accent-{row_id}"))
                .test_support()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(3.))
                .bg(cx.theme().accent),
            ("row-accent", *seq as usize),
            cx,
        )
    }

    pub(crate) fn set_crashed(&mut self, ids: Vec<String>) {
        self.crashed = ids.into_iter().collect();
    }

    pub(crate) fn set_traffic(&mut self, traffic: HashMap<String, (u64, u64)>) {
        self.traffic = traffic;
        self.apply_filter();
    }

    pub(crate) fn set_search_context(
        &mut self,
        sub_labels: HashMap<String, String>,
        preset_names: HashMap<String, String>,
    ) {
        self.sub_labels = sub_labels;
        self.preset_names = preset_names;
        self.refresh_search_words();
        self.apply_filter();
    }

    pub(crate) fn refresh_search_words(&mut self) {
        for (row, entry) in self.rows.iter().zip(self.search_index.iter_mut()) {
            entry.enrich(
                row,
                &self.sub_labels,
                &self.preset_names,
                &self.running,
                &self.health,
            );
        }
    }

    pub(crate) fn counts(&self) -> (usize, usize) {
        (
            self.rows.len(),
            self.rows.iter().filter(|row| row.enabled).count(),
        )
    }

    #[cfg(test)]
    pub(crate) fn visible_counts(&self) -> (usize, usize) {
        (self.visible.len(), self.rows.len())
    }

    pub(crate) fn delegate_counts_for_search(&self) -> (usize, usize, String) {
        (self.visible.len(), self.rows.len(), self.query.clone())
    }
}

pub(crate) fn select_first_visible(
    state: &mut ListState<LibraryDelegate>,
    ids: &[String],
    window: &mut Window,
    cx: &mut Context<ListState<LibraryDelegate>>,
) -> bool {
    let picked = {
        let delegate = state.delegate_mut();
        delegate.visible.iter().position(|&visible_ix| {
            delegate
                .rows
                .get(visible_ix)
                .is_some_and(|row| ids.contains(&row.id))
        })
    };
    match picked {
        Some(position) => {
            state.set_selected_index(Some(IndexPath::new(position)), window, cx);
            true
        }
        None => false,
    }
}

pub(crate) struct LogLinesDelegate {
    pub(crate) lines: Vec<String>,
}

impl ListDelegate for LogLinesDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.lines.len()
    }

    fn set_selected_index(
        &mut self,
        _index: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
    }

    fn render_item(
        &mut self,
        index: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let line = self.lines.get(index.row)?.clone();
        let tip = line.clone();
        let tone = log_level_text_color(log_line_level(&line), cx.theme());
        Some(
            ListItem::new(format!("log-line-{}", index.row)).child(
                div()
                    .id(format!("log-line-text-{}", index.row))
                    .text_sm()
                    .text_color(tone)
                    .truncate()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                    .child(line),
            ),
        )
    }
}

impl ListDelegate for LibraryDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.visible.len()
    }

    fn set_selected_index(
        &mut self,
        index: Option<IndexPath>,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = index;
        // F19: picking a row opens the inspector again — it is the only way
        // back to the row's controls once Esc closed it. The update is
        // deferred: the selection often changes inside the shell's own
        // update, and an entity cannot be updated while it is updating.
        if index.is_some() {
            if let Some(app_view) = self.app_view.clone() {
                window.defer(cx, move |_, cx| {
                    app_view.update(cx, |view, _| {
                        view.open_inspector();
                    });
                });
            }
        }
    }

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.query = query.to_string();
        self.apply_filter();
        cx.notify();
        Task::ready(())
    }

    fn render_item(
        &mut self,
        index: IndexPath,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.row_ref(index)?;
        let row_id = row.id.clone();
        let row_name = row.name.clone();
        let row_protocol = row.protocol.clone();
        let row_bind_ip = row.bind_ip.clone();
        let row_port = row.port;
        let port_stored = row_port.to_string();
        let selected = self
            .selected
            .map(|current| current.section == index.section && current.row == index.row)
            .unwrap_or(false);

        let health = self
            .health
            .get(&row_id)
            .filter(|summary| !summary.is_empty())
            .cloned();
        let is_running = self.running.contains(&row_id);

        let traffic = self.traffic.get(&row_id).copied();
        let upstream = self
            .upstream_for(&row_id)
            .unwrap_or("no upstream")
            .to_string();
        // Everything below the helpers mutates the delegate, so the row borrow
        // ends here; the copies above are the fields the frame actually paints.
        let full_route = library_row_full_route(row, &upstream);
        let [upstream_text, health_text, group_text, traffic_text] = library_row_cells(
            row,
            &upstream,
            health.as_deref(),
            is_running,
            traffic,
            self.show_security,
        );

        let list_view = cx.entity();
        // The checkbox's and the lamp's handlers are separate closures; both
        // update this list state, so both need their own handle.
        let lamp_view = list_view.clone();
        let lamp_app = self.app_view.clone();
        let is_checked = self.checked.contains(&row_id);
        let toggle_id = row_id.clone();
        let checkbox = Checkbox::new(format!("check-{row_id}"))
            .small()
            .checked(is_checked)
            .accessibility_label(if is_checked {
                format!("Checked {} (bulk action target)", row_name)
            } else {
                format!("Check {} for bulk actions", row_name)
            })
            .on_change(move |_, _, cx| {
                let toggle_id = toggle_id.clone();
                list_view.update(cx, |state, cx| {
                    state.delegate_mut().toggle_checked(&toggle_id);
                    cx.notify();
                });
                // F3: checking is a bulk-set edit, not a selection gesture.
                // The parent list item would otherwise select the row and
                // move the inspector on every check/uncheck.
                cx.stop_propagation();
            });
        // The kit checkbox owns no observed element id, so tests click this
        // content-sized wrapper; it only exists in the test build.
        #[cfg(test)]
        let checkbox = div()
            .id(format!("check-target-{row_id}"))
            .test_support()
            .flex()
            .child(checkbox);

        self.ensure_row_inputs(&row_id, &row_bind_ip, &port_stored, window, cx);
        self.sync_row_inputs(&row_id, &row_bind_ip, &port_stored, window, cx);

        if self.pending_endpoint_commits.contains(&row_id) {
            self.pending_endpoint_commits.retain(|id| id != &row_id);
            if let Some(app_view) = self.app_view.clone() {
                let commit_id = row_id.clone();
                window.defer(cx, move |window, cx| {
                    app_view.update(cx, |view, cx| {
                        view.commit_row_endpoint(&commit_id, window, cx);
                    });
                });
            }
        }

        let item = ListItem::new(format!("library-row-{}", row_id))
            .selected(selected)
            .px_1()
            // T-03: the row carries a definite height and the card leaves a
            // hairline of room above and below it, so the pitch is fixed and
            // two cards never touch.
            .py(px(RowMetrics::CARD_GAP))
            .h(px(RowMetrics::ROW_PITCH));

        let selected_bg = if cx.theme().list.active_highlight {
            cx.theme().list_active
        } else {
            cx.theme().accent
        };
        // T-03: one line per config. The card carries a definite height, so
        // a long name or a crowded column truncates instead of stacking the
        // row into a column of cards.
        let mut card = div()
            .id(format!("row-card-{row_id}"))
            .test_support()
            .h(px(RowMetrics::CARD_HEIGHT))
            .px_2()
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .bg(if selected {
                selected_bg
            } else {
                cx.theme().group_box
            });

        // K4: the checked set no longer borrows selection's rim. The card keeps
        // its hairline and carries a 3px signal-blue bar on the leading edge;
        // the kit checkbox fills when checked. Selection alone keeps its 2px
        // rim, so the two grammars never collide.
        card = card.relative().border_1().border_color(cx.theme().border);
        if is_checked {
            card = card.child(
                div()
                    .id(format!("row-check-bar-{row_id}"))
                    .test_support()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .bg(cx.theme().list_active_border),
            );
        } else if let Some(accent) = self.row_accent(&row_id, cx) {
            // B18: one accent blink on the row whose engine state just settled.
            // Checked wins the edge — one grammar per row.
            card = card.child(accent);
        }
        if !selected {
            card = card.hover(|this| this.bg(cx.theme().list_hover));
        }
        // T-06: focus is the kit's ring, not a hand-rolled accent rim. The
        // keyboard lands on the list, so the selected row wears the ring while
        // that handle is focused.
        if selected
            && self
                .list_focus
                .as_ref()
                .is_some_and(|handle| handle.is_focused(window))
        {
            card = card.focus_ring_style(window, cx);
        }
        let tone = serving_tone(
            is_running,
            health.as_deref(),
            self.crashed.contains(&row_id),
        );
        let probe_tone = self
            .health_tone
            .get(&row_id)
            .copied()
            .unwrap_or(LatencyTone::Unknown);
        let health_text_color = health_tone_color(probe_tone, cx);
        let lamp_color = match tone {
            ServingTone::Failed => cx.theme().danger,
            ServingTone::Serving => cx.theme().success,
            ServingTone::Stopped => cx.theme().muted_foreground,
        };
        let lamp_label = lamp_access_label(tone, &row_name);
        // K11/A2-1: the occupancy lamp doubles as the health filter's toggle —
        // its tone names the verdict a click filters by.
        let lamp_status = match tone {
            ServingTone::Serving => FacetStatus::Running,
            ServingTone::Failed => FacetStatus::Dead,
            ServingTone::Stopped => FacetStatus::Stopped,
        };
        let lamp_active = self.facet_status == Some(lamp_status);
        let lamp_hint = if lamp_active {
            format!("{lamp_label} · click to stop filtering")
        } else {
            format!(
                "{lamp_label} · click to show only {} rows",
                lamp_status.label().to_lowercase()
            )
        };
        {
            let name = row_name.clone();
            let metrics = self.row_metrics;
            let mut cells: Vec<AnyElement> = Vec::new();
            if metrics.show_group {
                cells.push(
                    div()
                        .id(format!("row-group-{row_id}"))
                        .test_support()
                        .w(px(metrics.group))
                        .flex_shrink_0()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(group_text.clone())
                        .into_any_element(),
                );
            }
            if metrics.show_protocol {
                cells.push(
                    div()
                        .id(format!("row-protocol-{row_id}"))
                        .test_support()
                        .w(px(metrics.protocol))
                        .flex_shrink_0()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(row_protocol)
                        .into_any_element(),
                );
            }
            if metrics.show_upstream {
                let tip = upstream_text.clone();
                cells.push(
                    div()
                        .id(format!("row-upstream-{row_id}"))
                        .test_support()
                        .w(px(metrics.upstream))
                        .flex_shrink_0()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .tooltip({
                            let tip = tip.clone();
                            move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                        })
                        .child(upstream_text.clone())
                        .into_any_element(),
                );
            }
            // T-03: the endpoint is read, not edited — the bind rides as
            // muted small text and the port as the quiet `row-port-<id>` pill.
            cells.push(
                div()
                    .id(format!("row-endpoint-{row_id}"))
                    .test_support()
                    .w(px(metrics.endpoint))
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .id(format!("row-bind-text-{row_id}"))
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .tooltip({
                                let tip = format!("Serve bind address {}", row_bind_ip);
                                move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                            })
                            .child(row_bind_ip.clone()),
                    )
                    .child(
                        div()
                            .id(format!("row-port-{row_id}"))
                            .test_support()
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .tooltip({
                                let tip = format!(
                                    "Local endpoint {}:{} — edit it in the inspector",
                                    row_bind_ip, row_port
                                );
                                move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                            })
                            .aria_label(format!("Local port {} on {}", row_port, row_bind_ip))
                            .child(format!(":{}", row_port)),
                    )
                    .into_any_element(),
            );
            if metrics.show_health {
                let tip = health_text.clone();
                cells.push(
                    div()
                        .id(format!("row-health-{row_id}"))
                        .test_support()
                        .w(px(metrics.health))
                        .flex_shrink_0()
                        .truncate()
                        .text_sm()
                        .text_color(health_text_color)
                        .tooltip({
                            let tip = tip.clone();
                            move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                        })
                        .child(health_text.clone())
                        .into_any_element(),
                );
            }
            if metrics.show_traffic {
                cells.push(
                    div()
                        .id(format!("row-traffic-{row_id}"))
                        .test_support()
                        .w(px(metrics.traffic))
                        .flex_shrink_0()
                        .truncate()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(traffic_text.clone())
                        .into_any_element(),
                );
            }
            // T-03: the trailing cell holds the row-scoped copy action; the
            // header strip reserves the same width, so the columns line up.
            let copy_view = self.app_view.clone();
            let copy_id = row_id.clone();
            let copy_name = row_name;
            let copy_bind = row_bind_ip;
            let copy_port = row_port;
            cells.push(
                div()
                    .id(format!("row-copy-cell-{row_id}"))
                    .test_support()
                    .w(px(metrics.copy))
                    .flex_shrink_0()
                    .child(
                        Button::new(format!("row-copy-{row_id}"))
                            .small()
                            // Pinned row test: 72px cell. The default small
                            // padding clipped the label; compact keeps the
                            // full "Copy SOCKS" inside the cell.
                            .compact()
                            .label("Copy SOCKS")
                            .accessibility_label(format!("Copy SOCKS URL for {copy_name}"))
                            .tooltip(format!(
                                "Copy this row's SOCKS proxy URL only — socks5://{copy_bind}:{copy_port}"
                            ))
                            .on_click(move |_, window, cx| {
                                if let Some(app) = copy_view.clone() {
                                    app.update(cx, |view, cx| {
                                        view.copy_row_socks(&copy_id, window, cx);
                                    });
                                }
                            }),
                    )
                    .into_any_element(),
            );
            let header = div()
                .flex()
                .flex_row()
                .flex_nowrap()
                .items_center()
                .gap_2()
                .w_full()
                .h_full()
                .child(
                    div()
                        .w(px(metrics.lamp_check))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(2.))
                        .child(
                            div()
                                .id(format!("row-dot-{row_id}"))
                                .test_support()
                                .size(px(12.))
                                .rounded_full()
                                .bg(lamp_color)
                                .cursor_pointer()
                                .tooltip({
                                    let tip = lamp_hint.clone();
                                    move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                                })
                                .aria_label(lamp_hint.clone())
                                .on_click(move |_, window, cx| {
                                    // The lamp owns its click; the card's
                                    // double-click-to-edit must not swallow it.
                                    cx.stop_propagation();
                                    let status = lamp_status;
                                    lamp_view.update(cx, |state, cx| {
                                        let delegate = state.delegate_mut();
                                        if delegate.facet_status == Some(status) {
                                            delegate.set_facet_status(None);
                                        } else {
                                            delegate.set_facet_status(Some(status));
                                        }
                                        cx.notify();
                                    });
                                    // Keep the one facet menu honest: the lamp is
                                    // its second input, so the collapsed display
                                    // must re-sync in the same click.
                                    if let Some(app) = lamp_app.as_ref() {
                                        app.update(cx, |view, cx| {
                                            view.sync_facet_pickers(window, cx);
                                        });
                                    }
                                }),
                        )
                        .child(checkbox),
                )
                // The check chip's slot stays reserved for the column grid;
                // K4 removed the numeral chip itself, so nothing paints here.
                .child(div().w(px(metrics.check_slot)).flex_shrink_0())
                .child(
                    div()
                        .id(format!("row-name-{row_id}"))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .child(
                            div()
                                .id(format!("row-name-text-{row_id}"))
                                .test_support()
                                .max_w(relative(1.))
                                .truncate()
                                // F14: the tooltip belongs to the text, not the
                                // flexible cell, so blank space after a short
                                // name does not open the full route.
                                .tooltip({
                                    let tip = full_route.clone();
                                    move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                                })
                                .child(Label::new(name)),
                        ),
                )
                .children(cells);
            card = card.child(header);
        }

        let rendered = match self.app_view.clone() {
            Some(app_view) => {
                let edit_view = app_view.clone();
                let edit_id = row_id.clone();
                let menu_view = app_view.clone();
                let menu_id = row_id.clone();
                item.child(
                    card.on_click(move |event, window, cx| {
                        if event.click_count() == 2 {
                            let id = edit_id.clone();
                            edit_view.update(cx, |view, cx| {
                                view.open_edit_for_row(&id, window, cx);
                            });
                        }
                    })
                    .context_menu(move |menu, window, cx| {
                        AppView::row_context_menu(menu, &menu_id, &menu_view, window, cx)
                    }),
                )
            }
            None => item.child(card),
        };
        Some(rendered)
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let list_view = cx.entity();
        // B16: with no rows at all, the list is either waiting or it failed —
        // neither may wear the "no configurations yet" or "no matches" copy.
        if self.rows.is_empty() {
            match self.load.clone() {
                LibraryLoad::Loading(label) => {
                    let mut column = div()
                        .id("library-loading")
                        .test_support()
                        .flex()
                        .flex_col()
                        .size_full()
                        .child(
                            div()
                                .id("library-loading-label")
                                .test_support()
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(label),
                        );
                    // The bars keep `ROW_PITCH`, so the first real row lands
                    // where the skeleton was instead of shoving the list down.
                    const BAR_W: [f32; 6] = [190., 250., 150., 300., 220., 170.];
                    for (ix, width) in BAR_W.into_iter().enumerate() {
                        column = column.child(
                            div()
                                .id(format!("skeleton-row-{ix}"))
                                .test_support()
                                .flex()
                                .flex_row()
                                .items_center()
                                .h(px(RowMetrics::ROW_PITCH))
                                .px_3()
                                .child(
                                    div()
                                        .w(px(width))
                                        .h(px(10.))
                                        .rounded(px(3.))
                                        .bg(cx.theme().muted),
                                ),
                        );
                    }
                    return column;
                }
                LibraryLoad::Failed(message) => {
                    let retry = self.app_view.clone();
                    return div()
                        .id("library-load-failed")
                        .test_support()
                        .flex()
                        .flex_col()
                        .size_full()
                        .child(
                            Empty::new()
                                .header(
                                    EmptyHeader::new()
                                        .title(
                                            EmptyTitle::new()
                                                .child("Could not load configurations"),
                                        )
                                        .description(EmptyDescription::new().child(message)),
                                )
                                .content(
                                    EmptyContent::new().child(
                                        Button::new("library-retry")
                                            .small()
                                            .label("Retry")
                                            .tooltip("Fetch the subscriptions again")
                                            .on_click(move |_, window, cx| {
                                                if let Some(view) = retry.clone() {
                                                    view.update(cx, |view, cx| {
                                                        view.retry_library_load(window, cx);
                                                    });
                                                }
                                            }),
                                    ),
                                ),
                        );
                }
                LibraryLoad::Ready => {}
            }
        }
        if !self.rows.is_empty() && self.visible.is_empty() {
            let (visible, total) = (self.visible.len(), self.rows.len());
            let mut description = search_count_text(visible, total);
            let filters = self.active_filter_text();
            if !filters.is_empty() {
                description.push_str(&format!(" · {filters}"));
            }
            let hint_view = list_view.clone();
            let all_view = list_view.clone();
            let hint_sync = self.app_view.clone();
            let all_sync = self.app_view.clone();
            return div()
                .id("library-empty")
                .test_support()
                .flex()
                .flex_col()
                .size_full()
                .child(
                Empty::new()
                    .header(
                        EmptyHeader::new()
                            .title(EmptyTitle::new().child(no_matches_text()))
                            .description(EmptyDescription::new().child(description)),
                    )
                    .content(
                        EmptyContent::new().child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(
                                    Button::new("clear-search-hint")

                                        .small()
                                        .label("Clear search")
                                        .tooltip("Clear the search query (keeps facet filters and checked)")
                                        .on_click(move |_, window, cx| {
                                            hint_view.update(cx, |state, list_cx| {





                                                state.set_query("", window, list_cx);
                                            });
                                            if let Some(app) = hint_sync.clone() {
                                                AppView::sync_search_box(&app, "", window, cx);
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("clear-search-facets")

                                        .small()
                                        .label("Clear all")
                                        .tooltip("Clear search and every filter")
                                        .on_click(move |_, window, cx| {
                                            all_view.update(cx, |state, list_cx| {
                                                state.set_query("", window, list_cx);
                                                state.delegate_mut().clear_facets();
                                                list_cx.notify();
                                            });
                                            if let Some(app) = all_sync.clone() {
                                                AppView::sync_search_box(&app, "", window, cx);
                                            }
                                        }),
                                ),
                        ),
                    ),
            );
        }

        // F18: with no configurations the empty canvas carries the two
        // intake actions under the sentence that names them.
        let empty = Empty::new().header(
            EmptyHeader::new()
                .title(EmptyTitle::new().child(empty_library_title()))
                .description(EmptyDescription::new().child(empty_library_text())),
        );
        let empty = match self.app_view.clone() {
            Some(app_view) => {
                empty.content(EmptyContent::new().child(AppView::render_empty_ctas(app_view)))
            }
            None => empty,
        };
        div()
            .id("library-empty")
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .child(empty)
    }
}

#[cfg(test)]
mod tests {
    use super::{FacetStatus, LibraryDelegate};
    use crate::store::ProxyConfig;
    use std::collections::{HashMap, HashSet};

    fn row(id: &str, name: &str, group: &str, enabled: bool) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{}".to_string().into(),
            group: group.to_string(),
            enabled,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    fn delegate(rows: Vec<ProxyConfig>) -> LibraryDelegate {
        let mut delegate = LibraryDelegate::new();
        delegate.set_rows(rows);
        delegate
    }

    /// Search and facet gating decide which rows the list paints, and the bulk
    /// bar's scope is the visible set — never the library total.
    #[test]
    fn filter_facets_and_bulk_scope_agree() {
        let mut delegate = delegate(vec![
            row("1", "alpha", "work", true),
            row("2", "beta", "home", false),
            row("3", "alpine", "work", true),
        ]);
        assert_eq!(delegate.visible_rows().len(), 3, "unfiltered shows all");

        delegate.query = "alp".to_string();
        delegate.apply_filter();
        assert_eq!(delegate.counts(), (3, 2), "counts ignore the filter");
        assert_eq!(
            delegate
                .visible_rows()
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "alpine"],
            "name search narrows the visible set"
        );

        delegate.set_facet_status(Some(FacetStatus::Disabled));
        assert!(delegate.visible_rows().is_empty(), "no disabled alp rows");

        delegate.clear_facets();
        delegate.query.clear();
        delegate.set_facet_group(Some("work".to_string()));
        delegate.check_ids(&["1".to_string(), "2".to_string()]);
        assert_eq!(
            delegate.bulk_label(),
            "2 checked · 2 shown",
            "checked counts the whole set, shown counts the visible scope"
        );
        assert_eq!(
            delegate.checked_position("1"),
            Some(1),
            "positions follow the library order"
        );
        assert_eq!(
            delegate
                .checked_rows()
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2"],
            "checked rows come back in library order"
        );
    }

    /// A status facet reads the same serving tone the row lamp paints, so a
    /// running row can never answer `Dead`.
    #[test]
    fn status_facet_matches_the_lamp_tone() {
        let row = row("1", "alpha", "work", true);
        let empty = HashSet::new();
        let no_health = HashMap::new();
        let running: HashSet<String> = ["1".to_string()].into_iter().collect();
        let dead: HashMap<String, String> = [("1".to_string(), "dead".to_string())]
            .into_iter()
            .collect();

        assert!(FacetStatus::Running.matches(&row, &running, &no_health, &empty));
        assert!(FacetStatus::Dead.matches(&row, &running, &dead, &empty));
        assert!(!FacetStatus::Running.matches(&row, &running, &dead, &empty));
        assert!(FacetStatus::Stopped.matches(&row, &empty, &no_health, &empty));
        assert!(FacetStatus::Enabled.matches(&row, &empty, &no_health, &empty));
        assert!(!FacetStatus::Disabled.matches(&row, &empty, &no_health, &empty));
    }
}
