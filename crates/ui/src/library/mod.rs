//! The library list delegate: rows, visibility, filtering, selection, row render.
//! Belongs: list state and the per-row element; it reads the store, never writes it.
//! Not here: dialogs or engine actions — those call back into `AppView`.
use crate::*;

use gpui_kit::component::list::ListState;
use gpui_kit::component::ThemeStyled as _;

pub mod columns;
pub mod shard;
pub mod tone;

pub use columns::*;
pub use shard::*;
pub use tone::*;

/// R17: the type scale is fixed at 14 / 13 / 12. The values are rems, so the
/// user's text-size setting still scales the table.
pub const ROW_NAME_REMS: f32 = 0.875;
pub const ROW_CELL_REMS: f32 = 0.8125;
pub const ROW_GROUP_REMS: f32 = 0.75;

/// One entry of the endpoint chip's chevron menu: a copy format and the row
/// it belongs to.
pub fn endpoint_copy_item(
    id: String,
    label: String,
    view: Option<Entity<AppView>>,
    row_id: String,
    act: fn(&mut AppView, &str, &mut Window, &mut Context<AppView>),
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        div()
            .id(id.clone())
            .test_support()
            .role(Role::MenuItem)
            .aria_label(label.clone())
            .child(label.clone())
    })
    .on_click(move |_, window, cx| {
        if let Some(app) = view.clone() {
            app.update(cx, |view, cx| act(view, &row_id, window, cx));
        }
    })
}

/// B16: what the list paints when it has no rows to show. A fetch that has not
/// landed is never "empty" — the list holds skeleton rows at the row pitch so
/// the first real row does not appear from nowhere — and a failed one names the
/// reason and carries the way back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LibraryLoad {
    #[default]
    Ready,
    Loading(String),
    Failed(String),
}

pub struct LibraryDelegate {
    /// B16: pushed by every refresh origin; the list never reads the app's
    /// in-flight set itself, because a frame is already borrowing that entity.
    pub load: LibraryLoad,
    pub rows: Vec<ProxyConfig>,
    pub search_index: Vec<RowSearchIndex>,
    pub upstream_cache: HashMap<String, String>,

    /// T-06: the list's own focus handle, handed in by the frame that owns the
    /// `ListState`. The selected row wears the kit focus ring while this handle
    /// is focused - the row card is the app's custom control, but the keyboard
    /// lands on the list.
    pub list_focus: Option<FocusHandle>,

    pub health: HashMap<String, String>,

    /// The probe's last failure cause per row. The health cell folds it into
    /// the two words it knows: `timeout` and `refused`.
    pub health_errors: HashMap<String, String>,

    pub health_tone: HashMap<String, LatencyTone>,

    pub running: HashSet<String>,

    pub crashed: HashSet<String>,

    pub traffic: HashMap<String, (u64, u64)>,

    pub sub_labels: HashMap<String, String>,
    pub preset_names: HashMap<String, String>,

    pub show_security: bool,

    /// T-27: the one sort concept — a key plus a direction, persisted in the
    /// `library_sort` meta key. Replaces the retired `type_security_sort` bool.
    pub sort: LibrarySort,

    /// T-27 mirrors the sort keys read on render. All three are pushed from
    /// state the app already holds (health store samples, session traffic
    /// mirror, out-IP resolutions) — the sort never polls.
    pub latency: HashMap<String, u64>,

    pub data_traffic: HashMap<String, (u64, u64)>,

    pub out_ips: HashMap<String, String>,

    /// T-03/R11: the geometry the header strip and the row both read. The
    /// owner resolves the app's one layout against the table width once per
    /// frame and hands the same snapshot to both readers.
    pub row_metrics: RowMetrics,

    /// R22: the density. It owns the row pitch and every chrome height, so
    /// the row, the header strip and the bars cannot disagree.
    pub density: Density,

    pub app_view: Option<Entity<AppView>>,

    /// One frame of the open endpoint draft, handed over by `AppView` before
    /// the list renders. Only the owning row paints it, so no per-row editor
    /// state survives the frame.
    pub endpoint_frame: Option<EndpointFrame>,

