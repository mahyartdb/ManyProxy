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
use gpui_kit::component::badge::Badge;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariant, ButtonVariants};
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
use gpui_kit::component::notification::{Notification, NotificationType};
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
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Sizable;
use gpui_kit::component::TitleBar;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt};
use gpui_kit::*;
use qrcodegen::{QrCode, QrCodeEcc};

use engine::{
    engine_binary_file_name, fork_binary_file_name, is_custom_config_protocol,
    lan_confirm_description, lan_confirm_text, lan_exposure_warning, requires_lan_confirm,
    resolve_engine_binary, resolve_fork_binary, resolve_xray_binary, strip_ansi_codes,
    xray_binary_file_name, AutoRestartConfig, EngineController, EngineError, LAN_CONFIRM_TITLE,
};
use health::{
    clamp_log_max_lines, format_speed_result, outcome_is_neutral, ping_all, redact_secrets,
    resolve_hostname, sparkline_caption_tone, speed_group, test_all, test_group, truncate,
    url_test_sparkline, HealthKind, HealthProber, HealthStore, LatencyTone, LogLevel, LogStore,
    LogTagIndex, ProbeExclusions, RealHealthProber, SpeedtestConfig, SpeedtestMode,
    SpeedtestOutcome, TestOutcome, TrafficSeenMap, DEFAULT_TEST_URL, RESOLVE_IP_URL,
    RESOLVE_TIMEOUT, SPEED_TEST_TIMEOUT,
};
use import::{
    classify_open_with, commit_batch, effective_hint_for_text, format_import_failures,
    import_file_content, import_file_size_ok, import_multi_file_contents, import_text,
    is_addsub_attempt, is_library_export, parse_addsub_link, reject_file_bytes, AddSubRequest,
    ImportError, OpenWithKind,
};
use store::{
    chain_hop_rejection, chain_nodes_of, clear_group_tests_confirm_text, compare_type_security,
    count_importable, delete_confirm_text, delete_running_confirm_text, due_for_refresh_for,
    export_confirm_description, extra_config_prefill, fold_amneziawg_into_params,
    fold_anytls_into_params, fold_chain_nodes_into_params, fold_dial_into_params,
    fold_extra_into_params, fold_guided_into_params, fold_http_into_params,
    fold_hysteria_into_params, fold_mux_into_params, fold_naive_into_params,
    fold_shadowsocks_inner_into_params, fold_shadowtls_into_params, fold_snell_into_params,
    fold_socks_into_params, fold_ssh_into_params, fold_tls_forced_into_params,
    fold_tls_optional_into_params, fold_tls_vmess_into_params, fold_transport_into_params,
    fold_trojan_inner_into_params, fold_tuic_into_params, fold_vless_inner_into_params,
    fold_vmess_inner_into_params, fold_wireguard_into_params, fold_xray_into_params,
    guided_identity_fields, guided_prefill_bool, guided_prefill_list, guided_prefill_value,
    is_virtual_protocol, now_unix, pristine_template_rewrite, protocol_params_template, proxy_urls,
    prune_duplicates_confirm_text, prune_invalid_confirm_text, prune_unavailable_confirm_text,
    rewrite_server_to_ip, row_security_label, row_server_hostname, validate_extra_params,
    vmess_cipher_value, vmess_tls_enabled, AmneziaFold, AnytlsFold, AutoClearGroupConfig,
    ChainHopSlot, CloseAction, ConfigPatch, DialFold, ExtraFold, GroupPruneKind, HealthSnapshot,
    HttpFold, HysteriaFold, LibrarySort, LibrarySortKey, MuxFold, NaiveFold, NewConfig,
    ProxyConfig, RestoreOutcome, ShadowTlsFold, SnellFold, SocksFold, SortDirection, SshFold,
    Store, StoreError, Subscription as ProxySubscription, TestAllScope, TlsForcedFold,
    TlsOptionalFold, TransportFold, TuicFold, WindowGeometry, WireguardFold, XrayFold,
    DEFAULT_BIND, DEFAULT_GROUP, DEFAULT_PROBE_TIMEOUT_SECS, DEFAULT_PROTOCOL,
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

use app::open::*;
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
        InspectorPresets,
        InspectorLogs,
        InspectorConnections,
        InspectorTraffic,
        OpenPalette,
        CopyUrls,
        UpdateCurrentGroup
    ]
);

fn shortcut_kbd(keystroke: &str, fallback: &str) -> AnyElement {
    match Keystroke::parse(keystroke) {
        Ok(stroke) => Kbd::new(stroke).into_any_element(),
        Err(_) => fallback.to_string().into_any_element(),
    }
}

fn overlay_open(window: &mut Window, cx: &mut App) -> bool {
    window.has_active_dialog(cx) || window.has_active_sheet(cx)
}

/// T-06: the copy a refused shortcut shows, so the key never looks dead.
pub(crate) const OVERLAY_REFUSAL_NOTE: &str = "Close the open dialog first";

/// T-06: a shortcut that cannot run because an overlay owns the keyboard says
/// so out loud instead of looking like a dead key.
pub(crate) fn refuse_shortcut_under_overlay(window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(OVERLAY_REFUSAL_NOTE), cx);
}

/// T-06: the shell's focus handle, published once per window, so every
/// overlay-dismiss path can hand keyboard focus back without threading a
/// `FocusHandle` through each dialog builder and footer closure.
struct ShellFocus {
    window: WindowId,
    handle: FocusHandle,
}

impl Global for ShellFocus {}

/// T-06: the end of every overlay-dismiss path. The shell reclaims keyboard
/// focus only when no overlay is open and no element holds focus - a reclaim
/// that fires while the user has already clicked elsewhere, or while a second
/// overlay has just opened, steals the keystroke they were aiming.
pub(crate) fn reclaim_shell_focus(window: &mut Window, cx: &mut App) {
    if overlay_open(window, cx) {
        return;
    }
    if window.focused(cx).is_some() {
        return;
    }
    focus_shell(window, cx);
}

/// T-06: before an overlay opens, the shell takes focus when nothing else
/// holds it. `Root::open_dialog` remembers `window.focused(cx)` and hands that
/// handle back when the layer closes, so this is what makes Esc, a backdrop
/// press, the close button and a footer commit return keyboard control to the
/// shell instead of leaving the window with a dead focus id.
pub(crate) fn focus_shell_before_overlay(window: &mut Window, cx: &mut App) {
    if window.focused(cx).is_none() {
        focus_shell(window, cx);
    }
}

fn focus_shell(window: &mut Window, cx: &mut App) {
    let Some(shell) = cx.try_global::<ShellFocus>() else {
        return;
    };
    if shell.window != window.window_handle().window_id() {
        return;
    }
    let handle = shell.handle.clone();
    window.focus(&handle, cx);
}

fn topbar_menu_row(
    id: &'static str,
    label: &'static str,
    accessible: &'static str,
    tip: &'static str,
    shortcut: Option<(&'static str, &'static str, &'static str)>,
) -> AnyElement {
    match shortcut {
        Some((parse, chip, aria)) => div()
            .id(id)
            .test_support()
            .role(Role::MenuItem)
            .aria_label(accessible)
            .aria_keyshortcuts(aria)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip)).into())
            .child(label)
            .child(shortcut_kbd(parse, chip))
            .into_any_element(),
        None => div()
            .id(id)
            .test_support()
            .role(Role::MenuItem)
            .aria_label(accessible)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip)).into())
            .child(label)
            .into_any_element(),
    }
}

const SEARCH_PLACEHOLDER: &str = "Search configurations";

/// T-23: the one hint that the `port=` grammar exists — a tooltip on the search
/// box, because the pinned placeholder is too narrow to carry it.
const SEARCH_HINT: &str =
    "port=N or port=N:M filters by listen port (port=10801, port=10800:10900, port=10801:, port=:10900)";

/// Store meta key for "Automatically restart crashed cores"; a missing or corrupt
/// value reads as enabled, matching the engine's default.
const META_AUTO_RESTART: &str = "auto_restart_enabled";

fn is_row_local_start_error(error: &EngineError) -> bool {
    crate::engine::is_row_local_error(error)
}

fn normalize_test_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("test URL must not be empty".to_string());
    }
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .ok_or_else(|| "test URL must start with http:// or https://".to_string())?;
    let host = rest.split('/').next().unwrap_or("").trim();
    if host.is_empty() {
        return Err("test URL needs a host".to_string());
    }
    Ok(trimmed.to_string())
}

fn effective_probe_url(row_override: Option<&str>, global: Option<&str>) -> String {
    row_override
        .filter(|url| !url.trim().is_empty())
        .or_else(|| global.filter(|url| !url.trim().is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_TEST_URL.to_string())
}

fn format_params_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("params are not valid JSON: {error}"))?;
    serde_json::to_string_pretty(&value)
        .map_err(|error| format!("params cannot be formatted: {error}"))
}

fn validate_params_json(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map(|_| ())
        .map_err(|error| format!("params are not valid JSON: {error}"))
}

fn guided_identity_label(protocol_raw: &str) -> String {
    match guided_identity_fields(protocol_raw).and_then(|guided| guided.key) {
        Some(key) => format!("Identity ({key})"),
        None => "Identity".to_string(),
    }
}

fn guided_hint_line(protocol_raw: &str) -> Option<String> {
    let name = protocol_raw.trim();
    let guided = guided_identity_fields(protocol_raw)?;
    if name.is_empty() {
        return None;
    }
    match guided.key {
        Some(_) => Some(format!("{name} needs {}.", guided.needs)),
        None if guided.needs == "custom JSON" => Some(format!("{name} takes custom JSON.")),
        None => Some(format!("No secret needed for {name}.")),
    }
}

fn block_owns_identity(protocol_raw: &str) -> bool {
    matches!(
        protocol_raw.trim(),
        "vless"
            | "vmess"
            | "trojan"
            | "shadowsocks"
            | "wireguard"
            | "amneziawg"
            | "hysteria"
            | "hysteria2"
            | "tuic"
            | "anytls"
            | "naive"
            | "ssh"
            | "snell"
            | "xray"
    )
}

fn tls_panel_plan(protocol_raw: &str) -> Option<bool> {
    match protocol_raw.trim() {
        "vless" | "vmess" | "trojan" | "http" | "https" => Some(false),
        "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls" => Some(true),
        _ => None,
    }
}

fn has_network_panel(protocol_raw: &str) -> bool {
    matches!(protocol_raw.trim(), "vless" | "vmess" | "trojan")
}

fn has_dial_panel(protocol_raw: &str) -> bool {
    matches!(
        protocol_raw.trim(),
        "vless"
            | "vmess"
            | "trojan"
            | "shadowsocks"
            | "wireguard"
            | "amneziawg"
            | "hysteria"
            | "hysteria2"
            | "tuic"
            | "socks"
            | "http"
            | "https"
            | "anytls"
            | "naive"
            | "ssh"
            | "snell"
            | "shadowtls"
    )
}

