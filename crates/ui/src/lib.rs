#![recursion_limit = "512"]

pub mod app;
pub mod appearance;
pub mod chain;
pub mod dialog;
pub mod forms;
mod i18n;
pub mod keys;
pub mod library;
pub mod os;
pub mod pickers;
pub mod preset;
pub mod text;
pub mod tray;
pub mod update;

use manyproxy_core::*;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::chart::LineChart;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::{AlertDialog, Confirm, Dialog, DialogButtonProps, DialogFooter};
use gpui_kit::component::empty::{Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyTitle};
use gpui_kit::component::form::{field, v_form, Field};
use gpui_kit::component::group_box::GroupBox;
use gpui_kit::component::input::{
    AnyInputState, Input, InputEvent, InputState, Textarea, TextareaState,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::label::Label;
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::resizable::{h_resizable, resizable_panel, ResizableState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::searchable_list::{SearchableListDelegate, SearchableListItem};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::separator::Separator;
use gpui_kit::component::sheet::Sheet;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::Sizable;
use gpui_kit::component::TitleBar;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt};
use gpui_kit::Subscription;
// `gpui_kit::*` re-exports gpui itself, so it also brings gpui's `Path` and
// `ListState` into scope alongside the kit's own types. Modules that need the
// kit's `ListState` or std's `Path` import them by name, which shadows the
// glob and keeps name resolution unambiguous.
use gpui_kit::*;
use qrcodegen::{QrCode, QrCodeEcc};

use manyproxy_core::engine::{
    engine_binary_file_name, fork_binary_file_name, is_custom_config_protocol,
    requires_lan_confirm, resolve_engine_binary, resolve_fork_binary, resolve_xray_binary,
    xray_binary_file_name, AutoRestartConfig, EngineController, EngineError,
};
use manyproxy_core::health::{
    clamp_log_max_lines, outcome_is_neutral, ping_all, redact_secrets, sparkline_caption_tone,
    test_all_stoppable, test_group, truncate, url_test_sparkline, HealthKind, HealthProber,
    HealthStore, LatencyTone, LogLevel, LogStore, LogTagIndex, ProbeExclusions, RealHealthProber,
    TestOutcome, TrafficSeenMap, DEFAULT_TEST_URL,
};
use manyproxy_core::import::{
    classify_open_with, commit_batch, format_import_failures, import_text, is_addsub_attempt,
    is_library_export, parse_addsub_link, AddSubRequest, OpenWithKind,
};
use manyproxy_core::store::{
    blank_name, chain_hop_rejection, chain_nodes_of, compare_type_security, count_importable,
    due_for_refresh_for, extra_config_prefill, fold_amneziawg_into_params, fold_anytls_into_params,
    fold_chain_nodes_into_params, fold_dial_into_params, fold_extra_into_params,
    fold_guided_into_params, fold_http_into_params, fold_hysteria_into_params,
    fold_mux_into_params, fold_naive_into_params, fold_shadowsocks_inner_into_params,
    fold_shadowtls_into_params, fold_snell_into_params, fold_socks_into_params,
    fold_ssh_into_params, fold_tls_forced_into_params, fold_tls_optional_into_params,
    fold_tls_vmess_into_params, fold_transport_into_params, fold_trojan_inner_into_params,
    fold_tuic_into_params, fold_vless_inner_into_params, fold_vmess_inner_into_params,
    fold_wireguard_into_params, fold_xray_into_params, guided_blank_field, guided_identity_fields,
    guided_prefill_bool, guided_prefill_list, guided_prefill_value, is_virtual_protocol, now_unix,
    pristine_template_rewrite, protocol_params_template, proxy_urls, validate_extra_params,
    vmess_cipher_value, vmess_tls_enabled, AmneziaFold, AnytlsFold, AutoClearGroupConfig,
    ChainHopSlot, CloseAction, ConfigPatch, DialFold, ExtraFold, GroupPruneKind, HealthSnapshot,
    HttpFold, HysteriaFold, LibrarySort, LibrarySortKey, MuxFold, NaiveFold, NewConfig,
    ProxyConfig, ShadowTlsFold, SnellFold, SocksFold, SshFold, Store, StoreError,
    Subscription as ProxySubscription, TestAllScope, TlsForcedFold, TlsOptionalFold, TransportFold,
    TuicFold, WindowGeometry, WireguardFold, XrayFold, DEFAULT_BIND, DEFAULT_GROUP,
    DEFAULT_PROBE_TIMEOUT_SECS, DEFAULT_PROTOCOL, MAX_PROBE_TIMEOUT_SECS, MIN_PROBE_TIMEOUT_SECS,
    VALID_PROTOCOLS,
};

/// T-03: the inspector docks to the RIGHT of the library. Below this window
/// width it collapses to a slide-over drawer, so the table never loses the
/// width it needs.
pub const INSPECTOR_DRAWER_BREAKPOINT: f32 = 1080.;
/// R12: the inspector's width range. The default and the clamped range live
/// in the store, which owns the `inspector_width` key.
pub const INSPECTOR_WIDTH_MIN: f32 = store::Store::INSPECTOR_WIDTH_MIN;
pub const INSPECTOR_WIDTH_MAX: f32 = store::Store::INSPECTOR_WIDTH_MAX;

pub use i18n::translate;

use app::actions::*;
use app::bits::*;
use app::endpoint_editor::{
    render_endpoint_form, render_endpoint_popover, EndpointDraft, EndpointFrame, EndpointHost,
    ENDPOINT_EDITOR_CONTEXT,
};
use app::focus::*;
use app::kit_bits::*;
use app::nav_scopes::{RowDrag, RowDragPreview};
use app::open::*;
use app::selection::{BatchKind, BatchRun};
use app::state::{
    ConnectionState, ConsoleState, GroupEntry, LogsState, PaneWindow, RefreshBatch, SourceEntry,
};
use app::window::*;
use appearance::*;
use chain::*;
use dialog::*;
use forms::*;
use keys::*;
use library::*;
use pickers::*;
use preset::*;
use text::*;

// The UI's localized copy shadows the core crate's plain builders of the same
// name. An explicit import wins over both globs, so every call site below
// resolves to the copy that can reach the Fluent bundle.
#[allow(unused_imports)]
pub use text::bulk_delete_result_message;
pub use text::{
    about_body, addsub_error_message, already_running_message, already_stopped_message,
    already_subscribed_message, auto_clear_opt_in_note, auto_clear_summary_toast,
    bind_port_collision_warning, bulk_start_enable_flag_failed_message, bulk_start_failed_message,
    bulk_start_refused_message, bulk_start_unreadable_message, clear_group_tests_confirm_text,
    close_confirm_description, crash_notification_text, db_list_error_message,
    enabled_flag_message, export_confirm_description, export_count_message,
    export_overwrite_confirm_description, export_preview_description,
    group_delete_confirm_description, group_row_views, group_test_summary_toast,
    http_subscription_warning, import_files_label, import_preview_message, inspector_badge_text,
    lan_confirm_description, lan_confirm_text, lan_confirm_title, lan_exposure_warning,
    lan_hint_for_bind, log_level_tooltip, logs_overwrite_text, logs_save_path_text,
    no_selection_message, preset_delete_confirm_description, preset_error_message,
    preset_imported_message, preset_row_summary, preset_summary_text, probe_batched_toast,
    probe_result_toast, probe_start_toast, prune_duplicates_confirm_text,
    prune_invalid_confirm_text, prune_unavailable_confirm_text, refresh_summary_text,
    removed_message, restart_to_apply_message, restore_done_message, row_action_message,
    rule_set_summary, settings_clear_health_text, settings_clear_snapshot_text,
    settings_clear_traffic_text, sidecar_presence_line, start_all_message,
    start_all_partial_message, start_busy_message, stop_all_confirm_description, stop_all_message,
    subscription_failures_text, subscription_interval_failure_message, undo_failed_message,
    undo_restored_message, unreadable_drop_message, update_all_confirm_text,
    update_feed_failure_message,
};
use update::*;

actions!(
    manyproxy,
    [
        AddConfig,
        ImportConfigs,
        PasteImport,
        StartSelected,
        StopSelected,
        TestSelected,
        StopTesting,
        ClearSelection,
        EditSelected,
        EditEndpoint,
        ApplyEndpoint,
        EscapeEndpoint,
        DeleteSelected,
        SelectPreviousRow,
        SelectNextRow,
        FocusPreviousRow,
        FocusNextRow,
        FocusFirstRow,
        FocusLastRow,
        ToggleFocused,
        SelectAllRows,
        FocusSearch,
        ToggleInspector,
        ToggleConsole,
        OpenPalette,
        CopyUrls,
        UpdateCurrentGroup
    ]
);

pub struct AppView {
    pub focus: FocusHandle,
    pub libraries: Entity<ListState<LibraryDelegate>>,

    pub detail_scroll: ScrollHandle,
    pub store: Store,

    /// T-08: the last failed write probe, as the standing banner's trigger.
    /// `Some(reason)` means the data folder refused a real write; `None` means
    /// the folder is writable (or the store is in-memory, which has no folder).
    /// Set after a probe, cleared only by a probe that succeeds.
    pub write_blocked: Option<String>,

    /// T-08: the folder the banner names when no store can supply one. Only a
    /// session that fell back to memory after a failed open sets it; a
    /// deliberate in-memory store leaves it `None`.
    pub write_blocked_folder: Option<PathBuf>,

    /// T-08: the banner is pinned by a failed open. A probe may prove the
    /// folder writable again, but this session still cannot save, so the
    /// banner must not be cleared into a lie.
    pub write_blocked_pinned: bool,

    pub engine: EngineController,

    pub health_store: HealthStore,

    pub logs: LogsState,

    /// T-04: the Settings dialog's Enter-commit subscriptions. The dialog owns
    /// its text fields, so the subscriptions live as long as it does.
    pub settings_subscriptions: Vec<Subscription>,

    /// T-04: the timestamp of the last update check, mirrored into the open
    /// Settings dialog so the Updates section can show it.
    pub settings_last_check: Option<Rc<RefCell<Option<i64>>>>,

    pub expanded_subs: HashSet<String>,

    /// The one selection model: the set, the anchor a range grows from, and
    /// the focus the keyboard is on. Session-only — nothing about it is
    /// persisted, and it never survives a launch.
    pub selection: HashSet<String>,

    pub selection_anchor: Option<String>,

    pub selection_focus: Option<String>,

    /// The last batch: the ids it snapshotted, how far it got and what it
    /// found. Transient — the bar shows it until the selection changes or
    /// the user clears the result.
    pub batch: Option<BatchRun>,

    /// The row density. It owns every chrome height and the row pitch, and
    /// it is persisted under `row_density`.
    pub density: Density,

    /// T-27: resolved out-IP text per row id, session-only. The resolution
    /// action already computes it; keeping it per row is what `out_ip` sorts by.
    pub out_ip_by_row: HashMap<String, String>,

    pub traffic_last_seen: TrafficSeenMap,

    pub traffic_tick_in_flight: bool,

    pub traffic_mirror_hydrated: bool,

    pub prober: Box<dyn HealthProber + Send + Sync>,
    pub ticks_enabled: bool,

    pub refresh: RefreshBatch,

    pub probe_url_in_flight: bool,
    pub probe_ping_in_flight: bool,

    pub update_check_in_flight: bool,

    /// The newer release a passive check found. It is the status-bar hint's
    /// whole state: set on `Newer`, cleared on a check, an up-to-date result,
    /// or the opt-out.
    pub update_available: Option<FetchedRelease>,

    /// Bumped whenever a manual check or the opt-out supersedes a passive
    /// fetch, so the stale result cannot land after the user moved on.
    pub passive_update_epoch: u64,

    pub probe_stop: Arc<AtomicBool>,

    pub group_test_in_flight: bool,

    /// K5: this view's fixed key for the one `Testing N…` toast, so a sweep's
    /// progress repaints that toast instead of pushing another.
    pub probe_toast_key: u64,

    pub subs_form: Option<SubForm>,

    pub preset_form: Option<PresetForm>,

    /// F7: the open Import form, so a window drop lands in its textarea the
    /// way Browse does. `Some` only while the Import dialog is up; every
    /// close path clears it, and a stale marker self-heals on the next drop.
    pub import_form: Option<ImportForm>,

    /// T-19: the row whose ConfigForm edit dialog is open, while it is open —
    /// auto-clear never removes the row under edit.
    pub edit_form_open: Option<String>,

    /// Slice 5: one endpoint draft at a time. The row popover and the
    /// inspector inline form both render this; no per-row editor map exists.
    pub endpoint_draft: Option<EndpointDraft>,

    /// T-12: one confirmed rule-set download at a time; the explicit per-file
    /// confirm never batches.
    pub rule_set_download_in_flight: bool,

    pub import_parse_in_flight: bool,

    pub import_parse_epoch: u64,

    pub import_cancel_epoch: Option<u64>,

    pub close_confirmed: bool,

    pub tray: Option<crate::tray::TrayHandle>,

    /// Task 12: DWM can drop the corner preference when the window leaves the
    /// maximized state, so render re-applies the stored style on that edge
    /// only — never once per frame.
    pub window_was_maximized: bool,

    /// Taskbar progress + thumbnail toolbar. `None` until the main window's
    /// handle exists; the install is a no-op under the test harness.
    pub os_taskbar: Option<crate::os::taskbar::Taskbar>,

    /// Hidden-window balloons for failures that happen while the window is
    /// gone. Same lifecycle as `os_taskbar`.
    pub os_notifier: Option<crate::os::notify::Notifier>,

    /// Dedup and flood rules for [`Self::os_notifier`], kept in the view so the
    /// policy outlives any single balloon.
    pub os_notice_policy: crate::os::notify::NoticePolicy,

    pub last_snapshot: Vec<String>,

    pub start_in_flight: bool,

    pub ticks_started: bool,

    pub sidecar_presence: Vec<String>,

    pub _libraries_subscription: Subscription,

    /// Slice 3: the one query. Session-only, never written to the store.
    pub query: FilterQuery,
    /// Slice 7: the nav-owned scope. `All` until the user picks a group or a
    /// source; the delegate reads it and never owns one.
    pub scope: LibraryScope,
    /// Slice 7: every scope's total, recomputed on the row-load path. The nav
    /// reads this instead of walking the library per frame.
    pub scope_counts: Vec<(LibraryScope, usize)>,
    /// Slice 7: enabled rows in the current scope, for the action-bar count.
    pub scope_enabled: usize,
    /// Slice 7: groups created in this session before any row used them.
    /// Session-only; the store has no group registry.
    pub created_groups: Vec<String>,
    pub group_list: Vec<GroupEntry>,
    pub source_list: Vec<SourceEntry>,
    /// Slice 7: the group entry the row drag is hovering, for the drop rim.
    pub row_drop_target: Option<String>,
    /// Slice 7: the scope list's scroll offset. Session-only, never persisted.
    pub nav_scroll: ScrollHandle,
    /// Slice 3: the rows the query keeps, in paint order. Owned here so the
    /// filter row, the batch bar and the empty state read one list.
    pub visible: Vec<usize>,
    /// R29: the last batch's failures, narrowed to themselves until an
    /// ordinary filter control takes the filter away.
    pub failure_filter: Option<HashSet<String>>,

    pub search_input: Entity<InputState>,
    pub _search_subscription: Subscription,
    /// Slice 3: one cancellable 80ms task and the epoch that retires it.
    pub search_debounce: Option<Task<()>>,
    pub search_epoch: u64,

    /// The Filter popover's port fields. They are two views of `query.port`;
    /// editing either updates the query without rewriting the search bytes.
    pub port_from: Entity<InputState>,
    pub port_to: Entity<InputState>,
    pub _port_from_subscription: Subscription,
    pub _port_to_subscription: Subscription,
    pub port_error: Option<String>,
    /// A `port=` token in the search changed the predicate; the next frame
    /// writes it into the popover's From/To fields.
    pub port_fields_stale: bool,

    /// The protocol popover's local filter field, shown above eight options.
    pub protocol_filter: Entity<InputState>,
    pub _protocol_filter_subscription: Subscription,

    /// T-03/R11: the one column layout — order, visibility and widths. The
    /// header and the delegate both read this copy; nothing else owns one.
    pub column_layout: ColumnLayout,

    /// T-03: the width drag in flight. `None` means no pointer is resizing a
    /// column; the layout updates every move and only mouse-up writes the
    /// store.
    pub column_resize: Option<ColumnResize>,

    /// T-04: the header drag's live landing slot. `None` while not dragging (or
    /// between cells), so the strip paints one indicator at most.
    pub column_drop: Option<(ColumnId, DropSide)>,

    /// The horizontal offset shared by the header strip and the rows. One
    /// scroll container owns it, so the two can never drift apart.
    pub table_scroll: ScrollHandle,

    /// T-03: the split's own state. The width is persisted under
    /// `inspector_width` on mouse-up; between drags the split reads live.
    pub split_state: Entity<ResizableState>,

    /// R6: ONE flag owns the inspector's presence in both presentations —
    /// docked (≥1080) and the slide-over drawer below it. It starts closed;
    /// `Ctrl+I` or picking a row opens it, Esc or the close button shuts it.
    /// Esc hands focus back to the list. The target is the selection.
    pub inspector_visible: bool,

    /// Slice 6: the console dock — open state, tab, scope, quick filter and
    /// the pause snapshot. Session-only; a launch starts collapsed on Logs/All.
    pub console: ConsoleState,

    /// The console quick filter's input entity. One dock, one filter, rebuilt
    /// with the view like the search field.
    pub console_filter_input: Entity<InputState>,
    pub _console_filter_subscription: Subscription,

    /// B3: one scroll window per row-listing console pane, so an open console
    /// builds only the rows it can show and a return to a pane keeps its
    /// position. The old inspector `pane_*` handles renamed to their new home.
    pub console_logs: PaneWindow,
    pub console_connections: PaneWindow,
    pub console_traffic: PaneWindow,

    pub conn: ConnectionState,

    pub palette: Option<Entity<CommandState>>,

    /// T-30: per-shard CPU (% of one core) / RAM (MB) rings, keyed by the
    /// shard label `poll_shards` uses. One entry per live shard, 60 samples.
    pub shard_rings: BTreeMap<String, ShardRing>,

    /// T-30: previous raw counters per label — the delta baseline for the next
    /// engine tick. Dropped with the shard, re-primed on respawn.
    pub shard_baselines: BTreeMap<String, (u32, ProcessSample, Instant)>,

    /// T-30: shards whose handle the OS refused on the last tick.
    pub shard_denied: HashSet<String>,

    /// T-30: the read-only sampler seam; production reads Win32, tests inject.
    pub process_sampler: ProcessSampler,

    pub started_at: std::time::Instant,
}

/// T-03: the start of one header-edge drag. The delta is measured from the
/// pointer's start x, so a move that returns to the origin restores the width
/// without accumulating rounding drift.
#[derive(Clone, Copy)]
pub struct ColumnResize {
    pub column: ColumnId,
    pub start_x: f32,
    pub start_width: f32,
}

/// T-04: where a dragged header would land relative to the hovered column.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DropSide {
    Before,
    After,
}