    /// B18: the one row whose engine state just settled, with a nonce that
    /// makes a second settle run the accent again instead of reusing the
    /// finished one. Only a single-id change sets it — bulk ops and the boot
    /// restore move several rows at once and stay still.
    pub settled: Option<(String, u64)>,
    pub settle_seq: u64,
}

impl LibraryDelegate {
    pub fn new() -> Self {
        Self {
            load: LibraryLoad::Ready,
            rows: Vec::new(),
            search_index: Vec::new(),
            upstream_cache: HashMap::new(),
            list_focus: None,
            health: HashMap::new(),
            health_errors: HashMap::new(),
            health_tone: HashMap::new(),
            running: HashSet::new(),
            crashed: HashSet::new(),
            traffic: HashMap::new(),
            sub_labels: HashMap::new(),
            preset_names: HashMap::new(),
            show_security: false,
            sort: LibrarySort::default(),
            latency: HashMap::new(),
            data_traffic: HashMap::new(),
            out_ips: HashMap::new(),
            row_metrics: RowMetrics::for_table_width(0., ColumnLayout::default()),
            density: Density::default(),
            app_view: None,
            endpoint_frame: None,
            settled: None,
            settle_seq: 0,
        }
    }

    /// B16: returns whether the state actually changed, so a caller can skip a
    /// notify when nothing on screen moves.
    pub fn set_load(&mut self, load: LibraryLoad) -> bool {
        if self.load == load {
            return false;
        }
        self.load = load;
        true
    }

    pub fn set_rows(&mut self, rows: Vec<ProxyConfig>) {
        self.search_index = rows.iter().map(RowSearchIndex::of).collect();
        self.upstream_cache = rows
            .iter()
            .map(|row| {
                (
                    row.id.clone(),
                    upstream_host_port(row).unwrap_or_else(|| t!("library-no-upstream")),
                )
            })
            .collect();
        self.rows = rows;

        self.refresh_search_words();
    }

    pub fn set_endpoint_frame(&mut self, frame: Option<EndpointFrame>) {
        self.endpoint_frame = frame;
    }

    /// R22: the density owns the row pitch and every chrome height.
    pub fn set_density(&mut self, density: Density) {
        self.density = density;
    }

