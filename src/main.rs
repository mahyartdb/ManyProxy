#![recursion_limit = "512"]

mod dpapi;
mod engine;
mod health;
mod import;
mod store;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::{Dialog, DialogButtonProps, DialogFooter};
use gpui_kit::component::input::{AnyInputState, Input, InputState, Textarea, TextareaState};
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
    engine_binary_file_name, fork_binary_file_name, lan_confirm_description, lan_confirm_text,
    lan_exposure_warning, requires_lan_confirm, resolve_engine_binary, resolve_fork_binary,
    resolve_xray_binary, strip_ansi_codes, xray_binary_file_name, EngineController, EngineError,
    LAN_CONFIRM_TITLE,
};
use health::{
    ping_all, redact_secrets, test_all, truncate, HealthProber, HealthStore, LogStore,
    RealHealthProber, TestOutcome, DEFAULT_TEST_URL,
};
use import::{
    commit_batch, effective_hint_for_text, format_import_failures, import_file_content,
    import_multi_file_contents, import_text, is_addsub_attempt, is_library_export,
    parse_addsub_link, AddSubRequest, ImportError,
};
use store::{
    count_importable, delete_confirm_text, due_for_refresh, export_confirm_description, now_unix,
    protocol_params_template, proxy_urls, ConfigPatch, HealthSnapshot, NewConfig, ProxyConfig,
    RestoreOutcome, Store, StoreError, Subscription as ProxySubscription,
    DEFAULT_AUTO_REFRESH_SECS, DEFAULT_BIND, DEFAULT_GROUP, DEFAULT_PROTOCOL,
};

// Ticket 26: application keyboard actions. Bound to [`APP_KEY_CONTEXT`] by
// [`bind_app_keys`]; the shell div handles them via `.on_action(...)`.
// Every handler is a no-op while any dialog is open, so shortcut keys never
// steal dialog input (the dialog keeps its own Confirm/Cancel keys).
actions!(
    manyproxy,
    [
        AddConfig,
        ImportConfigs,
        StartSelected,
        StopSelected,
        TestSelected
    ]
);

/// Ticket 26: key context owning the five app shortcuts (was the bare
/// `"manyproxy-library"` literal with no bindings — a dead context).
pub const APP_KEY_CONTEXT: &str = "manyproxy-library";

/// Ticket 26: register the app shortcuts under [`APP_KEY_CONTEXT`].
/// Modifier-only chords (never printable characters) so list search typing
/// is unaffected; dialogs are guarded at the handler, not the binding.
pub fn bind_app_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-n", AddConfig, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-o", ImportConfigs, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("f5", StartSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("shift-f5", StopSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-t", TestSelected, Some(APP_KEY_CONTEXT)),
    ]);
}

/// Ticket 12: background tick intervals. Each tick is bounded (engine poll
/// is O(1); the subscription pass only starts already-due subs; the
/// re-probe only covers running rows with the existing 5s per-test bound)
/// and stoppable (see [`AppView::stop_background_ticks`]; the loop also
/// exits when the view is released).
pub const ENGINE_POLL_TICK_SECS: u64 = 5;
pub const DUE_SUBSCRIPTION_TICK_SECS: u64 = 300;
pub const REPROBE_TICK_SECS: u64 = 60;

/// Ticket 21: single branding seam — the OS window title and the TitleBar
/// caption both read this name (the exe icon is `assets/icon.ico`, wired
/// through `manyproxy.rc` + `build.rs`).
pub const APP_TITLE: &str = "ManyProxy";

/// Ticket 21: window options with the branded OS title. The base is
/// [`TitleBar::window_options`]; `WindowOptions` itself has no title field —
/// the initial title lives in `TitlebarOptions::title`, which the base
/// leaves as `None`.
fn app_window_options() -> WindowOptions {
    let mut options = TitleBar::window_options();
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(APP_TITLE.into());
    }
    options
}

/// Ticket 33: crate version stamped into About (from `Cargo.toml`).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Ticket 33: git hash stamped by `build.rs` (`MANYPROXY_GIT_HASH`); plain
/// dev builds without git metadata show the fallback, never a build failure.
pub const GIT_HASH: &str = match option_env!("MANYPROXY_GIT_HASH") {
    Some(hash) => hash,
    None => "unknown (dev build)",
};

/// Ticket 33: the five app shortcuts in one place — the About dialog and the
/// toolbar hint labels both read this, so the visible hints can never drift
/// from the [`bind_app_keys`] bindings.
pub const SHORTCUT_LINES: &[(&str, &str)] = &[
    ("Add configuration", "Ctrl-N"),
    ("Import configurations", "Ctrl-O"),
    ("Start selected", "F5"),
    ("Stop selected", "Shift-F5"),
    ("Test selected", "Ctrl-T"),
];

/// Ticket 33: close-confirm text for a window close with running endpoints.
/// `None` when nothing runs (close proceeds silently — nothing to kill).
/// `Some` names every running row so the confirm can never be a blind kill.
fn close_confirm_description(running_names: &[String]) -> Option<String> {
    if running_names.is_empty() {
        return None;
    }
    let named = running_names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Closing now stops {} running endpoint(s): {named}. Stop all and close?",
        running_names.len()
    ))
}

/// Ticket 33: one sidecar-presence line for About: the expected file name
/// plus whether it resolved in the app directory (never a download — a
/// missing binary stays a missing binary).
fn sidecar_presence_line(file_name: &str, found: bool) -> String {
    if found {
        format!("{file_name}: found (app dir)")
    } else {
        format!("{file_name}: missing (place it next to the app exe)")
    }
}

/// Ticket 33: About dialog body — version + git hash + sidecar presence +
/// data paths + shortcuts list. Pure so tests pin the complete content.
fn about_body(
    version: &str,
    git_hash: &str,
    sidecars: &[String],
    db_path: &str,
    export_path: &str,
) -> String {
    let mut out = format!("ManyProxy v{version} (git {git_hash})\n\nSidecars (app dir only):\n");
    for line in sidecars {
        out.push_str(&format!("  {line}\n"));
    }
    out.push_str(&format!(
        "\nData:\n  library: {db_path}\n  export default: {export_path}\n"
    ));
    out.push_str("\nNo autostart, no registry writes, no auto-update in v1.\n");
    out.push_str("\nShortcuts:\n");
    for (action, keys) in SHORTCUT_LINES {
        out.push_str(&format!("  {action}: {keys}\n"));
    }
    out
}

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
    /// Ticket 30: serving truth per row (`config_id` set while running).
    /// The badge renders from this — never from the enabled flag.
    running: HashSet<String>,
}

impl LibraryDelegate {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            visible: Vec::new(),
            query: String::new(),
            selected: None,
            health: HashMap::new(),
            running: HashSet::new(),
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
        // Ticket 26: re-derive the selection by config id (the `set_rows`
        // pattern), never by stale index. A narrowed filter clears a
        // filtered-out selection instead of landing actions on the wrong row.
        let selected_id = self.selected_row().map(|row| row.id.clone());
        let query = self.query.clone();
        self.visible = self
            .rows
            .iter()
            .enumerate()
            // Ticket 17: search covers host/bind/port, not just
            // name/protocol/group (see [`library_row_matches`]).
            .filter(|(_, row)| library_row_matches(row, &query))
            .map(|(index, _)| index)
            .collect();
        self.selected = selected_id.and_then(|id| {
            self.visible
                .iter()
                .position(|&index| self.rows[index].id == id)
                .map(IndexPath::new)
        });
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

    /// Ticket 30: replace the serving set (`config_id`s currently running).
    /// Badges render from this — never from the enabled flag.
    fn set_running(&mut self, ids: Vec<String>) {
        self.running = ids.into_iter().collect();
    }

    /// (total rows, enabled rows) for the status bar.
    fn counts(&self) -> (usize, usize) {
        (
            self.rows.len(),
            self.rows.iter().filter(|row| row.enabled).count(),
        )
    }

    /// Ticket 30: (visible rows, total rows) for the `N of M` search count.
    /// Test-only seam (production reads `delegate_counts_for_search`).
    #[cfg(test)]
    fn visible_counts(&self) -> (usize, usize) {
        (self.visible.len(), self.rows.len())
    }

