#![recursion_limit = "512"]

mod engine;
mod import;
mod store;

use std::collections::HashMap;
use std::sync::Arc;

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

use engine::{lan_exposure_warning, EngineController};
use import::{commit_batch, import_file_content, import_text, parse_addsub_link, AddSubRequest};
use store::{
    delete_confirm_text, due_for_refresh, now_unix, proxy_urls, ConfigPatch, NewConfig,
    ProxyConfig, Store, StoreError, Subscription as ProxySubscription, DEFAULT_AUTO_REFRESH_SECS,
    DEFAULT_BIND, DEFAULT_GROUP, DEFAULT_PROTOCOL,
};

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
}

impl LibraryDelegate {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            visible: Vec::new(),
            query: String::new(),
            selected: None,
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
        Some(
            ListItem::new(format!("library-row-{}", row.id))
                .selected(selected)
                .child(format!("{} [{status}]", row.name))
                .child(format!(
                    "{} · {} · {}:{}",
                    row.protocol, row.group, row.bind_ip, row.port
                )),
        )
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
/// so `.conf` content keeps its stem for peer names and parses as conf even
/// after edits; otherwise the commit path sniffs the text.
#[derive(Clone)]
struct ImportForm {
    text: Entity<TextareaState>,
    group: Entity<InputState>,
    file_hint: Option<String>,
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
        }
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
fn redact_subscription_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    const LIMIT: usize = 96;
    if without_query.chars().count() > LIMIT {
        let kept: String = without_query.chars().take(LIMIT).collect();
        format!("{kept}…")
    } else {
        without_query.to_string()
    }
}

