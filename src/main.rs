#![recursion_limit = "512"]

mod engine;
mod health;
mod import;
mod store;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::{Dialog, DialogButtonProps};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::label::Label;
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::TitleBar;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt};
use gpui_kit::*;

use engine::{
    lan_confirm_description, lan_confirm_text, lan_exposure_warning, requires_lan_confirm,
    EngineController, LAN_CONFIRM_TITLE,
};
use health::{
    ping_all, test_all, HealthProber, HealthStore, LogStore, RealHealthProber, TestOutcome,
    DEFAULT_TEST_URL,
};
use import::{
    commit_batch, effective_hint_for_text, format_import_failures, import_file_content,
    import_multi_file_contents, import_text, is_addsub_attempt, parse_addsub_link, AddSubRequest,
    ImportError,
};
use store::{
    count_importable, delete_confirm_text, due_for_refresh, export_confirm_description, now_unix,
    proxy_urls, ConfigPatch, HealthSnapshot, NewConfig, ProxyConfig, Store, StoreError,
    Subscription as ProxySubscription, DEFAULT_AUTO_REFRESH_SECS, DEFAULT_BIND, DEFAULT_GROUP,
    DEFAULT_PROTOCOL,
};

/// Ticket 12: background tick intervals. Each tick is bounded (engine poll
/// is O(1); the subscription pass only starts already-due subs; the
/// re-probe only covers running rows with the existing 5s per-test bound)
/// and stoppable (see [`AppView::stop_background_ticks`]; the loop also
/// exits when the view is released).
pub const ENGINE_POLL_TICK_SECS: u64 = 5;
pub const DUE_SUBSCRIPTION_TICK_SECS: u64 = 300;
pub const REPROBE_TICK_SECS: u64 = 60;

/// Delegate for the configuration library list.
///
/// Ticket 02: renders stored rows with stable domain ids
/// (`library-row-{config.id}`), no entity per row. Selection is mirrored here
/// so the detail bar can follow it; search filters the visible rows.
struct LibraryDelegate {
    rows: Vec<ProxyConfig>,
    visible: Vec<usize>,
    query: String,
    selected: Option<IndexPath>,
    /// Ticket 06: per-row health summary (`config_id` → `87ms` / `dead`);
    /// empty or missing means untested (no health child rendered).
    health: HashMap<String, String>,
}

impl LibraryDelegate {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            visible: Vec::new(),
            query: String::new(),
            selected: None,
            health: HashMap::new(),
        }
    }

    /// Replace all rows, preserving the selection by config id when possible.
    fn set_rows(&mut self, rows: Vec<ProxyConfig>) {
        let selected_id = self.selected_row().map(|row| row.id.clone());
        self.rows = rows;
        self.apply_filter();
        self.selected = selected_id.and_then(|id| {
            self.visible
                .iter()
                .position(|&index| self.rows[index].id == id)
                .map(IndexPath::new)
        });
    }

    fn apply_filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                query.is_empty()
                    || row.name.to_lowercase().contains(&query)
                    || row.protocol.to_lowercase().contains(&query)
                    || row.group.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        if let Some(selected) = self.selected {
            if selected.row >= self.visible.len() {
                self.selected = None;
            }
        }
    }

    fn row_at(&self, index: IndexPath) -> Option<ProxyConfig> {
        self.visible
            .get(index.row)
            .and_then(|&row| self.rows.get(row))
            .cloned()
    }

    fn selected_row(&self) -> Option<ProxyConfig> {
        self.selected.and_then(|index| self.row_at(index))
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Ticket 06: replace the per-row health summaries (keyed by config id
    /// so row replacement never misattributes a latency).
    fn set_health(&mut self, health: HashMap<String, String>) {
        self.health = health;
    }

    /// (total rows, enabled rows) for the status bar.
    fn counts(&self) -> (usize, usize) {
        (
            self.rows.len(),
            self.rows.iter().filter(|row| row.enabled).count(),
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
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = index;
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
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.row_at(index)?;
        let selected = self
            .selected
            .map(|current| current.section == index.section && current.row == index.row)
            .unwrap_or(false);
        let status = if row.enabled { "on" } else { "off" };
        let mut item = ListItem::new(format!("library-row-{}", row.id))
            .selected(selected)
            .child(format!("{} [{status}]", row.name))
            .child(format!(
                "{} · {} · {}:{}",
                row.protocol, row.group, row.bind_ip, row.port
            ));
        if let Some(summary) = self.health.get(&row.id) {
            if !summary.is_empty() {
                item = item.child(summary.clone());
            }
        }
        Some(item)
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("No proxy configurations yet")
    }
}

/// One labeled text field inside the add/edit dialog. Takes the state
/// entity by value so the built element is `'static` (the element clones
/// the entity internally).
fn dialog_field(id: &'static str, label: &str, state: Entity<InputState>) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex()
        .flex_col()
        .gap_1()
        .child(Label::new(label))
        .child(Input::new(&state))
}

/// Handles to the add/edit dialog inputs. Entities are created once in the
/// button handler (never in render) and cloned into the dialog builder.
#[derive(Clone)]
struct ConfigForm {
    name: Entity<InputState>,
    protocol: Entity<InputState>,
    params: Entity<InputState>,
    group: Entity<InputState>,
    bind: Entity<InputState>,
    port: Entity<InputState>,
}

impl ConfigForm {
    fn blank(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Name")),
            protocol: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Protocol")
                    .default_value(DEFAULT_PROTOCOL)
            }),
            params: cx
                .new(|cx| InputState::new(window, cx).placeholder("Protocol params (optional)")),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(DEFAULT_GROUP)
            }),
            bind: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Bind address")
                    .default_value(DEFAULT_BIND)
            }),
            port: cx.new(|cx| InputState::new(window, cx).placeholder("Port (blank = auto)")),
        }
    }

    fn prefilled(window: &mut Window, cx: &mut Context<AppView>, config: &ProxyConfig) -> Self {
        Self {
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Name")
                    .default_value(config.name.clone())
            }),
            protocol: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Protocol")
                    .default_value(config.protocol.clone())
            }),
            params: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Protocol params (optional)")
                    .default_value(config.params.clone())
            }),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(config.group.clone())
            }),
            bind: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Bind address")
                    .default_value(config.bind_ip.clone())
            }),
            port: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Port (blank = auto)")
                    .default_value(config.port.to_string())
            }),
        }
    }

    /// Consumes a clone; every built element owns its state entity.
    fn element(self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dialog_field("field-name", "Name", self.name))
            .child(dialog_field("field-protocol", "Protocol", self.protocol))
            .child(dialog_field("field-params", "Params", self.params))
            .child(dialog_field("field-group", "Group", self.group))
            .child(dialog_field("field-bind", "Bind address", self.bind))
            .child(dialog_field("field-port", "Port", self.port))
    }

    fn as_new(&self, cx: &App) -> NewConfig {
        NewConfig {
            name: self.name.read(cx).value().to_string(),
            protocol: self.protocol.read(cx).value().to_string(),
            params: self.params.read(cx).value().to_string(),
            group: self.group.read(cx).value().to_string(),
            bind_ip: self.bind.read(cx).value().to_string(),
            port: self.port.read(cx).value().to_string(),
        }
    }

    fn as_patch(&self, cx: &App) -> ConfigPatch {
        ConfigPatch {
            name: Some(self.name.read(cx).value().to_string()),
            protocol: Some(self.protocol.read(cx).value().to_string()),
            params: Some(self.params.read(cx).value().to_string()),
            group: Some(self.group.read(cx).value().to_string()),
            bind_ip: Some(self.bind.read(cx).value().to_string()),
            port: Some(self.port.read(cx).value().to_string()),
        }
    }
}

/// Handles to the import dialog inputs: a multi-line paste box plus the
/// target group. `file_hint` carries the dropped file name (if exactly one)
/// so `.conf` content keeps its stem for peer names; `original_text` is the
/// prefill at open time (stale-hint guard: edited text parses as its own
/// type); `multi_files` carries every dropped file for multi-drop so per-file
/// stems/keys survive (merged text alone cannot recover file boundaries).
#[derive(Clone)]
struct ImportForm {
    text: Entity<TextareaState>,
    group: Entity<InputState>,
    file_hint: Option<String>,
    original_text: String,
    multi_files: Option<Vec<(String, String)>>,
}

impl ImportForm {
    fn new(
        window: &mut Window,
        cx: &mut Context<AppView>,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
    ) -> Self {
        Self {
            text: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("One share link per line, JSON, or a WireGuard conf")
                    .default_value(prefill_text.to_string())
            }),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(prefill_group.to_string())
            }),
            file_hint,
            original_text: prefill_text.to_string(),
            multi_files: None,
        }
    }

    fn new_multi(
        window: &mut Window,
        cx: &mut Context<AppView>,
        prefill_text: &str,
        prefill_group: &str,
        files: Vec<(String, String)>,
    ) -> Self {
        let mut form = Self::new(window, cx, prefill_text, prefill_group, None);
        form.multi_files = Some(files);
        form
    }

    /// Consumes a clone; every built element owns its state entity.
    fn element(self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("field-import-text")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new("Links, JSON, or conf"))
                    .child(Textarea::new(&self.text).h(px(160.))),
            )
            .child(dialog_field("field-import-group", "Group", self.group))
    }
}

/// Ticket 04: redacted subscription URL for display (query and fragment are
/// dropped so tokens never reach the screen or logs).
/// Ticket 14: userinfo is stripped too — same rule as
/// [`subscription_label_from_url`]: a pasted URL can embed `user:password`,
/// which must never appear in a row, notification, or log.
fn redact_subscription_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    // Strip `userinfo@` from the authority, keeping scheme + host + path.
    let stripped = match without_query.split_once("://") {
        Some((scheme, rest)) => {
            let path_at = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(path_at);
            let host = authority.rsplit('@').next().unwrap_or(authority);
            format!("{scheme}://{host}{path}")
        }
        None => {
            let path_at = without_query.find('/').unwrap_or(without_query.len());
            let (authority, path) = without_query.split_at(path_at);
            let host = authority.rsplit('@').next().unwrap_or(authority);
            format!("{host}{path}")
        }
    };
    const LIMIT: usize = 96;
    if stripped.chars().count() > LIMIT {
        let kept: String = stripped.chars().take(LIMIT).collect();
        format!("{kept}…")
    } else {
        stripped
    }
}

/// Ticket 04: default subscription label derived from the URL host (never
/// the userinfo, never the query — both may carry a token).
/// Ticket 09: userinfo stripped too (a pasted URL can embed credentials).
fn subscription_label_from_url(url: &str) -> String {
    let after = url.split("://").nth(1).unwrap_or(url);
    let host = after.split('/').next().unwrap_or(after);
    let host = host.rsplit('@').next().unwrap_or(host);
    if host.trim().is_empty() {
        "Subscription".to_string()
    } else {
        host.to_string()
    }
}

/// Ticket 14: single-line `throne://addsub/...` failure text. Surfaces the
/// intake's own reason (`bad-base64`, `bad-host`, …) — the parse errors never
/// echo the URL (see `addsub_rejects_non_http_and_unknown_command`), so this
/// is safe for notifications.
fn addsub_error_message(error: &ImportError) -> String {
    format!("Invalid subscription link: {error}")
}