    /// Ticket 30: (visible, total, query) for the search-count row.
    fn delegate_counts_for_search(&self) -> (usize, usize, String) {
        (self.visible.len(), self.rows.len(), self.query.clone())
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
        // Ticket 19: complete row content (name, protocol, upstream
        // host:port, bind endpoint, enabled state, serving badge) via the
        // tested [`library_row_lines`] seam — one child per line, stable
        // domain id, no entity per row.
        // Ticket 30: the badge reads the serving set, never the enabled flag.
        let health = self
            .health
            .get(&row.id)
            .map(String::as_str)
            .filter(|summary| !summary.is_empty());
        let is_running = self.running.contains(&row.id);
        let mut item = ListItem::new(format!("library-row-{}", row.id)).selected(selected);
        for line in library_row_lines(&row, health, is_running) {
            item = item.child(line);
        }
        Some(item)
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        // Ticket 30: a query with no hits is not an empty library. No-hits
        // shows `No matches — Clear search` + `N of M` (the user clears via
        // the search box); a truly empty library shows the Add/Import CTAs.
        if !self.query.trim().is_empty() && !self.rows.is_empty() {
            let (visible, total) = (self.visible.len(), self.rows.len());
            return div()
                .id("library-empty")
                .test_support()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .child(no_matches_text())
                .child(search_count_text(visible, total))
                .child(
                    div()
                        .id("clear-search-hint")
                        .test_support()
                        .child("Clear search"),
                );
        }
        div()
            .id("library-empty")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .child(empty_library_text())
            .child(
                div()
                    .id("empty-library-ctas")
                    .test_support()
                    .child("Add or Import"),
            )
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

/// Ticket 18: shared commit signature for plain-dialog footers — the same
/// closure runs on the Confirm key path (`button_props.on_ok`) and the
/// footer OK click. Returns true when the dialog should close (false keeps
/// it open, e.g. total import failure).
type DialogCommit = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

/// Ticket 18: explicit mouse commit path for plain dialogs. A plain `Dialog`
/// renders footer buttons ONLY via `.footer(...)` (`button_props.on_ok` fires
/// on the Confirm key action alone), so every `open_dialog` dialog gets one
/// of these footers. `commit` is the same `Rc`-shared closure the key path
/// runs through `button_props.on_ok`: OK clicks commit identically (including
/// stay-open-on-error — no close when it returns false) and Cancel closes
/// with no side effects. The click never dispatches Confirm/Cancel, so one
/// user action can never fire both paths (no double-commit). Button ids are
/// stable per dialog; observability comes from the base button itself
/// (`Button::new(id)` wraps a `div().id(id).test_support()`).
/// Ticket 26 (L7 ruling): no idempotency token is added on top. The framework
/// routes one user gesture to exactly one path (a click never dispatches the
/// Confirm key action and vice versa — verified against the `Dialog` render
/// in ticket 18), and both paths share the single `Rc` closure, so there is
/// no second execution to dedupe. Dialogs with real re-entrancy risk (import
/// parse, refresh fetch) carry their own in-flight guards instead.
fn dialog_footer(
    ok_id: &'static str,
    cancel_id: &'static str,
    commit: DialogCommit,
) -> DialogFooter {
    let commit_click = commit.clone();
    DialogFooter::new()
        .child(
            Button::new(cancel_id)
                .label("Cancel")
                .on_click(|_, window, cx| window.close_all_dialogs(cx)),
        )
        .child(
            Button::new(ok_id)
                .label("OK")
                .on_click(move |_, window, cx| {
                    if commit_click(window, cx) {
                        window.close_all_dialogs(cx);
                    }
                }),
        )
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
    /// Ticket 28: one input builder behind `blank` and `prefilled` (same
    /// placeholder + default wiring per field; `None` skips `default_value`
    /// exactly like the old blank fields did).
    fn field(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: &str,
        default: Option<&str>,
    ) -> Entity<InputState> {
        cx.new(|cx| {
            let state = InputState::new(window, cx).placeholder(placeholder);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    fn blank(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        // Ticket 31: the params box opens with the per-protocol field
        // template (guided add, not raw JSON first). Submitting it untouched
        // fails naming the missing field — never silently.
        let template = protocol_params_template(DEFAULT_PROTOCOL);
        Self {
            name: Self::field(window, cx, "Name", None),
            protocol: Self::field(window, cx, "Protocol", Some(DEFAULT_PROTOCOL)),
            params: Self::field(window, cx, "Protocol params (optional)", Some(&template)),
            group: Self::field(window, cx, "Group", Some(DEFAULT_GROUP)),
            bind: Self::field(window, cx, "Bind address", Some(DEFAULT_BIND)),
            port: Self::field(window, cx, "Port (blank = auto)", None),
        }
    }

    fn prefilled(window: &mut Window, cx: &mut Context<AppView>, config: &ProxyConfig) -> Self {
        Self {
            name: Self::field(window, cx, "Name", Some(config.name.as_str())),
            protocol: Self::field(window, cx, "Protocol", Some(config.protocol.as_str())),
            params: Self::field(
                window,
                cx,
                "Protocol params (optional)",
                Some(config.params.as_str()),
            ),
            group: Self::field(window, cx, "Group", Some(config.group.as_str())),
            bind: Self::field(window, cx, "Bind address", Some(config.bind_ip.as_str())),
            port: Self::field(
                window,
                cx,
                "Port (blank = auto)",
                Some(&config.port.to_string()),
            ),
        }
    }

    /// Consumes a clone; every built element owns its state entity.
    /// Ticket 17: appends the inline LAN-confirm hint when the current bind
    /// is non-loopback (rebuilt on every render, so it tracks typing).
    /// Ticket 31: `collision` is the precomputed bind/port warning for the
    /// live field values (computed per render by the caller, which owns the
    /// rows snapshot); `form_error` is the shared first-error slot shown
    /// inline under the fields. All owned strings — the built element keeps
    /// no borrows.
    fn element(
        self,
        cx: &App,
        collision: Option<String>,
        form_error: Option<(String, String)>,
    ) -> impl IntoElement {
        let bind = self.bind.read(cx).value().to_string();
        let mut col = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dialog_field("field-name", "Name", self.name))
            .child(dialog_field("field-protocol", "Protocol", self.protocol))
            .child(dialog_field("field-params", "Params", self.params))
            .child(dialog_field("field-group", "Group", self.group))
            .child(dialog_field("field-bind", "Bind address", self.bind))
            .child(dialog_field("field-port", "Port", self.port));
        if let Some(hint) = lan_hint_for_bind(&bind) {
            col = col.child(div().id("lan-hint").test_support().child(hint));
        }
        if let Some(warning) = collision {
            col = col.child(
                div()
                    .id("bind-collision-warning")
                    .test_support()
                    .child(warning),
            );
        }
        if let Some((field, message)) = form_error {
            col = col.child(
                div()
                    .id("form-field-error")
                    .test_support()
                    .child(format!("{field}: {message}")),
            );
        }
        col
    }

    /// Ticket 31: move keyboard focus to the named field's input
    /// (`name`/`protocol`/`params`/`group`/`bind_ip`/`port`). Unknown names
    /// focus nothing (never a wrong field).
    fn focus_field(&self, field: &str, window: &mut Window, cx: &mut App) {
        let target = match field {
            "name" => Some(&self.name),
            "protocol" => Some(&self.protocol),
            "params" => Some(&self.params),
            "group" => Some(&self.group),
            "bind_ip" => Some(&self.bind),
            "port" => Some(&self.port),
            _ => None,
        };
        if let Some(state) = target {
            let handle = AnyInputState::from(state.clone()).focus_handle(cx);
            window.focus(&handle, cx);
        }
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

    /// Ticket 26: patch relative to the dialog-open snapshot. Fields the user
    /// did not touch stay `None` (unchanged), so a concurrent refresh that
    /// landed after the dialog opened survives the commit on every untouched
    /// field. Only genuinely edited fields are written. Blank-vs-snapshot
    /// keeps the store's blank-equals-keep semantics (`Some("")` on genuinely
    /// edited fields behaves exactly like the old full patch).
    /// Ticket 28: reads the six fields once via [`ConfigForm::as_new`] (one
    /// read path for add and edit); only the baseline compare is edit-only.
    fn as_patch_vs(&self, cx: &App, baseline: &ProxyConfig) -> ConfigPatch {
        fn changed(current: String, baseline: &str) -> Option<String> {
            if current == baseline {
                None
            } else {
                Some(current)
            }
        }
        let current = self.as_new(cx);
        ConfigPatch {
            name: changed(current.name, &baseline.name),
            protocol: changed(current.protocol, &baseline.protocol),
            params: changed(current.params, &baseline.params),
            group: changed(current.group, &baseline.group),
            bind_ip: changed(current.bind_ip, &baseline.bind_ip),
            port: changed(current.port, &baseline.port.to_string()),
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
    /// Ticket 28: one constructor (the old `new`/`new_multi` pair shared the
    /// whole body; `multi` is `Some` only for an unedited multi-file drop).
    fn new(
        window: &mut Window,
        cx: &mut Context<AppView>,
        prefill_text: &str,
        prefill_group: &str,
        file_hint: Option<String>,
        multi: Option<Vec<(String, String)>>,
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
            multi_files: multi,
        }
    }

    /// Consumes a clone; every built element owns its state entity.
    /// Ticket 17: shows the live `X valid · Y invalid` preview (rebuilt on
    /// every render from the current box contents, so it tracks typing and
    /// pasting) plus the dropped file names when present. The preview is
    /// inline text, never a toast.
    fn element(self, cx: &App) -> impl IntoElement {
        let text = self.text.read(cx).value().to_string();
        let (valid, invalid) = import_preview_counts(&text, self.file_hint.as_deref());
        let mut col = div()
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
            .child(
                div()
                    .id("import-preview")
                    .test_support()
                    .child(import_preview_message(valid, invalid)),
            );
        if let Some(files) =
            import_files_label(self.file_hint.as_deref(), self.multi_files.as_deref())
        {
            col = col.child(div().id("import-files").test_support().child(files));
        }
        col
    }
}

/// Ticket 04: redacted subscription URL for display (query and fragment are
/// dropped so tokens never reach the screen or logs).
/// Ticket 14: userinfo is stripped too — same rule as
/// [`subscription_label_from_url`]: a pasted URL can embed `user:password`,
/// which must never appear in a row, notification, or log.
/// Ticket 28: kept separate from the import `redact_link` and health
/// `redact_secrets` on purpose — this drops query/fragment for http(s)
/// endpoint URLs, `redact_link` masks opaque link payloads but keeps
/// fragments (remarks are display names), and `redact_secrets` scrubs
/// token forms inside free text; one scrubber would leak on some path.
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
    // Ticket 28: length cap is health::truncate (same 96-char rule as the
    // link scrubber; one truncator for every display path).
    truncate(&stripped, 96)
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
/// Ticket 28: the count fragment is shared with
/// [`export_preview_description`] (one branch, two sentence frames).
fn export_count_fragment(total: usize, importable: usize) -> String {
    if importable == total {
        format!("{total} config(s)")
    } else {
        format!("{total} config(s) ({importable} importable)")
    }
}

fn export_count_message(total: usize, importable: usize, path: &str) -> String {
    format!(
        "Exported {} to {path}",
        export_count_fragment(total, importable)
    )
}

/// Ticket 17: success toast copy naming the affected row (add/edit/remove
/// each name their row; no toast fires on keystrokes or dialog opens).
/// Ticket 28: one verb slot instead of three parallel one-line wrappers.
fn row_action_message(verb: &str, name: &str) -> String {
    format!("{verb} \"{name}\"")
}

/// Ticket 17: live import preview counts, reusing the parser read-only (no
/// store writes, no toasts — the dialog renders these as inline text).
fn import_preview_counts(text: &str, file_hint: Option<&str>) -> (usize, usize) {
    let batch = match effective_hint_for_text(file_hint, text) {
        Some(name) => import_file_content(&name, text),
        None => import_text(text),
    };
    (batch.configs.len(), batch.failures.len())
}

/// Ticket 17: the inline preview line (`X valid · Y invalid`).
fn import_preview_message(valid: usize, invalid: usize) -> String {
    format!("{valid} valid · {invalid} invalid")
}

/// Ticket 26: parse-ready import inputs, shared by the synchronous commit
/// (tests, headless) and the background dialog commit. `original` is the
/// dialog prefill: edited text drops the stale drop hint and multi-file
/// bytes and sniffs as its own type.
struct ResolvedImport {
    text: String,
    group: String,
    file_hint: Option<String>,
    multi: Option<Vec<(String, String)>>,
}

fn resolve_import_inputs(
    text: String,
    group: String,
    file_hint: Option<String>,
    original: Option<String>,
    multi: Option<Vec<(String, String)>>,
) -> ResolvedImport {
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
    ResolvedImport {
        text,
        group,
        file_hint,
        multi,
    }
}

/// Ticket 26: pure CPU parse of resolved inputs (runs on the background
/// executor on the dialog path). Unedited multi-file drops parse per file
/// (stems/keys survive); everything else sniffs single-text.
fn parse_import_inputs(resolved: &ResolvedImport) -> crate::import::BatchResult {
    // Unedited multi-file drop: per-file parse keeps stems/keys.
    if let Some(files) = &resolved.multi {
        if resolved.file_hint.is_none() {
            let refs: Vec<(&str, &str)> = files
                .iter()
                .map(|(name, content)| (name.as_str(), content.as_str()))
                .collect();
            return import_multi_file_contents(&refs);
        }
    }
    match &resolved.file_hint {
        Some(name) => import_file_content(name, &resolved.text),
        None => import_text(&resolved.text),
    }
}

/// Ticket 17: basename of a dropped path (dialogs show file names, never
/// full paths — paths can carry OS usernames).
fn file_basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Ticket 17: file-names line for the import dialog after a drop (`None`
/// when nothing was dropped).
/// Ticket 28: one basename-join for the single- and multi-file arms (the
/// `File:`/`Files:` prefixes stay distinct; an empty multi-drop still shows
/// nothing even when a stale hint lingers).
fn import_files_label(
    file_hint: Option<&str>,
    multi_files: Option<&[(String, String)]>,
) -> Option<String> {
    let (prefix, names): (&str, Vec<String>) = match multi_files {
        Some(files) => (
            "Files",
            files.iter().map(|(name, _)| file_basename(name)).collect(),
        ),
        None => ("File", file_hint.map(file_basename).into_iter().collect()),
    };
    if names.is_empty() {
        None
    } else {
        Some(format!("{prefix}: {}", names.join(", ")))
    }
}

/// Ticket 17: export dialog preview — row counts plus the destination path,
/// with an overwrite warning when the file already exists. The plaintext
/// warning itself stays verbatim ([`export_confirm_description`]); this line
/// is appended after it.
fn export_preview_description(total: usize, importable: usize, path: &str, exists: bool) -> String {
    let mut out = format!("{} → {path}", export_count_fragment(total, importable));
    if exists {
        out.push_str(&format!(" — {path} already exists and will be overwritten"));
    }
    out
}

/// Ticket 17: per-subscription item failures line (renders the stored
/// failures vec, not just a count). Empty when there is nothing to show.
fn subscription_failures_text(failures: &[crate::import::ImportFailure]) -> String {
    if failures.is_empty() {
        String::new()
    } else {
        truncate(
            &format!(
                "item failures ({}): {}",
                failures.len(),
                format_import_failures(failures)
            ),
            400,
        )
    }
}

/// Ticket 17: one coalesced background-tick summary for N refreshes.
/// `None` when there is nothing to report (never an empty toast).
/// The bool is the warning flag (any failed member — or any preserved user
/// edit — warns).
fn coalesced_refresh_message(
    subs: usize,
    added: usize,
    updated: usize,
    removed: usize,
    failed: usize,
    conflicts: usize,
) -> Option<(bool, String)> {
    if subs == 0 {
        return None;
    }
    let text =
        format!("Background refresh: {subs} subscription(s): +{added} ~{updated} -{removed}");
    if failed == 0 && conflicts == 0 {
        Some((false, text))
    } else {
        let mut full = format!("{text} · {failed} failed");
        if conflicts > 0 {
            full.push_str(&format!(" · {conflicts} conflict(s) — your edits kept"));
        }
        Some((true, full))
    }
}

/// Ticket 17: honest start-all copy — never "Started 0".
fn start_all_message(count: usize) -> String {
    if count == 0 {
        "Nothing to start — the library is empty".to_string()
    } else {
        format!("Started {count} endpoint(s)")
    }
}

/// Ticket 17: honest stop-all copy — never a success claim over silence.
fn stop_all_message(running: usize) -> String {
    if running == 0 {
        "Nothing to stop — no endpoints are running".to_string()
    } else {
        "Stopped all endpoints".to_string()
    }
}

/// Ticket 17: honest stop-on-stopped copy naming the row.
fn already_stopped_message(name: &str) -> String {
    format!("\"{name}\" is already stopped")
}

/// Ticket 30: serving badge for one row — running rows show
/// `● Running :port` plus ` · health` when a summary exists; stopped rows
/// show `○ Stopped`. The enabled flag never appears here (it stays secondary
/// in the row's third line). Port + health are display-safe (no secrets).
fn serving_badge(is_running: bool, port: u16, health: Option<&str>) -> String {
    if !is_running {
        return "○ Stopped".to_string();
    }
    match health.map(str::trim).filter(|summary| !summary.is_empty()) {
        Some(summary) => format!("● Running :{port} · {summary}"),
        None => format!("● Running :{port}"),
    }
}

/// Ticket 30: honest start-on-running copy — a restart, never a bare
/// `Started`.
fn already_running_message(name: &str) -> String {
    format!("\"{name}\" already running (restarted)")
}

/// Ticket 30: no-selection copy for Start/Stop/Test/Ping with nothing
/// selected (F5 path included).
fn no_selection_message() -> String {
    "Select a configuration first".to_string()
}

/// Ticket 30: busy-guard copy — a second Start while one is in flight is
/// ignored, never double-applied.
fn start_busy_message() -> String {
    "Starting… already in progress".to_string()
}

/// Ticket 30: partial start-all copy — names the failed rows plus both
/// counts so the toast never claims a clean sweep over failures.
fn start_all_partial_message(started: usize, failed_names: &[String]) -> String {
    let failed = failed_names.len();
    if failed == 0 {
        return start_all_message(started);
    }
    format!(
        "Started {started} endpoint(s), {failed} failed: {}",
        failed_names.join(", ")
    )
}

/// Ticket 30: search count `N of M` (visible of total).
fn search_count_text(visible: usize, total: usize) -> String {
    format!("{visible} of {total}")
}

/// Ticket 30: no-hits copy for a query that matches nothing (total > 0).
fn no_matches_text() -> String {
    "No matches — Clear search".to_string()
}

/// Ticket 30: empty-library copy with Add/Import affordances.
fn empty_library_text() -> String {
    "No proxy configurations yet — Add or Import to start".to_string()
}

/// Ticket 30: sidecar-crash notification copy — names the joined per-shard
/// detail (`shard: cause[; …]`) and points at the log viewer. `detail` comes
/// from [`engine::EngineController::crash_detail`] (already redacted).
fn crash_notification_text(detail: &str) -> String {
    format!("Engine crashed ({detail}) — View logs")
}

/// Ticket 30: true for start-all failures that isolate to one broken row
/// (bad params, unsupported protocol, wrong-layer/custom/chain schema
/// failures — each names its row). Union-level failures (port conflicts,
/// missing binaries, check/spawn failures, bridge skew, LAN gate) stay
/// atomic: nothing starts, exactly like before.
fn is_row_local_start_error(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::InvalidParamsJson { .. }
            | EngineError::BadParams { .. }
            | EngineError::UnsupportedProtocol { .. }
            | EngineError::BadAwg { .. }
            | EngineError::WrongLayer { .. }
            | EngineError::BadCustom { .. }
            | EngineError::ChainViolation { .. }
    )
}

/// Ticket 26: the enabled flag is NOT the engine. The switch and the Enable
/// button write the stored flag only — they never start or stop an endpoint
/// (see [`AppView::commit_enabled_flag`]). This copy says so on every
/// successful toggle so the two states can never be confused.
fn enabled_flag_message(name: &str, enabled: bool) -> String {
    if enabled {
        format!("\"{name}\" enabled (flag only — Start to serve)")
    } else {
        format!("\"{name}\" disabled (flag only — Stop to close the endpoint)")
    }
}

/// Ticket 26: store-list failure copy. Names the database error instead of
/// rendering (or toasting about) an empty library that is not really empty.
fn db_list_error_message(error: &StoreError) -> String {
    format!("Cannot load library: {error}")
}

/// Ticket 26: one refresh completion rendered as text. Returns the warning
/// flag (any failure OR any preserved user edit warns) plus the line.
/// Conflicts are user edits the merge refused to revert — they are named,
/// never folded into silence.
#[allow(clippy::too_many_arguments)]
fn refresh_summary_text(
    label: &str,
    added: usize,
    updated: usize,
    removed: usize,
    skipped: usize,
    duplicates: usize,
    failures: usize,
    conflicts: usize,
) -> (bool, String) {
    let mut summary = format!("{label}: +{added} ~{updated} -{removed}");
    if skipped > 0 {
        summary.push_str(&format!(" · {skipped} skipped"));
    }
    if duplicates > 0 {
        summary.push_str(&format!(" · {duplicates} duplicates"));
    }
    if failures > 0 {
        summary.push_str(&format!(" · {failures} failed"));
    }
    if conflicts > 0 {
        summary.push_str(&format!(" · {conflicts} conflict(s) — your edits kept"));
    }
    (failures > 0 || conflicts > 0, summary)
}

/// Ticket 26 (ticket-25 consumer): restore completion copy. Counts come from
/// [`RestoreOutcome`]; failures are loud errors at the call site, never
/// folded into this line.
fn restore_done_message(outcome: &RestoreOutcome) -> String {
    format!(
        "Restored {} config(s), {} subscription(s)",
        outcome.configs, outcome.subscriptions
    )
}

/// Ticket 26: dropped-but-unreadable files are named (basenames only — full
/// paths can carry OS usernames), never swallowed.
fn unreadable_drop_message(names: &[String]) -> String {
    format!("Skipped unreadable file(s): {}", names.join(", "))
}

/// Ticket 19: upstream `host:port` for a library row, read from the params
/// JSON only (`server`/`address` + `server_port`/`port` — the same keys the
/// store validation and import grammar use). Credential keys are never read,
/// so the display string cannot leak secrets by construction. `None` for
/// metadata-only rows with no upstream host.
fn upstream_host_port(row: &ProxyConfig) -> Option<String> {
    let map = serde_json::from_str::<serde_json::Value>(&row.params)
        .ok()?
        .as_object()?
        .clone();
    let host = ["server", "address"]
        .iter()
        .filter_map(|key| map.get(*key))
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .find(|host| !host.is_empty())?;
    let port = ["server_port", "port"]
        .iter()
        .filter_map(|key| map.get(*key))
        .find_map(|value| {
            if let Some(port) = value.as_u64() {
                (port > 0 && port <= 65535).then(|| port.to_string())
            } else {
                value.as_str().and_then(|text| {
                    text.trim()
                        .parse::<u64>()
                        .ok()
                        .filter(|port| *port > 0 && *port <= 65535)
                        .map(|port| port.to_string())
                })
            }
        });
    Some(match port {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

/// Ticket 19: every text line of a library row — name, protocol + upstream
/// host:port, group + bind endpoint + enabled state, plus the serving badge.
/// Ticket 30: the badge (`● Running :port · health` / `○ Stopped`) is always
/// present from the serving truth; the enabled flag stays secondary in the
/// third line. Health folds into the badge when running (no separate health
/// child). Pure so tests pin the complete content;
/// [`LibraryDelegate::render_item`] maps each line to a child.
fn library_row_lines(row: &ProxyConfig, health: Option<&str>, is_running: bool) -> Vec<String> {
    let upstream = upstream_host_port(row).unwrap_or_else(|| "no upstream".to_string());
    let state = if row.enabled { "enabled" } else { "disabled" };
    vec![
        row.name.clone(),
        format!("{} · {upstream}", row.protocol),
        format!(
            "{} · bind {}:{} · {state}",
            row.group, row.bind_ip, row.port
        ),
        serving_badge(is_running, row.port, health),
    ]
}

/// Ticket 17: a library row matches the search query by name, protocol,
/// group, bind address, port, or upstream host (the `server`/`address` inside
/// params — never credential fields).
fn library_row_matches(row: &ProxyConfig, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if row.name.to_lowercase().contains(&q)
        || row.protocol.to_lowercase().contains(&q)
        || row.group.to_lowercase().contains(&q)
        || row.bind_ip.to_lowercase().contains(&q)
        || row.port.to_string().contains(&q)
    {
        return true;
    }
    if let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&row.params)
    {
        for key in ["server", "address"] {
            if let Some(serde_json::Value::String(host)) = map.get(key) {
                if host.to_lowercase().contains(&q) {
                    return true;
                }
            }
        }
    }
    false
}

/// Ticket 17: inline LAN-confirm hint for add/edit dialogs. `Some` when
/// starting this bind will stop at the LAN confirm gate; `None` for
/// loopback and for blank (blank means the loopback default).
fn lan_hint_for_bind(bind: &str) -> Option<String> {
    if bind.trim().is_empty() || !requires_lan_confirm(bind) {
        return None;
    }
    Some(format!(
        "Starting on {} needs a LAN confirm — anyone on the network could use it",
        bind.trim()
    ))
}

/// Ticket 31: owning dialog field for a save error — the first invalid
/// field's name plus its message, so the dialog can show it inline and move
/// focus to it. `None` for non-field errors (`NotFound`/database). Carries
/// field names only, never values (no secrets by construction).
fn field_error_target(error: &StoreError) -> Option<(&'static str, String)> {
    match error {
        StoreError::Invalid(errors) => errors
            .first()
            .map(|first| (first.field, first.message.clone())),
        _ => None,
    }
}

/// Ticket 31: non-blocking bind/port collision warning naming the colliding
/// row. `None` when the pair is free, when it is the edited row itself
/// (`exclude_id`), when the port is blank (auto-assign never collides), or
/// when the port is unparseable (validation owns that error — the warning
/// must never double-report it). Saving is always still allowed: at rest two
/// rows may share an endpoint; the conflict error naming both parties stays
/// an activation-time concern.
fn bind_port_collision_warning(
    rows: &[ProxyConfig],
    exclude_id: Option<&str>,
    bind_raw: &str,
    port_raw: &str,
) -> Option<String> {
    let port: u16 = port_raw.trim().parse().ok()?;
    if port == 0 {
        return None;
    }
    let bind = bind_raw.trim();
    let bind = if bind.is_empty() { DEFAULT_BIND } else { bind };
    rows.iter()
        .filter(|row| Some(row.id.as_str()) != exclude_id)
        .find(|row| row.bind_ip == bind && row.port == port)
        .map(|row| {
            format!(
                "bind {bind}:{port} is already used by \"{}\" — saving is still allowed",
                row.name
            )
        })
}

/// Ticket 31: visible plaintext-fetch warning for subscription URLs (the
/// Warn policy surfaced, never the URL). `None` for https and for anything
/// the validators reject (those are errors, not warnings).
fn http_subscription_warning(url: &str) -> Option<String> {
    match store::subscription_fetch_policy(url) {
        store::SubscriptionFetchPolicy::Warn { reason } => Some(reason.to_string()),
        _ => None,
    }
}

/// Ticket 31: friendly duplicate-subscription copy — names the existing
/// label (never the URL: tokens hide in queries).
fn already_subscribed_message(label: &str) -> String {
    format!("Already subscribed as \"{label}\" — no duplicate added")
}

/// Ticket 04: fetch a subscription body over the platform HTTP client.
/// Errors are generic on purpose (never echo the URL: tokens hide in
/// queries). Bodies are capped at 8 MiB and must be UTF-8; anything else is
/// a refresh failure that keeps the last-good members.
/// Ticket 27: the fetch enforces [`store::subscription_fetch_policy`] first —
/// internal hosts Block without touching the network (SSRF guard); plaintext
/// http Warns (proceeds: LAN-provider compat, tokens leak in plaintext).
async fn fetch_subscription_body(client: Arc<dyn HttpClient>, url: &str) -> Result<String, String> {
    use futures::AsyncReadExt as _;
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    const TIMEOUT_SECS: u64 = 20;
    match store::subscription_fetch_policy(url) {
        store::SubscriptionFetchPolicy::Block { reason } => return Err(reason.to_string()),
        store::SubscriptionFetchPolicy::Warn { .. } => {}
        store::SubscriptionFetchPolicy::Allow => {}
    }
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
/// Ticket 17: `auto_refresh` is the add-form checkbox state (default on —
/// no more create-then-toggle). A shared cell (not an entity): the dialog
/// builder reads it on every render and the toggle writes it on click; the
/// OK handler commits whatever it holds.
#[derive(Clone)]
struct SubForm {
    label: Entity<InputState>,
    url: Entity<InputState>,
    group: Entity<InputState>,
    auto_refresh: Rc<RefCell<bool>>,
}

impl SubForm {
    /// Ticket 26: the only add-form constructor. Fresh opens pass empty
    /// strings; refresh-driven reopens pass the stashed in-progress text so
    /// a background completion never wipes what the user already typed.
    fn restored(
        window: &mut Window,
        cx: &mut Context<AppView>,
        label: &str,
        url: &str,
        group: &str,
        auto_refresh: bool,
    ) -> Self {
        Self {
            label: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Label")
                    .default_value(label.to_string())
            }),
            url: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://…")
                    .default_value(url.to_string())
            }),
            group: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Group")
                    .default_value(group.to_string())
            }),
            auto_refresh: Rc::new(RefCell::new(auto_refresh)),
        }
    }

    /// Consumes a clone; every built element owns its state entity.
    /// Ticket 31: appends the inline plaintext-fetch warning when the live
    /// URL input is http (rebuilt on every render, so it tracks typing —
    /// same pattern as the LAN hint on [`ConfigForm::element`]).
    fn element(self, cx: &App) -> impl IntoElement {
        let auto = self.auto_refresh.clone();
        let checked = *auto.borrow();
        let url = self.url.read(cx).value().to_string();
        let mut col = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dialog_field("field-sub-label", "Label", self.label))
            .child(dialog_field("field-sub-url", "Subscription URL", self.url))
            .child(dialog_field("field-sub-group", "Group", self.group))
            .child(
                div().id("field-sub-auto").test_support().child(
                    Switch::new("sub-auto-add")
                        .checked(checked)
                        .label("Auto-refresh")
                        .on_change(move |next: &bool, _, _| {
                            *auto.borrow_mut() = *next;
                        }),
                ),
            );
        if let Some(warning) = http_subscription_warning(&url) {
            col = col.child(
                div()
                    .id("sub-add-http-warning")
                    .test_support()
                    .child(warning),
            );
        }
        col
    }
}

/// Ticket 04: one subscription row for the manager dialog (label, redacted
/// URL, group, member count, auto flag, last error) with Refresh / Auto /
/// Remove actions. Refresh re-reads the record so a stale snapshot cannot
/// fetch a removed subscription.
/// Ticket 31: plus an Edit action opening the label/url/group editor.
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
    // Ticket 17: item failures viewable per subscription (the stored vec,
    // not just the aggregated count).
    let failures = subscription_failures_text(&sub.failures);
    if !failures.is_empty() {
        row = row.child(
            div()
                .id(format!("sub-failures-{}", sub.id))
                .test_support()
                .child(failures),
        );
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
                    // Ticket 26: only the id travels — the current flag is
                    // re-read at commit (the rendered label may be stale).
                    view.commit_subscription_auto(&sub_id, window, cx);
                });
            }),
    );
    let remove_view = view.clone();
    let edit_view = view.clone();
    let edit_sub = sub.clone();
    row = row.child(
        Button::new(format!("sub-edit-{}", sub.id))
            .label("Edit")
            .on_click(move |_, window, cx| {
                let sub = edit_sub.clone();
                edit_view.update(cx, |view, cx| {
                    view.open_subscription_edit_dialog(sub, window, cx);
                });
            }),
    );
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
/// Ticket 26: the sleep runs in 1s slices with a stop/release check per
/// slice, so stopping ticks (or closing the window) cancels promptly instead
/// of lingering up to a full 300s due-subscription interval.
fn spawn_tick(
    cx: &mut Context<AppView>,
    secs: u64,
    tick: impl Fn(&mut AppView, &mut Context<AppView>) + 'static,
) {
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_entity, cx| {
        let mut elapsed = 0u64;
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let live = cx.update(|cx| {
                weak.update(cx, |view, _| view.ticks_enabled)
                    .unwrap_or(false)
            });
            if !live {
                break;
            }
            elapsed += 1;
            if elapsed >= secs.max(1) {
                elapsed = 0;
                let ran = cx.update(|cx| {
                    weak.update(cx, |view, cx| {
                        tick(view, cx);
                        true
                    })
                    .unwrap_or(false)
                });
                if !ran {
                    break;
                }
            }
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
    /// Ticket 26: `Send` so blocking probes can move to the background
    /// executor ([`AppView::spawn_probe`]); the fake stays API-identical.
    prober: Box<dyn HealthProber + Send>,
    ticks_enabled: bool,
    /// Ticket 17: background-refresh toast coalescing. `refresh_batch_members`
    /// holds the in-flight background member ids; completions for members
    /// accumulate counts instead of notifying, and the last member flushes at
    /// most one toast. Manual refreshes notify immediately as before.
    /// Ticket 26: `refresh_generation` is the batch epoch (bumped only when a
    /// batch opens from empty — a tick never resets an open batch, it unions
    /// into it); `inflight_refreshes` dedupes manual-vs-tick double fetches
    /// by sub id; `refresh_batch_conflicts` accumulates preserved user edits
    /// for the coalesced copy.
    refresh_batch_total: usize,
    refresh_batch_members: HashSet<String>,
    refresh_batch_added: usize,
    refresh_batch_updated: usize,
    refresh_batch_removed: usize,
    refresh_batch_failed: usize,
    refresh_batch_conflicts: usize,
    refresh_generation: u64,
    inflight_refreshes: HashSet<String>,
    /// Ticket 26: a health probe (URL test or ping) is running off-thread.
    /// A second probe request toasts instead of stacking (the placeholder
    /// prober swap in [`AppView::spawn_probe`] is never observable).
    probe_in_flight: bool,
    /// Ticket 26: the live subscriptions-dialog add form, stashed on open so
    /// a genuine reopen (background refresh completion) can carry
    /// in-progress text across instead of rebuilding a blank form. `None`
    /// means no subs dialog was opened (foreign dialogs are never touched).
    subs_form: Option<SubForm>,
    /// Ticket 26: an import-dialog parse is running on the background
    /// executor. A second OK while it runs is ignored (the first completion
    /// closes or keeps the dialog exactly once — no double commit).
    import_parse_in_flight: bool,
    /// Ticket 33: set once the user confirms the close-with-running-endpoints
    /// dialog, so the confirmed second close is never vetoed again.
    close_confirmed: bool,
    /// Ticket 30: an engine start (single or start-all) is in flight. A
    /// second Start while set is ignored with an info toast (no second
    /// apply); the detail Start button reads `Starting…` while set.
    start_in_flight: bool,
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
            refresh_batch_total: 0,
            refresh_batch_members: HashSet::new(),
            refresh_batch_added: 0,
            refresh_batch_updated: 0,
            refresh_batch_removed: 0,
            refresh_batch_failed: 0,
            refresh_batch_conflicts: 0,
            refresh_generation: 0,
            inflight_refreshes: HashSet::new(),
            probe_in_flight: false,
            subs_form: None,
            import_parse_in_flight: false,
            close_confirmed: false,
            start_in_flight: false,
            _libraries_subscription: cx.subscribe(
                &libraries,
                |_view, _state, _event: &ListEvent, cx| {
                    cx.notify();
                },
            ),
        };
        let mut view = view;
        let rows = match view.store.list() {
            Ok(rows) => rows,
            // Ticket 14: a broken database must not render as a silent empty
            // library.
            Err(error) => {
                window.push_notification(Notification::error(db_list_error_message(&error)), cx);
                Vec::new()
            }
        };
        // Ticket 12: restore last latencies + dead flags across restart.
        // Only snapshots of still-known rows hydrate (removed configs never
        // resurrect); traffic stays session-only; logs are never persisted.
        // Ticket 26: the known set reuses the rows listed above (a second
        // list that fails would silently hydrate nothing on a broken DB).
        if let Ok(snapshots) = view.store.load_health_snapshots() {
            let known: HashSet<String> = rows.iter().map(|row| row.id.clone()).collect();
            let kept: Vec<HealthSnapshot> = snapshots
                .into_iter()
                .filter(|snapshot| known.contains(&snapshot.config_id))
                .collect();
            view.health_store.hydrate(&kept);
        }
        libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_rows(rows);
            // Ticket 19: the rows cache refreshes inside `ListState::render`
            // (the `List` element embeds the state entity as a child view),
            // which only runs when the list entity itself is notified — the
            // parent notify below never reaches it.
            list_cx.notify();
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
    /// Ticket 28: the three reload variants stay — the notify/ignore split
    /// is load-bearing (ticket-14 ruling: headless paths stay silent by
    /// necessity, every user action notifies).
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Ticket 30: a pending sidecar crash notifies here (windowed) with
        // shard/cause + View-logs — never status-bar-only. `reload_inner`
        // would otherwise consume the one-shot poll for the status bar alone.
        self.notify_engine_crash(window, cx);
        if let Err(error) = self.reload_inner(cx) {
            window.push_notification(Notification::error(db_list_error_message(&error)), cx);
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
        let running = self.engine.running_ids();
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_rows(rows);
            state.delegate_mut().set_health(health);
            // Ticket 30: serving truth follows the engine on every reload.
            state.delegate_mut().set_running(running);
            // Ticket 19: notify the list entity itself (see `with_store`).
            list_cx.notify();
        });
        Ok(())
    }

    fn report_save_error(&self, error: StoreError, window: &mut Window, cx: &mut App) {
        window.push_notification(Notification::error(format!("Cannot save: {error}")), cx);
    }

    /// Ticket 26: list the library or notify once with the database error.
    /// `None` means the rows are unknown — callers must return early instead
    /// of acting on a lying empty list (never "the library is empty" over a
    /// corrupt database).
    fn list_or_notify(&self, window: &mut Window, cx: &mut App) -> Option<Vec<ProxyConfig>> {
        match self.store.list() {
            Ok(rows) => Some(rows),
            Err(error) => {
                window.push_notification(Notification::error(db_list_error_message(&error)), cx);
                None
            }
        }
    }

    /// Ticket 26: running rows for a probe, or notify once on a database
    /// error (never "no running endpoints" over a corrupt database).
    /// Ticket 28: filters over [`list_or_notify`] (one list/notify seam).
    fn running_rows_or_notify(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Vec<ProxyConfig>> {
        let running = self.engine.running_ids();
        self.list_or_notify(window, cx).map(|rows| {
            rows.into_iter()
                .filter(|row| running.contains(&row.id))
                .collect()
        })
    }

    /// Ticket 17: success names the row (informational toast, never a
    /// blocking modal; nothing fires on keystrokes).
    /// Ticket 26 (L6 ruling): the search filter is user state and survives
    /// the add — a new row hidden by the active filter is confirmed by the
    /// toast plus the status count, and appears when the filter clears.
    /// Clearing the user's query behind their back would be worse.
    /// Ticket 31: dialogs commit through [`AppView::commit_add_with_focus`];
    /// this stays as the headless/test seam.
    #[cfg_attr(not(test), allow(dead_code))]
    fn commit_add(
        &mut self,
        input: NewConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.add(input) {
            Ok(created) => {
                let message = row_action_message("Added", &created.name);
                self.reload(window, cx);
                window.push_notification(Notification::success(message), cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    /// Ticket 31: dialog commit behind [`AppView::commit_add`]. Reads the
    /// input from the form, and on failure records the first field error in
    /// the shared inline slot (rendered under the fields on the next frame)
    /// and moves keyboard focus to the owning field. The dialog stays open
    /// (false); the toast still fires so headless callers see the error too.
    fn commit_add_with_focus(
        &mut self,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.add(form.as_new(cx)) {
            Ok(created) => {
                *form_error.borrow_mut() = None;
                let message = row_action_message("Added", &created.name);
                self.reload(window, cx);
                window.push_notification(Notification::success(message), cx);
                true
            }
            Err(error) => {
                if let Some((field, message)) = field_error_target(&error) {
                    *form_error.borrow_mut() = Some((field.to_string(), message));
                    form.focus_field(field, window, cx);
                }
                self.report_save_error(error, window, cx);
                cx.notify();
                false
            }
        }
    }

    /// Ticket 26: re-read the row at commit. A row deleted while the dialog
    /// was open warns "Already removed" and closes (nothing left to edit);
    /// a store failure keeps the dialog open with the error (retryable).
    /// Staleness inside the row is handled by the caller, which builds the
    /// patch relative to the dialog-open snapshot (only user-touched fields
    /// are `Some` — see [`ConfigForm::as_patch_vs`]).
    /// Ticket 31: dialogs commit through [`AppView::commit_edit_with_focus`];
    /// this stays as the headless/test seam.
    #[cfg_attr(not(test), allow(dead_code))]
    fn commit_edit(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.get(id) {
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
            Ok(Some(_)) => match self.store.update(id, patch) {
                Ok(updated) => {
                    let message = row_action_message("Updated", &updated.name);
                    self.reload(window, cx);
                    window.push_notification(Notification::success(message), cx);
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            },
        }
    }

    /// Ticket 31: dialog commit behind [`AppView::commit_edit`]. Same
    /// ghost/store-failure arms, plus the inline first-error slot and focus
    /// move on validation failure (the dialog stays open, retryable).
    fn commit_edit_with_focus(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.get(id) {
            Ok(None) => {
                *form_error.borrow_mut() = None;
                window.push_notification(Notification::warning("Already removed"), cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
            Ok(Some(_)) => match self.store.update(id, patch) {
                Ok(updated) => {
                    *form_error.borrow_mut() = None;
                    let message = row_action_message("Updated", &updated.name);
                    self.reload(window, cx);
                    window.push_notification(Notification::success(message), cx);
                    true
                }
                Err(error) => {
                    if let Some((field, message)) = field_error_target(&error) {
                        *form_error.borrow_mut() = Some((field.to_string(), message));
                        form.focus_field(field, window, cx);
                    }
                    self.report_save_error(error, window, cx);
                    cx.notify();
                    false
                }
            },
        }
    }

    fn commit_remove(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Ticket 17: the toast names the row (looked up before the delete).
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());
        match self.store.remove(id) {
            Ok(true) => {
                // Ticket 19: only drop the selection when the removed row was
                // selected — any other row keeps the selection, which
                // `set_rows` then preserves by id across the reload.
                if self.selected_config(cx).map(|row| row.id).as_deref() == Some(id) {
                    self.libraries.update(cx, |state, list_cx| {
                        state.delegate_mut().clear_selection();
                        // Ticket 19: notify the list entity itself (see
                        // `with_store`); the reload below refreshes rows too.
                        list_cx.notify();
                    });
                }
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(row_action_message("Removed", &name)),
                    cx,
                );
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

    /// Ticket 26: the single enabled-flag commit behind the detail switch and
    /// the Enable/Disable button. The flag is re-read fresh (never a stale
    /// snapshot write) and the engine is deliberately untouched — the toast
    /// says so ([`enabled_flag_message`]): enable ≠ start, disable ≠ stop.
    /// One semantic, documented at the only place it can be confused.
    fn commit_enabled_flag(
        &mut self,
        id: &str,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());
        // Ticket 14: a failed write notifies (never a dead switch).
        match self.store.set_enabled(id, enabled) {
            Ok(_) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::info(enabled_flag_message(&name, enabled)),
                    cx,
                );
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Cannot toggle: {error}")), cx);
            }
        }
    }

    /// Ticket 30: the removed duplicate Enable/Disable button's commit path.
    /// Test-only seam — production uses the `Enabled` Switch.
    #[cfg(test)]
    fn toggle_selected_enabled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        let id = config.id.clone();
        let enabled = !config.enabled;
        self.commit_enabled_flag(&id, enabled, window, cx);
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
    /// Ticket 30: the joined per-shard detail (`shard: cause[; …]`) is
    /// available via [`AppView::poll_engine_detail`] for notifications —
    /// a crash is never status-bar-only.
    fn poll_engine(&mut self) -> Option<String> {
        self.engine.poll()
    }

    /// Ticket 30: poll for crashes with the joined per-shard detail (always
    /// `shard: cause` per shard, via `EngineController::crash_detail`).
    /// Single-shard crashes keep their shard prefix here (unlike `poll()`'s
    /// bare cause) so the notification can name the shard.
    fn poll_engine_detail(&mut self) -> Option<String> {
        let crashes = self.engine.poll_shards();
        if crashes.is_empty() {
            return None;
        }
        // Matches `poll()`'s active-empty silence: with nothing served there
        // is no serving truth to report (the one-shot poll is consumed).
        if self.engine.running_count() == 0 {
            return None;
        }
        Some(EngineController::crash_detail(&crashes))
    }

    /// Ticket 30: notify one crash as an error naming shard/cause with the
    /// View-logs path. No-op when there is no crash. Returns the detail when
    /// notified (tests pin delivery by count + the pure copy).
    fn notify_engine_crash(&mut self, window: &mut Window, cx: &mut App) -> Option<String> {
        let detail = self.poll_engine_detail()?;
        window.push_notification(Notification::error(crash_notification_text(&detail)), cx);
        Some(detail)
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
        // Ticket 30: no selection + Start (F5 included) explains itself —
        // never a dead click.
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        // Ticket 30: a second Start while one is in flight is ignored (no
        // second apply).
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
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
    /// Ticket 23: post-gate starts use the confirmed variant (the controller
    /// enforces LAN confirms itself); check output is redacted for display.
    /// Ticket 30: re-entrant starts are ignored (busy guard); starting an
    /// already-running row reports `already running (restarted)`, never a
    /// bare `Started`.
    fn commit_start_selected(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        self.start_in_flight = true;
        let Ok(Some(config)) = self.store.get(id) else {
            self.start_in_flight = false;
            return;
        };
        let was_running = self.engine.is_running(id);
        if let Some(warning) = lan_exposure_warning(&config.name, &config.bind_ip) {
            window.push_notification(Notification::warning(warning), cx);
        }
        match self.engine.start_one_confirmed(&config) {
            Ok(()) => {
                // Ticket 26: a flag write that fails after a successful start
                // is loud (never a silent `let _`): the endpoint runs while
                // the flag lies, so the toast says exactly that.
                match self.store.set_enabled(id, true) {
                    Ok(_) => {
                        self.reload(window, cx);
                        let message = if was_running {
                            already_running_message(&config.name)
                        } else {
                            format!("Started \"{}\"", config.name)
                        };
                        window.push_notification(Notification::success(message), cx);
                    }
                    Err(error) => {
                        self.reload(window, cx);
                        window.push_notification(
                            Notification::error(format!(
                                "Started \"{}\" but could not save its flag: {error}",
                                config.name
                            )),
                            cx,
                        );
                    }
                }
            }
            Err(error) => {
                window
                    .push_notification(Notification::error(redact_secrets(&error.to_string())), cx);
            }
        }
        self.start_in_flight = false;
    }

    /// Ticket 05: stop the selected config; its endpoint closes, the rest
    /// keep serving on a regenerated config.
    fn stop_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            return;
        };
        let id = config.id.clone();
        if self.engine.stop_one(&id) {
            // Ticket 26: loud on flag-write failure (the endpoint stopped
            // while the flag still claims enabled).
            match self.store.set_enabled(&id, false) {
                Ok(_) => {
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(format!("Stopped \"{}\"", config.name)),
                        cx,
                    );
                }
                Err(error) => {
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::error(format!(
                            "Stopped \"{}\" but could not save its flag: {error}",
                            config.name
                        )),
                        cx,
                    );
                }
            }
        } else {
            // Ticket 17: honest stop-on-stopped (never silence, never a
            // success claim over nothing).
            window.push_notification(
                Notification::warning(already_stopped_message(&config.name)),
                cx,
            );
        }
    }

    /// Ticket 05: start every stored config at once (each keeps its own
    /// endpoint; the enabled flag follows the engine state).
    /// Ticket 09: any non-loopback bind routes through one confirm dialog
    /// naming every LAN-bound row — nothing starts until the user confirms.
    /// Ticket 26 (L2 ruling): the confirm text snapshots the rows at open
    /// while [`AppView::commit_start_all`] re-lists fresh at commit. The text
    /// is point-in-time information; the commit set is authoritative, and the
    /// LAN gate re-evaluates on the fresh rows — a row added or removed
    /// between open and confirm can neither sneak past nor dodge the gate.
    fn start_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Ticket 30: a second Start-all while one is in flight is ignored.
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        // Ticket 26: a broken database errors here (never "the library is
        // empty" over rows we simply failed to read).
        let Some(rows) = self.list_or_notify(window, cx) else {
            return;
        };
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
    /// Ticket 17: an empty library warns honestly (never "Started 0").
    /// Ticket 23: post-gate starts use the confirmed variant; check output
    /// is redacted for display.
    /// Ticket 30: best-effort — one broken row never blocks the rest. The
    /// fast path starts the whole set at once; on failure each row starts
    /// alone so valid rows keep serving while the toast names the failed
    /// rows plus both counts. Re-entrant calls are ignored (busy guard).
    fn commit_start_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        self.start_in_flight = true;
        // Ticket 26: same honesty as the pre-gate read — a corrupt database
        // names its error instead of claiming an empty library.
        let Some(rows) = self.list_or_notify(window, cx) else {
            self.start_in_flight = false;
            return;
        };
        if rows.is_empty() {
            window.push_notification(Notification::warning(start_all_message(0)), cx);
            self.start_in_flight = false;
            return;
        }
        match self.engine.start_all_confirmed(&rows) {
            Ok(()) => {
                // Ticket 26: flag writes after a successful start are loud
                // in aggregate (never silent per-row `let _`).
                let mut flag_errors = 0;
                let mut first_error = String::new();
                for row in &rows {
                    if let Err(error) = self.store.set_enabled(&row.id, true) {
                        if flag_errors == 0 {
                            first_error = error.to_string();
                        }
                        flag_errors += 1;
                    }
                }
                self.reload(window, cx);
                self.warn_for_lan_binds(window, cx);
                self.start_in_flight = false;
                if flag_errors == 0 {
                    window.push_notification(
                        Notification::success(start_all_message(rows.len())),
                        cx,
                    );
                } else {
                    window.push_notification(
                        Notification::error(format!(
                            "Started {} endpoint(s) but {flag_errors} flag(s) failed to save: {first_error}",
                            rows.len()
                        )),
                        cx,
                    );
                }
            }
            Err(fast_error) => {
                // Ticket 30: union-level failures (port conflicts, missing
                // binaries, check/spawn, bridge, LAN gate) stay atomic —
                // nothing starts, exactly like before. Only row-local schema
                // failures isolate per-row so one broken row never blocks the
                // rest (best-effort fallback below).
                if !is_row_local_start_error(&fast_error) {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    window.push_notification(
                        Notification::error(redact_secrets(&fast_error.to_string())),
                        cx,
                    );
                    return;
                }
                // Best-effort fallback: isolate the broken rows per-row.
                // Start from a clean slate so a half-validated fast path
                // leaves no residue, then serve each row alone.
                self.engine.stop_all();
                let mut started: Vec<ProxyConfig> = Vec::new();
                let mut failed_names: Vec<String> = Vec::new();
                for row in &rows {
                    match self.engine.start_one_confirmed(row) {
                        Ok(()) => started.push(row.clone()),
                        Err(_) => {
                            failed_names.push(format!("\"{}\"", row.name));
                        }
                    }
                }
                if started.is_empty() {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    window.push_notification(
                        Notification::error(redact_secrets(&fast_error.to_string())),
                        cx,
                    );
                    return;
                }
                // Flag only the rows that actually serve (never claim a
                // failed row started).
                let mut flag_errors = 0;
                let mut first_error = String::new();
                for row in &started {
                    if let Err(error) = self.store.set_enabled(&row.id, true) {
                        if flag_errors == 0 {
                            first_error = error.to_string();
                        }
                        flag_errors += 1;
                    }
                }
                self.reload(window, cx);
                self.warn_for_lan_binds(window, cx);
                self.start_in_flight = false;
                if flag_errors > 0 {
                    window.push_notification(
                        Notification::error(format!(
                            "Started {} endpoint(s) but {flag_errors} flag(s) failed to save: {first_error}",
                            started.len()
                        )),
                        cx,
                    );
                    return;
                }
                window.push_notification(
                    Notification::warning(start_all_partial_message(started.len(), &failed_names)),
                    cx,
                );
            }
        }
    }

    /// Ticket 05: stop everything and take the sidecar down.
    /// Ticket 17: honest when nothing runs (never a success claim over
    /// silence).
    fn stop_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let running = self.engine.running_count();
        self.engine.stop_all();
        // Ticket 26: the engine stops regardless; the flag sweep after it is
        // loud on database failure (and skips the stale-flag lie).
        match self.store.list() {
            Ok(rows) => {
                let mut flag_errors = 0;
                let mut first_error = String::new();
                for row in &rows {
                    if let Err(error) = self.store.set_enabled(&row.id, false) {
                        if flag_errors == 0 {
                            first_error = error.to_string();
                        }
                        flag_errors += 1;
                    }
                }
                self.reload(window, cx);
                if flag_errors > 0 {
                    window.push_notification(
                        Notification::error(format!(
                            "Stopped all endpoints but {flag_errors} flag(s) failed to save: {first_error}"
                        )),
                        cx,
                    );
                    return;
                }
            }
            Err(error) => {
                window.push_notification(Notification::error(db_list_error_message(&error)), cx);
                self.reload_quiet(cx);
                return;
            }
        }
        let message = stop_all_message(running);
        if running == 0 {
            window.push_notification(Notification::warning(message), cx);
        } else {
            window.push_notification(Notification::success(message), cx);
        }
    }

    /// Ticket 33: names of the currently running endpoints (for the
    /// close-confirm). A broken database yields no names — the close then
    /// proceeds, since there is nothing nameable to protect.
    fn running_names_for_close(&self) -> Vec<String> {
        let running = self.engine.running_ids();
        if running.is_empty() {
            return Vec::new();
        }
        match self.store.list() {
            Ok(rows) => rows
                .iter()
                .filter(|row| running.contains(&row.id))
                .map(|row| row.name.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Ticket 33: close-confirm verdict for the `on_window_should_close`
    /// hook. Returns the confirm text when the close must be vetoed (the
    /// caller opens the dialog and returns false); `None` lets the close
    /// proceed (nothing running, or already confirmed).
    fn close_veto_text(&self) -> Option<String> {
        if self.close_confirmed {
            return None;
        }
        close_confirm_description(&self.running_names_for_close())
    }

    /// Ticket 33: the confirmed close path (runs from the confirm dialog's
    /// OK): stops every endpoint, best-effort clears the enabled flags so a
    /// relaunch does not show running rows that died with the window, and
    /// marks the close confirmed so the hook lets the second close through.
    fn confirm_close_stop(&mut self) {
        self.engine.stop_all();
        if let Ok(rows) = self.store.list() {
            for row in &rows {
                let _ = self.store.set_enabled(&row.id, false);
            }
        }
        self.close_confirmed = true;
    }

    /// Ticket 33: About dialog — version + git hash + sidecar presence +
    /// data paths + shortcuts list. Informational only; OK just closes.
    fn open_about_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sidecars = [
            (engine_binary_file_name(), resolve_engine_binary().is_ok()),
            (fork_binary_file_name(), resolve_fork_binary().is_ok()),
            (xray_binary_file_name(), resolve_xray_binary().is_ok()),
        ]
        .into_iter()
        .map(|(file, found)| sidecar_presence_line(&file, found))
        .collect::<Vec<_>>();
        let body = about_body(
            APP_VERSION,
            GIT_HASH,
            &sidecars,
            &Store::default_path().display().to_string(),
            &Store::default_export_path().display().to_string(),
        );
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .title(format!("About {APP_TITLE}"))
                .description(body.clone())
                .on_ok(|_, _, _| true)
        });
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
    /// traffic, refresh the rows, and summarize.
    /// Ticket 12: outcomes persist (latest sample + dead flag survive
    /// restart) via [`AppView::record_and_persist`].
    /// Ticket 26 (ticket-25 consumer): the blocking probes run on the
    /// background executor via [`AppView::spawn_probe`] — the click returns
    /// after starting them, never after a 5s-per-row freeze. Returns the
    /// number of probes started.
    fn test_all_health(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let Some(rows) = self.running_rows_or_notify(window, cx) else {
            return 0;
        };
        if rows.is_empty() {
            window.push_notification(Notification::warning("No running endpoints to test"), cx);
            return 0;
        }
        if self.probe_in_flight {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return 0;
        }
        self.spawn_probe(rows, false, true, Some(window.window_handle()), cx)
    }

    /// Ticket 06: URL-test just the selected config's endpoint.
    /// Ticket 26: off-thread via [`AppView::spawn_probe`] (silent record,
    /// like before — no toast).
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
        if self.probe_in_flight {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return;
        }
        self.spawn_probe(vec![config], false, false, None, cx);
    }

    /// Ticket 06: TCP-ping just the selected config's endpoint.
    /// Ticket 26: off-thread via [`AppView::spawn_probe`] (silent record).
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
        if self.probe_in_flight {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return;
        }
        self.spawn_probe(vec![config], true, false, None, cx);
    }

    /// Ticket 26 (ticket-25 consumer): run `test_all`/`ping_all` off the UI
    /// thread. The boxed prober moves into the background task (a default
    /// stands in, guarded by `probe_in_flight` so it is never observable);
    /// completion restores the prober and records on the main thread via
    /// [`AppView::finish_probe`]. `toast` selects the manual-test summary
    /// (the captured window) versus the silent tick record. A released view
    /// — or a window closed mid-flight — fails closed: outcomes are recorded
    /// quietly or dropped without touching dead state.
    /// Returns the number of probes started.
    fn spawn_probe(
        &mut self,
        rows: Vec<ProxyConfig>,
        ping: bool,
        toast: bool,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) -> usize {
        if rows.is_empty() || self.probe_in_flight {
            return 0;
        }
        let count = rows.len();
        self.probe_in_flight = true;
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let (outcomes, prober) = cx
                .background_executor()
                .spawn(async move {
                    let mut prober = prober;
                    let outcomes = if ping {
                        ping_all(&rows, &mut *prober)
                    } else {
                        test_all(&rows, DEFAULT_TEST_URL, &mut *prober)
                    };
                    (outcomes, prober)
                })
                .await;
            cx.update(|cx| {
                // Record half: restore the prober, clear the flag, persist.
                // The borrow ends here so the toast half below re-enters
                // cleanly (never update-while-updating).
                let recorded = weak.update(cx, |view, _| {
                    view.prober = prober;
                    view.probe_in_flight = false;
                    view.record_and_persist(&outcomes);
                    (view.health_counts(), outcomes.len())
                });
                let Ok(((live, dead), count)) = recorded else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };
                // Toast half (fresh borrow): manual tests toast through the
                // captured window; everything else reloads quietly.
                if toast {
                    if let Some(handle) = window {
                        if handle
                            .update(cx, |_, window, cx| {
                                view.update(cx, |view, cx| {
                                    view.reload(window, cx);
                                    window.push_notification(
                                        Notification::success(format!(
                                            "Tested {count} endpoint(s): {live} live · {dead} dead"
                                        )),
                                        cx,
                                    );
                                });
                            })
                            .is_ok()
                        {
                            return;
                        }
                    }
                }
                view.update(cx, |view, cx| view.reload_quiet(cx));
            });
        })
        .detach();
        count
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
        // Ticket 26: a broken database skips ingest (keeps old lines) instead
        // of tagging against a lying empty row set.
        let Ok(active) = self.store.list() else {
            return;
        };
        for line in &lines {
            self.logs.push_raw(line, &active);
        }
    }

    /// Ticket 06: visible log lines for the dialog — the selected config's
    /// lines when a row is selected, else engine-global + all — further
    /// narrowed by the `log_filter` substring. Newest last, capped.
    /// Ticket 34: sidecar stderr carries ANSI color escapes (sing-box
    /// `[31m`/`[38;5;…`); the Logs dialog renders them stripped via the
    /// shared `strip_ansi_codes` helper. Stored records stay verbatim so
    /// filtering and any future export see the original bytes.
    fn log_lines(&self, cx: &App) -> Vec<String> {
        let selected = self.selected_config(cx).map(|config| config.id);
        let filter = self.log_filter.trim().to_lowercase();
        let render =
            |level: &str, message: &str| format!("[{}] {}", level, strip_ansi_codes(message));
        let mut lines: Vec<String> = match selected.as_deref() {
            Some(id) => self
                .logs
                .for_config(id)
                .iter()
                .map(|record| render(record.level.as_str(), &record.message))
                .collect(),
            None => self
                .logs
                .all()
                .iter()
                .map(|record| render(record.level.as_str(), &record.message))
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
    /// Ticket 26 (L4 ruling): the lines snapshot at open; new sidecar lines
    /// need a reopen. Live re-rendering an open dialog on every engine tick
    /// would fight the user's scroll position and filter typing.
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
        // Ticket 18: Rc-shared with the footer OK button (mouse commits
        // identically to the Confirm key path).
        // Ticket 26 (L5): the commit never closes directly — returning true
        // lets the key path (framework) and the mouse path (footer) each
        // close exactly once. The previous explicit `close_all_dialogs` here
        // double-closed on the mouse path.
        let commit: DialogCommit = Rc::new(move |_window, cx| {
            let filter = reader.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.log_filter = filter;
                cx.notify();
            });
            true
        });
        let commit_key = commit.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
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
            dialog
                .title(title.clone())
                .child(body)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "logs-dialog-ok",
                    "logs-dialog-cancel",
                    commit_click,
                ))
        });
    }

    /// Ticket 11: user-owned JSON export behind the plaintext-secret
    /// warning. The confirm dialog shows [`export_confirm_description`]
    /// (the [`store::export_warning_text`] verbatim); confirming writes the
    /// file, dismissing writes nothing. The payload round-trips through the
    /// import grammar (re-import yields duplicates, never adds).
    /// Ticket 17: the dialog previews row counts, the destination path, and
    /// an overwrite warning when the file already exists.
    /// Ticket 26 (L3 ruling): the preview snapshots at open (standard dialog
    /// behavior). The commit re-lists for its toast, and the overwrite
    /// confirm stands regardless — reopening the dialog refreshes the
    /// preview.
    fn open_export_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        // Ticket 26: a broken database errors instead of previewing counts
        // over rows we failed to read.
        let Some(rows) = self.list_or_notify(window, cx) else {
            return;
        };
        let path = Store::default_export_path();
        let preview = export_preview_description(
            rows.len(),
            count_importable(&rows),
            &path.display().to_string(),
            path.exists(),
        );
        let description = format!("{}\n\n{preview}", export_confirm_description());
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

    /// Ticket 26 (ticket-25 consumer): library-export restore behind an
    /// explicit confirm. The payload carries full-fidelity rows, subs, and
    /// tombstones; restoring replaces the current library verbatim (row ids,
    /// groups, enabled flags, subscriptions, and deletions land exactly as
    /// exported), so the dialog names what it will do. Dismissing changes
    /// nothing. The notification names only counts — never row contents.
    fn open_restore_confirm(
        &mut self,
        payload: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let payload = payload.clone();
            alert
                .confirm()
                .title("Restore library")
                .description(
                    "This replaces the current library with the exported backup \
                     (row ids, groups, enabled flags, subscriptions, and deletions \
                     are restored exactly).",
                )
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_library_restore(&payload, window, cx)
                    });
                    true
                })
        });
    }

    /// Ticket 26: the actual restore (post-confirm). The payload is dry-run
    /// into a throwaway store first (a malformed backup must never wipe a
    /// good library), then the current library is wiped and the payload
    /// rebuilt verbatim in its single transaction. Success reloads and names
    /// the restored counts ([`restore_done_message`]); failure names the
    /// export error. Row ids survive, so running endpoints are unaffected.
    fn commit_library_restore(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match Store::open_in_memory()
            .and_then(|mut dry| dry.restore_library_json(payload).map(|_| ()))
        {
            Ok(()) => {}
            Err(error) => {
                window
                    .push_notification(Notification::error(format!("Restore failed: {error}")), cx);
                return;
            }
        }
        if let Err(error) = self.wipe_library_for_restore() {
            window.push_notification(
                Notification::error(format!(
                    "Restore failed: cannot clear the library ({error})"
                )),
                cx,
            );
            return;
        }
        match self.store.restore_library_json(payload) {
            Ok(outcome) => {
                self.reload(window, cx);
                window.push_notification(Notification::success(restore_done_message(&outcome)), cx);
            }
            Err(error) => {
                // The dry run passed, so this is unreachable in practice;
                // reported loudly all the same (the library was cleared).
                self.reload(window, cx);
                window.push_notification(
                    Notification::error(format!(
                        "Restore failed after clearing the library: {error}"
                    )),
                    cx,
                );
            }
        }
    }

    /// Ticket 26: empty the library ahead of a verbatim restore.
    /// Subscriptions go first: [`Store::remove_subscription`] releases their
    /// rows to hand-owned instead of deleting them, so the wipe itself
    /// records no tombstones; the row sweep then deletes cleanly. Tombstones
    /// that predate the wipe carry over (they only gate re-adding vanished
    /// rows, never the restored rows themselves).
    fn wipe_library_for_restore(&mut self) -> Result<(), StoreError> {
        let subs = self.store.list_subscriptions()?;
        for sub in &subs {
            self.store.remove_subscription(&sub.id)?;
        }
        let rows = self.store.list()?;
        for row in &rows {
            self.store.remove(&row.id)?;
        }
        Ok(())
    }

    fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = ConfigForm::blank(window, cx);
        let view = cx.entity();
        let reader = form.clone();
        // Ticket 31: the shared first-error slot — the commit records it and
        // the builder renders it inline under the fields on the next frame.
        let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let error_for_commit = form_error.clone();
        // Ticket 31: rows snapshot for the live bind/port collision warning
        // (a broken database warns nothing — the commit still reports it).
        let rows = self.store.list().unwrap_or_default();
        // Ticket 18: Rc-shared with the footer OK button (mouse commits
        // identically to the Confirm key path, including stay-open-on-error).
        let commit: DialogCommit = Rc::new(move |window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_with_focus(&reader, &error_for_commit, window, cx)
            })
        });
        let commit_key = commit.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            // Ticket 31: live warning inputs — the builder re-runs every
            // render, so the collision line tracks typing.
            let live = form.clone();
            let bind = live.bind.read(cx).value().to_string();
            let port = live.port.read(cx).value().to_string();
            let collision = bind_port_collision_warning(&rows, None, &bind, &port);
            let inline = form_error.borrow().clone();
            dialog
                .title("Add proxy configuration")
                .child(form.clone().element(cx, collision, inline))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "add-dialog-ok",
                    "add-dialog-cancel",
                    commit_click,
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
        // Ticket 31: the shared first-error slot (see `open_add_dialog`).
        let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let error_for_commit = form_error.clone();
        // Ticket 31: rows snapshot for the live collision warning; the edited
        // row itself is excluded.
        let rows = self.store.list().unwrap_or_default();
        let exclude = id.clone();
        // Ticket 18: Rc-shared with the footer OK button (mouse commits
        // identically to the Confirm key path, including stay-open-on-error).
        // Ticket 26: the patch is relative to this dialog-open snapshot —
        // only fields the user actually changed are written, so a refresh
        // that lands mid-edit survives on every untouched field.
        let snapshot = config.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let patch = reader.as_patch_vs(cx, &snapshot);
            view.update(cx, |view, cx| {
                view.commit_edit_with_focus(&id, patch, &reader, &error_for_commit, window, cx)
            })
        });
        let commit_key = commit.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let live = form.clone();
            let bind = live.bind.read(cx).value().to_string();
            let port = live.port.read(cx).value().to_string();
            let collision = bind_port_collision_warning(&rows, Some(&exclude), &bind, &port);
            let inline = form_error.borrow().clone();
            dialog
                .title(title.clone())
                .child(form.clone().element(cx, collision, inline))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "edit-dialog-ok",
                    "edit-dialog-cancel",
                    commit_click,
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
            Some(files) => {
                ImportForm::new(window, cx, prefill_text, prefill_group, None, Some(files))
            }
            None => ImportForm::new(window, cx, prefill_text, prefill_group, file_hint, None),
        };
        let view = cx.entity();
        let reader = form.clone();
        // Ticket 18: Rc-shared with the footer OK button (mouse commits
        // identically to the Confirm key path, including stay-open-on-error).
        // Ticket 26: the dialog commit parses on the background executor
        // ([`AppView::commit_import_dialog`]) — it returns false immediately
        // and the completion closes the dialog on success.
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let text = reader.text.read(cx).value().to_string();
            let group = reader.group.read(cx).value().to_string();
            let hint = reader.file_hint.clone();
            let original = reader.original_text.clone();
            let multi = reader.multi_files.clone();
            view.update(cx, |view, cx| {
                view.commit_import_dialog(text, group, hint, Some(original), multi, window, cx)
            })
        });
        let commit_key = commit.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title("Import proxy configurations")
                .child(form.clone().element(cx))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "import-dialog-ok",
                    "import-dialog-cancel",
                    commit_click,
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
    /// Ticket 26: unreadable files are named (basenames only — full paths can
    /// carry OS usernames), never silently skipped.
    fn import_dropped_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut merged = String::new();
        let mut names: Vec<String> = Vec::new();
        let mut files: Vec<(String, String)> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();
        for path in paths.0.iter() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if !merged.is_empty() {
                    merged.push('\n');
                }
                merged.push_str(&content);
                let name = path.to_string_lossy().into_owned();
                names.push(name.clone());
                files.push((name, content));
            } else {
                unreadable.push(file_basename(&path.to_string_lossy()));
            }
        }
        if merged.trim().is_empty() {
            if unreadable.is_empty() {
                window.push_notification(Notification::warning("No readable files dropped"), cx);
            } else {
                window.push_notification(
                    Notification::warning(unreadable_drop_message(&unreadable)),
                    cx,
                );
            }
            return;
        }
        if !unreadable.is_empty() {
            window.push_notification(
                Notification::warning(unreadable_drop_message(&unreadable)),
                cx,
            );
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
    /// Ticket 26: library-export payloads divert to the restore confirm
    /// path; everything else shares [`resolve_import_inputs`] /
    /// [`parse_import_inputs`] with the background dialog path.
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
        if is_library_export(&text) {
            self.open_restore_confirm(text, window, cx);
            return false;
        }
        let resolved = resolve_import_inputs(text, group, file_hint, original, multi);
        if let Some(done) = self.try_addsub_commit(&resolved, window, cx) {
            return done;
        }
        let batch = parse_import_inputs(&resolved);
        self.finish_import_batch(batch, &resolved.group, window, cx)
    }

    /// Ticket 26: the dialog import commit — the CPU parse runs on the
    /// background executor, the store commit stays on the main thread (the
    /// store owns a `!Send` connection; the single-transaction
    /// [`commit_batch`] keeps it fast). Returns false immediately: the dialog
    /// stays open until the completion commits and closes it on success, or
    /// toasts the failure with the dialog kept open. A second OK while a
    /// parse runs is ignored (no double commit).
    #[allow(clippy::too_many_arguments)]
    fn commit_import_dialog(
        &mut self,
        text: String,
        group: String,
        file_hint: Option<String>,
        original: Option<String>,
        multi: Option<Vec<(String, String)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_library_export(&text) {
            self.open_restore_confirm(text, window, cx);
            return false;
        }
        let resolved = resolve_import_inputs(text, group, file_hint, original, multi);
        if let Some(done) = self.try_addsub_commit(&resolved, window, cx) {
            return done;
        }
        if self.import_parse_in_flight {
            return false;
        }
        self.import_parse_in_flight = true;
        let weak = cx.entity().downgrade();
        // Ticket 26: the completion commits through THIS window's handle
        // (captured now — `active_window` is unreliable by completion time,
        // and the dialog to close lives here). A window closed mid-parse
        // falls back to the silent headless commit.
        let handle = window.window_handle();
        cx.spawn(async move |_entity, cx| {
            let group = resolved.group.clone();
            let batch = cx
                .background_executor()
                .spawn(async move { parse_import_inputs(&resolved) })
                .await;
            cx.update(|cx| {
                let mut pending = Some((batch, group));
                let delivered = handle
                    .update(cx, |_, window, cx| {
                        if let Some((batch, group)) = pending.take() {
                            let _ = weak.update(cx, |view, cx| {
                                view.finish_import_parse(batch, group, window, cx);
                            });
                        }
                    })
                    .is_ok();
                if !delivered {
                    if let Some((batch, group)) = pending.take() {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_import_parse_no_window(batch, group, cx);
                        });
                    }
                }
            });
        })
        .detach();
        false
    }

    /// Ticket 26: main-thread completion for [`AppView::commit_import_dialog`].
    /// Commits through the shared [`AppView::finish_import_batch`] tail and
    /// closes the dialog on success (total failure keeps it open with the
    /// error, exactly like the synchronous path).
    fn finish_import_parse(
        &mut self,
        batch: crate::import::BatchResult,
        group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.import_parse_in_flight = false;
        if self.finish_import_batch(batch, &group, window, cx) {
            window.close_all_dialogs(cx);
        }
    }

    /// Ticket 26: window-less completion (the window closed mid-parse). The
    /// user-initiated commit still lands via the single-transaction
    /// [`commit_batch`]; with no window there is nothing to toast to and
    /// nothing to close.
    fn finish_import_parse_no_window(
        &mut self,
        batch: crate::import::BatchResult,
        group: String,
        cx: &mut Context<Self>,
    ) {
        self.import_parse_in_flight = false;
        if commit_batch(&mut self.store, batch, &group).is_ok() {
            self.reload_quiet(cx);
        }
    }

    /// Ticket 26: single-line `throne://addsub/...` fast path shared by the
    /// synchronous and background commits. Returns `Some(done)` when the
    /// inputs are addsub-shaped (the parse itself is trivial CPU; the intake
    /// touches the store on this thread), `None` for everything else.
    /// Ticket 14: a malformed link reports its own parse reason (never the
    /// generic unknown-command) and keeps the dialog open.
    fn try_addsub_commit(
        &mut self,
        resolved: &ResolvedImport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        let trimmed = resolved.text.trim();
        if trimmed.is_empty() || trimmed.contains('\n') || resolved.file_hint.is_some() {
            return None;
        }
        if !is_addsub_attempt(trimmed) {
            return None;
        }
        Some(match parse_addsub_link(trimmed) {
            Ok(request) => {
                let effective = effective_addsub_group(&resolved.group, &request.group);
                self.commit_addsub_request_with_group(request, &effective, window, cx)
            }
            Err(error) => {
                window.push_notification(Notification::error(addsub_error_message(&error)), cx);
                false
            }
        })
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
        self.commit_subscription_add(
            label,
            request.url,
            dialog_group.to_string(),
            true,
            window,
            cx,
        )
    }

    /// Ticket 04: add a subscription record; on success the dialog closes and
    /// a first refresh starts in the background. Invalid input keeps the
    /// dialog open with an error notification.
    /// Ticket 17: the add-form auto-refresh checkbox lands here (no more
    /// create-then-toggle).
    /// Ticket 31: an already-subscribed URL takes the friendly path — a
    /// warning naming the existing label, no double row, dialog stays open.
    fn commit_subscription_add(
        &mut self,
        label: String,
        url: String,
        group: String,
        auto_refresh: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // A broken-database lookup must not block the add: fall through and
        // let the insert report the real error.
        if let Ok(Some(existing)) = self.store.find_subscription_by_url(&url) {
            window.push_notification(
                Notification::warning(already_subscribed_message(&existing.label)),
                cx,
            );
            return false;
        }
        match self
            .store
            .add_subscription(&label, &url, &group, auto_refresh)
        {
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

    /// Ticket 31: edit a subscription's label/url/group in place. Persists
    /// via [`Store::update_subscription`], refreshes the list, and closes on
    /// success (the manager dialog reopens with the new values through
    /// [`AppView::reopen_subscriptions_if_open`]). A row removed mid-edit
    /// warns and closes; validation failures keep the dialog open with the
    /// error (retryable).
    fn commit_subscription_edit(
        &mut self,
        id: &str,
        label: String,
        url: String,
        group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.update_subscription(id, &label, &url, &group) {
            Ok(updated) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Updated \"{}\"", updated.label)),
                    cx,
                );
                self.reopen_subscriptions_if_open(window, cx);
                true
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                self.reopen_subscriptions_if_open(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    /// Ticket 17: open a coalescing batch for background refreshes. Member
    /// completions accumulate instead of notifying; the last member flushes
    /// at most one toast (see [`AppView::accumulate_batch_member`]). Empty
    /// input is a no-op (never an empty toast).
    /// Ticket 26: opening never resets an open batch. A tick that fires while
    /// members are still in flight unions its ids in (totals grow by the new
    /// ids only) and keeps every accumulated count — no partial loss, no dup
    /// toasts. The generation bumps only on the empty → open transition, so
    /// tests can pin the batch epoch.
    fn begin_refresh_batch(&mut self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        if self.refresh_batch_members.is_empty() {
            self.refresh_generation += 1;
        }
        for id in ids {
            if self.refresh_batch_members.insert(id.clone()) {
                self.refresh_batch_total += 1;
            }
        }
    }

    /// Ticket 17: accumulate one batch member completion. Returns true when
    /// this completion finished the batch (the caller then flushes once).
    /// Callers check [`AppView::is_batch_member`] first: non-members notify
    /// immediately instead of accumulating.
    /// Ticket 26: conflicts (preserved user edits) accumulate alongside the
    /// churn counts for the coalesced copy.
    fn accumulate_batch_member(
        &mut self,
        sub_id: &str,
        added: usize,
        updated: usize,
        removed: usize,
        failed: usize,
        conflicts: usize,
    ) -> bool {
        if !self.refresh_batch_members.remove(sub_id) {
            return false;
        }
        self.refresh_batch_added += added;
        self.refresh_batch_updated += updated;
        self.refresh_batch_removed += removed;
        self.refresh_batch_failed += failed;
        self.refresh_batch_conflicts += conflicts;
        self.refresh_batch_members.is_empty()
    }

    /// Ticket 17: true while `sub_id` belongs to the open background batch.
    fn is_batch_member(&self, sub_id: &str) -> bool {
        self.refresh_batch_members.contains(sub_id)
    }

    /// Ticket 17: close one batch slot, flushing the single coalesced toast
    /// when the last member lands. No-op for non-members.
    fn close_batch_slot(&mut self, sub_id: &str, window: &mut Window, cx: &mut App) {
        if self.is_batch_member(sub_id) && self.accumulate_batch_member(sub_id, 0, 0, 0, 0, 0) {
            self.flush_refresh_batch(window, cx);
        }
    }

    /// Ticket 17: route one refresh completion. Batch members accumulate
    /// (the last member flushes at most one toast); everyone else notifies
    /// immediately with `notice`.
    #[allow(clippy::too_many_arguments)]
    fn route_refresh_completion(
        &mut self,
        sub_id: &str,
        notice: Notification,
        added: usize,
        updated: usize,
        removed: usize,
        failed: usize,
        conflicts: usize,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.is_batch_member(sub_id) {
            if self.accumulate_batch_member(sub_id, added, updated, removed, failed, conflicts) {
                self.flush_refresh_batch(window, cx);
            }
        } else {
            window.push_notification(notice, cx);
        }
    }

    /// Ticket 26: take every batch counter (plus the member set size) and
    /// reset the batch in one step — the atomic swap. A completion racing a
    /// flush can never observe a half-reset batch.
    fn take_batch_counts(&mut self) -> (usize, usize, usize, usize, usize, usize) {
        self.refresh_batch_members.clear();
        let total = std::mem::take(&mut self.refresh_batch_total);
        let added = std::mem::take(&mut self.refresh_batch_added);
        let updated = std::mem::take(&mut self.refresh_batch_updated);
        let removed = std::mem::take(&mut self.refresh_batch_removed);
        let failed = std::mem::take(&mut self.refresh_batch_failed);
        let conflicts = std::mem::take(&mut self.refresh_batch_conflicts);
        (total, added, updated, removed, failed, conflicts)
    }

    /// Ticket 17: flush a completed batch as at most one toast, then reset.
    /// `total` is always ≥ 1 here (batches never open empty), so the message
    /// helper always returns `Some`.
    fn flush_refresh_batch(&mut self, window: &mut Window, cx: &mut App) {
        let (total, added, updated, removed, failed, conflicts) = self.take_batch_counts();
        let (warn, text) =
            coalesced_refresh_message(total, added, updated, removed, failed, conflicts)
                .unwrap_or((false, String::new()));
        if text.is_empty() {
            return;
        }
        window.push_notification(
            if warn {
                Notification::warning(text)
            } else {
                Notification::success(text)
            },
            cx,
        );
    }

    /// Ticket 14: manual per-row refresh with truthful outcomes — a missing
    /// subscription warns (it was already removed), a store failure names
    /// the error (never a dead click).
    /// Ticket 17: manual refreshes leave any background batch (their
    /// completion notifies immediately instead of merging into it).
    /// Ticket 26: a manual refresh never double-fetches. When the sub already
    /// has a fetch in flight (tick or earlier click), the manual call adopts
    /// it — the sub leaves the background batch (with the total adjusted so
    /// the coalesced copy stays honest) and the shared completion notifies
    /// immediately. Returns true when a new fetch started.
    fn refresh_subscription_by_id(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Adopt first: the completion must notify immediately either way.
        if self.refresh_batch_members.remove(sub_id) {
            self.refresh_batch_total = self.refresh_batch_total.saturating_sub(1);
        }
        match self.store.get_subscription(sub_id) {
            Ok(Some(sub)) => self.start_subscription_refresh(sub, cx),
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                false
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh subscription: {error}")),
                    cx,
                );
                false
            }
        }
    }

    /// Ticket 14: auto-refresh toggle with truthful outcomes (never a dead
    /// click on store failure).
    /// Ticket 26: the current value is re-read at commit — the row's `Auto`
    /// button snapshot may be stale (a tick or another click may have flipped
    /// it since the dialog rendered). A missing row warns; a store failure
    /// names the error.
    fn commit_subscription_auto(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = match self.store.get_subscription(sub_id) {
            Ok(Some(sub)) => sub.auto_refresh,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot change auto-refresh: {error}")),
                    cx,
                );
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
        };
        match self.store.set_subscription_auto_refresh(sub_id, !current) {
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

    /// Ticket 26: record one fetch as in flight. Returns false when the sub
    /// already has a fetch running (manual-vs-tick double fetch is skipped,
    /// never stacked). Pure state — tests drive it without touching the
    /// network.
    fn note_refresh_started(&mut self, sub_id: &str) -> bool {
        self.inflight_refreshes.insert(sub_id.to_string())
    }

    /// Ticket 04: fetch one subscription without blocking the UI, then merge
    /// through the tested [`Store::refresh_with_body`] seam.
    /// Ticket 12: window-free (the completion path branches on the active
    /// window itself) so the due-subscription background tick can call it.
    /// Ticket 26: the task holds a `WeakEntity` — a released view fails
    /// closed (no post-close store writes). A sub with a fetch already in
    /// flight is skipped (see [`AppView::note_refresh_started`]); manual
    /// callers adopt the shared fetch instead (see
    /// [`AppView::refresh_subscription_by_id`]). Returns true when a fetch
    /// started.
    fn start_subscription_refresh(
        &mut self,
        sub: ProxySubscription,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.note_refresh_started(&sub.id) {
            return false;
        }
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        let sub_id = sub.id.clone();
        let label = sub.label.clone();
        let url = sub.url.clone();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_subscription_body(client, &url).await;
            let now = now_unix();
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_subscription_refresh(
                                &sub_id, &label, fetched, now, window, cx,
                            );
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_subscription_refresh_no_window(&sub_id, fetched, now, cx);
                    });
                }
            });
        })
        .detach();
        true
    }

    /// Ticket 04: apply a fetched body (or fetch error) with user feedback,
    /// then refresh the dialog contents if it is still open.
    /// Ticket 17: completions for background-batch members accumulate and
    /// flush at most one toast; every other completion notifies immediately.
    fn finish_subscription_refresh(
        &mut self,
        sub_id: &str,
        label: &str,
        fetched: Result<String, String>,
        now: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ticket 26: the fetch is no longer in flight (a manual refresh may
        // already have adopted its completion — see
        // [`AppView::refresh_subscription_by_id`]).
        self.inflight_refreshes.remove(sub_id);
        // Ticket 14: the subscription may have been deleted while the fetch
        // was in flight — drop the result silently (no failure toast for a
        // sub the user removed on purpose). A store failure here still
        // surfaces instead of silently swallowing the completion.
        match self.store.get_subscription(sub_id) {
            Ok(None) => {
                self.reload(window, cx);
                // Ticket 17: a deleted batch member still closes its slot.
                self.close_batch_slot(sub_id, window, cx);
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
            Err(error) => {
                self.route_refresh_completion(
                    sub_id,
                    Notification::error(format!("Cannot refresh subscription: {error}")),
                    0,
                    0,
                    0,
                    1,
                    0,
                    window,
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
                        self.route_refresh_completion(
                            sub_id,
                            Notification::error(format!("{label}: refresh failed ({error})")),
                            0,
                            0,
                            0,
                            1,
                            0,
                            window,
                            cx,
                        );
                    } else {
                        // Ticket 26 (ticket-25 consumer): conflicts surface —
                        // the merge kept the user's edits, and the toast says
                        // so instead of reporting a clean sync.
                        let failed = outcome.failures.len();
                        let (warn, summary) = refresh_summary_text(
                            label,
                            outcome.added,
                            outcome.updated,
                            outcome.removed,
                            outcome.skipped,
                            outcome.duplicates,
                            failed,
                            outcome.conflicts,
                        );
                        let notice = if warn {
                            Notification::warning(summary)
                        } else {
                            Notification::success(summary)
                        };
                        self.route_refresh_completion(
                            sub_id,
                            notice,
                            outcome.added,
                            outcome.updated,
                            outcome.removed,
                            failed,
                            outcome.conflicts,
                            window,
                            cx,
                        );
                    }
                }
                Err(error) => {
                    if matches!(error, StoreError::NotFound(_)) {
                        // Deleted between the check above and the merge.
                        self.reload(window, cx);
                        self.close_batch_slot(sub_id, window, cx);
                    } else {
                        self.route_refresh_completion(
                            sub_id,
                            Notification::error(format!("{label}: refresh failed ({error})")),
                            0,
                            0,
                            0,
                            1,
                            0,
                            window,
                            cx,
                        );
                    }
                }
            },
            Err(error) => match self.store.record_subscription_error(sub_id, &error) {
                Ok(_) => {
                    self.reload(window, cx);
                    self.route_refresh_completion(
                        sub_id,
                        Notification::error(format!("{label}: {error}")),
                        0,
                        0,
                        0,
                        1,
                        0,
                        window,
                        cx,
                    );
                }
                Err(StoreError::NotFound(_)) => {
                    // Deleted mid-flight: silent, like the pre-check arm.
                    self.reload(window, cx);
                    self.close_batch_slot(sub_id, window, cx);
                }
                Err(db_error) => {
                    self.route_refresh_completion(
                        sub_id,
                        Notification::error(format!("Cannot refresh subscription: {db_error}")),
                        0,
                        0,
                        0,
                        1,
                        0,
                        window,
                        cx,
                    );
                }
            },
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    /// Ticket 04: headless completion path (no window for notifications).
    /// Ticket 17: batch members accumulate silently (no window exists to
    /// flush a toast to); the counters reset when the batch completes.
    /// Ticket 26: conflicts accumulate like any other count; the reset is
    /// the atomic [`AppView::take_batch_counts`] swap. L1 ruling: a
    /// completed headless batch drops its toast by necessity (no window
    /// exists to toast to — the ticket-14 window-free precedent); the counts
    /// still accumulate so a later windowed flush stays honest.
    fn finish_subscription_refresh_no_window(
        &mut self,
        sub_id: &str,
        fetched: Result<String, String>,
        now: i64,
        cx: &mut Context<Self>,
    ) {
        self.inflight_refreshes.remove(sub_id);
        match fetched {
            Ok(body) => match self.store.refresh_with_body(sub_id, &body, now) {
                Ok(outcome) => {
                    self.accumulate_batch_member(
                        sub_id,
                        outcome.added,
                        outcome.updated,
                        outcome.removed,
                        outcome.failures.len(),
                        outcome.conflicts,
                    );
                }
                Err(_) => {
                    self.accumulate_batch_member(sub_id, 0, 0, 0, 1, 0);
                }
            },
            Err(error) => {
                let _ = self.store.record_subscription_error(sub_id, &error);
                self.accumulate_batch_member(sub_id, 0, 0, 0, 1, 0);
            }
        }
        if self.refresh_batch_members.is_empty() {
            self.take_batch_counts();
        }
        self.reload_quiet(cx);
    }

    /// Ticket 04: re-render the subscriptions dialog after an action, but
    /// only if the user still has it open.
    /// Ticket 26: never touch a foreign dialog. The reopen fires only while
    /// the subscriptions dialog itself owns the screen — tracked by the
    /// stashed [`SubForm`], which every dismiss path (OK, Cancel, Escape,
    /// backdrop/X via `.on_close`) clears. A background refresh completing
    /// while the user types in Add/Import/Edit/Export/Logs finds no stash
    /// and leaves that dialog and its input intact. A genuine reopen carries
    /// the in-progress add-form text across instead of rebuilding it blank.
    fn reopen_subscriptions_if_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.has_active_dialog(cx) {
            // No dialog at all: drop any stale stash and leave.
            self.subs_form = None;
            return;
        }
        let Some(form) = &self.subs_form else {
            // Some other dialog owns the screen — leave it alone.
            return;
        };
        let label = form.label.read(cx).value().to_string();
        let url = form.url.read(cx).value().to_string();
        let group = form.group.read(cx).value().to_string();
        let auto = *form.auto_refresh.borrow();
        window.close_all_dialogs(cx);
        self.open_subscriptions_dialog_restored(&label, &url, &group, auto, window, cx);
    }

    /// Ticket 04: subscription manager dialog — existing subscriptions with
    /// per-row Refresh / Auto / Remove plus an add form committed by OK.
    /// An empty add form just closes the dialog.
    /// Ticket 26: user entry always starts blank; refresh-driven reopens go
    /// through [`AppView::open_subscriptions_dialog_restored`] (carries
    /// in-progress text).
    fn open_subscriptions_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_subscriptions_dialog_restored("", "", DEFAULT_GROUP, true, window, cx);
    }

    /// Ticket 26: subscriptions dialog with a prefilled add form (the
    /// refresh-driven reopen path). The form is stashed on the view so the
    /// next genuine reopen can carry the text forward again; any OK-dismiss
    /// (empty close or committed add) clears the stash, and Cancel/close
    /// leftovers are dropped by
    /// [`AppView::reopen_subscriptions_if_open`] when no dialog is open.
    #[allow(clippy::too_many_arguments)]
    fn open_subscriptions_dialog_restored(
        &mut self,
        label: &str,
        url: &str,
        group: &str,
        auto: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = SubForm::restored(window, cx, label, url, group, auto);
        self.subs_form = Some(form.clone());
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
        // Ticket 18: Rc-shared with the footer OK button (mouse commits
        // identically to the Confirm key path; an empty add form just closes).
        // `commit_view` is a clone: the builder below still needs `view` for
        // the per-row Refresh / Auto / Remove buttons.
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let label = reader.label.read(cx).value().to_string();
            let url = reader.url.read(cx).value().to_string();
            let group = reader.group.read(cx).value().to_string();
            let auto = *reader.auto_refresh.borrow();
            if label.trim().is_empty() && url.trim().is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.commit_subscription_add(label, url, group, auto, window, cx)
            })
        });
        let commit_key = commit.clone();
        // Ticket 26: every dismiss path (OK-close, Cancel, Escape,
        // backdrop/X) clears the stashed form through `.on_close`, so a
        // later foreign dialog can never be mistaken for this one.
        let close_view = view.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let view_for_rows = view.clone();
            let close_view = close_view.clone();
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
            body = body.child(form.clone().element(cx));
            dialog
                .title("Subscriptions")
                .child(body)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .on_close(move |_, _, cx| {
                    close_view.update(cx, |view, _| {
                        view.subs_form = None;
                    });
                })
                .footer(dialog_footer(
                    "subscriptions-dialog-ok",
                    "subscriptions-dialog-cancel",
                    commit_click,
                ))
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

    /// Ticket 31: edit a subscription's label/url/group. The fields open
    /// prefilled (no auto-refresh toggle here — the commit writes only
    /// label/url/group, so no control may imply otherwise). The plaintext
    /// warning tracks the live URL input. Commits through
    /// [`AppView::commit_subscription_edit`], which reopens the manager on
    /// success. Cancel closes this dialog (stacked above the manager, which
    /// stays intact underneath).
    fn open_subscription_edit_dialog(
        &mut self,
        sub: ProxySubscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Label")
                .default_value(sub.label.clone())
        });
        let url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://…")
                .default_value(sub.url.clone())
        });
        let group_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Group")
                .default_value(sub.group.clone())
        });
        let view = cx.entity();
        let id = sub.id.clone();
        let title = format!("Edit {}", sub.label);
        let read_label = label_state.clone();
        let read_url = url_state.clone();
        let read_group = group_state.clone();
        // Ticket 18: Rc-shared with the footer OK button (mouse commits
        // identically to the Confirm key path, including stay-open-on-error).
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let label = read_label.read(cx).value().to_string();
            let url = read_url.read(cx).value().to_string();
            let group = read_group.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_subscription_edit(&id, label, url, group, window, cx)
            })
        });
        let commit_key = commit.clone();
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let mut col = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(dialog_field(
                    "field-sub-edit-label",
                    "Label",
                    label_state.clone(),
                ))
                .child(dialog_field(
                    "field-sub-edit-url",
                    "Subscription URL",
                    url_state.clone(),
                ))
                .child(dialog_field(
                    "field-sub-edit-group",
                    "Group",
                    group_state.clone(),
                ));
            // Ticket 31: live plaintext warning for the URL being typed.
            let url_now = url_state.read(cx).value().to_string();
            if let Some(warning) = http_subscription_warning(&url_now) {
                col = col.child(
                    div()
                        .id("sub-edit-http-warning")
                        .test_support()
                        .child(warning),
                );
            }
            dialog
                .title(title.clone())
                .child(col)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "subscription-edit-dialog-ok",
                    "subscription-edit-dialog-cancel",
                    commit_click,
                ))
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
    /// Ticket 26: window-less by necessity (ticks and startup have no window
    /// to notify through), so a database error yields no due subs — the pass
    /// starts nothing instead of acting on a lie (ticket-14 precedent).
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
    /// Ticket 30: windowless ticks stay status-only by necessity (no window
    /// exists to notify through — ticket-14 precedent); windowed callers use
    /// [`AppView::tick_engine_poll_notify`] for the error notification.
    fn tick_engine_poll(&mut self, cx: &mut Context<Self>) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }
        let cause = self.poll_engine();
        self.reload_quiet(cx);
        cause
    }

    /// Ticket 30: windowed engine-crash tick — polls the joined per-shard
    /// detail, reloads, and pushes the error notification naming shard/cause
    /// with the View-logs path. Returns the detail when a crash was reported.
    /// Test-only seam: the live tick (`tick_engine_poll`) is windowless by
    /// necessity, so production notifies via `reload` → `notify_engine_crash`.
    #[cfg(test)]
    fn tick_engine_poll_notify(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }
        let detail = self.poll_engine_detail();
        self.reload_quiet(cx);
        if let Some(detail) = detail {
            window.push_notification(Notification::error(crash_notification_text(&detail)), cx);
            return Some(detail);
        }
        None
    }

    /// One due-subscription pass without user clicks: starts a refresh only
    /// for already-due subscriptions (never beyond them, never enabling or
    /// starting configs). Returns the number of refreshes started. Stopped
    /// views start nothing.
    /// Ticket 17: the pass opens one coalescing batch, so N completions
    /// flush at most one toast instead of N.
    /// Ticket 26: returns the number of fetches actually started (a sub with
    /// a fetch already in flight is skipped, never double-fetched).
    fn tick_due_subscriptions(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let due = self.due_subscriptions();
        let ids: Vec<String> = due.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in due {
            if self.start_subscription_refresh(sub, cx) {
                started += 1;
            }
        }
        started
    }

    /// One re-probe of running rows only, without user clicks: silent (no
    /// notification), recorded + persisted like a manual test. Idle views
    /// (nothing running) do nothing: no network traffic, no errors.
    /// Stopped views probe nothing.
    /// Ticket 26: off-thread via [`AppView::spawn_probe`]. Returns the number
    /// of probes started (outcomes land on completion).
    fn tick_reprobe_running(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let running = self.engine.running_ids();
        let Ok(rows) = self.store.list() else {
            return 0;
        };
        let rows: Vec<ProxyConfig> = rows
            .into_iter()
            .filter(|row| running.contains(&row.id))
            .collect();
        if rows.is_empty() {
            return 0;
        }
        self.spawn_probe(rows, false, false, None, cx)
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
        let switch_id = format!("enabled-{}", config.id);
        let switch_view = cx.entity();
        let switch_id_for_handler = config.id.clone();
        // Ticket 19: complete detail pane — endpoint URLs (bind + port only,
        // never params or secrets) plus the upstream host:port when known.
        // Ticket 30: the header carries the same serving badge as the list
        // row (`● Running :port · health` / `○ Stopped`); the enabled flag
        // stays secondary (Switch only — the duplicate Enable button is
        // gone). The Start button reads `Starting…` while a start is in
        // flight (second clicks are ignored, never double-applied).
        let (socks_url, http_url) = proxy_urls(&config.bind_ip, config.port);
        let upstream = upstream_host_port(&config).unwrap_or_else(|| "no upstream".to_string());
        let is_running = self.engine.is_running(&config.id);
        let badge = if is_running {
            let summary = self.health_store.row_summary(&config.id);
            let summary = summary.trim();
            serving_badge(
                true,
                config.port,
                if summary.is_empty() {
                    None
                } else {
                    Some(summary)
                },
            )
        } else {
            serving_badge(false, config.port, None)
        };
        let start_label = if self.start_in_flight {
            "Starting…"
        } else {
            "Start"
        };
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
                    ))
                    .child(div().id("detail-serving").test_support().child(badge)),
            )
            .child(
                div()
                    .id("config-endpoints")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(format!("upstream {upstream}"))
                    .child(format!("{socks_url} · {http_url}")),
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
                                    // Ticket 26: one flag semantic — the
                                    // switch writes the stored flag only (see
                                    // [`AppView::commit_enabled_flag`]);
                                    // starting/stopping stays on Start/Stop.
                                    // Ticket 30: the Switch is the only
                                    // enabled control (no duplicate button).
                                    view.commit_enabled_flag(&id, enabled, window, cx);
                                });
                            }),
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
                            .label(start_label)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.start_selected_engine(window, cx);
                            })),
                    )
                    .child(
                        Button::new("engine-stop")
                            .label("Stop (Shift-F5)")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.stop_selected_engine(window, cx);
                            })),
                    )
                    .child(
                        Button::new("health-test")
                            .label("Test (Ctrl-T)")
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

    /// Ticket 30: `N of M` search count above the list (visible while a
    /// query is active). Includes a Clear-search button that resets the
    /// filter in place. Empty when no query is active.
    /// Ticket 31 deviation (mechanical repair — the helper lives on the
    /// delegate, not on `ListState`): route through `.delegate()`.
    fn render_search_count(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (visible, total, query) = self
            .libraries
            .read(cx)
            .delegate()
            .delegate_counts_for_search();
        if query.trim().is_empty() {
            return div().id("search-count").test_support().into_any_element();
        }
        let clear_view = cx.entity();
        div()
            .id("search-count")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(search_count_text(visible, total))
            .child(Button::new("clear-search").label("Clear search").on_click(
                move |_, window, cx| {
                    clear_view.update(cx, |view, cx| {
                        view.clear_library_search(window, cx);
                    });
                },
            ))
            .into_any_element()
    }

    /// Ticket 30: clear the library search filter in place (selection
    /// re-derives by id via `apply_filter`).
    fn clear_library_search(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.libraries.update(cx, |state, list_cx| {
            let delegate = state.delegate_mut();
            delegate.query = String::new();
            delegate.apply_filter();
            list_cx.notify();
        });
        cx.notify();
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
            .key_context(APP_KEY_CONTEXT)
            // Ticket 26: app shortcuts (Add/Import/Start/Stop/Test). Each
            // handler no-ops while any dialog is open, so shortcut keys
            // never steal dialog input.
            .on_action(cx.listener(|view, _: &AddConfig, window, cx| {
                if window.has_active_dialog(cx) {
                    return;
                }
                view.open_add_dialog(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ImportConfigs, window, cx| {
                if window.has_active_dialog(cx) {
                    return;
                }
                view.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
            }))
            .on_action(cx.listener(|view, _: &StartSelected, window, cx| {
                if window.has_active_dialog(cx) {
                    return;
                }
                view.start_selected_engine(window, cx);
            }))
            .on_action(cx.listener(|view, _: &StopSelected, window, cx| {
                if window.has_active_dialog(cx) {
                    return;
                }
                view.stop_selected_engine(window, cx);
            }))
            .on_action(cx.listener(|view, _: &TestSelected, window, cx| {
                if window.has_active_dialog(cx) {
                    return;
                }
                view.test_selected_health(window, cx);
            }))
            .on_drop(move |paths: &ExternalPaths, window, cx| {
                drop_view.update(cx, |view, cx| {
                    view.import_dropped_files(paths, window, cx);
                });
            })
            .child(TitleBar::new().child(APP_TITLE))
            .child(
                div()
                    .id("manyproxy-toolbar")
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_2()
                    // Ticket 33: shortcut hints ride in the labels (F5/Shift-F5/
                    // Ctrl-T/Ctrl-N/Ctrl-O) so the bindings in [`bind_app_keys`]
                    // are visible without opening About. Button ids are
                    // unchanged.
                    .child(
                        Button::new("add-config")
                            .label("Add (Ctrl-N)")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_add_dialog(window, cx);
                            })),
                    )
                    .child(
                        Button::new("import-config")
                            .label("Import (Ctrl-O)")
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
                    .child(
                        Button::new("engine-start-all")
                            .label("Start all (F5)")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.start_all_engines(window, cx);
                            })),
                    )
                    .child(
                        Button::new("engine-stop-all")
                            .label("Stop all (Shift-F5)")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.stop_all_engines(window, cx);
                            })),
                    )
                    .child(
                        Button::new("health-test-all")
                            .label("Test health (Ctrl-T)")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.test_all_health(window, cx);
                            })),
                    )
                    .child(
                        Button::new("about-dialog")
                            .label("About")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.open_about_dialog(window, cx);
                            })),
                    ),
            )
            // Ticket 17: visible drop affordance (the shell has no
            // hover-during-drag event, so the hint stays visible instead of
            // appearing only on hover).
            .child(
                div()
                    .id("drop-hint")
                    .test_support()
                    .px_2()
                    .child("Drop .conf / .json / .txt files here to import"),
            )
            .child(
                div().flex().flex_col().flex_1().child(
                    div()
                        .id("config-list")
                        .test_support()
                        .flex_1()
                        .child(self.render_search_count(cx))
                        .child(
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
        // Ticket 26: app keyboard shortcuts (Add/Import/Start/Stop/Test).
        bind_app_keys(cx);
        cx.spawn(async move |cx| {
            cx.open_window(app_window_options(), |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                view.update(cx, |view, cx| {
                    view.refresh_due_on_startup(cx);
                    view.start_background_ticks(cx);
                });
                // Ticket 33: never silently kill running endpoints on window
                // close. The hook vetoes the close while endpoints run and
                // opens a confirm naming them; OK stops everything and closes
                // via `remove_window` (the confirmed flag lets that second
                // close through). No tray/minimize in v1 (kit-only, no new
                // deps) — see PACKAGING-NOTES.md.
                let weak = view.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    let Some(view) = weak.upgrade() else {
                        return true;
                    };
                    match view.read_with(cx, |view, _| view.close_veto_text()) {
                        None => true,
                        Some(description) => {
                            let confirm_view = view.clone();
                            window.open_alert_dialog(cx, move |alert, _, _| {
                                let confirm_view = confirm_view.clone();
                                let description = description.clone();
                                alert
                                    .confirm()
                                    .title("Close ManyProxy?")
                                    .description(description)
                                    .on_ok(move |_, window, cx| {
                                        confirm_view.update(cx, |view, _| {
                                            view.confirm_close_stop();
                                        });
                                        window.remove_window();
                                        true
                                    })
                            });
                            false
                        }
                    }
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

    /// Ticket 21: the branded window options carry the OS title (the
    /// `TitleBar::window_options` base leaves `TitlebarOptions::title` as
    /// `None`, which renders as untitled).
    #[test]
    fn window_options_carry_app_title() {
        let options = crate::app_window_options();
        let title = options
            .titlebar
            .expect("titlebar options")
            .title
            .expect("window title");
        assert_eq!(title.as_str(), crate::APP_TITLE);
    }

    /// Ticket 21: the exe icon asset is committed in the repo (no external
    /// fetch at build) and is a well-formed ICO.
    #[test]
    fn exe_icon_asset_is_valid_ico() {
        let bytes = std::fs::read("assets/icon.ico").expect("assets/icon.ico is committed");
        assert!(bytes.len() > 6, "non-trivial icon file");
        assert_eq!(&bytes[0..4], &[0, 0, 1, 0], "ICO magic");
        let count = u16::from_le_bytes([bytes[4], bytes[5]]);
        assert!(count >= 1, "at least one icon image");
    }

    /// Ticket 33: close-confirm names every running row (never a blind
    /// kill); nothing running means no veto text.
    #[test]
    fn close_confirm_names_running_rows() {
        assert_eq!(crate::close_confirm_description(&[]), None);
        let text = crate::close_confirm_description(&["alpha".to_string(), "beta".to_string()])
            .expect("veto text while running");
        assert!(text.contains("\"alpha\""), "names rows, got {text}");
        assert!(text.contains("\"beta\""), "names rows, got {text}");
        assert!(text.contains('2'), "counts rows, got {text}");
    }

    /// Ticket 33: About carries version + git hash + sidecar presence +
    /// data paths + the shortcut list (and the no-autostart contract).
    #[test]
    fn about_body_carries_version_sidecars_paths_shortcuts() {
        let body = crate::about_body(
            "0.1.0",
            "abc123",
            &["sing-box.exe: found (app dir)".to_string()],
            "C:\\data\\library.db",
            "C:\\data\\manyproxy-export.json",
        );
        for needle in [
            "0.1.0",
            "abc123",
            "sing-box.exe",
            "library.db",
            "manyproxy-export.json",
            "F5",
            "Shift-F5",
            "Ctrl-T",
            "Ctrl-N",
            "Ctrl-O",
            "No autostart",
        ] {
            assert!(body.contains(needle), "about names {needle}");
        }
        assert_eq!(
            crate::sidecar_presence_line("x.exe", false),
            "x.exe: missing (place it next to the app exe)"
        );
    }

    /// Ticket 34: the Logs dialog strips sidecar ANSI colors at render
    /// (live-proven: sing-box `[31m`/`[38;5;…` escapes reached the dialog);
    /// stored records stay verbatim.
    #[gpui_kit::test]
    fn logs_dialog_strips_sidecar_ansi(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.logs.push_raw(
                    "\u{1b}[31mERROR\u{1b}[0m[0033] dial tcp 127.0.0.1:8388: refused",
                    &[],
                );
            });
        });
        let stored = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.logs
                    .all()
                    .iter()
                    .map(|r| r.message.clone())
                    .collect::<Vec<_>>()
            })
        });
        assert_eq!(stored.len(), 1);
        assert!(
            stored[0].contains('\u{1b}'),
            "stored record stays verbatim, got {:?}",
            stored[0]
        );
        let lines = cx.update(|_, cx| view.update(cx, |view, cx| view.log_lines(cx)));
        assert_eq!(lines.len(), 1);
        assert!(
            !lines[0].contains('\u{1b}'),
            "rendered line has no escapes, got {:?}",
            lines[0]
        );
        assert!(lines[0].contains("ERROR"), "keeps text, got {:?}", lines[0]);
        assert!(
            lines[0].contains("refused"),
            "keeps text, got {:?}",
            lines[0]
        );
    }

    /// Ticket 33: the close veto passes with nothing running, and the
    /// confirmed path marks the close confirmed (headless: in-memory store,
    /// no sidecars — `stop_all` on an idle engine is a no-op).
    #[gpui_kit::test]
    fn close_veto_passes_when_nothing_running(cx: &mut TestAppContext) {
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

        assert_eq!(view.read_with(cx, |view, _| view.close_veto_text()), None);
        cx.update(|_, cx| {
            view.update(cx, |view, _| view.confirm_close_stop());
        });
        assert!(view.read_with(cx, |view, _| view.close_confirmed));
        assert_eq!(view.read_with(cx, |view, _| view.close_veto_text()), None);
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
        // Ticket 26: probes complete off-thread — pump the completion.
        cx.run_until_parked();
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
        // Ticket 26: probes complete off-thread — pump the completion.
        cx.run_until_parked();
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
        // Ticket 26: probes complete off-thread — pump the completion.
        cx.run_until_parked();

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
                view.commit_subscription_auto(&sub_id, window, cx)
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

    // -- Ticket 17: UX feedback across library flows -------------------------

    /// Scenario: add/edit/remove → success toast naming the row (content at
    /// the pure seam — notification text is framework-private, so delivery
    /// is asserted by count in `crud_commits_toast_once_naming_row`).
    #[test]
    fn crud_toast_messages_name_the_row() {
        use crate::row_action_message;
        assert_eq!(row_action_message("Added", "Work"), "Added \"Work\"");
        assert_eq!(row_action_message("Updated", "Work"), "Updated \"Work\"");
        assert_eq!(row_action_message("Removed", "Work"), "Removed \"Work\"");
    }

    /// Scenario: add/edit/remove commits each fire exactly one toast.
    #[gpui_kit::test]
    fn crud_commits_toast_once_naming_row(cx: &mut TestAppContext) {
        use crate::store::ConfigPatch;
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let id = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.commit_add(seed_input("Work"), window, cx));
                view.store.list().expect("list works").remove(0).id
            })
        });
        assert_eq!(notification_count(cx), 1, "add toasts once");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.commit_edit(
                    &id,
                    ConfigPatch {
                        name: Some("Cabin".to_string()),
                        ..Default::default()
                    },
                    window,
                    cx,
                ));
            });
        });
        assert_eq!(notification_count(cx), 2, "edit toasts once");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.commit_remove(&id, window, cx));
        });
        assert_eq!(notification_count(cx), 3, "remove toasts once");
        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);
    }

    /// Scenario: import dialog with pasted text → live `X valid · Y invalid`
    /// preview before commit (pure counts + message).
    #[test]
    fn import_preview_counts_and_message() {
        use crate::{import_preview_counts, import_preview_message};
        let body = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x";
        assert_eq!(import_preview_counts(body, None), (1, 1));
        assert_eq!(import_preview_message(1, 1), "1 valid · 1 invalid");
        assert_eq!(import_preview_message(0, 0), "0 valid · 0 invalid");
        // A dropped conf keeps its hint for the preview too.
        let conf = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        assert_eq!(import_preview_counts(conf, Some("home.conf")), (1, 0));
    }

    /// Scenario: import dialog shows the preview line and the dropped file
    /// names after a drop.
    #[gpui_kit::test]
    fn import_dialog_shows_preview_and_files(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let a = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_import_dialog_with_files(
                    a,
                    "default",
                    None,
                    Some(vec![("a.conf".to_string(), a.to_string())]),
                    window,
                    cx,
                );
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
        assert!(cx.update(|window, _| window.try_find("import-preview").is_some()));
        assert!(cx.update(|window, _| window.try_find("import-files").is_some()));
    }

    /// Scenario: drag files over shell → visible drop hint.
    #[gpui_kit::test]
    fn shell_renders_drop_hint(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_, cx) = mount_view(store, cx);
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(cx.update(|window, _| window.try_find("drop-hint").is_some()));
    }

    /// Scenario: export dialog → row counts + destination path + overwrite
    /// warning when the file exists (pure description).
    #[test]
    fn export_preview_shows_counts_path_and_overwrite() {
        use crate::export_preview_description;
        let fresh = export_preview_description(3, 3, "/tmp/x.json", false);
        assert!(fresh.contains('3'), "count in: {fresh}");
        assert!(fresh.contains("/tmp/x.json"), "path in: {fresh}");
        assert!(!fresh.contains("overwrit"), "no warning for fresh: {fresh}");
        let both = export_preview_description(4, 3, "/tmp/x.json", false);
        assert!(
            both.contains('4') && both.contains('3'),
            "both numbers: {both}"
        );
        let exists = export_preview_description(3, 3, "/tmp/x.json", true);
        assert!(
            exists.contains("/tmp/x.json") && exists.contains("overwrit"),
            "overwrite warning names the path: {exists}"
        );
    }

    /// Scenario: subscription refresh with item failures → failures viewable
    /// per subscription (stored vec rendering, empty when clean).
    #[test]
    fn subscription_failures_text_renders_items() {
        use crate::import::{ImportErrorKind, ImportFailure};
        use crate::subscription_failures_text;
        assert_eq!(
            subscription_failures_text(&[]),
            "",
            "clean subs show no failures line"
        );
        let failures = vec![ImportFailure {
            index: 1,
            label: "bogus://x".to_string(),
            reason: "unknown-scheme: unknown scheme 'bogus'".to_string(),
            code: ImportErrorKind::UnknownScheme,
        }];
        let text = subscription_failures_text(&failures);
        assert!(text.contains("bogus://x"), "label in: {text}");
        assert!(text.contains("unknown-scheme"), "reason in: {text}");
    }

    /// Scenario: subscriptions dialog shows the auto-refresh checkbox and
    /// the per-subscription failures after a partial refresh.
    #[gpui_kit::test]
    fn subscriptions_dialog_shows_auto_and_failures(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "travel", true)
            .expect("seed works");
        store
            .refresh_with_body(
                &sub.id,
                "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x",
                100,
            )
            .expect("refresh works");
        let sub_id = sub.id.clone();
        let (view, cx) = mount_view(store, cx);
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
        assert!(cx.update(|window, _| window.try_find("field-sub-auto").is_some()));
        assert!(
            cx.update(move |window, _| window.try_find(format!("sub-failures-{sub_id}")).is_some()),
            "stored failures render per subscription"
        );
    }

    /// Scenario: subscription add form → auto-refresh checkbox present and
    /// default on.
    #[gpui_kit::test]
    fn sub_add_form_auto_defaults_on(cx: &mut TestAppContext) {
        use crate::store::DEFAULT_GROUP;
        use crate::SubForm;
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let auto = cx.update(|window, cx| {
            view.update(cx, |_, cx| {
                SubForm::restored(window, cx, "", "", DEFAULT_GROUP, true)
                    .auto_refresh
                    .clone()
            })
        });
        assert!(*auto.borrow(), "auto-refresh defaults on");
    }

    /// Scenario: background tick refreshes 3 subs → at most one coalesced
    /// toast, not three (pure message + headless batch).
    #[test]
    fn coalesced_refresh_message_single_toast() {
        use crate::coalesced_refresh_message;
        assert_eq!(coalesced_refresh_message(0, 0, 0, 0, 0, 0), None);
        let (warn, text) = coalesced_refresh_message(3, 5, 1, 0, 0, 0).expect("batch reports");
        assert!(!warn);
        assert!(
            text.contains('3') && text.contains("+5"),
            "one line: {text}"
        );
        let (warn, text) = coalesced_refresh_message(3, 5, 1, 0, 2, 0).expect("batch reports");
        assert!(warn, "failures warn: {text}");
        assert!(text.contains('2'), "failed count in: {text}");
        // Ticket 26: preserved user edits surface in the coalesced copy and
        // warn (a "clean" sync that silently kept yours would lie).
        let (warn, text) = coalesced_refresh_message(3, 5, 1, 0, 0, 2).expect("batch reports");
        assert!(warn, "conflicts warn: {text}");
        assert!(
            text.contains('2') && text.contains("conflict"),
            "conflict count + copy in: {text}"
        );
    }

    /// Scenario: 3 background completions flush exactly one toast.
    #[gpui_kit::test]
    fn background_batch_coalesces_to_one_toast(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut ids = Vec::new();
        for (label, group) in [("One", "g1"), ("Two", "g2"), ("Three", "g3")] {
            let sub = store
                .add_subscription(label, "https://example.com/sub.txt", group, true)
                .expect("seed works");
            ids.push((sub.id.clone(), sub.label.clone()));
        }
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let keys: Vec<String> = ids.iter().map(|(id, _)| id.clone()).collect();
                view.begin_refresh_batch(&keys);
            });
        });
        for (id, label) in &ids {
            let id = id.clone();
            let label = label.clone();
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_subscription_refresh(
                        &id,
                        &label,
                        Ok(
                            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                                .to_string(),
                        ),
                        999,
                        window,
                        cx,
                    );
                });
            });
        }
        assert_eq!(notification_count(cx), 1, "three refreshes, one toast");
    }

    /// Scenario: Start all on empty library / Stop on stopped row → honest
    /// messages, never "Started 0" (pure copy).
    #[test]
    fn honest_start_stop_messages() {
        use crate::{already_stopped_message, start_all_message, stop_all_message};
        let empty = start_all_message(0);
        assert!(empty.contains("Nothing to start"), "honest: {empty}");
        assert!(!empty.contains("Started 0"), "never Started 0: {empty}");
        assert!(start_all_message(3).contains('3'));
        let idle = stop_all_message(0);
        assert!(idle.contains("Nothing to stop"), "honest: {idle}");
        assert_eq!(stop_all_message(2), "Stopped all endpoints");
        assert!(already_stopped_message("Work").contains("Work"));
        assert!(already_stopped_message("Work").contains("already stopped"));
    }

    /// Scenario: honest empty start-all and stop-on-stopped toasts headless.
    #[gpui_kit::test]
    fn empty_start_and_stopped_stop_warn_honestly(cx: &mut TestAppContext) {
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        // Empty-library arm first: remove the seed so start-all is honest.
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let rows = view.store.list().expect("list works");
                for row in &rows {
                    view.store.remove(&row.id).expect("remove works");
                }
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        assert_eq!(notification_count(cx), 1, "empty start warns once");
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            0,
            "nothing starts"
        );

        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.add(seed_input("Work")).expect("seed works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.reload(window, cx));
        });
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.stop_selected_engine(window, cx));
        });
        assert_eq!(notification_count(cx), 2, "stop-on-stopped warns once");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.stop_all_engines(window, cx));
        });
        assert_eq!(notification_count(cx), 3, "idle stop-all warns once");
    }

    /// Scenario: search matches host/bind/port as well as
    /// name/protocol/group (pure matcher + delegate filter).
    #[test]
    fn library_search_matches_host_bind_port() {
        use crate::store::ProxyConfig;
        use crate::{library_row_matches, LibraryDelegate};
        fn row(name: &str, server: &str, bind: &str, port: u16) -> ProxyConfig {
            ProxyConfig {
                id: format!("id-{name}"),
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: format!(
                    "{{\"type\":\"vless\",\"server\":\"{server}\",\"server_port\":443}}"
                ),
                group: "default".to_string(),
                enabled: false,
                bind_ip: bind.to_string(),
                port,
                subscription_id: None,
                identity_key: None,
            }
        }
        let alpha = row("Alpha", "a.example.com", "127.0.0.1", 10801);
        let beta = row("Beta", "10.0.0.5", "10.0.0.5", 10802);
        assert!(library_row_matches(&beta, "10.0.0.5"), "bind/host match");
        assert!(library_row_matches(&alpha, "10801"), "port match");
        assert!(!library_row_matches(&beta, "a.example.com"));
        assert!(library_row_matches(&alpha, "a.example.com"), "host match");
        assert!(library_row_matches(&alpha, "alpha"), "name still matches");
        assert!(
            library_row_matches(&alpha, "vless"),
            "protocol still matches"
        );
        assert!(!library_row_matches(&alpha, "nope"), "no false positive");

        let mut delegate = LibraryDelegate::new();
        delegate.set_rows(vec![alpha, beta]);
        delegate.query = "10.0.0.5".to_string();
        delegate.apply_filter();
        assert_eq!(
            delegate.visible.len(),
            1,
            "filter narrows to the bind/host row"
        );
        delegate.query = "10801".to_string();
        delegate.apply_filter();
        assert_eq!(delegate.visible.len(), 1, "filter narrows by port");
        delegate.query = String::new();
        delegate.apply_filter();
        assert_eq!(delegate.visible.len(), 2, "empty query shows all");
    }

    /// Scenario: add/edit with a non-loopback bind → inline hint that
    /// starting will need the LAN confirm.
    #[test]
    fn lan_hint_for_bind() {
        use crate::lan_hint_for_bind;
        assert_eq!(lan_hint_for_bind("127.0.0.1"), None);
        assert_eq!(lan_hint_for_bind(""), None, "blank means loopback default");
        assert_eq!(lan_hint_for_bind("  "), None);
        let hint = lan_hint_for_bind("0.0.0.0").expect("LAN binds hint");
        assert!(hint.contains("0.0.0.0"), "names the bind: {hint}");
        assert!(hint.contains("LAN confirm"), "names the gate: {hint}");
    }

    /// Scenario: add dialog with a LAN bind shows the inline hint; the
    /// loopback add dialog shows none.
    #[gpui_kit::test]
    fn add_dialog_shows_lan_hint_only_for_lan_bind(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let lan = store
            .add(NewConfig {
                name: "Lan".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: "0.0.0.0".to_string(),
                port: String::new(),
            })
            .expect("seed works");
        let loopback = store.add(seed_input("Loop")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_edit_dialog(lan, window, cx));
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
        assert!(cx.update(|window, _| window.try_find("lan-hint").is_some()));

        cx.update(|window, cx| window.close_all_dialogs(cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_edit_dialog(loopback, window, cx));
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
        assert!(
            cx.update(|window, _| window.try_find("lan-hint").is_none()),
            "loopback binds show no LAN hint"
        );
    }

    /// Scenario: dropped file names render as basenames (never full paths).
    #[test]
    fn import_files_label_basenames() {
        use crate::import_files_label;
        assert_eq!(import_files_label(None, None), None);
        assert_eq!(
            import_files_label(Some("C:\\drops\\home.conf"), None),
            Some("File: home.conf".to_string())
        );
        assert_eq!(
            import_files_label(
                None,
                Some(&[
                    ("/tmp/a.conf".to_string(), "x".to_string()),
                    ("b.json".to_string(), "y".to_string())
                ])
            ),
            Some("Files: a.conf, b.json".to_string())
        );
    }

    // -- Ticket 18: dialog footers (mouse OK/Cancel on every plain dialog) ---

    /// Paint the freshly opened dialog to rest (same two-frame rhythm the
    /// existing dialog tests use) so footer buttons are visible + clickable.
    /// Callers set reduce-motion first (see below): without it the dialog's
    /// 250ms slide-down entrance can still be in flight under parallel-test
    /// load, moving the button between the click's down/up frames and
    /// flaking the hit.
    fn paint_dialog_open(cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    }

    /// Scenario: import dialog with valid text → click OK → rows created,
    /// dialog closes, one success toast. This is the previously-missing mouse
    /// path end to end (plain `Dialog` paints no footer buttons on its own).
    #[gpui_kit::test]
    fn dialog_footer_ok_commits_import_end_to_end(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        // Settle the dialog entrance animation on its first frame so the
        // click's down/up frames share one button position (framework's own
        // dialog tests do the same).
        cx.update(|cx| cx.set_reduce_motion(true));
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_import_dialog(
                    "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one",
                    "travel",
                    None,
                    window,
                    cx,
                );
            });
        });
        paint_dialog_open(cx);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("import-dialog-ok").is_some()));
        cx.update(|window, cx| {
            window.click("import-dialog-ok", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(
            !cx.update(|window, cx| window.has_active_dialog(cx)),
            "OK click closes the dialog"
        );
        assert_eq!(
            view.read_with(cx, |view, cx| view.config_count(cx)),
            1,
            "one click commits exactly one row (no double-commit)"
        );
        assert_eq!(notification_count(cx), 1, "one click fires one toast");
        let group = view.read_with(cx, |view, _| {
            view.store.list().expect("list works").remove(0).group
        });
        assert_eq!(group, "travel");
    }

    /// Scenario: add dialog → click Cancel → dialog closes, no row created.
    #[gpui_kit::test]
    fn dialog_footer_cancel_closes_add_without_row(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_add_dialog(window, cx));
        });
        paint_dialog_open(cx);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("add-dialog-cancel").is_some()));
        cx.update(|window, cx| {
            window.click("add-dialog-cancel", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(
            !cx.update(|window, cx| window.has_active_dialog(cx)),
            "Cancel closes the dialog"
        );
        assert_eq!(
            view.read_with(cx, |view, cx| view.config_count(cx)),
            0,
            "Cancel creates no row"
        );
        assert_eq!(notification_count(cx), 0, "Cancel fires no toast");
    }

    /// Scenario: every `open_dialog` call site renders a footer with stable
    /// button ids — no dialog relies on the key-only commit path.
    #[gpui_kit::test]
    fn dialog_footers_audit_every_plain_dialog(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let seeded = store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        for (open, ok, cancel) in [
            ("add", "add-dialog-ok", "add-dialog-cancel"),
            ("edit", "edit-dialog-ok", "edit-dialog-cancel"),
            ("import", "import-dialog-ok", "import-dialog-cancel"),
            (
                "subscriptions",
                "subscriptions-dialog-ok",
                "subscriptions-dialog-cancel",
            ),
            ("logs", "logs-dialog-ok", "logs-dialog-cancel"),
        ] {
            let target = seeded.clone();
            cx.update(|window, cx| {
                view.update(cx, |view, cx| match open {
                    "add" => view.open_add_dialog(window, cx),
                    "edit" => view.open_edit_dialog(target, window, cx),
                    "import" => view.open_import_dialog("vless://x", "travel", None, window, cx),
                    "subscriptions" => view.open_subscriptions_dialog(window, cx),
                    _ => view.open_logs_dialog(window, cx),
                });
            });
            paint_dialog_open(cx);
            assert!(
                cx.update(|window, cx| window.has_active_dialog(cx)),
                "{open} dialog opens"
            );
            assert!(
                cx.update(|window, _| window.try_find(ok).is_some()),
                "{open} dialog renders footer OK ({ok})"
            );
            assert!(
                cx.update(|window, _| window.try_find(cancel).is_some()),
                "{open} dialog renders footer Cancel ({cancel})"
            );
            cx.update(|window, cx| window.close_all_dialogs(cx));
            cx.run_until_parked();
        }
    }

    /// Scenario: footer OK with an invalid form behaves exactly like the
    /// Enter path — stays open + error, never a silent close.
    #[gpui_kit::test]
    fn dialog_footer_ok_invalid_import_stays_open(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_import_dialog("bogus://x", "travel", None, window, cx);
            });
        });
        paint_dialog_open(cx);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        cx.update(|window, cx| {
            window.click("import-dialog-ok", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "invalid OK keeps the dialog open (same as Enter)"
        );
        assert_eq!(
            view.read_with(cx, |view, cx| view.config_count(cx)),
            0,
            "nothing stored"
        );
        assert_eq!(notification_count(cx), 1, "one error notice, never silent");
    }

    // -- Ticket 19: library list paints stored rows --------------------------

    /// One completed frame at desktop size after external changes.
    fn paint_frame(cx: &mut VisualTestContext) {
        cx.simulate_resize(size(px(1000.), px(700.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    }

    /// Paint means painted: present in the completed frame, visible, with
    /// non-zero bounds. Element presence alone never proves paint.
    fn assert_painted(cx: &mut VisualTestContext, id: &str) {
        let snapshot = cx.update(|window, _| window.try_find(id.to_owned()));
        let snapshot =
            snapshot.unwrap_or_else(|| panic!("{id} renders: missing element in completed frame"));
        assert!(snapshot.visible(), "{id} is visible");
        let bounds = snapshot.bounds();
        assert!(
            bounds.size.width > px(0.) && bounds.size.height > px(0.),
            "{id} paints with non-zero bounds: {bounds:?}"
        );
    }

    fn assert_absent(cx: &mut VisualTestContext, id: String) {
        assert!(
            cx.update(|window, _| window.try_find(id.clone()).is_none()),
            "{id} is not in the completed frame"
        );
    }

    fn row19_seed(name: &str, host: &str, port: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: format!(
                "{{\"type\":\"vless\",\"server\":\"{host}\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\"}}"
            ),
            group: "travel".to_string(),
            bind_ip: "127.0.0.1".to_string(),
            port: port.to_string(),
        }
    }

    /// Scenario: store with 2 configs, fresh window → 2 painted rows with
    /// stable ids, findable in a completed frame.
    #[gpui_kit::test]
    fn library_paints_two_stored_rows(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(row19_seed("Work", "a.example.com", "10801"))
            .expect("seed works");
        let second = store
            .add(row19_seed("Home", "b.example.com", "10802"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);
        assert_painted(cx, &format!("library-row-{}", first.id));
        assert_painted(cx, &format!("library-row-{}", second.id));
    }

    /// Scenario: empty store → the empty state actually paints (asserted via
    /// bounds/visibility, not just element presence).
    #[gpui_kit::test]
    fn library_paints_empty_state(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_view, cx) = mount_view(store, cx);
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);

        assert_painted(cx, "library-empty");
    }

    /// Scenario: add/remove/search each repaint to match the store with no
    /// restart and no stale rows; selection survives by id; the detail pane
    /// paints endpoint URLs for the selected row.
    #[gpui_kit::test]
    fn library_mutations_repaint_and_keep_selection(cx: &mut TestAppContext) {
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(row19_seed("Work", "a.example.com", "10801"))
            .expect("seed works");
        let second = store
            .add(row19_seed("Home", "b.example.com", "10802"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.run_until_parked();
        paint_frame(cx);

        // Select the first row, then add a third: the list repaints with all
        // three and the selection stays on the first row's id.
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                    list_cx.notify();
                });
            });
        });
        let third_id = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.commit_add(seed_input("Cabin"), window, cx));
                view.store
                    .list()
                    .expect("list works")
                    .into_iter()
                    .find(|row| row.name == "Cabin")
                    .expect("added row stored")
                    .id
            })
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 3);
        assert_painted(cx, &format!("library-row-{}", first.id));
        assert_painted(cx, &format!("library-row-{}", second.id));
        assert_painted(cx, &format!("library-row-{third_id}"));
        assert_eq!(
            view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id)),
            Some(first.id.clone()),
            "selection preserved by id across add"
        );
        assert_painted(cx, "config-endpoints");

        // Remove the third: it leaves the painted frame (no stale row) and
        // the selection still follows the first row's id.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.commit_remove(&third_id, window, cx));
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);
        assert_painted(cx, &format!("library-row-{}", first.id));
        assert_painted(cx, &format!("library-row-{}", second.id));
        assert_absent(cx, format!("library-row-{third_id}"));
        assert_eq!(
            view.read_with(cx, |view, cx| view.selected_config(cx).map(|row| row.id)),
            Some(first.id.clone()),
            "selection preserved by id across remove"
        );

        // Search narrows the painted rows (incremental filter, no rebuild);
        // clearing it repaints everything.
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    delegate.query = "home".to_string();
                    delegate.apply_filter();
                    list_cx.notify();
                });
            });
        });
        cx.run_until_parked();
        paint_frame(cx);
        assert_painted(cx, &format!("library-row-{}", second.id));
        assert_absent(cx, format!("library-row-{}", first.id));

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    delegate.query = String::new();
                    delegate.apply_filter();
                    list_cx.notify();
                });
            });
        });
        cx.run_until_parked();
        paint_frame(cx);
        assert_painted(cx, &format!("library-row-{}", first.id));
        assert_painted(cx, &format!("library-row-{}", second.id));

        // Removing everything paints the empty state (never a blank list
        // with no rows and no empty text).
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_remove(&first.id, window, cx);
                view.commit_remove(&second.id, window, cx);
            });
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);

        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);
        assert_painted(cx, "library-empty");
        assert_eq!(
            view.read_with(cx, |view, cx| view.selected_config(cx)),
            None,
            "removing the selected row clears the selection"
        );
    }

    /// Scenario: a health tick (fake engine + scripted prober) refreshes the
    /// rows in place — painted rows match the store and carry the new health
    /// summaries with no restart.
    #[gpui_kit::test]
    fn library_health_tick_repaints_rows(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(row19_seed("Work", "a.example.com", "10811"))
            .expect("seed works");
        let second = store
            .add(row19_seed("Home", "b.example.com", "10812"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.run_until_parked();
        paint_frame(cx);

        let (prober, handle) = FakeHealthProber::shared();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                view.prober = Box::new(prober);
            });
        });
        handle.script_latency(&first.id, 33);
        handle.script_failure(&second.id, "upstream refused");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        let tested =
            cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
        assert_eq!(tested, 2);
        // Ticket 26: probes complete off-thread — pump the completion.
        cx.run_until_parked();
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);

        assert_painted(cx, &format!("library-row-{}", first.id));
        assert_painted(cx, &format!("library-row-{}", second.id));
        let health = view.read_with(cx, |view, cx| {
            view.libraries.read(cx).delegate().health.clone()
        });
        assert_eq!(
            health.get(&first.id).map(String::as_str),
            Some("33ms"),
            "live row summary refreshed: {health:?}"
        );
        assert_eq!(
            health.get(&second.id).map(String::as_str),
            Some("dead"),
            "dead row summary refreshed: {health:?}"
        );
    }

    /// Row content (pure seam): name, protocol, upstream host:port, bind
    /// endpoint, enabled state, and the health summary are all present; the
    /// health line appears only when a summary exists.
    /// Ticket 31 deviation (mechanical repair — ticket 30 changed the
    /// signature mid-flight without updating this caller): passes the serving
    /// flag through; the badge line is always present, so the count is 4.
    #[test]
    fn library_row_lines_show_complete_content() {
        use crate::library_row_lines;
        use crate::store::ProxyConfig;
        fn row(enabled: bool, params: &str) -> ProxyConfig {
            ProxyConfig {
                id: "7".to_string(),
                name: "Work".to_string(),
                protocol: "vless".to_string(),
                params: params.to_string(),
                group: "travel".to_string(),
                enabled,
                bind_ip: "127.0.0.1".to_string(),
                port: 10801,
                subscription_id: None,
                identity_key: None,
            }
        }
        let full = library_row_lines(
            &row(true, "{\"server\":\"a.example.com\",\"server_port\":443}"),
            Some("87ms"),
            true,
        )
        .join("\n");
        for want in [
            "Work",
            "vless",
            "a.example.com:443",
            "travel",
            "127.0.0.1:10801",
            "enabled",
            "87ms",
        ] {
            assert!(full.contains(want), "{want} visible in:\n{full}");
        }
        let plain = library_row_lines(&row(false, "{}"), None, false).join("\n");
        assert!(
            plain.contains("disabled"),
            "enabled state visible:\n{plain}"
        );
        assert!(
            plain.contains("no upstream"),
            "metadata-only rows say so honestly:\n{plain}"
        );
        assert_eq!(
            library_row_lines(&row(false, "{}"), None, false).len(),
            4,
            "badge line always present (stopped here)"
        );
    }

    /// Upstream extraction reads host/port keys only — credentials in params
    /// can never reach the row text through this seam.
    #[test]
    fn upstream_host_port_reads_only_host_keys() {
        use crate::store::ProxyConfig;
        use crate::upstream_host_port;
        fn row(params: &str) -> ProxyConfig {
            ProxyConfig {
                id: "7".to_string(),
                name: "Work".to_string(),
                protocol: "vless".to_string(),
                params: params.to_string(),
                group: "default".to_string(),
                enabled: false,
                bind_ip: "127.0.0.1".to_string(),
                port: 10801,
                subscription_id: None,
                identity_key: None,
            }
        }
        assert_eq!(
            upstream_host_port(&row("{\"server\":\"a.example.com\",\"server_port\":443,\
                    \"uuid\":\"SECRET-UUID\",\"password\":\"hunter2\"}"))
            .as_deref(),
            Some("a.example.com:443")
        );
        assert_eq!(
            upstream_host_port(&row("{\"address\":\"10.0.0.5\",\"port\":\"51820\"}")).as_deref(),
            Some("10.0.0.5:51820"),
            "address/port aliases work"
        );
        assert_eq!(
            upstream_host_port(&row("{\"server\":\"a.example.com\"}")).as_deref(),
            Some("a.example.com"),
            "host alone still shows"
        );
        assert_eq!(upstream_host_port(&row("{}")), None);
        assert_eq!(upstream_host_port(&row("not json")), None);
        for (params, canary) in [
            (
                "{\"server\":\"a.example.com\",\"server_port\":443,\"password\":\"hunter2\"}",
                "hunter2",
            ),
            (
                "{\"server\":\"a.example.com\",\"server_port\":443,\"uuid\":\"SECRET-UUID\"}",
                "SECRET-UUID",
            ),
            (
                "{\"server\":\"a.example.com\",\"server_port\":443,\"token\":\"abc123\"}",
                "abc123",
            ),
        ] {
            let shown = upstream_host_port(&row(params)).unwrap_or_default();
            assert!(!shown.contains(canary), "no credentials in {shown}");
        }
    }

    // -- Ticket 26: UI-state races + error surfacing -------------------------

    /// Seed with engine-startable params (the fake driver validates like the
    /// real one; bare `seed_input` rows fail the start gate).
    fn t26_engine_seed(name: &str, port: &str) -> NewConfig {
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

    /// Scenario: select A, filter to only C → the selection clears (never
    /// lands on C); a filter that still matches A keeps A (pure delegate).
    #[test]
    fn filter_selection_follows_id_never_wrong_row() {
        use crate::store::ProxyConfig;
        use crate::LibraryDelegate;
        fn row(id: &str, name: &str) -> ProxyConfig {
            ProxyConfig {
                id: id.to_string(),
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: "{}".to_string(),
                group: "default".to_string(),
                enabled: false,
                bind_ip: "127.0.0.1".to_string(),
                port: 10801,
                subscription_id: None,
                identity_key: None,
            }
        }
        use gpui_kit::component::IndexPath;
        let mut delegate = LibraryDelegate::new();
        delegate.set_rows(vec![row("1", "Alpha"), row("2", "Beta"), row("3", "Gamma")]);
        delegate.selected = Some(IndexPath::new(0));
        assert_eq!(
            delegate.selected_row().map(|row| row.id),
            Some("1".to_string())
        );
        // Narrow to only Gamma: Alpha is gone, so the selection clears
        // instead of pointing at Gamma's index.
        delegate.query = "gamma".to_string();
        delegate.apply_filter();
        assert_eq!(
            delegate.selected_row(),
            None,
            "filtered-out selection clears"
        );
        // Clearing the filter restores all rows (selection stays cleared —
        // filtering never invents one).
        delegate.query = String::new();
        delegate.apply_filter();
        assert_eq!(delegate.selected_row(), None);
        // A filter matching every row keeps Alpha by id (not by index).
        delegate.selected = Some(IndexPath::new(0));
        delegate.query = "a".to_string();
        delegate.apply_filter();
        assert_eq!(
            delegate.selected_row().map(|row| row.id),
            Some("1".to_string()),
            "visible selection preserved by id"
        );
    }

    /// Scenario: type in the Add dialog during a background refresh →
    /// the Add dialog and its input stay intact (never hijacked).
    #[gpui_kit::test]
    fn background_refresh_never_hijacks_add_dialog(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_add_dialog(window, cx));
        });
        paint_dialog_open(cx);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.update(|window, _| window.try_find("field-name").is_some()));
        // A background refresh completion arrives mid-typing.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
        });
        cx.run_until_parked();
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "foreign dialog survives the refresh"
        );
        assert!(
            cx.update(|window, _| window.try_find("field-name").is_some()),
            "Add dialog keeps its fields"
        );
        assert!(
            cx.update(|window, _| window.try_find("field-sub-url").is_none()),
            "Subscriptions dialog never opens uninvited"
        );
        assert_eq!(notification_count(cx), 0, "no toast for a non-event");
    }

    /// Scenario: subscriptions dialog open with typed text + background
    /// refresh → the dialog reopens with the text intact, not blank.
    #[gpui_kit::test]
    fn subs_dialog_reopen_preserves_form_text(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_subscriptions_dialog(window, cx));
        });
        paint_dialog_open(cx);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        // Type into the add form (the stashed live entities).
        cx.update(|window, cx| {
            let form = view.read_with(cx, |view, _| view.subs_form.clone());
            let form = form.expect("subs form stashed on open");
            form.url.update(cx, |state, cx| {
                state.set_value("https://example.com/typed.txt", window, cx);
            });
        });
        // Genuine reopen (background completion while subs is open).
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.reopen_subscriptions_if_open(window, cx));
        });
        paint_dialog_open(cx);
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "subs dialog reopens"
        );
        let url = view.read_with(cx, |view, cx| {
            view.subs_form
                .clone()
                .expect("form restashed")
                .url
                .read(cx)
                .value()
                .to_string()
        });
        assert_eq!(url, "https://example.com/typed.txt", "typed text survives");
    }

    /// Scenario: edit opened, refresh changes params underneath, user renames
    /// → commit writes ONLY the rename; the refreshed params survive.
    #[gpui_kit::test]
    fn edit_commit_writes_only_user_changed_fields(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let seeded = store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        // Dialog-open snapshot.
        let snapshot = view.read_with(cx, |view, _| {
            view.store
                .get(&seeded.id)
                .expect("get works")
                .expect("row there")
        });
        // Concurrent refresh lands after the dialog opened.
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                use crate::store::ConfigPatch;
                view.store
                    .update(
                        &seeded.id,
                        ConfigPatch {
                            params: Some("{\"type\":\"vless\",\"server\":\"9.9.9.9\"}".to_string()),
                            ..Default::default()
                        },
                    )
                    .expect("concurrent edit works");
            });
        });
        // User renames only: build the relative patch like the dialog does.
        let renamed = cx.update(|window, cx| {
            view.update(cx, |_view, cx| {
                use crate::ConfigForm;
                let form = ConfigForm::prefilled(window, cx, &snapshot);
                form.name.update(cx, |state, cx| {
                    state.set_value("Renamed", window, cx);
                });
                form.as_patch_vs(cx, &snapshot)
            })
        });
        assert!(renamed.name.is_some(), "touched field writes");
        assert!(renamed.params.is_none(), "untouched field stays None");
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_edit(&seeded.id, renamed, window, cx)
            })
        });
        assert!(closed, "valid edit closes");
        let row = view.read_with(cx, |view, _| {
            view.store
                .get(&seeded.id)
                .expect("get works")
                .expect("row there")
        });
        assert_eq!(row.name, "Renamed");
        assert_eq!(
            row.params, "{\"type\":\"vless\",\"server\":\"9.9.9.9\"}",
            "concurrent refresh survives on the untouched field"
        );
    }

    /// Scenario: row deleted while the edit dialog is open → commit warns
    /// "Already removed" and closes (never edits a ghost).
    #[gpui_kit::test]
    fn edit_commit_missing_row_warns_and_closes(cx: &mut TestAppContext) {
        use crate::store::ConfigPatch;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let seeded = store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.remove(&seeded.id).expect("remove works");
            });
        });
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_edit(&seeded.id, ConfigPatch::default(), window, cx)
            })
        });
        assert!(closed, "ghost edit closes");
        assert_eq!(notification_count(cx), 1, "removal warns once");
    }

    /// Scenario: batch {a,b} open, tick adds {c} mid-flight → union, no
    /// reset, one toast; the generation only bumps on empty → open.
    #[gpui_kit::test]
    fn batch_union_never_resets_open_batch(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut ids = Vec::new();
        for label in ["One", "Two", "Three"] {
            let sub = store
                .add_subscription(label, "https://example.com/sub.txt", "g", true)
                .expect("seed works");
            ids.push((sub.id.clone(), sub.label.clone()));
        }
        let (view, cx) = mount_view(store, cx);
        let gen0 = view.read_with(cx, |view, _| view.refresh_generation);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.begin_refresh_batch(&[ids[0].0.clone(), ids[1].0.clone()]);
            });
        });
        let gen1 = view.read_with(cx, |view, _| view.refresh_generation);
        assert_eq!(gen1, gen0 + 1, "fresh batch bumps the generation");
        // First member completes with churn.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.finish_subscription_refresh(
                    &ids[0].0,
                    &ids[0].1,
                    Ok(
                        "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                            .to_string(),
                    ),
                    999,
                    window,
                    cx,
                );
            });
        });
        assert_eq!(notification_count(cx), 0, "open batch stays silent");
        // A second tick unions its id: no reset, same generation.
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.begin_refresh_batch(&[ids[2].0.clone()]);
            });
        });
        let (gen2, total) = view.read_with(cx, |view, _| {
            (view.refresh_generation, view.refresh_batch_total)
        });
        assert_eq!(gen2, gen1, "union keeps the epoch");
        assert_eq!(total, 3, "union grows the total");
        assert_eq!(
            view.read_with(cx, |view, _| view.refresh_batch_added),
            1,
            "accumulated churn survives the union"
        );
        for (id, label) in ids.iter().skip(1) {
            let id = id.clone();
            let label = label.clone();
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_subscription_refresh(
                        &id,
                        &label,
                        Ok(
                            "vless://11111111-2222-4333-8444-555555555555@b.example.com:443#two"
                                .to_string(),
                        ),
                        999,
                        window,
                        cx,
                    );
                });
            });
        }
        assert_eq!(notification_count(cx), 1, "one batch, one toast");
    }

    /// Scenario: manual refresh while a tick fetch runs → adopted (no second
    /// fetch); the shared completion notifies immediately, once.
    #[gpui_kit::test]
    fn manual_refresh_adopts_inflight_fetch(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.begin_refresh_batch(std::slice::from_ref(&sub_id));
                assert!(view.note_refresh_started(&sub_id), "first fetch starts");
                assert!(!view.note_refresh_started(&sub_id), "second fetch dedupes");
            });
        });
        // Manual click adopts: no new fetch, member leaves the batch.
        let started = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.refresh_subscription_by_id(&sub_id, window, cx)
            })
        });
        assert!(!started, "in-flight fetch is adopted, not restarted");
        // The shared completion notifies immediately (not batched).
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.finish_subscription_refresh(
                    &sub_id,
                    "Nightly",
                    Ok(
                        "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one"
                            .to_string(),
                    ),
                    999,
                    window,
                    cx,
                );
            });
        });
        assert_eq!(
            notification_count(cx),
            1,
            "adopted completion notifies once"
        );
    }

    /// Scenario: auto flag flipped behind the dialog's back → commit toggles
    /// the CURRENT value, not the stale rendered one.
    #[gpui_kit::test]
    fn commit_subscription_auto_rereads_current(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/sub.txt", "g", true)
            .expect("seed works");
        let sub_id = sub.id.clone();
        let (view, cx) = mount_view(store, cx);
        // Someone flips it behind the dialog's back (dialog still shows on).
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store
                    .set_subscription_auto_refresh(&sub_id, false)
                    .expect("flip works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_subscription_auto(&sub_id, window, cx)
            });
        });
        let current = view.read_with(cx, |view, _| {
            view.store
                .get_subscription(&sub_id)
                .expect("get works")
                .expect("sub there")
                .auto_refresh
        });
        assert!(current, "fresh false toggles back to true (not stale on)");
    }

    /// Scenario: corrupt DB → Start all errors honestly, never claims the
    /// library is empty (pure copy + headless delivery).
    #[test]
    fn db_list_error_names_database_never_empty() {
        use crate::db_list_error_message;
        use crate::store::StoreError;
        let text = db_list_error_message(&StoreError::NotFound("7".to_string()));
        assert!(text.contains("Cannot load library"), "names it: {text}");
        assert!(!text.contains("empty"), "never the empty claim: {text}");
    }

    /// Scenario: corrupt DB → Start all errors honestly, never claims the
    /// library is empty (headless delivery: exactly one notice).
    #[gpui_kit::test]
    fn corrupt_db_start_all_errors_never_empty_claim(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.store.break_schema_for_test().expect("break works");
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        assert_eq!(notification_count(cx), 1, "db error notifies once");
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            0,
            "nothing starts on an unreadable library"
        );
    }

    /// Scenario: the enable toast always says flag-only (pure copy).
    #[test]
    fn enabled_flag_message_names_row_and_meaning() {
        use crate::enabled_flag_message;
        let on = enabled_flag_message("Work", true);
        assert!(on.contains("Work"), "names the row: {on}");
        assert!(on.contains("flag only"), "one semantic: {on}");
        assert!(on.contains("Start"), "points at Start: {on}");
        let off = enabled_flag_message("Work", false);
        assert!(off.contains("Work"), "names the row: {off}");
        assert!(off.contains("Stop"), "points at Stop: {off}");
    }

    /// Scenario: a successful toggle toasts the flag meaning (never silent,
    /// never start-shaped).
    #[gpui_kit::test]
    fn toggle_success_toasts_flag_meaning(cx: &mut TestAppContext) {
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
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.toggle_selected_enabled(window, cx));
        });
        assert_eq!(notification_count(cx), 1, "toggle explains itself");
        assert!(
            view.read_with(cx, |view, cx| view
                .selected_config(cx)
                .map(|row| row.enabled)
                .unwrap_or(false)),
            "flag flips"
        );
    }

    /// Scenario: read-only DB → Start toasts the flag failure loudly (the
    /// endpoint runs while the flag lies — never silent).
    #[gpui_kit::test]
    fn start_selected_flag_write_failure_is_loud(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                view.store
                    .set_query_only_for_test()
                    .expect("readonly works");
            });
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
            });
        });
        let id = view.read_with(cx, |view, cx| {
            view.selected_config(cx).expect("selected").id
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
        });
        assert_eq!(notification_count(cx), 1, "flag failure is loud");
    }

    /// Scenario: dropped paths name the unreadable files (pure copy +
    /// headless delivery, basenames only).
    #[test]
    fn unreadable_drop_message_names_basenames() {
        use crate::unreadable_drop_message;
        let text = unreadable_drop_message(&["a.conf".to_string(), "b.txt".to_string()]);
        assert!(text.contains("a.conf") && text.contains("b.txt"), "{text}");
        assert!(!text.contains("Users"), "never full paths: {text}");
    }

    /// Scenario: dropping an unreadable path toasts its name, stores nothing.
    #[gpui_kit::test]
    fn import_drop_names_unreadable_files(cx: &mut TestAppContext) {
        use gpui_kit::ExternalPaths;
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let paths = ExternalPaths(
            vec![std::path::PathBuf::from(
                "E:\\definitely-not-here\\ghost.conf",
            )]
            .into(),
        );
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.import_dropped_files(&paths, window, cx));
        });
        assert_eq!(notification_count(cx), 1, "unreadable drop names itself");
        assert_eq!(
            view.read_with(cx, |view, cx| view.config_count(cx)),
            0,
            "nothing stored"
        );
    }

    /// Scenario: Ctrl+N opens Add; a second Ctrl+N with the dialog open is a
    /// no-op (never steals dialog keys, never stacks dialogs).
    #[gpui_kit::test]
    fn keyboard_shortcuts_open_and_guard_dialogs(cx: &mut TestAppContext) {
        use gpui_kit::Keystroke;
        cx.update(gpui_kit::init);
        cx.update(crate::bind_app_keys);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        // Focus the shell so the library key context matches.
        let focus = view.read_with(cx, |view, _| view.focus.clone());
        cx.update(|window, cx| window.focus(&focus, cx));
        let press = |cx: &mut VisualTestContext, key: &str| {
            let keystroke = Keystroke::parse(key).expect("test keystroke parses");
            cx.update(|window, cx| {
                window.dispatch_keystroke(keystroke, cx);
            });
        };
        press(cx, "ctrl-n");
        cx.run_until_parked();
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "Ctrl+N opens Add"
        );
        assert!(cx.update(|window, _| window.try_find("field-name").is_some()));
        // Guarded while open: no second dialog, no toast, no steal.
        press(cx, "ctrl-n");
        cx.run_until_parked();
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "dialog stays exactly one"
        );
        assert_eq!(notification_count(cx), 0, "guarded shortcut is silent");
    }

    /// Scenario: F5 starts the selected loopback row headless.
    #[gpui_kit::test]
    fn keyboard_f5_starts_selected(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use gpui_kit::component::IndexPath;
        use gpui_kit::Keystroke;
        cx.update(gpui_kit::init);
        cx.update(crate::bind_app_keys);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(t26_engine_seed("Work", "10911"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
            });
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
            });
        });
        let focus = view.read_with(cx, |view, _| view.focus.clone());
        cx.update(|window, cx| window.focus(&focus, cx));
        let keystroke = Keystroke::parse("f5").expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            1,
            "F5 starts the selected row"
        );
    }

    /// Scenario: Logs OK applies the filter and closes exactly once (L5:
    /// never an explicit close plus a framework close).
    #[gpui_kit::test]
    fn logs_ok_closes_exactly_once(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_logs_dialog(window, cx));
        });
        paint_dialog_open(cx);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        cx.update(|window, cx| {
            window.click("logs-dialog-ok", cx);
        });
        cx.run_until_parked();
        paint_dialog_open(cx);
        assert!(
            !cx.update(|window, cx| window.has_active_dialog(cx)),
            "Logs OK closes"
        );
        assert_eq!(notification_count(cx), 0, "filter apply is silent");
    }

    /// Scenario: a refresh that preserves user edits warns and names the
    /// conflict count (pure copy).
    #[test]
    fn refresh_summary_text_names_conflicts() {
        use crate::refresh_summary_text;
        let (warn, text) = refresh_summary_text("Nightly", 1, 0, 0, 0, 0, 0, 0);
        assert!(!warn);
        assert!(text.contains("Nightly") && text.contains("+1"), "{text}");
        let (warn, text) = refresh_summary_text("Nightly", 1, 0, 0, 0, 0, 0, 2);
        assert!(warn, "conflicts warn: {text}");
        assert!(
            text.contains('2') && text.contains("conflict"),
            "conflict copy in: {text}"
        );
        assert!(
            text.contains("your edits kept"),
            "kept, not reverted: {text}"
        );
    }

    /// Scenario: pasting an export payload opens the restore confirm (never
    /// the import path); confirming restores rows verbatim.
    #[gpui_kit::test]
    fn export_payload_restores_library(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Work")).expect("seed works");
        let payload = store.export_library_json().expect("export works");
        // Diverge: an extra row the backup does not have.
        store.add(seed_input("Extra")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);
        // Pasting the payload diverts to the restore confirm.
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_import(payload.clone(), "g".to_string(), None, window, cx)
            })
        });
        assert!(!closed, "restore waits for confirm");
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "restore confirm opens"
        );
        cx.update(|window, cx| window.close_all_dialogs(cx));
        cx.run_until_parked();
        // Confirm path restores verbatim (Extra is gone, Work is back).
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_library_restore(&payload, window, cx)
            });
        });
        assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);
        let name = view.read_with(cx, |view, _| {
            view.store.list().expect("list works").remove(0).name
        });
        assert_eq!(name, "Work");
        assert_eq!(notification_count(cx), 1, "restore names its counts");
    }

    /// Scenario: a second probe while one runs is deduped with an info
    /// toast (never stacked blocking work).
    #[gpui_kit::test]
    fn probe_dedupe_second_request_warns(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use crate::health::FakeHealthProber;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(t26_engine_seed("Work", "10912"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
                let (prober, _) = FakeHealthProber::shared();
                view.prober = Box::new(prober);
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        let before = notification_count(cx);
        let first =
            cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
        assert_eq!(first, 1, "first probe starts");
        let second =
            cx.update(|window, cx| view.update(cx, |view, cx| view.test_all_health(window, cx)));
        assert_eq!(second, 0, "second probe dedupes");
        cx.run_until_parked();
        // Info (already running) + success (completion) = two new notices.
        assert_eq!(notification_count(cx) - before, 2, "dedupe explains itself");
        assert!(
            !view.read_with(cx, |view, _| view.probe_in_flight),
            "flag clears on completion"
        );
    }

    // -- Ticket 30: list + engine feedback -----------------------------------

    /// Scenario: serving badge copy — running rows show
    /// `● Running :port · health`, stopped rows show `○ Stopped`; the
    /// enabled flag never leaks into the badge (synthetic ports only).
    #[test]
    fn serving_badge_copy() {
        use crate::serving_badge;
        assert_eq!(
            serving_badge(true, 10801, Some("87ms")),
            "● Running :10801 · 87ms"
        );
        assert_eq!(serving_badge(true, 10801, None), "● Running :10801");
        assert_eq!(serving_badge(true, 10801, Some("  ")), "● Running :10801");
        assert_eq!(serving_badge(false, 10801, Some("87ms")), "○ Stopped");
        assert_eq!(serving_badge(false, 10801, None), "○ Stopped");
        for canary in ["hunter2", "SECRET-UUID", "token=abc"] {
            assert!(!serving_badge(true, 10801, Some("87ms")).contains(canary));
        }
    }

    /// Scenario: running vs stopped rows render the badge in the row lines;
    /// the enabled flag stays secondary in its own line.
    #[test]
    fn row_lines_badge_follows_serving_not_flag() {
        use crate::store::ProxyConfig;
        use crate::{library_row_lines, serving_badge};
        fn row() -> ProxyConfig {
            ProxyConfig {
                id: "7".to_string(),
                name: "Work".to_string(),
                protocol: "vless".to_string(),
                params: "{\"server\":\"a.example.com\",\"server_port\":443}".to_string(),
                group: "travel".to_string(),
                enabled: false,
                bind_ip: "127.0.0.1".to_string(),
                port: 10801,
                subscription_id: None,
                identity_key: None,
            }
        }
        let running = library_row_lines(&row(), Some("87ms"), true).join("\n");
        assert!(
            running.contains(&serving_badge(true, 10801, Some("87ms"))),
            "running badge in row:\n{running}"
        );
        assert!(
            running.contains("disabled"),
            "flag stays secondary:\n{running}"
        );
        let stopped = library_row_lines(&row(), Some("87ms"), false).join("\n");
        assert!(
            stopped.contains("○ Stopped"),
            "stopped badge in row:\n{stopped}"
        );
        assert!(
            !stopped.contains("87ms"),
            "stopped rows show no latency:\n{stopped}"
        );
    }

    /// Scenario: honest start-on-running / no-selection / busy / partial /
    /// crash / search-count copies (pure).
    #[test]
    fn honest_engine_feedback_copies() {
        use crate::engine::EngineController;
        use crate::{
            already_running_message, crash_notification_text, empty_library_text, no_matches_text,
            no_selection_message, search_count_text, start_all_partial_message, start_busy_message,
        };
        let restarted = already_running_message("Work");
        assert!(restarted.contains("Work"), "{restarted}");
        assert!(restarted.contains("already running"), "{restarted}");
        assert!(restarted.contains("restarted"), "{restarted}");
        assert!(
            !restarted.starts_with("Started \""),
            "never bare Started: {restarted}"
        );
        assert_eq!(no_selection_message(), "Select a configuration first");
        assert!(
            start_busy_message().contains("Starting"),
            "{}",
            start_busy_message()
        );
        let partial = start_all_partial_message(4, &["\"Broken\"".to_string()]);
        assert!(partial.contains('4'), "{partial}");
        assert!(partial.contains('1'), "{partial}");
        assert!(partial.contains("Broken"), "{partial}");
        // Row-local schema failures isolate; union failures stay atomic.
        assert!(crate::is_row_local_start_error(
            &crate::engine::EngineError::BadParams {
                name: "Broken".to_string(),
                field: "server",
            }
        ));
        assert!(!crate::is_row_local_start_error(
            &crate::engine::EngineError::PortConflict {
                bind: "127.0.0.1".to_string(),
                port: 10901,
                first: "Alpha".to_string(),
                second: "Beta".to_string(),
            }
        ));
        assert_eq!(search_count_text(0, 5), "0 of 5");
        assert_eq!(search_count_text(4, 5), "4 of 5");
        assert!(
            no_matches_text().contains("No matches"),
            "{}",
            no_matches_text()
        );
        assert!(
            no_matches_text().contains("Clear search"),
            "{}",
            no_matches_text()
        );
        let empty = empty_library_text();
        assert!(empty.contains("Add"), "{empty}");
        assert!(empty.contains("Import"), "{empty}");
        let detail = EngineController::crash_detail(&[
            ("stock".to_string(), "signal: killed".to_string()),
            ("xray".to_string(), "signal: segmentation fault".to_string()),
        ]);
        assert!(detail.contains("stock"), "{detail}");
        assert!(detail.contains("xray"), "{detail}");
        assert!(detail.contains("killed"), "{detail}");
        let single =
            EngineController::crash_detail(&[("stock".to_string(), "signal: killed".to_string())]);
        assert!(
            single.contains("stock"),
            "single crash keeps its shard: {single}"
        );
        let notice = crash_notification_text(&single);
        assert!(notice.contains("stock"), "{notice}");
        assert!(notice.contains("killed"), "{notice}");
        assert!(notice.contains("View logs"), "{notice}");
        for text in [restarted, partial, notice, empty] {
            for canary in ["hunter2", "SECRET-UUID", "token=abc"] {
                assert!(!text.contains(canary), "no secrets in {text}");
            }
        }
    }

    fn t30_engine_seed(name: &str, port: &str) -> NewConfig {
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

    /// Scenario: Start-all with 1 broken of 5 → valid rows keep serving,
    /// one toast naming the failed row plus both counts.
    #[gpui_kit::test]
    fn start_all_partial_names_failed_row_and_serves_rest(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        for (name, port) in [
            ("A", "10921"),
            ("B", "10922"),
            ("C", "10923"),
            ("D", "10924"),
        ] {
            store.add(t30_engine_seed(name, port)).expect("seed works");
        }
        // Broken: stored params are not valid for the engine (never echoes).
        store
            .add(NewConfig {
                name: "Broken".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: "10925".to_string(),
            })
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.commit_start_all(window, cx));
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            4,
            "valid rows keep serving"
        );
        assert_eq!(notification_count(cx), 1, "one partial toast");
        // The copy itself (names + counts) is pinned at the pure seam.
        let copy = crate::start_all_partial_message(4, &["\"Broken\"".to_string()]);
        assert!(copy.contains('4') && copy.contains('1') && copy.contains("Broken"));
    }

    /// Scenario: double Start while busy → ignored with an info toast, no
    /// second apply.
    #[gpui_kit::test]
    fn double_start_while_busy_is_ignored(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(t30_engine_seed("Work", "10931"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        let handle = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, handle) = EngineController::fake();
                view.engine = engine;
                view.start_in_flight = true;
                handle
            })
        });
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
            });
        });
        let checks_before = handle.check_calls();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_selected_engine(window, cx));
        });
        assert_eq!(notification_count(cx), 1, "busy explains itself once");
        assert_eq!(handle.check_calls(), checks_before, "no second apply");
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            0,
            "nothing starts while busy"
        );
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.start_in_flight = false;
            });
        });
    }

    /// Scenario: Start on already-running → honest `already running
    /// (restarted)` path, still serving exactly once.
    #[gpui_kit::test]
    fn start_on_running_restarts_honestly(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(t30_engine_seed("Work", "10932"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, _) = EngineController::fake();
                view.engine = engine;
            });
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
            });
        });
        let id = view.read_with(cx, |view, cx| {
            view.selected_config(cx).expect("selected").id
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
        });
        assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 1);
        assert_eq!(notification_count(cx), 1);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            1,
            "restart serves exactly once"
        );
        assert_eq!(notification_count(cx), 2, "restart toasts honestly");
    }

    /// Scenario: no selection + Start (F5 path) → info toast, never a dead
    /// click.
    #[gpui_kit::test]
    fn start_with_no_selection_explains_itself(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        // No selection by default.
        assert!(view.read_with(cx, |view, cx| view.selected_config(cx).is_none()));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_selected_engine(window, cx));
        });
        assert_eq!(notification_count(cx), 1, "no-selection explains itself");
        assert_eq!(
            view.read_with(cx, |view, _| view.engine.running_count()),
            0,
            "nothing starts"
        );
    }

    /// Scenario: sidecar crash → poll tick pushes an error notification
    /// naming shard/cause with the View-logs path (not status-bar-only).
    #[gpui_kit::test]
    fn crash_tick_notifies_with_shard_and_logs_path(cx: &mut TestAppContext) {
        use crate::engine::EngineController;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(t30_engine_seed("Work", "10933"))
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        let stock = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let (engine, stock) = EngineController::fake();
                view.engine = engine;
                stock
            })
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.start_all_engines(window, cx));
        });
        assert_eq!(view.read_with(cx, |view, _| view.engine.running_count()), 1);
        let before = notification_count(cx);
        stock.inject_crash("signal: killed");
        let detail = cx.update(|window, cx| {
            view.update(cx, |view, cx| view.tick_engine_poll_notify(window, cx))
        });
        let detail = detail.expect("crash reported");
        assert!(detail.contains("stock"), "names the shard: {detail}");
        assert!(detail.contains("killed"), "names the cause: {detail}");
        assert_eq!(
            notification_count(cx),
            before + 1,
            "crash notifies (not status-bar-only)"
        );
    }

    /// Scenario: search no-hits vs empty library at the delegate seam —
    /// no-hits keeps the total (`0 of M`), empty stays `0 of 0`.
    #[test]
    fn search_no_hits_vs_empty_counts() {
        use crate::store::ProxyConfig;
        use crate::LibraryDelegate;
        fn row(id: &str, name: &str) -> ProxyConfig {
            ProxyConfig {
                id: id.to_string(),
                name: name.to_string(),
                protocol: "vless".to_string(),
                params: "{}".to_string(),
                group: "default".to_string(),
                enabled: false,
                bind_ip: "127.0.0.1".to_string(),
                port: 10801,
                subscription_id: None,
                identity_key: None,
            }
        }
        let mut delegate = LibraryDelegate::new();
        delegate.set_rows(vec![row("1", "Alpha"), row("2", "Beta")]);
        delegate.query = "zzz-no-hit".to_string();
        delegate.apply_filter();
        assert_eq!(delegate.visible_counts(), (0, 2));
        assert_eq!(crate::search_count_text(0, 2), "0 of 2");
        assert!(crate::no_matches_text().contains("Clear search"));
        let mut empty = LibraryDelegate::new();
        empty.set_rows(Vec::new());
        assert_eq!(empty.visible_counts(), (0, 0));
        assert!(crate::empty_library_text().contains("Add"));
    }

    /// Scenario: no-hits paints `No matches — Clear search` + `N of M`
    /// with a clear affordance; the detail pane carries the serving badge
    /// and no duplicate enable button.
    #[gpui_kit::test]
    fn no_hits_paints_and_detail_has_single_switch(cx: &mut TestAppContext) {
        use gpui_kit::component::IndexPath;
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let seeded = store.add(seed_input("Work")).expect("seed works");
        let (view, cx) = mount_view(store, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    delegate.query = "zzz-no-hit".to_string();
                    delegate.apply_filter();
                    list_cx.notify();
                });
                view.libraries.update(cx, |state, _| {
                    state.delegate_mut().selected = Some(IndexPath::new(0));
                });
            });
        });
        // Selection cleared by the narrowing filter (filtered-out rows never
        // keep the selection); reselect is impossible with zero visible, so
        // restore the query for the detail half of this scenario.
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    delegate.query = String::new();
                    delegate.apply_filter();
                    delegate.selected = Some(IndexPath::new(0));
                    list_cx.notify();
                });
            });
        });
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
        assert!(
            cx.update(|window, _| window
                .try_find(format!("library-row-{}", seeded.id))
                .is_some()),
            "row paints once the filter clears"
        );
        assert_painted(cx, "detail-serving");
        assert!(
            cx.update(|window, _| window.try_find("toggle-enabled".to_string()).is_none()),
            "no duplicate enable button — Switch only"
        );
        // No-hits half: narrow again and check the empty-state affordances.
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.libraries.update(cx, |state, list_cx| {
                    let delegate = state.delegate_mut();
                    delegate.query = "zzz-no-hit".to_string();
                    delegate.apply_filter();
                    list_cx.notify();
                });
            });
        });
        cx.run_until_parked();
        paint_frame(cx);
        assert_painted(cx, "library-empty");
        assert!(
            cx.update(|window, _| window.try_find("clear-search-hint".to_string()).is_some()),
            "no-hits names the Clear search affordance"
        );
        assert!(
            cx.update(|window, _| window.try_find("search-count".to_string()).is_some()),
            "N of M row renders"
        );
    }

    /// Scenario: empty library paints Add/Import CTAs (never a bare blank).
    #[gpui_kit::test]
    fn empty_library_paints_ctas(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (_view, cx) = mount_view(store, cx);
        cx.run_until_parked();
        paint_frame(cx);
        cx.run_until_parked();
        paint_frame(cx);
        assert_painted(cx, "library-empty");
        assert!(
            cx.update(|window, _| window.try_find("empty-library-ctas".to_string()).is_some()),
            "empty library offers Add/Import"
        );
    }

    // -- Ticket 31: dialogs + import + subscriptions ---------------------

    /// Ticket 31: field errors map to their owning dialog field (first error
    /// wins); non-field errors map nowhere. Credentials never appear: the
    /// mapping carries field names only.
    #[test]
    fn field_error_targets_name_the_owning_field() {
        use crate::field_error_target;
        use crate::store::NewConfig;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "   ".to_string(),
                protocol: "bogus".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("bad add fails");
        assert_eq!(
            field_error_target(&error).map(|(field, _)| field),
            Some("name"),
            "first error wins, got {error}"
        );
        let error = store
            .add(NewConfig {
                name: "Work".to_string(),
                protocol: "bogus".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect_err("bad protocol fails");
        let (field, message) = field_error_target(&error).expect("protocol maps");
        assert_eq!(field, "protocol");
        assert!(message.contains("valid"), "message guides, got {message}");
        let error = store
            .add(NewConfig {
                name: "Work".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: String::new(),
                port: "99999".to_string(),
            })
            .expect_err("bad port fails");
        assert_eq!(
            field_error_target(&error).map(|(field, _)| field),
            Some("port"),
            "got {error}"
        );
        assert_eq!(
            field_error_target(&crate::store::StoreError::NotFound("7".to_string())),
            None,
            "non-field errors map nowhere"
        );
    }

    /// Ticket 31: bind/port collision warns naming the colliding row (save
    /// still allowed); free, self, auto, and invalid pairs stay quiet.
    #[test]
    fn bind_port_collision_warning_names_colliding_row() {
        use crate::bind_port_collision_warning;
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add(NewConfig {
                name: "One".to_string(),
                protocol: "vless".to_string(),
                params: String::new(),
                group: String::new(),
                bind_ip: "127.0.0.1".to_string(),
                port: "12001".to_string(),
            })
            .expect("seed works");
        let rows = store.list().expect("list works");
        let warning =
            bind_port_collision_warning(&rows, None, "127.0.0.1", "12001").expect("collides");
        assert!(warning.contains("One"), "names the row, got {warning}");
        assert!(warning.contains("12001"), "names the port, got {warning}");
        assert!(
            warning.contains("still allowed"),
            "non-blocking, got {warning}"
        );
        assert!(
            bind_port_collision_warning(&rows, None, "127.0.0.1", "12002").is_none(),
            "free pair stays quiet"
        );
        assert!(
            bind_port_collision_warning(&rows, None, "127.0.0.1", "").is_none(),
            "blank port means auto — never collides"
        );
        assert!(
            bind_port_collision_warning(&rows, None, "127.0.0.1", "bogus").is_none(),
            "invalid port is validation's job, not the warning's"
        );
        let self_id = rows[0].id.clone();
        assert!(
            bind_port_collision_warning(&rows, Some(&self_id), "127.0.0.1", "12001").is_none(),
            "edit excludes its own row"
        );
        assert!(
            bind_port_collision_warning(&rows, None, "", "12001").is_some(),
            "blank bind means the loopback default"
        );
    }

    /// Ticket 31: plaintext http subscription URLs surface the fetch warning;
    /// https (and unparseable input) stays quiet.
    #[test]
    fn http_subscription_warning_only_for_plaintext() {
        use crate::http_subscription_warning;
        let warning = http_subscription_warning("http://example.com/sub.txt").expect("http warns");
        assert!(
            warning.contains("plaintext"),
            "Warn policy surfaced, got {warning}"
        );
        assert!(
            http_subscription_warning("https://example.com/sub.txt").is_none(),
            "https stays quiet"
        );
        assert!(
            http_subscription_warning("").is_none(),
            "blank is a validation error, not a warning"
        );
        assert!(
            http_subscription_warning("not-a-url").is_none(),
            "unparseable is a validation error, not a warning"
        );
    }

    /// Ticket 31: the friendly duplicate message names the existing label.
    #[test]
    fn already_subscribed_message_names_existing_label() {
        use crate::already_subscribed_message;
        let text = already_subscribed_message("Nightly");
        assert!(
            text.contains("Already subscribed"),
            "friendly path, got {text}"
        );
        assert!(text.contains("Nightly"), "names the row, got {text}");
    }

    /// Ticket 31: invalid add commit stays open with the error, focuses the
    /// owning field, and stores nothing.
    #[gpui_kit::test]
    fn add_commit_invalid_stays_open_and_focuses_field(cx: &mut TestAppContext) {
        use crate::ConfigForm;
        use gpui_kit::component::input::AnyInputState;
        cx.update(gpui_kit::init);
        let store = Store::open_in_memory().expect("in-memory store opens");
        let (view, cx) = mount_view(store, cx);
        let form_error: std::rc::Rc<std::cell::RefCell<Option<(String, String)>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let form = ConfigForm::blank(window, cx);
                form.name.update(cx, |state, cx| {
                    state.set_value("   ", window, cx);
                });
                let expected = AnyInputState::from(form.name.clone()).focus_handle(cx);
                let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
                assert_eq!(
                    window.focused(cx),
                    Some(expected),
                    "focus moves to the error field"
                );
                closed
            })
        });
        assert!(!closed, "invalid commit keeps the dialog open");
        let (field, _) = form_error.borrow().clone().expect("inline error recorded");
        assert_eq!(field, "name", "inline error names the field");
        assert_eq!(
            view.read_with(cx, |view, cx| view.config_count(cx)),
            0,
            "nothing stored"
        );
    }

    /// Ticket 31: duplicate subscription URL takes the friendly path — the
    /// dialog stays open with a warning and no double row appears.
    #[gpui_kit::test]
    fn duplicate_subscription_url_warns_without_double_row(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        store
            .add_subscription("Nightly", "https://example.com/a.txt", "g", true)
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_subscription_add(
                    "Copy".to_string(),
                    "  https://example.com/a.txt  ".to_string(),
                    "g".to_string(),
                    true,
                    window,
                    cx,
                )
            })
        });
        assert!(!closed, "dup keeps the dialog open");
        assert_eq!(notification_count(cx), 1, "one friendly warning");
        let subs = view.read_with(cx, |view, _| {
            view.store.list_subscriptions().expect("list works")
        });
        assert_eq!(subs.len(), 1, "no double row");
        assert_eq!(subs[0].label, "Nightly", "original kept");
    }

    /// Ticket 31: edit subscription persists label/url/group via
    /// `Store::update_subscription` and refreshes the list; a ghost edit
    /// warns and closes.
    #[gpui_kit::test]
    fn edit_subscription_persists_and_refreshes(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let sub = store
            .add_subscription("Nightly", "https://example.com/a.txt", "g1", true)
            .expect("seed works");
        let (view, cx) = mount_view(store, cx);
        let closed = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_subscription_edit(
                    &sub.id,
                    "Daily".to_string(),
                    "https://example.com/b.txt".to_string(),
                    "g2".to_string(),
                    window,
                    cx,
                )
            })
        });
        assert!(closed, "valid edit closes");
        assert_eq!(notification_count(cx), 1, "one success notice");
        let subs = view.read_with(cx, |view, _| {
            view.store.list_subscriptions().expect("list works")
        });
        assert_eq!(subs.len(), 1, "edit refreshes in place");
        assert_eq!(subs[0].label, "Daily");
        assert_eq!(subs[0].url, "https://example.com/b.txt");
        assert_eq!(subs[0].group, "g2");
        let ghost = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.commit_subscription_edit(
                    "9999",
                    "x".to_string(),
                    "https://example.com/x.txt".to_string(),
                    "g".to_string(),
                    window,
                    cx,
                )
            })
        });
        assert!(ghost, "ghost edit closes with a warning");
        assert_eq!(notification_count(cx), 2, "ghost warns once more");
    }
}