/// Ticket 04: default subscription label derived from the URL host (never
/// the query, which may carry a token).
fn subscription_label_from_url(url: &str) -> String {
    let after = url.split("://").nth(1).unwrap_or(url);
    let host = after.split('/').next().unwrap_or(after);
    if host.trim().is_empty() {
        "Subscription".to_string()
    } else {
        host.to_string()
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
                    if let Some(sub) = view.store.get_subscription(&sub_id).unwrap_or(None) {
                        view.start_subscription_refresh(sub, window, cx);
                    }
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
                    if view
                        .store
                        .set_subscription_auto_refresh(&sub_id, !auto_now)
                        .is_ok()
                    {
                        view.reload(cx);
                    }
                    view.reopen_subscriptions_if_open(window, cx);
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
            _libraries_subscription: cx.subscribe(
                &libraries,
                |_view, _state, _event: &ListEvent, cx| {
                    cx.notify();
                },
            ),
        };
        let rows = view.store.list().unwrap_or_default();
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

    /// Reload rows from the store, keeping the selection by id.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.poll_engine();
        if let Ok(rows) = self.store.list() {
            self.libraries.update(cx, |state, _| {
                state.delegate_mut().set_rows(rows);
            });
        }
        cx.notify();
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
                self.reload(cx);
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
                self.reload(cx);
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
                self.reload(cx);
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

    fn toggle_selected_enabled(&mut self, cx: &mut Context<Self>) {
        if let Some(config) = self.selected_config(cx) {
            if self.store.set_enabled(&config.id, !config.enabled).is_ok() {
                self.reload(cx);
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
    fn poll_engine(&mut self) {
        let _ = self.engine.poll();
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
    fn start_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        let id = config.id.clone();
        if let Some(warning) = lan_exposure_warning(&config.name, &config.bind_ip) {
            window.push_notification(Notification::warning(warning), cx);
        }
        match self.engine.start_one(&config) {
            Ok(()) => {
                let _ = self.store.set_enabled(&id, true);
                self.reload(cx);
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
            self.reload(cx);
            window.push_notification(
                Notification::success(format!("Stopped \"{}\"", config.name)),
                cx,
            );
        }
    }

    /// Ticket 05: start every stored config at once (each keeps its own
    /// endpoint; the enabled flag follows the engine state).
    fn start_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.store.list().unwrap_or_default();
        match self.engine.start_all(&rows) {
            Ok(()) => {
                for row in &rows {
                    let _ = self.store.set_enabled(&row.id, true);
                }
                self.reload(cx);
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
        self.reload(cx);
        window.push_notification(Notification::success("Stopped all endpoints"), cx);
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
        let form = ImportForm::new(window, cx, prefill_text, prefill_group, file_hint);
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
                        view.update(cx, |view, cx| {
                            view.commit_import(text, group, hint, window, cx)
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
    fn import_dropped_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut merged = String::new();
        let mut names: Vec<String> = Vec::new();
        for path in paths.0.iter() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if !merged.is_empty() {
                    merged.push('\n');
                }
                merged.push_str(&content);
                names.push(path.to_string_lossy().into_owned());
            }
        }
        if merged.trim().is_empty() {
            window.push_notification(Notification::warning("No readable files dropped"), cx);
            return;
        }
        let file_hint = if names.len() == 1 {
            names.into_iter().next()
        } else {
            None
        };
        self.open_import_dialog(&merged, DEFAULT_GROUP, file_hint, window, cx);
    }

    /// Ticket 03: commit a paste box through the existing add flow. Valid
    /// items become disabled records in `group`; failures are reported with
    /// item index + reason (never partial silent import); identical
    /// re-imports add nothing. Import never starts configs.
    /// Ticket 04: a lone `throne://addsub/...` line diverts to the
    /// subscription intake instead (record + immediate first refresh).
    fn commit_import(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = text.trim();
        if !trimmed.is_empty() && !trimmed.contains('\n') && file_hint.is_none() {
            if let Ok(request) = parse_addsub_link(trimmed) {
                return self.commit_addsub_request(request, window, cx);
            }
        }
        let batch = match file_hint {
            Some(name) => import_file_content(&name, &text),
            None => import_text(&text),
        };
        match commit_batch(&mut self.store, batch, &group) {
            Ok(outcome) => {
                self.reload(cx);
                let mut summary = format!(
                    "Imported {} · {} duplicates skipped · {} failed",
                    outcome.imported,
                    outcome.duplicates,
                    outcome.failures.len()
                );
                if let Some(first) = outcome.failures.first() {
                    summary.push_str(&format!(
                        " — item {} ({}): {}",
                        first.index + 1,
                        first.label,
                        first.reason
                    ));
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
    fn commit_addsub_request(
        &mut self,
        request: AddSubRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let label = subscription_label_from_url(&request.url);
        self.commit_subscription_add(label, request.url, request.group, window, cx)
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
                self.reload(cx);
                window.push_notification(
                    Notification::success(format!(
                        "Subscription \"{}\" added — refreshing",
                        sub.label
                    )),
                    cx,
                );
                self.start_subscription_refresh(sub, window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    /// Ticket 04: fetch one subscription without blocking the UI, then merge
    /// through the tested [`Store::refresh_with_body`] seam.
    fn start_subscription_refresh(
        &mut self,
        sub: ProxySubscription,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        match fetched {
            Ok(body) => match self.store.refresh_with_body(sub_id, &body, now) {
                Ok(outcome) => {
                    self.reload(cx);
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
                    window.push_notification(
                        Notification::error(format!("{label}: refresh failed ({error})")),
                        cx,
                    );
                }
            },
            Err(error) => {
                let _ = self.store.record_subscription_error(sub_id, &error);
                self.reload(cx);
                window.push_notification(Notification::error(format!("{label}: {error}")), cx);
            }
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
        self.reload(cx);
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
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let rows = self.store.list().unwrap_or_default();
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
                        match view.store.remove_subscription(&id) {
                            Ok(true) => {
                                view.reload(cx);
                            }
                            _ => {
                                window.push_notification(
                                    Notification::warning("Already removed"),
                                    cx,
                                );
                            }
                        }
                        view.reopen_subscriptions_if_open(window, cx);
                    });
                    true
                })
        });
    }

    /// Ticket 04: refresh due subscriptions once at startup (the auto-refresh
    /// scheduling hook evaluation). Never called by tests, so no headless
    /// test ever touches the network.
    fn refresh_due_on_startup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = now_unix();
        let due: Vec<ProxySubscription> = self
            .store
            .list_subscriptions()
            .unwrap_or_default()
            .into_iter()
            .filter(|sub| due_for_refresh(sub, now, DEFAULT_AUTO_REFRESH_SECS))
            .collect();
        for sub in due {
            self.start_subscription_refresh(sub, window, cx);
        }
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
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new(switch_id)
                            .checked(config.enabled)
                            .label("Enabled")
                            .on_change(move |enabled: &bool, _window, cx| {
                                let enabled = *enabled;
                                let id = switch_id_for_handler.clone();
                                switch_view.update(cx, |view, cx| {
                                    if view.store.set_enabled(&id, enabled).is_ok() {
                                        view.reload(cx);
                                    }
                                });
                            }),
                    )
                    .child(
                        Button::new("toggle-enabled")
                            .label(toggle_label)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.toggle_selected_enabled(cx);
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
                    ),
            )
            .into_any_element()
    }

    fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let (total, enabled) = self.libraries.read(cx).delegate().counts();
        let engine_text = self.engine.status_text();
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
                .child(div().id("engine-status").test_support().child(engine_text)),
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
                view.update(cx, |view, cx| view.refresh_due_on_startup(window, cx));
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
}