fn has_advanced_dialog(protocol_raw: &str) -> bool {
    has_dial_panel(protocol_raw) || tls_panel_plan(protocol_raw).is_some()
}

pub const INTERVAL_PRESETS: &[i64] = &[0, 3_600, 21_600, 43_200, 86_400, 604_800];

pub fn summed_traffic_text(store: &Store) -> Option<String> {
    let totals = store.traffic_totals().ok()?;
    let (mut rx, mut tx) = (0u64, 0u64);
    for (row_rx, row_tx) in totals.values() {
        rx = rx.saturating_add(*row_rx);
        tx = tx.saturating_add(*row_tx);
    }
    Some(format!("↓ {} · ↑ {}", format_bytes(rx), format_bytes(tx)))
}

fn interval_picker_row(
    id_prefix: &str,
    interval_secs: Rc<RefCell<i64>>,
    repaint: DialogRepaint,
) -> impl IntoElement {
    let current = *interval_secs.borrow();
    let selected = INTERVAL_PRESETS
        .iter()
        .position(|preset| *preset == current);
    let mut group = RadioGroup::horizontal(format!("{id_prefix}-options"))
        .selected_index(selected)
        .on_change(move |ix, window, cx| {
            if let Some(secs) = INTERVAL_PRESETS.get(*ix) {
                *interval_secs.borrow_mut() = *secs;
            }
            repaint(window, cx);
        });
    for preset in INTERVAL_PRESETS {
        let label = format_interval_secs(*preset);
        group = group.child(
            Radio::new(label.clone())
                .label(label.clone())
                .tooltip(format!("Refresh every {label}")),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .id(format!("{id_prefix}-caption"))
                .test_support()
                .child(Label::new("Refresh interval")),
        )
        .child(
            div()
                .id(format!("{id_prefix}-group"))
                .test_support()
                .child(group),
        )
}

fn spawn_tick(
    cx: &mut Context<AppView>,
    secs: u64,
    tick: impl Fn(&mut AppView, &mut Context<AppView>) + 'static,
) {
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_entity, cx| loop {
        cx.background_executor()
            .timer(Duration::from_secs(secs.max(1)))
            .await;
        let live = cx.update(|cx| {
            weak.update(cx, |view, _| view.ticks_enabled)
                .unwrap_or(false)
        });
        if !live {
            break;
        }
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
    })
    .detach();
}

type EchoSelect<'a> = (
    &'a Entity<SelectState<PanelChoiceList>>,
    &'a [&'a str],
    &'a str,
);

const SHARD_NOT_RUNNING: &str = "not running";
const SHARD_SAMPLING_UNAVAILABLE: &str = "(sampling unavailable)";
const SHARD_COLLECTING: &str = "collecting first sample";

/// One row of the Runtime-stats per-shard section: either two plottable series
/// or a caption saying why there is nothing to plot.
#[derive(Debug, Clone, PartialEq)]
struct ShardStatRow {
    label: String,
    cpu: Vec<f32>,
    ram: Vec<f32>,
    caption: Option<&'static str>,
}

fn shard_stat_rows(
    shards: &[(String, Option<u32>)],
    rings: &BTreeMap<String, ShardRing>,
    denied: &HashSet<String>,
) -> Vec<ShardStatRow> {
    shards
        .iter()
        .map(|(label, pid)| {
            let ring = rings.get(label);
            let caption = if pid.is_none() {
                Some(SHARD_NOT_RUNNING)
            } else if denied.contains(label) {
                Some(SHARD_SAMPLING_UNAVAILABLE)
            } else if ring.is_none_or(|ring| ring.cpu.len() < 2) {
                Some(SHARD_COLLECTING)
            } else {
                None
            };
            ShardStatRow {
                label: label.clone(),
                cpu: ring.map(|ring| ring.cpu.clone()).unwrap_or_default(),
                ram: ring.map(|ring| ring.ram.clone()).unwrap_or_default(),
                caption,
            }
        })
        .collect()
}

/// Sparkline-shaped line for one series: same `LineChart` family and sizing as
/// the health sparkline, one per series so CPU and RAM never share a scale.
fn shard_sparkline(series: &[f32]) -> impl IntoElement {
    struct SeriesPoint {
        x: String,
        y: f64,
    }
    let points: Vec<SeriesPoint> = series
        .iter()
        .enumerate()
        .map(|(index, value)| SeriesPoint {
            x: index.to_string(),
            y: f64::from(*value),
        })
        .collect();
    div().w(px(120.)).h(px(24.)).child(
        LineChart::new(points)
            .x(|point: &SeriesPoint| point.x.clone())
            .y(|point: &SeriesPoint| point.y)
            .x_axis(false)
            .grid(false),
    )
}

fn shard_stat_row_view(row: &ShardStatRow) -> AnyElement {
    let mut entry = div()
        .id(format!("shard-stat-{}", row.label))
        .test_support()
        .flex()
        .flex_col()
        .gap_1();
    match row.caption {
        Some(caption) => {
            entry = entry.child(
                div()
                    .id(format!("shard-caption-{}", row.label))
                    .test_support()
                    .text_xs()
                    .child(format!("{}: {caption}", row.label)),
            );
        }
        None => {
            let latest_cpu = row.cpu.last().copied().unwrap_or(0.0);
            let latest_ram = row.ram.last().copied().unwrap_or(0.0);
            entry = entry
                .child(
                    div()
                        .id(format!("shard-label-{}", row.label))
                        .test_support()
                        .text_xs()
                        .child(format!(
                            "{} · CPU {latest_cpu:.1}% of one core · RAM {latest_ram:.1} MB",
                            row.label
                        )),
                )
                .child(
                    div()
                        .id(format!("shard-chart-{}", row.label))
                        .test_support()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(shard_sparkline(&row.cpu))
                        .child(shard_sparkline(&row.ram)),
                );
        }
    }
    entry.into_any_element()
}

/// Read one child's counters. Windows-only, same posture as `dpapi`: raw
/// `extern "system"` declarations, no crate, and the handle is closed on every
/// path.
#[cfg(all(windows, not(test)))]
mod win32_process {
    use std::ffi::c_void;

    use super::{
        ProcessSample, SampleRead, ERROR_ACCESS_DENIED, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        /// `FILETIME` counts 100 ns ticks in two halves.
        fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }

    /// `PROCESS_MEMORY_COUNTERS` — only `WorkingSetSize` is read; the rest
    /// keeps the layout the callee fills.
    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        _page_fault_count: u32,
        _peak_working_set_size: usize,
        working_set_size: usize,
        _quota_peak_paged_pool_usage: usize,
        _quota_paged_pool_usage: usize,
        _quota_peak_non_paged_pool_usage: usize,
        _quota_non_paged_pool_usage: usize,
        _pagefile_usage: usize,
        _peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    /// Owns the query handle; dropping it is the only side effect in here.
    struct OwnedHandle(*mut c_void);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) fn sample(pid: u32) -> SampleRead {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return if GetLastError() == ERROR_ACCESS_DENIED {
                    SampleRead::Denied
                } else {
                    SampleRead::Gone
                };
            }
            let handle = OwnedHandle(handle);

            let mut creation = FileTime::default();
            let mut exit = FileTime::default();
            let mut kernel = FileTime::default();
            let mut user = FileTime::default();
            if GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
                return SampleRead::Gone;
            }

            let cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
            let mut counters = ProcessMemoryCounters {
                cb,
                ..Default::default()
            };
            if GetProcessMemoryInfo(handle.0, &mut counters, cb) == 0 {
                return SampleRead::Gone;
            }

            SampleRead::Sampled(ProcessSample {
                cpu_100ns: kernel.ticks().saturating_add(user.ticks()),
                ram_bytes: counters.working_set_size as u64,
            })
        }
    }
}

#[cfg(all(windows, not(test)))]
fn win32_process_sample(pid: u32) -> SampleRead {
    win32_process::sample(pid)
}

/// Tests inject their own readings (no handle ever opens under `cargo test`),
/// and non-Windows builds have no Win32 to ask — both degrade to the caption
/// instead of pretending a number exists.
#[cfg(any(test, not(windows)))]
fn win32_process_sample(_pid: u32) -> SampleRead {
    SampleRead::Denied
}

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
    logs: LogStore,
    log_filter: String,

    log_level: Option<LogLevel>,
    log_tail: bool,
    logs_open: bool,

    logs_seen: u64,

    log_inputs: Option<(Entity<InputState>, Entity<InputState>, Entity<InputState>)>,
    log_cap_input: Option<Entity<InputState>>,

    /// T-04: the Settings dialog's Enter-commit subscriptions. The dialog owns
    /// its text fields, so the subscriptions live as long as it does.
    settings_subscriptions: Vec<Subscription>,

    /// T-04: the timestamp of the last update check, mirrored into the open
    /// Settings dialog so the Updates section can show it.
    settings_last_check: Option<Rc<RefCell<Option<i64>>>>,

    /// Captured just before a control-driven sheet swap. The live input text
    /// wins over the committed filter fields so typed-but-unapplied text
    /// survives the swap, then is consumed once on the next open.
    log_reopen_draft: Option<(String, String, String)>,

    log_list: Option<Entity<ListState<LogLinesDelegate>>>,

    expanded_subs: HashSet<String>,

    /// T-27: resolved out-IP text per row id, session-only. The resolution
    /// action already computes it; keeping it per row is what `out_ip` sorts by.
    out_ip_by_row: HashMap<String, String>,

    traffic_last_seen: TrafficSeenMap,

    traffic_tick_in_flight: bool,

    traffic_mirror_hydrated: bool,

    prober: Box<dyn HealthProber + Send + Sync>,
    ticks_enabled: bool,

    refresh_batch_total: usize,
    refresh_batch_members: HashSet<String>,
    refresh_batch_added: usize,
    refresh_batch_updated: usize,
    refresh_batch_removed: usize,
    refresh_batch_failed: usize,
    refresh_batch_conflicts: usize,
    refresh_batch_in_use: usize,
    refresh_batch_added_names: Vec<String>,
    refresh_batch_updated_names: Vec<String>,
    refresh_batch_removed_names: Vec<String>,
    refresh_batch_in_use_names: Vec<String>,
    refresh_generation: u64,
    inflight_refreshes: HashSet<String>,
    /// T-20: refreshes whose completion carries the named `[+]/[~]/[-]/[=]`
    /// report — set for manual origins only, so due-tick refreshes finish
    /// count-only.
    inflight_named_refreshes: HashSet<String>,

    probe_url_in_flight: bool,
    probe_ping_in_flight: bool,

    update_check_in_flight: bool,

    probe_stop: Arc<AtomicBool>,

    group_test_in_flight: bool,

    speedtest_mode: SpeedtestMode,

    subs_form: Option<SubForm>,

    preset_form: Option<PresetForm>,

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
    facet_protocol: FacetPicker,
    facet_status: FacetPicker,

    search_input: Entity<InputState>,
    _search_subscription: Subscription,

    row_columns: RowColumns,

    inspector_tab: InspectorTab,

    /// T-03: the split's own state. The inspector's width is session-only —
    /// nothing about it is persisted — but the table's column budget has to
    /// follow the drag, so the owner reads the live size from here.
    split_state: Entity<ResizableState>,

    /// T-03: below the drawer breakpoint the inspector slides over the table
    /// instead of squeezing it. Esc closes it and hands focus back to the
    /// list; picking a row opens it again.
    inspector_drawer_open: bool,

    conn_snapshot: Vec<(String, crate::health::ClashConn)>,

    /// T-25: counts derived from `conn_snapshot` on the traffic tick, so the
    /// status bar never walks the snapshot per frame.
    conn_counts: crate::health::ConnCounts,

    conn_delta: HashMap<String, (u64, u64)>,

    conn_selected_only: bool,

    conn_sort_desc: bool,

    conn_copied: Option<ConnCopyTarget>,

    conn_copied_generation: u64,

    last_traffic_poll_at: i64,

    log_exclude: String,

    log_regex: String,

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