/// Ticket 16: dialog group wins for single-addsub intake. A blank dialog
/// falls back to the payload group; an untouched `default` dialog keeps a
/// custom payload group; any edited (non-default) dialog group lands the
/// subscription in the edited group, not the payload group.
fn effective_addsub_group(dialog_group: &str, payload_group: &str) -> String {
    let dialog = dialog_group.trim();
    if dialog.is_empty() {
        return payload_group.to_string();
    }
    if dialog == DEFAULT_GROUP && payload_group != DEFAULT_GROUP {
        return payload_group.to_string();
    }
    dialog.to_string()
}

/// Ticket 16: honest export toast. The payload only re-imports import-shaped
/// rows, so when hand-made metadata-only rows exist the toast states both
/// numbers instead of claiming every row re-imports.
fn export_count_message(total: usize, importable: usize, path: &str) -> String {
    if importable == total {
        format!("Exported {total} config(s) to {path}")
    } else {
        format!("Exported {total} config(s) ({importable} importable) to {path}")
    }
}

/// Ticket 04: fetch a subscription body over the platform HTTP client.
/// Errors are generic on purpose (never echo the URL: tokens hide in
/// queries). Bodies are capped at 8 MiB and must be UTF-8; anything else is
/// a refresh failure that keeps the last-good members.
async fn fetch_subscription_body(client: Arc<dyn HttpClient>, url: &str) -> Result<String, String> {
    use futures::AsyncReadExt as _;
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    const TIMEOUT_SECS: u64 = 20;
    let request = http_client::http::Request::builder()
        .uri(url)
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .body(AsyncBody::empty())
        .map_err(|_| "subscription request failed".to_string())?;
    let mut response = client
        .send(request)
        .await
        .map_err(|_| "subscription fetch failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "subscription fetch failed (status {})",
            response.status().as_u16()
        ));
    }
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65_536];
    loop {
        let n = response
            .body_mut()
            .read(&mut chunk)
            .await
            .map_err(|_| "subscription fetch failed".to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_BYTES {
            return Err("subscription body too large".to_string());
        }
    }
    String::from_utf8(buf).map_err(|_| "subscription body is not UTF-8".to_string())
}

/// Handles to the subscription add form inside the subscriptions dialog.
#[derive(Clone)]
struct SubForm {
    label: Entity<InputState>,
    url: Entity<InputState>,
    group: Entity<InputState>,
}

impl SubForm {
    fn blank(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            label: cx.new(|cx| InputState::new(window, cx).placeholder("Label")),
            url: cx.new(|cx| InputState::new(window, cx).placeholder("https://…")),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(DEFAULT_GROUP)
            }),
        }
    }

    /// Consumes a clone; every built element owns its state entity.
    fn element(self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dialog_field("field-sub-label", "Label", self.label))
            .child(dialog_field("field-sub-url", "Subscription URL", self.url))
            .child(dialog_field("field-sub-group", "Group", self.group))
    }
}

/// Ticket 04: one subscription row for the manager dialog (label, redacted
/// URL, group, member count, auto flag, last error) with Refresh / Auto /
/// Remove actions. Refresh re-reads the record so a stale snapshot cannot
/// fetch a removed subscription.
fn subscription_row(
    view: Entity<AppView>,
    sub: ProxySubscription,
    count: usize,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(sub.label.clone()))
        .child(format!(
            "{} · {} · {count} configs · auto {}",
            redact_subscription_url(&sub.url),
            sub.group,
            if sub.auto_refresh { "on" } else { "off" }
        ));
    if let Some(error) = sub.last_error.as_deref() {
        row = row.child(format!("last error: {error}"));
    }
    let refresh_view = view.clone();
    let refresh_id = sub.id.clone();
    row = row.child(
        Button::new(format!("sub-refresh-{}", sub.id))
            .label("Refresh")
            .on_click(move |_, window, cx| {
                let sub_id = refresh_id.clone();
                refresh_view.update(cx, |view, cx| {
                    view.refresh_subscription_by_id(&sub_id, window, cx);
                });
            }),
    );
    let auto_view = view.clone();
    let auto_id = sub.id.clone();
    let auto_now = sub.auto_refresh;
    row = row.child(
        Button::new(format!("sub-auto-{}", sub.id))
            .label(if auto_now { "Auto on" } else { "Auto off" })
            .on_click(move |_, window, cx| {
                let sub_id = auto_id.clone();
                auto_view.update(cx, |view, cx| {
                    view.commit_subscription_auto(&sub_id, auto_now, window, cx);
                });
            }),
    );
    let remove_view = view.clone();
    row.child(
        Button::new(format!("sub-remove-{}", sub.id))
            .label("Remove")
            .on_click(move |_, window, cx| {
                let sub = sub.clone();
                remove_view.update(cx, |view, cx| {
                    view.open_subscription_remove_confirm(sub, window, cx);
                });
            }),
    )
}

/// Ticket 12: run one background tick loop: sleep `secs`, run one bounded
/// pass, then exit when ticks are stopped or the view is released (fail
/// closed: a released view ends its loops instead of updating dead state).
fn spawn_tick(
    cx: &mut Context<AppView>,
    secs: u64,
    tick: impl Fn(&mut AppView, &mut Context<AppView>) + 'static,
) {
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_entity, cx| loop {
        cx.background_executor()
            .timer(Duration::from_secs(secs))
            .await;
        let keep_going = cx.update(|cx| {
            weak.update(cx, |view, cx| {
                if !view.ticks_enabled {
                    return false;
                }
                tick(view, cx);
                true
            })
            .unwrap_or(false)
        });
        if !keep_going {
            break;
        }
    })
    .detach();
}

