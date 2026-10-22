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

    fn reload_quiet(&mut self, cx: &mut Context<Self>) {
        let _ = self.reload_inner(cx);
        cx.notify();
    }

    fn reload_quiet_no_poll(&mut self, cx: &mut Context<Self>) {
        let _ = self.reload_inner_no_poll(cx);
        cx.notify();
    }

    fn reload_inner(&mut self, cx: &mut Context<Self>) -> Result<(), StoreError> {
        self.reload_inner_impl(cx, true)
    }

    fn reload_inner_no_poll(&mut self, cx: &mut Context<Self>) -> Result<(), StoreError> {
        self.reload_inner_impl(cx, false)
    }

    fn reload_inner_impl(&mut self, cx: &mut Context<Self>, poll: bool) -> Result<(), StoreError> {
        let rows = self.store.list()?;
        let bind_hosts = self.store.bind_hosts();
        self.drain_engine_logs(&rows);

        if poll {
            self.poll_engine();
        }
        let health = self.health_map();
        let health_tones = self.health_tones();
        // T-27: sort mirrors — pulled from state the ticks already maintain.
        let latency = self.latency_map(&rows);
        let data_traffic = self.data_traffic_map(&rows);
        let out_ips = self.out_ip_by_row.clone();
        let running = self.engine.running_ids();

        self.snapshot_running_set(&running);
        self.sync_running_title(running.len(), cx);
        if let Some(tray) = &self.tray {
            tray.set_running(running.len());
        }

        let traffic = self.store.traffic_totals().unwrap_or_default();
        let mut sub_labels = HashMap::new();
        if let Ok(subs) = self.store.list_subscriptions() {
            for sub in &subs {
                sub_labels.insert(sub.id.clone(), sub.label.clone());
            }
        }
        let mut preset_names = HashMap::new();
        if let Ok(presets) = self.store.list_presets() {
            for preset in &presets {
                preset_names.insert(preset.id.clone(), preset.name.clone());
            }
        }
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_rows(rows);
            state.delegate_mut().set_bind_hosts(bind_hosts);
            state.delegate_mut().set_health(health);

            state.delegate_mut().set_health_tone(health_tones);

            state.delegate_mut().set_running(running);

            state
                .delegate_mut()
                .set_crashed(self.engine.ids_on_crashed_shards());
            state.delegate_mut().set_traffic(traffic);
            state.delegate_mut().set_latency(latency);
            state.delegate_mut().set_data_traffic(data_traffic);
            state.delegate_mut().set_out_ips(out_ips);
            state
                .delegate_mut()
                .set_search_context(sub_labels, preset_names);

            list_cx.notify();
        });
        Ok(())
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

    #[cfg(test)]
    fn commit_add(
        &mut self,
        input: NewConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Err(reason) = save_listen_gate(&input.name, &input.bind_ip, &input.port) {
            window.push_notification(Notification::error(reason), cx);
            return false;
        }
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

    fn commit_add_with_focus(
        &mut self,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let input = form.as_new(cx);
        if let Err(reason) = save_listen_gate(&input.name, &input.bind_ip, &input.port) {
            *form_error.borrow_mut() = Some(("port".to_string(), reason.clone()));
            form.focus_field("port", window, cx);
            window.push_notification(Notification::error(reason), cx);
            cx.notify();
            return false;
        }

        if input.protocol == "extra-core" {
            if let Err(reason) = validate_extra_params(&input.params) {
                *form_error.borrow_mut() = Some(("params".to_string(), reason.clone()));
                *form.advanced.borrow_mut() = true;
                form.focus_field("params", window, cx);
                window.push_notification(Notification::error(reason), cx);
                cx.notify();
                return false;
            }
        }
        match self.store.add(input) {
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

                    if field == "params" {
                        *form.advanced.borrow_mut() = true;
                    }
                    form.focus_field(field, window, cx);
                }
                self.report_save_error(error, window, cx);
                cx.notify();
                false
            }
        }
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
            Ok(Some(existing)) => {
                let name = patch.name.as_deref().unwrap_or(&existing.name);
                let bind = patch.bind_ip.as_deref().unwrap_or(&existing.bind_ip);
                let port_gate = match patch.port.as_deref() {
                    Some(port_raw) => save_listen_gate(name, bind, port_raw),
                    None if patch.bind_ip.is_some() || patch.name.is_some() => {
                        save_listen_gate(name, bind, &existing.port.to_string())
                    }
                    None => Ok(()),
                };
                if let Err(reason) = port_gate {
                    window.push_notification(Notification::error(reason), cx);
                    return false;
                }
                match self.store.update(id, patch) {
                    Ok(updated) => {
                        let message = if self.engine.is_running(id) {
                            restart_to_apply_message(&updated.name)
                        } else {
                            row_action_message("Updated", &updated.name)
                        };
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
        }
    }

    fn commit_row_endpoint(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (bind_raw, port_raw, stored) = self.libraries.read_with(cx, |list, cx| {
            let delegate = list.delegate();
            let drafts = delegate.endpoint_draft_texts(id, cx).unwrap_or_default();
            let stored = delegate
                .rows
                .iter()
                .find(|row| row.id == id)
                .map(|row| (row.name.clone(), row.bind_ip.clone(), row.port.to_string()));
            (drafts.0, drafts.1, stored)
        });
        let Some((name, bind_stored, port_stored)) = stored else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return true;
        };
        if bind_raw.trim() == bind_stored && port_raw.trim() == port_stored {
            self.libraries.update(cx, |list, _| {
                if let Some(inputs) = list.delegate_mut().endpoint_inputs.get_mut(id) {
                    inputs.clean_bind = bind_raw.clone();
                    inputs.clean_port = port_raw.clone();
                }
            });
            return true;
        }
        if let Err(reason) = save_listen_gate(&name, &bind_raw, &port_raw) {
            window.push_notification(Notification::error(reason), cx);
            return false;
        }
        let patch = ConfigPatch {
            bind_ip: Some(bind_raw),
            port: Some(port_raw),
            ..Default::default()
        };
        let ok = self.commit_edit(id, patch, window, cx);
        if ok {
            if let Ok(Some(fresh)) = self.store.get(id) {
                let bind_fresh = fresh.bind_ip.clone();
                let port_fresh = fresh.port.to_string();
                self.libraries.update(cx, |list, cx| {
                    let delegate = list.delegate_mut();
                    delegate
                        .pending_endpoint_commits
                        .retain(|pending| pending != id);
                    if let Some(inputs) = delegate.endpoint_inputs.get_mut(id) {
                        inputs.bind.update(cx, |state, cx| {
                            state.set_value(bind_fresh.clone(), window, cx);
                        });
                        inputs.port.update(cx, |state, cx| {
                            state.set_value(port_fresh.clone(), window, cx);
                        });
                        inputs.clean_bind = bind_fresh;
                        inputs.clean_port = port_fresh;
                    }
                });
            }
        }
        ok
    }

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
            Ok(Some(existing)) => {
                let name = patch.name.as_deref().unwrap_or(&existing.name);
                let bind = patch.bind_ip.as_deref().unwrap_or(&existing.bind_ip);
                let port_gate = match patch.port.as_deref() {
                    Some(port_raw) => save_listen_gate(name, bind, port_raw),
                    None if patch.bind_ip.is_some() || patch.name.is_some() => {
                        save_listen_gate(name, bind, &existing.port.to_string())
                    }
                    None => Ok(()),
                };
                if let Err(reason) = port_gate {
                    *form_error.borrow_mut() = Some(("port".to_string(), reason.clone()));
                    form.focus_field("port", window, cx);
                    window.push_notification(Notification::error(reason), cx);
                    cx.notify();
                    return false;
                }

                if patch.params.is_some() && existing.protocol == "extra-core" {
                    let params = patch.params.as_deref().unwrap_or(&existing.params);
                    if let Err(reason) = validate_extra_params(params) {
                        *form_error.borrow_mut() = Some(("params".to_string(), reason.clone()));
                        *form.advanced.borrow_mut() = true;
                        form.focus_field("params", window, cx);
                        window.push_notification(Notification::error(reason), cx);
                        cx.notify();
                        return false;
                    }
                }
                match self.store.update(id, patch) {
                    Ok(updated) => {
                        *form_error.borrow_mut() = None;

                        let message = if self.engine.is_running(id) {
                            restart_to_apply_message(&updated.name)
                        } else {
                            row_action_message("Updated", &updated.name)
                        };
                        self.reload(window, cx);
                        window.push_notification(Notification::success(message), cx);
                        true
                    }
                    Err(error) => {
                        if let Some((field, message)) = field_error_target(&error) {
                            *form_error.borrow_mut() = Some((field.to_string(), message));

                            if field == "params" {
                                *form.advanced.borrow_mut() = true;
                            }
                            form.focus_field(field, window, cx);
                        }
                        self.report_save_error(error, window, cx);
                        cx.notify();
                        false
                    }
                }
            }
        }
    }

    fn commit_remove(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());
        let was_running = self.engine.stop_one(id);
        match self.store.remove(id) {
            Ok(true) => {
                if self.selected_config(cx).map(|row| row.id).as_deref() == Some(id) {
                    self.libraries.update(cx, |state, list_cx| {
                        // T-06: clear through the list so the kit's own index
                        // (and the UIA selection it reports) drops the row too.
                        state.set_selected_index(None, window, list_cx);

                        list_cx.notify();
                    });
                }
                self.reload(window, cx);

                window.focus(&self.focus, cx);
                window.push_notification(
                    Notification::success(removed_message(&name, was_running)),
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

    fn commit_duplicate(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let source = match self.store.get(id) {
            Ok(Some(row)) => row,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return;
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                return;
            }
        };
        let taken: Vec<String> = self
            .store
            .list()
            .map(|rows| rows.into_iter().map(|row| row.name).collect())
            .unwrap_or_default();
        let name = duplicate_name(&source.name, &taken);
        let input = NewConfig {
            name: name.clone(),
            protocol: source.protocol.clone(),
            params: source.params.clone(),
            group: source.group.clone(),
            bind_ip: source.bind_ip.clone(),
            port: String::new(),
        };
        match self.store.add(input) {
            Ok(created) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Duplicated \"{}\" as \"{}\" (disabled, port :{})",
                        source.name, created.name, created.port
                    )),
                    cx,
                );
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
    }

    fn row_config(&self, id: &str) -> Option<ProxyConfig> {
        self.store.get(id).ok().flatten()
    }

    fn commit_reset_traffic(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());
        match self.store.reset_traffic(id) {
            Ok(()) => {
                self.health_store.set_data_traffic(id, 0, 0);
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Reset traffic for \"{name}\"")),
                    cx,
                );
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot reset traffic: {error}")),
                    cx,
                );
            }
        }
    }

    fn commit_test_url(&mut self, raw: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            match self.store.clear_test_url() {
                Ok(()) => {
                    window.push_notification(
                        Notification::success(format!(
                            "Test URL cleared — probes use the default ({DEFAULT_TEST_URL})"
                        )),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            }
        } else {
            match normalize_test_url(trimmed) {
                Ok(url) => match self.store.set_test_url(&url) {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(format!("Test URL set — probes use {url}")),
                            cx,
                        );
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                },
                Err(reason) => {
                    window.push_notification(
                        Notification::error(format!("Invalid test URL: {reason}")),
                        cx,
                    );
                    false
                }
            }
        }
    }

    fn commit_update_feed_url(
        &mut self,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            match self.store.clear_update_feed_url() {
                Ok(()) => {
                    window.push_notification(
                        Notification::success(format!(
                            "Feed URL cleared — updates check {UPDATE_FEED_URL_DEFAULT}"
                        )),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            }
        } else {
            match normalize_update_feed_url(trimmed) {
                Ok(url) => {
                    let shown = redact_subscription_url(&url);
                    match self.store.set_update_feed_url(&url) {
                        Ok(()) => {
                            if matches!(
                                store::subscription_fetch_policy(trimmed),
                                store::SubscriptionFetchPolicy::Warn { .. }
                            ) {
                                window.push_notification(
                                    Notification::warning(format!(
                                        "Feed URL set over plaintext http — prefer https: {shown}"
                                    )),
                                    cx,
                                );
                            } else {
                                window.push_notification(
                                    Notification::success(format!(
                                        "Feed URL set — updates check {shown}"
                                    )),
                                    cx,
                                );
                            }
                            true
                        }
                        Err(error) => {
                            self.report_save_error(error, window, cx);
                            false
                        }
                    }
                }
                Err(reason) => {
                    window.push_notification(
                        Notification::error(format!("Invalid feed URL: {reason}")),
                        cx,
                    );
                    false
                }
            }
        }
    }

    fn commit_row_test_url(
        &mut self,
        id: &str,
        raw: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = self.store.row_test_url(id).unwrap_or_default();
        let trimmed = raw.trim();
        if trimmed == current.trim() {
            return true;
        }
        let name = self
            .store
            .get(id)
            .ok()
            .flatten()
            .map(|row| row.name)
            .unwrap_or_else(|| id.to_string());
        if trimmed.is_empty() {
            match self.store.clear_row_test_url(id) {
                Ok(()) => {
                    window.push_notification(
                        Notification::success(format!(
                            "Test URL for \"{name}\" cleared — uses the global"
                        )),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            }
        } else {
            match normalize_test_url(trimmed) {
                Ok(url) => match self.store.set_row_test_url(id, &url) {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(format!(
                                "Test URL for \"{name}\" set — probes use {url}"
                            )),
                            cx,
                        );
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                },
                Err(reason) => {
                    window.push_notification(
                        Notification::error(format!("Invalid test URL: {reason}")),
                        cx,
                    );
                    false
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_edit_with_test_url(
        &mut self,
        id: &str,
        patch: ConfigPatch,
        raw_url: &str,
        form: &ConfigForm,
        form_error: &Rc<RefCell<Option<(String, String)>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.commit_edit_with_focus(id, patch, form, form_error, window, cx) {
            return false;
        }
        if self.commit_row_test_url(id, raw_url, window, cx) {
            // T-19: the edit committed — release the row-under-edit protection.
            self.edit_form_open = None;
            return true;
        }
        *form_error.borrow_mut() = Some((
            "test_url".to_string(),
            "Test URL rejected — fix it or clear the field".to_string(),
        ));
        form.focus_field("test_url", window, cx);
        cx.notify();
        false
    }

    fn commit_clear_traffic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.clear_all_traffic() {
            Ok(count) => {
                self.traffic_last_seen.clear();
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Cleared traffic for {count} row(s)")),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot clear traffic: {error}")),
                    cx,
                );
            }
        }
    }

    fn commit_clear_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.clear_health_snapshots() {
            Ok(count) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Cleared {count} health snapshot(s)")),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot clear health: {error}")),
                    cx,
                );
            }
        }
    }

    fn commit_clear_snapshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.clear_running_snapshot() {
            Ok(()) => {
                self.last_snapshot = Vec::new();
                window.push_notification(
                    Notification::success("Running snapshot cleared — next launch starts clean"),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot clear snapshot: {error}")),
                    cx,
                );
            }
        }
    }

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

    #[cfg(test)]
    fn toggle_selected_enabled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        let id = config.id.clone();
        let enabled = !config.enabled;
        self.commit_enabled_flag(&id, enabled, window, cx);
    }

    /// T-03: the one SOCKS copy. It reads the row by id and uses the STORED
    /// bind host, so the row's own `Copy proxy` and the inspector's SOCKS
    /// control always copy the row they belong to — never the selection.
    pub(crate) fn copy_row_socks(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let (socks, _) = proxy_urls(&config.bind_ip, config.port);
        cx.write_to_clipboard(ClipboardItem::new_string(socks));
        window.push_notification(Notification::success("Copied SOCKS URL"), cx);
    }

    fn copy_single_endpoint_url(
        &mut self,
        which: EndpointUrl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(config) = self.selected_config(cx) {
            // T-03: SOCKS has one path — the row-scoped copy.
            if matches!(which, EndpointUrl::Socks) {
                self.copy_row_socks(&config.id, window, cx);
                return;
            }
            let (_, http) = proxy_urls(&config.bind_ip, config.port);
            let (url, name) = match which {
                EndpointUrl::Socks => (None, "SOCKS"),
                EndpointUrl::Http => (Some(http), "HTTP"),

                EndpointUrl::Upstream => (upstream_host_port(&config), "upstream"),
            };
            match url {
                Some(url) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                    window
                        .push_notification(Notification::success(format!("Copied {name} URL")), cx);
                }
                None => {
                    window.push_notification(
                        Notification::info(format!("No upstream endpoint for \"{}\"", config.name)),
                        cx,
                    );
                }
            }
        } else {
            window.push_notification(Notification::info(no_selection_message()), cx);
        }
    }

    fn copy_selected_urls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.copy_row_urls(&config.id, window, cx);
    }

    fn copy_row_urls(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let (socks, http) = proxy_urls(&config.bind_ip, config.port);
        cx.write_to_clipboard(ClipboardItem::new_string(format!("{socks}\n{http}")));
        window.push_notification(Notification::success("Copied proxy URLs"), cx);
    }

    fn poll_engine(&mut self) -> Option<String> {
        self.engine.poll()
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

    #[cfg(test)]
    fn restart_crashed_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let crashed = self.engine.crashed_shards();
        if crashed.is_empty() {
            window.push_notification(Notification::info("No crashed endpoints to restart"), cx);
            return;
        }
        match self.engine.restart_crashed() {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!("Restarted {}", crashed.join(", "))),
                    cx,
                );
            }
            Err(error) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::error(format!(
                        "Restart failed: {}",
                        redact_secrets(&error.to_string())
                    )),
                    cx,
                );
            }
        }
    }

    fn warn_for_lan_binds(&self, window: &mut Window, cx: &mut App) {
        for warning in self.engine.lan_warnings() {
            window.push_notification(Notification::warning(warning), cx);
        }
    }

    fn start_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.start_row(&config.id, window, cx);
    }

    fn start_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };

        if self.libraries.read(cx).delegate().endpoint_is_dirty(id, cx) {
            window.push_notification(Notification::info(dirty_endpoint_message(&config.name)), cx);
            return;
        }
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        if requires_lan_confirm(&config.bind_ip) {
            self.open_lan_confirm_single(config, window, cx);
            return;
        }

        if cfg!(test) {
            self.commit_start_selected(&config.id, window, cx);
        } else {
            self.spawn_commit_start_selected(config.id.clone(), window, cx);
        }
    }

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

        if let Err(reason) = self.sync_presets_to_engine() {
            self.start_in_flight = false;
            window.push_notification(Notification::error(format!("Cannot start: {reason}")), cx);
            return;
        };
        let was_running = self.engine.is_running(id);
        if let Some(warning) = lan_exposure_warning(&config.name, &config.bind_ip) {
            window.push_notification(Notification::warning(warning), cx);
        }
        match self.engine.start_one_confirmed(&config) {
            Ok(()) => match self.store.set_enabled(id, true) {
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
            },
            Err(error) => {
                window
                    .push_notification(Notification::error(redact_secrets(&error.to_string())), cx);
            }
        }
        self.start_in_flight = false;
    }

    fn stop_selected_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.stop_row(&config.id, window, cx);
    }

    fn stop_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let id = config.id.clone();
        let (removed, stop_error) = self.engine.stop_one_result(&id);
        if removed {
            if let Some(error) = stop_error {
                self.reload(window, cx);
                window.push_notification(
                    Notification::error(format!(
                        "Stop of \"{}\" failed: {}",
                        config.name,
                        redact_secrets(&error.to_string())
                    )),
                    cx,
                );
                return;
            }

            match self.store.set_enabled(&id, false) {
                Ok(_) => {
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(format!(
                            "Stopped \"{}\" — flag cleared",
                            config.name
                        )),
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
            window.push_notification(
                Notification::warning(already_stopped_message(&config.name)),
                cx,
            );
        }
    }

    fn toggle_global_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        if self.engine.running_count() == 0 {
            self.start_all_engines(window, cx);
        } else {
            self.stop_all_engines(window, cx);
        }
    }

    fn start_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }

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
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text("Start all"),
                    )
                    .title(LAN_CONFIRM_TITLE)
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| view.commit_start_all(window, cx));
                        true
                    })
            });
            return;
        }
        if cfg!(test) {
            self.commit_start_all(window, cx);
        } else {
            self.spawn_commit_start_all(window, cx);
        }
    }

    fn commit_start_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start_in_flight {
            window.push_notification(Notification::info(start_busy_message()), cx);
            return;
        }
        self.start_in_flight = true;

        let Some(rows) = self.list_or_notify(window, cx) else {
            self.start_in_flight = false;
            return;
        };
        if rows.is_empty() {
            window.push_notification(Notification::warning(start_all_message(0)), cx);
            self.start_in_flight = false;
            return;
        }
        if rows.iter().all(|row| self.engine.is_running(&row.id)) {
            self.start_in_flight = false;
            window.push_notification(
                Notification::info(format!("All {} endpoint(s) already running", rows.len())),
                cx,
            );
            return;
        }

        if let Err(reason) = self.sync_presets_to_engine() {
            self.start_in_flight = false;
            window.push_notification(Notification::error(format!("Cannot start: {reason}")), cx);
            return;
        }
        match self.engine.start_all_confirmed(&rows) {
            Ok(()) => {
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
                if !is_row_local_start_error(&fast_error) {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    window.push_notification(
                        Notification::error(redact_secrets(&fast_error.to_string())),
                        cx,
                    );
                    return;
                }

                let outcome = self.engine.start_all_subset(&rows, true);
                if let Some(error) = outcome.error.as_deref() {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    window.push_notification(Notification::error(redact_secrets(error)), cx);
                    return;
                }
                let started: Vec<ProxyConfig> = rows
                    .iter()
                    .filter(|row| outcome.applied.contains(&row.id))
                    .cloned()
                    .collect();
                let failed_names: Vec<String> = outcome
                    .failed
                    .iter()
                    .map(|(name, _)| format!("\"{name}\""))
                    .collect();
                if started.is_empty() {
                    self.reload(window, cx);
                    self.start_in_flight = false;
                    let reason = outcome
                        .failed
                        .first()
                        .map(|(_, cause)| cause.clone())
                        .unwrap_or_else(|| fast_error.to_string());
                    window.push_notification(Notification::error(redact_secrets(&reason)), cx);
                    return;
                }

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

                if !outcome.warnings.is_empty() {
                    window
                        .push_notification(Notification::warning(outcome.warnings.join("; ")), cx);
                }
            }
        }
    }

    fn stop_all_engines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.engine.running_count() == 0 {
            let message = stop_all_message(0);
            window.push_notification(Notification::warning(message), cx);
            return;
        }
        let names = self.running_names_for_close();
        let view = cx.entity();
        let description = stop_all_confirm_description(&names);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Stop all"),
                )
                .title("Stop all endpoints")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_stop_all(window, cx));
                    true
                })
        });
    }

    fn commit_stop_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let running = self.engine.running_count();
        let stop_error = self.engine.stop_all_result().map(|error| error.to_string());
        if let Some(error) = stop_error {
            self.reload(window, cx);
            window.push_notification(
                Notification::error(format!("Stop-all failed: {}", redact_secrets(&error))),
                cx,
            );
            return;
        }

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

    /// T-32: mirrors the window rectangle into meta on the same orderly-exit
    /// paths `snapshot_running_set` already uses. A killed process keeps the
    /// last clean-exit save — the snapshot's own tolerance. Maximized
    /// rectangles are not persisted (out of scope): the pre-maximize windowed
    /// rect from the previous clean exit stays current.
    fn save_window_geometry(&mut self, window: &Window) {
        if !matches!(window.window_bounds(), WindowBounds::Windowed(_)) {
            return;
        }
        let bounds = window.bounds();
        let geometry = WindowGeometry {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };
        if let Err(error) = self.store.set_window_geometry(geometry) {
            eprintln!("window geometry save failed: {error}");
        }
    }

    fn snapshot_running_set(&mut self, running: &[String]) {
        if self.last_snapshot.as_slice() == running {
            return;
        }
        self.last_snapshot = running.to_vec();
        if let Err(error) = self.store.save_running_snapshot(running) {
            eprintln!("running snapshot failed: {error}");
        }
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

    fn copy_config_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = crate::tray::config_folder();
        cx.write_to_clipboard(ClipboardItem::new_string(folder.clone()));
        window.push_notification(
            Notification::success(format!("Copied config folder path ({folder})")),
            cx,
        );
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

    fn finish_update_check(
        &mut self,
        fetched: Result<Option<FetchedRelease>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_check_in_flight = false;
        // T-04: the check happened — remember when, so the Settings → Updates
        // section can say so.
        self.record_update_check();
        cx.notify();
        match update_check_outcome(fetched) {
            Ok(outcome) => self.open_update_check_dialog(outcome, window, cx),
            Err(why) => {
                // every remaining failure is a plain error notification; the
                // strings carry no URL or body echoes, only the fixed cause
                window.push_notification(
                    Notification::error(format!("Update check failed: {why}")),
                    cx,
                );
            }
        }
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

    fn finish_update_check_no_window(
        &mut self,
        _fetched: Result<Option<FetchedRelease>, String>,
        cx: &mut Context<Self>,
    ) {
        self.update_check_in_flight = false;
        cx.notify();
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

    fn health_tones(&self) -> HashMap<String, LatencyTone> {
        self.engine
            .running_ids()
            .iter()
            .filter(|id| !self.health_store.row_summary(id).trim().is_empty())
            .map(|id| (id.clone(), self.health_store.health_tone(id)))
            .collect()
    }

    fn health_counts(&self) -> (usize, usize) {
        self.health_store.counts(&self.engine.running_ids())
    }

    fn health_status_text(&self) -> String {
        let (live, dead) = self.health_counts();
        if live == 0 && dead == 0 {
            "health: no probes this session".to_string()
        } else {
            format!("health: {live} live · {dead} dead")
        }
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

    fn probe_kind_in_flight(&self, ping: bool) -> bool {
        if ping {
            self.probe_ping_in_flight
        } else {
            self.probe_url_in_flight
        }
    }

    fn test_all_health(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let Some(rows) = self.running_rows_or_notify(window, cx) else {
            return 0;
        };
        // T-04: the scope decides which running rows the run covers.
        let rows = if self.store.test_all_scope() == TestAllScope::Checked {
            let checked: HashSet<String> = self
                .libraries
                .read(cx)
                .delegate()
                .checked_rows()
                .into_iter()
                .map(|row| row.id)
                .collect();
            rows.into_iter()
                .filter(|row| checked.contains(&row.id))
                .collect()
        } else {
            rows
        };
        if rows.is_empty() {
            window.push_notification(Notification::warning("No running endpoints to test"), cx);
            return 0;
        }
        if self.probe_kind_in_flight(false) {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return 0;
        }
        self.spawn_probe(rows, false, true, Some(window.window_handle()), cx)
    }

    fn test_selected_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.test_row(&config.id, window, cx);
    }

    fn test_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        if self.probe_kind_in_flight(false) {
            window.push_notification(Notification::info("A health test is already running"), cx);
            return;
        }
        self.spawn_probe(vec![config], false, true, Some(window.window_handle()), cx);
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

    fn test_group_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let rows = self.group_running_rows(&group);
        if rows.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "Group \"{group}\" has no running endpoints to test"
                )),
                cx,
            );
            return;
        }
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.spawn_group_probe(rows, group, Some(window.window_handle()), cx);
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

    fn stop_group_tests(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        if !self.group_test_in_flight {
            window.push_notification(Notification::info("No group test is running"), cx);
            return;
        }
        self.probe_stop.store(true, Ordering::SeqCst);
        window.push_notification(Notification::info("Stopping the group test…"), cx);
    }

    fn speedtest_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.speedtest_rows(vec![config], None, Some(window.window_handle()), cx);
    }

    fn speedtest_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        self.speedtest_rows(vec![config], None, Some(window.window_handle()), cx);
    }

    fn speedtest_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let rows = self.group_running_rows(&group);
        if rows.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "Group \"{group}\" has no running endpoints to speedtest"
                )),
                cx,
            );
            return;
        }
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.speedtest_rows(rows, Some(group), Some(window.window_handle()), cx);
    }

    fn speedtest_rows(
        &mut self,
        rows: Vec<ProxyConfig>,
        group: Option<String>,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        use std::sync::atomic::Ordering;
        if rows.is_empty() || self.group_test_in_flight {
            return;
        }
        let mode = self.speedtest_mode;
        let config = SpeedtestConfig {
            mode,
            timeout: SPEED_TEST_TIMEOUT,
        };
        let url = self.effective_test_url(&rows);
        let scope = group.unwrap_or_else(|| {
            rows.first()
                .map(|row| format!("\"{}\"", row.name))
                .unwrap_or_default()
        });
        let single_name: Option<String> = if rows.len() == 1 {
            rows.first().map(|row| row.name.clone())
        } else {
            None
        };
        self.group_test_in_flight = true;
        self.probe_stop.store(false, Ordering::SeqCst);
        let stop = self.probe_stop.clone();
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let (outcomes, prober) = cx
                .background_executor()
                .spawn(async move {
                    let prober = prober;
                    let outcomes = speed_group(
                        &rows,
                        &url,
                        config.mode,
                        config.timeout,
                        &*prober,
                        &stop,
                        |_, _| {},
                    );
                    (outcomes, prober)
                })
                .await;
            cx.update(|cx| {
                let summary = weak.update(cx, |view, _| {
                    view.prober = prober;
                    view.group_test_in_flight = false;
                    view.record_and_persist_speed(&outcomes);
                    let measured = outcomes.iter().filter(|o| o.mbps.is_some()).count();
                    let failed = outcomes.len() - measured;
                    let single = single_name.as_deref().and_then(|name| {
                        outcomes
                            .first()
                            .map(|outcome| speed_result_toast(name, outcome))
                    });
                    (measured, failed, outcomes.len(), single)
                });
                let Ok((measured, failed, count, single)) = summary else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };
                let message = single.unwrap_or_else(|| {
                    if count == 1 {
                        format!("Speedtest {scope}: {measured} measured · {failed} failed")
                    } else {
                        speed_test_summary_toast(&scope, mode, measured, failed)
                    }
                });
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

    fn cycle_speedtest_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.speedtest_mode = self.speedtest_mode.next();
        window.push_notification(
            Notification::info(format!("Speedtest mode: {}", self.speedtest_mode.as_str())),
            cx,
        );
    }

    fn commit_resolve_domain_group(
        &mut self,
        candidates: &[(String, String, String)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if candidates.is_empty() || self.group_test_in_flight {
            return;
        }
        self.group_test_in_flight = true;
        self.probe_stop
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let stop = self.probe_stop.clone();
        let handle = window.window_handle();
        let weak = cx.entity().downgrade();
        let candidates = candidates.to_vec();
        let store_snapshot: Vec<(String, String)> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|row| (row.id, row.params))
            .collect();
        cx.spawn(async move |_entity, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    let mut rewritten = Vec::new();
                    let mut failed = Vec::new();
                    for (id, name, host) in &candidates {
                        if stop.load(std::sync::atomic::Ordering::SeqCst) {
                            break;
                        }
                        let params = store_snapshot
                            .iter()
                            .find(|(row_id, _)| row_id == id)
                            .map(|(_, params)| params.clone())
                            .unwrap_or_default();
                        match resolve_hostname(host, RESOLVE_TIMEOUT) {
                            Some(ip) => match rewrite_server_to_ip(&params, &ip.to_string()) {
                                Some(new_params) => rewritten.push((
                                    id.clone(),
                                    name.clone(),
                                    host.clone(),
                                    new_params,
                                )),
                                None => failed.push(format!("\"{name}\": no domain server")),
                            },
                            None => failed.push(format!("\"{name}\": could not resolve {host}")),
                        }
                    }
                    (rewritten, failed)
                })
                .await;
            cx.update(|cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.group_test_in_flight = false;
                        let (rewritten, failed) = done;
                        let mut saved: Vec<(String, String, String)> = Vec::new();
                        let mut save_failed = failed;
                        let mut any_running = false;
                        for (id, name, host, new_params) in rewritten {
                            if view.engine.is_running(&id) {
                                any_running = true;
                            }
                            let patch = ConfigPatch {
                                params: Some(new_params),
                                ..Default::default()
                            };
                            match view.store.update(&id, patch) {
                                Ok(updated) => {
                                    let new_host = row_server_hostname(&updated.params)
                                        .unwrap_or_else(|| updated.params.clone());
                                    saved.push((name, host, new_host));
                                }
                                Err(error) => save_failed.push(format!("\"{name}\": {error}")),
                            }
                        }
                        view.reload(window, cx);
                        let (severity, message) =
                            resolve_domain_group_notification(&saved, &save_failed, any_running);
                        window.push_notification(
                            Notification::new().message(message).with_type(severity),
                            cx,
                        );
                    });
                });
            });
        })
        .detach();
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

    fn commit_prune(&mut self, kind: GroupPruneKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };

        let targets: Vec<ProxyConfig> = match kind {
            GroupPruneKind::Duplicates => self
                .store
                .duplicate_rows_in_group(&group)
                .unwrap_or_default(),
            GroupPruneKind::Unavailable => self
                .store
                .unavailable_rows_in_group(&group)
                .unwrap_or_default(),
            GroupPruneKind::Invalid => self.store.invalid_rows_in_group(&group).unwrap_or_default(),
            GroupPruneKind::ClearTests => match self.store.clear_tests_in_group(&group) {
                Ok(cleared) => {
                    self.health_store_prune_group(&group);
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(format!(
                            "Cleared {cleared} test result(s) in group \"{group}\""
                        )),
                        cx,
                    );
                    return;
                }
                Err(error) => {
                    window.push_notification(Notification::error(error.to_string()), cx);
                    return;
                }
            },
        };
        // Stop every candidate first: a row that refuses to stop must stay in
        // the library so the engine never orphans a live listener (BUG-07).
        let mut skip: HashSet<String> = HashSet::new();
        let mut skipped: Vec<String> = Vec::new();
        for row in &targets {
            let (was_active, stop_error) = self.engine.stop_one_result(&row.id);
            if was_active && stop_error.is_some() {
                skip.insert(row.id.clone());
                skipped.push(row.name.clone());
            }
        }
        let pruned = match kind {
            GroupPruneKind::Duplicates => self.store.remove_duplicates_in_group(&group, &skip),
            GroupPruneKind::Unavailable => self.store.remove_unavailable_in_group(&group, &skip),
            GroupPruneKind::Invalid => self.store.remove_invalid_in_group(&group, &skip),
            GroupPruneKind::ClearTests => Ok(Vec::new()),
        };
        let removed: Vec<String> = match pruned {
            Ok(rows) => rows.into_iter().map(|row| row.name).collect(),
            Err(error) => {
                window.push_notification(Notification::error(error.to_string()), cx);
                return;
            }
        };
        self.reload(window, cx);
        if removed.is_empty() && skipped.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "{} in group \"{group}\": nothing removed — rows changed",
                    kind.label()
                )),
                cx,
            );
            return;
        }
        if removed.is_empty() {
            window.push_notification(
                Notification::error(format!(
                    "{} in group \"{group}\": nothing removed — {} row(s) could not be stopped ({})",
                    kind.label(),
                    skipped.len(),
                    skipped.join(", ")
                )),
                cx,
            );
            return;
        }
        let shown: Vec<&str> = removed.iter().take(3).map(String::as_str).collect();
        let mut text = format!(
            "{} in group \"{group}\": {} removed ({})",
            kind.label(),
            removed.len(),
            shown.join(", ")
        );
        if removed.len() > shown.len() {
            text.push_str(&format!(" (+{} more)", removed.len() - shown.len()));
        }
        if skipped.is_empty() {
            window.push_notification(Notification::success(text), cx);
        } else {
            window.push_notification(
                Notification::warning(format!(
                    "{text} — {} could not be stopped ({})",
                    skipped.len(),
                    skipped.join(", ")
                )),
                cx,
            );
        }
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

    /// T-19: only this confirm's ACK enables the flag — the note is
    /// auto-clear's own, and the store rejects any enable without note_seen.
    fn commit_auto_clear_opt_in(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = AutoClearGroupConfig {
            enabled: true,
            on_date: now_unix(),
            note_seen: true,
        };
        match self.store.set_auto_clear_group(group, config) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Auto-clear unavailable enabled for \"{group}\""
                    )),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot enable auto-clear: {error}")),
                    cx,
                );
            }
        }
    }

    fn commit_auto_clear_off(&mut self, group: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (on_date, note_seen) = match self.store.auto_clear_group(group) {
            Some(config) => (config.on_date, config.note_seen),
            None => (0, false),
        };
        let config = AutoClearGroupConfig {
            enabled: false,
            on_date,
            note_seen,
        };
        match self.store.set_auto_clear_group(group, config) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Auto-clear unavailable disabled for \"{group}\""
                    )),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot disable auto-clear: {error}")),
                    cx,
                );
            }
        }
    }

    fn health_store_prune_group(&mut self, group: &str) {
        let ids: HashSet<String> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == group)
            .map(|row| row.id)
            .collect();
        self.health_store.prune_configs(&ids);
    }

    fn toggle_show_security(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let show = !self.libraries.read(cx).delegate().show_security;
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_show_security(show);
            list_cx.notify();
        });
        window.push_notification(
            Notification::info(if show {
                "Show security: on".to_string()
            } else {
                "Show security: off".to_string()
            }),
            cx,
        );
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

    fn toggle_library_sort_direction(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.libraries.read(cx).delegate().sort;
        self.commit_library_sort(
            LibrarySort::new(current.key, current.direction.toggled()),
            window,
            cx,
        );
    }

    fn commit_library_sort(
        &mut self,
        sort: LibrarySort,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_sort(sort);
            list_cx.notify();
        });
        if let Err(error) = self.store.set_library_sort(sort) {
            window.push_notification(
                Notification::error(format!("Could not save sort: {error}")),
                cx,
            );
        }
        window.push_notification(
            Notification::info(if sort.is_none() {
                "Sort: library order".to_string()
            } else {
                format!("Sort: {} · {}", sort.key.label(), sort.direction.label())
            }),
            cx,
        );
        cx.notify();
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

    fn drain_engine_logs(&mut self, active: &[ProxyConfig]) {
        let lines = self.engine.drain_log_lines();
        if lines.is_empty() {
            return;
        }

        let tags = LogTagIndex::build(active);
        for line in &lines {
            self.logs.push_raw(line, &tags);
        }
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

    fn write_visible_logs_to(&self, path: &std::path::Path, cx: &App) -> Result<(), String> {
        let body = self.visible_log_lines(cx).join("\n");
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, body).map_err(|error| format!("Cannot save logs: {error}"))?;
        std::fs::rename(&tmp, path).map_err(|error| format!("Cannot save logs: {error}"))?;
        Ok(())
    }

    fn save_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = Store::default_logs_path();
        if path.exists() {
            let view = cx.entity();
            let description = logs_overwrite_text(&path);
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text("Overwrite")
                            .ok_variant(ButtonVariant::Danger),
                    )
                    .title("Overwrite saved logs?")
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| view.commit_save_visible_logs(window, cx));
                        true
                    })
            });
            return;
        }
        self.commit_save_visible_logs(window, cx);
    }

    fn commit_save_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = Store::default_logs_path();
        match self.write_visible_logs_to(&path, cx) {
            Ok(()) => window.push_notification(
                Notification::success(format!("Logs saved to {}", path.display())),
                cx,
            ),
            Err(error) => window.push_notification(Notification::error(error), cx),
        }
    }

    fn copy_visible_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lines = self.visible_log_lines(cx);
        if lines.is_empty() {
            window.push_notification(Notification::warning("No log lines to copy"), cx);
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
        window.push_notification(
            Notification::success(format!("Copied {} log line(s)", lines.len())),
            cx,
        );
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

    fn toggle_log_tail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.log_tail = !self.log_tail;
        window.push_notification(
            Notification::info(if self.log_tail {
                "Log tail resumed"
            } else {
                "Log tail paused"
            }),
            cx,
        );
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

    #[cfg(test)]
    fn apply_log_filter(&mut self, filter: String, cx: &mut Context<Self>) {
        self.log_filter = filter;
        cx.notify();
    }

    fn apply_log_filters(
        &mut self,
        include: String,
        exclude: String,
        pattern: String,
        cx: &mut Context<Self>,
    ) {
        self.log_filter = include;
        self.log_exclude = exclude;
        self.log_regex = pattern;
        cx.notify();
    }

    /// T-31: persist `log_max_lines` and rebuild the ring at the clamped cap.
    /// The rebuild preserves nothing: changing the cap mid-session truncates
    /// every present record for the fresh ring (no partial-growth state), which
    /// the Settings note beside the field states.
    fn commit_log_cap(&mut self, window: &mut Window, raw: &str, cx: &mut Context<Self>) {
        let Some(cap) = parse_log_cap(raw) else {
            window.push_notification(
                Notification::warning(format!(
                    "Maximum kept log lines must be a whole number (500–50,000) — keeping {}",
                    self.logs.cap()
                )),
                cx,
            );
            return;
        };
        if let Err(error) = self.store.set_log_max_lines(cap) {
            window.push_notification(
                Notification::error(format!("Cannot save the log cap: {error}")),
                cx,
            );
            return;
        }
        if let Some(input) = self.log_cap_input.clone() {
            input.update(cx, |state, cx| state.set_value(cap.to_string(), window, cx));
        }
        if cap == self.logs.cap() {
            return;
        }
        self.logs = LogStore::with_capacity(cap);
        self.logs_seen = 0;
        if let Some(list) = self.log_list.clone() {
            let lines = self.log_lines(cx);
            list.update(cx, |state, cx| {
                state.delegate_mut().lines = lines;
                cx.notify();
            });
        }
        cx.notify();
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

    fn commit_export_dialog(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            window.push_notification(Notification::error("Export path must not be empty"), cx);
            return false;
        }
        if std::path::Path::new(trimmed).exists() {
            let view = cx.entity();
            let description = export_overwrite_confirm_description(trimmed);
            let target = trimmed.to_string();
            open_app_alert_dialog(window, cx, move |alert, _, _| {
                let view = view.clone();
                let target = target.clone();
                let description = description.clone();
                alert
                    .confirm()
                    .button_props(
                        DialogButtonProps::default()
                            .show_cancel(true)
                            .ok_text("Overwrite")
                            .ok_variant(ButtonVariant::Danger),
                    )
                    .title("Overwrite export?")
                    .description(description)
                    .on_ok(move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.commit_export_write_to(&target, window, cx)
                        });
                        true
                    })
            });
            return false;
        }
        self.commit_export_write_to(trimmed, window, cx);
        true
    }

    fn commit_export_write_to(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        let target = std::path::PathBuf::from(path.trim());
        match self.store.commit_export(&target, true) {
            Ok(Some(path)) => {
                let rendered = path.display().to_string();

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

        if let Some(live_path) = self.store.db_path() {
            match Store::restore_library_file(&live_path, payload, now_unix()) {
                Ok((outcome, backup)) => {
                    let backup_text = backup.as_deref().map(|path| path.display().to_string());
                    let backup_text = backup_text.as_deref().unwrap_or("(no prior library file)");

                    match Store::open_file(&live_path) {
                        Ok(fresh) => {
                            self.store = fresh;
                            self.reload(window, cx);
                            window.push_notification(
                                Notification::success(format!(
                                    "{} (backup {})",
                                    restore_done_message(&outcome),
                                    backup_text
                                )),
                                cx,
                            );
                        }
                        Err(error) => {
                            self.reload(window, cx);
                            window.push_notification(
                                Notification::error(format!(
                                    "Restore swapped but reopen failed ({error}); backup at {backup_text}"
                                )),
                                cx,
                            );
                        }
                    }
                    return;
                }
                Err(error) => {
                    window.push_notification(
                        Notification::error(format!("Restore failed: {error}")),
                        cx,
                    );
                    return;
                }
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

    fn commit_usage_reset(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.reset_traffic_counters(id) {
            Ok(()) => {
                self.reload(window, cx);
                window.push_notification(Notification::success("Usage counters reset to zero"), cx);
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot reset usage: {error}")),
                    cx,
                );
            }
        }
    }

    fn paste_intake_refusal(text: &str) -> Option<&'static str> {
        use crate::preset::{is_remote_route_attempt, is_route_attempt, looks_like_preset_json};
        if text.trim().is_empty() {
            return Some("Clipboard has no text to import");
        }
        if is_library_export(text) {
            return None;
        }
        let trimmed = text.trim();
        let single_line = !trimmed.contains('\n');
        if single_line && is_addsub_attempt(trimmed) {
            return None;
        }
        if single_line && (is_route_attempt(trimmed) || is_remote_route_attempt(trimmed)) {
            return None;
        }
        if looks_like_preset_json(trimmed) {
            return None;
        }
        if import_text(text).configs.is_empty() {
            return Some(
                "Clipboard text is not importable — nothing imported (paste a share link, JSON, conf, or preset link)",
            );
        }
        None
    }

    fn commit_bare_url_subscription(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        let label = subscription_label_from_url(&url);
        self.commit_subscription_add(
            label,
            url,
            DEFAULT_GROUP.to_string(),
            true,
            store::DEFAULT_AUTO_REFRESH_SECS,
            window,
            cx,
        );
    }

    fn import_dropped_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if overlay_open(window, cx) {
            window.push_notification(
                Notification::warning(
                    "Close the open dialog first — dropped files were not imported",
                ),
                cx,
            );
            return;
        }
        let mut merged = String::new();
        let mut names: Vec<String> = Vec::new();
        let mut files: Vec<(String, String)> = Vec::new();
        let mut unreadable: Vec<String> = Vec::new();

        for path in paths.0.iter() {
            let display = path.to_string_lossy().into_owned();
            match read_import_file(path) {
                ImportFileRead::Text(content) => {
                    if !merged.is_empty() {
                        merged.push('\n');
                    }
                    merged.push_str(&content);
                    names.push(display.clone());
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

    fn finish_ssh_key_pick(
        &mut self,
        path_entity: Entity<InputState>,
        key_entity: Entity<TextareaState>,
        path: String,
        content: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(content) = content else {
            window.push_notification(Notification::warning(ssh_key_pick_error(&path)), cx);
            return;
        };
        path_entity.update(cx, |state, cx| {
            state.set_value(path.clone(), window, cx);
        });
        key_entity.update(cx, |state, cx| {
            state.set_value(content, window, cx);
        });
        window.push_notification(
            Notification::success(format!("Loaded SSH key from {path}")),
            cx,
        );
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

    fn finish_extra_exe_pick(
        &mut self,
        path_entity: Entity<InputState>,
        display: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stored = std::path::Path::new(&display)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| display.clone());
        path_entity.update(cx, |state, cx| {
            state.set_value(stored.clone(), window, cx);
        });
        window.push_notification(
            Notification::success(format!("Picked extra-core executable {stored}")),
            cx,
        );
    }

    fn finish_browse_pick(
        &mut self,
        current_text: String,
        current_group: String,
        files: Vec<(String, String)>,
        unreadable: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !unreadable.is_empty() {
            window.push_notification(
                Notification::warning(unreadable_drop_message(&unreadable)),
                cx,
            );
        }
        if files.is_empty() {
            return;
        }
        let names: Vec<String> = files.iter().map(|(name, _)| name.clone()).collect();
        let (merged, hint, multi) = merge_browse_results(&current_text, &files);
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        self.open_import_dialog_with_files(&merged, &current_group, hint, multi, window, cx);
        window.push_notification(Notification::success(browse_read_message(&names)), cx);
    }

    #[cfg(test)]
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

    #[cfg(test)]
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
        if let Some(done) = self.try_preset_commit(&resolved, window, cx) {
            return done;
        }
        let batch = parse_import_inputs(&resolved);
        self.finish_import_batch(batch, &resolved.group, window, cx)
    }

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
        if let Some(done) = self.try_preset_commit(&resolved, window, cx) {
            return done;
        }
        if self.import_parse_in_flight {
            return false;
        }
        self.import_parse_in_flight = true;
        self.import_parse_epoch = self.import_parse_epoch.wrapping_add(1);
        let epoch = self.import_parse_epoch;
        self.import_cancel_epoch = None;
        cx.notify();
        let weak = cx.entity().downgrade();

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
                                view.finish_import_parse(epoch, batch, group, window, cx);
                            });
                        }
                    })
                    .is_ok();
                if !delivered {
                    if let Some((batch, group)) = pending.take() {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_import_parse_no_window(epoch, batch, group, cx);
                        });
                    }
                }
            });
        })
        .detach();
        false
    }

    /// Marks the running (or just-finished) import parse as cancelled so its
    /// batch is dropped instead of landing after the dialog was dismissed.
    fn cancel_import_parse(&mut self, cx: &mut Context<Self>) {
        self.import_cancel_epoch = Some(self.import_parse_epoch);
        cx.notify();
    }

    fn import_parse_cancelled(&mut self, epoch: u64) -> bool {
        if self.import_cancel_epoch == Some(epoch) {
            self.import_cancel_epoch = None;
            true
        } else {
            false
        }
    }

    fn finish_import_parse(
        &mut self,
        epoch: u64,
        batch: crate::import::BatchResult,
        group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.import_parse_in_flight = false;
        if self.import_parse_cancelled(epoch) {
            return;
        }
        if self.finish_import_batch(batch, &group, window, cx) {
            window.close_all_dialogs(cx);
            reclaim_shell_focus(window, cx);
        }
    }

    fn finish_import_parse_no_window(
        &mut self,
        epoch: u64,
        batch: crate::import::BatchResult,
        group: String,
        cx: &mut Context<Self>,
    ) {
        self.import_parse_in_flight = false;
        if self.import_parse_cancelled(epoch) {
            return;
        }
        if commit_batch(&mut self.store, batch, &group).is_ok() {
            self.reload_quiet(cx);
        }
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

                let summary =
                    import_success_summary(outcome.imported, outcome.duplicates, &outcome.failures);
                if outcome.failures.is_empty() {
                    window.push_notification(Notification::success(summary), cx);
                } else {
                    window.push_notification(Notification::warning(summary), cx);
                }

                if !outcome.imported_ids.is_empty() {
                    let first = outcome.imported_ids.clone();
                    self.libraries.update(cx, |list_state, list_cx| {
                        let picked = {
                            let delegate = list_state.delegate_mut();
                            delegate.check_ids(&first);
                            first.iter().find_map(|id| {
                                delegate.visible.iter().position(|&visible_ix| {
                                    delegate
                                        .rows
                                        .get(visible_ix)
                                        .map(|row| &row.id == id)
                                        .unwrap_or(false)
                                })
                            })
                        };
                        // T-06: selection belongs to the list, not to a
                        // delegate field the kit never sees. Routing it
                        // through the list's own setter is what makes
                        // `ListItem::selected` and `aria_selected` name the
                        // imported row.
                        if let Some(pos) = picked {
                            list_state.set_selected_index(
                                Some(IndexPath::new(pos)),
                                window,
                                list_cx,
                            );
                        }
                        list_cx.notify();
                    });
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
            store::DEFAULT_AUTO_REFRESH_SECS,
            window,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_subscription_add(
        &mut self,
        label: String,
        url: String,
        group: String,
        auto_refresh: bool,
        interval_secs: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
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
                let interval_error = self
                    .store
                    .set_subscription_interval(&sub.id, interval_secs)
                    .err();
                self.reload(window, cx);
                match interval_error {
                    Some(error) => window.push_notification(
                        Notification::warning(subscription_interval_failure_message(
                            "added",
                            &sub.label,
                            &error.to_string(),
                        )),
                        cx,
                    ),
                    None => window.push_notification(
                        Notification::success(format!(
                            "Subscription \"{}\" added — refreshing",
                            sub.label
                        )),
                        cx,
                    ),
                }
                self.start_subscription_refresh(sub, true, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_subscription_edit(
        &mut self,
        id: &str,
        label: String,
        url: String,
        group: String,
        interval_secs: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.update_subscription(id, &label, &url, &group) {
            Ok(updated) => {
                let interval_error = self
                    .store
                    .set_subscription_interval(id, interval_secs)
                    .err();
                self.reload(window, cx);
                match interval_error {
                    Some(error) => window.push_notification(
                        Notification::warning(subscription_interval_failure_message(
                            "updated",
                            &updated.label,
                            &error.to_string(),
                        )),
                        cx,
                    ),
                    None => window.push_notification(
                        Notification::success(format!("Updated \"{}\"", updated.label)),
                        cx,
                    ),
                }
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

    fn take_batch_name_lists(&mut self) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
        (
            std::mem::take(&mut self.refresh_batch_added_names),
            std::mem::take(&mut self.refresh_batch_updated_names),
            std::mem::take(&mut self.refresh_batch_removed_names),
            std::mem::take(&mut self.refresh_batch_in_use_names),
        )
    }

    fn take_batch_counts(&mut self) -> (usize, usize, usize, usize, usize, usize, usize) {
        self.refresh_batch_members.clear();
        let total = std::mem::take(&mut self.refresh_batch_total);
        let added = std::mem::take(&mut self.refresh_batch_added);
        let updated = std::mem::take(&mut self.refresh_batch_updated);
        let removed = std::mem::take(&mut self.refresh_batch_removed);
        let failed = std::mem::take(&mut self.refresh_batch_failed);
        let conflicts = std::mem::take(&mut self.refresh_batch_conflicts);
        let in_use = std::mem::take(&mut self.refresh_batch_in_use);
        self.refresh_batch_added_names.clear();
        self.refresh_batch_updated_names.clear();
        self.refresh_batch_removed_names.clear();
        self.refresh_batch_in_use_names.clear();
        (total, added, updated, removed, failed, conflicts, in_use)
    }

    fn flush_refresh_batch(&mut self, window: &mut Window, cx: &mut App) {
        let (added_names, updated_names, removed_names, kept_in_use_names) =
            self.take_batch_name_lists();
        let (total, added, updated, removed, failed, conflicts, in_use) = self.take_batch_counts();
        let (warn, mut text) =
            coalesced_refresh_message(total, added, updated, removed, failed, conflicts, in_use)
                .unwrap_or((false, String::new()));
        if text.is_empty() {
            return;
        }
        if let Some(report) = refresh_named_report_block(
            &added_names,
            &updated_names,
            &removed_names,
            &kept_in_use_names,
        ) {
            text.push('\n');
            text.push_str(&report);
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

    fn note_refresh_started(&mut self, sub_id: &str) -> bool {
        self.inflight_refreshes.insert(sub_id.to_string())
    }

    fn start_subscription_refresh(
        &mut self,
        sub: ProxySubscription,
        named: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.note_refresh_started(&sub.id) {
            return false;
        }
        if named {
            self.inflight_named_refreshes.insert(sub.id.clone());
        }
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        let sub_id = sub.id.clone();
        let label = sub.label.clone();
        let url = sub.url.clone();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_subscription_body(client, &url, resolve_subscription_host).await;
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

    fn finish_subscription_refresh(
        &mut self,
        sub_id: &str,
        label: &str,
        fetched: Result<FetchedSubscription, String>,
        now: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inflight_refreshes.remove(sub_id);
        let named = self.inflight_named_refreshes.remove(sub_id);

        match self.store.get_subscription(sub_id) {
            Ok(None) => {
                self.reload(window, cx);

                self.close_batch_slot(sub_id, window, cx);
                self.reopen_subscriptions_if_open(window, cx);
                return;
            }
            Err(error) => {
                self.push_refresh_log(&format!(
                    "error cannot refresh subscription \"{label}\": {error}"
                ));
                self.route_refresh_completion(
                    sub_id,
                    Notification::error(format!("Cannot refresh subscription: {error}")),
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    &[],
                    &[],
                    &[],
                    &[],
                    window,
                    cx,
                );
                return;
            }
            Ok(Some(_)) => {}
        }

        let running: HashSet<String> = self.engine.running_ids().into_iter().collect();
        match fetched {
            Ok(fetched) => {
                match self
                    .store
                    .refresh_with_body_running(sub_id, &fetched.body, now, &running)
                {
                    Ok(outcome) => {
                        self.reload(window, cx);
                        if let Some(error) = outcome.error {
                            self.push_refresh_log(&format!(
                                "error subscription refresh \"{label}\": refresh failed ({error})"
                            ));
                            self.route_refresh_completion(
                                sub_id,
                                Notification::error(format!("{label}: refresh failed ({error})")),
                                0,
                                0,
                                0,
                                1,
                                0,
                                0,
                                &[],
                                &[],
                                &[],
                                &[],
                                window,
                                cx,
                            );
                        } else {
                            let _ = self.store.set_subscription_quota(sub_id, fetched.userinfo);
                            let failed = outcome.failures.len();
                            let (added_names, updated_names, removed_names, kept_in_use_names) =
                                if named {
                                    (
                                        outcome.added_names.as_slice(),
                                        outcome.updated_names.as_slice(),
                                        outcome.removed_names.as_slice(),
                                        outcome.kept_in_use_names.as_slice(),
                                    )
                                } else {
                                    let empty: &[String] = &[];
                                    (empty, empty, empty, empty)
                                };
                            let (warn, summary) = refresh_summary_text(
                                label,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                outcome.skipped,
                                outcome.duplicates,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                            );

                            if warn {
                                self.push_refresh_log(&format!(
                                    "warning subscription refresh {summary}"
                                ));
                            } else {
                                self.push_refresh_log(&format!("subscription refresh {summary}"));
                            }
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
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                                window,
                                cx,
                            );
                        }
                    }
                    Err(error) => {
                        if matches!(error, StoreError::NotFound(_)) {
                            self.reload(window, cx);
                            self.close_batch_slot(sub_id, window, cx);
                        } else {
                            self.push_refresh_log(&format!(
                                "error subscription refresh \"{label}\": refresh failed ({error})"
                            ));
                            self.route_refresh_completion(
                                sub_id,
                                Notification::error(format!("{label}: refresh failed ({error})")),
                                0,
                                0,
                                0,
                                1,
                                0,
                                0,
                                &[],
                                &[],
                                &[],
                                &[],
                                window,
                                cx,
                            );
                        }
                    }
                }
            }
            Err(error) => match self.store.record_subscription_error(sub_id, &error) {
                Ok(_) => {
                    self.reload(window, cx);
                    self.push_refresh_log(&format!(
                        "error subscription refresh \"{label}\": {error}"
                    ));
                    self.route_refresh_completion(
                        sub_id,
                        Notification::error(format!("{label}: {error}")),
                        0,
                        0,
                        0,
                        1,
                        0,
                        0,
                        &[],
                        &[],
                        &[],
                        &[],
                        window,
                        cx,
                    );
                }
                Err(StoreError::NotFound(_)) => {
                    self.reload(window, cx);
                    self.close_batch_slot(sub_id, window, cx);
                }
                Err(db_error) => {
                    self.push_refresh_log(&format!(
                        "error cannot refresh subscription \"{label}\": {db_error}"
                    ));
                    self.route_refresh_completion(
                        sub_id,
                        Notification::error(format!("Cannot refresh subscription: {db_error}")),
                        0,
                        0,
                        0,
                        1,
                        0,
                        0,
                        &[],
                        &[],
                        &[],
                        &[],
                        window,
                        cx,
                    );
                }
            },
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    fn finish_subscription_refresh_no_window(
        &mut self,
        sub_id: &str,
        fetched: Result<FetchedSubscription, String>,
        now: i64,
        cx: &mut Context<Self>,
    ) {
        self.inflight_refreshes.remove(sub_id);
        let named = self.inflight_named_refreshes.remove(sub_id);

        let running: HashSet<String> = self.engine.running_ids().into_iter().collect();
        match fetched {
            Ok(fetched) => {
                match self
                    .store
                    .refresh_with_body_running(sub_id, &fetched.body, now, &running)
                {
                    Ok(outcome) => {
                        if outcome.error.is_none() {
                            let _ = self.store.set_subscription_quota(sub_id, fetched.userinfo);
                        }
                        let label = self
                            .store
                            .get_subscription(sub_id)
                            .ok()
                            .flatten()
                            .map(|sub| sub.label)
                            .unwrap_or_else(|| sub_id.to_string());
                        let failed = outcome.failures.len();
                        if let Some(error) = outcome.error {
                            self.push_refresh_log(&format!(
                                "error subscription refresh \"{label}\": refresh failed ({error})"
                            ));
                            self.accumulate_batch_member(
                                sub_id,
                                0,
                                0,
                                0,
                                1,
                                0,
                                0,
                                &[],
                                &[],
                                &[],
                                &[],
                            );
                        } else {
                            let (added_names, updated_names, removed_names, kept_in_use_names) =
                                if named {
                                    (
                                        outcome.added_names.as_slice(),
                                        outcome.updated_names.as_slice(),
                                        outcome.removed_names.as_slice(),
                                        outcome.kept_in_use_names.as_slice(),
                                    )
                                } else {
                                    let empty: &[String] = &[];
                                    (empty, empty, empty, empty)
                                };
                            let (warn, summary) = refresh_summary_text(
                                &label,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                outcome.skipped,
                                outcome.duplicates,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                            );
                            if warn {
                                self.push_refresh_log(&format!(
                                    "warning subscription refresh {summary}"
                                ));
                            } else {
                                self.push_refresh_log(&format!("subscription refresh {summary}"));
                            }
                            self.accumulate_batch_member(
                                sub_id,
                                outcome.added,
                                outcome.updated,
                                outcome.removed,
                                failed,
                                outcome.conflicts,
                                outcome.in_use,
                                added_names,
                                updated_names,
                                removed_names,
                                kept_in_use_names,
                            );
                        }
                    }
                    Err(_) => {
                        self.accumulate_batch_member(sub_id, 0, 0, 0, 1, 0, 0, &[], &[], &[], &[]);
                    }
                }
            }
            Err(error) => {
                let _ = self.store.record_subscription_error(sub_id, &error);
                self.accumulate_batch_member(sub_id, 0, 0, 0, 1, 0, 0, &[], &[], &[], &[]);
            }
        }
        if self.refresh_batch_members.is_empty() {
            self.take_batch_counts();
        }
        self.reload_quiet(cx);
    }

    fn toggle_sub_expanded(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.expanded_subs.remove(id) {
            self.expanded_subs.insert(id.to_string());
        }
        self.reopen_subscriptions_if_open(window, cx);
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

    fn commit_preset_create_with_boxes(
        &mut self,
        name: String,
        rules_text: String,
        boxes: SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if name.trim().is_empty() && rules_text.trim().is_empty() && boxes.is_empty() {
            return true;
        }
        let mut rules = if rules_text.trim().is_empty() {
            Vec::new()
        } else {
            match crate::preset::parse_preset_json(&rules_text, &name) {
                Ok(parsed) => parsed.rules,
                Err(error) => {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    return false;
                }
            }
        };
        match AppView::expand_simple_rule_boxes(&boxes, window, cx) {
            Ok(mut expanded) => rules.append(&mut expanded),
            Err(()) => return false,
        }
        let create_name = if name.trim().is_empty() {
            window.push_notification(
                Notification::error("Invalid routing preset: preset needs a name"),
                cx,
            );
            return false;
        } else {
            name.trim().to_string()
        };
        match self
            .store
            .create_preset(&create_name, "manual", None, &rules)
        {
            Ok(preset) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(preset_imported_message(&preset.name, rules.len())),
                    cx,
                );
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
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

    fn commit_preset_create_structured(
        &mut self,
        name: String,
        action: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if name.trim().is_empty() {
            window.push_notification(
                Notification::error("Invalid routing preset: preset needs a name"),
                cx,
            );
            return false;
        }
        let rules = match crate::preset::structured_preset_rules(&action) {
            Ok(rules) => rules,
            Err(error) => {
                window.push_notification(Notification::error(preset_error_message(&error)), cx);
                return false;
            }
        };
        match self
            .store
            .create_preset(name.trim(), "manual", None, &rules)
        {
            Ok(preset) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(preset_imported_message(&preset.name, rules.len())),
                    cx,
                );
                self.reopen_presets_if_open(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    fn commit_preset_create_remote(
        &mut self,
        name: String,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if name.trim().is_empty() {
            window.push_notification(
                Notification::error("Invalid routing preset: preset needs a name"),
                cx,
            );
            return false;
        }
        match crate::store::subscription_fetch_policy(url.trim()) {
            crate::store::SubscriptionFetchPolicy::Block { reason } => {
                window.push_notification(
                    Notification::error(format!("Invalid preset URL: {reason}")),
                    cx,
                );
                return false;
            }
            crate::store::SubscriptionFetchPolicy::Warn { reason } => {
                window.push_notification(Notification::warning(reason.to_string()), cx);
            }
            crate::store::SubscriptionFetchPolicy::Allow => {}
        }
        match self
            .store
            .import_remote_preset_shell(name.trim(), url.trim())
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
                self.reopen_presets_if_open(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    /// Compat entry for contract tests that only exercise the rules JSON
    /// layer (empty boxes expand to nothing).
    #[cfg(test)]
    fn commit_preset_edit(
        &mut self,
        id: &str,
        name: String,
        rules_text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.commit_preset_edit_with_boxes(
            id,
            name,
            rules_text,
            SimpleRuleBoxes::default(),
            window,
            cx,
        )
    }

    fn commit_preset_edit_with_boxes(
        &mut self,
        id: &str,
        name: String,
        rules_text: String,
        boxes: SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut rules = if rules_text.trim().is_empty() {
            Vec::new()
        } else {
            match crate::preset::parse_preset_json(&rules_text, &name) {
                Ok(parsed) => parsed.rules,
                Err(error) => {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    return false;
                }
            }
        };
        match AppView::expand_simple_rule_boxes(&boxes, window, cx) {
            Ok(mut expanded) => rules.append(&mut expanded),
            Err(()) => return false,
        }
        let renamed = match self.store.rename_preset(id, &name) {
            Ok(preset) => preset,
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return true;
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                return false;
            }
        };
        match self.store.set_preset_rules(id, &rules) {
            Ok(_) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Preset \"{}\" updated ({} rule(s))",
                        renamed.name,
                        rules.len()
                    )),
                    cx,
                );
                self.reopen_presets_if_open(window, cx);
                true
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
                false
            }
        }
    }

    fn commit_preset_export_link(
        &mut self,
        preset: &store::RoutePreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fresh = match self.store.get_preset(&preset.id) {
            Ok(Some(fresh)) => fresh,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot export routing preset: {error}")),
                    cx,
                );
                return;
            }
        };
        match fresh.rules() {
            Ok(rules) => {
                let link = crate::preset::preset_share_link(&fresh.name, &rules);
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                window.push_notification(
                    Notification::success(format!("Copied preset link for \"{}\"", fresh.name)),
                    cx,
                );
            }
            Err(message) => {
                window.push_notification(Notification::error(message), cx);
            }
        }
    }

    fn commit_preset_duplicate(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let preset = match self.store.get_preset(id) {
            Ok(Some(preset)) => preset,
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot duplicate routing preset: {error}")),
                    cx,
                );
                return;
            }
        };
        let taken: Vec<String> = self
            .store
            .list_presets()
            .map(|presets| presets.into_iter().map(|preset| preset.name).collect())
            .unwrap_or_default();
        let name = duplicate_name(&preset.name, &taken);
        let rules = preset.rules().unwrap_or_default();
        match self
            .store
            .create_preset(&name, &preset.source, preset.source_url.as_deref(), &rules)
        {
            Ok(created) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(preset_imported_message(&created.name, rules.len())),
                    cx,
                );
                self.reopen_presets_if_open(window, cx);
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
    }

    /// T-10 prevent-modifications lock. The toggle is the owner's positive
    /// action (no confirm dialog); it protects the rules, never the attach
    /// relationship.
    fn commit_preset_lock(
        &mut self,
        id: &str,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.set_preset_locked(id, locked) {
            Ok(_) => {
                self.reload(window, cx);
                if locked {
                    window.push_notification(
                        Notification::success("Routing preset locked — import a copy to modify"),
                        cx,
                    );
                } else {
                    window.push_notification(Notification::success("Routing preset unlocked"), cx);
                }
            }
            Err(StoreError::NotFound(_)) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
        self.reopen_presets_if_open(window, cx);
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

    fn commit_preset_remove(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.delete_preset(id) {
            Ok(true) => {
                self.reload(window, cx);
                window.push_notification(Notification::success("Routing preset removed"), cx);
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(StoreError::Invalid(_)) => {
                let (name, attached) = match self.store.get_preset(id) {
                    Ok(Some(preset)) => {
                        let rows = self
                            .store
                            .preset_attachments(id)
                            .unwrap_or_default()
                            .iter()
                            .map(|row| format!("\"{}\"", row.name))
                            .collect::<Vec<_>>();
                        (preset.name, rows)
                    }
                    _ => ("preset".to_string(), Vec::new()),
                };
                let preset_id = id.to_string();
                self.open_preset_detach_delete_confirm(&preset_id, &name, &attached, window, cx);
                return;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot remove routing preset: {error}")),
                    cx,
                );
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    fn commit_preset_detach_delete(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.delete_preset_detaching(id) {
            Ok(true) => {
                self.reload(window, cx);
                window.push_notification(
                    Notification::success("Routing preset removed (rows detached)"),
                    cx,
                );
            }
            Ok(false) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot remove routing preset: {error}")),
                    cx,
                );
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    fn commit_preset_attach(
        &mut self,
        config_id: &str,
        preset_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .store
            .set_config_preset(config_id, preset_id.as_deref())
        {
            Ok(row) => {
                self.reload(window, cx);
                let message = match preset_id {
                    Some(_) => {
                        let name = self
                            .preset_name_for(row.preset_id.as_deref())
                            .unwrap_or_else(|| "preset".to_string());
                        format!("\"{}\" now routes with \"{name}\"", row.name)
                    }
                    None => format!("\"{}\" detached from its routing preset", row.name),
                };
                window.push_notification(Notification::success(message), cx);
                window.close_all_dialogs(cx);
                reclaim_shell_focus(window, cx);
            }
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
        }
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

    fn start_preset_refresh(&mut self, preset: store::RoutePreset, cx: &mut Context<Self>) -> bool {
        let Some(url) = preset.source_url.clone() else {
            return false;
        };
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        let preset_id = preset.id.clone();
        let preset_name = preset.name.clone();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_subscription_body(client, &url, resolve_subscription_host).await;
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_preset_refresh(
                                &preset_id,
                                &preset_name,
                                fetched,
                                window,
                                cx,
                            );
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_preset_refresh_no_window(&preset_id, fetched, cx);
                    });
                }
            });
        })
        .detach();
        true
    }

    fn finish_preset_refresh(
        &mut self,
        id: &str,
        name: &str,
        fetched: Result<FetchedSubscription, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let body = match fetched {
            Ok(fetched) => fetched.body,
            Err(reason) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh routing preset: {reason}")),
                    cx,
                );
                return;
            }
        };
        let rules = match crate::preset::parse_preset_json(&body, name) {
            Ok(parsed) => parsed.rules,
            Err(error) => {
                window.push_notification(Notification::error(preset_error_message(&error)), cx);
                return;
            }
        };
        match self.store.set_preset_rules(id, &rules) {
            Ok(preset) => {
                let count = preset.rules().map(|rules| rules.len()).unwrap_or(0);
                self.reload(window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Routing preset \"{}\" refreshed ({count} rule(s))",
                        preset.name
                    )),
                    cx,
                );
            }
            Err(StoreError::NotFound(_)) => {
                self.reload(window, cx);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot refresh routing preset: {error}")),
                    cx,
                );
            }
        }
        self.reopen_presets_if_open(window, cx);
    }

    fn finish_preset_refresh_no_window(
        &mut self,
        id: &str,
        fetched: Result<FetchedSubscription, String>,
        cx: &mut Context<Self>,
    ) {
        let Ok(fetched) = fetched else {
            return;
        };
        let body = fetched.body;
        let name = self
            .store
            .get_preset(id)
            .ok()
            .flatten()
            .map(|preset| preset.name)
            .unwrap_or_default();
        let Ok(parsed) = crate::preset::parse_preset_json(&body, &name) else {
            return;
        };
        if self.store.set_preset_rules(id, &parsed.rules).is_ok() {
            self.reload_quiet(cx);
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

    /// Reached only from the confirm dialog's Download click. Fetches through
    /// the shared subscription pipeline, then stages bytes + row.
    fn start_rule_set_download(
        &mut self,
        grant: RuleSetDownloadGrant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rule_set_download_in_flight {
            window.push_notification(
                Notification::warning("A rule set download is already in flight — wait for it"),
                cx,
            );
            return;
        }
        self.rule_set_download_in_flight = true;
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_rule_set_body(client, &grant.url, resolve_subscription_host).await;
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_rule_set_download(&grant, fetched, window, cx);
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_rule_set_download_no_window(&grant, fetched, cx);
                    });
                }
            });
        })
        .detach();
    }

    /// Post-download staging shared by both finish variants: write the staged
    /// bytes, then record the row (or refresh the identical row on re-download).
    fn stage_rule_set_bytes(
        &mut self,
        grant: &RuleSetDownloadGrant,
        bytes: Vec<u8>,
    ) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&grant.dir)
            .map_err(|error| format!("cannot create {}: {error}", grant.dir.display()))?;
        let path = grant.dir.join(&grant.file_name);
        std::fs::write(&path, &bytes)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        let existing = self
            .store
            .list_rule_sets()
            .ok()
            .and_then(|rows| rows.into_iter().find(|row| row.name == grant.name));
        let recorded = match existing {
            Some(row) => self
                .store
                .touch_rule_set(&row.id, &grant.url, bytes.len() as u64),
            None => self
                .store
                .create_rule_set(&grant.name, &grant.url, bytes.len() as u64),
        };
        if let Err(error) = recorded {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_string());
        }
        self.sync_presets_to_engine()?;
        Ok(path)
    }

    fn finish_rule_set_download(
        &mut self,
        grant: &RuleSetDownloadGrant,
        fetched: Result<Vec<u8>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.rule_set_download_in_flight = false;
        let bytes = match fetched {
            Ok(bytes) => bytes,
            Err(reason) => {
                window.push_notification(
                    Notification::error(format!(
                        "Cannot download rule set \"{}\" ({}): {reason}",
                        grant.name,
                        crate::import::redact_link(&grant.url),
                    )),
                    cx,
                );
                return false;
            }
        };
        let bytes_len = bytes.len();
        match self.stage_rule_set_bytes(grant, bytes) {
            Ok(path) => {
                window.push_notification(
                    Notification::success(format!(
                        "Rule set \"{}\" staged ({} bytes) — {}",
                        grant.name,
                        bytes_len,
                        crate::health::truncate(&path.display().to_string(), 160),
                    )),
                    cx,
                );
                self.reload(window, cx);
                self.reopen_presets_if_open(window, cx);
                true
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!(
                        "Cannot stage rule set \"{}\" ({}): {error}",
                        grant.name,
                        crate::import::redact_link(&grant.url),
                    )),
                    cx,
                );
                false
            }
        }
    }

    fn finish_rule_set_download_no_window(
        &mut self,
        grant: &RuleSetDownloadGrant,
        fetched: Result<Vec<u8>, String>,
        cx: &mut Context<Self>,
    ) {
        self.rule_set_download_in_flight = false;
        if let Ok(bytes) = fetched {
            if self.stage_rule_set_bytes(grant, bytes).is_ok() {
                self.reload_quiet(cx);
            }
        }
    }

    /// Remove: blocked while any preset references the set (loud inline), the
    /// file delete lands right after the row so no orphan cache bytes survive.
    fn commit_rule_set_remove(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let existing = self.store.get_rule_set(id).ok().flatten();
        let Some(row) = existing else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        match self.store.delete_rule_set(id) {
            Err(error) => {
                self.report_save_error(error, window, cx);
            }
            Ok(None) => {
                window.push_notification(Notification::warning("Already removed"), cx);
            }
            Ok(Some(file_name)) => {
                let path = crate::store::staged_rule_set_path(&file_name);
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        window.push_notification(
                            Notification::success(format!(
                                "Rule set \"{}\" removed (staged file deleted)",
                                row.name
                            )),
                            cx,
                        );
                    }
                    Err(error) => {
                        window.push_notification(
                            Notification::error(format!(
                                "Rule set \"{}\" removed, but the staged file could not be deleted: {} ({error}) — delete it manually",
                                row.name,
                                path.display()
                            )),
                            cx,
                        );
                    }
                }
                let _ = self.sync_presets_to_engine();
                self.reload(window, cx);
                self.reopen_presets_if_open(window, cx);
            }
        }
    }

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

    fn start_explicit_refresh(
        &mut self,
        subs: Vec<ProxySubscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        if subs.len() == 1 {
            let sub = subs.into_iter().next().expect("one sub");
            if self.refresh_subscription_by_id(&sub.id, window, cx) {
                return 1;
            }
            return 0;
        }
        let ids: Vec<String> = subs.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in subs {
            if self.start_subscription_refresh(sub, true, cx) {
                started += 1;
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
        started
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

    fn commit_subs_global_auto(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.store.set_subs_auto_refresh_enabled(on) {
            window.push_notification(
                Notification::error(format!("Cannot change auto-refresh: {error}")),
                cx,
            );
        }
        self.reopen_subscriptions_if_open(window, cx);
    }

    #[cfg(test)]
    fn stop_background_ticks(&mut self) {
        self.ticks_enabled = false;
    }

    fn tick_engine_poll(&mut self, cx: &mut Context<Self>) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }

        // T-08: the tick is the seam that lets a fixed folder clear the banner
        // without a restart.
        self.refresh_write_blocked(cx);
        let cause = self.poll_engine();
        self.tick_shard_samples();
        self.reload_quiet_no_poll(cx);
        if let Some(notification) = self.service_engine_crash() {
            self.push_engine_notification(notification, cx);
        }
        cause
    }

    #[cfg(test)]
    fn tick_engine_poll_notify(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.ticks_enabled {
            return None;
        }
        self.refresh_write_blocked(cx);
        let detail = self.poll_engine();
        self.tick_shard_samples();
        self.reload_quiet_no_poll(cx);
        if let Some(notification) = self.service_engine_crash() {
            window.push_notification(notification, cx);
        }
        detail
    }

    /// T-30: one sampling pass, riding the existing engine tick (no new timer).
    /// The pid cohort comes from the controller, so only this app's own
    /// sidecars can ever be opened.
    fn tick_shard_samples(&mut self) {
        let live: Vec<(String, u32)> = self
            .engine
            .shard_processes()
            .into_iter()
            .filter_map(|(label, pid)| pid.map(|pid| (label, pid)))
            .collect();
        self.shard_denied = fold_shard_samples(
            &mut self.shard_rings,
            &mut self.shard_baselines,
            &live,
            self.process_sampler.as_ref(),
            Instant::now(),
        );
    }

    fn tick_due_subscriptions(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let due = self.due_subscriptions();
        let ids: Vec<String> = due.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in due {
            if self.start_subscription_refresh(sub, false, cx) {
                started += 1;
            }
        }
        started
    }

    fn tick_reprobe_running(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        let running = self.engine.running_ids();
        let crashed: HashSet<String> = self.engine.ids_on_crashed_shards().into_iter().collect();
        let Ok(rows) = self.store.list() else {
            return 0;
        };
        let rows: Vec<ProxyConfig> = rows
            .into_iter()
            .filter(|row| {
                running.contains(&row.id)
                    && self.engine.is_running(&row.id)
                    && !crashed.contains(&row.id)
            })
            .collect();
        if rows.is_empty() {
            return 0;
        }
        self.spawn_probe(rows, false, false, None, cx)
    }

    fn ensure_traffic_mirror(&mut self, cx: &mut Context<Self>) {
        if self.traffic_mirror_hydrated {
            return;
        }
        self.traffic_mirror_hydrated = true;
        let Ok(totals) = self.store.traffic_totals() else {
            return;
        };
        let known: HashSet<String> = self
            .libraries
            .read(cx)
            .delegate()
            .rows
            .iter()
            .map(|row| row.id.clone())
            .collect();
        for (id, (rx, tx)) in &totals {
            if (*rx, *tx) != (0, 0) && known.contains(id) {
                self.health_store.set_data_traffic(id, *rx, *tx);
            }
        }
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

    #[cfg(test)]
    fn tick_traffic_poll(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled {
            return 0;
        }
        self.ensure_traffic_mirror(cx);
        let targets = self.engine.live_clash_targets();
        if targets.is_empty() {
            return 0;
        }
        let results = crate::health::poll_clash_targets(
            &targets,
            crate::health::CLASH_PER_CORE_TIMEOUT,
            crate::health::CLASH_TOTAL_DEADLINE,
        );
        let mut exclusions = self.traffic_probe_exclusions();
        exclusions.resolve_ips();
        self.apply_traffic_results(results, &exclusions, cx)
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

    fn apply_traffic_results(
        &mut self,
        results: Vec<(
            crate::engine::ClashTarget,
            Result<Vec<crate::health::ClashConn>, String>,
        )>,
        exclusions: &ProbeExclusions,
        cx: &mut Context<Self>,
    ) -> usize {
        let now = now_unix();
        let mut deltas: HashMap<String, (u64, u64)> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut ok_cores: HashSet<String> = HashSet::new();

        let mut fresh_rows: HashSet<String> = HashSet::new();

        let mut snapshot: Vec<(String, crate::health::ClashConn)> = Vec::new();
        let mut conn_deltas: HashMap<String, (u64, u64)> = HashMap::new();
        for (target, result) in &results {
            let Ok(entries) = result else {
                continue;
            };
            ok_cores.insert(target.core.clone());
            for entry in entries {
                if exclusions.excludes(&entry.dest) {
                    continue;
                }
                let key = format!("{}/{}", target.core, entry.id);
                let (rx, tx) = (entry.rx_bytes, entry.tx_bytes);
                let (last_rx, last_tx) = self.traffic_last_seen.get(&key).unwrap_or((0, 0));
                let (delta_rx, delta_tx) = if rx >= last_rx && tx >= last_tx {
                    (rx - last_rx, tx - last_tx)
                } else {
                    (rx, tx)
                };
                self.traffic_last_seen.insert(key.clone(), rx, tx, now);
                seen.insert(key.clone());
                snapshot.push((target.core.clone(), entry.clone()));
                if delta_rx > 0 || delta_tx > 0 {
                    conn_deltas.insert(key, (delta_rx, delta_tx));
                }
                let Some(row) = entry.row_id.as_deref() else {
                    continue;
                };
                fresh_rows.insert(row.to_string());
                if delta_rx > 0 || delta_tx > 0 {
                    let slot = deltas.entry(row.to_string()).or_insert((0, 0));
                    slot.0 = slot.0.saturating_add(delta_rx);
                    slot.1 = slot.1.saturating_add(delta_tx);
                }
            }
        }

        let live_cores: HashSet<String> = results
            .iter()
            .map(|(target, _)| target.core.clone())
            .collect();
        self.traffic_last_seen
            .sweep(now, &ok_cores, &seen, &live_cores);

        self.conn_snapshot = snapshot;
        self.conn_counts =
            crate::health::ConnCounts::from_conns(self.conn_snapshot.iter().map(|(_, conn)| conn));
        self.conn_delta = conn_deltas;
        self.last_traffic_poll_at = now;
        let mut updated = 0;

        if !deltas.is_empty() {
            let batch: Vec<(String, u64, u64)> = deltas
                .iter()
                .map(|(row, (rx, tx))| (row.clone(), *rx, *tx))
                .collect();
            match self.store.add_traffic_batch(&batch) {
                Ok(count) => {
                    updated = count;

                    for (row, (rx, tx)) in &deltas {
                        let (prev_rx, prev_tx) =
                            self.health_store.data_traffic(row).unwrap_or((0, 0));
                        self.health_store.set_data_traffic(
                            row,
                            prev_rx.saturating_add(*rx),
                            prev_tx.saturating_add(*tx),
                        );
                    }
                }
                Err(error) => eprintln!("traffic tick failed: {error}"),
            }
        }

        self.health_store
            .mark_stale_all(&self.engine.ids_on_crashed_shards());
        for row in &fresh_rows {
            self.health_store.clear_stale(row);
        }
        if updated > 0 {
            self.reload_quiet(cx);
        }
        updated
    }

    fn start_background_ticks(&mut self, cx: &mut Context<Self>) {
        if self.ticks_started {
            return;
        }
        self.ticks_started = true;
        spawn_tick(cx, ENGINE_POLL_TICK_SECS, |view, cx| {
            view.tick_engine_poll(cx);
        });
        spawn_tick(cx, DUE_SUBSCRIPTION_TICK_SECS, |view, cx| {
            view.tick_due_subscriptions(cx);
        });
        spawn_tick(cx, REPROBE_TICK_SECS, |view, cx| {
            view.tick_reprobe_running(cx);
        });
        spawn_tick(cx, TRAFFIC_FAST_TICK_SECS, |view, cx| {
            view.spawn_traffic_poll(cx);
        });
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

    /// Writes the cell value to the clipboard (bind:port-safe display strings
    /// only). Returns `false` without touching the clipboard for an empty value.
    fn copy_connection_dest(
        &mut self,
        dest: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if dest.is_empty() {
            window.push_notification(Notification::warning("No destination to copy"), cx);
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(dest.to_string()));
        true
    }

    /// Click-to-copy for a Connections cell: clipboard write plus a transient
    /// inline `Copied!` marker anchored at the clicked cell. Repeat clicks reuse
    /// the generation guard so the newest marker survives its full window.
    fn copy_conn_cell(
        &mut self,
        target: ConnCopyTarget,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.copy_connection_dest(value, window, cx) {
            return;
        }
        self.conn_copied = Some(target);
        self.conn_copied_generation = self.conn_copied_generation.wrapping_add(1);
        let generation = self.conn_copied_generation;
        cx.notify();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(Self::CONN_COPIED_TOOLTIP_MS))
                .await;
            cx.update(|cx| {
                weak.update(cx, |view, cx| {
                    if view.conn_copied_generation == generation {
                        view.conn_copied = None;
                        cx.notify();
                    }
                })
                .ok();
            });
        })
        .detach();
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