    /// Every row's id, in library order.
    pub fn row_ids(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.id.clone()).collect()
    }

    pub fn upstream_for(&self, id: &str) -> Option<&str> {
        self.upstream_cache.get(id).map(String::as_str)
    }

    /// T-03/R11: the owner hands both readers the same geometry.
    pub fn set_row_metrics(&mut self, metrics: RowMetrics) {
        self.row_metrics = metrics;
    }

    pub fn set_show_security(&mut self, show: bool) {
        self.show_security = show;
    }

    /// T-27: swap the whole sort model (key + direction) and re-rank what the
    /// filters kept. The setter never persists — the owner (AppView) writes the
    /// meta row, matching how the column toggles work.
    pub fn set_sort(&mut self, sort: LibrarySort) {
        self.sort = sort;
    }

    pub fn set_latency(&mut self, latency: HashMap<String, u64>) {
        self.latency = latency;
    }

    pub fn set_data_traffic(&mut self, data_traffic: HashMap<String, (u64, u64)>) {
        self.data_traffic = data_traffic;
    }

    pub fn set_out_ips(&mut self, out_ips: HashMap<String, String>) {
        self.out_ips = out_ips;
    }

    /// Borrowed row for per-frame rendering: `render_item` copies only the
    /// fields it needs instead of cloning a whole config every frame. The
    /// visible order is the app's; the delegate only paints it.
    pub fn row_ref(&self, index: IndexPath, cx: &App) -> Option<&ProxyConfig> {
        let row = {
            let app = self.app_view.as_ref()?.read(cx);
            *app.visible.get(index.row)?
        };
        self.rows.get(row)
    }

    /// One row by id, borrowed.
    pub fn row_by_id(&self, id: &str) -> Option<&ProxyConfig> {
        self.rows.iter().find(|row| row.id == id)
    }

    pub fn set_health(&mut self, health: HashMap<String, String>) {
        self.health = health;
        self.refresh_search_words();
    }

    pub fn set_health_tone(&mut self, health_tone: HashMap<String, LatencyTone>) {
        self.health_tone = health_tone;
    }

    pub fn set_running(&mut self, ids: Vec<String>) {
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
    }

    /// B18: the accent strip for the row the engine just started or stopped —
    /// the same 3px leading edge the checked set uses, so it costs the row no
    /// geometry at all. `None` for every other row, and for every row under
    /// reduced motion.
    pub fn row_accent(&self, row_id: &str, cx: &App) -> Option<impl IntoElement> {
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

    pub fn set_crashed(&mut self, ids: Vec<String>) {
        self.crashed = ids.into_iter().collect();
    }

    pub fn set_traffic(&mut self, traffic: HashMap<String, (u64, u64)>) {
        self.traffic = traffic;
    }

    pub fn set_search_context(
        &mut self,
        sub_labels: HashMap<String, String>,
        preset_names: HashMap<String, String>,
    ) {
        self.sub_labels = sub_labels;
        self.preset_names = preset_names;
        self.refresh_search_words();
    }

    pub fn refresh_search_words(&mut self) {
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

    pub fn counts(&self) -> (usize, usize) {
        (
            self.rows.len(),
            self.rows.iter().filter(|row| row.enabled).count(),
        )
    }
}

/// Group names become stable ids by keeping ASCII alphanumerics and folding
/// every other run into one dash. Empty input still needs a usable id.
pub fn slugify_group_id(group: &str) -> String {
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

pub struct LogLinesDelegate {
    pub lines: Vec<String>,
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

    fn items_count(&self, _section: usize, cx: &App) -> usize {
        self.app_view
            .as_ref()
            .map_or(0, |view| view.read(cx).visible.len())
    }

    /// The kit owns one selected index and confirms it here. The selection
    /// lives on `AppView`, so this confirmation writes nothing: the app's
    /// own model decides what a click, a range or a keyboard move selects.
    fn set_selected_index(
        &mut self,
        _index: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
    }

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        if let Some(app) = self.app_view.clone() {
            let query = query.to_string();
            app.update(cx, |view, cx| view.apply_search_now(query, cx));
        }
        Task::ready(())
    }

    fn render_item(
        &mut self,
        index: IndexPath,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.row_ref(index, cx)?;
        let row_id = row.id.clone();
        let density = self.density;
        let metrics = self.row_metrics;
        let upstream = self
            .upstream_for(&row_id)
            .map(str::to_string)
            .unwrap_or_else(|| t!("library-no-upstream"));
        // The app owns the selection, the focus and the start-in-flight
        // signal. The delegate reads them and paints what they say.
        let (selected, focused_row, start_in_flight) = match self.app_view.as_ref() {
            Some(app_view) => {
                let view = app_view.read(cx);
                (
                    view.is_selected(&row_id),
                    view.is_focused(&row_id),
                    view.start_in_flight,
                )
            }
            None => (false, false, false),
        };
        // The pure row view: plain fields only, never the config's secret.
        let view = {
            let inputs = RowViewInputs {
                running: &self.running,
                crashed: &self.crashed,
                starting: start_in_flight,
                health: &self.health,
                health_errors: &self.health_errors,
                latency: &self.latency,
                traffic: &self.traffic,
                sub_labels: &self.sub_labels,
                preset_names: &self.preset_names,
            };
            RowView::of(row, &inputs)
        };
        let row_name = view.name.clone();
        let endpoint_editable = !is_custom_config_protocol(&view.protocol);
        let traffic_text = view
            .traffic
            .and_then(|(rx, tx)| traffic_summary_text(rx, tx))
            .unwrap_or_default();
        // Everything below the helpers mutates the delegate, so the row borrow
        // ends here; the copies above are the fields the frame actually paints.
        let full_route = library_row_full_route(row, &upstream);

        let item = ListItem::new(format!("library-row-{}", row_id))
            .selected(selected)
            .aria_selected(selected)
            // The content width is exact: the lead, the columns and the
            // trailing reserve fill it with no outer padding.
            .px_0()
            // The row is flat: a definite height and a hairline, never a card
            // with a gap under it.
            .py_0()
            .h(px(density.row_height()));

        let selected_bg = if cx.theme().list.active_highlight {
            cx.theme().list_active
        } else {
            cx.theme().accent
        };
        // The row is flat: no card, no shadow, no gap. Two rows are separated
        // by a hairline, and the row carries the hover group the trailing
        // actions watch.
        let mut surface = div()
            .id(format!("row-surface-{row_id}"))
            .test_support()
            .group(format!("row-hover-{row_id}"))
            .relative()
            // The surface fills the row's height on its own: the kit's item
            // centers its content, so a percentage height would collapse to
            // the content box.
            .h(px(density.row_height()))
            .w_full()
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border);

        // Selection paints a wash plus a rim over the whole row. Focus paints
        // the kit ring inside it — the two are independent reads, so a row that
        // holds the focus without being selected still shows its ring.
        if selected {
            surface = surface
                .bg(selected_bg)
                .border_1()
                .border_color(cx.theme().list_active_border);
        } else {
            surface = surface.hover(|this| this.bg(cx.theme().list_hover));
        }
        if focused_row
            && self
                .list_focus
                .as_ref()
                .is_some_and(|handle| handle.is_focused(window))
        {
            surface = surface.focus_ring_style(window, cx);
        }
        if let Some(accent) = self.row_accent(&row_id, cx) {
            // B18: one accent blink on the row whose engine state just settled.
            surface = surface.child(accent);
        }
        let probe_tone = self
            .health_tone
            .get(&row_id)
            .copied()
            .unwrap_or(LatencyTone::Unknown);
        // The cell text and the color carry the same verdict: a crashed row
        // never probed still reads `Failed` in the failure tone.
        let health_text_color = if view.state == RowPhase::Failed {
            health_tone_color(LatencyTone::Failed, cx.theme())
        } else {
            health_tone_color(probe_tone, cx.theme())
        };
        {
            let mut cells: Vec<AnyElement> = Vec::new();
            // The lead: one state glyph and nothing else. It reports what the
            // row is doing — it is not a button, so it has no hit target and
            // no hover state.
            cells.push(
                div()
                    .id(format!("row-lead-{row_id}"))
                    .test_support()
                    .w(px(metrics.lead))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    // The lead is status, never a control: it owns no action,
                    // so a click on it must not select the row either.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(state_glyph(view.state, cx.theme()))
                    .into_any_element(),
            );
            // Every column follows the user's order, Name included. The
            // wrapper owns the inter-column gaps so the lead and the trailing
            // reserve sit flush against the run, matching `content_width`.
            let mut column_cells: Vec<AnyElement> = Vec::new();
            for column in *metrics.layout.order() {
                if !metrics.column_visible(column) {
                    continue;
                }
                let width = metrics.column_width(column);
                let cell: AnyElement = match column {
                    ColumnId::Name => {
                        div()
                            .id(format!("row-name-{row_id}"))
                            .test_support()
                            .w(px(width))
                            .min_w_0()
                            .flex_shrink_0()
                            .flex()
                            .child(
                                div()
                                    .id(format!("row-name-text-{row_id}"))
                                    .test_support()
                                    .max_w(relative(1.))
                                    .truncate()
                                    .text_size(rems(ROW_NAME_REMS))
                                    .text_color(if view.enabled {
                                        cx.theme().foreground
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    // F14: the tooltip belongs to the text, not
                                    // the cell, so blank space after a short
                                    // name does not open the full route.
                                    .tooltip({
                                        let tip = full_route.clone();
                                        move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                                    })
                                    .child(Label::new(row_name.clone())),
                            )
                            .into_any_element()
                    }
                    ColumnId::Endpoint => {
                        let chip_view = self.app_view.clone();
                        let chip_id = row_id.clone();
                        let menu_view = self.app_view.clone();
                        let menu_id = row_id.clone();
                        let editing = self.endpoint_frame.as_ref().is_some_and(|frame| {
                            frame.row_id == row_id && frame.host == EndpointHost::RowPopover
                        });
                        let editor = editing
                            .then(|| self.endpoint_frame.clone().zip(self.app_view.clone()))
                            .flatten();
                        let pencil_view = self.app_view.clone();
                        let pencil_id = row_id.clone();
                        let pencil_name = row_name.clone();
                        let mut cell = div()
                            .id(format!("row-endpoint-{row_id}"))
                            .test_support()
                            .w(px(width))
                            .flex_shrink_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .overflow_hidden()
                            .child(
                                Button::new(format!("row-endpoint-chip-{row_id}"))
                                    .small()
                                    .compact()
                                    .label(view.endpoint_display())
                                    .accessibility_label(t!("endpoint-chip-tip"))
                                    .tooltip(t!("endpoint-chip-tip"))
                                    .on_click(move |_, window, cx| {
                                        // The chip owns its click: it copies,
                                        // and the row does not select.
                                        cx.stop_propagation();
                                        if let Some(app) = chip_view.clone() {
                                            app.update(cx, |view, cx| {
                                                view.copy_row_socks(&chip_id, window, cx);
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new(format!("row-endpoint-menu-{row_id}"))
                                    .small()
                                    .compact()
                                    .label("▾")
                                    .accessibility_label(t!("endpoint-chip-menu-aria"))
                                    .tooltip(t!("endpoint-chip-tip"))
                                    .dropdown_menu(move |menu, _, _| {
                                        let socks_view = menu_view.clone();
                                        let socks_id = menu_id.clone();
                                        let http_view = menu_view.clone();
                                        let http_id = menu_id.clone();
                                        let both_view = menu_view.clone();
                                        let both_id = menu_id.clone();
                                        menu.item(endpoint_copy_item(
                                            format!("row-endpoint-copy-socks-{menu_id}"),
                                            t!("copy-socks"),
                                            socks_view,
                                            socks_id,
                                            AppView::copy_row_socks,
                                        ))
                                        .item(endpoint_copy_item(
                                            format!("row-endpoint-copy-http-{menu_id}"),
                                            t!("copy-http"),
                                            http_view,
                                            http_id,
                                            AppView::copy_row_http,
                                        ))
                                        .item(
                                            endpoint_copy_item(
                                                format!("row-endpoint-copy-both-{menu_id}"),
                                                t!("copy-both"),
                                                both_view,
                                                both_id,
                                                AppView::copy_row_both,
                                            ),
                                        )
                                    }),
                            );
                        if endpoint_editable {
                            cell = match editor {
                                Some((frame, app)) => {
                                    cell.child(render_endpoint_popover(app, frame))
                                }
                                None => cell.child(
                                    div()
                                        .id(format!("row-endpoint-edit-slot-{row_id}"))
                                        .test_support()
                                        .opacity(if focused_row { 1. } else { 0. })
                                        .group_hover(format!("row-hover-{row_id}"), |style| {
                                            style.opacity(1.)
                                        })
                                        .child(
                                            Button::new(format!("row-endpoint-edit-{row_id}"))
                                                .small()
                                                .compact()
                                                .label("✎")
                                                .accessibility_label(t!(
                                                    "endpoint-edit-aria",
                                                    name = pencil_name
                                                ))
                                                .tooltip(t!("endpoint-edit-tip"))
                                                .on_click(move |_, window, cx| {
                                                    cx.stop_propagation();
                                                    if let Some(app) = pencil_view.clone() {
                                                        app.update(cx, |view, cx| {
                                                            view.open_endpoint_draft(
                                                                &pencil_id,
                                                                EndpointHost::RowPopover,
                                                                window,
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        ),
                                ),
                            };
                        }
                        cell.into_any_element()
                    }
                    ColumnId::Health => {
                        let health_text = view.health_text.clone();
                        let mut cell = div()
                            .id(format!("row-health-{row_id}"))
                            .test_support()
                            .w(px(width))
                            .flex_shrink_0()
                            .truncate()
                            .text_size(rems(ROW_CELL_REMS))
                            .text_color(health_text_color)
                            .child(health_text.clone());
                        // A stopped row's cell is empty by design; an empty
                        // tooltip is a popover that names nothing.
                        if !health_text.is_empty() {
                            let tip = health_text.clone();
                            cell = cell
                                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into());
                        }
                        cell.into_any_element()
                    }
                    ColumnId::Group => div()
                        .id(format!("row-group-{row_id}"))
                        .test_support()
                        .w(px(width))
                        .flex_shrink_0()
                        .truncate()
                        .text_size(rems(ROW_GROUP_REMS))
                        .text_color(cx.theme().muted_foreground)
                        .child(view.group.clone())
                        .into_any_element(),
                    ColumnId::Protocol => div()
                        .id(format!("row-protocol-{row_id}"))
                        .test_support()
                        .w(px(width))
                        .flex_shrink_0()
                        .truncate()
                        .text_size(rems(ROW_CELL_REMS))
                        .text_color(cx.theme().muted_foreground)
                        .child(view.protocol.clone())
                        .into_any_element(),
                    ColumnId::Upstream => div()
                        .id(format!("row-upstream-{row_id}"))
                        .test_support()
                        .w(px(width))
                        .flex_shrink_0()
                        .truncate()
                        .text_size(rems(ROW_CELL_REMS))
                        .text_color(cx.theme().muted_foreground)
                        .child(upstream.clone())
                        .into_any_element(),
                    ColumnId::Traffic => div()
                        .id(format!("row-traffic-{row_id}"))
                        .test_support()
                        .w(px(width))
                        .flex_shrink_0()
                        .truncate()
                        .text_size(rems(ROW_CELL_REMS))
                        .text_color(cx.theme().muted_foreground)
                        .child(traffic_text.clone())
                        .into_any_element(),
                };
                column_cells.push(cell);
            }
            cells.push(
                div()
                    .id(format!("row-columns-{row_id}"))
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(RowMetrics::GAP))
                    .flex_shrink_0()
                    .children(column_cells)
                    .into_any_element(),
            );
            // The trailing reserve. It is always there, so the grid never
            // shifts; the buttons appear on hover or keyboard focus.
            let transport_view = self.app_view.clone();
            let transport_id = row_id.clone();
            let transport: AnyElement = match view.state {
                RowPhase::Starting => div()
                    .id(format!("row-start-{row_id}"))
                    .test_support()
                    .w(px(22.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Spinner::new().small())
                    .into_any_element(),
                RowPhase::Running => {
                    let stop_view = transport_view.clone();
                    let stop_id = transport_id.clone();
                    Button::new(format!("row-stop-{row_id}"))
                        .small()
                        .compact()
                        .label(t!("action-stop"))
                        .accessibility_label(t!("action-stop"))
                        .on_click(move |_, window, cx| {
                            if let Some(app) = stop_view.clone() {
                                app.update(cx, |view, cx| view.stop_row(&stop_id, window, cx));
                            }
                        })
                        .into_any_element()
                }
                _ => {
                    let start_view = transport_view.clone();
                    let start_id = transport_id.clone();
                    Button::new(format!("row-start-{row_id}"))
                        .small()
                        .compact()
                        .label(t!("action-start"))
                        .accessibility_label(t!("action-start"))
                        .on_click(move |_, window, cx| {
                            if let Some(app) = start_view.clone() {
                                app.update(cx, |view, cx| view.start_row(&start_id, window, cx));
                            }
                        })
                        .into_any_element()
                }
            };
            let more_view = self.app_view.clone();
            let more_id = row_id.clone();
            let mut actions = div()
                .id(format!("row-actions-{row_id}"))
                .test_support()
                .w(px(metrics.actions))
                .flex_shrink_0()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap_1()
                .opacity(0.)
                .group_hover(format!("row-hover-{row_id}"), |style| style.opacity(1.));
            if focused_row {
                actions = actions.opacity(1.);
            }
            cells.push(
                actions
                    .child(transport)
                    .child(
                        Button::new(format!("row-more-{row_id}"))
                            .small()
                            .compact()
                            .label("⋯")
                            .accessibility_label(t!("row-more-aria"))
                            .tooltip(t!("row-more-tip"))
                            .dropdown_menu(move |menu, window, cx| match more_view.clone() {
                                Some(app) => {
                                    AppView::row_more_menu(menu, &more_id, &app, window, cx)
                                }
                                None => menu,
                            }),
                    )
                    .into_any_element(),
            );
            surface = surface.children(cells);
        }

        let right_view = self.app_view.clone();
        let right_id = row_id.clone();
        let click_view = self.app_view.clone();
        let click_id = row_id.clone();

        let rendered = match self.app_view.clone() {
            Some(app_view) => {
                let edit_view = app_view.clone();
                let edit_id = row_id.clone();
                let menu_view = app_view.clone();
                let menu_id = row_id.clone();
                let drag_view = app_view.clone();
                let drag_name = row_name.clone();
                item.child(
                    surface
                        // The row is the drag source; the nav's group entries are
                        // the drop targets. The kit's drag threshold keeps the
                        // row's click and selection intact.
                        .on_drag(RowDrag { id: row_id.clone() }, move |_, _, _, cx| {
                            drag_view.update(cx, |view, cx| view.clear_row_drop(cx));
                            cx.new(|_| RowDragPreview {
                                label: drag_name.clone(),
                            })
                        })
                        // The row's own click. It lives on the surface, not on
                        // the list item, so the endpoint chip keeps its own
                        // click: the chip copies, the rest of the row selects.
                        .on_click(move |event, window, cx| {
                            let modifiers = event.modifiers();
                            let id = click_id.clone();
                            if let Some(app) = click_view.clone() {
                                app.update(cx, |view, cx| {
                                    if modifiers.shift {
                                        view.extend_to_row(&id, cx);
                                    } else if modifiers.secondary() {
                                        view.toggle_row(&id, cx);
                                    } else {
                                        view.select_row(&id, cx);
                                    }
                                    // F19: picking a row opens the inspector
                                    // again — it is the only way back to the
                                    // row's controls once Esc closed it.
                                    view.open_inspector();
                                });
                            }
                            if event.click_count() == 2 {
                                let id = edit_id.clone();
                                edit_view.update(cx, |view, cx| {
                                    view.open_edit_for_row(&id, window, cx);
                                });
                            }
                            // The table's blank area clears the selection; a
                            // click that landed on a row is not blank.
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                            if let Some(app) = right_view.clone() {
                                app.update(cx, |view, cx| {
                                    // Right-clicking outside the set makes that
                                    // row the subject; right-clicking inside a
                                    // multi-selection keeps the set whole.
                                    if !view.is_selected(&right_id) {
                                        view.select_row(&right_id, cx);
                                    }
                                });
                            }
                        })
                        .context_menu(move |menu, window, cx| {
                            AppView::row_more_menu(menu, &menu_id, &menu_view, window, cx)
                        }),
                )
            }
            None => item.child(surface),
        };
        Some(rendered)
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        // B16: with no rows at all, the list is either waiting or it failed -
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
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    div()
                                        .id("library-loading-spinner")
                                        .test_support()
                                        .child(Spinner::new().small()),
                                )
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
                                .h(px(self.density.row_height()))
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
                                        .title(EmptyTitle::new().child(t!("library-load-failed")))
                                        .description(EmptyDescription::new().child(message)),
                                )
                                .content(
                                    EmptyContent::new().child(
                                        Button::new("library-retry")
                                            .small()
                                            .label(t!("library-retry"))
                                            .tooltip(t!("library-retry-tip"))
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
        let visible = self
            .app_view
            .as_ref()
            .map_or(0, |view| view.read(cx).visible.len());
        if !self.rows.is_empty() && visible == 0 {
            let description = search_count_text(visible, self.rows.len());
            let clear = self.app_view.clone();
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
                                Button::new("library-clear-filters")
                                    .small()
                                    .label(t!("filter-clear-all"))
                                    .tooltip(t!("filter-clear-all-tip"))
                                    .on_click(move |_, window, cx| {
                                        if let Some(view) = clear.clone() {
                                            view.update(cx, |view, cx| {
                                                view.clear_query(window, cx);
                                            });
                                        }
                                    }),
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