impl AppView {
    /// Defect: boot runs inside `cx.open_window`'s closure, and the window only
    /// adopts its `gpui_component::Root` layer after that closure returns —
    /// `Root::new` does not install the first layer during it. A notification
    /// or dialog pushed there panics inside `Root::update` ("window first layer
    /// should be a gpui_component::Root"). Boot paths hand their overlay to
    /// these helpers, which land it on the first frame, after Root exists. The
    /// decision (message, severity, dialog content) is still taken at boot.
    fn after_root(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        open: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        cx.on_next_frame(window, move |view, window, cx| open(view, window, cx));
    }

    fn notify_after_root(
        &mut self,
        notification: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.after_root(window, cx, move |_view, window, cx| {
            window.push_notification(notification, cx);
        });
    }

    /// Whether the window has adopted its `gpui_component::Root` first layer
    /// yet. `Root::update` - which every `WindowExt` overlay call funnels
    /// through - does `window.root::<Root>().flatten().expect(..)`, so this is
    /// exactly the panic condition: it is false while `cx.open_window`'s
    /// closure is still running, and also for a window whose root view is not
    /// a `Root` at all (the test harness shape).
    fn root_installed(window: &Window) -> bool {
        window.root::<Root>().flatten().is_some()
    }

    /// A path that boots *and* runs later (`reload`): deliver now when the
    /// window already has its Root, so ordinary use keeps its exact timing,
    /// and hand the notification to the first frame during boot.
    fn notify_now_or_after_root(
        &mut self,
        notification: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::root_installed(window) {
            window.push_notification(notification, cx);
        } else {
            self.notify_after_root(notification, window, cx);
        }
    }

