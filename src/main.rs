#![recursion_limit = "512"]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod appearance;
mod consts;
mod dialog;
mod dpapi;
mod engine;
mod forms;
mod health;
mod import;
mod import_flow;
mod keys;
mod library;
mod models;
mod pickers;
mod preset;
mod store;
mod text;
mod tray;
mod update;

use std::cell::RefCell;
use std::cmp::Ordering;
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
use gpui_kit::component::progress::Progress;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::resizable::{h_resizable, resizable_panel, ResizableState};
use gpui_kit::component::searchable_list::{SearchableListDelegate, SearchableListItem};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::separator::Separator;
use gpui_kit::component::sheet::Sheet;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Sizable;
use gpui_kit::component::TitleBar;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt};
// `gpui_kit::*` re-exports gpui itself, so it also brings gpui's `Path` and
// `ListState` into scope alongside the kit's own types. Modules that need the
// kit's `ListState` or std's `Path` import them by name, which shadows the
// glob and keeps name resolution unambiguous.
use gpui_kit::*;
use qrcodegen::{QrCode, QrCodeEcc};

use engine::{
    engine_binary_file_name, fork_binary_file_name, is_custom_config_protocol,
    lan_confirm_description, lan_confirm_text, lan_exposure_warning, requires_lan_confirm,
    resolve_engine_binary, resolve_fork_binary, resolve_xray_binary, strip_ansi_codes,
    xray_binary_file_name, AutoRestartConfig, EngineController, EngineError, LAN_CONFIRM_TITLE,
};
use health::{
    clamp_log_max_lines, outcome_is_neutral, ping_all, redact_secrets, sparkline_caption_tone,
    test_all, test_group, truncate, url_test_sparkline, HealthKind, HealthProber, HealthStore,
    LatencyTone, LogLevel, LogStore, LogTagIndex, ProbeExclusions, RealHealthProber, TestOutcome,
    TrafficSeenMap, DEFAULT_TEST_URL,
};
use import::{
    classify_open_with, commit_batch, effective_hint_for_text, format_import_failures,
    import_file_content, import_file_size_ok, import_multi_file_contents, import_text,
    is_addsub_attempt, is_library_export, parse_addsub_link, reject_file_bytes, AddSubRequest,
    ImportError, OpenWithKind,
};
use store::{
    blank_name, chain_hop_rejection, chain_nodes_of, clear_group_tests_confirm_text,
    compare_type_security, count_importable, due_for_refresh_for, export_confirm_description,
    extra_config_prefill, fold_amneziawg_into_params, fold_anytls_into_params,
    fold_chain_nodes_into_params, fold_dial_into_params, fold_extra_into_params,
    fold_guided_into_params, fold_http_into_params, fold_hysteria_into_params,
    fold_mux_into_params, fold_naive_into_params, fold_shadowsocks_inner_into_params,
    fold_shadowtls_into_params, fold_snell_into_params, fold_socks_into_params,
    fold_ssh_into_params, fold_tls_forced_into_params, fold_tls_optional_into_params,
    fold_tls_vmess_into_params, fold_transport_into_params, fold_trojan_inner_into_params,
    fold_tuic_into_params, fold_vless_inner_into_params, fold_vmess_inner_into_params,
    fold_wireguard_into_params, fold_xray_into_params, guided_blank_field, guided_identity_fields,
    guided_prefill_bool, guided_prefill_list, guided_prefill_value, is_virtual_protocol, now_unix,
    pristine_template_rewrite, protocol_params_template, proxy_urls, prune_duplicates_confirm_text,
    prune_invalid_confirm_text, prune_unavailable_confirm_text, row_security_label,
    validate_extra_params, vmess_cipher_value, vmess_tls_enabled, AmneziaFold, AnytlsFold,
    AutoClearGroupConfig, ChainHopSlot, CloseAction, ConfigPatch, DialFold, ExtraFold,
    GroupPruneKind, HealthSnapshot, HttpFold, HysteriaFold, LibrarySort, LibrarySortKey, MuxFold,
    NaiveFold, NewConfig, ProxyConfig, RestoreOutcome, ShadowTlsFold, SnellFold, SocksFold,
    SortDirection, SshFold, Store, StoreError, Subscription as ProxySubscription, TestAllScope,
    TlsForcedFold, TlsOptionalFold, TransportFold, TuicFold, WindowGeometry, WireguardFold,
    XrayFold, DEFAULT_BIND, DEFAULT_GROUP, DEFAULT_PROBE_TIMEOUT_SECS, DEFAULT_PROTOCOL,
    MAX_PROBE_TIMEOUT_SECS, MIN_PROBE_TIMEOUT_SECS, VALID_PROTOCOLS,
};