/// T-04: the payload carried by a header drag. The movable set is the payload
/// type, so a drag can never target the pinned lamp/check/name/copy cells.
#[derive(Clone, Copy)]
pub struct ColumnDrag {
    pub column: ColumnId,
}

/// T-08: why this session is running in memory instead of on disk. The banner
/// names `folder` and carries `reason` in its tooltip; the pair comes from the
/// open failure itself, because an in-memory store has no folder to probe.
pub struct WriteFallback {
    pub folder: Option<PathBuf>,
    pub reason: String,
}

/// T-08: the folder a failed open was trying to use, when the path has one.
pub fn store_folder(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

pub fn install_crash_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut report = format!("panic: {info}\n");
        report.push_str(&format!(
            "backtrace:\n{}\n",
            std::backtrace::Backtrace::force_capture()
        ));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let file_name = format!("manyproxy-crash-{stamp}.txt");
        let path = crate::store::app_data_dir().join(&file_name);
        if append_crash_report(&path, &report).is_err() {
            // A panic before the first store open (the startup failures worth
            // catching) has no `data\` yet; fall back beside the exe. Both
            // failures are silent: the hook itself must never panic.
            if let Some(app_dir) = crate::engine::current_app_dir() {
                let _ = append_crash_report(&app_dir.join(&file_name), &report);
            }
        }
        previous_hook(info);
    }));
}