/// Root view of the ManyProxy window: title bar, toolbar, virtualized
/// configuration list, detail bar, status bar, plus overlay layers.
struct AppView {
    focus: FocusHandle,
    libraries: Entity<ListState<LibraryDelegate>>,
    store: Store,
    /// Ticket 05: supervised sidecar owner (one combined config for the
    /// active set). Production uses the real `sing-box` driver; tests swap
    /// in the fake via direct field assignment.
    engine: EngineController,
    /// Ticket 06: health samples + traffic counters, redacted sidecar logs
    /// with a per-config UI filter, and the boxed measure driver
    /// (production dials each config's own endpoint; tests swap in the
    /// fake via direct field assignment like the engine).
    /// Ticket 12: `ticks_enabled` gates the three background ticks
    /// ([`AppView::stop_background_ticks`]); the latest sample per row is
    /// also persisted so restart restores latencies + dead flags.
    health_store: HealthStore,
    logs: LogStore,
    log_filter: String,
    prober: Box<dyn HealthProber>,
    ticks_enabled: bool,
    /// Retained list subscription: dropping it would stop detail-bar
    /// refreshes on selection/search events.
    _libraries_subscription: Subscription,
}

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        match Store::open_file(&Store::default_path()) {
            Ok(store) => Self::with_store(window, cx, store),
            Err(error) => {
                let view = Self::with_store(
                    window,
                    cx,
                    Store::open_in_memory().expect("in-memory store opens"),
                );
                window.push_notification(
                    Notification::warning(format!("Running without persistence: {error}")),
                    cx,
                );
                view
            }
        }
    }

    fn with_store(window: &mut Window, cx: &mut Context<Self>, store: Store) -> Self {
        let libraries = cx.new(|cx| {
            ListState::new(LibraryDelegate::new(), window, cx)
                .searchable(true)
                .selectable(true)
        });
        let view = Self {
            focus: cx.focus_handle(),
            libraries: libraries.clone(),
            store,
            engine: EngineController::production(),
            health_store: HealthStore::default(),
            logs: LogStore::default(),
            log_filter: String::new(),
            prober: Box::new(RealHealthProber::default()),
            ticks_enabled: true,
            _libraries_subscription: cx.subscribe(
                &libraries,
                |_view, _state, _event: &ListEvent, cx| {
                    cx.notify();
                },
            ),
        };
        let mut view = view;
        // Ticket 12: restore last latencies + dead flags across restart.
        // Only snapshots of still-known rows hydrate (removed configs never
        // resurrect); traffic stays session-only; logs are never persisted.
        if let Ok(snapshots) = view.store.load_health_snapshots() {
            let known: HashSet<String> = view
                .store
                .list()
                .unwrap_or_default()
                .into_iter()
                .map(|row| row.id)
                .collect();
            let kept: Vec<HealthSnapshot> = snapshots
                .into_iter()
                .filter(|snapshot| known.contains(&snapshot.config_id))
                .collect();
            view.health_store.hydrate(&kept);
        }
        let rows = match view.store.list() {
            Ok(rows) => rows,
            // Ticket 14: a broken database must not render as a silent empty
            // library.
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load library: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        libraries.update(cx, |state, _| {
            state.delegate_mut().set_rows(rows);
        });
        cx.notify();
        view
    }

    /// Number of stored configurations (also asserted by UI tests).
    #[cfg_attr(not(test), allow(dead_code))]
    fn config_count(&self, cx: &App) -> usize {
        self.libraries.read(cx).delegate().counts().0
    }

    fn selected_config(&self, cx: &App) -> Option<ProxyConfig> {
        self.libraries.read(cx).delegate().selected_row()
    }

    /// Reload rows from the store, keeping the selection by id. A list
    /// failure notifies (never a silent empty list); callers without a
    /// window (background ticks, headless refresh) use [`AppView::reload_quiet`].
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.reload_inner(cx) {
            window.push_notification(
                Notification::error(format!("Cannot load library: {error}")),
                cx,
            );
        }
        cx.notify();
    }

    /// Window-less reload for background ticks and headless paths: keeps the
    /// last-good rows when the store fails (no window exists to notify).
    fn reload_quiet(&mut self, cx: &mut Context<Self>) {
        let _ = self.reload_inner(cx);
        cx.notify();
    }

    fn reload_inner(&mut self, cx: &mut Context<Self>) -> Result<(), StoreError> {
        self.drain_engine_logs();
        self.poll_engine();
        let rows = self.store.list()?;
        let health = self.health_map();
        self.libraries.update(cx, |state, _| {
            state.delegate_mut().set_rows(rows);
            state.delegate_mut().set_health(health);
        });
        Ok(())
    }

    fn report_save_error(&self, error: StoreError, window: &mut Window, cx: &mut App) {
        window.push_notification(Notification::error(format!("Cannot save: {error}")), cx);
    }

    fn commit_add(
        &mut self,
        input: NewConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.add(input) {
            Ok(_) => {
                self.reload(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    fn commit_edit(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.update(id, patch) {
            Ok(_) => {
                self.reload(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    fn commit_remove(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.remove(id) {
            Ok(true) => {
                self.libraries.update(cx, |state, _| {
                    state.delegate_mut().clear_selection();
                });
                self.reload(window, cx);
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Cannot remove: {error}")), cx);
            }
        }
    }

    fn toggle_selected_enabled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        // Ticket 14: a failed write notifies (never a dead click).
        match self.store.set_enabled(&config.id, !config.enabled) {
            Ok(_) => self.reload(window, cx),
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Cannot toggle: {error}")), cx);
            }
        }
    }

    fn copy_selected_urls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(config) = self.selected_config(cx) {
            let (socks, http) = proxy_urls(&config.bind_ip, config.port);
            cx.write_to_clipboard(ClipboardItem::new_string(format!("{socks}\n{http}")));
            window.push_notification(Notification::success("Copied proxy URLs"), cx);
        }
    }

    /// Ticket 05: surface a detected sidecar crash on every reload so the
    /// status bar reflects it without a background timer.
    /// Ticket 12: returns the named cause once (the engine-crash tick uses
    /// it; reload ignores it).
    fn poll_engine(&mut self) -> Option<String> {
        self.engine.poll()
    }

    /// Ticket 05: warn once per non-loopback bind going live (LAN exposure
    /// needs an explicit user action — the Start click — plus this warning).
    fn warn_for_lan_binds(&self, window: &mut Window, cx: &mut App) {
        for warning in self.engine.lan_warnings() {
            window.push_notification(Notification::warning(warning), cx);
        }
    }

    /// Ticket 05: start the selected config. The engine regenerates one
    /// combined config, runs the engine `check`, then restarts the sidecar;
    /// a port conflict names both configs and changes nothing.
    /// Ticket 09: non-loopback binds stop at an explicit confirm dialog
    /// first (LAN-exposure gate) — nothing starts until the user confirms.
    fn start_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        if requires_lan_confirm(&config.bind_ip) {
            self.open_lan_confirm_single(config, window, cx);
            return;
        }
        self.commit_start_selected(&config.id, window, cx);
    }

    /// Ticket 09: LAN confirm gate for one config. The dialog names the
    /// config + bind only (see [`lan_confirm_text`]); confirming starts it,
    /// dismissing changes nothing.
    fn open_lan_confirm_single(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let description = lan_confirm_text(&config.name, &config.bind_ip);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            let description = description.clone();
            alert
                .confirm()
                .title(LAN_CONFIRM_TITLE)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
                    true
                })
        });
    }

    /// Ticket 09: the actual single start (post-gate). Looks the config up
    /// fresh so a confirmed dialog never starts a stale row.
    fn commit_start_selected(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(Some(config)) = self.store.get(id) else {
            return;
        };
        if let Some(warning) = lan_exposure_warning(&config.name, &config.bind_ip) {
            window.push_notification(Notification::warning(warning), cx);
        }
        match self.engine.start_one(&config) {
            Ok(()) => {
                let _ = self.store.set_enabled(id, true);
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Started \"{}\"", config.name)),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(Notification::error(error.to_string()), cx);
            }
        }
    }

    /// Ticket 05: stop the selected config; its endpoint closes, the rest
    /// keep serving on a regenerated config.
    fn stop_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        let id = config.id.clone();
        if self.engine.stop_one(&id) {
            let _ = self.store.set_enabled(&id, false);
            self.reload(window, cx);
            window.push_notification(
                Notification::success(format!("Stopped \"{}\"", config.name)),
                cx,
            );
        }
    }

    /// Ticket 05: start every stored config at once (each keeps its own
    /// endpoint; the enabled flag follows the engine state).
    /// Ticket 09: any non-loopback bind routes through one confirm dialog
    /// naming every LAN-bound row — nothing starts until the user confirms.
    fn start_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.store.list().unwrap_or_default();
        let lan: Vec<(String, String)> = rows
            .iter()
            .filter(|row| requires_lan_confirm(&row.bind_ip))
            .map(|row| (row.name.clone(), row.bind_ip.clone()))
            .collect();
        if !lan.is_empty() {
            let view = cx.entity();
            let description = lan_confirm_description(&lan);
            window.open_alert_dialog(cx, move |alert, _, _| {
                let view = view.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .title(LAN_CONFIRM_TITLE)
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| view.commit_start_all(window, cx));
                        true
                    })
            });
            return;
        }
        self.commit_start_all(window, cx);
    }

    /// Ticket 09: the actual start-all (post-gate). Rows are re-listed so a
    /// confirmed dialog never starts a stale set.
    fn commit_start_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.store.list().unwrap_or_default();
        match self.engine.start_all(&rows) {
            Ok(()) => {
                for row in &rows {
                    let _ = self.store.set_enabled(&row.id, true);
                }
                self.reload(window, cx);
                self.warn_for_lan_binds(window, cx);
                window.push_notification(
                    Notification::success(format!("Started {} endpoint(s)", rows.len())),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(Notification::error(error.to_string()), cx);
            }
        }
    }

    /// Ticket 05: stop everything and take the sidecar down.
    fn stop_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.engine.stop_all();
        let rows = self.store.list().unwrap_or_default();
        for row in &rows {
            let _ = self.store.set_enabled(&row.id, false);
        }
        self.reload(window, cx);
        window.push_notification(Notification::success("Stopped all endpoints"), cx);
    }

    /// Ticket 06: stored rows backing the running endpoints (each probe
    /// dials its own row's endpoint, so results are per-egress).
    fn active_health_rows(&self) -> Vec<ProxyConfig> {
        let running = self.engine.running_ids();
        self.store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| running.contains(&row.id))
            .collect()
    }

    /// Ticket 06: per-row summaries for the virtualized list.
    fn health_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for id in self.engine.running_ids() {
            let summary = self.health_store.row_summary(&id);
            if !summary.is_empty() {
                map.insert(id, summary);
            }
        }
        map
    }

    /// Ticket 06: `(live, dead)` over the running endpoints for the
    /// status bar.
    fn health_counts(&self) -> (usize, usize) {
        self.health_store.counts(&self.engine.running_ids())
    }

    fn health_status_text(&self) -> String {
        let (live, dead) = self.health_counts();
        if live == 0 && dead == 0 {
            "health: not tested".to_string()
        } else {
            format!("health: {live} live · {dead} dead")
        }
    }

    /// Ticket 12: record probe outcomes into the session store and persist
    /// each row's latest sample (+ dead flag) for restart. Traffic stays
    /// session-only; logs are never persisted. Persistence failures are
    /// ignored (a full disk must not break the health view).
    fn record_and_persist(&mut self, outcomes: &[TestOutcome]) {
        for outcome in outcomes {
            self.health_store.record_outcome(outcome);
            let snapshot = HealthSnapshot {
                config_id: outcome.sample.config_id.clone(),
                at: outcome.sample.at,
                kind: outcome.sample.kind.as_str().to_string(),
                latency_ms: outcome.sample.latency_ms,
                error: outcome.sample.error.clone(),
            };
            let _ = self.store.save_health_snapshot(&snapshot);
        }
    }

    /// Ticket 06: run one URL test per active endpoint, record samples +
    /// traffic, refresh the rows, and summarize. Synchronous in v1 with a
    /// bounded per-test timeout (background scheduling deferred).
    /// Ticket 12: outcomes persist (latest sample + dead flag survive
    /// restart) via [`AppView::record_and_persist`].
    fn test_all_health(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let rows = self.active_health_rows();
        if rows.is_empty() {
            window.push_notification(Notification::warning("No running endpoints to test"), cx);
            return 0;
        }
        let outcomes = test_all(&rows, DEFAULT_TEST_URL, &mut *self.prober);
        self.record_and_persist(&outcomes);
        let (live, dead) = self.health_counts();
        self.reload(window, cx);
        window.push_notification(
            Notification::success(format!(
                "Tested {} endpoint(s): {live} live · {dead} dead",
                outcomes.len()
            )),
            cx,
        );
        outcomes.len()
    }

    /// Ticket 06: URL-test just the selected config's endpoint.
    fn test_selected_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        if !self.engine.running_ids().contains(&config.id) {
            window.push_notification(
                Notification::warning(format!("\"{}\" is not running", config.name)),
                cx,
            );
            return;
        }
        let outcomes = test_all(
            std::slice::from_ref(&config),
            DEFAULT_TEST_URL,
            &mut *self.prober,
        );
        self.record_and_persist(&outcomes);
        self.reload(window, cx);
    }

    /// Ticket 06: TCP-ping just the selected config's endpoint.
    fn ping_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        if !self.engine.running_ids().contains(&config.id) {
            window.push_notification(
                Notification::warning(format!("\"{}\" is not running", config.name)),
                cx,
            );
            return;
        }
        let outcomes = ping_all(std::slice::from_ref(&config), &mut *self.prober);
        self.record_and_persist(&outcomes);
        self.reload(window, cx);
    }

    /// Ticket 13: ingest buffered sidecar stderr into the session LogStore
    /// (existing tag match, ingest-time redaction, bounded ring). Runs on
    /// every reload, so live lines appear without user clicks and the exit
    /// note path stays silent-free. Logs stay session-only (never
    /// persisted).
    fn drain_engine_logs(&mut self) {
        let lines = self.engine.drain_log_lines();
        if lines.is_empty() {
            return;
        }
        let active = self.store.list().unwrap_or_default();
        for line in &lines {
            self.logs.push_raw(line, &active);
        }
    }

    /// Ticket 06: visible log lines for the dialog — the selected config's
    /// lines when a row is selected, else engine-global + all — further
    /// narrowed by the `log_filter` substring. Newest last, capped.
    fn log_lines(&self, cx: &App) -> Vec<String> {
        let selected = self.selected_config(cx).map(|config| config.id);
        let filter = self.log_filter.trim().to_lowercase();
        let mut lines: Vec<String> = match selected.as_deref() {
            Some(id) => self
                .logs
                .for_config(id)
                .iter()
                .map(|record| format!("[{}] {}", record.level.as_str(), record.message))
                .collect(),
            None => self
                .logs
                .all()
                .iter()
                .map(|record| format!("[{}] {}", record.level.as_str(), record.message))
                .collect(),
        };
        if !filter.is_empty() {
            lines.retain(|line| line.to_lowercase().contains(&filter));
        }
        if lines.len() > 200 {
            lines = lines.split_off(lines.len() - 200);
        }
        lines
    }

    /// Ticket 06: per-config log dialog with a filter field. OK applies the
    /// filter and re-renders when still open.
    fn open_logs_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.log_filter.clone();
        let filter_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Filter logs")
                .default_value(current)
        });
        let view = cx.entity();
        let reader = filter_state.clone();
        let lines = self.log_lines(cx);
        let title = match self.selected_config(cx) {
            Some(config) => format!("Logs — {}", config.name),
            None => "Logs".to_string(),
        };
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            let reader = reader.clone();
            let mut body = div().flex().flex_col().gap_2().child(dialog_field(
                "field-log-filter",
                "Filter",
                filter_state.clone(),
            ));
            let mut list = div()
                .id("log-lines")
                .test_support()
                .flex()
                .flex_col()
                .gap_1();
            if lines.is_empty() {
                list = list.child("No log lines for this filter yet");
            }
            for line in &lines {
                list = list.child(line.clone());
            }
            body = body.child(list);
            let view = view.clone();
            dialog.title(title.clone()).child(body).button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .on_ok(move |_, window, cx| {
                        let filter = reader.read(cx).value().to_string();
                        view.update(cx, |view, cx| {
                            view.log_filter = filter;
                            cx.notify();
                        });
                        window.close_all_dialogs(cx);
                        true
                    }),
            )
        });
    }

    /// Ticket 11: user-owned JSON export behind the plaintext-secret
    /// warning. The confirm dialog shows [`export_confirm_description`]
    /// (the [`store::export_warning_text`] verbatim); confirming writes the
    /// file, dismissing writes nothing. The payload round-trips through the
    /// import grammar (re-import yields duplicates, never adds).
    fn open_export_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let description = export_confirm_description().to_string();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .title("Export library")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_export_write(window, cx));
                    true
                })
        });
    }

    /// Ticket 11: the actual export write (post-confirm). The notification
    /// names only the destination path and the row count — never secrets.
    /// Ticket 16: the count is honest — import-shaped (re-importable) vs
    /// total, so hand-made metadata-only rows never inflate the claim.
    fn commit_export_write(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = Store::default_export_path();
        match self.store.commit_export(&path, true) {
            Ok(Some(path)) => {
                let rendered = path.display().to_string();
                // Ticket 14: the export itself succeeded — name the path with
                // the real count, or without a count when the count query
                // fails (never claim "0 config(s)").
                match self.store.list() {
                    Ok(rows) => window.push_notification(
                        Notification::success(export_count_message(
                            rows.len(),
                            count_importable(&rows),
                            &rendered,
                        )),
                        cx,
                    ),
                    Err(_) => window.push_notification(
                        Notification::success(format!("Exported to {rendered}")),
                        cx,
                    ),
                }
            }
            Ok(None) => {}
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Export failed: {error}")), cx);
            }
        }
    }

    fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = ConfigForm::blank(window, cx);
        let view = cx.entity();
        let reader = form.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            let reader = reader.clone();
            dialog
                .title("Add proxy configuration")
                .child(form.clone().element())
                .button_props(DialogButtonProps::default().show_cancel(true).on_ok(
                    move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.commit_add(reader.as_new(cx), window, cx)
                        })
                    },
                ))
        });
    }

    fn open_edit_dialog(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = ConfigForm::prefilled(window, cx, &config);
        let view = cx.entity();
        let reader = form.clone();
        let id = config.id.clone();
        let title = format!("Edit {}", config.name);
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            let reader = reader.clone();
            let id = id.clone();
            dialog
                .title(title.clone())
                .child(form.clone().element())
                .button_props(DialogButtonProps::default().show_cancel(true).on_ok(
                    move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.commit_edit(&id, reader.as_patch(cx), window, cx)
                        })
                    },
                ))
        });
    }

    fn open_remove_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let name = config.name.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .confirm()
                .title("Remove configuration")
                .description(delete_confirm_text(&name))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_remove(&id, window, cx));
                    true
                })
        });
    }

    /// Ticket 03: paste/clipboard entry point. The batch commits through the
    /// existing add flow, so every record lands disabled in the chosen group.
    fn open_import_dialog(
        &mut self,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_import_dialog_with_files(
            prefill_text,
            prefill_group,
            file_hint,
            None,
            window,
            cx,
        );
    }

    /// Ticket 16: multi-file opener — the dialog shows the merged text but
    /// retains every `(file_name, content)` pair so an unedited commit keeps
    /// per-file stems/keys; any edit falls back to single-text sniffing.
    fn open_import_dialog_with_files(
        &mut self,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
        multi_files: Option<Vec<(String, String)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = match multi_files {
            Some(files) => ImportForm::new_multi(window, cx, prefill_text, prefill_group, files),
            None => ImportForm::new(window, cx, prefill_text, prefill_group, file_hint),
        };
        let view = cx.entity();
        let reader = form.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            let reader = reader.clone();
            dialog
                .title("Import proxy configurations")
                .child(form.clone().element())
                .button_props(DialogButtonProps::default().show_cancel(true).on_ok(
                    move |_, window, cx| {
                        let text = reader.text.read(cx).value().to_string();
                        let group = reader.group.read(cx).value().to_string();
                        let hint = reader.file_hint.clone();
                        let original = reader.original_text.clone();
                        let multi = reader.multi_files.clone();
                        view.update(cx, |view, cx| {
                            view.commit_import_with_context(
                                text,
                                group,
                                hint,
                                Some(original),
                                multi,
                                window,
                                cx,
                            )
                        })
                    },
                ))
        });
    }

    /// Ticket 03: clipboard entry point. Clipboard text prefills the same
    /// dialog (same group choice, same commit path).
    fn open_import_from_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        if text.trim().is_empty() {
            window.push_notification(Notification::warning("Clipboard has no text to import"), cx);
            return;
        }
        self.open_import_dialog(&text, DEFAULT_GROUP, None, window, cx);
    }

    /// Ticket 03: drag-drop/file entry point. Readable files are concatenated
    /// into the same import dialog; a single dropped file keeps its name so
    /// `.conf` content parses as WireGuard with stem-based peer names.
    /// Ticket 16: multi-file drops retain every file's bytes so per-file
    /// `.conf` stems/keys survive; the dialog still shows the merged text.
    fn import_dropped_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut merged = String::new();
        let mut names: Vec<String> = Vec::new();
        let mut files: Vec<(String, String)> = Vec::new();
        for path in paths.0.iter() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if !merged.is_empty() {
                    merged.push('\n');
                }
                merged.push_str(&content);
                let name = path.to_string_lossy().into_owned();
                names.push(name.clone());
                files.push((name, content));
            }
        }
        if merged.trim().is_empty() {
            window.push_notification(Notification::warning("No readable files dropped"), cx);
            return;
        }
        if names.len() == 1 {
            let file_hint = names.into_iter().next();
            self.open_import_dialog(&merged, DEFAULT_GROUP, file_hint, window, cx);
        } else {
            self.open_import_dialog_with_files(
                &merged,
                DEFAULT_GROUP,
                None,
                Some(files),
                window,
                cx,
            );
        }
    }

    /// Ticket 03: commit a paste box through the existing add flow. Valid
    /// items become disabled records in `group`; failures are reported with
    /// item index + reason (never partial silent import); identical
    /// re-imports add nothing. Import never starts configs.
    /// Ticket 04: a lone `throne://addsub/...` line diverts to the
    /// subscription intake instead (record + immediate first refresh).
    /// Ticket 16: stale `.conf` hints are cleared when the text no longer
    /// looks like a conf, and single-addsub honors the dialog group.
    /// Test-only callers (existing import_commit_* tests) go through here.
    #[cfg_attr(not(test), allow(dead_code))]
    fn commit_import(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.commit_import_with_context(text, group, file_hint, None, None, window, cx)
    }

    /// Ticket 16: context-aware commit behind [`commit_import`]. `original`
    /// is the dialog prefill (when `text` differs, the drop hint and the
    /// multi-file bytes are stale and the edited text sniffs as its own
    /// type); `multi` carries every dropped file for unedited multi-commits.
    #[allow(clippy::too_many_arguments)]
    fn commit_import_with_context(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        original: Option<String>,
        multi: Option<Vec<(String, String)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let edited = match original.as_deref() {
            Some(prefill) => text.trim() != prefill.trim(),
            None => false,
        };
        let multi = if edited { None } else { multi };
        let file_hint = if edited {
            None
        } else {
            effective_hint_for_text(file_hint.as_deref(), &text)
        };
        // Unedited multi-file drop: per-file parse keeps stems/keys.
        if let Some(files) = multi {
            if file_hint.is_none() {
                let refs: Vec<(&str, &str)> = files
                    .iter()
                    .map(|(name, content)| (name.as_str(), content.as_str()))
                    .collect();
                let batch = import_multi_file_contents(&refs);
                return self.finish_import_batch(batch, &group, window, cx);
            }
        }
        let trimmed = text.trim();
        if !trimmed.is_empty() && !trimmed.contains('\n') && file_hint.is_none() {
            // Ticket 14: only addsub lines divert; a malformed one reports
            // its own parse reason (never the generic unknown-command) and
            // keeps the dialog open.
            if is_addsub_attempt(trimmed) {
                return match parse_addsub_link(trimmed) {
                    Ok(request) => {
                        let effective = effective_addsub_group(&group, &request.group);
                        self.commit_addsub_request_with_group(request, &effective, window, cx)
                    }
                    Err(error) => {
                        window.push_notification(
                            Notification::error(addsub_error_message(&error)),
                            cx,
                        );
                        false
                    }
                };
            }
        }
        let batch = match file_hint {
            Some(name) => import_file_content(&name, &text),
            None => import_text(&text),
        };
        self.finish_import_batch(batch, &group, window, cx)
    }

    /// Ticket 03/14: shared batch-commit tail (reload + truthful summary).
    fn finish_import_batch(
        &mut self,
        batch: crate::import::BatchResult,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match commit_batch(&mut self.store, batch, group) {
            Ok(outcome) => {
                self.reload(window, cx);
                // Ticket 14: total failure keeps the dialog open and lists
                // every failure (index/label/reason), not just the first.
                if outcome.imported == 0 && !outcome.failures.is_empty() {
                    window.push_notification(
                        Notification::error(format!(
                            "Import failed: {}",
                            format_import_failures(&outcome.failures)
                        )),
                        cx,
                    );
                    return false;
                }
                let mut summary = format!(
                    "Imported {} · {} duplicates skipped · {} failed",
                    outcome.imported,
                    outcome.duplicates,
                    outcome.failures.len()
                );
                if !outcome.failures.is_empty() {
                    summary.push_str(&format!(" — {}", format_import_failures(&outcome.failures)));
                }
                if outcome.failures.is_empty() {
                    window.push_notification(Notification::success(summary), cx);
                } else {
                    window.push_notification(Notification::warning(summary), cx);
                }
                true
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Import failed: {error}")), cx);
                false
            }
        }
    }

    /// Ticket 04: `throne://addsub/...` intake — record the subscription
    /// (label from the URL host, never the query) and fetch immediately so
    /// members appear without waiting for the next startup pass.
    /// Ticket 16: the dialog group wins over the payload group (see
    /// [`effective_addsub_group`]).
    fn commit_addsub_request_with_group(
        &mut self,
        request: AddSubRequest,
        dialog_group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let label = subscription_label_from_url(&request.url);
        self.commit_subscription_add(label, request.url, dialog_group.to_string(), window, cx)
    }

    /// Ticket 04: add a subscription record; on success the dialog closes and
    /// a first refresh starts in the background. Invalid input keeps the
    /// dialog open with an error notification.
    fn commit_subscription_add(
        &mut self,
        label: String,
        url: String,
        group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.add_subscription(&label, &url, &group, true) {
            Ok(sub) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Subscription \"{}\" added — refreshing",
                        sub.label
                    )),
                    cx,
                );
                self.start_subscription_refresh(sub, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    /// Ticket 14: manual per-row refresh with truthful outcomes — a missing
    /// subscription warns (it was already removed), a store failure names
    /// the error (never a dead click).
    fn refresh_subscription_by_id(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.get_subscription(sub_id) {
            Ok(Some(sub)) => self.start_subscription_refresh(sub, cx),
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh subscription: {error}")),
                    cx,
                );
            }
        }
    }

    /// Ticket 14: auto-refresh toggle with truthful outcomes (never a dead
    /// click on store failure).
    fn commit_subscription_auto(
        &mut self,
        sub_id: &str,
        auto_now: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.set_subscription_auto_refresh(sub_id, !auto_now) {
            Ok(_) => self.reload(window, cx),
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot change auto-refresh: {error}")),
                    cx,
                );
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    /// Ticket 04: fetch one subscription without blocking the UI, then merge
    /// through the tested [`Store::refresh_with_body`] seam.
    /// Ticket 12: window-free (the completion path branches on the active
    /// window itself) so the due-subscription background tick can call it.
    fn start_subscription_refresh(&mut self, sub: ProxySubscription, cx: &mut Context<Self>) {
        let client = cx.http_client();
        let view = cx.entity();
        let sub_id = sub.id.clone();
        let label = sub.label.clone();
        let url = sub.url.clone();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_subscription_body(client, &url).await;
            let now = now_unix();
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.finish_subscription_refresh(
                                &sub_id, &label, fetched, now, window, cx,
                            );
                        });
                    });
                } else {
                    view.update(cx, |view, cx| {
                        view.finish_subscription_refresh_no_window(&sub_id, fetched, now, cx);
                    });
                }
            });
        })
        .detach();
    }

    /// Ticket 04: apply a fetched body (or fetch error) with user feedback,
    /// then refresh the dialog contents if it is still open.
    fn finish_subscription_refresh(
        &mut self,
        sub_id: &str,
        label: &str,
        fetched: Result<String, String>,
        now: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ticket 14: the subscription may have been deleted while the fetch
        // was in flight — drop the result silently (no failure toast for a
        // sub the user removed on purpose). A store failure here still
        // surfaces instead of silently swallowing the completion.
        match self.store.get_subscription(sub_id) {
            Ok(None) => {
                self.reload(window, cx);
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh subscription: {error}")),
                    cx,
                );
                return;
            }
            Ok(Some(_)) => {}
        }
        match fetched {
            Ok(body) => match self.store.refresh_with_body(sub_id, &body, now) {
                Ok(outcome) => {
                    self.reload(window, cx);
                    if let Some(error) = outcome.error {
                        window.push_notification(
                            Notification::error(format!("{label}: refresh failed ({error})")),
                            cx,
                        );
                    } else {
                        let mut summary = format!(
                            "{}: +{} ~{} -{}",
                            label, outcome.added, outcome.updated, outcome.removed
                        );
                        if outcome.skipped > 0 {
                            summary.push_str(&format!(" · {} skipped", outcome.skipped));
                        }
                        if outcome.duplicates > 0 {
                            summary.push_str(&format!(" · {} duplicates", outcome.duplicates));
                        }
                        if outcome.failures.is_empty() {
                            window.push_notification(Notification::success(summary), cx);
                        } else {
                            summary.push_str(&format!(" · {} failed", outcome.failures.len()));
                            window.push_notification(Notification::warning(summary), cx);
                        }
                    }
                }
                Err(error) => {
                    if matches!(error, StoreError::NotFound(_)) {
                        // Deleted between the check above and the merge.
                        self.reload(window, cx);
                    } else {
                        window.push_notification(
                            Notification::error(format!("{label}: refresh failed ({error})")),
                            cx,
                        );
                    }
                }
            },
            Err(error) => match self.store.record_subscription_error(sub_id, &error) {
                Ok(_) => {
                    self.reload(window, cx);
                    window.push_notification(Notification::error(format!("{label}: {error}")), cx);
                }
                Err(StoreError::NotFound(_)) => {
                    // Deleted mid-flight: silent, like the pre-check arm.
                    self.reload(window, cx);
                }
                Err(db_error) => {
                    window.push_notification(
                        Notification::error(format!("Cannot refresh subscription: {db_error}")),
                        cx,
                    );
                }
            },
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    /// Ticket 04: headless completion path (no window for notifications).
    fn finish_subscription_refresh_no_window(
        &mut self,
        sub_id: &str,
        fetched: Result<String, String>,
        now: i64,
        cx: &mut Context<Self>,
    ) {
        match fetched {
            Ok(body) => {
                let _ = self.store.refresh_with_body(sub_id, &body, now);
            }
            Err(error) => {
                let _ = self.store.record_subscription_error(sub_id, &error);
            }
        }
        self.reload_quiet(cx);
    }

    /// Ticket 04: re-render the subscriptions dialog after an action, but
    /// only if the user still has it open.
    fn reopen_subscriptions_if_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            window.close_all_dialogs(cx);
            self.open_subscriptions_dialog(window, cx);
        }
    }

    /// Ticket 04: subscription manager dialog — existing subscriptions with
    /// per-row Refresh / Auto / Remove plus an add form committed by OK.
    /// An empty add form just closes the dialog.
    fn open_subscriptions_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = SubForm::blank(window, cx);
        let view = cx.entity();
        let reader = form.clone();
        let subs = match self.store.list_subscriptions() {
            Ok(subs) => subs,
            // Ticket 14: a broken database must not render as a lying empty
            // state — the dialog still opens (form usable) plus a notice.
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load subscriptions: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let rows = match self.store.list() {
            Ok(rows) => rows,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load library: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let mut counts: HashMap<String, usize> = HashMap::new();
        for row in &rows {
            if let Some(owner) = row.subscription_id.as_deref() {
                *counts.entry(owner.to_string()).or_insert(0) += 1;
            }
        }
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let view_for_rows = view.clone();
            let mut body = div().flex().flex_col().gap_2();
            if subs.is_empty() {
                body = body.child("No subscriptions yet — paste a subscription URL below");
            }
            for sub in &subs {
                body = body.child(subscription_row(
                    view_for_rows.clone(),
                    sub.clone(),
                    counts.get(&sub.id).copied().unwrap_or(0),
                ));
            }
            body = body.child(form.clone().element());
            let view = view.clone();
            let reader = reader.clone();
            dialog.title("Subscriptions").child(body).button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .on_ok(move |_, window, cx| {
                        let label = reader.label.read(cx).value().to_string();
                        let url = reader.url.read(cx).value().to_string();
                        let group = reader.group.read(cx).value().to_string();
                        if label.trim().is_empty() && url.trim().is_empty() {
                            return true;
                        }
                        view.update(cx, |view, cx| {
                            view.commit_subscription_add(label, url, group, window, cx)
                        })
                    }),
            )
        });
    }

    /// Ticket 14: truthful remove-subscription outcome — success reloads, an
    /// already-gone row warns "Already removed", and any store failure names
    /// the error (never "Already removed", which would lie about an `Err`).
    fn commit_subscription_remove(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.remove_subscription(id) {
            Ok(true) => {
                self.reload(window, cx);
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot remove subscription: {error}")),
                    cx,
                );
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    /// Ticket 04: confirm removing a subscription. Members stay in the
    /// library as hand-owned configs (release, never delete).
    fn open_subscription_remove_confirm(
        &mut self,
        sub: ProxySubscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = sub.id.clone();
        let label = sub.label.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .confirm()
                .title("Remove subscription")
                .description(format!(
                    "Remove \"{label}\"? Its configs stay in the library."
                ))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_subscription_remove(&id, window, cx);
                    });
                    true
                })
        });
    }

    /// Ticket 04: refresh due subscriptions once at startup (the auto-refresh
    /// scheduling hook evaluation). Never called by tests, so no headless
    /// test ever touches the network.
    /// Ticket 12: delegates to [`AppView::due_subscriptions`].
    fn refresh_due_on_startup(&mut self, cx: &mut Context<Self>) {
        for sub in self.due_subscriptions() {
            self.start_subscription_refresh(sub, cx);
        }
    }

    // -- Ticket 12: background ticks (bounded, stoppable) --------------------

    /// Subscriptions due for auto-refresh right now (pure query behind the
    /// due-subscription pass and the startup pass). Never enables anything;
    /// the pass only fetches already-due subscription URLs.
    fn due_subscriptions(&self) -> Vec<ProxySubscription> {
        let now = now_unix();
        self.store
            .list_subscriptions()
            .unwrap_or_default()
            .into_iter()
            .filter(|sub| due_for_refresh(sub, now, DEFAULT_AUTO_REFRESH_SECS))
            .collect()
    }

    /// Stop the background ticks (each tick also no-ops once stopped, so a
    /// stopped view never touches the network or the engine).
    #[cfg_attr(not(test), allow(dead_code))]
    fn stop_background_ticks(&mut self) {
        self.ticks_enabled = false;
    }

    /// One engine-crash poll without user clicks: surfaces the named cause
    /// via the status bar on the next render. Bounded (one `poll`, no I/O
    /// beyond reaping an exited sidecar) and stoppable.
    fn tick_engine_poll(&mut self, cx: &mut Context<Self>) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }
        let cause = self.poll_engine();
        self.reload_quiet(cx);
        cause
    }

    /// One due-subscription pass without user clicks: starts a refresh only
    /// for already-due subscriptions (never beyond them, never enabling or
    /// starting configs). Returns the number of refreshes started. Stopped
    /// views start nothing.
    fn tick_due_subscriptions(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let due = self.due_subscriptions();
        let count = due.len();
        for sub in due {
            self.start_subscription_refresh(sub, cx);
        }
        count
    }

    /// One re-probe of running rows only, without user clicks: silent (no
    /// notification), recorded + persisted like a manual test. Idle views
    /// (nothing running) do nothing: no network traffic, no errors.
    /// Stopped views probe nothing.
    fn tick_reprobe_running(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let rows = self.active_health_rows();
        if rows.is_empty() {
            return 0;
        }
        let outcomes = test_all(&rows, DEFAULT_TEST_URL, &mut *self.prober);
        let count = outcomes.len();
        self.record_and_persist(&outcomes);
        self.reload_quiet(cx);
        count
    }

    /// Spawn the three background tick loops. Each loop sleeps its interval,
    /// runs one bounded pass, and exits when ticks are stopped or the view
    /// is released. Never called by tests (headless tests call the tick
    /// methods directly with mock seams, never touching the network).
    fn start_background_ticks(&mut self, cx: &mut Context<Self>) {
        spawn_tick(cx, ENGINE_POLL_TICK_SECS, |view, cx| {
            view.tick_engine_poll(cx);
        });
        spawn_tick(cx, DUE_SUBSCRIPTION_TICK_SECS, |view, cx| {
            view.tick_due_subscriptions(cx);
        });
        spawn_tick(cx, REPROBE_TICK_SECS, |view, cx| {
            view.tick_reprobe_running(cx);
        });
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(config) = self.selected_config(cx) else {
            return div()
                .id("config-detail")
                .test_support()
                .p_2()
                .child("Select a configuration")
                .into_any_element();
        };
        let toggle_label = if config.enabled { "Disable" } else { "Enable" };
        let switch_id = format!("enabled-{}", config.id);
        let switch_view = cx.entity();
        let switch_id_for_handler = config.id.clone();
        div()
            .id("config-detail")
            .test_support()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Label::new(config.name.clone()))
                    .child(format!(
                        "{} · {} · {}:{}",
                        config.protocol, config.group, config.bind_ip, config.port
                    )),
            )
            .child(
                div()
                    .id("config-health")
                    .test_support()
                    .child(self.health_store.detail_line(&config.id)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new(switch_id)
                            .checked(config.enabled)
                            .label("Enabled")
                            .on_change(move |enabled: &bool, window, cx| {
                                let enabled = *enabled;
                                let id = switch_id_for_handler.clone();
                                switch_view.update(cx, |view, cx| {
                                    // Ticket 14: a failed write notifies
                                    // (never a dead switch).
                                    match view.store.set_enabled(&id, enabled) {
                                        Ok(_) => view.reload(window, cx),
                                        Err(error) => {
                                            window.push_notification(
                                                Notification::error(format!(
                                                    "Cannot toggle: {error}"
                                                )),
                                                cx,
                                            );
                                        }
                                    }
                                });
                            }),
                    )
                    .child(
                        Button::new("toggle-enabled")
                            .label(toggle_label)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.toggle_selected_enabled(window, cx);
                            })),
                    )
                    .child(
                        Button::new("edit-config")
                            .label("Edit")
                            .on_click(cx.listener(|view, _, window, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.open_edit_dialog(config, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("delete-config")
                            .label("Delete")
                            .on_click(cx.listener(|view, _, window, cx| {
                                if let Some(config) = view.selected_config(cx) {
                                    view.open_remove_confirm(config, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("copy-urls")
                            .label("Copy URLs")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.copy_selected_urls(window, cx);
                            })),
                    )
                    .child(
                        Button::new("engine-start")
                            .label("Start")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.start_selected_engine(window, cx);
                            })),
                    )
                    .child(
                        Button::new("engine-stop")
                            .label("Stop")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.stop_selected_engine(window, cx);
                            })),
                    )
                    .child(
                        Button::new("health-test")
                            .label("Test")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.test_selected_health(window, cx);
                            })),
                    )
                    .child(
                        Button::new("health-ping")
                            .label("Ping")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.ping_selected(window, cx);
                            })),
                    )
                    .child(Button::new("logs-open").label("Logs").on_click(cx.listener(
                        |view, _, window, cx| {
                            view.open_logs_dialog(window, cx);
                        },
                    ))),
            )
            .into_any_element()
    }

    fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let (total, enabled) = self.libraries.read(cx).delegate().counts();
        let engine_text = self.engine.status_text();
        let health_text = self.health_status_text();
        StatusBar::new().left(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("status-count")
                        .test_support()
                        .child(format!("{total} configurations · {enabled} enabled")),
                )
                .child(div().id("engine-status").test_support().child(engine_text))
                .child(div().id("health-status").test_support().child(health_text)),
        )
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drop_view = cx.entity();
        div()
            .id("manyproxy-shell")
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context("manyproxy-library")
            .on_drop(move |paths: &ExternalPaths, window, cx| {
                drop_view.update(cx, |view, cx| {
                    view.import_dropped_files(paths, window, cx);
                });
            })
            .child(TitleBar::new())
            .child(
                div()
                    .id("manyproxy-toolbar")
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_2()
                    .child(Button::new("add-config").label("Add").on_click(cx.listener(
                        |view, _, window, cx| {
                            view.open_add_dialog(window, cx);
                        },
                    )))
                    .child(
                        Button::new("import-config")
                            .label("Import")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
                            })),
                    )
                    .child(
                        Button::new("import-clipboard")
                            .label("From clipboard")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_import_from_clipboard(window, cx);
                            })),
                    )
                    .child(
                        Button::new("export-library")
                            .label("Export")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_export_dialog(window, cx);
                            })),
                    )
                    .child(
                        Button::new("subscriptions")
                            .label("Subscriptions")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_subscriptions_dialog(window, cx);
                            })),
                    )
                    .child(Button::new("engine-start-all").label("Start all").on_click(
                        cx.listener(|view, _, window, cx| {
                            view.start_all_engines(window, cx);
                        }),
                    ))
                    .child(
                        Button::new("engine-stop-all")
                            .label("Stop all")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.stop_all_engines(window, cx);
                            })),
                    )
                    .child(
                        Button::new("health-test-all")
                            .label("Test health")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.test_all_health(window, cx);
                            })),
                    ),
            )
            .child(
                div().flex_1().child(
                    div().id("config-list").test_support().child(
                        List::new(&self.libraries).search_placeholder("Search configurations"),
                    ),
                ),
            )
            .child(self.render_detail(cx))
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(TitleBar::window_options(), |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                view.update(cx, |view, cx| {
                    view.refresh_due_on_startup(cx);
                    view.start_background_ticks(cx);
                });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::store::{NewConfig, Store};
    use crate::AppView;
    use gpui_kit::component::dialog::Dialog;
    use gpui_kit::component::{Root, WindowExt};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{px, size, AppContext, Entity, TestAppContext};

    fn seed_input(name: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: String::new(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        }
    }

    /// Ticket 01, scenario 1: cold start shows an empty library.
    /// Ticket 01, scenario 2: the mounted dialog layer renders a dialog.
    ///
    /// NOTE: compiling this test needs a large rustc stack on Windows
    /// (`RUST_MIN_STACK=67108864`) plus the crate `#![recursion_limit]`
    /// above; both come from the `gpui_kit::test` harness expansion.
    /// Hermetic: uses an in-memory store (never the real app-data file).
    #[gpui_kit::test]
    fn shell_boots_with_empty_library(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);

        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        let list_found = cx.update(|window, _| window.try_find("config-list").is_some());
        assert!(
            list_found,
            "config list renders: missing element, see registered paths in panic context"
        );
        assert!(cx.update(|window, _| window.try_find("status-count").is_some()));

        cx.update(|window, cx| {
            window.open_dialog(cx, |dialog: Dialog, _, _| {
                dialog.title("Add proxy configuration")
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(
            cx.debug_bounds("dialog-0").is_some(),
            "dialog layer paints the dialog"
        );
    }

    /// Ticket 02: stored rows render with stable domain ids; the detail bar
    /// and status bar follow the stored data.
    #[gpui_kit::test]
    fn library_lists_stored_rows(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store.add(seed_input("Work")).expect("seed works");
        let second = store.add(seed_input("Home")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);

        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        for id in [first.id, second.id] {
            let row_id = format!("library-row-{id}");
            assert!(
                cx.update(move |window, _| window.try_find(row_id).is_some()),
                "stored row renders with a stable domain id"
            );
        }
        assert!(cx.update(|window, _| window.try_find("config-detail").is_some()));
    }

    /// Ticket 02: the add dialog opens through WindowExt with all fields.
    #[gpui_kit::test]
    fn add_dialog_opens_with_fields(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_add_dialog(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        for field in ["field-name", "field-protocol", "field-bind", "field-port"] {
            assert!(
                cx.update(|window, _| window.try_find(field).is_some()),
                "add dialog renders {field}"
            );
        }
    }

    /// Ticket 02: edit dialog and delete confirm open for a stored config.
    #[gpui_kit::test]
    fn edit_and_remove_dialogs_open(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let config = store.add(seed_input("Work")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            let config = config.clone();
            view.update(cx, |view, cx| view.open_edit_dialog(config, window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("field-name").is_some()));

        cx.update(|window, cx| window.close_all_dialogs(cx));
        cx.run_until_parked();

        let target = config.clone();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_remove_confirm(target.clone(), window, cx)
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    /// Ticket 03: the import dialog opens with a paste box and a group field,
    /// wiring paste/clipboard/drop entry points to the add flow.
    #[gpui_kit::test]
    fn import_dialog_opens_with_fields(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_import_dialog("vless://x", "travel", None, window, cx)
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("field-import-text").is_some()));
        assert!(cx.update(|window, _| window.try_find("field-import-group").is_some()));
    }

    /// Ticket 03: committing pasted links stores disabled records in the
    /// chosen group without starting anything.
    #[gpui_kit::test]
    fn import_commit_stores_disabled_records(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.commit_import(
                    "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x"
                        .to_string(),
                    "travel".to_string(),
                    None,
                    window,
                    cx,
                ));
            });
        });
        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);
    }

    /// Ticket 11: the Export button opens a confirm dialog carrying the
    /// plaintext-secret warning verbatim; nothing is written until confirm.
    #[gpui_kit::test]
    fn export_dialog_opens_with_verbatim_warning(cx: &mut TestAppContext) {
        use crate::store::export_confirm_description;
        use crate::store::export_warning_text;

        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        // The dialog description is the warning text verbatim by construction.
        assert_eq!(export_confirm_description(), export_warning_text());

        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, _| window.try_find("export-library").is_some()));

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_export_dialog(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    // -- Ticket 12: health persistence + background ticks --------------------

    fn tick12_seed(name: &str, port: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                .to_string(),
            group: String::new(),
            bind_ip: String::new(),
            port: port.to_string(),
        }
    }

    /// Ticks idle with nothing running: no network traffic, no errors, no
    /// crash reports.
    #[gpui_kit::test]
    fn ticks_idle_with_nothing_running_touch_no_network(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;

        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        let (prober, handle) = FakeHealthProber::shared();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                view.prober = Box::new(prober);
            });
        });

        let poll = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
        assert_eq!(poll, None, "no crash without a sidecar");
        let subs = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_due_subscriptions(cx)));
        assert_eq!(subs, 0, "no subs, no fetches");
        let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
        assert_eq!(reprobed, 0, "nothing running, nothing probed");
        assert!(
            handle.calls_for("1").is_empty() && handle.calls_for("2").is_empty(),
            "idle ticks never touch the network"
        );
        assert!(
            view.read_with(cx, |view, _| view.engine.last_error().is_none()),
            "no errors recorded"
        );
    }

    /// The re-probe tick covers running rows only: records + persists, and
    /// never probes stopped rows.
    #[gpui_kit::test]
    fn tick_reprobe_running_records_and_persists(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;

        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let alpha = store
            .add(tick12_seed("Alpha", "10851"))
            .expect("seed works");
        let beta = store.add(tick12_seed("Beta", "10852")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        let (prober, handle) = FakeHealthProber::shared();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                view.prober = Box::new(prober);
            });
        });
        handle.script_latency(&alpha.id, 21);
        handle.script_failure(&beta.id, "upstream refused");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });

        let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
        assert_eq!(reprobed, 2, "both running rows re-probed");
        assert_eq!(handle.calls_for(&alpha.id).len(), 1);
        assert_eq!(handle.calls_for(&beta.id).len(), 1);

        let (alpha_live, beta_dead) = view.read_with(cx, |view, _| {
            (
                view.health_store.status(&alpha.id),
                view.health_store.status(&beta.id),
            )
        });
        assert_eq!(alpha_live.latency_ms, Some(21));
        assert!(beta_dead
            .cause
            .as_deref()
            .unwrap_or("")
            .contains("upstream refused"));
        let snaps = view.read_with(cx, |view, _| {
            view.store.load_health_snapshots().expect("snapshots load")
        });
        assert_eq!(snaps.len(), 2, "latest samples persisted");
    }

    /// Restart restores last latencies + dead flags (file-backed store,
    /// second view hydrates with no new probing).
    #[gpui_kit::test]
    fn restart_restores_last_latency_and_dead_flags(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;

        cx.update(gpui_kit::init);

        let path = std::env::temp_dir().join(format!(
            "manyproxy-tick12-restart-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let (alpha_id, beta_id) = {
            let mut store = Store::open_file(&path).expect("file store opens");
            let alpha = store
                .add(tick12_seed("Alpha", "10853"))
                .expect("seed works");
            let beta = store.add(tick12_seed("Beta", "10854")).expect("seed works");
            (alpha.id.clone(), beta.id.clone())
        };
        let store = Store::open_file(&path).expect("reopen works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        let (prober, handle) = FakeHealthProber::shared();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                view.prober = Box::new(prober);
            });
        });
        handle.script_latency(&alpha_id, 33);
        handle.script_failure(&beta_id, "upstream refused");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        let tested =
            cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
        assert_eq!(tested, 2);
        drop(view);

        // "Restart": a fresh view over the same file, no probing at all.
        let store = Store::open_file(&path).expect("restart reopen works");
        let stash2: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash2_for_window = stash2.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let restarted = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash2_for_window.borrow_mut() = Some(restarted.clone());
            Root::new(restarted, window, cx)
        });
        let restarted = stash2.borrow().clone().expect("AppView stashed");

        let (live, dead) = restarted.read_with(cx, |view, _| {
            (
                view.health_store.status(&alpha_id),
                view.health_store.status(&beta_id),
            )
        });
        assert_eq!(live.latency_ms, Some(33), "latency survives restart");
        assert!(
            dead.cause
                .as_deref()
                .unwrap_or("")
                .contains("upstream refused"),
            "dead flag + cause survive restart: {:?}",
            dead.cause
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Stopped ticks start nothing even with due subs and running rows (no
    /// telemetry, no auto-start, no fetch beyond due subs — here, no fetch
    /// at all).
    #[gpui_kit::test]
    fn stopped_ticks_start_nothing(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;

        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let alpha = store
            .add(tick12_seed("Alpha", "10855"))
            .expect("seed works");
        store
            .add_subscription("Nightly", "https://example.com/sub.txt", "travel", true)
            .expect("due sub (never refreshed)");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        let (prober, handle) = FakeHealthProber::shared();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                view.prober = Box::new(prober);
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        // The due-sub selection itself is pure (one due sub found).
        assert_eq!(
            view.read_with(cx, |view, _| view.due_subscriptions().len()),
            1
        );

        cx.update(|_, cx| {
            view.update(cx, |view, _| view.stop_background_ticks());
        });
        let poll = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
        assert_eq!(poll, None);
        let subs = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_due_subscriptions(cx)));
        assert_eq!(subs, 0, "stopped pass starts no fetch");
        let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
        assert_eq!(reprobed, 0, "stopped pass probes nothing");
        assert!(
            handle.calls_for(&alpha.id).is_empty(),
            "stopped ticks never touch the network"
        );
    }

    // -- Ticket 13: live sidecar log pipe ------------------------------------

    /// Mock sidecar emits tagged lines → drain → the logs dialog shows them
    /// under the right config with secrets redacted; an untagged line stays
    /// engine-global (never cross-config).
    #[gpui_kit::test]
    fn live_sidecar_lines_appear_under_right_config_redacted(cx: &mut TestAppContext) {
        use crate::engine::{inbound_tag, EngineController};
        use gpui_kit::component::IndexPath;

        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let alpha = store.add(seed_input("Work")).expect("seed works");
        let beta = store.add(seed_input("Play")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        let stock = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (controller, stock, _fork, _xray, _extra) = EngineController::fake_quartet();
                view.engine = controller;
                stock
            })
        });
        stock.emit_log(&format!(
            "warn {}: dial failed password=hunter2",
            inbound_tag(&alpha.id)
        ));
        stock.emit_log("info engine listening on 127.0.0.1:10801");

        cx.update(|_, cx| {
            view.update(cx, |view, _| view.drain_engine_logs());
        });

        let (alpha_lines, beta_lines) = view.read_with(cx, |view, _| {
            (
                view.logs
                    .for_config(&alpha.id)
                    .iter()
                    .map(|record| record.message.clone())
                    .collect::<Vec<_>>(),
                view.logs.for_config(&beta.id).len(),
            )
        });
        assert_eq!(alpha_lines.len(), 1, "only its lines show");
        assert!(
            !alpha_lines[0].contains("hunter2"),
            "redacted on ingest: {}",
            alpha_lines[0]
        );
        assert!(
            alpha_lines[0].contains(&inbound_tag(&alpha.id)),
            "tag preserved: {}",
            alpha_lines[0]
        );
        assert_eq!(beta_lines, 0, "no cross-config attribution");

        // The dialog renders the live lines for the selected row.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
                view.reload(window, cx);
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("log-lines").is_some()));
    }

    /// Ticket 04: the subscriptions dialog opens with a label/URL/group
    /// form for the add flow.
    #[gpui_kit::test]
    fn subscriptions_dialog_opens_with_fields(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("field-sub-label").is_some()));
        assert!(cx.update(|window, _| window.try_find("field-sub-url").is_some()));
        assert!(cx.update(|window, _| window.try_find("field-sub-group").is_some()));
    }

    /// Ticket 05: start-all over the mock driver brings up one endpoint per
    /// config (generated JSON asserted structurally); stop-all takes them
    /// down; a port collision blocks start naming both configs. The
    /// engine-status element and start/stop controls render headless.
    #[gpui_kit::test]
    fn engine_start_all_and_stop_all_via_fake(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::store::ConfigPatch;

        cx.update(gpui_kit::init);

        fn engine_seed(name: &str, port: &str) -> NewConfig {
            NewConfig {
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                    .to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: port.to_string(),
            }
        }

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let alpha = store
            .add(engine_seed("Alpha", "10831"))
            .expect("seed works");
        let beta = store.add(engine_seed("Beta", "10832")).expect("seed works");
        let _gamma = store
            .add(engine_seed("Gamma", "10833"))
            .expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        // Swap the real driver for the mock; production never starts here.
        let handle = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, handle) = EngineController::fake();
                view.engine = engine;
                handle
            })
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 3);
        assert!(view.read_with(cx, |view, _| view.engine.is_running(&alpha.id)));
        let recorded: serde_json::Value =
            serde_json::from_str(&handle.last_config().expect("mock recorded the config"))
                .expect("generated config is JSON");
        assert_eq!(recorded["inbounds"].as_array().expect("inbounds").len(), 3);
        assert_eq!(
            recorded["route"]["rules"].as_array().expect("rules").len(),
            3
        );
        assert_eq!(recorded["route"]["final"], serde_json::json!("direct"));

        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, _| window.try_find("engine-status").is_some()));
        assert!(cx.update(|window, _| window.try_find("engine-start-all").is_some()));
        assert!(cx.update(|window, _| window.try_find("engine-stop-all").is_some()));

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.stop_all_engines(window, cx));
        });
        assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 0);
        assert!(!handle.is_running());

        // Collision path: two rows on one port block start, naming both.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let _ = view.store.update(
                    &alpha.id,
                    ConfigPatch {
                        port: Some("10901".to_string()),
                        ..Default::default()
                    },
                );
                let _ = view.store.update(
                    &beta.id,
                    ConfigPatch {
                        port: Some("10901".to_string()),
                        ..Default::default()
                    },
                );
                view.start_all_engines(window, cx);
            });
        });
        assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 0);
        let error = view.read_with(cx, |view, _| {
            view.engine.last_error().unwrap_or("").to_string()
        });
        assert!(
            error.contains("Alpha") && error.contains("Beta"),
            "conflict names both configs, got {error}"
        );
    }

    /// Ticket 06: health test-all over the mock driver records per-row
    /// latency, flags the dead row with its named cause, and renders the
    /// health line, status totals, and toolbar controls headless.
    #[gpui_kit::test]
    fn health_test_all_via_fake_flags_dead(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;

        cx.update(gpui_kit::init);

        fn health_seed(name: &str, port: &str) -> NewConfig {
            NewConfig {
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                    \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
                    .to_string(),
                group: String::new(),
                bind_ip: String::new(),
                port: port.to_string(),
            }
        }

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let alpha = store
            .add(health_seed("Alpha", "10841"))
            .expect("seed works");
        let beta = store.add(health_seed("Beta", "10842")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                let (prober, handle) = FakeHealthProber::shared();
                handle.script_latency(&alpha.id, 33);
                handle.script_failure(&beta.id, "upstream refused");
                view.prober = Box::new(prober);
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        let tested =
            cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
        assert_eq!(tested, 2);

        let (alpha_live, beta_dead) = view.read_with(cx, |view, _| {
            (
                view.health_store.status(&alpha.id),
                view.health_store.status(&beta.id),
            )
        });
        assert_eq!(alpha_live.latency_ms, Some(33));
        assert!(
            beta_dead
                .cause
                .as_deref()
                .unwrap_or("")
                .contains("upstream refused"),
            "dead row names cause: {:?}",
            beta_dead.cause
        );
        assert_eq!(view.read_with(cx, |view, _| view.health_counts()), (1, 1));

        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, _| window.try_find("health-test-all").is_some()));
        assert!(cx.update(|window, _| window.try_find("health-status").is_some()));
    }

    /// Ticket 06: the logs dialog opens with a per-config filter field and
    /// shows only the selected config's redacted lines.
    #[gpui_kit::test]
    fn logs_dialog_filters_to_selected_and_redacts(cx: &mut TestAppContext) {
        use crate::engine::inbound_tag;
        use gpui_kit::component::IndexPath;

        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let config = store.add(seed_input("Work")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
                let rows = vec![config.clone()];
                view.logs.push_raw(
                    &format!(
                        "warn {}: dial failed password=hunter2",
                        inbound_tag(&config.id)
                    ),
                    &rows,
                );
                view.logs.push_raw("info engine started", &rows);
            });
        });

        let lines = view.read_with(cx, |view, _cx| {
            view.logs
                .for_config(&config.id)
                .iter()
                .map(|record| record.message.clone())
                .collect::<Vec<_>>()
        });
        assert_eq!(lines.len(), 1, "only its lines show");
        assert!(!lines[0].contains("hunter2"), "redacted: {}", lines[0]);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("field-log-filter").is_some()));
        assert!(cx.update(|window, _| window.try_find("log-lines").is_some()));
    }

    // -- Ticket 09: hardening + release proof --------------------------------

    /// Subscription labels never carry credentials: userinfo and query are
    /// both stripped (the only network surface besides health probes is the
    /// user-initiated subscription refresh, and its errors never echo URLs).
    #[test]
    fn subscription_label_strips_credentials_and_query() {
        use crate::subscription_label_from_url;
        assert_eq!(
            subscription_label_from_url("https://example.com/sub?id=42"),
            "example.com"
        );
        let labeled =
            subscription_label_from_url("https://user:hunter2@example.com:8443/sub?token=abc");
        assert!(!labeled.contains("hunter2"), "no password: {labeled}");
        assert!(!labeled.contains("token=abc"), "no query: {labeled}");
        assert!(labeled.contains("example.com"), "host kept: {labeled}");
        assert_eq!(subscription_label_from_url(""), "Subscription");
    }

    /// Copy-URL clipboard content is bind + port only: no params, keys, or
    /// tokens can reach the clipboard through this path by construction.
    #[test]
    fn copy_urls_carry_binds_not_secrets() {
        use crate::store::proxy_urls;
        let (socks, http) = proxy_urls("127.0.0.1", 10801);
        assert_eq!(socks, "socks5://127.0.0.1:10801");
        assert_eq!(http, "http://127.0.0.1:10801");
        for canary in ["hunter2", "LOCAL-PRIVATE", "SECRET-UUID", "token=abc"] {
            assert!(!socks.contains(canary));
            assert!(!http.contains(canary));
        }
    }

    fn seed_lan_input(name: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: String::new(),
            group: String::new(),
            bind_ip: "0.0.0.0".to_string(),
            port: String::new(),
        }
    }

    /// LAN single-start stops at the confirm gate: a dialog opens and the
    /// engine stays stopped until the user confirms.
    #[gpui_kit::test]
    fn lan_single_start_opens_confirm_and_stays_stopped(cx: &mut TestAppContext) {
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_lan_input("Lan")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
                view.start_selected_engine(window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            0,
            "nothing starts before confirm"
        );
    }

    /// LAN start-all stops at the confirm gate too: one dialog naming the
    /// LAN rows, engine stopped until confirm.
    #[gpui_kit::test]
    fn lan_start_all_opens_confirm_and_stays_stopped(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Loop")).expect("seed works");
        store.add(seed_lan_input("Lan")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });

        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            0,
            "nothing starts before confirm"
        );
    }

    /// Loopback single-start skips the confirm gate (no dialog); the start
    /// itself fails here only because no real sidecar binary ships in tests.
    #[gpui_kit::test]
    fn loopback_single_start_skips_confirm(cx: &mut TestAppContext) {
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);

        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Loop")).expect("seed works");

        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
                view.start_selected_engine(window, cx);
            });
        });
        cx.run_until_parked();

        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    // -- Ticket 14: import-safety ------------------------------------------

    use gpui_kit::VisualTestContext;

    fn mount_view(
        store: Store,
        cx: &mut TestAppContext,
    ) -> (Entity<AppView>, &mut VisualTestContext) {
        let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
        let stash_for_window = stash.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AppView::with_store(window, cx, store));
            *stash_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = stash.borrow().clone().expect("AppView stashed");
        (view, cx)
    }

    fn notification_count(cx: &mut VisualTestContext) -> usize {
        cx.update(|window, cx| window.notifications(cx).len())
    }

    /// Scenario: subscription URL with userinfo → userinfo never displayed.
    #[test]
    fn redact_subscription_url_strips_userinfo() {
        use crate::redact_subscription_url;
        let shown =
            redact_subscription_url("https://user:hunter2@example.com:8443/sub?token=abc#frag");
        assert!(!shown.contains("hunter2"), "no password: {shown}");
        assert!(!shown.contains("user:"), "no userinfo: {shown}");
        assert!(!shown.contains("token=abc"), "no query: {shown}");
        assert!(!shown.contains("#frag"), "no fragment: {shown}");
        assert_eq!(shown, "https://example.com:8443/sub");
        assert_eq!(
            redact_subscription_url("https://example.com/sub"),
            "https://example.com/sub"
        );
        let long = format!("https://user:pw@example.com/{}", "x".repeat(200));
        let shown = redact_subscription_url(&long);
        assert!(
            !shown.contains("user:pw"),
            "stripped before truncate: {shown}"
        );
        assert!(shown.ends_with('…'), "still truncated: {shown}");
    }

    /// Scenario: bad addsub link → original reason, never generic
    /// unknown-command.
    #[test]
    fn addsub_error_message_surfaces_original_reason() {
        use crate::addsub_error_message;
        use crate::import::parse_addsub_link;
        let bad_b64 = parse_addsub_link("throne://addsub/!!!").expect_err("bad base64 fails");
        let text = addsub_error_message(&bad_b64);
        assert!(
            text.contains("bad-base64"),
            "original kind kept, got {text}"
        );
        assert!(
            !text.contains("unknown throne command"),
            "not generic: {text}"
        );
        let bad_host = parse_addsub_link("throne://addsub/aGk").expect_err("non-http fails");
        let text = addsub_error_message(&bad_host);
        assert!(text.contains("bad-host"), "original kind kept, got {text}");
    }

    /// Scenario: paste with all-invalid items → dialog stays open (false),
    /// one notification, nothing stored.
    #[gpui_kit::test]
    fn import_commit_all_invalid_stays_open(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let body = "vless://not-a-uuid@b.example.com:notaport#bad\nbogus://x\ntrojan://pw@c.example.com:99999";
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import(body.to_string(), "g".to_string(), None, window, cx)
            })
        });
        assert!(!closed, "total failure keeps the dialog open");
        assert_eq!(notification_count(cx), 1, "all failures in one notice");
        assert_eq!(
            view.read_with(cx, |view, cx| view.config_count(cx)),
            0,
            "nothing stored"
        );
    }

    /// Scenario: bad addsub link → dialog stays open with the original
    /// reason instead of the generic unknown-command.
    #[gpui_kit::test]
    fn import_commit_bad_addsub_stays_open(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import(
                    "throne://addsub/!!!".to_string(),
                    "g".to_string(),
                    None,
                    window,
                    cx,
                )
            })
        });
        assert!(!closed, "bad addsub keeps the dialog open");
        assert_eq!(notification_count(cx), 1);
    }

    /// A non-addsub all-invalid single line still takes the generic import
    /// path — and stays open too.
    #[gpui_kit::test]
    fn import_commit_generic_single_invalid_stays_open(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import("bogus://x".to_string(), "g".to_string(), None, window, cx)
            })
        });
        assert!(!closed, "all-invalid single line stays open too");
        assert_eq!(notification_count(cx), 1);
    }

    /// Scenario: DB error on toggle → error notification, flag unchanged
    /// (never a dead click).
    #[gpui_kit::test]
    fn db_error_on_toggle_notifies(cx: &mut TestAppContext) {
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
                view.store
                    .set_query_only_for_test()
                    .expect("readonly works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.toggle_selected_enabled(window, cx));
        });
        assert_eq!(notification_count(cx), 1, "toggle failure notifies");
        let enabled = view.read_with(cx, |view, cx| {
            view.selected_config(cx).map(|row| row.enabled)
        });
        assert_eq!(enabled, Some(false), "failed toggle changes nothing");
    }

    /// Scenario: DB error on list → error notification, never a silent
    /// empty list.
    #[gpui_kit::test]
    fn db_error_on_reload_notifies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.break_schema_for_test().expect("break works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.reload(window, cx));
        });
        assert!(notification_count(cx) >= 1, "list failure notifies");
    }

    /// Scenario: DB error on manual refresh → error notification, never a
    /// dead click.
    #[gpui_kit::test]
    fn db_error_on_subscription_refresh_notifies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.break_schema_for_test().expect("break works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.refresh_subscription_by_id(&sub_id, window, cx)
            });
        });
        assert_eq!(notification_count(cx), 1, "refresh failure notifies");
    }

    /// A refresh for an unknown id warns without touching the network.
    #[gpui_kit::test]
    fn refresh_unknown_subscription_warns_without_fetch(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.refresh_subscription_by_id("999", window, cx)
            });
        });
        assert_eq!(notification_count(cx), 1, "gone sub warns, no fetch");
    }

    /// Scenario: DB error on the auto-refresh toggle → error notification.
    #[gpui_kit::test]
    fn db_error_on_subscription_auto_toggle_notifies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.break_schema_for_test().expect("break works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_subscription_auto(&sub_id, true, window, cx)
            });
        });
        assert_eq!(notification_count(cx), 1, "auto-toggle failure notifies");
    }

    /// Scenario: remove subscription with a broken store → error
    /// notification, never "Already removed".
    #[gpui_kit::test]
    fn remove_subscription_db_error_is_truthful(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.break_schema_for_test().expect("break works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_subscription_remove(&sub_id, window, cx)
            });
        });
        // Exactly one notice: the store truthfully reports Err (never
        // Ok(false)), so the error arm fires — pinned at the store seam too.
        assert_eq!(notification_count(cx), 1);
    }

    /// Scenario: subscriptions dialog with a broken store → dialog still
    /// opens (form usable) plus an error notification, never a lying empty
    /// state.
    #[gpui_kit::test]
    fn subscriptions_dialog_db_error_notifies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.break_schema_for_test().expect("break works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(notification_count(cx) >= 1, "listing failure notifies");
    }

    /// Scenario: background refresh for a deleted sub → silently ignored,
    /// no failure toast, for both body and fetch-error completions.
    #[gpui_kit::test]
    fn background_refresh_deleted_sub_is_silent(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let label = sub.label.clone();
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store
                    .remove_subscription(&sub_id)
                    .expect("remove works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.finish_subscription_refresh(
                    &sub_id,
                    &label,
                    Ok("bogus://x".to_string()),
                    999,
                    window,
                    cx,
                );
                view.finish_subscription_refresh(
                    &sub_id,
                    &label,
                    Err("subscription fetch failed".to_string()),
                    999,
                    window,
                    cx,
                );
            });
        });
        assert_eq!(notification_count(cx), 0, "deleted sub: no failure toast");
    }

    /// Guard: a live subscription's refresh failure still notifies.
    #[gpui_kit::test]
    fn background_refresh_live_sub_failure_still_notifies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let label = sub.label.clone();
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.finish_subscription_refresh(
                    &sub_id,
                    &label,
                    Ok("bogus://x\nbogus://y".to_string()),
                    999,
                    window,
                    cx,
                );
            });
        });
        assert_eq!(notification_count(cx), 1, "live sub failure notifies");
    }

    // -- Ticket 16: validation coherence + dedupe/group consistency --------

    /// Scenario 4 (pure): an edited dialog group beats the payload group.
    #[test]
    fn effective_addsub_group_dialog_wins() {
        use crate::effective_addsub_group;
        assert_eq!(effective_addsub_group("edited", "payload-group"), "edited");
        assert_eq!(effective_addsub_group("", "payload-group"), "payload-group");
        assert_eq!(
            effective_addsub_group("default", "payload-group"),
            "payload-group",
            "untouched default keeps the payload group"
        );
        assert_eq!(effective_addsub_group("default", "default"), "default");
        assert_eq!(
            effective_addsub_group("  edited  ", "payload-group"),
            "edited"
        );
    }

    /// Scenario 7 (pure): the toast equals re-importable rows, or states
    /// both numbers when hand-made rows exist.
    #[test]
    fn export_count_message_states_both_numbers() {
        use crate::export_count_message;
        assert_eq!(
            export_count_message(3, 3, "/tmp/x.json"),
            "Exported 3 config(s) to /tmp/x.json"
        );
        let both = export_count_message(4, 3, "/tmp/x.json");
        assert!(both.contains('4'), "total in: {both}");
        assert!(both.contains('3'), "importable in: {both}");
        assert!(both.contains("/tmp/x.json"), "path in: {both}");
    }

    /// Scenario 5 (commit): dropping a `.conf` then editing the box to links
    /// parses the edited text, not the old file type.
    #[gpui_kit::test]
    fn import_commit_stale_conf_hint_parses_edited_links(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let links = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import(
                    links.to_string(),
                    "travel".to_string(),
                    Some("drop.conf".to_string()),
                    window,
                    cx,
                )
            })
        });
        assert!(closed, "edited links commit and close");
        let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].protocol, "vless");
        assert_eq!(rows[0].group, "travel");
    }

    /// Scenario 6 (commit): an unedited multi-file drop keeps per-file stems.
    #[gpui_kit::test]
    fn import_commit_multi_files_keep_stems(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let b = "[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";
        let merged = format!("{a}\n{b}");
        let files = vec![
            ("a.conf".to_string(), a.to_string()),
            ("b.conf".to_string(), b.to_string()),
        ];
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import_with_context(
                    merged,
                    "travel".to_string(),
                    None,
                    None,
                    Some(files),
                    window,
                    cx,
                )
            })
        });
        assert!(closed, "multi commit closes");
        let mut names = view.read_with(cx, |view, _| {
            view.store
                .list()
                .expect("list works")
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>()
        });
        names.sort();
        assert_eq!(names, vec!["a-p1".to_string(), "b-p1".to_string()]);
    }

    /// Scenario 6 (edited multi): editing the merged text drops the per-file
    /// bytes and sniffs the edited text instead.
    #[gpui_kit::test]
    fn import_commit_edited_multi_parses_edited_text(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        let files = vec![("a.conf".to_string(), a.to_string())];
        let links = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import_with_context(
                    links.to_string(),
                    "travel".to_string(),
                    Some("a.conf".to_string()),
                    Some(a.to_string()),
                    Some(files),
                    window,
                    cx,
                )
            })
        });
        assert!(closed, "edited text commits");
        let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].protocol, "vless");
    }
}