/// T-03: the inspector docks to the RIGHT of the library. Below this window
/// width it collapses to a slide-over drawer, so the table never loses the
/// width it needs.
pub(crate) const INSPECTOR_DRAWER_BREAKPOINT: f32 = 1080.;
/// The inspector's default width. The user may drag it inside the range
/// below; the width is session-only — nothing about it is persisted.
pub(crate) const INSPECTOR_WIDTH: f32 = 260.;
pub(crate) const INSPECTOR_WIDTH_MIN: f32 = 240.;
pub(crate) const INSPECTOR_WIDTH_MAX: f32 = 360.;

use app::actions::*;
use app::bits::*;
use app::focus::*;
use app::kit_bits::*;
use app::open::*;
use app::state::{ConnectionState, LogsState, RefreshBatch};
use app::window::*;
use appearance::*;
use consts::*;
use dialog::*;
use forms::*;
use import_flow::*;
use keys::*;
use library::*;
use models::*;
use pickers::*;
use preset::*;
use text::*;
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
        ClearChecked,
        EditSelected,
        DeleteSelected,
        FocusSearch,
        InspectorDetails,
        InspectorHealth,
        InspectorLogs,
        InspectorLinks,
        InspectorTraffic,
        OpenPalette,
        CopyUrls,
        UpdateCurrentGroup
    ]
);

struct AppView {
    focus: FocusHandle,
    libraries: Entity<ListState<LibraryDelegate>>,

    detail_scroll: ScrollHandle,
    store: Store,

    /// T-08: the last failed write probe, as the standing banner's trigger.
    /// `Some(reason)` means the data folder refused a real write; `None` means
    /// the folder is writable (or the store is in-memory, which has no folder).
    /// Set after a probe, cleared only by a probe that succeeds.
    write_blocked: Option<String>,

    /// T-08: the folder the banner names when no store can supply one. Only a
    /// session that fell back to memory after a failed open sets it; a
    /// deliberate in-memory store leaves it `None`.
    write_blocked_folder: Option<PathBuf>,

    /// T-08: the banner is pinned by a failed open. A probe may prove the
    /// folder writable again, but this session still cannot save, so the
    /// banner must not be cleared into a lie.
    write_blocked_pinned: bool,

    engine: EngineController,

    health_store: HealthStore,

    logs: LogsState,

    /// T-04: the Settings dialog's Enter-commit subscriptions. The dialog owns
    /// its text fields, so the subscriptions live as long as it does.
    settings_subscriptions: Vec<Subscription>,

    /// T-04: the timestamp of the last update check, mirrored into the open
    /// Settings dialog so the Updates section can show it.
    settings_last_check: Option<Rc<RefCell<Option<i64>>>>,

    expanded_subs: HashSet<String>,

    /// T-27: resolved out-IP text per row id, session-only. The resolution
    /// action already computes it; keeping it per row is what `out_ip` sorts by.
    out_ip_by_row: HashMap<String, String>,

    traffic_last_seen: TrafficSeenMap,

    traffic_tick_in_flight: bool,

    traffic_mirror_hydrated: bool,

    prober: Box<dyn HealthProber + Send + Sync>,
    ticks_enabled: bool,

    refresh: RefreshBatch,

    probe_url_in_flight: bool,
    probe_ping_in_flight: bool,

    update_check_in_flight: bool,

    probe_stop: Arc<AtomicBool>,

    group_test_in_flight: bool,

    subs_form: Option<SubForm>,

    preset_form: Option<PresetForm>,

    /// F7: the open Import form, so a window drop lands in its textarea the
    /// way Browse does. `Some` only while the Import dialog is up; every
    /// close path clears it, and a stale marker self-heals on the next drop.
    import_form: Option<ImportForm>,

    /// T-19: the row whose ConfigForm edit dialog is open, while it is open —
    /// auto-clear never removes the row under edit.
    edit_form_open: Option<String>,

    /// T-12: one confirmed rule-set download at a time; the explicit per-file
    /// confirm never batches.
    rule_set_download_in_flight: bool,