    fn with_store_inner(
        window: &mut Window,
        cx: &mut Context<Self>,
        store: Store,
        fallback: Option<WriteFallback>,
    ) -> Self {
        let mut store = store;
        // T-34: read the previous session's verdict first, then assume this
        // session is dirty. The write lands before any restore logic, so a
        // kill/crash leaves "0" (unclean) for the next boot.
        let previous_clean_exit = store.last_clean_exit();
        let last_snapshot = store.load_running_snapshot();
        if let Err(error) = store.set_last_clean_exit(false) {
            eprintln!("clean-exit marker reset failed: {error}");
        }
        let unclean_notice = unclean_exit_notice(previous_clean_exit, &last_snapshot);
        // T-33: the first read seeds the address list once (constants + every
        // row's current bind); every later read is the stored extras alone.
        let bind_hosts = store.bind_hosts();
        // T-04: the probe budget is the persisted one, not the built-in default.
        let probe_timeout_secs = store.probe_timeout_secs();
        let libraries = cx.new(|cx| {
            ListState::new(LibraryDelegate::new(), window, cx)
                .searchable(false)
                .selectable(true)
        });
        let last_query = store.library_last_query().unwrap_or_default();
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(SEARCH_PLACEHOLDER)
                .default_value(last_query.clone())
        });
        let row_columns = RowColumns::parse(&store.library_columns_raw().unwrap_or_default());
        let sort = store.library_sort();
        libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_row_columns(row_columns);
            state.delegate_mut().set_sort(sort);
            state.delegate_mut().query = last_query;
            state.delegate_mut().set_bind_hosts(bind_hosts);
            list_cx.notify();
        });

        let empty_delegate = LibraryDelegate::new();
        let (group_choices, group_current) = FacetMenu::Group.choices(&empty_delegate);
        let facet_group =
            FacetPicker::new(FacetMenu::Group, group_choices, group_current, window, cx);
        let (protocol_choices, protocol_current) = FacetMenu::Protocol.choices(&empty_delegate);
        let facet_protocol = FacetPicker::new(
            FacetMenu::Protocol,
            protocol_choices,
            protocol_current,
            window,
            cx,
        );
        let (status_choices, status_current) = FacetMenu::Status.choices(&empty_delegate);
        let facet_status = FacetPicker::new(
            FacetMenu::Status,
            status_choices,
            status_current,
            window,
            cx,
        );

        apply_appearance(&store.appearance(), Some(window), cx);

        {
            let our_ports: Vec<u16> = store
                .list()
                .map(|rows| rows.iter().map(|row| row.port).collect())
                .unwrap_or_default();
            crate::engine::production_startup_reap(&our_ports);
        }

        let sidecar_presence = [
            (engine_binary_file_name(), resolve_engine_binary().is_ok()),
            (fork_binary_file_name(), resolve_fork_binary().is_ok()),
            (xray_binary_file_name(), resolve_xray_binary().is_ok()),
        ]
        .into_iter()
        .map(|(file, found)| sidecar_presence_line(&file, found))
        .collect::<Vec<_>>();
        let log_cap = store.log_max_lines();
        // T-08: the first honest question after the store opens is whether its
        // folder accepts a write — a read-only folder reads fine and lets a
        // save evaporate. Probe once here; the tick and every failed save
        // re-probe the same state. A session that fell back to memory has no
        // folder to probe, so its open failure is the standing cause instead.
        let (write_blocked, write_blocked_folder, write_blocked_pinned) = match fallback {
            Some(fallback) => (Some(fallback.reason), fallback.folder, true),
            None => (store.write_probe().err(), None, false),
        };
        let split_state = cx.new(|_| ResizableState::default());
        let view = Self {
            focus: cx.focus_handle(),
            libraries: libraries.clone(),
            detail_scroll: ScrollHandle::new(),
            split_state,
            inspector_drawer_open: true,
            store,
            write_blocked,
            write_blocked_folder,
            write_blocked_pinned,
            engine: EngineController::production(),
            health_store: HealthStore::default(),
            logs: LogStore::with_capacity(log_cap),
            log_filter: String::new(),
            log_exclude: String::new(),
            log_regex: String::new(),
            log_level: None,
            log_tail: true,
            logs_open: false,
            logs_seen: 0,

            log_inputs: None,
            log_cap_input: None,
            settings_subscriptions: Vec::new(),
            settings_last_check: None,
            log_reopen_draft: None,

            log_list: None,
            expanded_subs: HashSet::new(),
            out_ip_by_row: HashMap::new(),
            traffic_last_seen: TrafficSeenMap::default(),
            traffic_tick_in_flight: false,
            traffic_mirror_hydrated: false,
            prober: Box::new(RealHealthProber::new(std::time::Duration::from_secs(
                probe_timeout_secs,
            ))),
            ticks_enabled: true,
            refresh_batch_total: 0,
            refresh_batch_members: HashSet::new(),
            refresh_batch_added: 0,
            refresh_batch_updated: 0,
            refresh_batch_removed: 0,
            refresh_batch_failed: 0,
            refresh_batch_conflicts: 0,
            refresh_batch_in_use: 0,
            refresh_batch_added_names: Vec::new(),
            refresh_batch_updated_names: Vec::new(),
            refresh_batch_removed_names: Vec::new(),
            refresh_batch_in_use_names: Vec::new(),
            refresh_generation: 0,
            inflight_refreshes: HashSet::new(),
            inflight_named_refreshes: HashSet::new(),
            probe_url_in_flight: false,
            probe_ping_in_flight: false,

            update_check_in_flight: false,
            probe_stop: Arc::new(AtomicBool::new(false)),
            group_test_in_flight: false,
            speedtest_mode: SpeedtestMode::Download,
            subs_form: None,
            preset_form: None,
            edit_form_open: None,
            rule_set_download_in_flight: false,
            import_parse_in_flight: false,
            import_parse_epoch: 0,
            import_cancel_epoch: None,
            close_confirmed: false,
            tray: None,
            last_snapshot,
            start_in_flight: false,
            ticks_started: false,
            sidecar_presence,
            _libraries_subscription: cx.subscribe(
                &libraries,
                |_view, _state, _event: &ListEvent, cx| {
                    cx.notify();
                },
            ),
            search_input: search_input.clone(),
            _search_subscription: cx.subscribe(
                &search_input,
                |view: &mut Self, emitter: Entity<InputState>, event: &InputEvent, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let query = emitter.read(cx).value().to_string();
                    view.remember_library_query(&query);
                    view.libraries.update(cx, |state, list_cx| {
                        let delegate = state.delegate_mut();
                        delegate.query = query;
                        delegate.apply_filter();
                        list_cx.notify();
                    });
                    cx.notify();
                },
            ),
            facet_group,
            facet_protocol,
            facet_status,
            row_columns,
            inspector_tab: InspectorTab::Details,
            conn_snapshot: Vec::new(),
            conn_counts: crate::health::ConnCounts::default(),
            conn_delta: HashMap::new(),
            conn_selected_only: true,
            conn_sort_desc: true,
            conn_copied: None,
            conn_copied_generation: 0,
            last_traffic_poll_at: 0,
            palette: None,
            traffic_period: store::TrafficPeriod::Day,
            shard_rings: BTreeMap::new(),
            shard_baselines: BTreeMap::new(),
            shard_denied: HashSet::new(),
            process_sampler: Box::new(win32_process_sample),
            started_at: std::time::Instant::now(),
        };
        let mut view = view;
        // T-06: publish the shell's focus handle once per window so every
        // overlay-dismiss path can reclaim keyboard focus without threading
        // the handle through each dialog builder and footer closure.
        cx.set_global(ShellFocus {
            window: window.window_handle().window_id(),
            handle: view.focus.clone(),
        });
        if let Some(notice) = unclean_notice {
            // The notification layer needs the Root view, which the window only
            // adopts after this constructor returns: the decision was taken
            // above at boot, the toast lands on the first frame — once.
            view.notify_after_root(notice, window, cx);
        }
        view.apply_engine_auto_restart();
        let rows = match view.store.list() {
            Ok(rows) => rows,

            Err(error) => {
                // Boot path (this constructor runs inside `cx.open_window`'s
                // closure): the window has no Root yet, so the toast lands on
                // the first frame instead of panicking in `Root::update`.
                view.notify_after_root(
                    Notification::error(db_list_error_message(&error)),
                    window,
                    cx,
                );
                Vec::new()
            }
        };

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

            list_cx.notify();
        });

        view.sync_facet_pickers(window, cx);
        cx.notify();
        view
    }

    fn config_count(&self, cx: &App) -> usize {
        self.libraries.read(cx).delegate().counts().0
    }

    fn selected_config(&self, cx: &App) -> Option<ProxyConfig> {
        self.libraries.read(cx).delegate().selected_row()
    }

    fn cached_upstream(&self, cx: &App, config: &ProxyConfig) -> String {
        self.libraries
            .read(cx)
            .delegate()
            .upstream_for(&config.id)
            .map(str::to_string)
            .unwrap_or_else(|| {
                upstream_host_port(config).unwrap_or_else(|| "no upstream".to_string())
            })
    }

    /// T-08: one probe, one state write. Returns true when the standing
    /// condition changed, so the tick only asks for a repaint when there is
    /// something new to paint.
    fn refresh_write_blocked(&mut self, cx: &mut Context<Self>) -> bool {
        // T-08: a failed open pins the banner. A probe can prove the folder
        // writable again, but this session is still in memory: clearing the
        // banner would promise saves that still cannot happen. Only a restart
        // that opens the file store clears it.
        if self.write_blocked_pinned {
            return false;
        }
        let blocked = self.store.write_probe().err();
        if blocked == self.write_blocked {
            return false;
        }
        self.write_blocked = blocked;
        cx.notify();
        true
    }

    /// T-08: a failed save may be the folder, not the row. The probe is the
    /// honest arbiter: a folder that refuses a write gets the standing banner
    /// (a toast would vanish); any other failure keeps its own honest toast.
    fn report_save_error(
        &mut self,
        error: StoreError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(reason) = self.store.write_probe() {
            self.write_blocked = Some(reason);
            cx.notify();
            return;
        }
        window.push_notification(Notification::error(format!("Cannot save: {error}")), cx);
    }

    fn list_or_notify(&self, window: &mut Window, cx: &mut App) -> Option<Vec<ProxyConfig>> {
        match self.store.list() {
            Ok(rows) => Some(rows),
            Err(error) => {
                window.push_notification(Notification::error(db_list_error_message(&error)), cx);
                None
            }
        }
    }

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

    fn format_dialog_params(
        &mut self,
        params: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = params.read(cx).value().to_string();
        if current.trim().is_empty() {
            window.push_notification(
                Notification::info("Params are blank — the protocol default applies"),
                cx,
            );
            return;
        }
        match format_params_json(&current) {
            Ok(pretty) => {
                params.update(cx, |state, cx| {
                    state.set_value(pretty, window, cx);
                });
            }
            Err(reason) => {
                window.push_notification(Notification::error(reason), cx);
            }
        }
    }

    fn template_dialog_params(
        &mut self,
        params: &Entity<TextareaState>,
        form: &ConfigForm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = form.protocol_text(cx);
        let template = protocol_params_template(&name);
        params.update(cx, |state, cx| {
            state.set_value(template, window, cx);
        });
        let shown = name.trim();
        window.push_notification(
            Notification::info(format!(
                "Loaded {} template — fill the blank fields",
                if shown.is_empty() { "default" } else { shown }
            )),
            cx,
        );
    }

    fn validate_dialog_params(
        &mut self,
        params: &Entity<TextareaState>,
        protocol: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = params.read(cx).value().to_string();
        match validate_params_json(&current) {
            Ok(()) => {}
            Err(reason) => {
                window.push_notification(Notification::error(reason), cx);
                return false;
            }
        }

        if protocol == "extra-core" {
            if let Err(reason) = validate_extra_params(&current) {
                window.push_notification(Notification::error(reason), cx);
                return false;
            }
        }
        window.push_notification(
            Notification::success(if current.trim().is_empty() {
                "Params blank — the protocol default applies"
            } else {
                "Params are valid JSON"
            }),
            cx,
        );
        true
    }

    fn row_config(&self, id: &str) -> Option<ProxyConfig> {
        self.store.get(id).ok().flatten()
    }

    /// Surfaces at most one engine-crash notification and drives the auto-restart
    /// policy. The pending cause lives in the controller, so this still sees a
    /// crash the 5 s tick already drained from the backend.
    fn service_engine_crash(&mut self) -> Option<Notification> {
        let now = std::time::Instant::now();
        let detail = self.engine.pending_crash();
        self.engine.settle_restart_budget(now);
        if self.engine.auto_restart_due(now) {
            self.engine.note_auto_restart_attempt(now);
            let notification = match self.engine.restart_crashed() {
                Ok(()) => Notification::success(format!(
                    "Engine recovered ({})",
                    detail.unwrap_or_else(|| "crashed cores restarted".to_string())
                )),
                Err(error) => Notification::error(format!(
                    "Engine auto-restart failed: {}",
                    redact_secrets(&error.to_string())
                )),
            };
            self.engine.clear_pending_crash();
            return Some(notification);
        }
        let detail = detail?;
        self.engine.clear_pending_crash();
        Some(Notification::error(crash_notification_text(&detail)))
    }

    fn notify_engine_crash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(notification) = self.service_engine_crash() {
            // Reachable from `reload`, which the boot path calls before the
            // window adopts its Root layer — deliver it the same safe way.
            self.notify_now_or_after_root(notification, window, cx);
        }
    }

    fn push_engine_notification(&self, notification: Notification, cx: &mut Context<Self>) {
        if let Some(handle) = cx.active_window() {
            let _ = handle.update(cx, move |_, window, cx| {
                window.push_notification(notification, cx);
            });
        }
    }

    fn warn_for_lan_binds(&self, window: &mut Window, cx: &mut App) {
        for warning in self.engine.lan_warnings() {
            window.push_notification(Notification::warning(warning), cx);
        }
    }

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

    fn close_veto_text(&self) -> Option<String> {
        if self.close_confirmed {
            return None;
        }
        close_confirm_description(&self.running_names_for_close())
    }

    fn confirm_close_stop(&mut self, window: &Window) {
        self.engine.stop_all();
        if let Ok(rows) = self.store.list() {
            for row in &rows {
                if let Err(error) = self.store.set_enabled(&row.id, false) {
                    eprintln!("close flag clear failed for {}: {error}", row.id);
                }
            }
        }
        self.snapshot_running_set(&[]);
        self.mark_clean_exit();
        self.save_window_geometry(window);
        self.close_confirmed = true;
    }

    /// T-34: every orderly exit writes the marker right after its snapshot
    /// clear. A kill/crash never reaches here, so boot's "0" stands.
    fn mark_clean_exit(&mut self) {
        if let Err(error) = self.store.set_last_clean_exit(true) {
            eprintln!("clean-exit marker failed: {error}");
        }
    }

    /// T-26: mirrors the running count into the OS window title on the exact
    /// cadence the tray tooltip is fed from — no extra poll or timer. The
    /// custom titlebar label composes the same `window_title_text` at render.
    fn sync_running_title(&self, running: usize, cx: &mut Context<Self>) {
        let title = window_title_text(running);
        if let Some(handle) = cx.active_window() {
            let _ = handle.update(cx, move |_, window, _| window.set_window_title(&title));
        }
    }

    fn hide_to_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(windows)]
        match crate::tray::hwnd_from_window(window) {
            Some(hwnd) => {
                crate::tray::hide_window_handle(hwnd);
            }

            None => window.minimize_window(),
        }
        #[cfg(not(windows))]
        window.minimize_window();
        if let Some(tray) = &self.tray {
            tray.set_running(self.engine.running_count());
        }
        cx.notify();
    }

    fn show_window_from_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(windows)]
        if let Some(hwnd) = crate::tray::hwnd_from_window(window) {
            crate::tray::show_window_handle(hwnd);
        }
        window.activate_window();
        cx.notify();
    }

    fn poll_tray_once(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.tray.is_none() {
            return false;
        }

        let mut actions = Vec::new();
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if let Some(action) = crate::tray::action_for_menu_id(event.id.as_ref()) {
                actions.push(action);
            }
        }
        if actions.contains(&crate::tray::TrayAction::Quit) {
            self.quit_from_tray(window);
            return true;
        }
        for action in actions {
            self.handle_tray_action(action, window, cx);
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                }
            ) {
                self.handle_tray_action(crate::tray::TrayAction::Show, window, cx);
            }
        }
        false
    }

    fn handle_tray_action(
        &mut self,
        action: crate::tray::TrayAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            crate::tray::TrayAction::Show => self.show_window_from_tray(window, cx),
            crate::tray::TrayAction::SelectExit => {
                self.show_window_from_tray(window, cx);
                self.open_tray_exit_picker(window, cx);
            }
            crate::tray::TrayAction::SelectPreset => {
                self.show_window_from_tray(window, cx);
                self.open_presets_dialog(window, cx);
            }
            crate::tray::TrayAction::StartAll => {
                self.show_window_from_tray(window, cx);
                self.start_all_engines(window, cx);
            }
            crate::tray::TrayAction::StopAll => {
                self.show_window_from_tray(window, cx);
                self.stop_all_engines(window, cx);
            }
            crate::tray::TrayAction::OpenConfig => {
                self.show_window_from_tray(window, cx);
                self.copy_config_folder(window, cx);
            }
            crate::tray::TrayAction::Quit => self.quit_from_tray(window),
        }
    }

    fn quit_from_tray(&mut self, window: &Window) {
        self.engine.stop_all();
        if let Ok(rows) = self.store.list() {
            for row in &rows {
                if let Err(error) = self.store.set_enabled(&row.id, false) {
                    eprintln!("tray quit flag clear failed for {}: {error}", row.id);
                }
            }
        }
        self.snapshot_running_set(&[]);
        self.mark_clean_exit();
        self.save_window_geometry(window);

        self.tray = None;
    }

    /// One update check per click: GET the JSON feed once, then surface the
    /// compare result. Nothing auto-checks at startup or in the background —
    /// the ticket forbids it (checking ≠ updating; nothing is ever fetched
    /// besides this one JSON read).
    fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_check_in_flight {
            window.push_notification(Notification::info("An update check is already running"), cx);
            return;
        }
        let configured = self
            .store
            .update_feed_url()
            .filter(|value| !value.trim().is_empty());
        let feed_url = effective_update_feed_url(configured.as_deref(), APP_VERSION);
        self.update_check_in_flight = true;
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_release_feed(client, &feed_url, resolve_subscription_host).await;
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_update_check(fetched, window, cx);
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_update_check_no_window(fetched, cx);
                    });
                }
            });
        })
        .detach();
    }

    /// T-04: writes the check's timestamp into the store and mirrors it into
    /// the open Settings dialog.
    fn record_update_check(&mut self) {
        let now = crate::store::now_unix();
        if let Err(error) = self.store.set_last_update_check(now) {
            eprintln!("update-check timestamp failed: {error}");
        }
        if let Some(cell) = &self.settings_last_check {
            *cell.borrow_mut() = Some(now);
        }
    }

    fn palette_op_at(index: IndexPath) -> Option<PaletteOp> {
        match (index.section, index.row) {
            (0, 0) => Some(PaletteOp::Add),
            (0, 1) => Some(PaletteOp::Import),
            (0, 2) => Some(PaletteOp::StartSelected),
            (0, 3) => Some(PaletteOp::StopSelected),
            (0, 4) => Some(PaletteOp::TestSelected),
            (0, 5) => Some(PaletteOp::CopyUrls),
            (0, 6) => Some(PaletteOp::EditSelected),
            (0, 7) => Some(PaletteOp::DeleteSelected),
            (0, 8) => Some(PaletteOp::FocusSearch),
            (0, 9) => Some(PaletteOp::ClearChecked),
            (0, 10) => Some(PaletteOp::DetailsTab),
            (0, 11) => Some(PaletteOp::HealthTab),
            (0, 12) => Some(PaletteOp::PresetsTab),

            (0, 13) => Some(PaletteOp::ImportClipboard),
            (0, 14) => Some(PaletteOp::ExportLibrary),
            (1, 0) => Some(PaletteOp::Subscriptions),
            (1, 1) => Some(PaletteOp::PresetManager),
            (1, 2) => Some(PaletteOp::Groups),
            (1, 3) => Some(PaletteOp::Logs),
            (1, 4) => Some(PaletteOp::Settings),
            (1, 5) => Some(PaletteOp::About),
            (1, 6) => Some(PaletteOp::StartAll),
            (1, 7) => Some(PaletteOp::StopAll),
            (1, 8) => Some(PaletteOp::TestAll),

            (1, 9) => Some(PaletteOp::RuntimeStats),
            (1, 10) => Some(PaletteOp::TrafficStats),
            (1, 11) => Some(PaletteOp::RowColumns),
            _ => None,
        }
    }

    fn run_palette_op(&mut self, index: IndexPath, window: &mut Window, cx: &mut Context<Self>) {
        let Some(op) = Self::palette_op_at(index) else {
            return;
        };
        window.close_all_dialogs(cx);
        match op {
            PaletteOp::Add => window.dispatch_action(Box::new(AddConfig), cx),
            PaletteOp::Import => window.dispatch_action(Box::new(ImportConfigs), cx),
            PaletteOp::StartSelected => window.dispatch_action(Box::new(StartSelected), cx),
            PaletteOp::StopSelected => window.dispatch_action(Box::new(StopSelected), cx),
            PaletteOp::TestSelected => window.dispatch_action(Box::new(TestSelected), cx),
            PaletteOp::CopyUrls => window.dispatch_action(Box::new(CopyUrls), cx),
            PaletteOp::EditSelected => window.dispatch_action(Box::new(EditSelected), cx),
            PaletteOp::DeleteSelected => window.dispatch_action(Box::new(DeleteSelected), cx),
            PaletteOp::FocusSearch => window.dispatch_action(Box::new(FocusSearch), cx),
            PaletteOp::ClearChecked => window.dispatch_action(Box::new(ClearChecked), cx),
            PaletteOp::DetailsTab => window.dispatch_action(Box::new(InspectorDetails), cx),
            PaletteOp::HealthTab => window.dispatch_action(Box::new(InspectorHealth), cx),
            PaletteOp::PresetsTab => window.dispatch_action(Box::new(InspectorPresets), cx),
            PaletteOp::Subscriptions => self.open_subscriptions_dialog(window, cx),
            PaletteOp::PresetManager => self.open_presets_dialog(window, cx),
            PaletteOp::Groups => self.open_groups_dialog(window, cx),
            PaletteOp::Logs => self.open_logs_dialog(window, cx),
            PaletteOp::Settings => self.open_settings_dialog(window, cx),
            PaletteOp::About => self.open_about_dialog(window, cx),
            PaletteOp::StartAll => self.start_all_engines(window, cx),
            PaletteOp::StopAll => self.stop_all_engines(window, cx),
            PaletteOp::TestAll => {
                self.test_all_health(window, cx);
            }
            PaletteOp::RuntimeStats => self.open_runtime_stats_dialog(window, cx),
            PaletteOp::TrafficStats => self.open_traffic_stats_dialog(window, cx),
            PaletteOp::RowColumns => self.open_row_columns_dialog(window, cx),

            PaletteOp::ImportClipboard => self.open_import_from_clipboard(window, cx),
            PaletteOp::ExportLibrary => self.open_export_dialog(window, cx),
        }
        // T-06: the palette's second overlay opens first; the reclaim runs last,
        // and its predicate leaves the new overlay's focus alone.
        reclaim_shell_focus(window, cx);
    }

    /// T-27: the latest URL-test sample per row, in ms. Dead samples (an error
    /// with no latency), ping-only evidence and never-tested rows are absent —
    /// which is what pins them last when `latency` sorts.
    fn latency_map(&self, rows: &[ProxyConfig]) -> HashMap<String, u64> {
        let mut map = HashMap::new();
        for row in rows {
            let Some(sample) = self.health_store.latest_of(&row.id, HealthKind::UrlTest) else {
                continue;
            };
            if sample.error.is_some() {
                continue;
            }
            if let Some(latency_ms) = sample.latency_ms {
                map.insert(row.id.clone(), latency_ms);
            }
        }
        map
    }

    /// T-27: the per-row session traffic mirror the traffic tick already keeps
    /// (`download`/`upload` sort keys).
    fn data_traffic_map(&self, rows: &[ProxyConfig]) -> HashMap<String, (u64, u64)> {
        rows.iter()
            .filter_map(|row| {
                self.health_store
                    .data_traffic(&row.id)
                    .map(|totals| (row.id.clone(), totals))
            })
            .collect()
    }

    /// T-25: `N active · M TCP · K UDP` from the last merged clash snapshot.
    /// Hidden while no core runs — there is no collection to report, and zeros
    /// would read as a measured idle instead of "nothing is being polled".
    fn conn_counter_text(&self) -> Option<String> {
        if self.engine.running_count() == 0 {
            return None;
        }
        Some(self.conn_counts.status_text())
    }

    fn record_and_persist(&mut self, outcomes: &[TestOutcome]) {
        let mut snapshots = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            self.health_store.record_outcome(outcome);
            if outcome_is_neutral(outcome) {
                // A neutral probe never became a health sample; keeping it out
                // of the snapshots stops hydrate from reviving it as latest.
                continue;
            }
            snapshots.push(HealthSnapshot {
                config_id: outcome.sample.config_id.clone(),
                at: outcome.sample.at,
                kind: outcome.sample.kind.as_str().to_string(),
                latency_ms: outcome.sample.latency_ms,
                error: outcome.sample.error.clone(),
            });
        }
        if !snapshots.is_empty() {
            if let Err(error) = self.store.save_health_snapshots(&snapshots) {
                eprintln!("health snapshot persist failed: {error}");
            }
        }
    }

    fn group_running_rows(&self, group: &str) -> Vec<ProxyConfig> {
        let running = self.engine.running_ids();
        self.store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == *group && running.contains(&row.id))
            .collect()
    }

    fn spawn_group_probe(
        &mut self,
        rows: Vec<ProxyConfig>,
        group: String,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        use std::sync::atomic::Ordering;
        if rows.is_empty() || self.group_test_in_flight {
            return;
        }
        let count = rows.len();
        let url = self.effective_test_url(&rows);
        self.group_test_in_flight = true;
        self.probe_stop.store(false, Ordering::SeqCst);
        let stop = self.probe_stop.clone();
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let (outcomes, prober, stopped) = cx
                .background_executor()
                .spawn(async move {
                    let prober = prober;
                    let outcomes = test_group(&rows, &url, &*prober, &stop, |_, _| {});
                    let stopped = stop.load(Ordering::SeqCst);
                    (outcomes, prober, stopped)
                })
                .await;
            cx.update(|cx| {
                let recorded = weak.update(cx, |view, _| {
                    view.prober = prober;
                    view.group_test_in_flight = false;
                    view.record_and_persist(&outcomes);
                    // T-19: the sweep's outcomes are already persisted by
                    // record_and_persist, so auto-clear consumes them through
                    // the same snapshots the manual prune reads — no second
                    // probe pass lives anywhere.
                    let cleared = view.auto_clear_after_sweep(&group);
                    let (live, dead) = view.health_counts();
                    (live, dead, cleared)
                });
                let Ok((live, dead, auto_cleared)) = recorded else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };
                let message = group_test_summary_toast(&group, count, live, dead, stopped);
                let auto_text = auto_cleared
                    .as_ref()
                    .map(|(removed, kept)| auto_clear_summary_toast(&group, *removed, kept));
                let auto_success = auto_cleared
                    .as_ref()
                    .is_some_and(|(removed, _)| *removed > 0);
                if let Some(handle) = window {
                    if handle
                        .update(cx, |_, window, cx| {
                            view.update(cx, |view, cx| {
                                view.reload(window, cx);
                                window.push_notification(Notification::success(message), cx);
                                // T-19: exactly ONE auto-clear summary per
                                // sweep — success when rows went, info when
                                // every candidate was protected.
                                if let Some(auto_text) = auto_text {
                                    let auto = if auto_success {
                                        Notification::success(auto_text)
                                    } else {
                                        Notification::info(auto_text)
                                    };
                                    window.push_notification(auto, cx);
                                }
                            });
                        })
                        .is_ok()
                    {
                        return;
                    }
                }
                view.update(cx, |view, cx| view.reload_quiet(cx));
            });
        })
        .detach();
    }

    fn record_and_persist_speed(&mut self, outcomes: &[SpeedtestOutcome]) {
        let mut snapshots = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            self.health_store.record_speed_sample(outcome, now_unix());
            snapshots.push(HealthSnapshot {
                config_id: outcome.config_id.clone(),
                at: now_unix(),
                kind: HealthKind::SpeedTest.as_str().to_string(),
                latency_ms: outcome.latency_ms,
                error: outcome.error.clone(),
            });
        }
        if !snapshots.is_empty() {
            if let Err(error) = self.store.save_health_snapshots(&snapshots) {
                eprintln!("speed snapshot persist failed: {error}");
            }
        }
    }

    fn group_test_sample_count(&self, group: &str) -> usize {
        let ids: HashSet<String> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == group)
            .map(|row| row.id)
            .collect();
        self.store
            .load_health_snapshots()
            .unwrap_or_default()
            .iter()
            .filter(|snap| ids.contains(&snap.config_id))
            .count()
    }

    /// T-19: end-of-group-test-sweep application. Runs after the sweep has
    /// persisted its outcomes, so candidates come from the same health
    /// snapshots the manual "Unavailable" prune reads — no second probe. The
    /// ticket's protected classes (running, row under edit, in-flight refresh
    /// batch) are excluded at the candidate-filter level; removal goes through
    /// the shared removal pipeline. Returns one (removed, kept-reasons) pair
    /// when a summary is warranted.
    fn auto_clear_after_sweep(&mut self, group: &str) -> Option<(usize, Vec<String>)> {
        if !self.store.auto_clear_enabled(group) {
            return None;
        }
        let dead = match self.store.unavailable_rows_in_group(group) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("auto-clear read failed: {error}");
                return None;
            }
        };
        if dead.is_empty() {
            return None;
        }
        let editing = self.edit_form_open.clone();
        let batch_members = self.refresh_batch_members.clone();
        let running: HashSet<String> = self.engine.running_ids().into_iter().collect();
        let mut skip = HashSet::new();
        let mut reasons_seen: HashSet<&'static str> = HashSet::new();
        for row in dead {
            let reason = if running.contains(&row.id) {
                Some("running")
            } else if editing.as_deref() == Some(row.id.as_str()) {
                Some("editing")
            } else if row
                .subscription_id
                .as_deref()
                .is_some_and(|subscription| batch_members.contains(subscription))
            {
                Some("in-flight batch")
            } else {
                None
            };
            if let Some(name) = reason {
                skip.insert(row.id.clone());
                reasons_seen.insert(name);
            }
        }
        let removed = match self.store.remove_unavailable_in_group(group, &skip) {
            Ok(rows) => rows.len(),
            Err(error) => {
                eprintln!("auto-clear removal failed: {error}");
                return None;
            }
        };
        let kept: Vec<String> = ["running", "editing", "in-flight batch"]
            .iter()
            .filter(|reason| reasons_seen.contains(**reason))
            .map(|reason| (*reason).to_string())
            .collect();
        if removed == 0 && kept.is_empty() {
            return None;
        }
        Some((removed, kept))
    }

    /// T-27: one writer for the sort model — persists the meta row beside
    /// `library_columns`, mirrors the choice to the list, and says which order
    /// is live (the retired toggle's notification, now covering every key).
    fn set_library_sort(
        &mut self,
        key: LibrarySortKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let direction = self.libraries.read(cx).delegate().sort.direction;
        self.commit_library_sort(LibrarySort::new(key, direction), window, cx);
    }

    fn ping_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.ping_row(&config.id, window, cx);
    }

    fn ping_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        if !self.engine.running_ids().contains(&config.id) {
            window.push_notification(
                Notification::warning(format!("\"{}\" is not running", config.name)),
                cx,
            );
            return;
        }
        if self.probe_kind_in_flight(true) {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return;
        }
        self.spawn_probe(vec![config], true, false, None, cx);
    }

    fn effective_test_url(&self, rows: &[ProxyConfig]) -> String {
        let global = self.store.test_url();
        if rows.len() == 1 {
            let row_override = self.store.row_test_url(&rows[0].id);
            return effective_probe_url(row_override.as_deref(), global.as_deref());
        }
        effective_probe_url(None, global.as_deref())
    }

    fn spawn_probe(
        &mut self,
        rows: Vec<ProxyConfig>,
        ping: bool,
        toast: bool,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) -> usize {
        if rows.is_empty() || self.probe_kind_in_flight(ping) {
            return 0;
        }
        let count = rows.len();
        let url = self.effective_test_url(&rows);
        let names: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.id.clone(), row.name.clone()))
            .collect();
        if ping {
            self.probe_ping_in_flight = true;
        } else {
            self.probe_url_in_flight = true;
        }
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();

        let toast_ping = ping;
        cx.spawn(async move |_entity, cx| {
            let (outcomes, prober) = cx
                .background_executor()
                .spawn(async move {
                    let prober = prober;
                    let outcomes = if ping {
                        ping_all(&rows, &*prober)
                    } else {
                        test_all(&rows, &url, &*prober)
                    };
                    (outcomes, prober)
                })
                .await;
            cx.update(|cx| {
                let recorded = weak.update(cx, |view, _| {
                    view.prober = prober;
                    if ping {
                        view.probe_ping_in_flight = false;
                    } else {
                        view.probe_url_in_flight = false;
                    }
                    view.record_and_persist(&outcomes);
                    (view.health_counts(), outcomes.len())
                });
                let Ok(((live, dead), count)) = recorded else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };

                if toast {
                    let message = if count == 1 {
                        match outcomes.first() {
                            Some(outcome) => {
                                let name = names
                                    .iter()
                                    .find(|(id, _)| *id == outcome.sample.config_id)
                                    .map(|(_, name)| name.clone())
                                    .unwrap_or_else(|| outcome.sample.config_id.clone());
                                probe_result_toast(&name, outcome, toast_ping)
                            }
                            None => probe_summary_toast(count, live, dead),
                        }
                    } else {
                        probe_summary_toast(count, live, dead)
                    };
                    if let Some(handle) = window {
                        if handle
                            .update(cx, |_, window, cx| {
                                view.update(cx, |view, cx| {
                                    view.reload(window, cx);
                                    window.push_notification(Notification::success(message), cx);
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

    fn log_lines(&self, cx: &App) -> Vec<String> {
        self.visible_log_lines(cx)
    }

    fn log_line_passes_filters(
        line: &str,
        exclude_lower: &str,
        pattern: Option<&regex::Regex>,
    ) -> bool {
        if !exclude_lower.is_empty() && line.to_lowercase().contains(exclude_lower) {
            return false;
        }
        pattern.is_none_or(|re| re.is_match(line))
    }

    fn visible_log_lines(&self, cx: &App) -> Vec<String> {
        let selected = self.selected_config(cx).map(|config| config.id);
        let filter = self.log_filter.trim().to_lowercase();
        let level = self.log_level;
        let render = |level: &str, message: &str| {
            format!("[{}] {}", level, redact_secrets(&strip_ansi_codes(message)))
        };
        let records: Vec<&crate::health::LogRecord> = match selected.as_deref() {
            Some(id) => self.logs.for_config(id),
            None => self.logs.all().iter().collect(),
        };
        let mut lines: Vec<String> = records
            .iter()
            .filter(|record| level.is_none_or(|want| record.level == want))
            .map(|record| render(record.level.as_str(), &record.message))
            .collect();
        if !filter.is_empty() {
            lines.retain(|line| line.to_lowercase().contains(&filter));
        }

        let exclude = self.log_exclude.trim().to_lowercase();
        let raw_pattern = self.log_regex.trim();
        let pattern = if raw_pattern.is_empty() {
            None
        } else {
            regex::Regex::new(raw_pattern).ok()
        };
        if !exclude.is_empty() || pattern.is_some() {
            lines.retain(|line| Self::log_line_passes_filters(line, &exclude, pattern.as_ref()));
        }
        if lines.len() > 200 {
            if self.log_tail {
                lines = lines.split_off(lines.len() - 200);
            } else {
                lines.truncate(200);
            }
        }
        lines
    }

    fn set_log_level(
        &mut self,
        level: Option<LogLevel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.log_level = level;
        if self.logs_open && window.has_active_sheet(cx) {
            self.log_reopen_draft = self.log_input_draft(cx);
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs_open = false;
            self.open_logs_dialog(window, cx);
        } else {
            cx.notify();
        }
    }

    fn clear_log_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.log_filter.clear();

        self.log_exclude.clear();
        self.log_regex.clear();
        self.log_level = None;
        // Drop any in-progress draft so the reopen seeds empty, not the text
        // the user was about to discard.
        self.log_reopen_draft = None;
        self.log_inputs = None;
        if self.logs_open && window.has_active_sheet(cx) {
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs_open = false;
            self.open_logs_dialog(window, cx);
        } else {
            cx.notify();
        }
    }

    /// Live text of the open sheet's three inputs (typed but not yet applied).
    fn log_input_draft(&self, cx: &App) -> Option<(String, String, String)> {
        self.log_inputs.as_ref().map(|(filter, exclude, pattern)| {
            (
                filter.read(cx).value().to_string(),
                exclude.read(cx).value().to_string(),
                pattern.read(cx).value().to_string(),
            )
        })
    }

    fn set_row_column(
        &mut self,
        column: &'static str,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut columns = self.row_columns;
        match column {
            "upstream" => columns.upstream = enabled,
            "health" => columns.health = enabled,
            "group" => columns.group = enabled,
            _ => columns.traffic = enabled,
        }
        self.row_columns = columns;
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_row_columns(columns);
            list_cx.notify();
        });
        if let Err(error) = self.store.set_library_columns(&columns.display()) {
            window.push_notification(
                Notification::error(format!("Could not save column choice: {error}")),
                cx,
            );
        }
        cx.notify();
    }

    fn browse_for_import_files(
        &mut self,
        current_text: String,
        current_group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let view = cx.entity();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                _ => return,
            };
            let mut files: Vec<(String, String)> = Vec::new();
            let mut unreadable: Vec<String> = Vec::new();

            for path in &paths {
                let display = path.to_string_lossy().into_owned();
                match read_import_file(path) {
                    ImportFileRead::Text(content) => {
                        files.push((display, content));
                    }
                    ImportFileRead::Unreadable => {
                        unreadable.push(file_basename(&display));
                    }
                    ImportFileRead::Refused(reason) => {
                        unreadable.push(format!("{} ({reason})", file_basename(&display)));
                    }
                }
            }
            if files.is_empty() && unreadable.is_empty() {
                return;
            }
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_browse_pick(
                        current_text,
                        current_group,
                        files,
                        unreadable,
                        window,
                        cx,
                    );
                })
            });
        })
        .detach();
    }

    fn browse_for_ssh_key(
        &mut self,
        path_entity: Entity<InputState>,
        key_entity: Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let view = cx.entity();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                _ => return,
            };
            let path = paths.into_iter().next().expect("non-empty checked");
            let display = path.to_string_lossy().into_owned();
            let content = std::fs::read_to_string(&path).ok();
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_ssh_key_pick(path_entity, key_entity, display, content, window, cx);
                })
            });
        })
        .detach();
    }

    fn chain_add_hop(
        &mut self,
        hops: &Rc<RefCell<Vec<serde_json::Value>>>,
        library: &Rc<RefCell<Vec<ChainLibraryEntry>>>,
        pick: &Entity<SelectState<PanelChoiceList>>,
        picked_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let picked_id = picked_id.trim();
        let entry = library
            .borrow()
            .iter()
            .find(|entry| entry.id == picked_id)
            .cloned();
        let Some(entry) = entry else {
            window.push_notification(
                Notification::warning("Pick a library row first — hops reference rows by id"),
                cx,
            );
            return false;
        };
        let kinds = chain_existing_kinds(&hops.borrow(), &library.borrow());
        if let Some(rule) =
            chain_hop_rejection(&kinds, chain_slot_kind(&entry.protocol), kinds.len(), false)
        {
            window.push_notification(Notification::warning(rule), cx);
            return false;
        }
        hops.borrow_mut()
            .push(serde_json::Value::String(entry.id.clone()));
        pick.update(cx, |state, cx| {
            state.set_selected_value(&String::new(), window, cx);
        });
        window.push_notification(
            Notification::success(format!("Added {} to the chain", entry.name)),
            cx,
        );
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn chain_replace_hop(
        &mut self,
        hops: &Rc<RefCell<Vec<serde_json::Value>>>,
        library: &Rc<RefCell<Vec<ChainLibraryEntry>>>,
        pick: &Entity<SelectState<PanelChoiceList>>,
        index: usize,
        picked_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let picked_id = picked_id.trim();
        let entry = library
            .borrow()
            .iter()
            .find(|entry| entry.id == picked_id)
            .cloned();
        let Some(entry) = entry else {
            window.push_notification(
                Notification::warning("Pick a library row first — hops reference rows by id"),
                cx,
            );
            return false;
        };
        let kinds = chain_existing_kinds(&hops.borrow(), &library.borrow());
        if let Some(rule) =
            chain_hop_rejection(&kinds, chain_slot_kind(&entry.protocol), index, true)
        {
            window.push_notification(Notification::warning(rule), cx);
            return false;
        }
        hops.borrow_mut()[index] = serde_json::Value::String(entry.id.clone());
        pick.update(cx, |state, cx| {
            state.set_selected_value(&String::new(), window, cx);
        });
        window.push_notification(
            Notification::success(format!("Replaced hop {} with {}", index + 1, entry.name)),
            cx,
        );
        true
    }

    fn browse_for_extra_exe(
        &mut self,
        path_entity: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let view = cx.entity();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                _ => return,
            };
            let path = paths.into_iter().next().expect("non-empty checked");
            let display = path.to_string_lossy().into_owned();
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_extra_exe_pick(path_entity, display, window, cx);
                })
            });
        })
        .detach();
    }

    /// Marks the running (or just-finished) import parse as cancelled so its
    /// batch is dropped instead of landing after the dialog was dismissed.
    fn cancel_import_parse(&mut self, cx: &mut Context<Self>) {
        self.import_cancel_epoch = Some(self.import_parse_epoch);
        cx.notify();
    }

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

    #[allow(clippy::too_many_arguments)]
    fn accumulate_batch_member(
        &mut self,
        sub_id: &str,
        added: usize,
        updated: usize,
        removed: usize,
        failed: usize,
        conflicts: usize,
        in_use: usize,
        added_names: &[String],
        updated_names: &[String],
        removed_names: &[String],
        kept_in_use_names: &[String],
    ) -> bool {
        if !self.refresh_batch_members.remove(sub_id) {
            return false;
        }
        self.refresh_batch_added += added;
        self.refresh_batch_updated += updated;
        self.refresh_batch_removed += removed;
        self.refresh_batch_failed += failed;
        self.refresh_batch_conflicts += conflicts;
        self.refresh_batch_in_use += in_use;
        self.refresh_batch_added_names
            .extend(added_names.iter().cloned());
        self.refresh_batch_updated_names
            .extend(updated_names.iter().cloned());
        self.refresh_batch_removed_names
            .extend(removed_names.iter().cloned());
        self.refresh_batch_in_use_names
            .extend(kept_in_use_names.iter().cloned());
        self.refresh_batch_members.is_empty()
    }

    fn is_batch_member(&self, sub_id: &str) -> bool {
        self.refresh_batch_members.contains(sub_id)
    }

    fn refresh_batch_progress(&self) -> Option<(usize, usize)> {
        if self.refresh_batch_members.is_empty() {
            return None;
        }
        let total = self
            .refresh_batch_total
            .max(self.refresh_batch_members.len());
        Some((
            total.saturating_sub(self.refresh_batch_members.len()),
            total,
        ))
    }

    fn close_batch_slot(&mut self, sub_id: &str, window: &mut Window, cx: &mut App) {
        if self.is_batch_member(sub_id)
            && self.accumulate_batch_member(sub_id, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[])
        {
            self.flush_refresh_batch(window, cx);
        }
    }

    fn push_refresh_log(&mut self, line: &str) {
        self.logs.push_raw(line, &LogTagIndex::default());
    }

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
        in_use: usize,
        added_names: &[String],
        updated_names: &[String],
        removed_names: &[String],
        kept_in_use_names: &[String],
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.is_batch_member(sub_id) {
            if self.accumulate_batch_member(
                sub_id,
                added,
                updated,
                removed,
                failed,
                conflicts,
                in_use,
                added_names,
                updated_names,
                removed_names,
                kept_in_use_names,
            ) {
                self.flush_refresh_batch(window, cx);
            }
        } else {
            window.push_notification(notice, cx);
        }
    }

    fn refresh_subscription_by_id(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.refresh_batch_members.remove(sub_id) {
            self.refresh_batch_total = self.refresh_batch_total.saturating_sub(1);
        }
        let started = match self.store.get_subscription(sub_id) {
            Ok(Some(sub)) => self.start_subscription_refresh(sub, true, cx),
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
        };

        if started {
            self.reopen_subscriptions_if_open(window, cx);
        }
        started
    }

    fn note_refresh_started(&mut self, sub_id: &str) -> bool {
        self.inflight_refreshes.insert(sub_id.to_string())
    }

    fn sync_presets_to_engine(&mut self) -> Result<(), String> {
        match self.store.preset_rules_map() {
            Ok(map) => {
                self.engine.set_preset_rules(map);
            }
            Err(error) => return Err(format!("cannot load routing presets: {error}")),
        }
        self.sync_rule_sets_to_engine()
    }

    fn try_preset_commit(
        &mut self,
        resolved: &ResolvedImport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        use crate::preset::{
            decode_remote_route_link, is_remote_route_attempt, is_route_attempt,
            looks_like_preset_json,
        };
        let trimmed = resolved.text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let single_line =
            !trimmed.contains('\n') && resolved.file_hint.is_none() && resolved.multi.is_none();
        if single_line && is_route_attempt(trimmed) {
            return Some(match self.store.import_route_link(trimmed) {
                Ok(preset) => {
                    let count = preset.rules().map(|rules| rules.len()).unwrap_or(0);
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(preset_imported_message(&preset.name, count)),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            });
        }
        if single_line && is_remote_route_attempt(trimmed) {
            return Some(match decode_remote_route_link(trimmed) {
                Ok(request) => {
                    match self
                        .store
                        .import_remote_preset_shell(&request.name, &request.url)
                    {
                        Ok(preset) => {
                            self.reload(window, cx);
                            window.push_notification(
                                Notification::success(format!(
                                    "Routing preset \"{}\" added — fetching rules",
                                    preset.name
                                )),
                                cx,
                            );
                            self.start_preset_refresh(preset, cx);
                            true
                        }
                        Err(error) => {
                            self.report_save_error(error, window, cx);
                            false
                        }
                    }
                }
                Err(error) => {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    false
                }
            });
        }
        if resolved.multi.is_none() && looks_like_preset_json(trimmed) {
            return Some(
                match self.store.import_preset_json(
                    trimmed,
                    if resolved.file_hint.is_some() {
                        "file"
                    } else {
                        "manual"
                    },
                    resolved.file_hint.as_deref(),
                ) {
                    Ok(preset) => {
                        let count = preset.rules().map(|rules| rules.len()).unwrap_or(0);
                        self.reload(window, cx);
                        window.push_notification(
                            Notification::success(preset_imported_message(&preset.name, count)),
                            cx,
                        );
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                },
            );
        }
        None
    }

    /// T-11: expand the simple-rules boxes into structured rules, appending
    /// them to the hand-built rules. Any invalid line blocks the save loud:
    /// nothing is dropped, corrected, or stored, and the boxes keep their
    /// text for the editor's "Invalid lines" fold. The expanded rules flow
    /// through the store's own `validate_preset_rules` gate unchanged — no
    /// new intake errors here.
    fn expand_simple_rule_boxes(
        boxes: &SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<serde_json::Value>, ()> {
        let mut rules: Vec<serde_json::Value> = Vec::new();
        let mut invalid: Vec<String> = Vec::new();
        for (r#box, text) in [
            (crate::preset::SimpleRulesBox::Direct, boxes.direct.as_str()),
            (crate::preset::SimpleRulesBox::Block, boxes.block.as_str()),
            (crate::preset::SimpleRulesBox::Proxy, boxes.proxy.as_str()),
        ] {
            if text.trim().is_empty() {
                continue;
            }
            let parsed = crate::preset::parse_simple_rules(r#box, r#box.action(), text);
            invalid.extend(
                parsed
                    .invalid
                    .into_iter()
                    .map(|line| format!("{} box: {}", r#box.label(), line)),
            );
            rules.extend(parsed.rules);
        }
        if invalid.is_empty() {
            return Ok(rules);
        }
        window.push_notification(
            Notification::error(format!(
                "Invalid simple-rules lines block saving — fix or remove them (shown under \"Invalid lines\"): {}",
                invalid.join(" | ")
            )),
            cx,
        );
        Err(())
    }

    fn rewrite_preset_rules_box(
        form: &PresetForm,
        op: &PresetRulesOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = form.name.read(cx).value().to_string();
        let text = form.rules.read(cx).value().to_string();
        let fallback = if name.trim().is_empty() {
            "preset"
        } else {
            name.trim()
        };
        let mut parsed = match crate::preset::parse_preset_json(&text, fallback) {
            Ok(parsed) => parsed.rules,
            Err(error) => {
                window.push_notification(Notification::error(preset_error_message(&error)), cx);
                return;
            }
        };
        match op {
            PresetRulesOp::CycleAction(index) => {
                let Some(rule) = parsed.get_mut(*index) else {
                    window.push_notification(Notification::warning("Already removed"), cx);
                    return;
                };
                let current = rule
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("direct");
                let next = match current {
                    "direct" => "proxy",
                    "proxy" => "block",
                    _ => "direct",
                };
                if let Err(error) = crate::preset::set_rule_action(rule, next) {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    return;
                }
            }
            PresetRulesOp::Remove(index) => {
                if *index >= parsed.len() {
                    window.push_notification(Notification::warning("Already removed"), cx);
                    return;
                }
                parsed.remove(*index);
            }
            PresetRulesOp::Add => {
                parsed.push(serde_json::json!({"action": "direct"}));
            }
        }
        let next_text = serde_json::Value::Array(parsed).to_string();
        form.rules.update(cx, |state, cx| {
            state.set_value(next_text, window, cx);
        });
    }

    fn preset_name_for(&self, preset_id: Option<&str>) -> Option<String> {
        let id = preset_id?;
        self.store
            .get_preset(id)
            .ok()
            .flatten()
            .map(|preset| preset.name)
    }

    fn preset_summary_for(&self, preset_id: Option<&str>) -> Option<(String, usize)> {
        let id = preset_id?;
        let preset = self.store.get_preset(id).ok().flatten()?;
        let count = preset.rules().ok().map(|rules| rules.len())?;
        Some((preset.name, count))
    }

    fn refresh_preset_by_id(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.get_preset(id) {
            Ok(Some(preset)) => {
                if preset.source != "url" || preset.source_url.is_none() {
                    window.push_notification(
                        Notification::warning("Only URL presets can refresh"),
                        cx,
                    );
                    return false;
                }
                self.start_preset_refresh(preset, cx);
                true
            }
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                false
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh routing preset: {error}")),
                    cx,
                );
                false
            }
        }
    }

    // ——— T-12: .srs rule-set staging (explicit confirm-per-download only) ——

    fn sync_rule_sets_to_engine(&mut self) -> Result<(), String> {
        let dir = crate::store::rule_sets_dir();
        let files: HashMap<String, PathBuf> = self
            .store
            .list_rule_sets()
            .map_err(|error| format!("cannot load rule sets: {error}"))?
            .into_iter()
            .map(|row| (row.name, dir.join(row.file_name)))
            .collect();
        self.engine.set_rule_set_files(files);
        Ok(())
    }

    fn refresh_due_on_startup(&mut self, cx: &mut Context<Self>) {
        for sub in self.due_subscriptions() {
            self.start_subscription_refresh(sub, false, cx);
        }
    }

    fn refresh_due_on_startup_deferred(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(STARTUP_REFRESH_DEFER_SECS))
                .await;
            cx.update(|cx| {
                weak.update(cx, |view, cx| {
                    if !view.ticks_enabled {
                        return;
                    }
                    view.refresh_due_on_startup(cx);
                })
                .ok();
            });
        })
        .detach();
    }

    fn due_subscriptions(&self) -> Vec<ProxySubscription> {
        if !self.store.subs_auto_refresh_enabled() {
            return Vec::new();
        }

        self.due_for_manual_refresh()
            .into_iter()
            .filter(|sub| !self.store.group_skip_refresh(&sub.group))
            .collect()
    }

    fn due_for_manual_refresh(&self) -> Vec<ProxySubscription> {
        let now = now_unix();
        self.store
            .list_subscriptions()
            .unwrap_or_default()
            .into_iter()
            .filter(|sub| due_for_refresh_for(sub, now))
            .collect()
    }

    fn refresh_all_due(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let due = self.due_for_manual_refresh();
        if due.is_empty() {
            window.push_notification(
                Notification::success("No subscriptions due for refresh"),
                cx,
            );
            return 0;
        }
        let ids: Vec<String> = due.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in due {
            if self.start_subscription_refresh(sub, true, cx) {
                started += 1;
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
        started
    }

    fn current_group(&self, cx: &App) -> Option<String> {
        if let Some(group) = self.libraries.read(cx).delegate().facet_group.clone() {
            return Some(group);
        }
        self.selected_config(cx).map(|config| config.group)
    }

    fn update_current_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let subs = self
            .store
            .list_subscriptions()
            .map(|subs| update_target_subs(&subs, &group))
            .unwrap_or_default();
        if subs.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "Group \"{group}\" has no subscriptions — nothing to update"
                )),
                cx,
            );
            return;
        }
        self.start_explicit_refresh(subs, window, cx);
    }

    fn update_group_subs(&mut self, group: &str, window: &mut Window, cx: &mut Context<Self>) {
        let subs = self
            .store
            .list_subscriptions()
            .map(|subs| update_target_subs(&subs, group))
            .unwrap_or_default();
        if subs.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "Group \"{group}\" has no subscriptions — nothing to update"
                )),
                cx,
            );
            return;
        }
        self.start_explicit_refresh(subs, window, cx);
    }

    /// Probe endpoints whose own bytes must stay out of row usage: the default
    /// test URL, the global custom test URL, and every running row's override.
    fn traffic_probe_exclusions(&self) -> ProbeExclusions {
        let mut exclusions = ProbeExclusions::new();
        exclusions.push_url(DEFAULT_TEST_URL);
        if let Some(global) = self.store.test_url() {
            exclusions.push_url(&global);
        }
        for id in self.engine.running_ids() {
            if let Some(url) = self.store.row_test_url(&id) {
                exclusions.push_url(&url);
            }
        }
        exclusions
    }

    fn spawn_traffic_poll(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled || self.traffic_tick_in_flight {
            return 0;
        }

        let now = now_unix();
        if !traffic_poll_due(self.inspector_tab, now, self.last_traffic_poll_at) {
            return 0;
        }
        self.last_traffic_poll_at = now;
        let targets = self.engine.live_clash_targets();
        if targets.is_empty() {
            return 0;
        }
        self.ensure_traffic_mirror(cx);
        self.traffic_tick_in_flight = true;
        let exclusions = self.traffic_probe_exclusions();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let (results, exclusions) = cx
                .background_executor()
                .spawn(async move {
                    let results = crate::health::poll_clash_targets(
                        &targets,
                        crate::health::CLASH_PER_CORE_TIMEOUT,
                        crate::health::CLASH_TOTAL_DEADLINE,
                    );
                    let mut exclusions = exclusions;
                    exclusions.resolve_ips();
                    (results, exclusions)
                })
                .await;
            cx.update(|cx| {
                weak.update(cx, |view, cx| {
                    view.traffic_tick_in_flight = false;
                    view.apply_traffic_results(results, &exclusions, cx);
                })
                .ok();
            });
        })
        .detach();
        1
    }

    fn usage_line(&self, config_id: &str) -> String {
        match self.store.traffic_totals() {
            Ok(totals) => match totals.get(config_id) {
                Some((rx, tx)) => format!(
                    "usage down {} · up {}",
                    format_bytes(*rx),
                    format_bytes(*tx)
                ),
                None => "usage unavailable".to_string(),
            },
            Err(_) => "usage unavailable".to_string(),
        }
    }

    fn usage_parts(&self, config_id: &str) -> Option<(String, String)> {
        match self.store.traffic_totals() {
            Ok(totals) => totals
                .get(config_id)
                .map(|(rx, tx)| (format_bytes(*rx), format_bytes(*tx))),
            Err(_) => None,
        }
    }

    fn history_line(&self, config_id: &str, cx: &App) -> AnyElement {
        let since = now_unix().saturating_sub(90 * 86_400);
        let entries = match self.store.load_health_history(config_id, Some(since)) {
            Ok(entries) => entries,
            Err(_) => return div().child("history unavailable").into_any_element(),
        };
        let latencies = url_test_sparkline(&entries, 120);
        let caption_tone = sparkline_caption_tone(&entries);
        if latencies.len() < 2 {
            return div()
                .id("config-history-empty")
                .test_support()
                .text_color(health_tone_color(caption_tone, cx))
                .child(match latencies.len() {
                    1 => format!("1 probe recorded — no trend yet ({})", latencies[0]),
                    _ => "no health history yet — press Test to record a probe".to_string(),
                })
                .into_any_element();
        }
        let latest = latencies.last().copied().unwrap_or(0);

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_color(health_tone_color(caption_tone, cx))
                    .child(format!("{} samples · latest {}ms", latencies.len(), latest)),
            )
            .child(Self::latency_sparkline(&latencies))
            .into_any_element()
    }

    const MOTION_FAST_MS: u64 = 150;

    /// How long a `Copied!` transient marker stays painted after a cell click.
    const CONN_COPIED_TOOLTIP_MS: u64 = 1500;

    fn motion_fast_duration(cx: &App) -> Duration {
        if cx.reduce_motion() {
            Duration::ZERO
        } else {
            Duration::from_millis(Self::MOTION_FAST_MS)
        }
    }

    fn motion_enter(
        child: impl IntoElement + 'static,
        id: impl Into<ElementId>,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .with_animation(
                id,
                Animation::new(Self::motion_fast_duration(cx)),
                |this, delta| this.opacity(delta),
            )
            .child(child)
    }

    fn set_inspector_tab(&mut self, tab: InspectorTab) {
        self.inspector_tab = tab;
        self.detail_scroll.set_offset(point(px(0.), px(0.)));
    }

    fn adjust_conn_column_widths(
        &mut self,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let widths = self.store.conn_column_widths();
        for (index, width) in widths.iter().enumerate() {
            let next = (*width as i32 + delta).clamp(40, 600) as u32;
            if let Err(error) = self.store.set_conn_column_width(index, next) {
                window.push_notification(
                    Notification::error(format!("Column width failed: {error}")),
                    cx,
                );
                return;
            }
        }
        cx.notify();
    }

    fn refresh_conn_column_widths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.reset_conn_column_widths() {
            Ok(()) => {
                window.push_notification(
                    Notification::success("Column widths reset to defaults"),
                    cx,
                );
                cx.notify();
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Column width reset failed: {error}")),
                    cx,
                );
            }
        }
    }

    fn detail_failed(&self, config: &ProxyConfig) -> bool {
        self.engine.ids_on_crashed_shards().contains(&config.id)
            || self.health_store.row_summary(&config.id).trim() == "dead"
    }

    fn select_visible_checked(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.libraries.update(cx, |state, list_cx| {
            let ids: Vec<String> = state
                .delegate()
                .visible_rows()
                .iter()
                .map(|row| row.id.clone())
                .collect();
            let delegate = state.delegate_mut();
            for id in ids {
                if !delegate.checked.contains(&id) {
                    delegate.toggle_checked(&id);
                }
            }
            list_cx.notify();
        });
        cx.notify();
    }
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
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);

        bind_app_keys(cx);
        cx.spawn(async move |cx| {
            cx.open_window(app_window_options(startup_geometry), |window, cx| {
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
                            view.update(cx, |view, _| view.mark_clean_exit());
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