pub fn append_crash_report(path: &std::path::Path, report: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(report.as_bytes())
}

pub fn run() {
    install_crash_hook();
    let _ = crate::tray::clear_dead_autostart_target();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    if crate::single_instance::forward_to_running(&argv) {
        return;
    }
    let instance_rx = crate::single_instance::listen();
    let launch_intake = argv.iter().find_map(|arg| classify_open_with(arg));
    let startup_geometry = stored_window_geometry();
    // F13: with no asset source every kit icon renders as an empty box, which
    // is what made the sheet header close unreadable.
    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);

        bind_app_keys(cx);
        let visible_bounds = cx.primary_display().map(|display| display.visible_bounds());
        let startup_bounds = startup_window_bounds(startup_geometry, visible_bounds);
        let window_options = app_window_options(startup_bounds, visible_bounds);
        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                view.update(cx, |view, cx| {
                    view.refresh_due_on_startup_deferred(cx);
                    view.start_background_ticks(cx);
                });

                view.update(cx, |view, cx| {
                    view.create_tray(window, cx);
                    view.install_os(window, cx);
                    if let Err(error) = crate::os::scheme::repair_if_registered() {
                        eprintln!("link handler repair failed: {error}");
                    }
                    view.restore_on_launch(window, cx);

                    // Task 14: with the icon off there is no way back from a
                    // hidden window, so an autostart `--tray` launch stays
                    // visible rather than stranding the user.
                    if crate::tray::launched_minimized() && view.tray.is_some() {
                        view.hide_to_tray(window, cx);
                    }

                    // The weekly check runs after the hidden state settles, so
                    // a tray launch can raise the balloon instead of a hint.
                    view.start_passive_update_check(cx);

                    if let Some(intake) = launch_intake.clone() {
                        view.handle_launch_intake(intake, window, cx);
                    }
                });

                if let Some(mut rx) = instance_rx {
                    let forward_view = view.clone();
                    let forward_handle = window.window_handle();
                    cx.spawn(async move |cx| {
                        use futures::StreamExt as _;
                        // Awaiting the channel wakes this task exactly when a
                        // second launch forwards its argv — no idle timer.
                        while let Some(argv) = rx.next().await {
                            // A plain relaunch has no intake; it still raises
                            // the window, including from the tray.
                            let intake = argv.iter().find_map(|arg| classify_open_with(arg));
                            let forward_view = forward_view.clone();
                            let opened = cx.update_window(forward_handle, move |_, window, cx| {
                                forward_view.update(cx, |view, cx| {
                                    view.show_window_from_tray(window, cx);
                                    if let Some(intake) = intake {
                                        view.handle_launch_intake(intake, window, cx);
                                    }
                                });
                            });
                            if opened.is_err() {
                                return;
                            }
                        }
                    })
                    .detach();
                }

                // The same pump serves the tray menu and the taskbar's
                // thumbnail buttons. It runs even without a tray because the
                // thumbnail toolbar and hidden-window notifications still need
                // a drain on a window that has no notification-area icon.
                let poll_view = view.clone();
                let poll_handle = window.window_handle();
                cx.spawn(async move |cx| loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(TRAY_PUMP_MILLIS))
                        .await;

                    let tick = cx.update_window(poll_handle, |_, window, cx| {
                        poll_view.update(cx, |view, cx| {
                            let quit = view.poll_tray_once(window, cx);
                            view.poll_os_once(window, cx);
                            quit
                        })
                    });
                    match tick {
                        Ok(true) => {
                            cx.update(|cx| cx.quit());
                            break;
                        }
                        Ok(false) => {}
                        Err(_) => break,
                    }
                })
                .detach();

                let weak = view.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    let Some(view) = weak.upgrade() else {
                        return true;
                    };

                    // T-04: the close button does what the user chose in
                    // Settings → General → Window behaviour. `Tray` hides the
                    // window and leaves the proxies running; `Quit` stops
                    // every profile and exits.
                    let has_tray = view.read_with(cx, |view, _| view.tray.is_some());
                    let route = view.read_with(cx, |view, _| view.close_route(has_tray));
                    if route == app::settings::CloseRoute::HideToTray {
                        view.update(cx, |view, cx| view.hide_to_tray(window, cx));
                        // T-04: the one-time hint that the app is still running.
                        view.update(cx, |view, cx| view.show_tray_hint_once(window, cx));
                        return false;
                    };
                    match view.read_with(cx, |view, _| view.close_veto_text()) {
                        None => {
                            // T-34: a plain close with nothing running is an
                            // orderly exit too — mark it before the window goes.
                            // The rectangle has to travel with it: without this
                            // save, a close-to-quit dropped the window's placed
                            // position and the next launch started centered.
                            view.update(cx, |view, _| {
                                view.save_window_geometry(window);
                                view.mark_clean_exit();
                            });
                            true
                        }
                        Some(description) => {
                            let confirm_view = view.clone();
                            open_app_alert_dialog(window, cx, move |alert, _, _| {
                                let confirm_view = confirm_view.clone();
                                let description = description.clone();
                                alert
                                    .confirm()
                                    .button_props(
                                        DialogButtonProps::default()
                                            .show_cancel(true)
                                            .ok_text(t!("close-ok"))
                                            .ok_variant(ButtonVariant::Danger),
                                    )
                                    .title(t!("close-title"))
                                    .description(description)
                                    .on_ok(move |_, window, cx| {
                                        confirm_view.update(cx, |view, _| {
                                            view.confirm_close_stop(window);
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