    import_parse_in_flight: bool,

    import_parse_epoch: u64,

    import_cancel_epoch: Option<u64>,

    close_confirmed: bool,

    tray: Option<crate::tray::TrayHandle>,

    last_snapshot: Vec<String>,

    start_in_flight: bool,

    ticks_started: bool,

    sidecar_presence: Vec<String>,

    _libraries_subscription: Subscription,

    facet_group: FacetPicker,

    search_input: Entity<InputState>,
    _search_subscription: Subscription,

    row_columns: RowColumns,

    inspector_tab: InspectorTab,

    /// T-03: the split's own state. The inspector's width is session-only —
    /// nothing about it is persisted — but the table's column budget has to
    /// follow the drag, so the owner reads the live size from here.
    split_state: Entity<ResizableState>,

    /// F19: ONE flag owns the inspector's presence in both presentations —
    /// docked (≥1080) and the slide-over drawer below it. It starts closed;
    /// picking a row or an inspector shortcut opens it, Esc or the close
    /// button shuts it. Esc hands focus back to the list.
    inspector_visible: bool,

    conn: ConnectionState,

    palette: Option<Entity<CommandState>>,

    traffic_period: store::TrafficPeriod,

    /// T-30: per-shard CPU (% of one core) / RAM (MB) rings, keyed by the
    /// shard label `poll_shards` uses. One entry per live shard, 60 samples.
    shard_rings: BTreeMap<String, ShardRing>,

    /// T-30: previous raw counters per label — the delta baseline for the next
    /// engine tick. Dropped with the shard, re-primed on respawn.
    shard_baselines: BTreeMap<String, (u32, ProcessSample, Instant)>,

    /// T-30: shards whose handle the OS refused on the last tick.
    shard_denied: HashSet<String>,

    /// T-30: the read-only sampler seam; production reads Win32, tests inject.
    process_sampler: ProcessSampler,

    started_at: std::time::Instant,
}

/// T-08: why this session is running in memory instead of on disk. The banner
/// names `folder` and carries `reason` in its tooltip; the pair comes from the
/// open failure itself, because an in-memory store has no folder to probe.
struct WriteFallback {
    folder: Option<PathBuf>,
    reason: String,
}

/// T-08: the folder a failed open was trying to use, when the path has one.
fn store_folder(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn install_crash_hook() {
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

fn append_crash_report(path: &std::path::Path, report: &str) -> std::io::Result<()> {
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

fn main() {
    install_crash_hook();
    let _ = crate::tray::clear_dead_autostart_target();

    let launch_intake = std::env::args()
        .skip(1)
        .find_map(|arg| classify_open_with(&arg));
    let startup_geometry = stored_window_geometry();
    // F13: with no asset source every kit icon renders as an empty box, which
    // is what made the sheet header close unreadable.
    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);

        bind_app_keys(cx);
        let startup_bounds = startup_window_bounds(
            startup_geometry,
            cx.primary_display().map(|display| display.visible_bounds()),
        );
        cx.spawn(async move |cx| {
            cx.open_window(app_window_options(startup_bounds), |window, cx| {
                let view = cx.new(|cx| AppView::new(window, cx));
                view.update(cx, |view, cx| {
                    view.refresh_due_on_startup_deferred(cx);
                    view.start_background_ticks(cx);
                });

                view.update(cx, |view, cx| {
                    view.create_tray(window, cx);
                    view.restore_on_launch(window, cx);

                    if crate::tray::launched_minimized() {
                        view.hide_to_tray(window, cx);
                    }

                    if let Some(intake) = launch_intake.clone() {
                        view.handle_launch_intake(intake, window, cx);
                    }
                });

                let has_tray = view.read_with(cx, |view, _| view.tray.is_some());
                if has_tray {
                    let poll_view = view.clone();
                    let poll_handle = window.window_handle();
                    cx.spawn(async move |cx| loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(TRAY_PUMP_MILLIS))
                            .await;

                        let tick = cx.update_window(poll_handle, |_, window, cx| {
                            poll_view.update(cx, |view, cx| view.poll_tray_once(window, cx))
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
                }

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
                                            .ok_text("Stop all and close")
                                            .ok_variant(ButtonVariant::Danger),
                                    )
                                    .title("Close ManyProxy?")
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

#[cfg(test)]
mod tests;
