#![recursion_limit = "512"]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod appearance;
mod consts;
mod dpapi;
mod engine;
mod health;
mod import;
mod keys;
mod library;
mod preset;
mod store;
mod text;
mod tray;

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
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

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// T-03: the inspector docks to the RIGHT of the library. Below this window
/// width it collapses to a slide-over drawer, so the table never loses the
/// width it needs.
pub(crate) const INSPECTOR_DRAWER_BREAKPOINT: f32 = 1080.;
/// The inspector's default width. The user may drag it inside the range
/// below; the width is session-only — nothing about it is persisted.
pub(crate) const INSPECTOR_WIDTH: f32 = 260.;
pub(crate) const INSPECTOR_WIDTH_MIN: f32 = 240.;
pub(crate) const INSPECTOR_WIDTH_MAX: f32 = 360.;

/// Spawn a console executable with no visible console window. `creation_flags`
/// REPLACES the whole flag word (std does `self.flags = flags`), so a future
/// site that needs other flags must OR them in:
/// `cmd.creation_flags(cmd.get_creation_flags() | CREATE_NO_WINDOW)`.
pub(crate) fn spawn_hidden(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

use appearance::*;
use consts::*;
use keys::*;
use library::*;
use text::*;

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

fn app_window_options(geometry: Option<WindowGeometry>) -> WindowOptions {
    let mut options = TitleBar::window_options();
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(APP_TITLE.into());
    }

    // T-32: a sane stored rectangle seeds the window; absent, corrupt, or
    // non-finite meta keeps today's fixed 1000×700 at (0, 0) — first run.
    let bounds = geometry.and_then(WindowGeometry::clamped).map_or_else(
        || Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(700.))),
        |geometry| {
            Bounds::new(
                point(px(geometry.x), px(geometry.y)),
                size(px(geometry.width), px(geometry.height)),
            )
        },
    );
    options.window_bounds = Some(WindowBounds::Windowed(bounds));
    options.window_min_size = Some(size(
        px(WindowGeometry::MIN_WIDTH),
        px(WindowGeometry::MIN_HEIGHT),
    ));
    options
}

/// T-32: the persisted rectangle, read before the window exists — the only
/// point `app_window_options` can seed `WindowBounds` from. A missing file, an
/// unreadable store, or an absent/corrupt key all read as `None`, which is
/// exactly today's startup rectangle.
fn stored_window_geometry() -> Option<WindowGeometry> {
    Store::peek_meta_readonly(&Store::default_path(), Store::META_GEOMETRY)
        .as_deref()
        .and_then(WindowGeometry::from_json)
}

/// T-33: one row of the per-row bind picker. The id carries the row so two
/// rows never register the same option path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindHostChoice {
    id: String,
    value: String,
}

impl SearchableListItem for BindHostChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.value.clone().into()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BindHostChoiceList(Vec<BindHostChoice>);

impl BindHostChoiceList {
    fn for_row(row_id: &str, hosts: &[String]) -> Self {
        Self(
            hosts
                .iter()
                .map(|host| BindHostChoice {
                    id: format!("row-bind-option-{row_id}-{host}"),
                    value: host.clone(),
                })
                .collect(),
        )
    }
}

impl SearchableListDelegate for BindHostChoiceList {
    type Item = BindHostChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&BindHostChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        BindHostChoice: SearchableListItem<Value = V>,
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
        item: &BindHostChoice,
        _checked: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<AnyElement> {
        Some(
            div()
                .id(item.id.clone())
                .test_support()
                .child(item.value.clone())
                .into_any_element(),
        )
    }
}

struct RowEndpointInputs {
    /// The draft holder for both control shapes: the free-text field on
    /// custom-config rows, and the value a pick writes on every other row.
    bind: Entity<InputState>,
    port: Entity<InputState>,
    /// T-33: the picker for rows whose listen comes from the generated config.
    /// Custom-config rows never draw it.
    picker: Entity<SelectState<BindHostChoiceList>>,
    /// The host list the picker was last built from.
    picker_hosts: Vec<String>,
    /// The value the picker was last told to show, so a stable row does not
    /// re-select on every paint.
    picker_selected: Option<String>,
    /// A confirmed pick waiting for the window-aware paint step to apply it.
    pick_pending: Option<String>,
    _subs: Vec<Subscription>,
    clean_bind: String,
    clean_port: String,
}

/// T-27: what a header-menu row does — pick a key, or reverse the direction.
#[derive(Debug, Clone, Copy)]
enum SortMenuAction {
    Key(LibrarySortKey),
    Direction,
}

/// T-31: the tone each level paints in, from the active theme. `Info` keeps the
/// default text color, `Debug` recedes to muted. `Warn`/`Error` are theme
/// *background* tokens (amber, danger red): legible behind white text on a dark
/// surface, but well under WCAG AA as text on the light theme's white — so in
/// light mode both are backed off toward black along their own hue (the T-30
/// `danger.blend(...)` precedent). Dark mode keeps the raw tokens.
fn log_level_text_color(level: LogLevel, theme: &Theme) -> Hsla {
    let text_safe = |color: Hsla, darken: f32| match theme.mode {
        ThemeMode::Light => color.blend(hsla(0., 0., 0., darken)),
        ThemeMode::Dark => color,
    };
    match level {
        LogLevel::Error => text_safe(theme.danger, 0.35),
        LogLevel::Warn => text_safe(theme.warning, 0.45),
        LogLevel::Info => theme.foreground,
        LogLevel::Debug => theme.muted_foreground,
    }
}

/// T-31: a visible line already carries its parsed level as `[level]`.
fn log_line_level(line: &str) -> LogLevel {
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(label, _)| LogLevel::from_label(label))
        .unwrap_or(LogLevel::Info)
}

/// T-31: the Settings "Maximum kept log lines" text to a clamped cap. `None`
/// means the text is not a whole number.
fn parse_log_cap(raw: &str) -> Option<usize> {
    let digits = raw.trim().replace(',', "");
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok().map(clamp_log_max_lines)
}

fn shortcut_kbd(keystroke: &str, fallback: &str) -> AnyElement {
    match Keystroke::parse(keystroke) {
        Ok(stroke) => Kbd::new(stroke).into_any_element(),
        Err(_) => fallback.to_string().into_any_element(),
    }
}

fn dialog_field(id: &'static str, label: &str, state: Entity<InputState>) -> Field {
    field()
        .label(label.to_string())
        .child(div().id(id).test_support().child(Input::new(&state)))
}

type DialogCommit = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

type DialogRepaint = Rc<dyn Fn(&mut Window, &mut App)>;

fn dialog_footer(
    ok_id: &'static str,
    cancel_id: &'static str,
    ok_label: &'static str,
    commit: DialogCommit,
) -> DialogFooter {
    debug_assert!(
        !ok_label.eq_ignore_ascii_case("ok") && !ok_label.eq_ignore_ascii_case("okay"),
        "Ticket 05: honest footers never use a bare OK label"
    );
    if ok_label.eq_ignore_ascii_case("close") {
        return DialogFooter::new().child(Button::new(ok_id).small().label(ok_label).on_click(
            move |_, window, cx| {
                if commit(window, cx) {
                    window.close_all_dialogs(cx);
                }
                reclaim_shell_focus(window, cx);
            },
        ));
    }
    let commit_click = commit.clone();
    DialogFooter::new()
        .child(
            Button::new(cancel_id)
                .small()
                .label("Cancel")
                .on_click(|_, window, cx| {
                    window.close_all_dialogs(cx);
                    reclaim_shell_focus(window, cx);
                }),
        )
        .child(
            Button::new(ok_id)
                .small()
                .label(ok_label)
                .on_click(move |_, window, cx| {
                    if commit_click(window, cx) {
                        window.close_all_dialogs(cx);
                    }
                    reclaim_shell_focus(window, cx);
                }),
        )
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

/// T-06: every app dialog opens through here, so Esc, a backdrop press and the
/// close button all end in `reclaim_shell_focus`. The kit fires `on_close`
/// after `Root::close_dialog` has already popped the layer, which is what makes
/// the end-of-path predicate correct.
pub(crate) fn open_app_dialog(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
) {
    focus_shell_before_overlay(window, cx);
    window.open_dialog(cx, move |dialog, window, cx| {
        build(dialog, window, cx).on_close(|_, window, cx| reclaim_shell_focus(window, cx))
    });
}

pub(crate) fn open_app_alert_dialog(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static,
) {
    focus_shell_before_overlay(window, cx);
    window.open_alert_dialog(cx, move |alert, window, cx| {
        build(alert, window, cx).on_close(|_, window, cx| reclaim_shell_focus(window, cx))
    });
}

pub(crate) fn open_app_sheet(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(Sheet, &mut Window, &mut App) -> Sheet + 'static,
) {
    focus_shell_before_overlay(window, cx);
    window.open_sheet(cx, move |sheet, window, cx| {
        build(sheet, window, cx).on_close(|_, window, cx| reclaim_shell_focus(window, cx))
    });
}

fn sheet_footer(
    ok_id: &'static str,
    cancel_id: &'static str,
    ok_label: &'static str,
    commit: DialogCommit,
) -> DialogFooter {
    debug_assert!(
        !ok_label.eq_ignore_ascii_case("ok") && !ok_label.eq_ignore_ascii_case("okay"),
        "Ticket 05: honest footers never use a bare OK label"
    );
    let commit_click = commit.clone();
    DialogFooter::new()
        .child(
            Button::new(cancel_id)
                .small()
                .label("Cancel")
                .on_click(|_, window, cx| {
                    window.close_sheet(cx);
                    reclaim_shell_focus(window, cx);
                }),
        )
        .child(
            Button::new(ok_id)
                .small()
                .label(ok_label)
                .on_click(move |_, window, cx| {
                    if commit_click(window, cx) {
                        window.close_sheet(cx);
                    }
                    reclaim_shell_focus(window, cx);
                }),
        )
}

fn footer_commit_label(dialog: &str) -> &'static str {
    match dialog {
        "settings" => "Done",
        "groups" | "preset-attach" => "Close",
        "logs" => "Apply filter",
        "presets" | "subscriptions" | "add" => "Add",
        "edit" | "preset-edit" | "subscription-edit" => "Save",
        "import" => "Import",
        "export" => "Export",
        "bulk-move" => "Move",
        "group-rename" => "Rename",
        "rule-set-add-dialog" => "Add",
        "share" => "Copy link",
        _ => "Close",
    }
}

const SHARE_WARNING: &str = "This link carries the endpoint's credentials — anyone holding it can use it. Prefer Copy URLs for credential-free sharing.";

// ---- T-28: share-dialog QR of the two listener URLs ---------------------

/// Modules of quiet zone baked into every grid — the QR spec's minimum.
const SHARE_QR_QUIET_ZONE: usize = 4;
/// Side of one QR plate in the Share dialog. The plate never shrinks; the module
/// grid scales inside it, so the quiet zone cannot be clipped by a resize.
const SHARE_QR_PLATE_PX: f32 = 132.;

/// A monochrome QR module grid with its quiet zone baked in, row-major and square.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QrGrid {
    side: usize,
    dark: Vec<bool>,
}

impl QrGrid {
    fn is_dark(&self, col: usize, row: usize) -> bool {
        col < self.side && row < self.side && self.dark[row * self.side + col]
    }

    /// FNV-1a over the row-major module bits: the regression fingerprint for a fixed
    /// payload, since decoding a rendered QR is out of scope by decision.
    #[cfg(test)]
    fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for &dark in &self.dark {
            hash ^= u64::from(dark);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

/// Encode one listener URL as a byte-mode QR at least error-correction level M (the ticket
/// allows L or M; M already fits these URLs in a 33-module grid, quiet zone included,
/// and tolerates a smudge on a phone screen). The encoder picks the smallest version
/// that fits, well past the ticket's 255-byte ceiling, and we add the quiet zone.
fn qr_grid(payload: &str) -> Result<QrGrid, String> {
    let code = QrCode::encode_binary(payload.as_bytes(), QrCodeEcc::Medium)
        .map_err(|_| format!("QR payload too long ({} bytes)", payload.len()))?;
    let inner = code.size() as usize;
    let side = inner + SHARE_QR_QUIET_ZONE * 2;
    let mut dark = vec![false; side * side];
    for row in 0..inner {
        for col in 0..inner {
            if code.get_module(col as i32, row as i32) {
                dark[(row + SHARE_QR_QUIET_ZONE) * side + col + SHARE_QR_QUIET_ZONE] = true;
            }
        }
    }
    Ok(QrGrid { side, dark })
}

/// The row's two listener URLs, byte-identical to `proxy_urls`, with their labels.
fn share_qr_payloads(config: &ProxyConfig) -> [(&'static str, String); 2] {
    let (socks, http) = proxy_urls(&config.bind_ip, config.port);
    [("socks", socks), ("http", http)]
}

/// Where the module grid lands inside a plate box: cell edge plus the grid origin. The
/// whole grid — quiet zone included — is centered and never exceeds the box, so a
/// resize cannot clip the quiet zone; whole-pixel cells keep modules square and crisp.
#[derive(Debug, Clone, Copy, PartialEq)]
struct QrPlate {
    cell: f32,
    x: f32,
    y: f32,
    side: usize,
}

impl QrPlate {
    fn fit(width: f32, height: f32, side: usize) -> Self {
        let box_side = width.min(height).max(0.);
        let raw = box_side / side as f32;
        let cell = if raw >= 1. { raw.floor() } else { raw };
        let plate = cell * side as f32;
        Self {
            cell,
            x: ((width - plate) / 2.).max(0.),
            y: ((height - plate) / 2.).max(0.),
            side,
        }
    }

    fn plate_side(&self) -> f32 {
        self.cell * self.side as f32
    }

    fn module_origin(&self, col: usize, row: usize) -> (f32, f32) {
        (
            self.x + col as f32 * self.cell,
            self.y + row as f32 * self.cell,
        )
    }
}

/// Paint the white plate (quiet zone included) and the dark modules on top of it.
/// Scanners read black-on-white, so the plate keeps that pair in both themes while the
/// surrounding dialog and the labels stay theme-colored.
fn paint_share_qr(grid: &QrGrid, plate: QrPlate, window: &mut Window) {
    let plate_side = px(plate.plate_side());
    window.paint_quad(fill(
        Bounds::new(
            point(px(plate.x), px(plate.y)),
            size(plate_side, plate_side),
        ),
        rgb(0xFFFFFF),
    ));
    for row in 0..grid.side {
        for col in 0..grid.side {
            if !grid.is_dark(col, row) {
                continue;
            }
            let (x, y) = plate.module_origin(col, row);
            window.paint_quad(fill(
                Bounds::new(point(px(x), px(y)), size(px(plate.cell), px(plate.cell))),
                rgb(0x000000),
            ));
        }
    }
}

/// One labeled QR plate. The payload that cannot encode (far beyond any listener URL)
/// degrades to a caption rather than an empty square.
fn share_qr_block(label: &'static str, url: &str, id: &'static str, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let content: AnyElement = match qr_grid(url) {
        Ok(grid) => {
            let grid = Rc::new(grid);
            let side = grid.side;
            canvas(
                move |bounds, _, _| {
                    QrPlate::fit(
                        bounds.size.width.as_f32(),
                        bounds.size.height.as_f32(),
                        side,
                    )
                },
                move |_, plate, window, _| paint_share_qr(&grid, plate, window),
            )
            .size_full()
            .into_any_element()
        }
        Err(reason) => div()
            .text_xs()
            .text_color(muted)
            .child(reason)
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .child(
            div()
                .id(id)
                .test_support()
                .w(px(SHARE_QR_PLATE_PX))
                .h(px(SHARE_QR_PLATE_PX))
                .flex_shrink_0()
                .child(content),
        )
        .child(
            div()
                .id(format!("{id}-label"))
                .test_support()
                .text_xs()
                .text_color(muted)
                .child(label),
        )
}

/// The dialog's QR region: both schemes side by side, one scannable code each.
fn share_qr_region(payloads: &[(&'static str, String); 2], cx: &App) -> impl IntoElement {
    div()
        .id("share-qr-region")
        .test_support()
        .flex()
        .flex_wrap()
        .gap_3()
        .child(share_qr_block(
            payloads[0].0,
            &payloads[0].1,
            "share-qr-socks",
            cx,
        ))
        .child(share_qr_block(
            payloads[1].0,
            &payloads[1].1,
            "share-qr-http",
            cx,
        ))
}

fn advanced_settings_button(view: &Entity<AppView>, form: &ConfigForm) -> impl IntoElement {
    let adv_view = view.clone();
    let adv_form = form.clone();
    Button::new("advanced-settings")
        .small()
        .label("Advanced Settings")
        .tooltip("Dial fields and TLS extras — edits apply directly, no extra checks")
        .accessibility_label("Open Advanced Settings: dial fields and TLS extras")
        .on_click(move |_, window, cx| {
            adv_view.update(cx, |view, cx| {
                view.open_advanced_dialog(&adv_form, window, cx);
            });
        })
}

fn password_row(label: &str, id: &'static str, secret: &Entity<InputState>, cx: &App) -> Field {
    let presence = secret_state_caption(secret.read(cx).value().as_ref()).to_string();
    field().label(label.to_string()).child(
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(id)
                    .test_support()
                    .child(Input::new(secret).mask_toggle()),
            )
            .child(
                div()
                    .id(format!("{id}-state"))
                    .test_support()
                    .text_sm()
                    .child(presence),
            ),
    )
}

fn advanced_toggle_button(
    label: &'static str,
    view: &Entity<AppView>,
    form: &ConfigForm,
) -> impl IntoElement {
    let toggle_view = view.clone();
    let toggle_cell = form.advanced.clone();
    let toggle_form = form.clone();
    Button::new("params-advanced-toggle")
        .small()
        .label(label)
        .on_click(move |_, window, cx| {
            toggle_view.update(cx, |_, cx| {
                let expanding = !*toggle_cell.borrow();
                if expanding {
                    toggle_form.expand_merge(window, cx);
                } else {
                    toggle_form.collapse_echo(window, cx);
                }
                *toggle_cell.borrow_mut() = expanding;
                cx.notify();
            });
        })
}

fn panel_toggle_button(
    id: &'static str,
    label: &'static str,
    view: &Entity<AppView>,
    open: &Rc<RefCell<bool>>,
) -> impl IntoElement {
    let toggle_view = view.clone();
    let toggle_cell = open.clone();
    Button::new(id)
        .small()
        .label(label)
        .on_click(move |_, _, cx| {
            toggle_view.update(cx, |_, cx| {
                let expanded = !*toggle_cell.borrow();
                *toggle_cell.borrow_mut() = expanded;
                cx.notify();
            });
        })
}

#[derive(Clone)]
struct ConfigForm {
    name: Entity<InputState>,

    protocol_select: Entity<SelectState<ProtocolChoiceList>>,

    last_protocol: Rc<RefCell<String>>,

    protocol_locked: bool,

    locked_protocol: String,

    vless_uuid: Entity<InputState>,
    vless_flow: Entity<SelectState<PanelChoiceList>>,
    vless_packet_encoding: Entity<SelectState<PanelChoiceList>>,

    s1_network: Entity<SelectState<PanelChoiceList>>,
    s1_path: Entity<InputState>,
    s1_host: Entity<InputState>,
    s1_method: Entity<InputState>,
    s1_service_name: Entity<InputState>,
    s1_open: Rc<RefCell<bool>>,

    s2_enable: Rc<RefCell<bool>>,
    s2_sni: Entity<InputState>,
    s2_alpn: Entity<InputState>,
    s2_fp: Entity<SelectState<PanelChoiceList>>,
    s2_insecure: Rc<RefCell<bool>>,
    s2_pbk: Entity<InputState>,
    s2_sid: Entity<InputState>,
    s2_open: Rc<RefCell<bool>>,

    s3_mode: Rc<RefCell<String>>,
    s3_brutal: Rc<RefCell<bool>>,
    s3_up: Entity<InputState>,
    s3_down: Entity<InputState>,
    s3_open: Rc<RefCell<bool>>,

    s4_reuse: Rc<RefCell<bool>>,
    s4_fastopen: Rc<RefCell<bool>>,
    s4_fragment: Rc<RefCell<bool>>,
    s4_multipath: Rc<RefCell<bool>>,
    s4_timeout: Entity<InputState>,
    s4_bind: Entity<InputState>,
    s4_inet4: Entity<InputState>,
    s4_inet6: Entity<InputState>,

    vless_open: Rc<RefCell<bool>>,

    vmess_uuid: Entity<InputState>,
    vmess_aid: Entity<InputState>,
    vmess_security: Entity<SelectState<PanelChoiceList>>,
    vmess_packet_encoding: Entity<SelectState<PanelChoiceList>>,

    vmess_open: Rc<RefCell<bool>>,

    trojan_password: Entity<InputState>,

    trojan_open: Rc<RefCell<bool>>,

    ss_method: Entity<SelectState<PanelChoiceList>>,
    ss_password: Entity<InputState>,
    ss_plugin: Entity<SelectState<PanelChoiceList>>,
    ss_plugin_opts: Entity<InputState>,
    ss_uot: Rc<RefCell<bool>>,

    ss_open: Rc<RefCell<bool>>,

    wg_private: Entity<InputState>,
    wg_public: Entity<InputState>,
    wg_preshared: Entity<InputState>,
    wg_reserved: Entity<InputState>,
    wg_keepalive: Entity<InputState>,
    wg_local_addr: Entity<InputState>,
    wg_dns: Entity<InputState>,
    wg_mtu: Entity<InputState>,
    wg_workers: Entity<InputState>,
    wg_sys_ifc: Rc<RefCell<bool>>,

    wg_open: Rc<RefCell<bool>>,

    awg_private: Entity<InputState>,
    awg_public: Entity<InputState>,
    awg_preshared: Entity<InputState>,
    awg_reserved: Entity<InputState>,
    awg_keepalive: Entity<InputState>,
    awg_local_addr: Entity<InputState>,
    awg_dns: Entity<InputState>,
    awg_mtu: Entity<InputState>,
    awg_workers: Entity<InputState>,
    awg_sys_ifc: Rc<RefCell<bool>>,
    awg_version: Entity<SelectState<PanelChoiceList>>,
    awg_enable: Rc<RefCell<bool>>,
    awg_jc: Entity<InputState>,
    awg_jmin: Entity<InputState>,
    awg_jmax: Entity<InputState>,
    awg_s1: Entity<InputState>,
    awg_s2: Entity<InputState>,
    awg_s3: Entity<InputState>,
    awg_s4: Entity<InputState>,
    awg_h1: Entity<InputState>,
    awg_h2: Entity<InputState>,
    awg_h3: Entity<InputState>,
    awg_h4: Entity<InputState>,
    awg_i1: Entity<InputState>,
    awg_i2: Entity<InputState>,
    awg_i3: Entity<InputState>,
    awg_i4: Entity<InputState>,
    awg_i5: Entity<InputState>,
    awg_hpk: Entity<InputState>,
    awg_pad: Entity<InputState>,
    awg_rekey_after: Entity<InputState>,
    awg_rekey_timeout: Entity<InputState>,
    awg_reject_after: Entity<InputState>,
    awg_keepalive_timeout: Entity<InputState>,
    awg_max_handshake: Entity<InputState>,
    awg_trailers: Rc<RefCell<bool>>,
    awg_cookies: Rc<RefCell<bool>>,

    awg_open: Rc<RefCell<bool>>,

    hy_server_ports: Entity<InputState>,
    hy_hop_interval: Entity<InputState>,
    hy_up: Entity<InputState>,
    hy_down: Entity<InputState>,
    hy_obfs: Entity<InputState>,
    hy_auth_type: Entity<SelectState<PanelChoiceList>>,
    hy_auth: Entity<InputState>,
    hy_recv_conn: Entity<InputState>,
    hy_recv: Entity<InputState>,
    hy_mtu: Rc<RefCell<bool>>,
    hy_password: Entity<InputState>,

    hy_open: Rc<RefCell<bool>>,

    tuic_uuid: Entity<InputState>,
    tuic_password: Entity<InputState>,
    tuic_cc: Entity<SelectState<PanelChoiceList>>,
    tuic_relay: Entity<SelectState<PanelChoiceList>>,
    tuic_uot: Rc<RefCell<bool>>,
    tuic_zero_rtt: Rc<RefCell<bool>>,
    tuic_heartbeat: Entity<InputState>,

    tuic_open: Rc<RefCell<bool>>,

    socks_version: Entity<SelectState<PanelChoiceList>>,
    socks_username: Entity<InputState>,
    socks_password: Entity<InputState>,
    socks_network: Entity<SelectState<PanelChoiceList>>,
    socks_uot: Rc<RefCell<bool>>,

    socks_open: Rc<RefCell<bool>>,

    http_username: Entity<InputState>,
    http_password: Entity<InputState>,
    http_path: Entity<InputState>,
    http_headers: Entity<InputState>,

    http_open: Rc<RefCell<bool>>,

    anytls_password: Entity<InputState>,
    anytls_interval: Entity<InputState>,
    anytls_timeout: Entity<InputState>,
    anytls_min: Entity<InputState>,

    anytls_open: Rc<RefCell<bool>>,

    naive_username: Entity<InputState>,
    naive_password: Entity<InputState>,
    naive_uot: Rc<RefCell<bool>>,
    naive_quic: Rc<RefCell<bool>>,
    naive_cc: Entity<SelectState<PanelChoiceList>>,

    naive_open: Rc<RefCell<bool>>,

    ssh_user: Entity<InputState>,
    ssh_password: Entity<InputState>,
    ssh_private_key: Entity<TextareaState>,
    ssh_key_path: Entity<InputState>,
    ssh_key_pass: Entity<InputState>,
    ssh_host_key: Entity<InputState>,
    ssh_host_key_algs: Entity<InputState>,
    ssh_client_version: Entity<InputState>,

    ssh_open: Rc<RefCell<bool>>,

    snell_psk: Entity<InputState>,
    snell_version: Entity<SelectState<PanelChoiceList>>,
    snell_obfs_mode: Entity<InputState>,
    snell_obfs_host: Entity<InputState>,
    snell_userkey: Entity<InputState>,
    snell_reuse: Rc<RefCell<bool>>,
    snell_network: Entity<InputState>,
    snell_mode: Entity<InputState>,

    snell_open: Rc<RefCell<bool>>,

    shadowtls_version: Entity<SelectState<PanelChoiceList>>,
    shadowtls_password: Entity<InputState>,

    shadowtls_open: Rc<RefCell<bool>>,

    xray_uuid: Entity<InputState>,
    xray_enc: Entity<InputState>,
    xray_flow: Entity<SelectState<PanelChoiceList>>,
    xray_sni: Entity<InputState>,
    xray_fp: Entity<SelectState<PanelChoiceList>>,
    xray_alpn: Entity<InputState>,
    xray_pbk: Entity<InputState>,
    xray_sid: Entity<InputState>,
    xray_spiderx: Entity<InputState>,
    xray_network: Entity<SelectState<PanelChoiceList>>,
    xray_mode: Entity<InputState>,
    xray_mux: Entity<SelectState<PanelChoiceList>>,

    xray_open: Rc<RefCell<bool>>,

    chain_hops: Rc<RefCell<Vec<serde_json::Value>>>,

    chain_pick: Entity<SelectState<PanelChoiceList>>,

    chain_library: Rc<RefCell<Vec<ChainLibraryEntry>>>,

    chain_open: Rc<RefCell<bool>>,

    extra_exe: Entity<InputState>,
    extra_args: Entity<InputState>,
    extra_config: Entity<TextareaState>,

    extra_open: Rc<RefCell<bool>>,

    params: Entity<TextareaState>,
    group: Entity<InputState>,
    bind: Entity<InputState>,
    port: Entity<InputState>,

    test_url: Entity<InputState>,

    guided_server: Entity<InputState>,
    guided_port: Entity<InputState>,
    guided_identity: Entity<InputState>,

    advanced: Rc<RefCell<bool>>,
}

impl ConfigForm {
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

    fn secret_field(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: &str,
        default: Option<&str>,
    ) -> Entity<InputState> {
        cx.new(|cx| {
            let state = InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(true);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    fn panel_select(
        window: &mut Window,
        cx: &mut Context<AppView>,
        choices: Vec<PanelChoice>,
        current: &str,
    ) -> Entity<SelectState<PanelChoiceList>> {
        let selected = choices
            .iter()
            .position(|choice| choice.value == current)
            .map(IndexPath::new);
        cx.new(|cx| SelectState::new(PanelChoiceList(choices), selected, window, cx))
    }

    fn area(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: &str,
        default: Option<&str>,
    ) -> Entity<TextareaState> {
        cx.new(|cx| {
            let state = TextareaState::new(window, cx).placeholder(placeholder);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    fn protocol_text(&self, cx: &App) -> String {
        if self.protocol_locked {
            return self.locked_protocol.clone();
        }
        self.protocol_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| self.last_protocol.borrow().clone())
    }

    fn sync_protocol_template(&self, window: &mut Window, cx: &mut App) {
        if self.protocol_locked {
            return;
        }
        let current = self
            .protocol_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| self.last_protocol.borrow().clone());
        let mut last = self.last_protocol.borrow_mut();
        if current == *last {
            return;
        }
        let old = last.clone();
        *last = current.clone();
        drop(last);
        let box_text = self.params.read(cx).value().to_string();
        let next = pristine_template_rewrite(&box_text, &old, &current);
        if next != box_text {
            self.params.update(cx, |state, cx| {
                state.set_value(next, window, cx);
            });
        }
    }

    fn blank(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let template = protocol_params_template(DEFAULT_PROTOCOL);
        let selected = protocol_choices()
            .iter()
            .position(|choice| choice.value == DEFAULT_PROTOCOL)
            .map(IndexPath::new);
        Self {
            name: Self::field(window, cx, "Name", None),
            protocol_select: cx.new(|cx| {
                SelectState::new(ProtocolChoiceList(protocol_choices()), selected, window, cx)
            }),
            last_protocol: Rc::new(RefCell::new(DEFAULT_PROTOCOL.to_string())),
            protocol_locked: false,
            locked_protocol: DEFAULT_PROTOCOL.to_string(),
            params: Self::area(
                window,
                cx,
                "Protocol params JSON (multiline)",
                Some(&template),
            ),
            group: Self::field(window, cx, "Group", Some(DEFAULT_GROUP)),
            bind: Self::field(window, cx, "Bind address", Some(DEFAULT_BIND)),
            port: Self::field(window, cx, "Port (blank or auto = auto-assign)", None),
            test_url: Self::field(window, cx, "Test URL (blank = global)", None),
            guided_server: Self::field(window, cx, "Server (e.g. example.com)", None),
            guided_port: Self::field(window, cx, "Port (e.g. 443)", None),
            guided_identity: Self::secret_field(window, cx, "Identity secret", None),

            vless_uuid: Self::secret_field(window, cx, "UUID", None),
            vless_flow: Self::panel_select(window, cx, flow_choices(), ""),
            vless_packet_encoding: Self::panel_select(window, cx, packet_encoding_choices(), ""),
            s1_network: Self::panel_select(window, cx, network_choices(), "tcp"),
            s1_path: Self::field(window, cx, "Path", None),
            s1_host: Self::field(window, cx, "Host", None),
            s1_method: Self::field(window, cx, "Method", None),
            s1_service_name: Self::field(window, cx, "Service name", None),
            s1_open: Rc::new(RefCell::new(false)),
            s2_enable: Rc::new(RefCell::new(false)),
            s2_sni: Self::field(window, cx, "SNI", None),
            s2_alpn: Self::field(window, cx, "ALPN (comma list)", None),
            s2_fp: Self::panel_select(window, cx, fp_choices(), ""),
            s2_insecure: Rc::new(RefCell::new(false)),
            s2_pbk: Self::field(window, cx, "Reality public key", None),
            s2_sid: Self::field(window, cx, "Reality short id", None),
            s2_open: Rc::new(RefCell::new(false)),
            s3_mode: Rc::new(RefCell::new(String::new())),
            s3_brutal: Rc::new(RefCell::new(false)),
            s3_up: Self::field(window, cx, "Brutal up Mbps", None),
            s3_down: Self::field(window, cx, "Brutal down Mbps", None),
            s3_open: Rc::new(RefCell::new(false)),
            s4_reuse: Rc::new(RefCell::new(false)),
            s4_fastopen: Rc::new(RefCell::new(false)),
            s4_fragment: Rc::new(RefCell::new(false)),
            s4_multipath: Rc::new(RefCell::new(false)),
            s4_timeout: Self::field(window, cx, "Connect timeout (e.g. 5s)", None),
            s4_bind: Self::field(window, cx, "Bind interface", None),
            s4_inet4: Self::field(window, cx, "IPv4 bind address", None),
            s4_inet6: Self::field(window, cx, "IPv6 bind address", None),
            vless_open: Rc::new(RefCell::new(false)),

            vmess_uuid: Self::secret_field(window, cx, "UUID", None),
            vmess_aid: Self::field(window, cx, "Alter ID (aid)", None),
            vmess_security: Self::panel_select(window, cx, vmess_security_choices(), "auto"),
            vmess_packet_encoding: Self::panel_select(window, cx, packet_encoding_choices(), ""),
            vmess_open: Rc::new(RefCell::new(false)),
            trojan_password: Self::secret_field(window, cx, "Password", None),
            trojan_open: Rc::new(RefCell::new(false)),
            ss_method: Self::panel_select(window, cx, ss_method_choices(), ""),
            ss_password: Self::secret_field(window, cx, "Password", None),
            ss_plugin: Self::panel_select(window, cx, ss_plugin_choices(), ""),
            ss_plugin_opts: Self::field(window, cx, "Plugin options", None),
            ss_uot: Rc::new(RefCell::new(false)),
            ss_open: Rc::new(RefCell::new(false)),

            wg_private: Self::secret_field(window, cx, "Private key", None),
            wg_public: Self::field(window, cx, "Peer public key", None),
            wg_preshared: Self::secret_field(window, cx, "Preshared key", None),
            wg_reserved: Self::field(window, cx, "Reserved (comma ints)", None),
            wg_keepalive: Self::field(window, cx, "Keepalive (seconds)", None),
            wg_local_addr: Self::field(window, cx, "Local address (comma CIDRs)", None),
            wg_dns: Self::field(window, cx, "DNS", None),
            wg_mtu: Self::field(window, cx, "MTU", None),
            wg_workers: Self::field(window, cx, "Workers", None),
            wg_sys_ifc: Rc::new(RefCell::new(false)),
            wg_open: Rc::new(RefCell::new(false)),
            awg_private: Self::secret_field(window, cx, "Private key", None),
            awg_public: Self::field(window, cx, "Peer public key", None),
            awg_preshared: Self::secret_field(window, cx, "Preshared key", None),
            awg_reserved: Self::field(window, cx, "Reserved (comma ints)", None),
            awg_keepalive: Self::field(window, cx, "Keepalive (seconds)", None),
            awg_local_addr: Self::field(window, cx, "Local address (comma CIDRs)", None),
            awg_dns: Self::field(window, cx, "DNS", None),
            awg_mtu: Self::field(window, cx, "MTU", None),
            awg_workers: Self::field(window, cx, "Workers", None),
            awg_sys_ifc: Rc::new(RefCell::new(false)),
            awg_version: Self::panel_select(window, cx, awg_version_choices(), ""),
            awg_enable: Rc::new(RefCell::new(false)),
            awg_jc: Self::field(window, cx, "Jc", None),
            awg_jmin: Self::field(window, cx, "Jmin", None),
            awg_jmax: Self::field(window, cx, "Jmax", None),
            awg_s1: Self::field(window, cx, "S1", None),
            awg_s2: Self::field(window, cx, "S2", None),
            awg_s3: Self::field(window, cx, "S3", None),
            awg_s4: Self::field(window, cx, "S4", None),
            awg_h1: Self::field(window, cx, "H1", None),
            awg_h2: Self::field(window, cx, "H2", None),
            awg_h3: Self::field(window, cx, "H3", None),
            awg_h4: Self::field(window, cx, "H4", None),
            awg_i1: Self::field(window, cx, "I1", None),
            awg_i2: Self::field(window, cx, "I2", None),
            awg_i3: Self::field(window, cx, "I3", None),
            awg_i4: Self::field(window, cx, "I4", None),
            awg_i5: Self::field(window, cx, "I5", None),
            awg_hpk: Self::field(window, cx, "Header protection key", None),
            awg_pad: Self::field(window, cx, "Content padding addition", None),
            awg_rekey_after: Self::field(window, cx, "Rekey after time", None),
            awg_rekey_timeout: Self::field(window, cx, "Rekey timeout", None),
            awg_reject_after: Self::field(window, cx, "Reject after time", None),
            awg_keepalive_timeout: Self::field(window, cx, "Keepalive timeout", None),
            awg_max_handshake: Self::field(window, cx, "Max handshake attempts", None),
            awg_trailers: Rc::new(RefCell::new(false)),
            awg_cookies: Rc::new(RefCell::new(false)),
            awg_open: Rc::new(RefCell::new(false)),

            hy_server_ports: Self::field(window, cx, "Server ports (e.g. 9000-9010)", None),
            hy_hop_interval: Self::field(window, cx, "Hop interval (e.g. 30s)", None),
            hy_up: Self::field(window, cx, "Up Mbps", None),
            hy_down: Self::field(window, cx, "Down Mbps", None),
            hy_obfs: Self::field(window, cx, "Obfs", None),
            hy_auth_type: Self::panel_select(window, cx, hy_auth_type_choices(), ""),
            hy_auth: Self::secret_field(window, cx, "Auth secret", None),
            hy_recv_conn: Self::field(window, cx, "Recv window conn", None),
            hy_recv: Self::field(window, cx, "Recv window", None),
            hy_mtu: Rc::new(RefCell::new(false)),
            hy_password: Self::secret_field(window, cx, "Password", None),
            hy_open: Rc::new(RefCell::new(false)),
            tuic_uuid: Self::secret_field(window, cx, "UUID", None),
            tuic_password: Self::secret_field(window, cx, "Password", None),
            tuic_cc: Self::panel_select(window, cx, tuic_cc_choices(), ""),
            tuic_relay: Self::panel_select(window, cx, tuic_relay_choices(), ""),
            tuic_uot: Rc::new(RefCell::new(false)),
            tuic_zero_rtt: Rc::new(RefCell::new(false)),
            tuic_heartbeat: Self::field(window, cx, "Heartbeat (e.g. 10s)", None),
            tuic_open: Rc::new(RefCell::new(false)),

            socks_version: Self::panel_select(window, cx, socks_version_choices(), ""),
            socks_username: Self::field(window, cx, "Username", None),
            socks_password: Self::secret_field(window, cx, "Password", None),
            socks_network: Self::panel_select(window, cx, socks_network_choices(), "tcp"),
            socks_uot: Rc::new(RefCell::new(false)),
            socks_open: Rc::new(RefCell::new(false)),
            http_username: Self::field(window, cx, "Username", None),
            http_password: Self::secret_field(window, cx, "Password", None),
            http_path: Self::field(window, cx, "Path", None),
            http_headers: Self::field(window, cx, "Headers", None),
            http_open: Rc::new(RefCell::new(false)),
            anytls_password: Self::secret_field(window, cx, "Password", None),
            anytls_interval: Self::field(window, cx, "Idle check interval (e.g. 30s)", None),
            anytls_timeout: Self::field(window, cx, "Idle timeout (e.g. 30s)", None),
            anytls_min: Self::field(window, cx, "Min idle sessions", None),
            anytls_open: Rc::new(RefCell::new(false)),
            naive_username: Self::field(window, cx, "Username", None),
            naive_password: Self::secret_field(window, cx, "Password", None),
            naive_uot: Rc::new(RefCell::new(false)),
            naive_quic: Rc::new(RefCell::new(false)),
            naive_cc: Self::panel_select(window, cx, naive_cc_choices(), ""),
            naive_open: Rc::new(RefCell::new(false)),
            ssh_user: Self::field(window, cx, "User", None),
            ssh_password: Self::secret_field(window, cx, "Password", None),
            ssh_private_key: Self::area(window, cx, "Private key (paste or Browse…)", None),
            ssh_key_path: Self::field(window, cx, "Private key path", None),
            ssh_key_pass: Self::secret_field(window, cx, "Private key passphrase", None),
            ssh_host_key: Self::field(window, cx, "Host key (comma list)", None),
            ssh_host_key_algs: Self::field(window, cx, "Host key algorithms (comma list)", None),
            ssh_client_version: Self::field(window, cx, "Client version", None),
            ssh_open: Rc::new(RefCell::new(false)),
            snell_psk: Self::secret_field(window, cx, "PSK", None),
            snell_version: Self::panel_select(window, cx, snell_version_choices(), ""),
            snell_obfs_mode: Self::field(window, cx, "Obfs mode", None),
            snell_obfs_host: Self::field(window, cx, "Obfs host", None),
            snell_userkey: Self::field(window, cx, "User key", None),
            snell_reuse: Rc::new(RefCell::new(false)),
            snell_network: Self::field(window, cx, "Network", None),
            snell_mode: Self::field(window, cx, "Mode", None),
            snell_open: Rc::new(RefCell::new(false)),
            shadowtls_version: Self::panel_select(window, cx, shadowtls_version_choices(), ""),
            shadowtls_password: Self::secret_field(window, cx, "Password", None),
            shadowtls_open: Rc::new(RefCell::new(false)),
            xray_uuid: Self::secret_field(window, cx, "UUID", None),
            xray_enc: Self::field(window, cx, "Encryption (default none)", None),
            xray_flow: Self::panel_select(window, cx, xray_flow_choices(), ""),
            xray_sni: Self::field(window, cx, "SNI", None),
            xray_fp: Self::panel_select(window, cx, fp_choices(), ""),
            xray_alpn: Self::field(window, cx, "ALPN (comma list)", None),
            xray_pbk: Self::field(window, cx, "REALITY public key (pbk)", None),
            xray_sid: Self::field(window, cx, "REALITY short id (sid)", None),
            xray_spiderx: Self::field(window, cx, "REALITY spiderX path", None),
            xray_network: Self::panel_select(window, cx, xray_network_choices(), ""),
            xray_mode: Self::field(window, cx, "Mode", None),
            xray_mux: Self::panel_select(window, cx, xray_mux_choices(), ""),
            xray_open: Rc::new(RefCell::new(false)),
            chain_hops: Rc::new(RefCell::new(Vec::new())),
            chain_pick: Self::panel_select(window, cx, chain_pick_choices(&[]), ""),
            chain_library: Rc::new(RefCell::new(Vec::new())),
            chain_open: Rc::new(RefCell::new(false)),
            extra_exe: Self::field(window, cx, "Executable (app dir file name)", None),
            extra_args: Self::field(window, cx, "Args", None),
            extra_config: Self::area(window, cx, "Config JSON (passed as -c <file>)", None),
            extra_open: Rc::new(RefCell::new(false)),
            advanced: Rc::new(RefCell::new(false)),
        }
    }

    fn prefilled(
        window: &mut Window,
        cx: &mut Context<AppView>,
        config: &ProxyConfig,
        test_url: Option<&str>,
    ) -> Self {
        let read_keys: &[&str] = guided_identity_fields(&config.protocol)
            .map(|guided| guided.read_keys)
            .unwrap_or(&[]);
        let selected = protocol_choices()
            .iter()
            .position(|choice| choice.value == config.protocol)
            .map(IndexPath::new);

        let params = config.params.as_str();
        let network_value = guided_prefill_value(params, &["network"]);
        let network = if network_value.is_empty() {
            "tcp".to_string()
        } else {
            network_value
        };
        let security = guided_prefill_value(params, &["security"]);

        let tls_on = if config.protocol == "vmess" {
            vmess_tls_enabled(params)
        } else {
            matches!(
                security.to_ascii_lowercase().as_str(),
                "tls" | "xtls" | "reality"
            )
        };
        Self {
            name: Self::field(window, cx, "Name", Some(config.name.as_str())),
            protocol_select: cx.new(|cx| {
                SelectState::new(ProtocolChoiceList(protocol_choices()), selected, window, cx)
            }),
            last_protocol: Rc::new(RefCell::new(config.protocol.clone())),
            protocol_locked: true,
            locked_protocol: config.protocol.clone(),
            params: Self::area(
                window,
                cx,
                "Protocol params JSON (multiline)",
                Some(config.params.as_str()),
            ),
            group: Self::field(window, cx, "Group", Some(config.group.as_str())),
            bind: Self::field(window, cx, "Bind address", Some(config.bind_ip.as_str())),
            port: Self::field(
                window,
                cx,
                "Port (blank or auto = auto-assign)",
                Some(&config.port.to_string()),
            ),
            test_url: Self::field(window, cx, "Test URL (blank = global)", test_url),
            guided_server: Self::field(
                window,
                cx,
                "Server (e.g. example.com)",
                Some(&guided_prefill_value(
                    &config.params,
                    &["server", "address"],
                )),
            ),
            guided_port: Self::field(
                window,
                cx,
                "Port (e.g. 443)",
                Some(&guided_prefill_value(
                    &config.params,
                    &["server_port", "port"],
                )),
            ),
            guided_identity: Self::secret_field(
                window,
                cx,
                "Identity secret",
                Some(&guided_prefill_value(&config.params, read_keys)),
            ),
            vless_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            vless_flow: Self::panel_select(
                window,
                cx,
                flow_choices(),
                &guided_prefill_value(params, &["flow"]),
            ),
            vless_packet_encoding: Self::panel_select(
                window,
                cx,
                packet_encoding_choices(),
                &guided_prefill_value(params, &["packet_encoding", "packetEncoding"]),
            ),
            s1_network: Self::panel_select(window, cx, network_choices(), &network),
            s1_path: Self::field(
                window,
                cx,
                "Path",
                Some(&guided_prefill_value(params, &["path"])),
            ),
            s1_host: Self::field(
                window,
                cx,
                "Host",
                Some(&guided_prefill_value(params, &["host"])),
            ),
            s1_method: Self::field(
                window,
                cx,
                "Method",
                Some(&guided_prefill_value(params, &["method"])),
            ),
            s1_service_name: Self::field(
                window,
                cx,
                "Service name",
                Some(&guided_prefill_value(
                    params,
                    &["service_name", "serviceName"],
                )),
            ),
            s1_open: Rc::new(RefCell::new(false)),
            s2_enable: Rc::new(RefCell::new(tls_on)),
            s2_sni: Self::field(
                window,
                cx,
                "SNI",
                Some(&guided_prefill_value(params, &["sni", "peer"])),
            ),
            s2_alpn: Self::field(
                window,
                cx,
                "ALPN (comma list)",
                Some(&guided_prefill_value(params, &["alpn"])),
            ),
            s2_fp: Self::panel_select(
                window,
                cx,
                fp_choices(),
                &guided_prefill_value(params, &["fp", "fingerprint"]),
            ),
            s2_insecure: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &[
                    "insecure",
                    "allowInsecure",
                    "allow_insecure",
                    "allowinsecure",
                ],
            ))),
            s2_pbk: Self::field(
                window,
                cx,
                "Reality public key",
                Some(&guided_prefill_value(params, &["pbk"])),
            ),
            s2_sid: Self::field(
                window,
                cx,
                "Reality short id",
                Some(&guided_prefill_value(params, &["sid"])),
            ),
            s2_open: Rc::new(RefCell::new(false)),
            s3_mode: Rc::new(RefCell::new(guided_prefill_value(params, &["mux"]))),
            s3_brutal: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["brutal_enable"],
            ))),
            s3_up: Self::field(
                window,
                cx,
                "Brutal up Mbps",
                Some(&guided_prefill_value(
                    params,
                    &["brutal_up_mbps", "brutal_up"],
                )),
            ),
            s3_down: Self::field(
                window,
                cx,
                "Brutal down Mbps",
                Some(&guided_prefill_value(
                    params,
                    &["brutal_down_mbps", "brutal_down"],
                )),
            ),
            s3_open: Rc::new(RefCell::new(false)),
            s4_reuse: Rc::new(RefCell::new(guided_prefill_bool(params, &["reuse_addr"]))),
            s4_fastopen: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["tcp_fast_open"],
            ))),
            s4_fragment: Rc::new(RefCell::new(guided_prefill_bool(params, &["udp_fragment"]))),
            s4_multipath: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["tcp_multi_path"],
            ))),
            s4_timeout: Self::field(
                window,
                cx,
                "Connect timeout (e.g. 5s)",
                Some(&guided_prefill_value(params, &["connect_timeout"])),
            ),
            s4_bind: Self::field(
                window,
                cx,
                "Bind interface",
                Some(&guided_prefill_value(params, &["bind_interface"])),
            ),
            s4_inet4: Self::field(
                window,
                cx,
                "IPv4 bind address",
                Some(&guided_prefill_value(params, &["inet4_bind_address"])),
            ),
            s4_inet6: Self::field(
                window,
                cx,
                "IPv6 bind address",
                Some(&guided_prefill_value(params, &["inet6_bind_address"])),
            ),
            vless_open: Rc::new(RefCell::new(false)),

            vmess_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            vmess_aid: Self::field(
                window,
                cx,
                "Alter ID (aid)",
                Some(&guided_prefill_value(params, &["alter_id", "aid"])),
            ),
            vmess_security: Self::panel_select(
                window,
                cx,
                vmess_security_choices(),
                &vmess_cipher_value(params),
            ),
            vmess_packet_encoding: Self::panel_select(
                window,
                cx,
                packet_encoding_choices(),
                &guided_prefill_value(params, &["packet_encoding", "packetEncoding"]),
            ),
            vmess_open: Rc::new(RefCell::new(false)),
            trojan_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            trojan_open: Rc::new(RefCell::new(false)),
            ss_method: Self::panel_select(
                window,
                cx,
                ss_method_choices(),
                &guided_prefill_value(params, &["method"]),
            ),
            ss_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            ss_plugin: Self::panel_select(
                window,
                cx,
                ss_plugin_choices(),
                &guided_prefill_value(params, &["plugin"]),
            ),
            ss_plugin_opts: Self::field(
                window,
                cx,
                "Plugin options",
                Some(&guided_prefill_value(
                    params,
                    &["plugin_opts", "plugin-opts", "pluginOpts"],
                )),
            ),
            ss_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),
            ss_open: Rc::new(RefCell::new(false)),

            wg_private: Self::secret_field(
                window,
                cx,
                "Private key",
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            wg_public: Self::field(
                window,
                cx,
                "Peer public key",
                Some(&guided_prefill_value(params, &["public_key", "publickey"])),
            ),
            wg_preshared: Self::secret_field(
                window,
                cx,
                "Preshared key",
                Some(&guided_prefill_value(
                    params,
                    &["pre_shared_key", "presharedkey", "preshared_key"],
                )),
            ),
            wg_reserved: Self::field(
                window,
                cx,
                "Reserved (comma ints)",
                Some(&guided_prefill_value(params, &["reserved"])),
            ),
            wg_keepalive: Self::field(
                window,
                cx,
                "Keepalive (seconds)",
                Some(&guided_prefill_value(
                    params,
                    &["persistent_keepalive", "persistent_keepalive_interval"],
                )),
            ),
            wg_local_addr: Self::field(
                window,
                cx,
                "Local address (comma CIDRs)",
                Some(&guided_prefill_value(
                    params,
                    &["address", "local_address", "local_addr"],
                )),
            ),
            wg_dns: Self::field(
                window,
                cx,
                "DNS",
                Some(&guided_prefill_value(params, &["dns"])),
            ),
            wg_mtu: Self::field(
                window,
                cx,
                "MTU",
                Some(&guided_prefill_value(params, &["mtu"])),
            ),
            wg_workers: Self::field(
                window,
                cx,
                "Workers",
                Some(&guided_prefill_value(params, &["workers"])),
            ),
            wg_sys_ifc: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["system", "sys_ifc"],
            ))),
            wg_open: Rc::new(RefCell::new(false)),
            awg_private: Self::secret_field(
                window,
                cx,
                "Private key",
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            awg_public: Self::field(
                window,
                cx,
                "Peer public key",
                Some(&guided_prefill_value(params, &["public_key", "publickey"])),
            ),
            awg_preshared: Self::secret_field(
                window,
                cx,
                "Preshared key",
                Some(&guided_prefill_value(
                    params,
                    &["pre_shared_key", "presharedkey", "preshared_key"],
                )),
            ),
            awg_reserved: Self::field(
                window,
                cx,
                "Reserved (comma ints)",
                Some(&guided_prefill_value(params, &["reserved"])),
            ),
            awg_keepalive: Self::field(
                window,
                cx,
                "Keepalive (seconds)",
                Some(&guided_prefill_value(
                    params,
                    &["persistent_keepalive", "persistent_keepalive_interval"],
                )),
            ),
            awg_local_addr: Self::field(
                window,
                cx,
                "Local address (comma CIDRs)",
                Some(&guided_prefill_value(
                    params,
                    &["address", "local_address", "local_addr"],
                )),
            ),
            awg_dns: Self::field(
                window,
                cx,
                "DNS",
                Some(&guided_prefill_value(params, &["dns"])),
            ),
            awg_mtu: Self::field(
                window,
                cx,
                "MTU",
                Some(&guided_prefill_value(params, &["mtu"])),
            ),
            awg_workers: Self::field(
                window,
                cx,
                "Workers",
                Some(&guided_prefill_value(params, &["workers"])),
            ),
            awg_sys_ifc: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["system", "sys_ifc"],
            ))),
            awg_version: Self::panel_select(
                window,
                cx,
                awg_version_choices(),
                &guided_prefill_value(params, &["awg_version", "version"]),
            ),
            awg_enable: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["enable_amnezia"],
            ))),
            awg_jc: Self::field(
                window,
                cx,
                "Jc",
                Some(&guided_prefill_value(params, &["jc"])),
            ),
            awg_jmin: Self::field(
                window,
                cx,
                "Jmin",
                Some(&guided_prefill_value(params, &["jmin"])),
            ),
            awg_jmax: Self::field(
                window,
                cx,
                "Jmax",
                Some(&guided_prefill_value(params, &["jmax"])),
            ),
            awg_s1: Self::field(
                window,
                cx,
                "S1",
                Some(&guided_prefill_value(params, &["s1"])),
            ),
            awg_s2: Self::field(
                window,
                cx,
                "S2",
                Some(&guided_prefill_value(params, &["s2"])),
            ),
            awg_s3: Self::field(
                window,
                cx,
                "S3",
                Some(&guided_prefill_value(params, &["s3"])),
            ),
            awg_s4: Self::field(
                window,
                cx,
                "S4",
                Some(&guided_prefill_value(params, &["s4"])),
            ),
            awg_h1: Self::field(
                window,
                cx,
                "H1",
                Some(&guided_prefill_value(params, &["h1"])),
            ),
            awg_h2: Self::field(
                window,
                cx,
                "H2",
                Some(&guided_prefill_value(params, &["h2"])),
            ),
            awg_h3: Self::field(
                window,
                cx,
                "H3",
                Some(&guided_prefill_value(params, &["h3"])),
            ),
            awg_h4: Self::field(
                window,
                cx,
                "H4",
                Some(&guided_prefill_value(params, &["h4"])),
            ),
            awg_i1: Self::field(
                window,
                cx,
                "I1",
                Some(&guided_prefill_value(params, &["i1"])),
            ),
            awg_i2: Self::field(
                window,
                cx,
                "I2",
                Some(&guided_prefill_value(params, &["i2"])),
            ),
            awg_i3: Self::field(
                window,
                cx,
                "I3",
                Some(&guided_prefill_value(params, &["i3"])),
            ),
            awg_i4: Self::field(
                window,
                cx,
                "I4",
                Some(&guided_prefill_value(params, &["i4"])),
            ),
            awg_i5: Self::field(
                window,
                cx,
                "I5",
                Some(&guided_prefill_value(params, &["i5"])),
            ),
            awg_hpk: Self::field(
                window,
                cx,
                "Header protection key",
                Some(&guided_prefill_value(params, &["headerprotectionkey"])),
            ),
            awg_pad: Self::field(
                window,
                cx,
                "Content padding addition",
                Some(&guided_prefill_value(params, &["contentpaddingaddition"])),
            ),
            awg_rekey_after: Self::field(
                window,
                cx,
                "Rekey after time",
                Some(&guided_prefill_value(params, &["rekeyaftertime"])),
            ),
            awg_rekey_timeout: Self::field(
                window,
                cx,
                "Rekey timeout",
                Some(&guided_prefill_value(params, &["rekeytimeout"])),
            ),
            awg_reject_after: Self::field(
                window,
                cx,
                "Reject after time",
                Some(&guided_prefill_value(params, &["rejectaftertime"])),
            ),
            awg_keepalive_timeout: Self::field(
                window,
                cx,
                "Keepalive timeout",
                Some(&guided_prefill_value(params, &["keepalivetimeout"])),
            ),
            awg_max_handshake: Self::field(
                window,
                cx,
                "Max handshake attempts",
                Some(&guided_prefill_value(params, &["maxhandshakeattempts"])),
            ),
            awg_trailers: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["randomtrailers", "random_trailers"],
            ))),
            awg_cookies: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["disablecookies", "disable_cookies"],
            ))),
            awg_open: Rc::new(RefCell::new(false)),

            hy_server_ports: Self::field(
                window,
                cx,
                "Server ports (e.g. 9000-9010)",
                Some(&guided_prefill_value(params, &["server_ports"])),
            ),
            hy_hop_interval: Self::field(
                window,
                cx,
                "Hop interval (e.g. 30s)",
                Some(&guided_prefill_value(params, &["hop_interval"])),
            ),
            hy_up: Self::field(
                window,
                cx,
                "Up Mbps",
                Some(&guided_prefill_value(params, &["up", "up_mbps", "upmbps"])),
            ),
            hy_down: Self::field(
                window,
                cx,
                "Down Mbps",
                Some(&guided_prefill_value(
                    params,
                    &["down", "down_mbps", "downmbps"],
                )),
            ),
            hy_obfs: Self::field(
                window,
                cx,
                "Obfs",
                Some(&guided_prefill_value(params, &["obfs"])),
            ),
            hy_auth_type: Self::panel_select(
                window,
                cx,
                hy_auth_type_choices(),
                &guided_prefill_value(params, &["auth_type"]),
            ),
            hy_auth: Self::secret_field(
                window,
                cx,
                "Auth secret",
                Some(&guided_prefill_value(params, &["auth"])),
            ),
            hy_recv_conn: Self::field(
                window,
                cx,
                "Recv window conn",
                Some(&guided_prefill_value(params, &["recv_window_conn"])),
            ),
            hy_recv: Self::field(
                window,
                cx,
                "Recv window",
                Some(&guided_prefill_value(params, &["recv_window"])),
            ),
            hy_mtu: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["disable_mtu_discovery"],
            ))),
            hy_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            hy_open: Rc::new(RefCell::new(false)),
            tuic_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            tuic_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            tuic_cc: Self::panel_select(
                window,
                cx,
                tuic_cc_choices(),
                &guided_prefill_value(params, &["congestion_control"]),
            ),
            tuic_relay: Self::panel_select(
                window,
                cx,
                tuic_relay_choices(),
                &guided_prefill_value(params, &["udp_relay_mode"]),
            ),
            tuic_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_stream"],
            ))),
            tuic_zero_rtt: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["zero_rtt_handshake", "zero_rtt"],
            ))),
            tuic_heartbeat: Self::field(
                window,
                cx,
                "Heartbeat (e.g. 10s)",
                Some(&guided_prefill_value(params, &["heartbeat"])),
            ),
            tuic_open: Rc::new(RefCell::new(false)),

            socks_version: Self::panel_select(
                window,
                cx,
                socks_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            socks_username: Self::field(
                window,
                cx,
                "Username",
                Some(&guided_prefill_value(params, &["username"])),
            ),
            socks_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            socks_network: Self::panel_select(
                window,
                cx,
                socks_network_choices(),
                &guided_prefill_value(params, &["network"]),
            ),
            socks_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),
            socks_open: Rc::new(RefCell::new(false)),
            http_username: Self::field(
                window,
                cx,
                "Username",
                Some(&guided_prefill_value(params, &["username"])),
            ),
            http_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            http_path: Self::field(
                window,
                cx,
                "Path",
                Some(&guided_prefill_value(params, &["path"])),
            ),
            http_headers: Self::field(
                window,
                cx,
                "Headers",
                Some(&guided_prefill_value(params, &["headers"])),
            ),
            http_open: Rc::new(RefCell::new(false)),
            anytls_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            anytls_interval: Self::field(
                window,
                cx,
                "Idle check interval (e.g. 30s)",
                Some(&guided_prefill_value(
                    params,
                    &["idle_session_check_interval", "interval"],
                )),
            ),
            anytls_timeout: Self::field(
                window,
                cx,
                "Idle timeout (e.g. 30s)",
                Some(&guided_prefill_value(
                    params,
                    &["idle_session_timeout", "timeout"],
                )),
            ),
            anytls_min: Self::field(
                window,
                cx,
                "Min idle sessions",
                Some(&guided_prefill_value(params, &["min_idle_session", "min"])),
            ),
            anytls_open: Rc::new(RefCell::new(false)),
            naive_username: Self::field(
                window,
                cx,
                "Username",
                Some(&guided_prefill_value(params, &["username"])),
            ),
            naive_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            naive_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),
            naive_quic: Rc::new(RefCell::new(guided_prefill_bool(params, &["quic"]))),
            naive_cc: Self::panel_select(
                window,
                cx,
                naive_cc_choices(),
                &guided_prefill_value(params, &["quic_congestion_control", "congestion_control"]),
            ),
            naive_open: Rc::new(RefCell::new(false)),
            ssh_user: Self::field(
                window,
                cx,
                "User",
                Some(&guided_prefill_value(params, &["user", "username"])),
            ),
            ssh_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            ssh_private_key: Self::area(
                window,
                cx,
                "Private key (paste or Browse…)",
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            ssh_key_path: Self::field(
                window,
                cx,
                "Private key path",
                Some(&guided_prefill_value(params, &["private_key_path"])),
            ),
            ssh_key_pass: Self::secret_field(
                window,
                cx,
                "Private key passphrase",
                Some(&guided_prefill_value(
                    params,
                    &["private_key_passphrase", "private_key_pass"],
                )),
            ),
            ssh_host_key: Self::field(
                window,
                cx,
                "Host key (comma list)",
                Some(&guided_prefill_list(params, &["host_key"])),
            ),
            ssh_host_key_algs: Self::field(
                window,
                cx,
                "Host key algorithms (comma list)",
                Some(&guided_prefill_list(params, &["host_key_algorithms"])),
            ),
            ssh_client_version: Self::field(
                window,
                cx,
                "Client version",
                Some(&guided_prefill_value(params, &["client_version"])),
            ),
            ssh_open: Rc::new(RefCell::new(false)),
            snell_psk: Self::secret_field(
                window,
                cx,
                "PSK",
                Some(&guided_prefill_value(params, &["psk"])),
            ),
            snell_version: Self::panel_select(
                window,
                cx,
                snell_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            snell_obfs_mode: Self::field(
                window,
                cx,
                "Obfs mode",
                Some(&guided_prefill_value(params, &["obfs_mode", "obfs"])),
            ),
            snell_obfs_host: Self::field(
                window,
                cx,
                "Obfs host",
                Some(&guided_prefill_value(params, &["obfs_host", "obfs-host"])),
            ),
            snell_userkey: Self::field(
                window,
                cx,
                "User key",
                Some(&guided_prefill_value(params, &["userkey"])),
            ),
            snell_reuse: Rc::new(RefCell::new(guided_prefill_bool(params, &["reuse"]))),
            snell_network: Self::field(
                window,
                cx,
                "Network",
                Some(&guided_prefill_value(params, &["network"])),
            ),
            snell_mode: Self::field(
                window,
                cx,
                "Mode",
                Some(&guided_prefill_value(params, &["mode"])),
            ),
            snell_open: Rc::new(RefCell::new(false)),
            shadowtls_version: Self::panel_select(
                window,
                cx,
                shadowtls_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            shadowtls_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            shadowtls_open: Rc::new(RefCell::new(false)),
            xray_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            xray_enc: Self::field(
                window,
                cx,
                "Encryption (default none)",
                Some(&guided_prefill_value(params, &["encryption"])),
            ),
            xray_flow: Self::panel_select(
                window,
                cx,
                xray_flow_choices(),
                &guided_prefill_value(params, &["flow"]),
            ),
            xray_sni: Self::field(
                window,
                cx,
                "SNI",
                Some(&guided_prefill_value(params, &["sni"])),
            ),
            xray_fp: Self::panel_select(
                window,
                cx,
                fp_choices(),
                &guided_prefill_value(params, &["fp", "fingerprint"]),
            ),
            xray_alpn: Self::field(
                window,
                cx,
                "ALPN (comma list)",
                Some(&guided_prefill_value(params, &["alpn"])),
            ),
            xray_pbk: Self::field(
                window,
                cx,
                "REALITY public key (pbk)",
                Some(&guided_prefill_value(params, &["pbk"])),
            ),
            xray_sid: Self::field(
                window,
                cx,
                "REALITY short id (sid)",
                Some(&guided_prefill_value(params, &["sid"])),
            ),
            xray_spiderx: Self::field(
                window,
                cx,
                "REALITY spiderX path",
                Some(&guided_prefill_value(params, &["spiderx"])),
            ),
            xray_network: Self::panel_select(
                window,
                cx,
                xray_network_choices(),
                &guided_prefill_value(params, &["network"]),
            ),
            xray_mode: Self::field(
                window,
                cx,
                "Mode",
                Some(&guided_prefill_value(params, &["mode"])),
            ),
            xray_mux: Self::panel_select(
                window,
                cx,
                xray_mux_choices(),
                &guided_prefill_value(params, &["xray_mux"]),
            ),
            xray_open: Rc::new(RefCell::new(false)),
            chain_hops: Rc::new(RefCell::new(chain_nodes_of(params))),
            chain_pick: Self::panel_select(window, cx, chain_pick_choices(&[]), ""),
            chain_library: Rc::new(RefCell::new(Vec::new())),
            chain_open: Rc::new(RefCell::new(false)),
            extra_exe: Self::field(
                window,
                cx,
                "Executable (app dir file name)",
                Some(&guided_prefill_value(params, &["exe", "binary"])),
            ),
            extra_args: Self::field(
                window,
                cx,
                "Args",
                Some(&guided_prefill_value(params, &["args"])),
            ),
            extra_config: Self::area(
                window,
                cx,
                "Config JSON (passed as -c <file>)",
                Some(&extra_config_prefill(params)),
            ),
            extra_open: Rc::new(RefCell::new(false)),
            advanced: Rc::new(RefCell::new(true)),
        }
    }

    fn port_budget_text(rows: &[ProxyConfig]) -> String {
        use crate::store::{
            AUTO_PORT_END, AUTO_PORT_SPILL_END, AUTO_PORT_SPILL_START, AUTO_PORT_START,
        };
        let total = (AUTO_PORT_END - AUTO_PORT_START + 1) as usize
            + (AUTO_PORT_SPILL_END - AUTO_PORT_SPILL_START + 1) as usize;
        let used = rows
            .iter()
            .filter(|row| crate::store::is_auto_port(row.port))
            .count();
        format!("Auto ports: {used} of {total} in use (10801–10999, spill 18081–18380)")
    }

    fn uuid_row(
        &self,
        view: &Entity<AppView>,
        secret: &Entity<InputState>,
        id_prefix: &str,
        cx: &App,
    ) -> impl IntoElement {
        let secret_entity = secret.clone();
        let uuid_view = view.clone();
        let presence = secret_state_caption(secret.read(cx).value().as_ref()).to_string();
        field().label("UUID *".to_string()).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id(format!("{id_prefix}-uuid"))
                                .test_support()
                                .child(Input::new(secret).mask_toggle()),
                        )
                        .child(
                            Button::new(format!("{id_prefix}-uuid-generate"))
                                .small()
                                .label("Generate")
                                .tooltip("Generate a random UUIDv4")
                                .accessibility_label("Generate a random UUIDv4")
                                .on_click(move |_, window, cx| {
                                    let generated = generate_uuid_v4();
                                    uuid_view.update(cx, |_, cx| {
                                        secret_entity.update(cx, |state, cx| {
                                            state.set_value(generated, &mut *window, cx);
                                        });
                                    });
                                    window.push_notification(
                                        Notification::info("Generated a new UUID"),
                                        cx,
                                    );
                                }),
                        ),
                )
                .child(
                    div()
                        .id(format!("{id_prefix}-uuid-state"))
                        .test_support()
                        .text_sm()
                        .child(presence),
                ),
        )
    }

    fn panel_select_row(
        label: &str,
        tip_id: &'static str,
        tip: &str,
        select_id: &'static str,
        a11y: &str,
        state: &Entity<SelectState<PanelChoiceList>>,
    ) -> Field {
        let tip = tip.to_string();
        field().label(label.to_string()).child(
            div()
                .id(tip_id)
                .test_support()
                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                })
                .child(
                    Select::new(state)
                        .id(select_id)
                        .small()
                        .accessibility_label(a11y),
                ),
        )
    }

    fn page_disclosure(
        caption: &str,
        caption_id: &'static str,
        toggle_id: &'static str,
        section_id: &'static str,
        open: &Rc<RefCell<bool>>,
        view: &Entity<AppView>,
        block: impl IntoElement,
    ) -> impl IntoElement {
        Collapsible::new()
            .open(*open.borrow())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .id(caption_id)
                            .test_support()
                            .child(Label::new(caption.to_string())),
                    )
                    .child(panel_toggle_button(
                        toggle_id,
                        if *open.borrow() { "Hide" } else { "Show" },
                        view,
                        open,
                    )),
            )
            .content(div().id(section_id).test_support().child(block))
    }

    fn s1_network_section(&self, view: &Entity<AppView>, _cx: &App) -> impl IntoElement {
        let network_select = self.s1_network.clone();
        let picker = field().label("Network".to_string()).child(
            div()
                .id("s1-network-tip")
                .test_support()
                .tooltip(move |_, cx| {
                    cx.new(|_| Tooltip::new("Stream transport network".to_string()))
                        .into()
                })
                .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                })
                .child(
                    Select::new(&network_select)
                        .id("s1-network")
                        .small()
                        .accessibility_label("Stream transport network"),
                ),
        );
        field().label_indent(false).child(
            Collapsible::new()
                .open(*self.s1_open.borrow())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id("s1-caption")
                                .test_support()
                                .child(Label::new("Network")),
                        )
                        .child(panel_toggle_button(
                            "s1-toggle",
                            if *self.s1_open.borrow() {
                                "Hide"
                            } else {
                                "Show"
                            },
                            view,
                            &self.s1_open,
                        )),
                )
                .content(div().id("s1-section").test_support().child(picker)),
        )
    }

    fn s1_camouflage_section(&self, cx: &App) -> Option<AnyElement> {
        let network = Self::panel_value(&self.s1_network, cx, "tcp");
        let conditional = |id: &'static str, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let mut body = div().flex().flex_col().gap_1();
        let disguised = match network.as_str() {
            "ws" => {
                body = body
                    .child(conditional("s1-path", "Path", &self.s1_path))
                    .child(conditional("s1-host", "Host", &self.s1_host));
                true
            }
            "httpupgrade" | "http" => {
                body = body
                    .child(conditional("s1-host", "Host", &self.s1_host))
                    .child(conditional("s1-path", "Path", &self.s1_path));
                if network == "http" {
                    body = body.child(conditional("s1-method", "Method", &self.s1_method));
                }
                true
            }
            "grpc" => {
                body = body.child(conditional(
                    "s1-service-name",
                    "Service name",
                    &self.s1_service_name,
                ));
                true
            }
            _ => false,
        };
        if !disguised {
            return None;
        }
        Some(
            field()
                .label_indent(false)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .id("camouflage-caption")
                                .test_support()
                                .child(Label::new("Camouflage")),
                        )
                        .child(div().id("camouflage-section").test_support().child(body)),
                )
                .into_any_element(),
        )
    }

    fn s2_tls_section(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        forced: bool,
    ) -> impl IntoElement {
        let _ = cx;
        let mut tls_body = div().flex().flex_col().gap_1();
        if forced {
            tls_body = tls_body.child(
                field().label_indent(false).child(
                    div()
                        .id("s2-forced-note")
                        .test_support()
                        .child("TLS is always on for this protocol (security locked tls)."),
                ),
            );
        } else {
            let s2_enable = self.s2_enable.clone();
            let s2_repaint = repaint.clone();
            tls_body = tls_body.child(
            field().label_indent(false).child(
                div().id("s2-enable").test_support().child(
                    Switch::new("s2-enable-switch")
                        .small()
                        .checked(*self.s2_enable.borrow())
                        .label("Enable TLS")
                        .tooltip(
                            "Wrap the stream in TLS (security=tls, or reality with a public key)",
                        )
                        .accessibility_label("Enable TLS for this row")
                        .on_change(move |next: &bool, window, cx| {
                            *s2_enable.borrow_mut() = *next;
                            s2_repaint(window, cx);
                        }),
                ),
            ),
        );
        }
        field().label_indent(false).child(
            Collapsible::new()
                .open(*self.s2_open.borrow())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id("s2-caption")
                                .test_support()
                                .child(Label::new(
                                    if forced { "TLS (always on)" } else { "TLS" }.to_string(),
                                )),
                        )
                        .child(panel_toggle_button(
                            "s2-toggle",
                            if *self.s2_open.borrow() {
                                "Hide"
                            } else {
                                "Show"
                            },
                            view,
                            &self.s2_open,
                        )),
                )
                .content(div().id("s2-section").test_support().child(tls_body)),
        )
    }

    fn s2_security_section(&self, cx: &App) -> impl IntoElement {
        let _ = cx;
        let fp_select = self.s2_fp.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                field().label("SNI".to_string()).child(
                    div()
                        .id("s2-sni")
                        .test_support()
                        .child(Input::new(&self.s2_sni)),
                ),
            )
            .child(
                field().label("ALPN (comma list)".to_string()).child(
                    div()
                        .id("s2-alpn")
                        .test_support()
                        .child(Input::new(&self.s2_alpn)),
                ),
            )
            .child(
                field().label("Fingerprint".to_string()).child(
                    div()
                        .id("s2-fp-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new("uTLS fingerprint".to_string()))
                                .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&fp_select)
                                .id("s2-fp")
                                .small()
                                .accessibility_label("uTLS fingerprint"),
                        ),
                ),
            );
        field().label_indent(false).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .id("tls-security-caption")
                        .test_support()
                        .child(Label::new("TLS Security")),
                )
                .child(div().id("tls-security-section").test_support().child(body)),
        )
    }

    fn s2_extras_section(&self, repaint: &DialogRepaint) -> impl IntoElement {
        let s2_insecure = self.s2_insecure.clone();
        let s2_repaint_insecure = repaint.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                field().label_indent(false).child(
                    div().id("s2-insecure").test_support().child(
                        Switch::new("s2-insecure-switch")
                            .small()
                            .checked(*self.s2_insecure.borrow())
                            .label("Allow insecure")
                            .tooltip("Skip certificate verification (testing only)")
                            .accessibility_label("Skip TLS certificate verification")
                            .on_change(move |next: &bool, window, cx| {
                                *s2_insecure.borrow_mut() = *next;
                                s2_repaint_insecure(window, cx);
                            }),
                    ),
                ),
            )
            .child(
                field().label("Reality public key".to_string()).child(
                    div()
                        .id("s2-pbk")
                        .test_support()
                        .child(Input::new(&self.s2_pbk)),
                ),
            )
            .child(
                field().label("Reality short id".to_string()).child(
                    div()
                        .id("s2-sid")
                        .test_support()
                        .child(Input::new(&self.s2_sid)),
                ),
            );
        field().label_indent(false).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .id("tls-extras-caption")
                        .test_support()
                        .child(Label::new("TLS extras")),
                )
                .child(div().id("tls-extras-section").test_support().child(body)),
        )
    }

    fn s3_section(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        vision: bool,
    ) -> AnyElement {
        let _ = cx;
        if vision {
            return field().label_indent(false).child(
                div()
                    .id("s3-vision-note")
                    .test_support()
                    .child("Mux is unavailable with the xtls-rprx-vision flow — stored mux keys are preserved."),
            ).into_any_element();
        }
        let effective = self.s3_effective_mode();
        let selected = ["", "on", "off"].iter().position(|mode| *mode == effective);
        let mode_labels = ["Keep Default", "On", "Off"];
        let mode_cell = self.s3_mode.clone();
        let mode_repaint = repaint.clone();
        let mut group = RadioGroup::horizontal("s3-mux-options")
            .selected_index(selected)
            .on_change(move |ix, window, cx| {
                if let Some(mode) = ["", "on", "off"].get(*ix) {
                    *mode_cell.borrow_mut() = mode.to_string();
                }
                mode_repaint(window, cx);
            });
        for label in mode_labels {
            group = group.child(
                Radio::new(label)
                    .small()
                    .label(label)
                    .tooltip(format!("Mux mode {label}")),
            );
        }
        let mut s3_body = div().flex().flex_col().gap_1().child(
            field().label("Multiplex mode".to_string()).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("s3-mux-caption")
                            .test_support()
                            .child(Label::new("Multiplex mode")),
                    )
                    .child(div().id("s3-mux-group").test_support().child(group)),
            ),
        );
        if *self.s3_brutal.borrow() && effective == "on" && self.s3_mode.borrow().as_str() != "on" {
            s3_body = s3_body.child(
                div()
                    .id("s3-brutal-note")
                    .test_support()
                    .child("Brutal requires mux (forced On)."),
            );
        }
        let brutal_cell = self.s3_brutal.clone();
        let brutal_repaint = repaint.clone();
        s3_body = s3_body
            .child(
                field().label_indent(false).child(
                    div().id("s3-brutal").test_support().child(
                        Switch::new("s3-brutal-switch")
                            .small()
                            .checked(*self.s3_brutal.borrow())
                            .label("Brutal (forces mux On)")
                            .tooltip("Brutal congestion speeds (Mbps); enabling forces mux On")
                            .accessibility_label("Enable Brutal speeds (forces mux On)")
                            .on_change(move |next: &bool, window, cx| {
                                *brutal_cell.borrow_mut() = *next;
                                brutal_repaint(window, cx);
                            }),
                    ),
                ),
            )
            .child(
                field().label("Brutal up Mbps".to_string()).child(
                    div()
                        .id("s3-up")
                        .test_support()
                        .child(Input::new(&self.s3_up)),
                ),
            )
            .child(
                field().label("Brutal down Mbps".to_string()).child(
                    div()
                        .id("s3-down")
                        .test_support()
                        .child(Input::new(&self.s3_down)),
                ),
            );
        field()
            .label_indent(false)
            .child(
                Collapsible::new()
                    .open(*self.s3_open.borrow())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .id("s3-caption")
                                    .test_support()
                                    .child(Label::new("Mux")),
                            )
                            .child(panel_toggle_button(
                                "s3-toggle",
                                if *self.s3_open.borrow() {
                                    "Hide"
                                } else {
                                    "Show"
                                },
                                view,
                                &self.s3_open,
                            )),
                    )
                    .content(div().id("s3-section").test_support().child(s3_body)),
            )
            .into_any_element()
    }

    fn s4_section(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let _ = cx;
        let dial_switch = |id: &'static str,
                           switch_id: &'static str,
                           label: &str,
                           tip: &str,
                           cell: &Rc<RefCell<bool>>| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let tip = tip.to_string();
            let label = label.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let mut s4_body = div().flex().flex_col().gap_1();
        s4_body = s4_body
            .child(dial_switch(
                "s4-reuse",
                "s4-reuse-switch",
                "Reuse address",
                "SO_REUSEADDR on the upstream socket",
                &self.s4_reuse,
            ))
            .child(dial_switch(
                "s4-fastopen",
                "s4-fastopen-switch",
                "TCP fast open",
                "TCP Fast Open on the upstream socket",
                &self.s4_fastopen,
            ))
            .child(dial_switch(
                "s4-fragment",
                "s4-fragment-switch",
                "UDP fragment",
                "Fragment upstream UDP packets",
                &self.s4_fragment,
            ))
            .child(dial_switch(
                "s4-multipath",
                "s4-multipath-switch",
                "TCP multipath",
                "Multipath TCP on the upstream socket",
                &self.s4_multipath,
            ))
            .child(
                field().label("Connect timeout".to_string()).child(
                    div()
                        .id("s4-timeout")
                        .test_support()
                        .child(Input::new(&self.s4_timeout)),
                ),
            )
            .child(
                field().label("Bind interface".to_string()).child(
                    div()
                        .id("s4-bind")
                        .test_support()
                        .child(Input::new(&self.s4_bind)),
                ),
            )
            .child(
                field().label("IPv4 bind address".to_string()).child(
                    div()
                        .id("s4-inet4")
                        .test_support()
                        .child(Input::new(&self.s4_inet4)),
                ),
            )
            .child(
                field().label("IPv6 bind address".to_string()).child(
                    div()
                        .id("s4-inet6")
                        .test_support()
                        .child(Input::new(&self.s4_inet6)),
                ),
            );

        field().label_indent(false).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .id("s4-caption")
                        .test_support()
                        .child(Label::new("Dial Fields")),
                )
                .child(div().id("s4-section").test_support().child(s4_body)),
        )
    }

    fn vmess_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.vmess_uuid, "vmess", cx));
        block = block.child(
            field().label("Alter ID".to_string()).child(
                div()
                    .id("vmess-aid")
                    .test_support()
                    .child(Input::new(&self.vmess_aid)),
            ),
        );
        block = block.child(Self::panel_select_row(
            "Security",
            "vmess-security-tip",
            "VMess packet cipher (legacy links keep it in security)",
            "vmess-security",
            "VMess security (packet cipher)",
            &self.vmess_security,
        ));
        block = block.child(Self::panel_select_row(
            "Packet encoding",
            "vmess-packet-encoding-tip",
            "VMess packet encoding",
            "vmess-packet-encoding",
            "VMess packet encoding",
            &self.vmess_packet_encoding,
        ));
        block = block.child(self.s1_network_section(view, cx));
        block = block.child(self.s2_tls_section(view, repaint, cx, false));
        block = block.child(self.s3_section(view, repaint, cx, false));

        Self::page_disclosure(
            "VMess settings",
            "vmess-section-caption",
            "vmess-section-toggle",
            "vmess-section",
            &self.vmess_open,
            view,
            block,
        )
    }

    fn trojan_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(password_row(
            "Password *",
            "trojan-password",
            &self.trojan_password,
            cx,
        ));
        block = block.child(self.s1_network_section(view, cx));
        block = block.child(self.s2_tls_section(view, repaint, cx, false));
        block = block.child(self.s3_section(view, repaint, cx, false));

        Self::page_disclosure(
            "Trojan settings",
            "trojan-section-caption",
            "trojan-section-toggle",
            "trojan-section",
            &self.trojan_open,
            view,
            block,
        )
    }

    fn shadowsocks_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            "Method",
            "ss-method-tip",
            "Shadowsocks cipher (all 18)",
            "ss-method",
            "Shadowsocks method",
            &self.ss_method,
        ));
        block = block.child(password_row(
            "Password *",
            "ss-password",
            &self.ss_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            "Plugin",
            "ss-plugin-tip",
            "Plugin executable (none runs stock)",
            "ss-plugin",
            "Shadowsocks plugin",
            &self.ss_plugin,
        ));
        block = block.child(
            field().label("Plugin options".to_string()).child(
                div()
                    .id("ss-plugin-opts")
                    .test_support()
                    .child(Input::new(&self.ss_plugin_opts)),
            ),
        );
        let uot_cell = self.ss_uot.clone();
        let uot_repaint = repaint.clone();
        block = block.child(
            field().label_indent(false).child(
                div().id("ss-uot").test_support().child(
                    Switch::new("ss-uot-switch")
                        .small()
                        .checked(*self.ss_uot.borrow())
                        .label("UDP over TCP (uot)")
                        .tooltip("UDP over TCP for shadowsocks (folds the uot flat)")
                        .accessibility_label("UDP over TCP for shadowsocks")
                        .on_change(move |next: &bool, window, cx| {
                            *uot_cell.borrow_mut() = *next;
                            uot_repaint(window, cx);
                        }),
                ),
            ),
        );
        if !Self::panel_value(&self.ss_plugin, cx, "").trim().is_empty() {
            block = block.child(field().label_indent(false).child(
                div().id("ss-plugin-note").test_support().child(
                    "Plugin rows need the plugin executable beside the engine — stock check fails without it.",
                ),
            ));
        }
        block = block.child(self.s3_section(view, repaint, cx, false));

        Self::page_disclosure(
            "Shadowsocks settings",
            "ss-section-caption",
            "ss-section-toggle",
            "ss-section",
            &self.ss_open,
            view,
            block,
        )
    }

    fn wg_peer_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        peer: WgPeerRefs<'_>,
    ) -> impl IntoElement {
        let prefix = peer.prefix;
        let secret_entity = peer.private_key.clone();
        let keygen_view = view.clone();
        let key_presence =
            secret_state_caption(peer.private_key.read(cx).value().as_ref()).to_string();
        let sys_cell = peer.sys_ifc.clone();
        let sys_repaint = repaint.clone();
        let row = |id: String, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let masked_row = |id: String, label: &str, state: &Entity<InputState>| {
            field().label(label.to_string()).child(
                div()
                    .id(id)
                    .test_support()
                    .child(Input::new(state).mask_toggle()),
            )
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                field().label("Private key *".to_string()).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .id(format!("{prefix}-private-key"))
                                        .test_support()
                                        .child(Input::new(peer.private_key).mask_toggle()),
                                )
                                .child(
                                    Button::new(format!("{prefix}-private-key-generate"))
                                .small()
                                .label("Generate")
                                .tooltip(
                                    "Generate a fresh WireGuard private key (local only — no WARP registration)",
                                )
                                .accessibility_label("Generate a fresh WireGuard private key")
                                .on_click(move |_, window, cx| {
                                    let generated = generate_wireguard_key();
                                    keygen_view.update(cx, |_, cx| {
                                        secret_entity.update(cx, |state, cx| {
                                            state.set_value(generated, &mut *window, cx);
                                        });
                                    });
                                    window.push_notification(
                                        Notification::info("Generated a new WireGuard private key"),
                                        cx,
                                    );
                                }),
                        )
                        .child(
                            div()
                                .id(format!("{prefix}-private-key-state"))
                                .test_support()
                                .text_sm()
                                .child(key_presence),
                        ),
                ),
            ))
            .child(row(
                format!("{prefix}-public-key"),
                "Public key (peer)",
                peer.public_key,
            ))
            .child(masked_row(
                format!("{prefix}-preshared-key"),
                "Preshared key",
                peer.pre_shared_key,
            ))
            .child(row(
                format!("{prefix}-reserved"),
                "Reserved (comma ints)",
                peer.reserved,
            ))
            .child(row(
                format!("{prefix}-keepalive"),
                "Keepalive (seconds)",
                peer.keepalive,
            ))
            .child(row(
                format!("{prefix}-local-addr"),
                "Local address (comma CIDRs)",
                peer.local_addr,
            ))
            .child(row(format!("{prefix}-dns"), "DNS", peer.dns))
            .child(row(format!("{prefix}-mtu"), "MTU", peer.mtu))
            .child(row(
                format!("{prefix}-workers"),
                "Workers",
                peer.workers,
            ))
            .child(
                field().label_indent(false).child(
                    div()
                        .id(format!("{prefix}-sys-ifc"))
                        .test_support()
                        .child(
                            Switch::new(format!("{prefix}-sys-ifc-switch"))
                                .small()
                                .checked(*peer.sys_ifc.borrow())
                                .label("System interface (sys_ifc)")
                                .tooltip("Use the system WireGuard interface (stores as system)")
                                .accessibility_label("Use the system WireGuard interface")
                                .on_change(move |next: &bool, window, cx| {
                                    *sys_cell.borrow_mut() = *next;
                                    sys_repaint(window, cx);
                                }),
                        ),
                ),
            )
    }

    fn wireguard_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.wg_peer_block(
            view,
            repaint,
            cx,
            WgPeerRefs {
                prefix: "wg",
                private_key: &self.wg_private,
                public_key: &self.wg_public,
                pre_shared_key: &self.wg_preshared,
                reserved: &self.wg_reserved,
                keepalive: &self.wg_keepalive,
                local_addr: &self.wg_local_addr,
                dns: &self.wg_dns,
                mtu: &self.wg_mtu,
                workers: &self.wg_workers,
                sys_ifc: &self.wg_sys_ifc,
            },
        ));

        block = block.child(field().label_indent(false).child(
            div().id("wg-notes").test_support().child(
                "WireGuard rows become endpoints[] at generate. Blank AllowedIPs defaults to \
                 0.0.0.0/0; DNS is stored but never emitted; udp_timeout stays JSON-only.",
            ),
        ));
        Self::page_disclosure(
            "WireGuard settings",
            "wg-section-caption",
            "wg-section-toggle",
            "wg-section",
            &self.wg_open,
            view,
            block,
        )
    }

    fn amneziawg_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let row = |id: &'static str, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let caption = |id: &'static str, text: &str| {
            div()
                .id(id)
                .test_support()
                .child(Label::new(text.to_string()))
        };
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.wg_peer_block(
            view,
            repaint,
            cx,
            WgPeerRefs {
                prefix: "awg",
                private_key: &self.awg_private,
                public_key: &self.awg_public,
                pre_shared_key: &self.awg_preshared,
                reserved: &self.awg_reserved,
                keepalive: &self.awg_keepalive,
                local_addr: &self.awg_local_addr,
                dns: &self.awg_dns,
                mtu: &self.awg_mtu,
                workers: &self.awg_workers,
                sys_ifc: &self.awg_sys_ifc,
            },
        ));
        block = block.child(switch_row(
            "awg-enable",
            "awg-enable-switch".to_string(),
            "Enable Amnezia",
            "AmneziaWG obfuscation marker (UI state — stripped at generate, like brutal speeds)",
            &self.awg_enable,
            repaint,
        ));
        block = block.child(Self::panel_select_row(
            "AWG version",
            "awg-version-tip",
            "AmneziaWG version (2.0 or 3.x — gates the v3 fields below)",
            "awg-version",
            "AmneziaWG version",
            &self.awg_version,
        ));
        block = block.child(caption("awg-junk-caption", "Junk train"));
        block = block
            .child(row("awg-jc", "Jc", &self.awg_jc))
            .child(row("awg-jmin", "Jmin", &self.awg_jmin))
            .child(row("awg-jmax", "Jmax", &self.awg_jmax));
        block = block.child(caption("awg-length-caption", "Length prefixes (S1–S4)"));
        block = block
            .child(row("awg-s1", "S1", &self.awg_s1))
            .child(row("awg-s2", "S2", &self.awg_s2))
            .child(row("awg-s3", "S3", &self.awg_s3))
            .child(row("awg-s4", "S4", &self.awg_s4));
        block = block.child(caption("awg-header-caption", "Header ranges (H1–H4)"));
        block = block
            .child(row("awg-h1", "H1", &self.awg_h1))
            .child(row("awg-h2", "H2", &self.awg_h2))
            .child(row("awg-h3", "H3", &self.awg_h3))
            .child(row("awg-h4", "H4", &self.awg_h4));
        block = block.child(caption("awg-chain-caption", "CPS chains (I1–I5)"));
        block = block
            .child(row("awg-i1", "I1", &self.awg_i1))
            .child(row("awg-i2", "I2", &self.awg_i2))
            .child(row("awg-i3", "I3", &self.awg_i3))
            .child(row("awg-i4", "I4", &self.awg_i4))
            .child(row("awg-i5", "I5", &self.awg_i5));

        if Self::panel_value(&self.awg_version, cx, "") == "3.x" {
            block = block.child(caption("awg-v3-caption", "v3-only fields"));
            block = block
                .child(row("awg-hpk", "Header protection key", &self.awg_hpk))
                .child(row("awg-pad", "Content padding addition", &self.awg_pad))
                .child(row(
                    "awg-rekey-after",
                    "Rekey after time",
                    &self.awg_rekey_after,
                ))
                .child(row(
                    "awg-rekey-timeout",
                    "Rekey timeout",
                    &self.awg_rekey_timeout,
                ))
                .child(row(
                    "awg-reject-after",
                    "Reject after time",
                    &self.awg_reject_after,
                ))
                .child(row(
                    "awg-keepalive-timeout",
                    "Keepalive timeout",
                    &self.awg_keepalive_timeout,
                ))
                .child(row(
                    "awg-max-handshake",
                    "Max handshake attempts",
                    &self.awg_max_handshake,
                ));
            block = block.child(switch_row(
                "awg-trailers",
                "awg-trailers-switch".to_string(),
                "Random trailers",
                "Append random trailers (v3-only — passes through for the fork check)",
                &self.awg_trailers,
                repaint,
            ));
            block = block.child(switch_row(
                "awg-cookies",
                "awg-cookies-switch".to_string(),
                "Disable cookies",
                "Disable handshake cookies (v3-only — passes through for the fork check)",
                &self.awg_cookies,
                repaint,
            ));
        } else {
            block = block.child(
                field().label_indent(false).child(
                    div()
                        .id("awg-v3-note")
                        .test_support()
                        .child("v3-only fields hidden — stored values preserved (never wiped)."),
                ),
            );
        }

        block = block.child(field().label_indent(false).child(
            div().id("awg-notes").test_support().child(
                "AmneziaWG rows run in the fork sidecar (sing-box-awg). v3-only keys on a 2.0 row \
                 fail generate naming the field; udp_timeout stays JSON-only.",
            ),
        ));
        Self::page_disclosure(
            "AmneziaWG settings",
            "awg-section-caption",
            "awg-section-toggle",
            "awg-section",
            &self.awg_open,
            view,
            block,
        )
    }

    fn hysteria_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let row = |id: &'static str, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let protocol = self.protocol_text(cx);
        let is_v2 = protocol == "hysteria2";
        let mut block = div().flex().flex_col().gap_2();
        if self.protocol_locked {
            block = block.child(
                field().label("Protocol version".to_string()).child(
                    div()
                        .id("hy-version-locked")
                        .test_support()
                        .child(if is_v2 { "2 (locked)" } else { "1 (locked)" }),
                ),
            );
        } else {
            let picker = self.protocol_select.clone();
            let version_repaint = repaint.clone();
            let selected = Some(if is_v2 { 1 } else { 0 });
            let mut group = RadioGroup::horizontal("hy-version-options")
                .selected_index(selected)
                .on_change(move |ix, window, cx| {
                    let want = if *ix == 1 { "hysteria2" } else { "hysteria" };
                    let next = want.to_string();
                    picker.update(cx, |state, cx| {
                        state.set_selected_value(&next, window, cx);
                    });
                    version_repaint(window, cx);
                });
            for label in ["1", "2"] {
                group = group.child(
                    Radio::new(label)
                        .small()
                        .label(label)
                        .tooltip(format!("Hysteria version {label}")),
                );
            }
            block = block.child(
                field().label("Protocol version".to_string()).child(
                    div()
                        .id("hy-version-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                Tooltip::new(
                                    "Hysteria protocol version (switches layout)".to_string(),
                                )
                            })
                            .into()
                        })
                        .child(div().id("hy-version-group").test_support().child(group)),
                ),
            );
        }
        block = block
            .child(row(
                "hy-server-ports",
                "Server ports (e.g. 9000-9010)",
                &self.hy_server_ports,
            ))
            .child(row(
                "hy-hop-interval",
                "Hop interval (e.g. 30s)",
                &self.hy_hop_interval,
            ))
            .child(row("hy-up", "Up Mbps", &self.hy_up))
            .child(row("hy-down", "Down Mbps", &self.hy_down))
            .child(row("hy-obfs", "Obfs", &self.hy_obfs));
        if is_v2 {
            block = block.child(password_row(
                "Password *",
                "hy-password",
                &self.hy_password,
                cx,
            ));
        } else {
            block = block.child(Self::panel_select_row(
                "Auth type",
                "hy-auth-type-tip",
                "v1 auth marker (blank leaves it unset)",
                "hy-auth-type",
                "Hysteria auth type",
                &self.hy_auth_type,
            ));
            block = block.child(password_row("Auth *", "hy-auth", &self.hy_auth, cx));
            block = block
                .child(row(
                    "hy-recv-window-conn",
                    "Recv window conn",
                    &self.hy_recv_conn,
                ))
                .child(row("hy-recv-window", "Recv window", &self.hy_recv));
            block = block.child(switch_row(
                "hy-mtu",
                "hy-mtu-switch".to_string(),
                "Disable MTU discovery",
                "Disable MTU discovery on v1 (folds disable_mtu_discovery)",
                &self.hy_mtu,
                repaint,
            ));
        }
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("hy-notes").test_support().child(
                "Hysteria v1 rows require up/down (blank refuses naming up_mbps/down_mbps); \
                 v2 omits them for BBR. v1-only keys on a v2 row fail generate naming the field.",
            ),
        ));
        Self::page_disclosure(
            "Hysteria settings",
            "hy-section-caption",
            "hy-section-toggle",
            "hy-section",
            &self.hy_open,
            view,
            block,
        )
    }

    fn tuic_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.tuic_uuid, "tuic", cx));
        block = block.child(password_row(
            "Password *",
            "tuic-password",
            &self.tuic_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            "Congestion control",
            "tuic-cc-tip",
            "TUIC congestion control (blank leaves it unset)",
            "tuic-cc",
            "TUIC congestion control",
            &self.tuic_cc,
        ));
        block = block.child(Self::panel_select_row(
            "UDP relay mode",
            "tuic-relay-tip",
            "TUIC relay mode (blank leaves it unset)",
            "tuic-relay",
            "TUIC UDP relay mode",
            &self.tuic_relay,
        ));
        block = block.child(switch_row(
            "tuic-uot",
            "tuic-uot-switch".to_string(),
            "UDP over stream",
            "UDP over stream (folds udp_over_stream)",
            &self.tuic_uot,
            repaint,
        ));
        block = block.child(switch_row(
            "tuic-zero-rtt",
            "tuic-zero-rtt-switch".to_string(),
            "Zero RTT",
            "Zero-RTT handshake (folds zero_rtt_handshake)",
            &self.tuic_zero_rtt,
            repaint,
        ));
        block = block.child(
            field().label("Heartbeat".to_string()).child(
                div()
                    .id("tuic-heartbeat")
                    .test_support()
                    .child(Input::new(&self.tuic_heartbeat)),
            ),
        );
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("tuic-notes").test_support().child(
                "TUIC runs TLS always-on (security locked tls). Blank congestion/relay stay unset.",
            ),
        ));
        Self::page_disclosure(
            "TUIC settings",
            "tuic-section-caption",
            "tuic-section-toggle",
            "tuic-section",
            &self.tuic_open,
            view,
            block,
        )
    }

    fn socks_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let uot_cell = self.socks_uot.clone();
        let uot_repaint = repaint.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            "Version",
            "socks-version-tip",
            "SOCKS version (blank leaves the stock default 5)",
            "socks-version",
            "SOCKS version",
            &self.socks_version,
        ));
        block = block.child(
            field().label("Username".to_string()).child(
                div()
                    .id("socks-username")
                    .test_support()
                    .child(Input::new(&self.socks_username)),
            ),
        );
        block = block.child(password_row(
            "Password",
            "socks-password",
            &self.socks_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            "Network",
            "socks-network-tip",
            "SOCKS network (tcp is the default)",
            "socks-network",
            "SOCKS network",
            &self.socks_network,
        ));
        block = block.child(
            field().label_indent(false).child(
                div().id("socks-uot").test_support().child(
                    Switch::new("socks-uot-switch")
                        .small()
                        .checked(*self.socks_uot.borrow())
                        .label("UDP over TCP (uot)")
                        .tooltip("UDP over TCP for socks (folds the uot flat)")
                        .accessibility_label("UDP over TCP for socks")
                        .on_change(move |next: &bool, window, cx| {
                            *uot_cell.borrow_mut() = *next;
                            uot_repaint(window, cx);
                        }),
                ),
            ),
        );

        block = block.child(field().label_indent(false).child(
            div().id("socks-notes").test_support().child(
                "SOCKS serves version 5 stock (blank leaves the default); 4 fails generate \
                 naming version. Blank network leaves tcp.",
            ),
        ));
        Self::page_disclosure(
            "SOCKS settings",
            "socks-section-caption",
            "socks-section-toggle",
            "socks-section",
            &self.socks_open,
            view,
            block,
        )
    }

    fn http_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label("Username".to_string()).child(
                    div()
                        .id("http-username")
                        .test_support()
                        .child(Input::new(&self.http_username)),
                ),
            )
            .child(password_row(
                "Password",
                "http-password",
                &self.http_password,
                cx,
            ))
            .child(
                field().label("Path".to_string()).child(
                    div()
                        .id("http-path")
                        .test_support()
                        .child(Input::new(&self.http_path)),
                ),
            )
            .child(
                field().label("Headers".to_string()).child(
                    div()
                        .id("http-headers")
                        .test_support()
                        .child(Input::new(&self.http_headers)),
                ),
            );
        block = block.child(self.s2_tls_section(view, repaint, cx, false));

        block = block.child(field().label_indent(false).child(
            div().id("http-notes").test_support().child(
                "HTTPS rows serve TLS-on (security=tls) even with the panel off. \
                 Path/headers ride the http outbound — check judges the headers shape.",
            ),
        ));
        Self::page_disclosure(
            "HTTP settings",
            "http-section-caption",
            "http-section-toggle",
            "http-section",
            &self.http_open,
            view,
            block,
        )
    }

    fn anytls_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(password_row(
            "Password *",
            "anytls-password",
            &self.anytls_password,
            cx,
        ));
        block = block
            .child(
                field().label("Idle check interval".to_string()).child(
                    div()
                        .id("anytls-interval")
                        .test_support()
                        .child(Input::new(&self.anytls_interval)),
                ),
            )
            .child(
                field().label("Idle timeout".to_string()).child(
                    div()
                        .id("anytls-timeout")
                        .test_support()
                        .child(Input::new(&self.anytls_timeout)),
                ),
            )
            .child(
                field().label("Min idle sessions".to_string()).child(
                    div()
                        .id("anytls-min")
                        .test_support()
                        .child(Input::new(&self.anytls_min)),
                ),
            );
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("anytls-notes").test_support().child(
                "AnyTLS runs TLS always-on (security locked tls). Interval/timeout are \
                 durations (e.g. 30s); min is a session count.",
            ),
        ));
        Self::page_disclosure(
            "AnyTLS settings",
            "anytls-section-caption",
            "anytls-section-toggle",
            "anytls-section",
            &self.anytls_open,
            view,
            block,
        )
    }

    fn naive_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label("Username *".to_string()).child(
                    div()
                        .id("naive-username")
                        .test_support()
                        .child(Input::new(&self.naive_username)),
                ),
            )
            .child(password_row(
                "Password *",
                "naive-password",
                &self.naive_password,
                cx,
            ));
        block = block.child(switch_row(
            "naive-uot",
            "naive-uot-switch".to_string(),
            "UDP over TCP (uot)",
            "UDP over TCP for naive (folds the uot flat)",
            &self.naive_uot,
            repaint,
        ));
        block = block.child(switch_row(
            "naive-quic",
            "naive-quic-switch".to_string(),
            "QUIC",
            "Run naive over QUIC instead of HTTP/2",
            &self.naive_quic,
            repaint,
        ));
        block = block.child(Self::panel_select_row(
            "Congestion control",
            "naive-cc-tip",
            "QUIC congestion control (blank leaves the chromium default cubic)",
            "naive-cc",
            "Naive congestion control",
            &self.naive_cc,
        ));
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("naive-notes").test_support().child(
                "Naïve runs TLS always-on (security locked tls). Blank congestion control \
                 leaves the default (cubic).",
            ),
        ));
        Self::page_disclosure(
            "Naïve settings",
            "naive-section-caption",
            "naive-section-toggle",
            "naive-section",
            &self.naive_open,
            view,
            block,
        )
    }

    fn ssh_block(
        &self,
        view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let browse_view = view.clone();
        let browse_path = self.ssh_key_path.clone();
        let browse_key = self.ssh_private_key.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label("User".to_string()).child(
                    div()
                        .id("ssh-user")
                        .test_support()
                        .child(Input::new(&self.ssh_user)),
                ),
            )
            .child(password_row(
                "Password",
                "ssh-password",
                &self.ssh_password,
                cx,
            ))
            .child(
                field().label("Private key".to_string()).child(
                    div()
                        .id("ssh-private-key")
                        .test_support()
                        .child(Textarea::new(&self.ssh_private_key).h(px(96.))),
                ),
            )
            .child(
                field().label("Private key path".to_string()).child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id("ssh-key-path")
                                .test_support()
                                .child(Input::new(&self.ssh_key_path)),
                        )
                        .child(
                            Button::new("ssh-key-browse")
                                .small()
                                .label("Browse…")
                                .tooltip(
                                    "Pick a private-key file — fills the path and loads the key",
                                )
                                .accessibility_label("Browse for an SSH private-key file")
                                .on_click(move |_, window, cx| {
                                    browse_view.update(cx, |view, cx| {
                                        view.browse_for_ssh_key(
                                            browse_path.clone(),
                                            browse_key.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        ),
                ),
            )
            .child(
                field().label("Private key passphrase".to_string()).child(
                    div()
                        .id("ssh-key-pass")
                        .test_support()
                        .child(Input::new(&self.ssh_key_pass).mask_toggle()),
                ),
            )
            .child(
                field().label("Host key (comma list)".to_string()).child(
                    div()
                        .id("ssh-host-key")
                        .test_support()
                        .child(Input::new(&self.ssh_host_key)),
                ),
            )
            .child(
                field()
                    .label("Host key algorithms (comma list)".to_string())
                    .child(
                        div()
                            .id("ssh-host-key-algs")
                            .test_support()
                            .child(Input::new(&self.ssh_host_key_algs)),
                    ),
            )
            .child(
                field().label("Client version".to_string()).child(
                    div()
                        .id("ssh-client-version")
                        .test_support()
                        .child(Input::new(&self.ssh_client_version)),
                ),
            );

        block = block.child(field().label_indent(false).child(
            div().id("ssh-notes").test_support().child(
                "Browse fills the path and loads the key file (no hand-paste). Host key \
                 lists are comma-separated; cipher/mac/kex stay JSON-only.",
            ),
        ));
        Self::page_disclosure(
            "SSH settings",
            "ssh-section-caption",
            "ssh-section-toggle",
            "ssh-section",
            &self.ssh_open,
            view,
            block,
        )
    }

    fn snell_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let reuse_cell = self.snell_reuse.clone();
        let reuse_repaint = repaint.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(password_row("PSK *", "snell-psk", &self.snell_psk, cx))
            .child(Self::panel_select_row(
                "Version",
                "snell-version-tip",
                "Snell version (blank leaves the engine default 4)",
                "snell-version",
                "Snell version",
                &self.snell_version,
            ))
            .child(
                field().label("Obfs mode".to_string()).child(
                    div()
                        .id("snell-obfs-mode")
                        .test_support()
                        .child(Input::new(&self.snell_obfs_mode)),
                ),
            )
            .child(
                field().label("Obfs host".to_string()).child(
                    div()
                        .id("snell-obfs-host")
                        .test_support()
                        .child(Input::new(&self.snell_obfs_host)),
                ),
            )
            .child(
                field().label("User key".to_string()).child(
                    div()
                        .id("snell-userkey")
                        .test_support()
                        .child(Input::new(&self.snell_userkey)),
                ),
            )
            .child(
                field().label("Network".to_string()).child(
                    div()
                        .id("snell-network")
                        .test_support()
                        .child(Input::new(&self.snell_network)),
                ),
            )
            .child(
                field().label("Mode".to_string()).child(
                    div()
                        .id("snell-mode")
                        .test_support()
                        .child(Input::new(&self.snell_mode)),
                ),
            );
        block = block.child(
            field().label_indent(false).child(
                div().id("snell-reuse").test_support().child(
                    Switch::new("snell-reuse-switch")
                        .small()
                        .checked(*self.snell_reuse.borrow())
                        .label("Reuse")
                        .tooltip("Reuse sessions for snell (folds the reuse flat)")
                        .accessibility_label("Reuse sessions for snell")
                        .on_change(move |next: &bool, window, cx| {
                            *reuse_cell.borrow_mut() = *next;
                            reuse_repaint(window, cx);
                        }),
                ),
            ),
        );

        block = block.child(field().label_indent(false).child(
            div().id("snell-notes").test_support().child(
                "Snell serves version 4 when blank; 6 needs a v6 peer. \
                 Obfs/userkey/reuse/network/mode ride the snell outbound — check judges their shapes.",
            ),
        ));
        Self::page_disclosure(
            "Snell settings",
            "snell-section-caption",
            "snell-section-toggle",
            "snell-section",
            &self.snell_open,
            view,
            block,
        )
    }

    fn shadowtls_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(Self::panel_select_row(
                "Version",
                "shadowtls-version-tip",
                "ShadowTLS version (blank takes the schema default 1; v1 rows omit the password)",
                "shadowtls-version",
                "ShadowTLS version",
                &self.shadowtls_version,
            ))
            .child(password_row(
                "Password",
                "shadowtls-password",
                &self.shadowtls_password,
                cx,
            ));
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("shadowtls-notes").test_support().child(
                "ShadowTLS runs TLS always-on (security locked tls). Version blank takes \
                 the schema default 1; v1 rows omit the password.",
            ),
        ));
        Self::page_disclosure(
            "ShadowTLS settings",
            "shadowtls-section-caption",
            "shadowtls-section-toggle",
            "shadowtls-section",
            &self.shadowtls_open,
            view,
            block,
        )
    }

    fn xray_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        _cx: &App,
    ) -> impl IntoElement {
        let _ = repaint;
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.xray_uuid, "xray", _cx));
        block = block
            .child(
                field().label("Encryption".to_string()).child(
                    div()
                        .id("xray-enc")
                        .test_support()
                        .child(Input::new(&self.xray_enc)),
                ),
            )
            .child(Self::panel_select_row(
                "Flow",
                "xray-flow-tip",
                "Xray flow (blank leaves the default)",
                "xray-flow",
                "Xray flow",
                &self.xray_flow,
            ))
            .child(
                field().label("SNI".to_string()).child(
                    div()
                        .id("xray-sni")
                        .test_support()
                        .child(Input::new(&self.xray_sni)),
                ),
            )
            .child(Self::panel_select_row(
                "Fingerprint",
                "xray-fp-tip",
                "Xray fingerprint (blank leaves the default)",
                "xray-fp",
                "Xray fingerprint",
                &self.xray_fp,
            ))
            .child(
                field().label("ALPN (comma list)".to_string()).child(
                    div()
                        .id("xray-alpn")
                        .test_support()
                        .child(Input::new(&self.xray_alpn)),
                ),
            )
            .child(
                field().label("REALITY public key (pbk)".to_string()).child(
                    div()
                        .id("xray-pbk")
                        .test_support()
                        .child(Input::new(&self.xray_pbk)),
                ),
            )
            .child(
                field().label("REALITY short id (sid)".to_string()).child(
                    div()
                        .id("xray-sid")
                        .test_support()
                        .child(Input::new(&self.xray_sid)),
                ),
            )
            .child(
                field().label("REALITY spiderX path".to_string()).child(
                    div()
                        .id("xray-spiderx")
                        .test_support()
                        .child(Input::new(&self.xray_spiderx)),
                ),
            )
            .child(Self::panel_select_row(
                "Network",
                "xray-network-tip",
                "Xray stream network (raw|xhttp|ws|httpupgrade|grpc)",
                "xray-network",
                "Xray network",
                &self.xray_network,
            ))
            .child(
                field().label("Mode".to_string()).child(
                    div()
                        .id("xray-mode")
                        .test_support()
                        .child(Input::new(&self.xray_mode)),
                ),
            )
            .child(Self::panel_select_row(
                "Mux",
                "xray-mux-tip",
                "Xray mux (Keep default leaves the sidecar default)",
                "xray-mux",
                "Xray mux",
                &self.xray_mux,
            ));
        block = block.child(field().label_indent(false).child(
            div().id("xray-notes").test_support().child(
                "Ph1 subset only — XHTTP/xmux/session keys (xPaddingBytes, maxConcurrency, \
                 downloadSettings…) stay JSON-only and are preserved, never stripped.",
            ),
        ));
        Self::page_disclosure(
            "Xray settings",
            "xray-section-caption",
            "xray-section-toggle",
            "xray-section",
            &self.xray_open,
            view,
            block,
        )
    }

    fn chain_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        _cx: &App,
    ) -> impl IntoElement {
        let hops = self.chain_hops.borrow().clone();
        let library = self.chain_library.borrow().clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            "Hop from library",
            "chain-pick-tip",
            "Add or replace hops from your row list (chain and off-index-0 extracore hops are rejected)",
            "chain-pick",
            "Chain hop from library",
            &self.chain_pick,
        ));
        {
            let add_view = view.clone();
            let add_hops = self.chain_hops.clone();
            let add_library = self.chain_library.clone();
            let add_pick = self.chain_pick.clone();
            let add_repaint = repaint.clone();
            block = block.child(
                field().label_indent(false).child(
                    div().id("chain-hop-add-row").test_support().child(
                        Button::new("chain-hop-add")
                            .small()
                            .label("Add hop")
                            .tooltip("Append the picked row to the chain")
                            .accessibility_label("Add the picked row to the chain")
                            .on_click(move |_, window, cx| {
                                let picked = add_pick.read(cx).selected_value().cloned();
                                let picked = picked.as_deref().unwrap_or("").to_string();
                                add_view.update(cx, |view, cx| {
                                    view.chain_add_hop(
                                        &add_hops,
                                        &add_library,
                                        &add_pick,
                                        &picked,
                                        window,
                                        cx,
                                    );
                                });
                                add_repaint(window, cx);
                            }),
                    ),
                ),
            );
        }
        if hops.is_empty() {
            block = block.child(
                div().id("chain-hops-empty").test_support().child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .title(EmptyTitle::new().child("No hops yet"))
                            .description(EmptyDescription::new().child(
                                "Add hops from the library — the first hop may be an extra-core row.",
                            )),
                    ),
                ),
            );
        } else {
            let mut list = div()
                .flex()
                .flex_col()
                .gap_1()
                .id("chain-hops-list")
                .test_support();
            for (index, node) in hops.iter().enumerate() {
                let label = chain_hop_label(node, index, &library);
                let replace_view = view.clone();
                let replace_hops = self.chain_hops.clone();
                let replace_library = self.chain_library.clone();
                let replace_pick = self.chain_pick.clone();
                let replace_repaint = repaint.clone();
                let remove_hops = self.chain_hops.clone();
                let remove_repaint = repaint.clone();
                list = list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id(format!("chain-hop-{index}"))
                                .test_support()
                                .child(Label::new(label)),
                        )
                        .child(
                            Button::new(format!("chain-hop-replace-{index}"))
                                .small()
                                .label("Replace")
                                .tooltip("Replace this hop with the picked row")
                                .accessibility_label(format!("Replace hop {}", index + 1))
                                .on_click(move |_, window, cx| {
                                    let picked = replace_pick.read(cx).selected_value().cloned();
                                    let picked = picked.as_deref().unwrap_or("").to_string();
                                    replace_view.update(cx, |view, cx| {
                                        view.chain_replace_hop(
                                            &replace_hops,
                                            &replace_library,
                                            &replace_pick,
                                            index,
                                            &picked,
                                            window,
                                            cx,
                                        );
                                    });
                                    replace_repaint(window, cx);
                                }),
                        )
                        .child(
                            Button::new(format!("chain-hop-remove-{index}"))
                                .small()
                                .label("Remove")
                                .tooltip("Remove this hop from the chain")
                                .accessibility_label(format!("Remove hop {}", index + 1))
                                .on_click(move |_, window, cx| {
                                    let mut hops = remove_hops.borrow_mut();
                                    if index < hops.len() {
                                        hops.remove(index);
                                    }
                                    drop(hops);
                                    remove_repaint(window, cx);
                                }),
                        ),
                );
            }
            block = block.child(list);
        }
        block = block.child(field().label_indent(false).child(
            div().id("chain-notes").test_support().child(
                "V1 chains are flat: no chain hops; at most one extra-core hop, first only. \
                 Extra/Custom/Xray hops must be last — the engine names violations at start.",
            ),
        ));
        Self::page_disclosure(
            "Chain settings",
            "chain-section-caption",
            "chain-section-toggle",
            "chain-section",
            &self.chain_open,
            view,
            block,
        )
    }

    fn extra_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let _ = (repaint, cx);
        let browse_view = view.clone();
        let browse_path = self.extra_exe.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field()
                    .label("Executable (app dir file name)".to_string())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .id("extra-exe")
                                    .test_support()
                                    .child(Input::new(&self.extra_exe)),
                            )
                            .child(
                                Button::new("extra-exe-browse")
                                    .small()
                                    .label("Browse…")
                                    .tooltip("Pick the sidecar executable — fills the path")
                                    .accessibility_label("Browse for the extra-core executable")
                                    .on_click(move |_, window, cx| {
                                        browse_view.update(cx, |view, cx| {
                                            view.browse_for_extra_exe(
                                                browse_path.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                            ),
                    ),
            )
            .child(
                field().label("Args".to_string()).child(
                    div()
                        .id("extra-args")
                        .test_support()
                        .child(Input::new(&self.extra_args)),
                ),
            )
            .child(
                field()
                    .label("Config (passed as -c <file>)".to_string())
                    .child(
                        div()
                            .id("extra-config")
                            .test_support()
                            .child(Textarea::new(&self.extra_config).h(px(96.))),
                    ),
            );
        block = block.child(field().label_indent(false).child(
            div().id("extra-notes").test_support().child(
                "Spawned unprivileged as <exe> -c <config-file> (v1 convention — extra flags \
                 ride args with the row). Validate checks exe + config shape before commit; \
                 there is no logs toggle by design.",
            ),
        ));
        Self::page_disclosure(
            "Extra-core settings",
            "extra-section-caption",
            "extra-section-toggle",
            "extra-section",
            &self.extra_open,
            view,
            block,
        )
    }

    fn vless_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();

        block = block.child(self.uuid_row(view, &self.vless_uuid, "vless", cx));
        let flow_select = self.vless_flow.clone();
        let packet_select = self.vless_packet_encoding.clone();
        block = block.child(
            field().label("Flow".to_string()).child(
                div()
                    .id("vless-flow-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Tooltip::new("VLESS flow (vision disables mux)".to_string()))
                            .into()
                    })
                    .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                    })
                    .child(
                        Select::new(&flow_select)
                            .id("vless-flow")
                            .small()
                            .accessibility_label("VLESS flow"),
                    ),
            ),
        );
        block = block.child(
            field().label("Packet encoding".to_string()).child(
                div()
                    .id("vless-packet-encoding-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Tooltip::new("VLESS packet encoding".to_string()))
                            .into()
                    })
                    .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                    })
                    .child(
                        Select::new(&packet_select)
                            .id("vless-packet-encoding")
                            .small()
                            .accessibility_label("VLESS packet encoding"),
                    ),
            ),
        );

        block = block.child(self.s1_network_section(view, cx));

        block = block.child(self.s2_tls_section(view, repaint, cx, false));

        let flow = Self::panel_value(&self.vless_flow, cx, "");
        block = block.child(self.s3_section(view, repaint, cx, flow == "xtls-rprx-vision"));

        Self::page_disclosure(
            "VLESS settings",
            "vless-section-caption",
            "vless-section-toggle",
            "vless-section",
            &self.vless_open,
            view,
            block,
        )
    }

    fn element(
        self,
        view: Entity<AppView>,
        cx: &App,
        collision: Option<String>,
        form_error: Option<(String, String)>,
        show_test_url: bool,
        port_budget: Option<String>,
    ) -> impl IntoElement {
        let bind = self.bind.read(cx).value().to_string();
        let protocol_text = self.protocol_text(cx);
        let guided = guided_identity_fields(&protocol_text);

        let virtual_row = is_virtual_protocol(&protocol_text);
        let format_view = view.clone();
        let format_params = self.params.clone();
        let template_view = view.clone();
        let template_params = self.params.clone();
        let template_form = self.clone();
        let validate_view = view.clone();
        let validate_params = self.params.clone();
        let validate_form = self.clone();
        let expanded = *self.advanced.borrow();

        let toggle_form = self.clone();

        let guided_panels = if expanded {
            None
        } else {
            let repaint_view = view.clone();
            let repaint: DialogRepaint = Rc::new(move |_window, cx| {
                repaint_view.update(cx, |_, cx| cx.notify());
            });
            match protocol_text.as_str() {
                "vless" => Some(self.vless_block(&view, &repaint, cx).into_any_element()),
                "vmess" => Some(self.vmess_block(&view, &repaint, cx).into_any_element()),
                "trojan" => Some(self.trojan_block(&view, &repaint, cx).into_any_element()),
                "shadowsocks" => Some(
                    self.shadowsocks_block(&view, &repaint, cx)
                        .into_any_element(),
                ),
                "wireguard" => Some(self.wireguard_block(&view, &repaint, cx).into_any_element()),
                "amneziawg" => Some(self.amneziawg_block(&view, &repaint, cx).into_any_element()),
                "hysteria" | "hysteria2" => {
                    Some(self.hysteria_block(&view, &repaint, cx).into_any_element())
                }
                "tuic" => Some(self.tuic_block(&view, &repaint, cx).into_any_element()),

                "socks" => Some(self.socks_block(&view, &repaint, cx).into_any_element()),
                "http" | "https" => Some(self.http_block(&view, &repaint, cx).into_any_element()),
                "anytls" => Some(self.anytls_block(&view, &repaint, cx).into_any_element()),
                "naive" => Some(self.naive_block(&view, &repaint, cx).into_any_element()),
                "ssh" => Some(self.ssh_block(&view, &repaint, cx).into_any_element()),

                "snell" => Some(self.snell_block(&view, &repaint, cx).into_any_element()),
                "shadowtls" => Some(self.shadowtls_block(&view, &repaint, cx).into_any_element()),
                "xray" => Some(self.xray_block(&view, &repaint, cx).into_any_element()),
                "chain" => Some(self.chain_block(&view, &repaint, cx).into_any_element()),
                "extra-core" => Some(self.extra_block(&view, &repaint, cx).into_any_element()),
                _ => None,
            }
        };

        let params_row = div()
            .id("field-params")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(Label::new("Params")))
                    .child(advanced_toggle_button(
                        "Back to guided",
                        &view,
                        &toggle_form,
                    )),
            )
            .child(Textarea::new(&self.params).h(px(96.)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Button::new("params-format").label("Format JSON").on_click(
                        move |_, window, cx| {
                            format_view.update(cx, |view, cx| {
                                view.format_dialog_params(&format_params, window, cx);
                            });
                        },
                    ))
                    .child(
                        Button::new("params-template")
                            .label("Load protocol template")
                            .on_click(move |_, window, cx| {
                                template_view.update(cx, |view, cx| {
                                    view.template_dialog_params(
                                        &template_params,
                                        &template_form,
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(Button::new("params-validate").label("Validate").on_click(
                        move |_, window, cx| {
                            validate_view.update(cx, |view, cx| {
                                let protocol = validate_form.protocol_text(cx);
                                view.validate_dialog_params(
                                    &validate_params,
                                    &protocol,
                                    window,
                                    cx,
                                );
                            });
                        },
                    )),
            );

        let mut form = v_form().small();

        let mut common_head = div().flex().flex_row().items_center().gap_2().child(
            div()
                .flex_1()
                .id("common-caption")
                .test_support()
                .child(Label::new("Common")),
        );
        if has_advanced_dialog(&protocol_text) {
            common_head = common_head.child(advanced_settings_button(&view, &toggle_form));
        }
        let mut common = div().flex().flex_col().gap_1().child(common_head).child(
            field().label("Name *".to_string()).child(
                div()
                    .id("field-name-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| {
                            Tooltip::new(
                                "Display name for this row (cards and toasts name it)".to_string(),
                            )
                        })
                        .into()
                    })
                    .child(
                        div()
                            .id("field-name")
                            .test_support()
                            .child(Input::new(&self.name)),
                    ),
            ),
        );

        if self.protocol_locked {
            let locked = self.locked_protocol.clone();
            common = common.child(
                field().label("Protocol".to_string()).child(
                    div()
                        .id("protocol-lock")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new("Type is locked after creation".to_string()))
                                .into()
                        })
                        .child(Label::new(format!("{locked} (locked)"))),
                ),
            );
        } else {
            let select = self.protocol_select.clone();
            common = common.child(
                field().label("Protocol".to_string()).child(
                    div()
                        .id("protocol-select-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new("Protocol (locked after creation)".to_string()))
                                .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&select)
                                .id("protocol-select")
                                .small()
                                .accessibility_label("Protocol"),
                        ),
                ),
            );
        }

        let has_page = guided_panels.is_some();
        let camouflage = if has_page && has_network_panel(&protocol_text) {
            self.s1_camouflage_section(cx)
        } else {
            None
        };
        let show_right =
            (has_page && tls_panel_plan(&protocol_text).is_some()) || camouflage.is_some();
        if expanded {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("common-section").test_support().child(common)),
            );
            form = form.child(
                field()
                    .label_indent(false)
                    .child(Collapsible::new().open(expanded).content(params_row)),
            );
        } else {
            if !virtual_row {
                common = common.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().child(
                                field().label("Server *").child(
                                    div()
                                        .id("field-guided-server-tip")
                                        .test_support()
                                        .tooltip(move |_, cx| {
                                            cx.new(|_| {
                                                Tooltip::new(
                                                    "Upstream server address (IP or hostname)"
                                                        .to_string(),
                                                )
                                            })
                                            .into()
                                        })
                                        .child(
                                            div()
                                                .id("field-guided-server")
                                                .test_support()
                                                .child(Input::new(&self.guided_server)),
                                        ),
                                ),
                            ),
                        )
                        .child(
                            div().flex_1().min_w_0().child(
                                field().label("Server port *").child(
                                    div()
                                        .id("field-guided-port-tip")
                                        .test_support()
                                        .tooltip(move |_, cx| {
                                            cx.new(|_| {
                                                Tooltip::new("Upstream server port".to_string())
                                            })
                                            .into()
                                        })
                                        .child(
                                            div()
                                                .id("field-guided-port")
                                                .test_support()
                                                .child(Input::new(&self.guided_port)),
                                        ),
                                ),
                            ),
                        ),
                );
            }

            if guided.is_some_and(|guided| guided.key.is_some())
                && !block_owns_identity(&protocol_text)
            {
                let identity_presence =
                    secret_state_caption(self.guided_identity.read(cx).value().as_ref())
                        .to_string();
                common = common.child(
                    field()
                        .label(format!("{} *", guided_identity_label(&protocol_text)))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .id("field-guided-identity")
                                        .test_support()
                                        .child(Input::new(&self.guided_identity).mask_toggle()),
                                )
                                .child(
                                    div()
                                        .id("field-guided-identity-state")
                                        .test_support()
                                        .text_sm()
                                        .child(identity_presence),
                                ),
                        ),
                );
            }
            let mut hint_row = div().flex().flex_row().items_center().gap_2();
            if let Some(hint) = guided_hint_line(&protocol_text) {
                hint_row =
                    hint_row.child(div().flex_1().id("guided-hint").test_support().child(hint));
            } else {
                hint_row = hint_row.child(div().flex_1());
            }
            common = common.child(hint_row.child(advanced_toggle_button(
                "Advanced (raw JSON)",
                &view,
                &toggle_form,
            )));
            let mut left_col = div().flex().flex_col().gap_2().flex_1().min_w_0().child(
                field()
                    .label_indent(false)
                    .child(div().id("common-section").test_support().child(common)),
            );
            if let Some(panels) = guided_panels {
                left_col = left_col.child(field().label_indent(false).child(panels));
            }
            if show_right {
                let mut right_col = div().flex().flex_col().gap_2().flex_1().min_w_0();
                if has_page && tls_panel_plan(&protocol_text).is_some() {
                    right_col = right_col.child(self.s2_security_section(cx));
                }
                if let Some(camouflage) = camouflage {
                    right_col = right_col.child(camouflage);
                }
                form = form.child(
                    field().label_indent(false).child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(left_col)
                            .child(right_col),
                    ),
                );
            } else {
                form = form.child(field().label_indent(false).child(left_col));
            }
        }
        form = form.child(dialog_field("field-group", "Group", self.group));
        form = form.child(
            field().label_indent(false).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-bind",
                        "Bind address",
                        self.bind,
                    )))
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-port",
                        "Port",
                        self.port,
                    ))),
            ),
        );
        if let Some(budget) = port_budget {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("port-budget")
                        .test_support()
                        .text_sm()
                        .child(budget),
                ),
            );
        }

        if show_test_url {
            form = form.child(dialog_field(
                "field-test-url",
                "Test URL (blank = global)",
                self.test_url,
            ));
        }
        if let Some(hint) = lan_hint_for_bind(&bind) {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("lan-hint").test_support().child(hint)),
            );
        }
        if let Some(warning) = collision {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("bind-collision-warning")
                        .test_support()
                        .child(warning),
                ),
            );
        }
        if let Some((field_name, message)) = form_error {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("form-field-error")
                        .test_support()
                        .child(format!("{field_name}: {message}")),
                ),
            );
        }
        form
    }

    fn focus_field(&self, field: &str, window: &mut Window, cx: &mut App) {
        if field == "protocol" {
            if !self.protocol_locked {
                self.protocol_select.update(cx, |state, cx| {
                    state.focus(window, cx);
                });
            }
            return;
        }

        let target: Option<AnyInputState> = match field {
            "name" => Some(AnyInputState::from(self.name.clone())),
            "params" => Some(AnyInputState::from(self.params.clone())),
            "group" => Some(AnyInputState::from(self.group.clone())),
            "bind_ip" => Some(AnyInputState::from(self.bind.clone())),
            "port" => Some(AnyInputState::from(self.port.clone())),
            "test_url" => Some(AnyInputState::from(self.test_url.clone())),
            _ => None,
        };
        if let Some(state) = target {
            let handle = state.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    fn as_new(&self, cx: &App) -> NewConfig {
        NewConfig {
            name: self.name.read(cx).value().to_string(),
            protocol: self.protocol_text(cx),
            params: self.effective_params(cx),
            group: self.group.read(cx).value().to_string(),
            bind_ip: self.bind.read(cx).value().to_string(),
            port: self.port.read(cx).value().to_string(),
        }
    }

    fn effective_params(&self, cx: &App) -> String {
        let params = self.params.read(cx).value().to_string();
        let protocol = self.protocol_text(cx);
        let identity = guided_identity_fields(&protocol)
            .and_then(|guided| guided.key.map(|key| (key, guided.read_keys)));
        let params = fold_guided_into_params(
            &params,
            self.guided_server.read(cx).value().as_ref(),
            self.guided_port.read(cx).value().as_ref(),
            identity,
            self.guided_identity.read(cx).value().as_ref(),
        );
        match protocol.as_str() {
            "vless" => {
                let (uuid, flow, packet_encoding) = self.vless_inner_values(cx);
                let params = fold_vless_inner_into_params(&params, &uuid, &flow, &packet_encoding);
                let params = fold_transport_into_params(&params, &self.transport_fold(cx));
                let params = fold_tls_optional_into_params(&params, &self.tls_optional_fold(cx));
                let params = if flow == "xtls-rprx-vision" {
                    params
                } else {
                    fold_mux_into_params(&params, &self.mux_fold(cx))
                };
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "vmess" => {
                let (uuid, aid, cipher, packet_encoding) = self.vmess_inner_values(cx);
                let params =
                    fold_vmess_inner_into_params(&params, &uuid, &aid, &cipher, &packet_encoding);
                let params = fold_transport_into_params(&params, &self.transport_fold(cx));
                let params = fold_tls_vmess_into_params(&params, &self.tls_optional_fold(cx));
                let params = fold_mux_into_params(&params, &self.mux_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "trojan" => {
                let password = self.trojan_password.read(cx).value().to_string();
                let params = fold_trojan_inner_into_params(&params, &password);
                let params = fold_transport_into_params(&params, &self.transport_fold(cx));
                let params = fold_tls_optional_into_params(&params, &self.tls_optional_fold(cx));
                let params = fold_mux_into_params(&params, &self.mux_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "shadowsocks" => {
                let (method, password, plugin, plugin_opts, uot) = self.ss_inner_values(cx);
                let params = fold_shadowsocks_inner_into_params(
                    &params,
                    &method,
                    &password,
                    &plugin,
                    &plugin_opts,
                    uot,
                );
                let params = fold_mux_into_params(&params, &self.mux_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "wireguard" => {
                let params =
                    fold_wireguard_into_params(&params, &self.wireguard_fold(cx, "wireguard"));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "amneziawg" => {
                let params =
                    fold_wireguard_into_params(&params, &self.wireguard_fold(cx, "amneziawg"));
                let params = fold_amneziawg_into_params(&params, &self.amnezia_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "hysteria" | "hysteria2" => {
                let params = fold_hysteria_into_params(
                    &params,
                    &self.hysteria_fold(cx),
                    protocol == "hysteria2",
                );
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "tuic" => {
                let params = fold_tuic_into_params(&params, &self.tuic_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }

            "socks" => {
                let params = fold_socks_into_params(&params, &self.socks_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "http" | "https" => {
                let params = fold_http_into_params(&params, &self.http_fold(cx));
                let params = fold_tls_optional_into_params(&params, &self.tls_optional_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "anytls" => {
                let params = fold_anytls_into_params(&params, &self.anytls_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "naive" => {
                let params = fold_naive_into_params(&params, &self.naive_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "ssh" => {
                let params = fold_ssh_into_params(&params, &self.ssh_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }

            "snell" => {
                let params = fold_snell_into_params(&params, &self.snell_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "shadowtls" => {
                let params = fold_shadowtls_into_params(&params, &self.shadowtls_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "xray" => fold_xray_into_params(&params, &self.xray_fold(cx)),
            "chain" => fold_chain_nodes_into_params(&params, &self.chain_hops.borrow()),
            "extra-core" => fold_extra_into_params(&params, &self.extra_fold(cx)),
            _ => params,
        }
    }

    fn panel_value(
        select: &Entity<SelectState<PanelChoiceList>>,
        cx: &App,
        default: &str,
    ) -> String {
        select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    fn vless_inner_values(&self, cx: &App) -> (String, String, String) {
        (
            self.vless_uuid.read(cx).value().to_string(),
            Self::panel_value(&self.vless_flow, cx, ""),
            Self::panel_value(&self.vless_packet_encoding, cx, ""),
        )
    }

    fn vmess_inner_values(&self, cx: &App) -> (String, String, String, String) {
        (
            self.vmess_uuid.read(cx).value().to_string(),
            self.vmess_aid.read(cx).value().to_string(),
            Self::panel_value(&self.vmess_security, cx, "auto"),
            Self::panel_value(&self.vmess_packet_encoding, cx, ""),
        )
    }

    fn ss_inner_values(&self, cx: &App) -> (String, String, String, String, bool) {
        (
            Self::panel_value(&self.ss_method, cx, ""),
            self.ss_password.read(cx).value().to_string(),
            Self::panel_value(&self.ss_plugin, cx, ""),
            self.ss_plugin_opts.read(cx).value().to_string(),
            *self.ss_uot.borrow(),
        )
    }

    fn wireguard_fold(&self, cx: &App, page: &str) -> WireguardFold {
        let awg = page == "amneziawg";
        WireguardFold {
            private_key: if awg {
                self.awg_private.read(cx).value().to_string()
            } else {
                self.wg_private.read(cx).value().to_string()
            },
            public_key: if awg {
                self.awg_public.read(cx).value().to_string()
            } else {
                self.wg_public.read(cx).value().to_string()
            },
            pre_shared_key: if awg {
                self.awg_preshared.read(cx).value().to_string()
            } else {
                self.wg_preshared.read(cx).value().to_string()
            },
            reserved: if awg {
                self.awg_reserved.read(cx).value().to_string()
            } else {
                self.wg_reserved.read(cx).value().to_string()
            },
            persistent_keepalive: if awg {
                self.awg_keepalive.read(cx).value().to_string()
            } else {
                self.wg_keepalive.read(cx).value().to_string()
            },
            local_addr: if awg {
                self.awg_local_addr.read(cx).value().to_string()
            } else {
                self.wg_local_addr.read(cx).value().to_string()
            },
            dns: if awg {
                self.awg_dns.read(cx).value().to_string()
            } else {
                self.wg_dns.read(cx).value().to_string()
            },
            mtu: if awg {
                self.awg_mtu.read(cx).value().to_string()
            } else {
                self.wg_mtu.read(cx).value().to_string()
            },
            workers: if awg {
                self.awg_workers.read(cx).value().to_string()
            } else {
                self.wg_workers.read(cx).value().to_string()
            },
            sys_ifc: if awg {
                *self.awg_sys_ifc.borrow()
            } else {
                *self.wg_sys_ifc.borrow()
            },
        }
    }

    fn amnezia_fold(&self, cx: &App) -> AmneziaFold {
        AmneziaFold {
            enable_amnezia: *self.awg_enable.borrow(),
            version: Self::panel_value(&self.awg_version, cx, ""),
            jc: self.awg_jc.read(cx).value().to_string(),
            jmin: self.awg_jmin.read(cx).value().to_string(),
            jmax: self.awg_jmax.read(cx).value().to_string(),
            s1: self.awg_s1.read(cx).value().to_string(),
            s2: self.awg_s2.read(cx).value().to_string(),
            s3: self.awg_s3.read(cx).value().to_string(),
            s4: self.awg_s4.read(cx).value().to_string(),
            h1: self.awg_h1.read(cx).value().to_string(),
            h2: self.awg_h2.read(cx).value().to_string(),
            h3: self.awg_h3.read(cx).value().to_string(),
            h4: self.awg_h4.read(cx).value().to_string(),
            i1: self.awg_i1.read(cx).value().to_string(),
            i2: self.awg_i2.read(cx).value().to_string(),
            i3: self.awg_i3.read(cx).value().to_string(),
            i4: self.awg_i4.read(cx).value().to_string(),
            i5: self.awg_i5.read(cx).value().to_string(),
            header_protection_key: self.awg_hpk.read(cx).value().to_string(),
            content_padding_addition: self.awg_pad.read(cx).value().to_string(),
            rekey_after_time: self.awg_rekey_after.read(cx).value().to_string(),
            rekey_timeout: self.awg_rekey_timeout.read(cx).value().to_string(),
            reject_after_time: self.awg_reject_after.read(cx).value().to_string(),
            keepalive_timeout: self.awg_keepalive_timeout.read(cx).value().to_string(),
            max_handshake_attempts: self.awg_max_handshake.read(cx).value().to_string(),
            random_trailers: *self.awg_trailers.borrow(),
            disable_cookies: *self.awg_cookies.borrow(),
        }
    }

    fn hysteria_fold(&self, cx: &App) -> HysteriaFold {
        HysteriaFold {
            server_ports: self.hy_server_ports.read(cx).value().to_string(),
            hop_interval: self.hy_hop_interval.read(cx).value().to_string(),
            up: self.hy_up.read(cx).value().to_string(),
            down: self.hy_down.read(cx).value().to_string(),
            obfs: self.hy_obfs.read(cx).value().to_string(),
            auth_type: Self::panel_value(&self.hy_auth_type, cx, ""),
            auth: self.hy_auth.read(cx).value().to_string(),
            recv_window_conn: self.hy_recv_conn.read(cx).value().to_string(),
            recv_window: self.hy_recv.read(cx).value().to_string(),
            disable_mtu_discovery: *self.hy_mtu.borrow(),
            password: self.hy_password.read(cx).value().to_string(),
        }
    }

    fn tuic_fold(&self, cx: &App) -> TuicFold {
        TuicFold {
            uuid: self.tuic_uuid.read(cx).value().to_string(),
            password: self.tuic_password.read(cx).value().to_string(),
            congestion_control: Self::panel_value(&self.tuic_cc, cx, ""),
            udp_relay_mode: Self::panel_value(&self.tuic_relay, cx, ""),
            udp_over_stream: *self.tuic_uot.borrow(),
            zero_rtt_handshake: *self.tuic_zero_rtt.borrow(),
            heartbeat: self.tuic_heartbeat.read(cx).value().to_string(),
        }
    }

    fn socks_fold(&self, cx: &App) -> SocksFold {
        SocksFold {
            version: Self::panel_value(&self.socks_version, cx, ""),
            username: self.socks_username.read(cx).value().to_string(),
            password: self.socks_password.read(cx).value().to_string(),
            network: Self::panel_value(&self.socks_network, cx, "tcp"),
            udp_over_tcp: *self.socks_uot.borrow(),
        }
    }

    fn http_fold(&self, cx: &App) -> HttpFold {
        HttpFold {
            username: self.http_username.read(cx).value().to_string(),
            password: self.http_password.read(cx).value().to_string(),
            path: self.http_path.read(cx).value().to_string(),
            headers: self.http_headers.read(cx).value().to_string(),
        }
    }

    fn anytls_fold(&self, cx: &App) -> AnytlsFold {
        AnytlsFold {
            password: self.anytls_password.read(cx).value().to_string(),
            interval: self.anytls_interval.read(cx).value().to_string(),
            timeout: self.anytls_timeout.read(cx).value().to_string(),
            min: self.anytls_min.read(cx).value().to_string(),
        }
    }

    fn naive_fold(&self, cx: &App) -> NaiveFold {
        NaiveFold {
            username: self.naive_username.read(cx).value().to_string(),
            password: self.naive_password.read(cx).value().to_string(),
            udp_over_tcp: *self.naive_uot.borrow(),
            quic: *self.naive_quic.borrow(),
            congestion_control: Self::panel_value(&self.naive_cc, cx, ""),
        }
    }

    fn ssh_fold(&self, cx: &App) -> SshFold {
        SshFold {
            user: self.ssh_user.read(cx).value().to_string(),
            password: self.ssh_password.read(cx).value().to_string(),
            private_key: self.ssh_private_key.read(cx).value().to_string(),
            private_key_path: self.ssh_key_path.read(cx).value().to_string(),
            private_key_passphrase: self.ssh_key_pass.read(cx).value().to_string(),
            host_key: self.ssh_host_key.read(cx).value().to_string(),
            host_key_algorithms: self.ssh_host_key_algs.read(cx).value().to_string(),
            client_version: self.ssh_client_version.read(cx).value().to_string(),
        }
    }

    fn snell_fold(&self, cx: &App) -> SnellFold {
        SnellFold {
            psk: self.snell_psk.read(cx).value().to_string(),
            version: Self::panel_value(&self.snell_version, cx, ""),
            obfs_mode: self.snell_obfs_mode.read(cx).value().to_string(),
            obfs_host: self.snell_obfs_host.read(cx).value().to_string(),
            userkey: self.snell_userkey.read(cx).value().to_string(),
            reuse: *self.snell_reuse.borrow(),
            network: self.snell_network.read(cx).value().to_string(),
            mode: self.snell_mode.read(cx).value().to_string(),
        }
    }

    fn shadowtls_fold(&self, cx: &App) -> ShadowTlsFold {
        ShadowTlsFold {
            version: Self::panel_value(&self.shadowtls_version, cx, ""),
            password: self.shadowtls_password.read(cx).value().to_string(),
        }
    }

    fn xray_fold(&self, cx: &App) -> XrayFold {
        XrayFold {
            uuid: self.xray_uuid.read(cx).value().to_string(),
            encryption: self.xray_enc.read(cx).value().to_string(),
            flow: Self::panel_value(&self.xray_flow, cx, ""),
            sni: self.xray_sni.read(cx).value().to_string(),
            fp: Self::panel_value(&self.xray_fp, cx, ""),
            alpn: self.xray_alpn.read(cx).value().to_string(),
            pbk: self.xray_pbk.read(cx).value().to_string(),
            sid: self.xray_sid.read(cx).value().to_string(),
            spiderx: self.xray_spiderx.read(cx).value().to_string(),
            network: Self::panel_value(&self.xray_network, cx, ""),
            mode: self.xray_mode.read(cx).value().to_string(),
            mux: Self::panel_value(&self.xray_mux, cx, ""),
        }
    }

    fn extra_fold(&self, cx: &App) -> ExtraFold {
        ExtraFold {
            exe: self.extra_exe.read(cx).value().to_string(),
            args: self.extra_args.read(cx).value().to_string(),
            config: self.extra_config.read(cx).value().to_string(),
        }
    }

    fn tls_forced_fold(&self, cx: &App) -> TlsForcedFold {
        TlsForcedFold {
            sni: self.s2_sni.read(cx).value().to_string(),
            alpn: self.s2_alpn.read(cx).value().to_string(),
            fp: Self::panel_value(&self.s2_fp, cx, ""),
            insecure: *self.s2_insecure.borrow(),
            pbk: self.s2_pbk.read(cx).value().to_string(),
            sid: self.s2_sid.read(cx).value().to_string(),
        }
    }

    fn transport_fold(&self, cx: &App) -> TransportFold {
        TransportFold {
            network: Self::panel_value(&self.s1_network, cx, "tcp"),
            path: self.s1_path.read(cx).value().to_string(),
            host: self.s1_host.read(cx).value().to_string(),
            method: self.s1_method.read(cx).value().to_string(),
            service_name: self.s1_service_name.read(cx).value().to_string(),
        }
    }

    fn tls_optional_fold(&self, cx: &App) -> TlsOptionalFold {
        TlsOptionalFold {
            enable: *self.s2_enable.borrow(),
            sni: self.s2_sni.read(cx).value().to_string(),
            alpn: self.s2_alpn.read(cx).value().to_string(),
            fp: Self::panel_value(&self.s2_fp, cx, ""),
            insecure: *self.s2_insecure.borrow(),
            pbk: self.s2_pbk.read(cx).value().to_string(),
            sid: self.s2_sid.read(cx).value().to_string(),
        }
    }

    fn s3_effective_mode(&self) -> String {
        if *self.s3_brutal.borrow() {
            "on".to_string()
        } else {
            self.s3_mode.borrow().clone()
        }
    }

    fn mux_fold(&self, cx: &App) -> MuxFold {
        MuxFold {
            mode: self.s3_effective_mode(),
            brutal_enable: *self.s3_brutal.borrow(),
            brutal_up_mbps: self.s3_up.read(cx).value().to_string(),
            brutal_down_mbps: self.s3_down.read(cx).value().to_string(),
        }
    }

    fn dial_fold(&self, cx: &App) -> DialFold {
        DialFold {
            reuse_addr: *self.s4_reuse.borrow(),
            tcp_fast_open: *self.s4_fastopen.borrow(),
            udp_fragment: *self.s4_fragment.borrow(),
            tcp_multi_path: *self.s4_multipath.borrow(),
            connect_timeout: self.s4_timeout.read(cx).value().to_string(),
            bind_interface: self.s4_bind.read(cx).value().to_string(),
            inet4_bind_address: self.s4_inet4.read(cx).value().to_string(),
            inet6_bind_address: self.s4_inet6.read(cx).value().to_string(),
        }
    }

    fn expand_merge(&self, window: &mut Window, cx: &mut Context<AppView>) {
        let box_text = self.params.read(cx).value().to_string();
        let protocol = self.protocol_text(cx);
        let identity = guided_identity_fields(&protocol)
            .and_then(|guided| guided.key.map(|key| (key, guided.read_keys)));
        let mut merged = fold_guided_into_params(
            &box_text,
            self.guided_server.read(cx).value().as_ref(),
            self.guided_port.read(cx).value().as_ref(),
            identity,
            self.guided_identity.read(cx).value().as_ref(),
        );

        let page = matches!(
            protocol.as_str(),
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
                | "xray"
                | "chain"
                | "extra-core"
        );
        if page {
            match protocol.as_str() {
                "vless" => {
                    let (uuid, flow, packet_encoding) = self.vless_inner_values(cx);
                    merged = fold_vless_inner_into_params(&merged, &uuid, &flow, &packet_encoding);
                    merged = fold_transport_into_params(&merged, &self.transport_fold(cx));
                    merged = fold_tls_optional_into_params(&merged, &self.tls_optional_fold(cx));
                    if flow != "xtls-rprx-vision" {
                        merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    }
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "vmess" => {
                    let (uuid, aid, cipher, packet_encoding) = self.vmess_inner_values(cx);
                    merged = fold_vmess_inner_into_params(
                        &merged,
                        &uuid,
                        &aid,
                        &cipher,
                        &packet_encoding,
                    );
                    merged = fold_transport_into_params(&merged, &self.transport_fold(cx));
                    merged = fold_tls_vmess_into_params(&merged, &self.tls_optional_fold(cx));
                    merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "trojan" => {
                    let password = self.trojan_password.read(cx).value().to_string();
                    merged = fold_trojan_inner_into_params(&merged, &password);
                    merged = fold_transport_into_params(&merged, &self.transport_fold(cx));
                    merged = fold_tls_optional_into_params(&merged, &self.tls_optional_fold(cx));
                    merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "shadowsocks" => {
                    let (method, password, plugin, plugin_opts, uot) = self.ss_inner_values(cx);
                    merged = fold_shadowsocks_inner_into_params(
                        &merged,
                        &method,
                        &password,
                        &plugin,
                        &plugin_opts,
                        uot,
                    );
                    merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "wireguard" => {
                    merged =
                        fold_wireguard_into_params(&merged, &self.wireguard_fold(cx, "wireguard"));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "amneziawg" => {
                    merged =
                        fold_wireguard_into_params(&merged, &self.wireguard_fold(cx, "amneziawg"));
                    merged = fold_amneziawg_into_params(&merged, &self.amnezia_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "hysteria" | "hysteria2" => {
                    merged = fold_hysteria_into_params(
                        &merged,
                        &self.hysteria_fold(cx),
                        protocol == "hysteria2",
                    );
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "tuic" => {
                    merged = fold_tuic_into_params(&merged, &self.tuic_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }

                "socks" => {
                    merged = fold_socks_into_params(&merged, &self.socks_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "http" | "https" => {
                    merged = fold_http_into_params(&merged, &self.http_fold(cx));
                    merged = fold_tls_optional_into_params(&merged, &self.tls_optional_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "anytls" => {
                    merged = fold_anytls_into_params(&merged, &self.anytls_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "naive" => {
                    merged = fold_naive_into_params(&merged, &self.naive_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "ssh" => {
                    merged = fold_ssh_into_params(&merged, &self.ssh_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }

                "snell" => {
                    merged = fold_snell_into_params(&merged, &self.snell_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "shadowtls" => {
                    merged = fold_shadowtls_into_params(&merged, &self.shadowtls_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "xray" => {
                    merged = fold_xray_into_params(&merged, &self.xray_fold(cx));
                }
                "chain" => {
                    merged = fold_chain_nodes_into_params(&merged, &self.chain_hops.borrow());
                }
                "extra-core" => {
                    merged = fold_extra_into_params(&merged, &self.extra_fold(cx));
                }
                _ => {}
            }
        }
        let parseable = merged.trim().is_empty()
            || serde_json::from_str::<serde_json::Value>(&merged)
                .ok()
                .is_some_and(|value| value.is_object());
        if merged != box_text {
            let next = merged.clone();
            self.params.update(cx, |state, cx| {
                state.set_value(next, window, cx);
            });
        }
        if parseable {
            for entity in [
                &self.guided_server,
                &self.guided_port,
                &self.guided_identity,
            ] {
                entity.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            }
            if page {
                self.clear_panels(window, cx, &protocol);
            }
        }
    }

    fn clear_panels(&self, window: &mut Window, cx: &mut Context<AppView>, protocol: &str) {
        let mut texts: Vec<&Entity<InputState>> = vec![
            &self.s4_timeout,
            &self.s4_bind,
            &self.s4_inet4,
            &self.s4_inet6,
        ];
        let mut selects: Vec<(&Entity<SelectState<PanelChoiceList>>, &'static str)> = vec![];
        let mut cells: Vec<&Rc<RefCell<bool>>> = vec![
            &self.s4_reuse,
            &self.s4_fastopen,
            &self.s4_fragment,
            &self.s4_multipath,
        ];
        let mux_page = matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks");
        if mux_page {
            texts.extend([&self.s3_up, &self.s3_down]);
            cells.push(&self.s3_brutal);
        }
        if protocol != "shadowsocks" {
            let forced_tls = matches!(
                protocol,
                "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls"
            );
            let inner_only = matches!(
                protocol,
                "socks" | "ssh" | "snell" | "xray" | "chain" | "extra-core"
            );
            let no_transport = matches!(protocol, "http" | "https");
            if !forced_tls && !inner_only && !no_transport {
                texts.extend([
                    &self.s1_path,
                    &self.s1_host,
                    &self.s1_method,
                    &self.s1_service_name,
                ]);
                selects.extend([(&self.s1_network, "tcp")]);
                cells.extend([&self.s2_enable]);
            }
            if !inner_only {
                if no_transport {
                    cells.extend([&self.s2_enable]);
                }
                texts.extend([&self.s2_sni, &self.s2_alpn, &self.s2_pbk, &self.s2_sid]);
                selects.extend([(&self.s2_fp, "")]);
                cells.extend([&self.s2_insecure]);
            }
        }
        match protocol {
            "vless" => {
                texts.push(&self.vless_uuid);
                selects.extend([(&self.vless_flow, ""), (&self.vless_packet_encoding, "")]);
            }
            "vmess" => {
                texts.extend([&self.vmess_uuid, &self.vmess_aid]);
                selects.extend([
                    (&self.vmess_security, "auto"),
                    (&self.vmess_packet_encoding, ""),
                ]);
            }
            "trojan" => {
                texts.push(&self.trojan_password);
            }
            "shadowsocks" => {
                texts.extend([&self.ss_password, &self.ss_plugin_opts]);
                selects.extend([(&self.ss_method, ""), (&self.ss_plugin, "")]);
                cells.push(&self.ss_uot);
            }
            "hysteria" | "hysteria2" => {
                texts.extend([
                    &self.hy_server_ports,
                    &self.hy_hop_interval,
                    &self.hy_up,
                    &self.hy_down,
                    &self.hy_obfs,
                    &self.hy_auth,
                    &self.hy_recv_conn,
                    &self.hy_recv,
                    &self.hy_password,
                ]);
                selects.extend([(&self.hy_auth_type, "")]);
                cells.push(&self.hy_mtu);
            }
            "tuic" => {
                texts.extend([&self.tuic_uuid, &self.tuic_password, &self.tuic_heartbeat]);
                selects.extend([(&self.tuic_cc, ""), (&self.tuic_relay, "")]);
                cells.extend([&self.tuic_uot, &self.tuic_zero_rtt]);
            }

            "socks" => {
                texts.extend([&self.socks_username, &self.socks_password]);
                selects.extend([(&self.socks_version, ""), (&self.socks_network, "tcp")]);
                cells.push(&self.socks_uot);
            }
            "http" | "https" => {
                texts.extend([
                    &self.http_username,
                    &self.http_password,
                    &self.http_path,
                    &self.http_headers,
                ]);
            }
            "anytls" => {
                texts.extend([
                    &self.anytls_password,
                    &self.anytls_interval,
                    &self.anytls_timeout,
                    &self.anytls_min,
                ]);
            }
            "naive" => {
                texts.extend([&self.naive_username, &self.naive_password]);
                selects.extend([(&self.naive_cc, "")]);
                cells.extend([&self.naive_uot, &self.naive_quic]);
            }
            "ssh" => {
                texts.extend([
                    &self.ssh_user,
                    &self.ssh_password,
                    &self.ssh_key_path,
                    &self.ssh_key_pass,
                    &self.ssh_host_key,
                    &self.ssh_host_key_algs,
                    &self.ssh_client_version,
                ]);

                self.ssh_private_key.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            }

            "snell" => {
                texts.extend([
                    &self.snell_psk,
                    &self.snell_obfs_mode,
                    &self.snell_obfs_host,
                    &self.snell_userkey,
                    &self.snell_network,
                    &self.snell_mode,
                ]);
                selects.extend([(&self.snell_version, "")]);
                cells.push(&self.snell_reuse);
            }
            "shadowtls" => {
                texts.push(&self.shadowtls_password);
                selects.extend([(&self.shadowtls_version, "")]);
            }
            "xray" => {
                texts.extend([
                    &self.xray_uuid,
                    &self.xray_enc,
                    &self.xray_sni,
                    &self.xray_alpn,
                    &self.xray_pbk,
                    &self.xray_sid,
                    &self.xray_spiderx,
                    &self.xray_mode,
                ]);
                selects.extend([
                    (&self.xray_flow, ""),
                    (&self.xray_fp, ""),
                    (&self.xray_network, ""),
                    (&self.xray_mux, ""),
                ]);
            }
            "chain" => {}
            "extra-core" => {
                texts.extend([&self.extra_exe, &self.extra_args]);

                self.extra_config.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            }
            "wireguard" => {
                texts.extend([
                    &self.wg_private,
                    &self.wg_public,
                    &self.wg_preshared,
                    &self.wg_reserved,
                    &self.wg_keepalive,
                    &self.wg_local_addr,
                    &self.wg_dns,
                    &self.wg_mtu,
                    &self.wg_workers,
                ]);
                cells.push(&self.wg_sys_ifc);
            }
            "amneziawg" => {
                texts.extend([
                    &self.awg_private,
                    &self.awg_public,
                    &self.awg_preshared,
                    &self.awg_reserved,
                    &self.awg_keepalive,
                    &self.awg_local_addr,
                    &self.awg_dns,
                    &self.awg_mtu,
                    &self.awg_workers,
                    &self.awg_jc,
                    &self.awg_jmin,
                    &self.awg_jmax,
                    &self.awg_s1,
                    &self.awg_s2,
                    &self.awg_s3,
                    &self.awg_s4,
                    &self.awg_h1,
                    &self.awg_h2,
                    &self.awg_h3,
                    &self.awg_h4,
                    &self.awg_i1,
                    &self.awg_i2,
                    &self.awg_i3,
                    &self.awg_i4,
                    &self.awg_i5,
                    &self.awg_hpk,
                    &self.awg_pad,
                    &self.awg_rekey_after,
                    &self.awg_rekey_timeout,
                    &self.awg_reject_after,
                    &self.awg_keepalive_timeout,
                    &self.awg_max_handshake,
                ]);
                selects.extend([(&self.awg_version, "")]);
                cells.extend([
                    &self.awg_sys_ifc,
                    &self.awg_enable,
                    &self.awg_trailers,
                    &self.awg_cookies,
                ]);
            }
            _ => {}
        }
        for entity in texts {
            entity.update(cx, |state, cx| {
                state.set_value(String::new(), window, cx);
            });
        }
        for (entity, default) in selects {
            let next = default.to_string();
            entity.update(cx, |state, cx| {
                state.set_selected_value(&next, window, cx);
            });
        }
        for cell in cells {
            *cell.borrow_mut() = false;
        }
        if mux_page {
            *self.s3_mode.borrow_mut() = String::new();
        }
    }

    fn collapse_echo(&self, window: &mut Window, cx: &mut Context<AppView>) {
        let box_text = self.params.read(cx).value().to_string();
        let protocol = self.protocol_text(cx);
        let read_keys: &[&str] = guided_identity_fields(&protocol)
            .map(|guided| guided.read_keys)
            .unwrap_or(&[]);
        for (entity, value) in [
            (
                &self.guided_server,
                guided_prefill_value(&box_text, &["server", "address"]),
            ),
            (
                &self.guided_port,
                guided_prefill_value(&box_text, &["server_port", "port"]),
            ),
            (
                &self.guided_identity,
                guided_prefill_value(&box_text, read_keys),
            ),
        ] {
            let blank = entity.read(cx).value().to_string().trim().is_empty();
            if blank && !value.is_empty() {
                let next = value.clone();
                entity.update(cx, |state, cx| {
                    state.set_value(next, window, cx);
                });
            }
        }
        if matches!(
            protocol.as_str(),
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
                | "xray"
                | "chain"
                | "extra-core"
        ) {
            self.echo_panels(window, cx, &box_text, &protocol);
        }
    }

    fn echo_panels(
        &self,
        window: &mut Window,
        cx: &mut Context<AppView>,
        box_text: &str,
        protocol: &str,
    ) {
        let full_panels = matches!(protocol, "vless" | "vmess" | "trojan");
        let mux_page = matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks");
        let mut shared_texts: Vec<(&Entity<InputState>, &[&str])> = vec![
            (&self.s4_timeout, &["connect_timeout"][..]),
            (&self.s4_bind, &["bind_interface"][..]),
            (&self.s4_inet4, &["inet4_bind_address"][..]),
            (&self.s4_inet6, &["inet6_bind_address"][..]),
        ];

        let forced_tls = matches!(
            protocol,
            "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls"
        );
        let optional_tls = matches!(protocol, "http" | "https");
        if full_panels {
            shared_texts.extend([
                (&self.s1_path, &["path"][..]),
                (&self.s1_host, &["host"][..]),
                (&self.s1_method, &["method"][..]),
                (&self.s1_service_name, &["service_name", "serviceName"][..]),
            ]);
        }
        if full_panels || forced_tls || optional_tls {
            shared_texts.extend([
                (&self.s2_sni, &["sni", "peer"][..]),
                (&self.s2_alpn, &["alpn"][..]),
                (&self.s2_pbk, &["pbk"][..]),
                (&self.s2_sid, &["sid"][..]),
            ]);
        }
        if mux_page {
            shared_texts.extend([
                (&self.s3_up, &["brutal_up_mbps", "brutal_up"][..]),
                (&self.s3_down, &["brutal_down_mbps", "brutal_down"][..]),
            ]);
        }
        for (entity, keys) in shared_texts {
            let value = guided_prefill_value(box_text, keys);
            let blank = entity.read(cx).value().to_string().trim().is_empty();
            if blank && !value.is_empty() {
                entity.update(cx, |state, cx| {
                    state.set_value(value.clone(), window, cx);
                });
            }
        }

        let inner_texts: &[(&Entity<InputState>, &[&str])] = match protocol {
            "vless" => &[(&self.vless_uuid, &["uuid", "id"][..])],
            "vmess" => &[
                (&self.vmess_uuid, &["uuid", "id"][..]),
                (&self.vmess_aid, &["alter_id", "aid"][..]),
            ],
            "trojan" => &[(&self.trojan_password, &["password"][..])],
            "shadowsocks" => &[
                (&self.ss_password, &["password"][..]),
                (
                    &self.ss_plugin_opts,
                    &["plugin_opts", "plugin-opts", "pluginOpts"][..],
                ),
            ],

            "hysteria" | "hysteria2" => &[
                (&self.hy_server_ports, &["server_ports"][..]),
                (&self.hy_hop_interval, &["hop_interval"][..]),
                (&self.hy_up, &["up", "up_mbps", "upmbps"][..]),
                (&self.hy_down, &["down", "down_mbps", "downmbps"][..]),
                (&self.hy_obfs, &["obfs"][..]),
                (&self.hy_auth, &["auth"][..]),
                (&self.hy_recv_conn, &["recv_window_conn"][..]),
                (&self.hy_recv, &["recv_window"][..]),
                (&self.hy_password, &["password"][..]),
            ],
            "tuic" => &[
                (&self.tuic_uuid, &["uuid", "id"][..]),
                (&self.tuic_password, &["password"][..]),
                (&self.tuic_heartbeat, &["heartbeat"][..]),
            ],

            "socks" => &[
                (&self.socks_username, &["username"][..]),
                (&self.socks_password, &["password"][..]),
            ],
            "http" | "https" => &[
                (&self.http_username, &["username"][..]),
                (&self.http_password, &["password"][..]),
                (&self.http_path, &["path"][..]),
                (&self.http_headers, &["headers"][..]),
            ],
            "anytls" => &[
                (&self.anytls_password, &["password"][..]),
                (
                    &self.anytls_interval,
                    &["idle_session_check_interval", "interval"][..],
                ),
                (
                    &self.anytls_timeout,
                    &["idle_session_timeout", "timeout"][..],
                ),
                (&self.anytls_min, &["min_idle_session", "min"][..]),
            ],
            "naive" => &[
                (&self.naive_username, &["username"][..]),
                (&self.naive_password, &["password"][..]),
            ],
            "ssh" => &[
                (&self.ssh_user, &["user", "username"][..]),
                (&self.ssh_password, &["password"][..]),
                (&self.ssh_key_path, &["private_key_path"][..]),
                (
                    &self.ssh_key_pass,
                    &["private_key_passphrase", "private_key_pass"][..],
                ),
                (&self.ssh_client_version, &["client_version"][..]),
            ],

            "snell" => &[
                (&self.snell_psk, &["psk"][..]),
                (&self.snell_obfs_mode, &["obfs_mode", "obfs"][..]),
                (&self.snell_obfs_host, &["obfs_host", "obfs-host"][..]),
                (&self.snell_userkey, &["userkey"][..]),
                (&self.snell_network, &["network"][..]),
                (&self.snell_mode, &["mode"][..]),
            ],
            "shadowtls" => &[(&self.shadowtls_password, &["password"][..])],
            "xray" => &[
                (&self.xray_uuid, &["uuid", "id"][..]),
                (&self.xray_enc, &["encryption"][..]),
                (&self.xray_sni, &["sni"][..]),
                (&self.xray_alpn, &["alpn"][..]),
                (&self.xray_pbk, &["pbk"][..]),
                (&self.xray_sid, &["sid"][..]),
                (&self.xray_spiderx, &["spiderx"][..]),
                (&self.xray_mode, &["mode"][..]),
            ],
            "extra-core" => &[
                (&self.extra_exe, &["exe", "binary"][..]),
                (&self.extra_args, &["args"][..]),
            ],

            "wireguard" => &[
                (&self.wg_private, &["private_key", "privatekey"][..]),
                (&self.wg_public, &["public_key", "publickey"][..]),
                (
                    &self.wg_preshared,
                    &["pre_shared_key", "presharedkey", "preshared_key"][..],
                ),
                (&self.wg_reserved, &["reserved"][..]),
                (
                    &self.wg_keepalive,
                    &["persistent_keepalive", "persistent_keepalive_interval"][..],
                ),
                (
                    &self.wg_local_addr,
                    &["address", "local_address", "local_addr"][..],
                ),
                (&self.wg_dns, &["dns"][..]),
                (&self.wg_mtu, &["mtu"][..]),
                (&self.wg_workers, &["workers"][..]),
            ],
            "amneziawg" => &[
                (&self.awg_private, &["private_key", "privatekey"][..]),
                (&self.awg_public, &["public_key", "publickey"][..]),
                (
                    &self.awg_preshared,
                    &["pre_shared_key", "presharedkey", "preshared_key"][..],
                ),
                (&self.awg_reserved, &["reserved"][..]),
                (
                    &self.awg_keepalive,
                    &["persistent_keepalive", "persistent_keepalive_interval"][..],
                ),
                (
                    &self.awg_local_addr,
                    &["address", "local_address", "local_addr"][..],
                ),
                (&self.awg_dns, &["dns"][..]),
                (&self.awg_mtu, &["mtu"][..]),
                (&self.awg_workers, &["workers"][..]),
                (&self.awg_jc, &["jc"][..]),
                (&self.awg_jmin, &["jmin"][..]),
                (&self.awg_jmax, &["jmax"][..]),
                (&self.awg_s1, &["s1"][..]),
                (&self.awg_s2, &["s2"][..]),
                (&self.awg_s3, &["s3"][..]),
                (&self.awg_s4, &["s4"][..]),
                (&self.awg_h1, &["h1"][..]),
                (&self.awg_h2, &["h2"][..]),
                (&self.awg_h3, &["h3"][..]),
                (&self.awg_h4, &["h4"][..]),
                (&self.awg_i1, &["i1"][..]),
                (&self.awg_i2, &["i2"][..]),
                (&self.awg_i3, &["i3"][..]),
                (&self.awg_i4, &["i4"][..]),
                (&self.awg_i5, &["i5"][..]),
                (&self.awg_hpk, &["headerprotectionkey"][..]),
                (&self.awg_pad, &["contentpaddingaddition"][..]),
                (&self.awg_rekey_after, &["rekeyaftertime"][..]),
                (&self.awg_rekey_timeout, &["rekeytimeout"][..]),
                (&self.awg_reject_after, &["rejectaftertime"][..]),
                (&self.awg_keepalive_timeout, &["keepalivetimeout"][..]),
                (&self.awg_max_handshake, &["maxhandshakeattempts"][..]),
            ],
            _ => &[],
        };
        for (entity, keys) in inner_texts {
            let value = guided_prefill_value(box_text, keys);
            let blank = entity.read(cx).value().to_string().trim().is_empty();
            if blank && !value.is_empty() {
                entity.update(cx, |state, cx| {
                    state.set_value(value.clone(), window, cx);
                });
            }
        }

        if protocol == "ssh" {
            for (entity, keys) in [
                (&self.ssh_host_key, &["host_key"][..]),
                (&self.ssh_host_key_algs, &["host_key_algorithms"][..]),
            ] {
                let value = guided_prefill_list(box_text, keys);
                let blank = entity.read(cx).value().to_string().trim().is_empty();
                if blank && !value.is_empty() {
                    entity.update(cx, |state, cx| {
                        state.set_value(value.clone(), window, cx);
                    });
                }
            }
            let key_value = guided_prefill_value(box_text, &["private_key", "privatekey"]);
            let key_blank = self
                .ssh_private_key
                .read(cx)
                .value()
                .to_string()
                .trim()
                .is_empty();
            if key_blank && !key_value.is_empty() {
                self.ssh_private_key.update(cx, |state, cx| {
                    state.set_value(key_value.clone(), window, cx);
                });
            }
        }

        if protocol == "extra-core" {
            let config_value = extra_config_prefill(box_text);
            let config_blank = self
                .extra_config
                .read(cx)
                .value()
                .to_string()
                .trim()
                .is_empty();
            if config_blank && !config_value.is_empty() {
                self.extra_config.update(cx, |state, cx| {
                    state.set_value(config_value.clone(), window, cx);
                });
            }
        }

        if protocol == "chain" {
            *self.chain_hops.borrow_mut() = chain_nodes_of(box_text);
        }

        if protocol == "socks" {
            let value = guided_prefill_value(box_text, &["network"]);
            let current = self
                .socks_network
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == "tcp" && !value.is_empty() && value != "tcp" {
                self.socks_network.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }

        let panel_selects: &[EchoSelect] = if full_panels {
            &[
                (&self.s1_network, &["network"][..], "tcp"),
                (&self.s2_fp, &["fp", "fingerprint"][..], ""),
            ]
        } else if forced_tls || optional_tls {
            &[(&self.s2_fp, &["fp", "fingerprint"][..], "")]
        } else {
            &[]
        };
        for (entity, keys, default) in panel_selects {
            let value = guided_prefill_value(box_text, keys);
            let current = entity
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == *default && !value.is_empty() && value != *default {
                entity.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }

        let inner_selects: &[EchoSelect] = match protocol {
            "vless" => &[
                (&self.vless_flow, &["flow"][..], ""),
                (
                    &self.vless_packet_encoding,
                    &["packet_encoding", "packetEncoding"][..],
                    "",
                ),
            ],
            "vmess" => &[(
                &self.vmess_packet_encoding,
                &["packet_encoding", "packetEncoding"][..],
                "",
            )],
            "shadowsocks" => &[
                (&self.ss_method, &["method"][..], ""),
                (&self.ss_plugin, &["plugin"][..], ""),
            ],

            "amneziawg" => &[(&self.awg_version, &["awg_version", "version"][..], "")],

            "hysteria" | "hysteria2" => &[(&self.hy_auth_type, &["auth_type"][..], "")],
            "tuic" => &[
                (&self.tuic_cc, &["congestion_control"][..], ""),
                (&self.tuic_relay, &["udp_relay_mode"][..], ""),
            ],
            "socks" => &[(&self.socks_version, &["version"][..], "")],
            "naive" => &[(
                &self.naive_cc,
                &["quic_congestion_control", "congestion_control"][..],
                "",
            )],

            "snell" => &[(&self.snell_version, &["version"][..], "")],
            "shadowtls" => &[(&self.shadowtls_version, &["version"][..], "")],
            "xray" => &[
                (&self.xray_flow, &["flow"][..], ""),
                (&self.xray_fp, &["fp", "fingerprint"][..], ""),
                (&self.xray_network, &["network"][..], ""),
                (&self.xray_mux, &["xray_mux"][..], ""),
            ],
            _ => &[],
        };
        for (entity, keys, default) in inner_selects {
            let value = guided_prefill_value(box_text, keys);
            let current = entity
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == *default && !value.is_empty() && value != *default {
                entity.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }

        if protocol == "vmess" {
            let value = vmess_cipher_value(box_text);
            let current = self
                .vmess_security
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == "auto" && !value.is_empty() && value != "auto" {
                self.vmess_security.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }
        let mut shared_cells: Vec<(&Rc<RefCell<bool>>, &[&str])> = vec![
            (&self.s4_reuse, &["reuse_addr"][..]),
            (&self.s4_fastopen, &["tcp_fast_open"][..]),
            (&self.s4_fragment, &["udp_fragment"][..]),
            (&self.s4_multipath, &["tcp_multi_path"][..]),
        ];

        if full_panels || forced_tls || optional_tls {
            shared_cells.push((
                &self.s2_insecure,
                &[
                    "insecure",
                    "allowInsecure",
                    "allow_insecure",
                    "allowinsecure",
                ][..],
            ));
        }
        if mux_page {
            shared_cells.push((&self.s3_brutal, &["brutal_enable"][..]));
        }
        for (cell, keys) in shared_cells {
            if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                *cell.borrow_mut() = true;
            }
        }

        if (full_panels || optional_tls) && !*self.s2_enable.borrow() {
            let on = if protocol == "vmess" {
                vmess_tls_enabled(box_text)
            } else {
                let security = guided_prefill_value(box_text, &["security"]);
                matches!(
                    security.to_ascii_lowercase().as_str(),
                    "tls" | "xtls" | "reality"
                )
            };
            if on {
                *self.s2_enable.borrow_mut() = true;
            }
        }
        if protocol == "shadowsocks"
            && !*self.ss_uot.borrow()
            && guided_prefill_bool(box_text, &["udp_over_tcp", "uot"])
        {
            *self.ss_uot.borrow_mut() = true;
        }

        if protocol == "socks"
            && !*self.socks_uot.borrow()
            && guided_prefill_bool(box_text, &["udp_over_tcp", "uot"])
        {
            *self.socks_uot.borrow_mut() = true;
        }
        if protocol == "naive" {
            for (cell, keys) in [
                (&self.naive_uot, &["udp_over_tcp", "uot"][..]),
                (&self.naive_quic, &["quic"][..]),
            ] {
                if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                    *cell.borrow_mut() = true;
                }
            }
        }

        if matches!(protocol, "hysteria" | "hysteria2")
            && !*self.hy_mtu.borrow()
            && guided_prefill_bool(box_text, &["disable_mtu_discovery"])
        {
            *self.hy_mtu.borrow_mut() = true;
        }
        if protocol == "tuic" {
            for (cell, keys) in [
                (&self.tuic_uot, &["udp_over_stream"][..]),
                (&self.tuic_zero_rtt, &["zero_rtt_handshake", "zero_rtt"][..]),
            ] {
                if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                    *cell.borrow_mut() = true;
                }
            }
        }

        if protocol == "wireguard"
            && !*self.wg_sys_ifc.borrow()
            && guided_prefill_bool(box_text, &["system", "sys_ifc"])
        {
            *self.wg_sys_ifc.borrow_mut() = true;
        }
        if protocol == "amneziawg" {
            for (cell, keys) in [
                (&self.awg_sys_ifc, &["system", "sys_ifc"][..]),
                (&self.awg_enable, &["enable_amnezia"][..]),
                (
                    &self.awg_trailers,
                    &["randomtrailers", "random_trailers"][..],
                ),
                (
                    &self.awg_cookies,
                    &["disablecookies", "disable_cookies"][..],
                ),
            ] {
                if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                    *cell.borrow_mut() = true;
                }
            }
        }
        if mux_page && self.s3_mode.borrow().is_empty() {
            let mode = guided_prefill_value(box_text, &["mux"]);
            if !mode.is_empty() {
                *self.s3_mode.borrow_mut() = mode;
            }
        }
    }

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

    fn element(self, view: Entity<AppView>, cx: &App) -> impl IntoElement {
        let text = self.text.read(cx).value().to_string();

        let preview = preview_batch_for_form(
            &text,
            self.file_hint.clone(),
            Some(self.original_text.clone()),
            self.multi_files.clone(),
        );
        let (valid, invalid) = (preview.configs.len(), preview.failures.len());
        let first_failure = preview.failures.first().map(|failure| {
            format!(
                "item {} ({}): {}",
                failure.index + 1,
                failure.label,
                failure.reason
            )
        });

        let mut form = v_form()
            .small()

            .child(
                field().label_indent(false).child(
                    div()
                        .id("import-guide")
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Paste links — nothing is added until you press Import. vless · vmess · trojan · ss · hysteria · tuic · wg · socks · http · anytls · naive · clash · conf files. e.g. vless://uuid@host:443#name. Drops land anywhere on the window.",
                        ),
                ),
            )
            .child(
                field().label("Links, JSON, or conf").child(
                    div()
                        .id("field-import-text")
                        .test_support()
                        .child(Textarea::new(&self.text).h(px(160.))),
                ),
            )
            .child(dialog_field(
                "field-import-group",
                "Group",
                self.group.clone(),
            ));

        {
            let browse_view = view.clone();
            let browse_text = self.text.clone();
            let browse_group = self.group.clone();
            form = form.child(
                field().label_indent(false).child(
                    Button::new("import-browse")
                        .label("Browse files…")
                        .on_click(move |_, window, cx| {
                            let current_text = browse_text.read(cx).value().to_string();
                            let current_group = browse_group.read(cx).value().to_string();
                            browse_view.update(cx, |view, cx| {
                                view.browse_for_import_files(
                                    current_text,
                                    current_group,
                                    window,
                                    cx,
                                );
                            });
                        }),
                ),
            );
        }
        form = form.child(
            field().label_indent(false).child(
                div()
                    .id("import-preview")
                    .test_support()
                    .child(import_preview_message(valid, invalid)),
            ),
        );

        if let Some(reason) = first_failure {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("import-first-failure")
                        .test_support()
                        .child(format!("first problem: {reason}")),
                ),
            );
        }
        if let Some(files) =
            import_files_label(self.file_hint.as_deref(), self.multi_files.as_deref())
        {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("import-files").test_support().child(files)),
            );
        }
        form
    }
}

fn redact_subscription_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);

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

    truncate(&stripped, 96)
}

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

#[cfg(test)]
fn import_preview_counts(text: &str, file_hint: Option<&str>) -> (usize, usize) {
    if text.trim().is_empty() {
        return (0, 0);
    }
    let batch = match effective_hint_for_text(file_hint, text) {
        Some(name) => import_file_content(&name, text),
        None => import_text(text),
    };
    (batch.configs.len(), batch.failures.len())
}

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

fn parse_import_inputs(resolved: &ResolvedImport) -> crate::import::BatchResult {
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

fn preview_batch_for_form(
    text: &str,
    file_hint: Option<String>,
    original: Option<String>,
    multi: Option<Vec<(String, String)>>,
) -> crate::import::BatchResult {
    if text.trim().is_empty() {
        return crate::import::BatchResult::default();
    }
    let resolved =
        resolve_import_inputs(text.to_string(), String::new(), file_hint, original, multi);
    parse_import_inputs(&resolved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServingTone {
    Failed,
    Serving,
    Stopped,
}

/// Theme color for a probe's LatencyTone: Throne §F.1 bands map green /
/// yellow / red with the 100/300 boundaries, a failed probe reads a darker
/// red than a plain poor latency, and an untested/no-sample row keeps the
/// neutral muted tone. Colors only — no probe, classification, or cutoff
/// change rides along.
fn health_tone_color(tone: LatencyTone, cx: &App) -> Hsla {
    match tone {
        LatencyTone::Fast => cx.theme().success,
        LatencyTone::Slow => cx.theme().warning,
        LatencyTone::Poor => cx.theme().danger,
        LatencyTone::Failed => cx.theme().danger.blend(hsla(0., 0., 0., 0.35)),
        LatencyTone::Unknown => cx.theme().muted_foreground,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointUrl {
    Socks,
    Http,
    Upstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowPhase {
    Stopped,
    Starting,
    Running,
    Failed,
}

impl RowPhase {
    fn of(is_running: bool, starting: bool, failed: bool) -> Self {
        if starting {
            Self::Starting
        } else if failed {
            Self::Failed
        } else if is_running {
            Self::Running
        } else {
            Self::Stopped
        }
    }

    fn start_label(self) -> &'static str {
        match self {
            Self::Starting => "Starting…",
            Self::Stopped | Self::Running | Self::Failed => "Start",
        }
    }
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

enum ImportFileRead {
    Text(String),
    Unreadable,
    Refused(&'static str),
}

fn read_import_file(path: &std::path::Path) -> ImportFileRead {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return ImportFileRead::Unreadable,
    };
    if !import_file_size_ok(bytes.len() as u64) {
        return ImportFileRead::Refused("over 50MB");
    }
    if let Some(reason) = reject_file_bytes(&bytes) {
        return ImportFileRead::Refused(reason);
    }
    match String::from_utf8(bytes) {
        Ok(content) => ImportFileRead::Text(content),
        Err(_) => ImportFileRead::Unreadable,
    }
}

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

#[cfg(test)]
fn library_row_lines(
    row: &ProxyConfig,
    health: Option<&str>,
    is_running: bool,
    traffic: Option<(u64, u64)>,
) -> Vec<String> {
    let upstream = upstream_host_port(row).unwrap_or_else(|| "no upstream".to_string());
    let state = if row.enabled { "enabled" } else { "disabled" };
    let mut lines = vec![
        row.name.clone(),
        format!("{} · {upstream}", row.protocol),
        format!(
            "{} · bind {}:{} · {state}",
            row.group, row.bind_ip, row.port
        ),
        serving_badge(is_running, &row.bind_ip, row.port, health),
    ];
    if let Some((rx, tx)) = traffic {
        if let Some(summary) = traffic_summary_text(rx, tx) {
            lines.push(summary);
        }
    }
    lines
}

fn library_row_cells(
    row: &ProxyConfig,
    upstream: &str,
    health: Option<&str>,
    is_running: bool,
    traffic: Option<(u64, u64)>,
    show_security: bool,
) -> [String; 4] {
    let mut upstream_cell = upstream.to_string();
    if show_security {
        apply_show_security(&mut upstream_cell, row);
    }
    let health_cell = if is_running {
        serving_badge(true, &row.bind_ip, row.port, health)
    } else {
        String::new()
    };
    let traffic_cell = if is_running {
        traffic
            .and_then(|(rx, tx)| traffic_summary_text(rx, tx))
            .unwrap_or_default()
    } else {
        String::new()
    };
    [upstream_cell, health_cell, row.group.clone(), traffic_cell]
}

fn library_row_full_route(row: &ProxyConfig, upstream: &str) -> String {
    let state = if row.enabled { "enabled" } else { "disabled" };
    format!(
        "{} · {upstream} · {} · bind {}:{} · {state}",
        row.protocol, row.group, row.bind_ip, row.port
    )
}

#[cfg(test)]
fn library_row_matches(row: &ProxyConfig, query: &str) -> bool {
    library_row_matches_full(row, query, &MatchContext::empty())
}

#[cfg(test)]
#[derive(Debug, Default)]
struct MatchContext {
    sub_labels: HashMap<String, String>,
    preset_names: HashMap<String, String>,
    running: HashSet<String>,
    health: HashMap<String, String>,
}

#[cfg(test)]
impl MatchContext {
    #[cfg(test)]
    fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
fn library_row_matches_full(row: &ProxyConfig, query: &str, ctx: &MatchContext) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if library_row_matches_base(row, &q) {
        return true;
    }
    if let Some(owner) = row.subscription_id.as_deref() {
        if let Some(label) = ctx.sub_labels.get(owner) {
            if label.to_lowercase().contains(&q) {
                return true;
            }
        }
    }
    if let Some(preset) = row.preset_id.as_deref() {
        if let Some(name) = ctx.preset_names.get(preset) {
            if name.to_lowercase().contains(&q) {
                return true;
            }
        }
    }
    let state_word = if row.enabled { "enabled" } else { "disabled" };
    if state_word.contains(&q) {
        return true;
    }
    let serve_word = if ctx.running.contains(&row.id) {
        "running"
    } else {
        "stopped"
    };
    if serve_word.contains(&q) {
        return true;
    }
    let summary = ctx
        .health
        .get(&row.id)
        .map(|summary| summary.trim().to_lowercase())
        .unwrap_or_default();
    if summary.is_empty() {
        return "untested".contains(&q);
    }
    if summary.contains(&q) {
        return true;
    }
    let health_word = if summary == "dead" { "dead" } else { "live" };
    health_word.contains(&q)
}

#[cfg(test)]
fn library_row_matches_base(row: &ProxyConfig, q: &str) -> bool {
    if row.name.to_lowercase().contains(q)
        || row.protocol.to_lowercase().contains(q)
        || row.group.to_lowercase().contains(q)
        || row.bind_ip.to_lowercase().contains(q)
        || row.port.to_string().contains(q)
    {
        return true;
    }
    if let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&row.params)
    {
        for key in ["server", "address"] {
            if let Some(serde_json::Value::String(host)) = map.get(key) {
                if host.to_lowercase().contains(q) {
                    return true;
                }
            }
        }
    }
    false
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

/// Same fetch policy as subscriptions — validated at save time, before any
/// update check ever runs: https-only allow, plaintext http warns, internal
/// hosts are refused outright.
fn normalize_update_feed_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("feed URL must not be empty".to_string());
    }
    match store::subscription_fetch_policy(trimmed) {
        store::SubscriptionFetchPolicy::Allow | store::SubscriptionFetchPolicy::Warn { .. } => {
            Ok(trimmed.to_string())
        }
        store::SubscriptionFetchPolicy::Block { reason } => Err(reason.to_string()),
    }
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

#[cfg(test)]
fn first_import_failure_reason(text: &str, file_hint: Option<&str>) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let batch = match effective_hint_for_text(file_hint, text) {
        Some(name) => import_file_content(&name, text),
        None => import_text(text),
    };
    batch.failures.first().map(|failure| {
        format!(
            "item {} ({}): {}",
            failure.index + 1,
            failure.label,
            failure.reason
        )
    })
}

type BrowsedImport = (String, Option<String>, Option<Vec<(String, String)>>);

fn merge_browse_results(existing: &str, files: &[(String, String)]) -> BrowsedImport {
    let mut merged = existing.to_string();
    for (_, content) in files {
        if !merged.trim().is_empty() {
            merged.push('\n');
        }
        merged.push_str(content);
    }
    if files.len() == 1 {
        let hint = files.first().map(|(name, _)| name.clone());
        (merged, hint, None)
    } else if files.len() > 1 {
        (merged, None, Some(files.to_vec()))
    } else {
        (merged, None, None)
    }
}

#[cfg(test)]
fn log_level_matches(level: LogLevel, filter: Option<LogLevel>) -> bool {
    match filter {
        None => true,
        Some(wanted) => level == wanted,
    }
}

fn subscription_member_names(rows: &[ProxyConfig], sub_id: &str) -> Vec<String> {
    rows.iter()
        .filter(|row| row.subscription_id.as_deref() == Some(sub_id))
        .map(|row| row.name.clone())
        .collect()
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

fn secret_state_caption(value: &str) -> &'static str {
    if value.trim().is_empty() {
        "Not Set"
    } else {
        "Already Set"
    }
}

fn field_error_target(error: &StoreError) -> Option<(&'static str, String)> {
    match error {
        StoreError::Invalid(errors) => errors
            .first()
            .map(|first| (first.field, first.message.clone())),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct FetchedSubscription {
    pub body: String,
    pub userinfo: Option<store::SubscriptionQuota>,
}

/// T-12: everything one confirmed `.srs` download needs, resolved when the
/// user clicks **Download** on the per-file confirm dialog. The type is
/// constructed only in the confirm handler — staging a download without the
/// on-screen explicit confirm is not expressible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSetDownloadGrant {
    pub name: String,
    pub file_name: String,
    pub url: String,
    pub dir: PathBuf,
}

/// Shared T-08 subscription fetch pipeline: policy gates before any network at
/// every hop, `ManyProxy/<version>` UA, no-follow redirect loop, 20 s timeout,
/// 8 MiB cap. T-12's `.srs` staging fetch routes through this exact pipeline so
/// rule-set downloads get byte-identical transport policy (parity is literal).
struct FetchedHttp {
    body: Vec<u8>,
    userinfo: Option<store::SubscriptionQuota>,
}

async fn fetch_http_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<FetchedHttp, String> {
    use futures::AsyncReadExt as _;
    const MAX_BYTES: usize = store::SUBSCRIPTION_MAX_BYTES;
    const TIMEOUT_SECS: u64 = store::SUBSCRIPTION_TIMEOUT_SECS;
    const MAX_REDIRECTS: usize = store::SUBSCRIPTION_MAX_REDIRECTS;
    let mut current = url.trim().to_string();
    for _ in 0..=MAX_REDIRECTS {
        if let store::SubscriptionFetchPolicy::Block { reason } =
            store::subscription_fetch_policy(&current)
        {
            return Err(reason.to_string());
        }
        if let Some(host) = store::subscription_url_host(&current) {
            let resolved = resolve_subscription_addresses(host, resolver).await;
            if let store::SubscriptionFetchPolicy::Block { reason } =
                store::subscription_fetch_policy_resolved(&current, &resolved)
            {
                return Err(reason.to_string());
            }
        }
        let request = http_client::http::Request::builder()
            .uri(current.as_str())
            .header("user-agent", user_agent_header())
            .follow_redirects(http_client::RedirectPolicy::NoFollow)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .body(AsyncBody::empty())
            .map_err(|_| "subscription request failed".to_string())?;
        let mut response = client
            .send(request)
            .await
            .map_err(|_| "subscription fetch failed".to_string())?;
        if is_fetch_redirect(response.status()) {
            match fetch_redirect_location(response.headers(), &current) {
                Some(next) => {
                    current = next;
                    continue;
                }
                None => return Err("subscription fetch failed (bad redirect)".to_string()),
            }
        }
        if !response.status().is_success() {
            return Err(format!(
                "subscription fetch failed (status {})",
                response.status().as_u16()
            ));
        }

        let userinfo = response
            .headers()
            .get("subscription-userinfo")
            .and_then(|value| value.to_str().ok())
            .and_then(store::parse_subscription_userinfo);
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
        return Ok(FetchedHttp {
            body: buf,
            userinfo,
        });
    }
    Err("subscription fetch failed (too many redirects)".to_string())
}

async fn fetch_subscription_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<FetchedSubscription, String> {
    let fetched = fetch_http_body(client, url, resolver).await?;
    String::from_utf8(fetched.body)
        .map(|body| FetchedSubscription {
            body,
            userinfo: fetched.userinfo,
        })
        .map_err(|_| "subscription body is not UTF-8".to_string())
}

/// T-12: the confirmed rule-set download rides the same pipeline; `.srs` is
/// binary, so the body stays bytes (never UTF-8-validated like subscriptions).
async fn fetch_rule_set_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<Vec<u8>, String> {
    Ok(fetch_http_body(client, url, resolver).await?.body)
}

type SubscriptionHostResolver = fn(&str) -> Result<Vec<std::net::IpAddr>, String>;

fn resolve_subscription_host(host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    use std::net::ToSocketAddrs as _;
    (host, 0u16)
        .to_socket_addrs()
        .map(|addrs| addrs.map(|addr| addr.ip()).collect())
        .map_err(|_| "subscription host does not resolve".to_string())
}

async fn resolve_subscription_addresses(
    host: String,
    resolver: SubscriptionHostResolver,
) -> Vec<std::net::IpAddr> {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(resolver(&host).unwrap_or_default());
    });
    rx.await.unwrap_or_default()
}

/// Every outbound fetch-layer request carries `User-Agent: ManyProxy/<APP_VERSION>`.
/// A static, per-release identifier with zero device identity: Throne's
/// `x-hwid`/`x-device-*` header family is device fingerprinting, and this
/// function must never gain any hostname/MAC/ID component.
pub(crate) fn user_agent_header() -> String {
    format!("ManyProxy/{}", APP_VERSION)
}

fn is_fetch_redirect(status: http_client::http::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

fn resolve_fetch_redirect(current_url: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.is_empty() {
        return None;
    }
    if location.starts_with('/') {
        let scheme_end = current_url.find("://")?;
        let after_scheme = &current_url[scheme_end + 3..];
        let authority_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let origin = &current_url[..scheme_end + 3 + authority_end];
        if origin.ends_with("://") {
            return None;
        }
        return Some(format!("{origin}{location}"));
    }
    if location
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || location
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return Some(location.to_string());
    }
    None
}

fn fetch_redirect_location(
    headers: &http_client::http::HeaderMap,
    current_url: &str,
) -> Option<String> {
    let location = headers
        .get(http_client::http::header::LOCATION)?
        .to_str()
        .ok()?;
    resolve_fetch_redirect(current_url, location)
}

pub const UPDATE_FEED_URL_DEFAULT: &str =
    "https://api.github.com/repos/QMahyar/ManyProxy/releases/latest";

/// T-06: the update check is a passive JSON GET — 10 s timeout, one MiB cap,
/// at most five hand-followed hops. It reads the feed and nothing else;
/// release assets are never fetched, hashed, or executed by the app.
const UPDATE_FEED_TIMEOUT_SECS: u64 = 10;

const UPDATE_FEED_MAX_REDIRECTS: usize = store::SUBSCRIPTION_MAX_REDIRECTS;

const UPDATE_FEED_MAX_BYTES: usize = 1024 * 1024;

pub const UPDATE_FEED_EMPTY_MESSAGE: &str = "No releases published yet.";

/// One release entry read out of the JSON feed. The app keeps parsed
/// metadata only — assets are never touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedRelease {
    pub tag_name: String,
    pub prerelease: bool,
    pub html_url: Option<String>,
    pub release_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseCompare {
    UpToDate,
    Newer,
    Older,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemverIdent {
    Numeric(u64),
    Alnum(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTag {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub idents: Vec<SemverIdent>,
}

/// Strips the leading `v`, splits `major.minor.patch` from the optional
/// `-prerelease` suffix, and ignores `+build` metadata per semver rule 10.
pub fn parse_release_tag(tag: &str) -> Option<ParsedTag> {
    let trimmed = tag.trim();
    let no_v = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let no_build = no_v.split('+').next().unwrap_or(no_v);
    let (core, suffix) = match no_build.split_once('-') {
        Some((core, suffix)) => (core, suffix),
        None => (no_build, ""),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if suffix.is_empty() {
        if no_build.contains('-') {
            return None;
        }
        return Some(ParsedTag {
            major,
            minor,
            patch,
            idents: Vec::new(),
        });
    }
    let mut idents = Vec::new();
    for ident in suffix.split('.') {
        if ident.is_empty() {
            return None;
        }
        if ident.bytes().all(|b| b.is_ascii_digit()) {
            idents.push(SemverIdent::Numeric(ident.parse::<u64>().ok()?));
        } else {
            idents.push(SemverIdent::Alnum(ident.to_string()));
        }
    }
    Some(ParsedTag {
        major,
        minor,
        patch,
        idents,
    })
}

/// Compares two parsed tags. Equal triples rank the plain release above its
/// own prereleases (rule 10); two prerelease tag sets compare per rule 11
/// (numeric < alphanumeric, larger set outranks the smaller).
pub fn compare_parsed_tags(a: &ParsedTag, b: &ParsedTag) -> std::cmp::Ordering {
    let triple = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if triple != std::cmp::Ordering::Equal {
        return triple;
    }
    match (a.idents.is_empty(), b.idents.is_empty()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => compare_pre_identifiers(&a.idents, &b.idents),
    }
}

fn compare_pre_identifiers(a: &[SemverIdent], b: &[SemverIdent]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = match (left, right) {
            (SemverIdent::Numeric(left), SemverIdent::Numeric(right)) => left.cmp(right),
            (SemverIdent::Numeric(_), SemverIdent::Alnum(_)) => std::cmp::Ordering::Less,
            (SemverIdent::Alnum(_), SemverIdent::Numeric(_)) => std::cmp::Ordering::Greater,
            (SemverIdent::Alnum(left), SemverIdent::Alnum(right)) => left.cmp(right),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_release_to_running(tag_name: &str, running: &str) -> ReleaseCompare {
    let Some(candidate) = parse_release_tag(tag_name) else {
        return ReleaseCompare::UpToDate;
    };
    let Some(current) = parse_release_tag(running) else {
        return ReleaseCompare::UpToDate;
    };
    match compare_parsed_tags(&candidate, &current) {
        std::cmp::Ordering::Greater => ReleaseCompare::Newer,
        std::cmp::Ordering::Less => ReleaseCompare::Older,
        std::cmp::Ordering::Equal => ReleaseCompare::UpToDate,
    }
}

/// The running binary only sees prerelease tags when it itself carries a
/// prerelease suffix (the brief: `APP_VERSION` rides the channel).
fn running_is_prerelease(running: &str) -> bool {
    running.contains('-')
}

fn newest_tag_among(entries: &[FetchedRelease], allow_prerelease: bool) -> Option<FetchedRelease> {
    let mut best: Option<(ParsedTag, &FetchedRelease)> = None;
    for entry in entries {
        if entry.prerelease && !allow_prerelease {
            continue;
        }
        let Some(parsed) = parse_release_tag(&entry.tag_name) else {
            continue;
        };
        let replace = match &best {
            Some((top, _)) => compare_parsed_tags(&parsed, top) == std::cmp::Ordering::Greater,
            None => true,
        };
        if replace {
            best = Some((parsed, entry));
        }
    }
    best.map(|(_, entry)| entry.clone())
}

fn json_str_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .map(str::to_string)
}

fn release_entry(value: &serde_json::Value) -> Option<FetchedRelease> {
    if !value.is_object() {
        return None;
    }
    Some(FetchedRelease {
        tag_name: json_str_field(value, "tag_name")?,
        prerelease: value
            .get("prerelease")
            .and_then(|field| field.as_bool())
            .unwrap_or(false),
        html_url: json_str_field(value, "html_url"),
        release_name: json_str_field(value, "name"),
    })
}

/// Parses the feed body. `Ok(None)` means nothing a compare can act on —
/// everything published is prerelease-only while the running build is stable,
/// or no release carries a comparable tag. HTTP 404 stays with the fetch fn.
pub fn parse_release_feed(
    body: &str,
    list_endpoint: bool,
    running: &str,
) -> Result<Option<FetchedRelease>, String> {
    const MALFORMED: &str = "update feed is not valid JSON";
    let value: serde_json::Value =
        serde_json::from_str(body.trim()).map_err(|_| MALFORMED.to_string())?;
    let allow_prerelease = running_is_prerelease(running);
    if list_endpoint {
        let entries = value.as_array().ok_or_else(|| MALFORMED.to_string())?;
        let mut candidates = Vec::new();
        for entry in entries {
            if let Some(release) = release_entry(entry) {
                candidates.push(release);
            }
        }
        Ok(newest_tag_among(&candidates, allow_prerelease))
    } else {
        let feed = release_entry(&value).ok_or_else(|| MALFORMED.to_string())?;
        if feed.prerelease && !allow_prerelease {
            return Ok(None);
        }
        if parse_release_tag(&feed.tag_name).is_none() {
            return Ok(None);
        }
        Ok(Some(feed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckOutcome {
    UpToDate,
    NoReleases,
    Newer(FetchedRelease),
}

/// Darkens the feed result into a dialog outcome: 404 is the exact
/// "No releases published yet." state (not an error class), a filtered
/// release set is up-to-date, and every other failure is a plain error.
pub fn update_check_outcome(
    fetched: Result<Option<FetchedRelease>, String>,
) -> Result<UpdateCheckOutcome, String> {
    match fetched {
        Ok(Some(release)) => {
            let candidate = compare_release_to_running(&release.tag_name, APP_VERSION);
            Ok(match candidate {
                ReleaseCompare::Newer => UpdateCheckOutcome::Newer(release),
                ReleaseCompare::UpToDate | ReleaseCompare::Older => UpdateCheckOutcome::UpToDate,
            })
        }
        Ok(None) => Ok(UpdateCheckOutcome::UpToDate),
        Err(why) if why == UPDATE_FEED_EMPTY_MESSAGE => Ok(UpdateCheckOutcome::NoReleases),
        Err(why) => Err(why),
    }
}

/// (message element id, message text, optional release title, optional https
/// releases page) — the single source the dialog renders from, so tests pin
/// the exact strings without pulling them out of a painted frame.
fn update_check_presentation(
    outcome: &UpdateCheckOutcome,
    running: &str,
) -> (&'static str, String, Option<String>, Option<String>) {
    match outcome {
        UpdateCheckOutcome::UpToDate => (
            "update-up-to-date",
            format!("ManyProxy {running} is up to date."),
            None,
            None,
        ),
        UpdateCheckOutcome::NoReleases => (
            "update-no-releases",
            UPDATE_FEED_EMPTY_MESSAGE.to_string(),
            None,
            None,
        ),
        UpdateCheckOutcome::Newer(release) => {
            let tag = release.tag_name.trim().to_string();
            let name = release
                .release_name
                .clone()
                .filter(|name| !name.trim().is_empty() && name.trim() != tag);
            let url = release
                .html_url
                .clone()
                .filter(|value| value.trim().starts_with("https://"));
            (
                "update-new-version",
                format!("New version available: {tag} (running {running})"),
                name,
                url,
            )
        }
    }
}

/// Stable binaries stay on `releases/latest`; a prerelease build rides the
/// releases list so prerelease tags become visible (the channel rule).
pub fn effective_update_feed_url(override_url: Option<&str>, running: &str) -> String {
    if let Some(url) = override_url.filter(|value| !value.trim().is_empty()) {
        return url.trim().to_string();
    }
    let default = UPDATE_FEED_URL_DEFAULT.trim();
    if running_is_prerelease(running) {
        if let Some(prefix) = default.strip_suffix("/releases/latest") {
            return format!("{prefix}/releases");
        }
    }
    default.to_string()
}

/// A feed URL targets the releases *list* when the path (sans query or
/// fragment) ends in a `releases` segment — GitHub exposes
/// `/repos/<owner>/<repo>/releases` (list) vs `/releases/latest` (single).
fn is_releases_list_url(url: &str) -> bool {
    let path = url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    path.rsplit('/').next().unwrap_or("") == "releases"
}

/// Fetches the JSON-only release feed through the subscription fetch policy —
/// the same SSRF guards (https-only allow, plaintext http warns, internal
/// hosts never fetched), redirects followed by hand.
pub async fn fetch_release_feed(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<Option<FetchedRelease>, String> {
    use futures::AsyncReadExt as _;
    const MAX_BYTES: usize = UPDATE_FEED_MAX_BYTES;
    let list_endpoint = is_releases_list_url(url);
    let mut current = url.trim().to_string();
    for _ in 0..=UPDATE_FEED_MAX_REDIRECTS {
        if let store::SubscriptionFetchPolicy::Block { reason } =
            store::subscription_fetch_policy(&current)
        {
            return Err(reason.to_string());
        }
        if let Some(host) = store::subscription_url_host(&current) {
            let resolved = resolve_subscription_addresses(host, resolver).await;
            if let store::SubscriptionFetchPolicy::Block { reason } =
                store::subscription_fetch_policy_resolved(&current, &resolved)
            {
                return Err(reason.to_string());
            }
        }
        let request = http_client::http::Request::builder()
            .uri(current.as_str())
            .header("user-agent", user_agent_header())
            .follow_redirects(http_client::RedirectPolicy::NoFollow)
            .timeout(std::time::Duration::from_secs(UPDATE_FEED_TIMEOUT_SECS))
            .body(AsyncBody::empty())
            .map_err(|_| "update feed request failed".to_string())?;
        let mut response = client
            .send(request)
            .await
            .map_err(|_| "update feed fetch failed".to_string())?;
        if is_fetch_redirect(response.status()) {
            match fetch_redirect_location(response.headers(), &current) {
                Some(next) => {
                    current = next;
                    continue;
                }
                None => return Err("update feed fetch failed (bad redirect)".to_string()),
            }
        }
        if response.status().as_u16() == 404 {
            // No releases published and still-private repos both land here.
            return Err(UPDATE_FEED_EMPTY_MESSAGE.to_string());
        }
        if !response.status().is_success() {
            return Err(format!(
                "update feed fetch failed (status {})",
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
                .map_err(|_| "update feed fetch failed".to_string())?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_BYTES {
                return Err("update feed body too large".to_string());
            }
        }
        let body =
            String::from_utf8(buf).map_err(|_| "update feed body is not UTF-8".to_string())?;
        return parse_release_feed(&body, list_endpoint, APP_VERSION);
    }
    Err("update feed fetch failed (too many redirects)".to_string())
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        lp_operation: *const u16,
        lp_file: *const u16,
        lp_parameters: *const u16,
        lp_directory: *const u16,
        n_show_cmd: i32,
    ) -> isize;
}

/// Opens the releases page in the user's own browser on their explicit
/// click — https-only, no prefetch, no app-initiated navigation.
fn open_release_in_browser(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return Err("only https release pages can open in a browser".to_string());
    }
    open_url_shell_execute(trimmed)
}

#[cfg(windows)]
fn open_url_shell_execute(url: &str) -> Result<(), String> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt as _;
    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(once(0))
            .collect()
    }
    let operation = wide("open");
    let file = wide(url);
    let instance = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    };
    if instance > 32 {
        Ok(())
    } else {
        Err(format!(
            "cannot open the browser to {url} (shell returned {instance})"
        ))
    }
}

#[cfg(not(windows))]
fn open_url_shell_execute(_url: &str) -> Result<(), String> {
    Err("opening the release page needs the Windows shell (ManyProxy is Windows-only)".to_string())
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

#[derive(Clone)]
struct SubForm {
    label: Entity<InputState>,
    url: Entity<InputState>,
    group: Entity<InputState>,
    auto_refresh: Rc<RefCell<bool>>,

    interval_secs: Rc<RefCell<i64>>,
}

impl SubForm {
    fn restored(
        window: &mut Window,
        cx: &mut Context<AppView>,
        label: &str,
        url: &str,
        group: &str,
        auto_refresh: bool,
        interval_secs: i64,
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
            interval_secs: Rc::new(RefCell::new(interval_secs)),
        }
    }

    fn element(self, view: Entity<AppView>, cx: &App) -> impl IntoElement {
        let auto = self.auto_refresh.clone();
        let checked = *auto.borrow();
        let url = self.url.read(cx).value().to_string();
        let repaint_view = view.clone();
        let repaint: DialogRepaint = Rc::new(move |_, cx| {
            repaint_view.update(cx, |_, _| {});
        });
        let mut form = v_form()
            .small()
            .child(dialog_field("field-sub-label", "Label", self.label))
            .child(dialog_field("field-sub-url", "Subscription URL", self.url));

        if let Some(warning) = http_subscription_warning(&url) {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("sub-add-http-warning")
                        .test_support()
                        .child(warning),
                ),
            );
        }
        form = form
            .child(dialog_field("field-sub-group", "Group", self.group))
            .child(field().label_indent(false).child(
                div().id("field-sub-auto").test_support().child(
                    Switch::new("sub-auto-add")
                        .checked(checked)
                        .label("Auto-refresh")

                        .tooltip(
                            "Automatic refresh of this subscription. Manual Refresh always works.",
                        )
                        .accessibility_label(
                            "Auto-refresh this subscription automatically. Manual Refresh always works.",
                        )
                        .on_change(move |next: &bool, _, _| {
                            *auto.borrow_mut() = *next;
                        }),
                ),
            ))

            .child(field().label_indent(false).child(
                div()
                    .id("field-sub-interval")
                    .test_support()
                    .child(interval_picker_row(
                        "sub-interval-add",
                        self.interval_secs,
                        repaint,
                    )),
            ));
        form
    }
}

fn subscription_row(
    view: Entity<AppView>,
    sub: ProxySubscription,
    count: usize,
    expanded: bool,
    members: Vec<String>,
    refreshing: bool,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(Label::new(sub.label.clone()))
        .child(format!(
            "{} · {} · {count} configs · auto {}",
            redact_subscription_url(&sub.url),
            sub.group,
            if sub.auto_refresh { "on" } else { "off" }
        ))
        .child(
            div()
                .id(format!("sub-meta-{}", sub.id))
                .test_support()
                .child(format!(
                    "last refresh: {} · every {}",
                    format_last_refresh(sub.last_refresh_at, now_unix()),
                    format_interval_secs(sub.interval_secs)
                )),
        );
    if let Some(error) = sub.last_error.as_deref() {
        row = row.child(format!("last error: {error}"));
    }

    let failures = subscription_failures_text(&sub.failures);
    if !failures.is_empty() {
        row = row.child(
            div()
                .id(format!("sub-failures-{}", sub.id))
                .test_support()
                .child(failures),
        );
    }

    let expand_view = view.clone();
    let expand_id = sub.id.clone();
    row = row.child(
        Button::new(format!("sub-expand-{}", sub.id))
            .label(if expanded { "Collapse" } else { "Expand" })
            .on_click(move |_, window, cx| {
                let sub_id = expand_id.clone();
                expand_view.update(cx, |view, cx| {
                    view.toggle_sub_expanded(&sub_id, window, cx);
                });
            }),
    );
    if expanded {
        let names = if members.is_empty() {
            "no member rows".to_string()
        } else {
            members.join(", ")
        };
        row = row.child(
            div()
                .id(format!("sub-members-{}", sub.id))
                .test_support()
                .child(format!("members: {names}")),
        );
        let copy_view = view.clone();
        let copy_url = sub.url.clone();
        row = row.child(
            Button::new(format!("sub-copy-url-{}", sub.id))
                .label("Copy URL")
                .on_click(move |_, window, cx| {
                    let redacted = redact_subscription_url(&copy_url);
                    cx.write_to_clipboard(ClipboardItem::new_string(redacted));
                    copy_view.update(cx, |_, cx| {
                        window.push_notification(
                            Notification::success("Copied subscription URL (redacted)"),
                            cx,
                        );
                    });
                }),
        );

        let full_view = view.clone();
        let full_url = sub.url.clone();
        let full_label = sub.label.clone();
        row = row.child(
            Button::new(format!("sub-copy-full-url-{}", sub.id))
                .label("Copy full URL")
                .on_click(move |_, window, cx| {
                    let url = full_url.clone();
                    let label = full_label.clone();
                    full_view.update(cx, |view, cx| {
                        view.open_sub_full_url_confirm(&label, &url, window, cx);
                    });
                }),
        );
    }
    let refresh_view = view.clone();
    let refresh_id = sub.id.clone();

    let mut refresh_button = Button::new(format!("sub-refresh-{}", sub.id))
        .label("Refresh")
        .loading(refreshing)
        .on_click(move |_, window, cx| {
            let sub_id = refresh_id.clone();
            refresh_view.update(cx, |view, cx| {
                view.refresh_subscription_by_id(&sub_id, window, cx);
            });
        });
    if refreshing {
        refresh_button = refresh_button.child(
            div()
                .id(format!("sub-refresh-spinner-{}", sub.id))
                .test_support()
                .child(Spinner::new().small()),
        );
    }
    row = row.child(refresh_button);

    if refreshing {
        row = row.child(
            div()
                .id(format!("sub-refreshing-{}", sub.id))
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(Skeleton::new().max_w(px(120.)))
                .child(
                    ShimmerText::new("Refreshing…").id(format!("sub-refreshing-text-{}", sub.id)),
                ),
        );
    }
    let auto_view = view.clone();
    let auto_id = sub.id.clone();
    let auto_now = sub.auto_refresh;
    let auto_label = sub.label.clone();
    row = row.child(
        Button::new(format!("sub-auto-{}", sub.id))
            .label(if auto_now { "Auto on" } else { "Auto off" })
            .tooltip(format!(
                "Automatic refresh of \"{auto_label}\" only. Manual Refresh always works."
            ))
            .accessibility_label(format!(
                "Auto-refresh \"{auto_label}\" automatically. Manual Refresh always works."
            ))
            .on_click(move |_, window, cx| {
                let sub_id = auto_id.clone();
                auto_view.update(cx, |view, cx| {
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupRowView {
    name: String,
    count: usize,
    kind: String,
    quota: Option<String>,
    skip: bool,
    auto_clear: bool,
    sub_ids: Vec<String>,
}

fn group_row_views(
    counts: &[(String, usize)],
    subs: &[ProxySubscription],
    skip: &dyn Fn(&str) -> bool,
    auto_clear: &dyn Fn(&str) -> bool,
    now: i64,
) -> Vec<GroupRowView> {
    use std::collections::BTreeMap;
    let mut counts_map: BTreeMap<&str, usize> = BTreeMap::new();
    for (group, count) in counts {
        counts_map.insert(group.as_str(), *count);
    }
    let mut feeding: BTreeMap<&str, Vec<&ProxySubscription>> = BTreeMap::new();
    for sub in subs {
        feeding.entry(sub.group.as_str()).or_default().push(sub);
    }
    let mut names: Vec<&str> = counts_map.keys().copied().collect();
    for name in feeding.keys() {
        if !counts_map.contains_key(name) {
            names.push(name);
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let feeding = feeding.get(name).cloned().unwrap_or_default();
            let count = counts_map.get(name).copied().unwrap_or(0);
            let kind = if feeding.is_empty() {
                "Basic".to_string()
            } else {
                format!("Subscription ({})", feeding.len())
            };
            let mut quota = group_quota_line(&feeding, now);
            let skipped = skip(name);
            if skipped {
                let marker = "auto-refresh skipped".to_string();
                quota = Some(match quota {
                    Some(line) => format!("{line} · {marker}"),
                    None => marker,
                });
            }
            GroupRowView {
                name: name.to_string(),
                count,
                kind,
                quota,
                skip: skipped,
                auto_clear: auto_clear(name),
                sub_ids: feeding.iter().map(|sub| sub.id.clone()).collect(),
            }
        })
        .collect()
}

fn group_quota_line(subs: &[&ProxySubscription], now: i64) -> Option<String> {
    if subs.is_empty() {
        return None;
    }
    let last = subs.iter().filter_map(|sub| sub.last_refresh_at).max();
    let mut segments = vec![format!("Last update {}", format_last_refresh(last, now))];
    let mut used: i64 = 0;
    let mut used_known = false;
    let mut total: i64 = 0;
    let mut total_known = false;
    let mut expire: Option<i64> = None;
    for sub in subs {
        if let Some(value) = sub.quota_used {
            used = used.saturating_add(value.max(0));
            used_known = true;
        }
        if let Some(value) = sub.quota_total {
            total = total.saturating_add(value.max(0));
            total_known = true;
        }
        expire = match (expire, sub.quota_expire) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
    }
    if used_known {
        segments.push(format!("Used {}", format_bytes(used.max(0) as u64)));
    }
    if total_known {
        segments.push(format!(
            "Remain {}",
            format_bytes(total.saturating_sub(used).max(0) as u64)
        ));
    }
    if let Some(expire) = expire {
        segments.push(format!("Expire {}", format_expire(expire, now)));
    }
    Some(segments.join(" | "))
}

fn update_target_subs(subs: &[ProxySubscription], group: &str) -> Vec<ProxySubscription> {
    let mut out: Vec<ProxySubscription> = subs
        .iter()
        .filter(|sub| sub.group == group)
        .cloned()
        .collect();
    out.sort_by_key(|sub| sub.id.parse::<i64>().unwrap_or(i64::MAX));
    out
}

fn update_all_targets(
    subs: Vec<ProxySubscription>,
    flagged: &HashSet<String>,
) -> (Vec<ProxySubscription>, Vec<ProxySubscription>) {
    let mut included = Vec::new();
    let mut skipped = Vec::new();
    for sub in subs {
        if flagged.contains(sub.group.as_str()) {
            skipped.push(sub);
        } else {
            included.push(sub);
        }
    }
    (included, skipped)
}

#[derive(Clone)]
/// T-11 simple-rules boxes (write-once authoring sugar over the same stored
/// rules): the rules JSON view stays the single source of truth — box text
/// expands into the structured rules on save and round-trips nowhere.
struct PresetForm {
    name: Entity<InputState>,
    rules: Entity<TextareaState>,
    simple_direct: Entity<TextareaState>,
    simple_block: Entity<TextareaState>,
    simple_proxy: Entity<TextareaState>,
}

impl PresetForm {
    fn restored(window: &mut Window, cx: &mut Context<AppView>, name: &str, rules: &str) -> Self {
        Self::restored_with_boxes(window, cx, name, rules, &SimpleRuleBoxes::default())
    }

    fn restored_with_boxes(
        window: &mut Window,
        cx: &mut Context<AppView>,
        name: &str,
        rules: &str,
        boxes: &SimpleRuleBoxes,
    ) -> Self {
        Self {
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Preset name")
                    .default_value(name.to_string())
            }),
            rules: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Optional rules JSON (preset object or rules array)")
                    .default_value(rules.to_string())
            }),
            simple_direct: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("domain:example.com, suffix:cn — one rule per line")
                    .default_value(boxes.direct.clone())
            }),
            simple_block: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("keyword:ads, processName:updater.exe")
                    .default_value(boxes.block.clone())
            }),
            simple_proxy: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("ruleset:streaming, domain:netflix.com")
                    .default_value(boxes.proxy.clone())
            }),
        }
    }

    fn element(self, cx: &App) -> impl IntoElement {
        use crate::preset::SimpleRulesBox;
        let mut simple = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id("preset-simple-rules")
                    .test_support()
                    .text_sm()
                    .child("Simple rules"),
            )
            .child(
                div()
                    .id("preset-simple-help")
                    .test_support()
                    .text_sm()
                    .child(
                        "One rule per line: tokens domain:/suffix:/keyword:/regex:/ruleset:/ip:/processName:/processPath: separated by commas or spaces; blank and # lines are skipped. ruleset: names require a staged rule set (Rule sets section) — unstaged sets fail loud when the preset attaches and at config generation.",
                    ),
            );
        for (r#box, state) in [
            (SimpleRulesBox::Direct, &self.simple_direct),
            (SimpleRulesBox::Block, &self.simple_block),
            (SimpleRulesBox::Proxy, &self.simple_proxy),
        ] {
            simple = simple
                .child(
                    div()
                        .id(format!("preset-simple-label-{}", r#box.action()))
                        .test_support()
                        .text_sm()
                        .child(r#box.label()),
                )
                .child(
                    div()
                        .id(format!("field-preset-simple-{}", r#box.action()))
                        .test_support()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Textarea::new(state).h(px(56.))),
                );
        }
        let invalid_lines = simple_rules_invalid_rows(&self, cx);
        if !invalid_lines.is_empty() {
            let mut fold = div()
                .id("preset-simple-invalid-lines")
                .test_support()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .id("preset-simple-invalid-header")
                        .test_support()
                        .text_sm()
                        .child("Invalid lines"),
                );
            for (index, (r#box, line)) in invalid_lines.into_iter().enumerate() {
                fold = fold.child(
                    div()
                        .id(format!("preset-simple-invalid-{index}"))
                        .test_support()
                        .text_sm()
                        .child(format!("{} box: {}", r#box.label(), line)),
                );
            }
            simple = simple.child(fold);
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dialog_field("field-preset-name", "Name", self.name))
            .child(
                div()
                    .id("field-preset-rules")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new("Rules JSON (optional)"))
                    .child(Textarea::new(&self.rules).h(px(120.))),
            )
            .child(simple)
    }
}

/// T-11: a snapshot of the three simple-rules boxes read from a form at
/// commit time. Empty boxes contribute nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SimpleRuleBoxes {
    direct: String,
    block: String,
    proxy: String,
}

impl SimpleRuleBoxes {
    fn from_form(form: &PresetForm, cx: &App) -> Self {
        Self {
            direct: form.simple_direct.read(cx).value().to_string(),
            block: form.simple_block.read(cx).value().to_string(),
            proxy: form.simple_proxy.read(cx).value().to_string(),
        }
    }

    fn is_empty(&self) -> bool {
        self.direct.trim().is_empty()
            && self.block.trim().is_empty()
            && self.proxy.trim().is_empty()
    }
}

/// The invalid lines the current parse of the three boxes produces; the
/// fold always mirrors the text as typed and exists for this session only —
/// nothing is persisted. Cheap: the parser is pure and small inputs are the
/// norm.
fn simple_rules_invalid_rows(
    form: &PresetForm,
    cx: &App,
) -> Vec<(crate::preset::SimpleRulesBox, String)> {
    use crate::preset::SimpleRulesBox;
    let mut rows = Vec::new();
    for (r#box, state) in [
        (SimpleRulesBox::Direct, &form.simple_direct),
        (SimpleRulesBox::Block, &form.simple_block),
        (SimpleRulesBox::Proxy, &form.simple_proxy),
    ] {
        let text = state.read(cx).value().to_string();
        if text.trim().is_empty() {
            continue;
        }
        let parsed = crate::preset::parse_simple_rules(r#box, r#box.action(), &text);
        rows.extend(
            parsed
                .invalid
                .into_iter()
                .map(|line| (r#box, line.to_string())),
        );
    }
    rows
}

fn preset_row(
    view: Entity<AppView>,
    preset: store::RoutePreset,
    attached: Vec<String>,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(preset.name.clone()))
        .child(preset_row_summary(&preset, &attached));
    if preset.locked {
        row = row.child(Label::new("Locked"));
    }
    if preset.source == "url" && preset.source_url.is_some() {
        let refresh_view = view.clone();
        let refresh_id = preset.id.clone();
        row = row.child(
            Button::new(format!("preset-refresh-{}", preset.id))
                .label("Refresh")
                .on_click(move |_, window, cx| {
                    let id = refresh_id.clone();
                    refresh_view.update(cx, |view, cx| {
                        view.refresh_preset_by_id(&id, window, cx);
                    });
                }),
        );
    }
    let remove_view = view.clone();
    let remove_id = preset.id.clone();
    let edit_view = view.clone();
    let edit_preset = preset.clone();
    let lock_view = view.clone();
    let lock_id = preset.id.clone();
    let lock_next = !preset.locked;
    let export_view = view.clone();
    let export_preset = preset.clone();
    row = row.child(
        Button::new(format!("preset-lock-{}", preset.id))
            .label(if preset.locked { "Unlock" } else { "Lock" })
            .tooltip(if preset.locked {
                "Allow modifications again (the rules gain an editor)"
            } else {
                "Prevent modifications: the rules stay byte-stable; attach/detach unaffected"
            })
            .on_click(move |_, window, cx| {
                let id = lock_id.clone();
                let locked = lock_next;
                lock_view.update(cx, |view, cx| {
                    view.commit_preset_lock(&id, locked, window, cx);
                });
            }),
    );
    row = row.child(
        Button::new(format!("preset-export-{}", preset.id))
            .label("Export link")
            .tooltip("Copy this preset's throne://route share link")
            .on_click(move |_, window, cx| {
                let preset = export_preset.clone();
                export_view.update(cx, |view, cx| {
                    view.commit_preset_export_link(&preset, window, cx);
                });
            }),
    );
    row = row.child(
        Button::new(format!("preset-edit-{}", preset.id))
            .label("Edit")
            .on_click(move |_, window, cx| {
                let preset = edit_preset.clone();
                edit_view.update(cx, |view, cx| {
                    view.open_preset_edit_dialog(preset, window, cx);
                });
            }),
    );
    let duplicate_view = view.clone();
    let duplicate_id = preset.id.clone();
    row = row.child(
        Button::new(format!("preset-duplicate-{}", preset.id))
            .label("Duplicate")
            .on_click(move |_, window, cx| {
                let id = duplicate_id.clone();
                duplicate_view.update(cx, |view, cx| {
                    view.commit_preset_duplicate(&id, window, cx);
                });
            }),
    );
    row.child(
        Button::new(format!("preset-remove-{}", preset.id))
            .label("Remove")
            .on_click(move |_, window, cx| {
                let id = remove_id.clone();
                remove_view.update(cx, |view, cx| {
                    view.commit_preset_remove(&id, window, cx);
                });
            }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresetRulesOp {
    CycleAction(usize),
    Remove(usize),
    Add,
}

fn rule_set_row(
    view: Entity<AppView>,
    row: store::RuleSetRow,
    used_by: Vec<String>,
) -> impl IntoElement {
    let redownload_view = view.clone();
    let redownload_row = row.clone();
    let remove_view = view.clone();
    let remove_id = row.id.clone();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(format!("{} · {}", row.name, row.file_name)))
        .child(Label::new(rule_set_summary(&row, &used_by)))
        .child(
            Button::new(format!("rule-set-redownload-{}", row.id))
                .label("Re-Download")
                .tooltip("Confirm the download again; bytes are overwritten")
                .on_click(move |_, window, cx| {
                    let row = redownload_row.clone();
                    redownload_view.update(cx, |view, cx| {
                        view.open_rule_set_download_confirm(
                            RuleSetDownloadGrant {
                                name: row.name.clone(),
                                file_name: row.file_name.clone(),
                                url: row.url.clone(),
                                dir: crate::store::rule_sets_dir(),
                            },
                            window,
                            cx,
                        );
                    });
                }),
        )
        .child(
            Button::new(format!("rule-set-remove-{}", row.id))
                .label("Remove")
                .tooltip("Removes the staged file; blocked while presets use the set")
                .on_click(move |_, window, cx| {
                    let id = remove_id.clone();
                    remove_view.update(cx, |view, cx| {
                        view.commit_rule_set_remove(&id, window, cx);
                    });
                }),
        )
}

/// T-09 attribute fold keys, rendered per rule as JSON-shape fields. The
/// rules JSON box stays the single editing surface; no per-key input state.
const PRESET_FOLD_KEYS: &[(&str, &str)] = &[
    ("source_ip_cidr", "Source CIDR"),
    ("source_ip_is_private", "Source is private"),
    ("source_port", "Source port"),
    ("source_port_range", "Source port range"),
    ("ip_version", "IP version"),
    ("invert", "Invert"),
    ("process_path_regex", "Process path regex"),
];

fn preset_rule_attribute_folds(
    preset_id: &str,
    index: usize,
    rule: &serde_json::Value,
) -> Vec<AnyElement> {
    PRESET_FOLD_KEYS
        .iter()
        .filter_map(|(key, label)| {
            let value = rule.get(*key)?;
            Some(
                div()
                    .id(format!("preset-rule-fold-{preset_id}-{index}-{key}"))
                    .test_support()
                    .text_sm()
                    .child(format!(
                        "{label}: {}",
                        serde_json::to_string(value).unwrap_or_default()
                    ))
                    .into_any_element(),
            )
        })
        .collect()
}

fn preset_advanced_section(
    view: Entity<AppView>,
    preset: &store::RoutePreset,
    form: Option<PresetForm>,
    exits: &[String],
) -> impl IntoElement {
    let mut section = div().flex().flex_col().gap_1().child(
        div()
            .id(format!("preset-advanced-{}", preset.id))
            .test_support()
            .text_sm()
            .child("Advanced"),
    );
    let rules = match preset.rules() {
        Ok(rules) => rules,
        Err(message) => {
            return section.child(
                div()
                    .id(format!("preset-advanced-invalid-{}", preset.id))
                    .test_support()
                    .text_sm()
                    .child(message),
            );
        }
    };
    let locked = preset.locked;
    for (index, rule) in rules.iter().enumerate() {
        let action = rule
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let tabs = crate::preset::rule_attribute_tabs(rule).join(" · ");
        let name = rule
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.trim().is_empty());
        let label = match name {
            Some(name) => format!("Rule {index} — \"{name}\" — {action} · {tabs}"),
            None => format!("Rule {index} — {action} · {tabs}"),
        };
        let mut row = div().flex().flex_row().items_center().gap_2().child(
            div()
                .id(format!("preset-rule-name-{}-{index}", preset.id))
                .test_support()
                .text_sm()
                .child(label),
        );
        if crate::preset::action_carries_outbound(action) {
            let outbound_text = match rule.get("outbound").and_then(serde_json::Value::as_str) {
                Some(tag) => format!("Outbound: {tag}"),
                None => "Outbound: serving row".to_string(),
            };
            row = row.child(
                div()
                    .id(format!("preset-rule-outbound-{}-{index}", preset.id))
                    .test_support()
                    .text_sm()
                    .child(outbound_text),
            );
            if !locked {
                row = row.child(
                    div()
                        .id(format!("preset-rule-exits-{}-{index}", preset.id))
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "Exits: {}",
                            crate::preset::preset_exit_suggestions(exits).join(" · ")
                        )),
                );
            }
        }
        if let Some(form) = &form {
            let cycle_view = view.clone();
            let cycle_form = form.clone();
            let remove_view = view.clone();
            let remove_form = form.clone();
            row = row
                .child(
                    Button::new(format!("preset-rule-action-{}-{index}", preset.id))
                        .small()
                        .label(format!("Action: {action}"))
                        .tooltip("Cycle this rule's action (direct → proxy → block)")
                        .on_click(move |_, window, cx| {
                            cycle_view.update(cx, |_, cx| {
                                AppView::rewrite_preset_rules_box(
                                    &cycle_form,
                                    &PresetRulesOp::CycleAction(index),
                                    window,
                                    cx,
                                );
                            });
                        }),
                )
                .child(
                    Button::new(format!("preset-rule-delete-{}-{index}", preset.id))
                        .small()
                        .label("Delete rule")
                        .tooltip("Remove this rule from the rules box")
                        .on_click(move |_, window, cx| {
                            remove_view.update(cx, |_, cx| {
                                AppView::rewrite_preset_rules_box(
                                    &remove_form,
                                    &PresetRulesOp::Remove(index),
                                    window,
                                    cx,
                                );
                            });
                        }),
                );
        }
        section = section.child(row);
        for fold in preset_rule_attribute_folds(&preset.id, index, rule) {
            section = section.child(fold);
        }
    }
    match form {
        Some(form) => {
            let add_view = view.clone();
            section.child(
                Button::new(format!("preset-rule-add-{}", preset.id))
                    .small()
                    .label("Add rule")
                    .tooltip("Append a fresh direct rule to the rules box")
                    .on_click(move |_, window, cx| {
                        add_view.update(cx, |_, cx| {
                            AppView::rewrite_preset_rules_box(
                                &form,
                                &PresetRulesOp::Add,
                                window,
                                cx,
                            );
                        });
                    }),
            )
        }
        None => section,
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FacetMenu {
    Group,
    Protocol,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FacetChoice {
    id: String,
    title: String,
    display: String,
    value: Option<String>,
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
struct FacetChoiceList(Vec<FacetChoice>);

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

fn slugify_group_id(group: &str) -> String {
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
    const ALL: [FacetMenu; 3] = [FacetMenu::Group, FacetMenu::Protocol, FacetMenu::Status];

    fn choices(&self, delegate: &LibraryDelegate) -> (Vec<FacetChoice>, Option<String>) {
        match self {
            FacetMenu::Group => (
                std::iter::once(FacetChoice {
                    id: "facet-group-all".to_string(),
                    title: "All groups".to_string(),
                    display: "Group: All".to_string(),
                    value: None,
                })
                .chain(
                    delegate
                        .group_options()
                        .into_iter()
                        .map(|(group, count)| FacetChoice {
                            id: format!("facet-group-{}", slugify_group_id(&group)),
                            title: format!("{group} ({count})"),
                            display: format!("Group: {group} ({count})"),
                            value: Some(group),
                        }),
                )
                .collect(),
                delegate.facet_group.clone(),
            ),
            FacetMenu::Protocol => (
                std::iter::once(FacetChoice {
                    id: "facet-protocol-all".to_string(),
                    title: "All protocols".to_string(),
                    display: "Protocol: All".to_string(),
                    value: None,
                })
                .chain(
                    delegate
                        .protocol_options()
                        .into_iter()
                        .map(|(protocol, count)| FacetChoice {
                            id: format!("facet-protocol-{protocol}"),
                            title: format!("{protocol} ({count})"),
                            display: format!("Protocol: {protocol} ({count})"),
                            value: Some(protocol),
                        }),
                )
                .collect(),
                delegate.facet_protocol.clone(),
            ),
            FacetMenu::Status => (
                std::iter::once(FacetChoice {
                    id: "facet-status-all".to_string(),
                    title: "All statuses".to_string(),
                    display: "Status: All".to_string(),
                    value: None,
                })
                .chain(
                    delegate
                        .status_options()
                        .into_iter()
                        .map(|(status, count)| {
                            let label = status.label().to_string();
                            FacetChoice {
                                id: format!("facet-status-{label}"),
                                title: format!("{label} ({count})"),
                                display: format!("Status: {label} ({count})"),
                                value: Some(label),
                            }
                        }),
                )
                .collect(),
                delegate
                    .facet_status
                    .map(|status| status.label().to_string()),
            ),
        }
    }

    fn current_display(&self, delegate: &LibraryDelegate) -> String {
        let (choices, current) = self.choices(delegate);
        choices
            .into_iter()
            .find(|choice| choice.value == current)
            .map(|choice| choice.display)
            .unwrap_or_else(|| "All".to_string())
    }

    fn apply(&self, view: &mut AppView, value: Option<String>, cx: &mut Context<AppView>) {
        view.libraries.update(cx, |state, list_cx| {
            let delegate = state.delegate_mut();
            match self {
                FacetMenu::Group => delegate.set_facet_group(value),
                FacetMenu::Protocol => delegate.set_facet_protocol(value),
                FacetMenu::Status => {
                    delegate
                        .set_facet_status(value.and_then(|label| FacetStatus::from_label(&label)));
                }
            }
            list_cx.notify();
        });
        cx.notify();
    }
}

struct FacetPicker {
    state: Entity<SelectState<FacetChoiceList>>,
    synced: Vec<FacetChoice>,
    _subscription: Subscription,
}

impl FacetPicker {
    fn new(
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProtocolChoice {
    id: String,
    title: String,
    display: String,
    value: String,
}

impl SearchableListItem for ProtocolChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone().into()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.display.clone().into_any_element())
    }

    fn value(&self) -> &String {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProtocolChoiceList(Vec<ProtocolChoice>);

impl SearchableListDelegate for ProtocolChoiceList {
    type Item = ProtocolChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&ProtocolChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        ProtocolChoice: SearchableListItem<Value = V>,
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
        item: &ProtocolChoice,
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

fn protocol_choices() -> Vec<ProtocolChoice> {
    VALID_PROTOCOLS
        .iter()
        .map(|protocol| ProtocolChoice {
            id: format!("protocol-option-{protocol}"),
            title: protocol.to_string(),
            display: format!("Protocol: {protocol}"),
            value: protocol.to_string(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelChoice {
    id: String,
    title: String,
    display: String,
    value: String,
}

impl SearchableListItem for PanelChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone().into()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.display.clone().into_any_element())
    }

    fn value(&self) -> &String {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PanelChoiceList(Vec<PanelChoice>);

impl SearchableListDelegate for PanelChoiceList {
    type Item = PanelChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&PanelChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        PanelChoice: SearchableListItem<Value = V>,
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
        item: &PanelChoice,
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

fn panel_choices(prefix: &str, items: &[(&str, &str)]) -> Vec<PanelChoice> {
    items
        .iter()
        .map(|(value, title)| {
            let slug = if value.trim().is_empty() {
                "default".to_string()
            } else {
                value.to_string()
            };
            PanelChoice {
                id: format!("{prefix}-{slug}"),
                title: title.to_string(),
                display: title.to_string(),
                value: value.to_string(),
            }
        })
        .collect()
}

fn flow_choices() -> Vec<PanelChoice> {
    panel_choices(
        "vless-flow-option",
        &[
            ("", "Default (no flow)"),
            ("xtls-rprx-vision", "xtls-rprx-vision"),
        ],
    )
}

fn packet_encoding_choices() -> Vec<PanelChoice> {
    panel_choices(
        "vless-packet-encoding-option",
        &[
            ("", "Default"),
            ("packetaddr", "packetaddr"),
            ("xudp", "xudp"),
        ],
    )
}

fn network_choices() -> Vec<PanelChoice> {
    panel_choices(
        "s1-network-option",
        &[
            ("tcp", "tcp"),
            ("ws", "ws"),
            ("httpupgrade", "httpupgrade"),
            ("http", "http"),
            ("grpc", "grpc"),
            ("quic", "quic"),
        ],
    )
}

fn fp_choices() -> Vec<PanelChoice> {
    panel_choices(
        "s2-fp-option",
        &[
            ("", "Default (no fingerprint)"),
            ("chrome", "chrome"),
            ("firefox", "firefox"),
            ("edge", "edge"),
            ("safari", "safari"),
            ("360", "360"),
            ("qq", "qq"),
            ("ios", "ios"),
            ("android", "android"),
            ("random", "random"),
            ("randomized", "randomized"),
        ],
    )
}

fn vmess_security_choices() -> Vec<PanelChoice> {
    use crate::store::VMESS_CIPHERS;
    let mut items: Vec<(&str, &str)> = Vec::with_capacity(VMESS_CIPHERS.len());
    for cipher in VMESS_CIPHERS {
        let title = if cipher == "auto" {
            "auto (default)"
        } else {
            cipher
        };
        items.push((cipher, title));
    }
    panel_choices("vmess-security-option", &items)
}

fn ss_method_choices() -> Vec<PanelChoice> {
    use crate::store::SS_METHODS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select method…")];
    for method in SS_METHODS {
        items.push((method, method));
    }
    panel_choices("ss-method-option", &items)
}

fn ss_plugin_choices() -> Vec<PanelChoice> {
    panel_choices(
        "ss-plugin-option",
        &[
            ("", "No plugin"),
            ("obfs-local", "obfs-local"),
            ("v2ray-plugin", "v2ray-plugin"),
        ],
    )
}

fn awg_version_choices() -> Vec<PanelChoice> {
    use crate::store::AWG_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version…")];
    for version in AWG_VERSIONS {
        items.push((version, version));
    }
    panel_choices("awg-version-option", &items)
}

fn hy_auth_type_choices() -> Vec<PanelChoice> {
    use crate::store::HY_AUTH_TYPES;
    let mut items: Vec<(&str, &str)> = vec![("", "Default")];
    for marker in HY_AUTH_TYPES {
        items.push((marker, marker));
    }
    panel_choices("hy-auth-type-option", &items)
}

fn tuic_cc_choices() -> Vec<PanelChoice> {
    use crate::store::TUIC_CC_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Select…")];
    for mode in TUIC_CC_MODES {
        items.push((mode, mode));
    }
    panel_choices("tuic-cc-option", &items)
}

fn tuic_relay_choices() -> Vec<PanelChoice> {
    use crate::store::TUIC_RELAY_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Select…")];
    for mode in TUIC_RELAY_MODES {
        items.push((mode, mode));
    }
    panel_choices("tuic-relay-option", &items)
}

fn socks_version_choices() -> Vec<PanelChoice> {
    use crate::store::SOCKS_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version… (blank = 5)")];
    for version in SOCKS_VERSIONS {
        items.push((version, version));
    }
    panel_choices("socks-version-option", &items)
}

fn socks_network_choices() -> Vec<PanelChoice> {
    panel_choices("socks-network-option", &[("tcp", "tcp"), ("udp", "udp")])
}

fn naive_cc_choices() -> Vec<PanelChoice> {
    use crate::store::NAIVE_CC_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Select…")];
    for mode in NAIVE_CC_MODES {
        items.push((mode, mode));
    }
    panel_choices("naive-cc-option", &items)
}

fn ssh_key_pick_error(path: &str) -> String {
    format!("SSH key file unreadable: {path} (check the path and pick again)")
}

fn snell_version_choices() -> Vec<PanelChoice> {
    use crate::store::SNELL_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version… (blank = 4)")];
    for version in SNELL_VERSIONS {
        items.push((version, version));
    }
    panel_choices("snell-version-option", &items)
}

fn shadowtls_version_choices() -> Vec<PanelChoice> {
    use crate::store::SHADOWTLS_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version… (blank = 1)")];
    for version in SHADOWTLS_VERSIONS {
        items.push((version, version));
    }
    panel_choices("shadowtls-version-option", &items)
}

fn xray_network_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_NETWORKS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select network…")];
    for network in XRAY_NETWORKS {
        items.push((network, network));
    }
    panel_choices("xray-network-option", &items)
}

fn xray_flow_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_FLOWS;
    let mut items: Vec<(&str, &str)> = vec![("", "Default (no flow)")];
    for flow in XRAY_FLOWS {
        items.push((flow, flow));
    }
    panel_choices("xray-flow-option", &items)
}

fn xray_mux_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_MUX_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Keep default")];
    for mode in XRAY_MUX_MODES {
        items.push((mode, mode));
    }
    panel_choices("xray-mux-option", &items)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChainLibraryEntry {
    id: String,
    name: String,
    protocol: String,
}

fn chain_pick_choices(library: &[ChainLibraryEntry]) -> Vec<PanelChoice> {
    let mut owned: Vec<(String, String)> = Vec::with_capacity(library.len() + 1);
    owned.push((String::new(), "Select a row…".to_string()));
    for entry in library {
        owned.push((
            entry.id.clone(),
            format!("{} ({})", entry.name, entry.protocol),
        ));
    }
    let items: Vec<(&str, &str)> = owned
        .iter()
        .map(|(value, title)| (value.as_str(), title.as_str()))
        .collect();
    panel_choices("chain-pick-option", &items)
}

fn chain_slot_kind(protocol: &str) -> ChainHopSlot {
    use crate::engine::{is_chain_protocol, is_extra_core_protocol};
    if protocol.trim().eq_ignore_ascii_case("autoselector") {
        return ChainHopSlot::Autoselector;
    }
    if is_chain_protocol(protocol) {
        ChainHopSlot::Chain
    } else if is_extra_core_protocol(protocol) {
        ChainHopSlot::Extra
    } else {
        ChainHopSlot::Other
    }
}

fn chain_existing_kinds(
    hops: &[serde_json::Value],
    library: &[ChainLibraryEntry],
) -> Vec<ChainHopSlot> {
    hops.iter()
        .map(|node| match node {
            serde_json::Value::String(id) => library
                .iter()
                .find(|entry| entry.id == id.trim())
                .map(|entry| chain_slot_kind(&entry.protocol))
                .unwrap_or(ChainHopSlot::Other),
            serde_json::Value::Object(map) => {
                let marker = map
                    .get("type")
                    .or_else(|| map.get("protocol"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                chain_slot_kind(marker)
            }
            _ => ChainHopSlot::Other,
        })
        .collect()
}

fn chain_hop_label(
    node: &serde_json::Value,
    index: usize,
    library: &[ChainLibraryEntry],
) -> String {
    match node {
        serde_json::Value::String(id) => {
            let id = id.trim();
            match library.iter().find(|entry| entry.id == id) {
                Some(entry) => format!("{}. {} ({})", index + 1, entry.name, entry.protocol),
                None => format!("{}. {id}", index + 1),
            }
        }
        serde_json::Value::Object(map) => {
            let marker = map
                .get("type")
                .or_else(|| map.get("protocol"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|marker| !marker.is_empty())
                .unwrap_or("outbound");
            format!("{}. {marker} (inline)", index + 1)
        }
        _ => format!("{}. hop", index + 1),
    }
}

struct WgPeerRefs<'a> {
    prefix: &'static str,
    private_key: &'a Entity<InputState>,
    public_key: &'a Entity<InputState>,
    pre_shared_key: &'a Entity<InputState>,
    reserved: &'a Entity<InputState>,
    keepalive: &'a Entity<InputState>,
    local_addr: &'a Entity<InputState>,
    dns: &'a Entity<InputState>,
    mtu: &'a Entity<InputState>,
    workers: &'a Entity<InputState>,
    sys_ifc: &'a Rc<RefCell<bool>>,
}

type EchoSelect<'a> = (
    &'a Entity<SelectState<PanelChoiceList>>,
    &'a [&'a str],
    &'a str,
);

fn generate_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now();
        let mut first = DefaultHasher::new();
        ("manyproxy-uuid-v1", std::process::id(), now, n).hash(&mut first);
        let mut second = DefaultHasher::new();
        ("manyproxy-uuid-v2", n, now, std::process::id()).hash(&mut second);
        let (a, b) = (first.finish().to_be_bytes(), second.finish().to_be_bytes());
        bytes[..8].copy_from_slice(&a);
        bytes[8..].copy_from_slice(&b);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let hex = |byte: u8| {
        [
            HEX[(byte >> 4) as usize] as char,
            HEX[(byte & 0x0f) as usize] as char,
        ]
    };
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.extend(hex(*byte));
    }
    out
}

fn generate_wireguard_key() -> String {
    use base64::Engine as _;
    let mut bytes = [0u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now();
        for (slot, tag) in bytes.chunks_mut(8).zip([
            "manyproxy-wg-1",
            "manyproxy-wg-2",
            "manyproxy-wg-3",
            "manyproxy-wg-4",
        ]) {
            let mut hasher = DefaultHasher::new();
            (tag, std::process::id(), now, n).hash(&mut hasher);
            slot.copy_from_slice(&hasher.finish().to_be_bytes());
        }
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteOp {
    Add,
    Import,
    StartSelected,
    StopSelected,
    TestSelected,
    CopyUrls,
    EditSelected,
    DeleteSelected,
    FocusSearch,
    ClearChecked,
    DetailsTab,
    HealthTab,
    PresetsTab,
    Subscriptions,
    PresetManager,
    Groups,
    Logs,
    RuntimeStats,
    TrafficStats,
    Settings,
    About,
    StartAll,
    StopAll,
    TestAll,
    ImportClipboard,
    ExportLibrary,
    RowColumns,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnRow {
    core: String,
    id: String,
    dest: String,
    row_id: Option<String>,
    rx_bytes: u64,
    tx_bytes: u64,
    delta_rx: u64,
    delta_tx: u64,
}

impl ConnRow {
    fn total(&self) -> u64 {
        self.rx_bytes.saturating_add(self.tx_bytes)
    }
}

/// The only two Connections-tab cells that click-to-copy: the destination cell
/// and the per-row exit endpoint (`bind:port`). Indexed by painted row order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnCopyTarget {
    Dest(usize),
    Endpoint(usize),
}

fn build_conn_rows(
    snapshot: &[(String, crate::health::ClashConn)],
    deltas: &HashMap<String, (u64, u64)>,
    selected_row: Option<&str>,
    selected_only: bool,
    desc: bool,
) -> Vec<ConnRow> {
    let mut rows: Vec<ConnRow> = snapshot
        .iter()
        .filter(
            |(_, entry)| match (selected_only, selected_row, entry.row_id.as_deref()) {
                (true, Some(selected), Some(row)) => row == selected,
                (true, Some(_), None) => false,
                _ => true,
            },
        )
        .map(|(core, entry)| {
            let key = format!("{core}/{}", entry.id);
            let (delta_rx, delta_tx) = deltas.get(&key).copied().unwrap_or((0, 0));
            ConnRow {
                core: core.clone(),
                id: entry.id.clone(),
                dest: entry.dest.clone(),
                row_id: entry.row_id.clone(),
                rx_bytes: entry.rx_bytes,
                tx_bytes: entry.tx_bytes,
                delta_rx,
                delta_tx,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        if desc {
            b.total().cmp(&a.total())
        } else {
            a.total().cmp(&b.total())
        }
    });
    rows
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExitTraffic {
    row_id: String,
    name: String,
    bind: String,
    rx_bytes: u64,
    tx_bytes: u64,
}

impl ExitTraffic {
    fn total(&self) -> u64 {
        self.rx_bytes.saturating_add(self.tx_bytes)
    }
}

/// How many per-exit rows the traffic dialog lists before rolling the tail up.
const TRAFFIC_TOP_EXITS: usize = 9;

/// One ranking for every place exits are shown: heaviest first, with totals
/// ties broken by stable identity so the same data always paints identically.
fn exit_traffic_order(a: &ExitTraffic, b: &ExitTraffic) -> std::cmp::Ordering {
    b.total()
        .cmp(&a.total())
        .then_with(|| a.row_id.cmp(&b.row_id))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.bind.cmp(&b.bind))
}

/// What the dialog lists: the top [`TRAFFIC_TOP_EXITS`] exits plus the count
/// and exact byte totals of every exit ranked below them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TrafficExitSplit {
    top: Vec<ExitTraffic>,
    other_exits: usize,
    other_rx: u64,
    other_tx: u64,
}

impl TrafficExitSplit {
    fn other_line(&self) -> Option<String> {
        (self.other_exits > 0)
            .then(|| traffic_other_line_text(self.other_exits, self.other_rx, self.other_tx))
    }
}

/// Ranks the exits, keeps the top nine and sums the rest. Recomputed from the
/// rows handed in, so every period paints its own split and nothing is stored.
fn traffic_top9_split(exits: &[ExitTraffic]) -> TrafficExitSplit {
    let mut ranked: Vec<ExitTraffic> = exits.to_vec();
    ranked.sort_by(exit_traffic_order);
    let split_at = ranked.len().min(TRAFFIC_TOP_EXITS);
    let rest = ranked.split_off(split_at);
    let mut other_rx = 0u64;
    let mut other_tx = 0u64;
    for exit in &rest {
        other_rx = other_rx.saturating_add(exit.rx_bytes);
        other_tx = other_tx.saturating_add(exit.tx_bytes);
    }
    TrafficExitSplit {
        top: ranked,
        other_exits: rest.len(),
        other_rx,
        other_tx,
    }
}

/// Bar denominator taken over every row (never the "Other" aggregate), so the
/// stacked bars keep truthful proportions once the tail is rolled up.
fn traffic_bar_max(exits: &[ExitTraffic]) -> u64 {
    exits
        .iter()
        .map(ExitTraffic::total)
        .max()
        .unwrap_or(1)
        .max(1)
}

fn conn_exit_of(rows: &[ProxyConfig], row_id: Option<&str>) -> (String, String, String) {
    match row_id.and_then(|id| rows.iter().find(|row| row.id == id)) {
        Some(row) => (
            row.name.clone(),
            format!("{}:{}", row.bind_ip, row.port),
            row.protocol.clone(),
        ),
        None => ("unclaimed".to_string(), "—".to_string(), "—".to_string()),
    }
}

fn aggregate_exit_traffic(
    rows: &[ProxyConfig],
    totals: &HashMap<String, (u64, u64)>,
) -> Vec<ExitTraffic> {
    let mut exits: Vec<ExitTraffic> = rows
        .iter()
        .filter_map(|row| {
            let (rx, tx) = totals.get(&row.id).copied().unwrap_or((0, 0));
            if rx == 0 && tx == 0 {
                return None;
            }
            Some(ExitTraffic {
                row_id: row.id.clone(),
                name: row.name.clone(),
                bind: format!("{}:{}", row.bind_ip, row.port),
                rx_bytes: rx,
                tx_bytes: tx,
            })
        })
        .collect();
    exits.sort_by(exit_traffic_order);
    exits
}

fn traffic_poll_due(tab: InspectorTab, now: i64, last_poll_at: i64) -> bool {
    if matches!(tab, InspectorTab::Connections | InspectorTab::Traffic) {
        return true;
    }
    now.saturating_sub(last_poll_at) >= TRAFFIC_TICK_SECS as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum InspectorTab {
    #[default]
    Details,
    Health,
    Presets,
    Logs,
    Connections,
    Traffic,
}

impl InspectorTab {
    fn index(self) -> usize {
        match self {
            InspectorTab::Details => 0,
            InspectorTab::Health => 1,
            InspectorTab::Presets => 2,
            InspectorTab::Logs => 3,
            InspectorTab::Connections => 4,
            InspectorTab::Traffic => 5,
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            1 => InspectorTab::Health,
            2 => InspectorTab::Presets,
            3 => InspectorTab::Logs,
            4 => InspectorTab::Connections,
            5 => InspectorTab::Traffic,
            _ => InspectorTab::Details,
        }
    }

    fn a11y_label(self) -> &'static str {
        match self {
            InspectorTab::Details => "Details",
            InspectorTab::Health => "Health",
            InspectorTab::Presets => "Presets",
            InspectorTab::Logs => "Logs",
            InspectorTab::Connections => "Connections",
            InspectorTab::Traffic => "Traffic",
        }
    }
}

// --- T-30: per-shard CPU/RAM sampling (read-only, own children only) ---------
//
// The sidecars are this app's own child processes, so reading their kernel
// counters is the user's own information: no enumeration, no network, no
// telemetry. Everything below is bounded by the pid list the controller hands
// over; nothing here can name a process the app did not spawn.

/// Samples kept per shard, one per engine tick (`ENGINE_POLL_TICK_SECS`), so
/// the ring spans five minutes of history and never grows.
const SHARD_SAMPLE_RING: usize = 60;

/// `PROCESS_QUERY_LIMITED_INFORMATION` — the least right that still answers
/// `GetProcessTimes` / `GetProcessMemoryInfo`.
#[cfg(all(windows, not(test)))]
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

/// `OpenProcess` says this when the pid exists but our token may not query it.
#[cfg(all(windows, not(test)))]
const ERROR_ACCESS_DENIED: u32 = 5;

/// One tick's raw counters for one shard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ProcessSample {
    /// Kernel + user CPU time in 100 ns units (`GetProcessTimes`).
    cpu_100ns: u64,
    /// `PROCESS_MEMORY_COUNTERS.WorkingSetSize` in bytes.
    ram_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleRead {
    Sampled(ProcessSample),
    /// Pid is gone (exited, or exited between ticks): drop it silently.
    Gone,
    /// The OS refused the handle (permission): say so instead of guessing.
    Denied,
}

/// The single seam between the sampler and Win32. Production points it at
/// [`win32_process_sample`]; tests inject canned readings, so `cargo test`
/// never opens a process handle.
type ProcessSampler = Box<dyn Fn(u32) -> SampleRead>;

/// CPU as a percentage of **one core** over `window`: `cpu delta / wall
/// time`. Both sides are in 100 ns units, so a sidecar that saturates one core
/// reads 100.0 and a multi-threaded one legitimately reads above it (each
/// extra core adds 100). `None` when there is no honest number to plot: an
/// empty window, or a counter that went backwards (a different process
/// generation took the pid).
fn cpu_percent_of_one_core(
    prev_cpu_100ns: u64,
    now_cpu_100ns: u64,
    window: Duration,
) -> Option<f32> {
    let window_100ns = window.as_nanos() / 100;
    if window_100ns == 0 {
        return None;
    }
    let delta = now_cpu_100ns.checked_sub(prev_cpu_100ns)?;
    Some((delta as f64 / window_100ns as f64 * 100.0) as f32)
}

/// CPU (% of one core) and RAM (MB) series for one shard, oldest first.
#[derive(Debug, Clone, Default, PartialEq)]
struct ShardRing {
    cpu: Vec<f32>,
    ram: Vec<f32>,
}

fn push_shard_sample(series: &mut Vec<f32>, value: f32) {
    if series.len() >= SHARD_SAMPLE_RING {
        series.remove(0);
    }
    series.push(value);
}

/// Fold one tick of readings into the rings.
///
/// - labels that are no longer live are evicted (shard died or was renamed);
/// - a changed pid is a respawn: its ring starts empty, and the old baseline
///   is gone, so no sample ever mixes two process generations;
/// - the first sample of a generation only primes the baseline (there is no
///   delta yet), so charts start on the second tick;
/// - a gone pid drops the shard's ring without a trace.
///
/// Returns the labels whose handle the OS refused on this tick.
fn fold_shard_samples(
    rings: &mut BTreeMap<String, ShardRing>,
    baselines: &mut BTreeMap<String, (u32, ProcessSample, Instant)>,
    live: &[(String, u32)],
    sampler: &dyn Fn(u32) -> SampleRead,
    at: Instant,
) -> HashSet<String> {
    rings.retain(|label, _| live.iter().any(|(name, _)| name == label));
    baselines.retain(|label, _| live.iter().any(|(name, _)| name == label));

    let mut denied = HashSet::new();
    for (label, pid) in live {
        let previous = baselines
            .get(label)
            .and_then(|(prev_pid, sample, prev_at)| {
                (*prev_pid == *pid).then_some((*sample, *prev_at))
            });
        if baselines.contains_key(label) && previous.is_none() {
            rings.insert(label.clone(), ShardRing::default());
            baselines.remove(label);
        }
        match sampler(*pid) {
            SampleRead::Sampled(sample) => {
                let ring = rings.entry(label.clone()).or_default();
                if let Some((prev_sample, prev_at)) = previous {
                    let window = at.saturating_duration_since(prev_at);
                    if let Some(cpu) =
                        cpu_percent_of_one_core(prev_sample.cpu_100ns, sample.cpu_100ns, window)
                    {
                        push_shard_sample(&mut ring.cpu, cpu);
                        push_shard_sample(&mut ring.ram, sample.ram_bytes as f32 / 1_048_576.0);
                    }
                }
                baselines.insert(label.clone(), (*pid, sample, at));
            }
            SampleRead::Gone => {
                rings.remove(label);
                baselines.remove(label);
            }
            SampleRead::Denied => {
                denied.insert(label.clone());
            }
        }
    }
    denied
}

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

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        match Store::open_file(&Store::default_path()) {
            Ok(store) => Self::with_store(window, cx, store),
            Err(error) => {
                let mut view = Self::with_store(
                    window,
                    cx,
                    Store::open_in_memory().expect("in-memory store opens"),
                );
                view.notify_after_root(
                    Notification::warning(format!("Running without persistence: {error}")),
                    window,
                    cx,
                );
                view
            }
        }
    }

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

    fn open_alert_dialog_after_root<F>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: F,
    ) where
        F: Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static,
    {
        self.after_root(window, cx, move |_view, window, cx| {
            open_app_alert_dialog(window, cx, build);
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

    fn with_store(window: &mut Window, cx: &mut Context<Self>, store: Store) -> Self {
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
        let split_state = cx.new(|_| ResizableState::default());
        let view = Self {
            focus: cx.focus_handle(),
            libraries: libraries.clone(),
            detail_scroll: ScrollHandle::new(),
            split_state,
            inspector_drawer_open: true,
            store,
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

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notify_engine_crash(window, cx);
        // `has_active_dialog` / `has_active_sheet` / `close_sheet` all read the
        // Root layer, which the window only adopts after the boot closure
        // returns — ask only once a Root is actually installed.
        let root_ready = Self::root_installed(window);
        // T-19: an edit dialog that left no dialog active anywhere is gone —
        // stop protecting its row from auto-clear.
        if self.edit_form_open.is_some() && root_ready && !window.has_active_dialog(cx) {
            self.edit_form_open = None;
        }
        if let Err(error) = self.reload_inner_no_poll(cx) {
            // Reached during boot (restore_on_launch -> commit_restore_subset)
            // as well as from ordinary clicks: `notify_now_or_after_root`
            // keeps the post-Root timing and defers only before Root exists.
            self.notify_now_or_after_root(
                Notification::error(db_list_error_message(&error)),
                window,
                cx,
            );
        }

        if self.logs_open
            && self.log_tail
            && self.logs.push_seq() != self.logs_seen
            && root_ready
            && window.has_active_sheet(cx)
        {
            self.log_reopen_draft = self.log_input_draft(cx);
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs_open = false;
            self.open_logs_dialog(window, cx);
            return;
        }
        cx.notify();
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

    fn report_save_error(&self, error: StoreError, window: &mut Window, cx: &mut App) {
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

    fn open_edit_for_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_edit_dialog(config, window, cx);
    }

    fn open_remove_for_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_remove_confirm(config, window, cx);
    }

    fn open_traffic_reset_for_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_traffic_reset_confirm(config, window, cx);
    }

    fn open_usage_reset_for_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        self.open_usage_reset_confirm(config, window, cx);
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

    fn open_traffic_reset_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let totals = self
            .store
            .traffic_totals()
            .ok()
            .and_then(|map| map.get(&config.id).copied())
            .unwrap_or((0, 0));
        let description = format!(
            "Reset traffic counters for \"{}\"? Current totals: ↓{} ↑{}. \
             This zeroes the stored counters.",
            config.name,
            format_bytes(totals.0),
            format_bytes(totals.1)
        );
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Reset")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset traffic")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_reset_traffic(&id, window, cx));
                    true
                })
        });
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

    fn open_clear_traffic_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let description =
            settings_clear_traffic_text(self.libraries.read(cx).delegate().counts().0);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Clear")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Clear traffic counters")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_clear_traffic(window, cx));
                    true
                })
        });
    }

    fn open_clear_health_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let description = settings_clear_health_text();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Clear")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Clear health snapshots")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_clear_health(window, cx));
                    true
                })
        });
    }

    fn open_clear_snapshot_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let description = settings_clear_snapshot_text();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Reset")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset running snapshot")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_clear_snapshot(window, cx));
                    true
                })
        });
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

    fn open_share_for_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let link =
            crate::import::share_link_for_config(&config.protocol, &config.name, &config.params);
        let title = format!("Share \"{}\"", config.name);
        let commit: DialogCommit = Rc::new(move |window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
            window.push_notification(Notification::success("Copied share link"), cx);
            true
        });
        let commit_key = commit.clone();
        let dialog_link =
            crate::import::share_link_for_config(&config.protocol, &config.name, &config.params);
        let payloads = share_qr_payloads(&config);
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(title.clone())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div().id("share-warning").test_support().child(
                                Alert::error("share-warning-box", SHARE_WARNING.to_string())
                                    .title("Share links carry credentials"),
                            ),
                        )
                        .child(
                            div()
                                .id("share-link-text")
                                .test_support()
                                .text_sm()
                                .child(dialog_link.clone()),
                        )
                        // T-28: the credential-free contact strings, one QR per scheme,
                        // as a transcription of the same text (never of the share link).
                        .child(share_qr_region(&payloads, cx)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "share-dialog-ok",
                    "share-dialog-cancel",
                    footer_commit_label("share"),
                    commit_click,
                ))
        });
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

    fn open_lan_confirm_single(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let description = lan_confirm_text(&config.name, &config.bind_ip);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Start"),
                )
                .title(LAN_CONFIRM_TITLE)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_start_selected(&id, window, cx));
                    true
                })
        });
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

    fn create_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match crate::tray::TrayHandle::build() {
            Ok(tray) => {
                tray.set_running(self.engine.running_count());
                self.tray = Some(tray);
                cx.notify();
            }
            Err(error) => {
                self.notify_after_root(
                    Notification::warning(format!(
                        "Tray unavailable ({error}) — closing still confirms"
                    )),
                    window,
                    cx,
                );
            }
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

    fn open_tray_exit_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_preset_chooser_for_selected(window, cx);
    }

    fn open_preset_chooser_for_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.selected_config(cx) {
            Some(config) => self.open_preset_attach_dialog(config, window, cx),
            None => window.push_notification(Notification::info(no_selection_message()), cx),
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

    fn restore_on_launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.store.restore_running() {
            return;
        }
        let snapshot = self.store.load_running_snapshot();
        if snapshot.is_empty() {
            return;
        }
        let rows = match self.store.list() {
            Ok(rows) => rows,
            Err(error) => {
                self.notify_after_root(
                    Notification::error(db_list_error_message(&error)),
                    window,
                    cx,
                );
                return;
            }
        };
        let (now, gated) = crate::tray::partition_restore(&rows, &snapshot);
        if !now.is_empty() {
            self.commit_restore_subset(&now, window, cx);
        }
        if !gated.is_empty() {
            self.open_restore_lan_confirm(gated, window, cx);
        }
    }

    fn commit_restore_subset(
        &mut self,
        rows: &[ProxyConfig],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(reason) = self.sync_presets_to_engine() {
            self.notify_after_root(
                Notification::error(format!("Cannot restore: {reason}")),
                window,
                cx,
            );
            return;
        }
        let mut started = 0usize;
        let mut failed: Vec<String> = Vec::new();
        for row in rows {
            match self.engine.start_one_confirmed(row) {
                Ok(()) => {
                    if self.store.set_enabled(&row.id, true).is_ok() {
                        started += 1;
                    } else {
                        failed.push(format!("\"{}\"", row.name));
                    }
                }
                Err(_) => failed.push(format!("\"{}\"", row.name)),
            }
        }
        self.reload(window, cx);
        if failed.is_empty() {
            self.notify_after_root(
                Notification::success(format!("Restored {started} endpoint(s)")),
                window,
                cx,
            );
        } else {
            self.notify_after_root(
                Notification::warning(start_all_partial_message(started, &failed)),
                window,
                cx,
            );
        }
    }

    fn open_restore_lan_confirm(
        &mut self,
        rows: Vec<ProxyConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let lan: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.name.clone(), row.bind_ip.clone()))
            .collect();
        let description = format!(
            "{}\n\nThese were running before; confirm to restart them.",
            lan_confirm_description(&lan)
        );
        self.open_alert_dialog_after_root(window, cx, move |alert, _, _| {
            let view = view.clone();
            let rows = rows.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Restart"),
                )
                .title(LAN_CONFIRM_TITLE)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_restore_subset(&rows, window, cx));
                    true
                })
        });
    }

    fn open_runtime_stats_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let uptime = format_uptime(self.started_at.elapsed().as_secs());
        let running = self.engine.running_count();
        let rows = self.store.list().unwrap_or_default();
        let total = rows.len();
        let running_binds: Vec<String> = rows
            .iter()
            .filter(|row| self.engine.is_running(&row.id))
            .map(|row| format!("{}:{}", row.bind_ip, row.port))
            .collect();
        let traffic_line =
            summed_traffic_text(&self.store).unwrap_or_else(|| "usage unavailable".to_string());
        let conns = self.conn_snapshot.len();
        let (delta_rx, delta_tx) = self
            .conn_delta
            .values()
            .fold((0u64, 0u64), |(rx, tx), (d_rx, d_tx)| {
                (rx.saturating_add(*d_rx), tx.saturating_add(*d_tx))
            });
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let now = now_unix();
        let due = subs
            .iter()
            .filter(|sub| due_for_refresh_for(sub, now))
            .count();
        let subs_line = if self.store.subs_auto_refresh_enabled() {
            format!("{} subscription(s), {due} due", subs.len())
        } else {
            format!(
                "{} subscription(s), {due} due (auto-refresh off)",
                subs.len()
            )
        };
        let db_line = self
            .store
            .db_path()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| format_bytes(meta.len()))
            .unwrap_or_else(|| "unknown".to_string());
        let selected_line = match self.selected_config(cx) {
            Some(config) => {
                let health = self.health_store.row_summary(&config.id);
                let health = health.trim();
                if health.is_empty() {
                    format!("{} (no probes this session)", config.name)
                } else {
                    format!("{} ({health})", config.name)
                }
            }
            None => "none selected".to_string(),
        };
        let shard_rows = shard_stat_rows(
            &self.engine.shard_processes(),
            &self.shard_rings,
            &self.shard_denied,
        );
        let mut lines = vec![
            format!("Uptime: {uptime}"),
            format!("Endpoints: {running} running / {total} stored"),
            format!(
                "Running: {}",
                if running_binds.is_empty() {
                    "—".to_string()
                } else {
                    running_binds.join(", ")
                }
            ),
            format!("Traffic: {traffic_line}"),
            format!("Connections: {conns} live"),
            format!(
                "Session Δ: ↓{} ↑{}",
                format_bytes(delta_rx),
                format_bytes(delta_tx)
            ),
            format!("Subscriptions: {subs_line}"),
            format!("Database: {db_line}"),
            format!("Selected: {selected_line}"),
        ];
        lines.push(format!("Processes: {}", shard_stats_summary(&shard_rows)));
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let mut body = div().flex().flex_col().gap_1();
            for (ix, line) in lines.iter().enumerate() {
                body = body.child(
                    div()
                        .id(format!("runtime-stat-{ix}"))
                        .test_support()
                        .text_sm()
                        .child(line.clone()),
                );
            }
            body =
                body.child(
                    div()
                        .id("runtime-shard-stats")
                        .test_support()
                        .mt_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_xs().child(
                            "Sidecar processes — CPU % of one core, RAM in MB, last 60 ticks",
                        )),
                );
            for row in shard_rows.iter() {
                body = body.child(shard_stat_row_view(row));
            }
            dialog
                .title("Runtime stats")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "runtime-stats-ok",
                    "runtime-stats-cancel",
                    footer_commit_label("runtime-stats"),
                    Rc::new(|_, _| true),
                ))
        });
    }

    fn open_traffic_stats_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let period = self.traffic_period;
        let rows = self.store.list().unwrap_or_default();
        let totals = self.store.traffic_totals().unwrap_or_default();
        let exits = aggregate_exit_traffic(&rows, &totals);
        let (exit_count, rx, tx) = store::Store::traffic_period_summary(&totals, period);
        let summary = traffic_stats_summary_text(period, exit_count, rx, tx);
        let split = traffic_top9_split(&exits);
        let max = traffic_bar_max(&exits);
        let cycle_view = view.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let cycle_view = cycle_view.clone();
            let mut body = div().flex().flex_col().gap_1();
            body = body.child(
                div()
                    .id("traffic-stats-summary")
                    .test_support()
                    .text_sm()
                    .child(summary.clone()),
            );
            body = body.child(
                Button::new("traffic-stats-period")
                    .small()
                    .label(format!("Period: {}", period.label()))
                    .tooltip("Cycle 24h → 7d → 30d → 90d (all-time totals throughout)")
                    .on_click(move |_, window, cx| {
                        cycle_view.update(cx, |view, cx| {
                            let next = match view.traffic_period {
                                store::TrafficPeriod::Day => store::TrafficPeriod::Week,
                                store::TrafficPeriod::Week => store::TrafficPeriod::Month,
                                store::TrafficPeriod::Month => store::TrafficPeriod::Quarter,
                                store::TrafficPeriod::Quarter => store::TrafficPeriod::Day,
                            };
                            view.traffic_period = next;
                            window.close_all_dialogs(cx);
                            reclaim_shell_focus(window, cx);
                            view.open_traffic_stats_dialog(window, cx);
                        });
                    }),
            );
            if exits.is_empty() {
                body = body.child(
                    div()
                        .id("traffic-stats-empty")
                        .test_support()
                        .text_sm()
                        .child("No traffic recorded yet — start an exit and pass data."),
                );
            }
            for exit in &split.top {
                body = body.child(
                    div()
                        .id(format!("traffic-stat-{}", exit.row_id))
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "{} {} · {} ↓{} ↑{}",
                            traffic_stacked_bar(exit.rx_bytes, exit.tx_bytes, max),
                            exit.name,
                            exit.bind,
                            format_bytes(exit.rx_bytes),
                            format_bytes(exit.tx_bytes),
                        )),
                );
            }
            if let Some(other) = split.other_line() {
                body = body.child(
                    div()
                        .id("traffic-stats-other")
                        .test_support()
                        .text_sm()
                        .child(other),
                );
            }
            dialog
                .title("Traffic stats")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "traffic-stats-ok",
                    "traffic-stats-cancel",
                    footer_commit_label("traffic-stats"),
                    Rc::new(|_, _| true),
                ))
        });
    }

    fn open_about_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sidecars = self.sidecar_presence.clone();
        let about_view = cx.entity();
        let body = about_body(
            APP_VERSION,
            GIT_HASH,
            &sidecars,
            &Store::default_path().display().to_string(),
            &Store::default_export_path().display().to_string(),
        );
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let check_view = about_view.clone();
            dialog
                .title(format!("About {APP_TITLE}"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .id("about-body")
                                .test_support()
                                .text_sm()
                                .child(body.clone()),
                        )
                        .child(
                            Button::new("about-check-for-updates")
                                .small()
                                .label("Check for updates")
                                .tooltip("Fetch the release feed once (JSON only — nothing downloads or runs)")
                                .on_click(move |_, window, cx| {
                                    window.close_all_dialogs(cx);
                                    reclaim_shell_focus(window, cx);
                                    check_view.update(cx, |view, cx| {
                                        view.check_for_updates(window, cx);
                                    });
                                }),
                        ),
                )
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "about-dialog-ok",
                    "about-dialog-cancel",
                    "Close",
                    Rc::new(|_, _| true),
                ))
        });
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

    fn open_update_check_dialog(
        &mut self,
        outcome: UpdateCheckOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, message, release_name, release_url) =
            update_check_presentation(&outcome, APP_VERSION);
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let mut body = div().flex().flex_col().gap_1();
            body = body.child(
                div()
                    .id(id.to_string())
                    .test_support()
                    .text_sm()
                    .child(message.clone()),
            );
            if let Some(name) = release_name.clone() {
                body = body.child(
                    div()
                        .id("update-release-name")
                        .test_support()
                        .text_sm()
                        .child(name),
                );
            }
            if let Some(url) = release_url.clone() {
                body = body.child(
                    Button::new("update-open-releases")
                        .small()
                        .label("Open releases in browser")
                        .tooltip(format!(
                            "Opens {url} in your own browser — the app downloads nothing"
                        ))
                        .on_click(move |_, window, cx| {
                            if let Err(why) = open_release_in_browser(&url) {
                                window.push_notification(Notification::error(why), cx);
                            }
                        }),
                );
            }
            dialog
                .title("Check for updates")
                .child(body)
                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "update-dialog-ok",
                    "update-dialog-cancel",
                    "Close",
                    Rc::new(|_, _| true),
                ))
        });
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

    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if overlay_open(window, cx) {
            return;
        }
        let state = cx.new(|cx| CommandState::new(window, cx));
        self.palette = Some(state.clone());
        let view = cx.entity();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            dialog.title("Command palette").child(
                Command::new(&state)
                    .placeholder("Type a command…")
                    .group(CommandGroup::new().label("Library").items(vec![
                            CommandItem::new()
                                .label("Add configuration")
                                .keywords(["new", "create"]),
                            CommandItem::new()
                                .label("Import configurations")
                                .keywords(["import", "link", "file"]),
                            CommandItem::new()
                                .label("Start selected")
                                .keywords(["start", "serve", "f5"]),
                            CommandItem::new().label("Stop selected").keywords(["stop"]),
                            CommandItem::new()
                                .label("Test selected")
                                .keywords(["test", "probe", "health"]),
                            CommandItem::new().label("Copy URLs").keywords([
                                "copy",
                                "clipboard",
                                "url",
                                "endpoint",
                            ]),
                            CommandItem::new()
                                .label("Edit selected")
                                .keywords(["edit", "rename"]),
                            CommandItem::new()
                                .label("Delete selected")
                                .keywords(["delete", "remove"]),
                            CommandItem::new()
                                .label("Focus search field")
                                .keywords(["search", "find", "filter"]),
                            CommandItem::new()
                                .label("Clear selection")
                                .keywords(["clear", "escape", "deselect"]),
                            CommandItem::new()
                                .label("Show Details tab")
                                .keywords(["details", "tab"]),
                            CommandItem::new()
                                .label("Show Health tab")
                                .keywords(["health", "tab"]),
                            CommandItem::new()
                                .label("Show Presets tab")
                                .keywords(["preset", "tab"]),

                            CommandItem::new()
                                .label("Import from clipboard")
                                .keywords(["clipboard", "paste", "import"]),
                            CommandItem::new()
                                .label("Export library")
                                .keywords(["export", "backup", "json"]),
                        ]))
                    .group(CommandGroup::new().label("Manage").items(vec![
                            CommandItem::new()
                                .label("Subscriptions")
                                .keywords(["subscription", "refresh"]),
                            CommandItem::new()
                                .label("Routing presets")
                                .keywords(["preset", "routing", "rules"]),
                            CommandItem::new()
                                .label("Groups")
                                .keywords(["group", "move"]),
                            CommandItem::new().label("Logs").keywords(["log", "output"]),
                            CommandItem::new()
                                .label("Settings")
                                .keywords(["settings", "options", "test url"]),
                            CommandItem::new().label("About").keywords([
                                "about",
                                "version",
                                "shortcuts",
                            ]),
                            CommandItem::new()
                                .label("Start all")
                                .keywords(["start", "all", "serve"]),
                            CommandItem::new()
                                .label("Stop all")
                                .keywords(["stop", "all"]),
                            CommandItem::new()
                                .label("Test all")
                                .keywords(["test", "all", "probe"]),

                            CommandItem::new()
                                .label("Runtime stats")
                                .keywords(["runtime", "stats", "health", "cpu", "memory"]),
                            CommandItem::new()
                                .label("Traffic stats")
                                .keywords(["traffic", "stats", "usage", "profile"]),
                            CommandItem::new()
                                .label("Row columns")
                                .keywords(["column", "columns", "rows", "display"]),
                        ]))
                    .on_confirm(move |index, window, cx| {
                        view.update(cx, |view, cx| view.run_palette_op(index, window, cx));
                    }),
            )
        });
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

    fn resolve_out_ip_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.resolve_out_ip_rows(vec![config], None, Some(window.window_handle()), cx);
    }

    fn resolve_out_ip_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.selected_config(cx) else {
            window.push_notification(Notification::info(no_selection_message()), cx);
            return;
        };
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.resolve_out_ip_rows(vec![config], None, Some(window.window_handle()), cx);
    }

    fn resolve_out_ip_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    "Group \"{group}\" has no running endpoints to resolve"
                )),
                cx,
            );
            return;
        }
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.resolve_out_ip_rows(
            rows,
            Some(format!("in group \"{group}\"")),
            Some(window.window_handle()),
            cx,
        );
    }

    fn resolve_out_ip_rows(
        &mut self,
        rows: Vec<ProxyConfig>,
        scope: Option<String>,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        use std::sync::atomic::Ordering;
        if rows.is_empty() || self.group_test_in_flight {
            return;
        }
        let scope = scope.unwrap_or_else(|| {
            rows.first()
                .map(|row| format!("of \"{}\"", row.name))
                .unwrap_or_default()
        });
        self.group_test_in_flight = true;
        self.probe_stop.store(false, Ordering::SeqCst);
        let stop = self.probe_stop.clone();
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let (resolved, prober) = cx
                .background_executor()
                .spawn(async move {
                    let prober = prober;
                    let mut ips = Vec::new();
                    let mut failed = Vec::new();
                    for row in &rows {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        if prober.supports(row) {
                            match prober.out_ip(row, &row.bind_ip, row.port, RESOLVE_IP_URL) {
                                // T-27: keep the row association so the `out_ip`
                                // sort key has per-row text to compare.
                                Ok(ip) => ips.push((row.id.clone(), ip)),
                                Err(cause) => failed.push(format!("\"{}\": {cause}", row.name)),
                            }
                        } else {
                            failed.push(format!(
                                "\"{}\": unsupported protocol for out-ip resolve",
                                row.name
                            ));
                        }
                    }
                    ((ips, failed), prober)
                })
                .await;
            cx.update(|cx| {
                let done = weak.update(cx, |view, _| {
                    view.prober = prober;
                    view.group_test_in_flight = false;
                });
                if done.is_err() {
                    return;
                }
                let Some(view) = weak.upgrade() else {
                    return;
                };
                let (ips, failed) = resolved;
                let message = out_ip_summary_toast(
                    &scope,
                    &ips.iter().map(|(_, ip)| ip.clone()).collect::<Vec<_>>(),
                    &failed,
                );
                view.update(cx, |view, _| {
                    for (id, ip) in &ips {
                        view.out_ip_by_row.insert(id.clone(), ip.clone());
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

    fn resolve_domain_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning("Already removed"), cx);
            return;
        };
        let Some(host) = row_server_hostname(&config.params) else {
            window.push_notification(
                Notification::info(format!(
                    "\"{}\" has no domain server to resolve",
                    config.name
                )),
                cx,
            );
            return;
        };
        if self.group_test_in_flight {
            window.push_notification(Notification::info("A group test is already running"), cx);
            return;
        }
        self.group_test_in_flight = true;
        let handle = window.window_handle();
        let weak = cx.entity().downgrade();
        let id = config.id.clone();
        let name = config.name.clone();
        let params = config.params.clone();
        cx.spawn(async move |_entity, cx| {
            let lookup = host.clone();
            let found = cx
                .background_executor()
                .spawn(async move { resolve_hostname(&lookup, RESOLVE_TIMEOUT) })
                .await;
            cx.update(|cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.group_test_in_flight = false;
                        let Some(ip) = found else {
                            window.push_notification(
                                Notification::warning(format!(
                                    "Could not resolve \"{name}\" — server unchanged"
                                )),
                                cx,
                            );
                            return;
                        };
                        let Some(rewritten) = rewrite_server_to_ip(&params, &ip.to_string()) else {
                            window.push_notification(
                                Notification::warning(format!(
                                    "\"{name}\" has no domain server to resolve"
                                )),
                                cx,
                            );
                            return;
                        };
                        let patch = ConfigPatch {
                            params: Some(rewritten),
                            ..Default::default()
                        };
                        match view.store.update(&id, patch) {
                            Ok(_) => {
                                let running = view.engine.is_running(&id);
                                view.reload(window, cx);
                                window.push_notification(
                                    Notification::success(resolve_domain_row_message(
                                        &name,
                                        &ip.to_string(),
                                        &host,
                                        running,
                                    )),
                                    cx,
                                );
                            }
                            Err(error) => {
                                window
                                    .push_notification(Notification::error(error.to_string()), cx);
                            }
                        }
                    });
                });
            });
        })
        .detach();
    }

    fn open_resolve_domain_group_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let candidates: Vec<(String, String, String)> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == group)
            .filter_map(|row| {
                row_server_hostname(&row.params)
                    .map(|host| (row.id.clone(), row.name.clone(), host))
            })
            .collect();
        if candidates.is_empty() {
            window.push_notification(
                Notification::info(format!(
                    "Group \"{group}\" has no domain servers to resolve"
                )),
                cx,
            );
            return;
        }
        let names: Vec<String> = candidates.iter().map(|(_, name, _)| name.clone()).collect();
        let description = resolve_domain_confirm_text(&group, &names);
        let view = cx.entity();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let candidates = candidates.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Rewrite")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(format!("Resolve domains in group \"{group}\"?"))
                .description(description)
                .on_ok(move |_, window, cx| {
                    let candidates = candidates.clone();
                    view.update(cx, |view, cx| {
                        view.commit_resolve_domain_group(&candidates, window, cx)
                    });
                    true
                })
        });
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

    fn open_prune_confirm(
        &mut self,
        kind: GroupPruneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            GroupPruneKind::ALL.contains(&kind),
            "prune kinds stay the pinned four"
        );
        let Some(group) = self.current_group(cx) else {
            window.push_notification(
                Notification::info("No group in focus — select a row or filter by group"),
                cx,
            );
            return;
        };
        let (title, description, ok) = match kind {
            GroupPruneKind::Duplicates => {
                let (total, preview) = self
                    .store
                    .duplicate_preview_in_group(&group)
                    .unwrap_or((0, Vec::new()));
                if total == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no duplicates")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Remove {total} duplicate(s) in group \"{group}\"?"),
                    prune_duplicates_confirm_text(&group, total, &preview),
                    "Remove",
                )
            }
            GroupPruneKind::Unavailable => {
                let count = self
                    .store
                    .unavailable_rows_in_group(&group)
                    .map(|rows| rows.len())
                    .unwrap_or(0);
                if count == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no unavailable rows")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Remove {count} unavailable row(s) in group \"{group}\"?"),
                    prune_unavailable_confirm_text(&group, count),
                    "Remove",
                )
            }
            GroupPruneKind::Invalid => {
                let count = self
                    .store
                    .invalid_rows_in_group(&group)
                    .map(|rows| rows.len())
                    .unwrap_or(0);
                if count == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no invalid rows")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Remove {count} invalid row(s) in group \"{group}\"?"),
                    prune_invalid_confirm_text(&group, count),
                    "Remove",
                )
            }
            GroupPruneKind::ClearTests => {
                let count = self.group_test_sample_count(&group);
                if count == 0 {
                    window.push_notification(
                        Notification::info(format!("Group \"{group}\" has no test results")),
                        cx,
                    );
                    return;
                }
                (
                    format!("Clear {count} test result(s) in group \"{group}\"?"),
                    clear_group_tests_confirm_text(&group, count),
                    "Clear",
                )
            }
        };
        let view = cx.entity();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let title = title.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text(ok)
                        .ok_variant(ButtonVariant::Danger),
                )
                .title(title)
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_prune(kind, window, cx));
                    true
                })
        });
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

    /// T-19: toggling auto-clear on opens this note once per group; the sheet
    /// origin re-opens the sheet on every exit so the switch re-reads the
    /// store, the facet origin just lets the window redraw revert visuals.
    fn open_auto_clear_opt_in_confirm(
        &mut self,
        group: &str,
        from_groups_sheet: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let group = group.to_string();
        let title = format!("Enable auto-clear in group \"{group}\"?");
        let description = auto_clear_opt_in_note(&group);
        focus_shell_before_overlay(window, cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            let group = group.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Enable"),
                )
                .title(title.clone())
                .description(description.clone())
                .on_ok({
                    let view = view.clone();
                    let group = group.clone();
                    move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.commit_auto_clear_opt_in(&group, window, cx);
                            if from_groups_sheet {
                                window.close_all_dialogs(cx);
                                reclaim_shell_focus(window, cx);
                                view.open_groups_dialog(window, cx);
                            }
                        });
                        !from_groups_sheet
                    }
                })
                .on_cancel({
                    let view = view.clone();
                    move |_, window, cx| {
                        if !from_groups_sheet {
                            return true;
                        }
                        view.update(cx, |exposed, cx| {
                            window.close_all_dialogs(cx);
                            reclaim_shell_focus(window, cx);
                            exposed.open_groups_dialog(window, cx);
                        });
                        false
                    }
                })
                .on_close({
                    let view = view.clone();
                    move |_, window, cx| {
                        if !from_groups_sheet {
                            return;
                        }
                        view.update(cx, |exposed, cx| {
                            window.close_all_dialogs(cx);
                            exposed.open_groups_dialog(window, cx);
                        });
                        reclaim_shell_focus(window, cx);
                    }
                })
        });
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

    fn open_logs_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A control-driven swap hands over the live draft; otherwise seed from
        // the committed filter fields.
        let (current, current_exclude, current_pattern) =
            self.log_reopen_draft.take().unwrap_or_else(|| {
                (
                    self.log_filter.clone(),
                    self.log_exclude.clone(),
                    self.log_regex.clone(),
                )
            });
        let filter_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Filter logs")
                .default_value(current)
        });

        let exclude_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Exclude keyword")
                .default_value(current_exclude)
        });
        let pattern_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Regex (empty = off)")
                .default_value(current_pattern)
        });
        self.log_inputs = Some((
            filter_state.clone(),
            exclude_state.clone(),
            pattern_state.clone(),
        ));
        let view = cx.entity();
        let reader = filter_state.clone();
        let exclude_reader = exclude_state.clone();
        let pattern_reader = pattern_state.clone();
        let lines = self.log_lines(cx);

        self.log_list = Some(cx.new(|cx| {
            ListState::new(
                LogLinesDelegate {
                    lines: lines.clone(),
                },
                window,
                cx,
            )
            .selectable(false)
        }));
        let log_list_state = self.log_list.clone();
        let title = match self.selected_config(cx) {
            Some(config) => format!("Logs — {}", config.name),
            None => "Logs".to_string(),
        };

        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |_window, cx| {
            let filter = reader.read(cx).value().to_string();
            let exclude = exclude_reader.read(cx).value().to_string();
            let pattern = pattern_reader.read(cx).value().to_string();
            commit_view.update(cx, |view, cx| {
                view.apply_log_filters(filter, exclude, pattern, cx);
            });
            true
        });

        self.logs_open = true;
        self.logs_seen = self.logs.push_seq();

        let chrome_view = view.clone();
        let chrome_level = self.log_level;
        let chrome_tail = self.log_tail;

        let chrome_filter_active = !self.log_filter.trim().is_empty();
        focus_shell_before_overlay(window, cx);
        window.open_sheet(cx, move |sheet: Sheet, _, _| {
            let commit_click = commit.clone();

            let (current_level, tail_on) = (chrome_level, chrome_tail);

            const LOG_LEVEL_LABELS: [&str; 5] = ["All", "Debug", "Info", "Warn", "Error"];
            let selected_level = LOG_LEVEL_LABELS
                .iter()
                .position(|label| LogLevel::from_label(label) == current_level);
            let mut level_group = RadioGroup::horizontal("log-level-options")
                .selected_index(selected_level)
                .on_change({
                    let group_view = chrome_view.clone();
                    move |ix, window, cx| {
                        let level = LOG_LEVEL_LABELS
                            .get(*ix)
                            .map(|label| LogLevel::from_label(label))
                            .unwrap_or(current_level);
                        let level_view = group_view.clone();
                        level_view.update(cx, |view, cx| {
                            view.set_log_level(level, window, cx);
                        });
                    }
                });
            for label in LOG_LEVEL_LABELS {
                level_group = level_group.child(
                    Radio::new(label)
                        .label(label)
                        .tooltip(log_level_tooltip(label)),
                );
            }
            let level_row = div().id("log-level-row").test_support().child(level_group);
            let copy_view = chrome_view.clone();
            let tail_view = chrome_view.clone();
            let save_view = chrome_view.clone();
            let clear_view = chrome_view.clone();

            let mut action_row = div()
                .id("log-actions-row")
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    Button::new("log-copy-visible")
                        .small()
                        .label("Copy visible")
                        .tooltip("Copy the visible log lines (redacted)")
                        .on_click(move |_, window, cx| {
                            copy_view.update(cx, |view, cx| {
                                view.copy_visible_logs(window, cx);
                            });
                        }),
                );
            if tail_on {
                action_row = action_row.child(
                    div()
                        .id("log-tail-spinner")
                        .test_support()
                        .child(Spinner::new().small()),
                );
            }
            action_row = action_row
                .child(
                    Button::new("log-tail-toggle")
                        .small()
                        .min_w(px(120.))
                        .label(if tail_on { "Pause tail" } else { "Resume tail" })
                        .tooltip(if tail_on {
                            "Pause the live tail"
                        } else {
                            "Resume the live tail"
                        })
                        .on_click(move |_, window, cx| {
                            tail_view.update(cx, |view, cx| {
                                view.toggle_log_tail(window, cx);
                            });
                        }),
                )
                .child(
                    Button::new("logs-save")
                        .small()
                        .label("Save")
                        .tooltip("Save the visible redacted lines to the logs file")
                        .on_click(move |_, window, cx| {
                            save_view.update(cx, |view, cx| {
                                view.save_visible_logs(window, cx);
                            });
                        }),
                );
            let save_path_text = logs_save_path_text();
            let mut body =
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(level_row)
                    .child(action_row)

                    .child(div().id("log-redact-note").test_support().text_sm().child(
                        "Copy and Save carry redacted lines — secrets never leave this dialog.",
                    ))
                    .child(
                        div()
                            .id("log-save-path")
                            .test_support()
                            .text_sm()
                            .child(save_path_text),
                    )
                    .child(dialog_field(
                        "field-log-filter",
                        "Filter",
                        filter_state.clone(),
                    ))
                    .child(dialog_field(
                        "field-log-exclude",
                        "Exclude",
                        exclude_state.clone(),
                    ))
                    .child(dialog_field(
                        "field-log-regex",
                        "Regex",
                        pattern_state.clone(),
                    ));
            let mut list = div().id("log-lines").test_support().w_full();

            let log_list_viewport = log_list_state.as_ref();
            if lines.is_empty() {
                let clear_filter_view = clear_view.clone();
                list = list.child(
                    Empty::new()
                        .header(
                            EmptyHeader::new()
                                .title(EmptyTitle::new().child("No log lines to show"))
                                .description(EmptyDescription::new().child(
                                    if chrome_filter_active || current_level.is_some() {
                                        "No lines match the current filter — clear it to see everything."
                                    } else {
                                        "No log lines recorded yet."
                                    },
                                )),
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
                                        Button::new("log-clear-filter")
                                            .small()
                                            .label("Clear filter")
                                            .tooltip("Clear the log text filter and level")
                                            .on_click(move |_, window, cx| {
                                                clear_filter_view.update(cx, |view, cx| {
                                                    view.clear_log_filter(window, cx);
                                                });
                                            }),
                                    ),
                            ),
                        ),
                );
            } else if let Some(state) = log_list_viewport {
                list = list.child(List::new(state).h(px(320.)).w_full());
            } else {
                for line in &lines {
                    list = list.child(line.clone());
                }
            }
            body = body.child(list);
            let close_view = chrome_view.clone();
            sheet
                .title(title.clone())
                .child(body)
                .size(px(520.))
                .on_close(move |_, window, cx| {
                    close_view.update(cx, |view, _| {
                        view.logs_open = false;
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(sheet_footer(
                    "logs-dialog-ok",
                    "logs-dialog-cancel",

                    footer_commit_label("logs"),
                    commit_click,
                ))
        });
    }

    fn open_export_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();

        let Some(rows) = self.list_or_notify(window, cx) else {
            return;
        };
        let default_path = Store::default_export_path().display().to_string();
        let path_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Export file path")
                .default_value(default_path)
        });
        let preview = export_preview_description(
            rows.len(),
            count_importable(&rows),
            &Store::default_export_path().display().to_string(),
            Store::default_export_path().exists(),
        );
        let warning = export_confirm_description().to_string();
        let reader = path_state.clone();
        let commit_view = view.clone();

        let commit: DialogCommit = Rc::new(move |window, cx| {
            let path = reader.read(cx).value().to_string();
            commit_view.update(cx, |view, cx| view.commit_export_dialog(path, window, cx))
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title("Export library")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div().id("export-warning").test_support().child(
                                Alert::error("export-warning-box", warning.clone())
                                    .title("Secrets leave in plaintext"),
                            ),
                        )
                        .child(div().id("export-safer-path").test_support().child(
                            "Safer: share one endpoint with Copy URLs — it never carries secrets.",
                        ))
                        .child(
                            div()
                                .id("export-preview")
                                .test_support()
                                .child(preview.clone()),
                        )
                        .child(dialog_field(
                            "field-export-path",
                            "Export file path",
                            path_state.clone(),
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "export-dialog-ok",
                    "export-dialog-cancel",
                    footer_commit_label("export"),
                    commit_click,
                ))
        });
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

    fn open_restore_confirm(
        &mut self,
        payload: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let payload = payload.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Restore")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Restore library")
                .description(format!(
                    "This replaces the current library with the exported backup \
                     (row ids, groups, enabled flags, subscriptions, and deletions \
                     are restored exactly). {}",
                    crate::store::RESTORE_REQUIRES_CLAUSE
                ))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_library_restore(&payload, window, cx)
                    });
                    true
                })
        });
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

    fn open_advanced_dialog(
        &mut self,
        form: &ConfigForm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let adv_view = view.clone();
        let repaint_view = view.clone();
        let adv_form = form.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let repaint: DialogRepaint = Rc::new({
                let repaint_view = repaint_view.clone();
                move |_window, cx| {
                    repaint_view.update(cx, |_, cx| cx.notify());
                }
            });
            let body_view = adv_view.clone();
            let protocol = adv_form.protocol_text(cx);
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .id("advanced-dialog")
                .test_support();
            if has_dial_panel(&protocol) {
                body = body.child(adv_form.s4_section(&body_view, &repaint, cx));
            }
            if tls_panel_plan(&protocol).is_some() {
                body = body.child(adv_form.s2_extras_section(&repaint));
            }

            let back = Rc::new(|window: &mut Window, cx: &mut App| {
                window.close_dialog(cx);
                reclaim_shell_focus(window, cx);
            });
            let back_key = back.clone();
            dialog
                .title("Advanced Settings")
                .child(body)
                .button_props(DialogButtonProps::default().show_cancel(false).on_ok(
                    move |_, window, cx| {
                        back_key(window, cx);
                        false
                    },
                ))
                .footer(
                    DialogFooter::new().child(
                        Button::new("advanced-dialog-back")
                            .small()
                            .label("Back")
                            .tooltip("Back to the main form — edits already apply")
                            .accessibility_label("Back to the main form")
                            .on_click(move |_, window, cx| back(window, cx)),
                    ),
                )
        });
    }

    fn open_row_columns_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live: Rc<RefCell<RowColumns>> = Rc::new(RefCell::new(self.row_columns));
        let view = cx.entity();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let view = view.clone();
            let current = *live.borrow();
            let mut body = div().flex().flex_col().gap_2();
            for (id, label, tip, enabled) in [
                (
                    "columns-upstream",
                    "Upstream",
                    "Show the upstream host:port on each row",
                    current.upstream,
                ),
                (
                    "columns-health",
                    "Health",
                    "Show serving state and probe evidence on each row",
                    current.health,
                ),
                (
                    "columns-group",
                    "Group",
                    "Show the group tag on each row",
                    current.group,
                ),
                (
                    "columns-traffic",
                    "Traffic",
                    "Show data-plane totals on serving rows",
                    current.traffic,
                ),
            ] {
                let column = match id {
                    "columns-upstream" => "upstream",
                    "columns-health" => "health",
                    "columns-group" => "group",
                    _ => "traffic",
                };
                let live = live.clone();
                let switch_view = view.clone();
                body = body.child(
                    Switch::new(id)
                        .checked(enabled)
                        .label(label)
                        .accessibility_label(label)
                        .tooltip(tip)
                        .on_change(move |enabled: &bool, window, cx| {
                            let enabled = *enabled;
                            {
                                let mut current = live.borrow_mut();
                                match column {
                                    "upstream" => current.upstream = enabled,
                                    "health" => current.health = enabled,
                                    "group" => current.group = enabled,
                                    _ => current.traffic = enabled,
                                }
                            }
                            switch_view.update(cx, |view, cx| {
                                view.set_row_column(column, enabled, window, cx);
                            });
                        }),
                );
            }
            let close: DialogCommit = Rc::new(|_, _| true);
            dialog
                .title("Row columns")
                .child(body)
                .footer(dialog_footer(
                    "columns-dialog-close",
                    "columns-dialog-cancel",
                    "Close",
                    close,
                ))
        });
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

    fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut form = ConfigForm::blank(window, cx);
        let view = cx.entity();

        let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let error_for_commit = form_error.clone();

        let rows = self.store.list().unwrap_or_default();

        let library: Vec<ChainLibraryEntry> = rows
            .iter()
            .map(|row| ChainLibraryEntry {
                id: row.id.clone(),
                name: row.name.clone(),
                protocol: row.protocol.clone(),
            })
            .collect();
        form.chain_library = Rc::new(RefCell::new(library.clone()));
        form.chain_pick = ConfigForm::panel_select(window, cx, chain_pick_choices(&library), "");
        let reader = form.clone();

        let budget = ConfigForm::port_budget_text(&rows);

        let element_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            view.update(cx, |view, cx| {
                view.commit_add_with_focus(&reader, &error_for_commit, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, window, cx| {
            form.sync_protocol_template(window, cx);
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();

            let live = form.clone();
            let bind = live.bind.read(cx).value().to_string();
            let port = live.port.read(cx).value().to_string();
            let collision = bind_port_collision_warning(&rows, None, &bind, &port);
            let inline = form_error.borrow().clone();
            dialog
                .title("Add proxy configuration")
                .overlay_closable(false)
                .child(form.clone().element(
                    element_view.clone(),
                    cx,
                    collision,
                    inline,
                    false,
                    Some(budget.clone()),
                ))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "add-dialog-ok",
                    "add-dialog-cancel",
                    footer_commit_label("add"),
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
        let mut form = ConfigForm::prefilled(
            window,
            cx,
            &config,
            self.store.row_test_url(&config.id).as_deref(),
        );
        let view = cx.entity();
        let id = config.id.clone();
        // T-19: protecting the row under edit from auto-clear starts here and
        // ends with one of the dialog's exits (commit success, footer Cancel,
        // Esc close) plus the reload guard for anything that slips past.
        self.edit_form_open = Some(id.clone());
        let title = format!("Edit {}", config.name);

        let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let error_for_commit = form_error.clone();

        let rows = self.store.list().unwrap_or_default();

        let library: Vec<ChainLibraryEntry> = rows
            .iter()
            .map(|row| ChainLibraryEntry {
                id: row.id.clone(),
                name: row.name.clone(),
                protocol: row.protocol.clone(),
            })
            .collect();
        form.chain_library = Rc::new(RefCell::new(library.clone()));
        form.chain_pick = ConfigForm::panel_select(window, cx, chain_pick_choices(&library), "");
        let reader = form.clone();

        let budget = ConfigForm::port_budget_text(&rows);
        let exclude = id.clone();

        let snapshot = config.clone();
        let element_view = view.clone();
        let track_source = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let patch = reader.as_patch_vs(cx, &snapshot);
            let url = reader.test_url.read(cx).value().to_string();
            view.update(cx, |view, cx| {
                view.commit_edit_with_test_url(
                    &id,
                    patch,
                    &url,
                    &reader,
                    &error_for_commit,
                    window,
                    cx,
                )
            })
        });
        let commit_key = commit.clone();
        focus_shell_before_overlay(window, cx);
        window.open_dialog(cx, move |dialog: Dialog, window, cx| {
            form.sync_protocol_template(window, cx);
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let live = form.clone();
            let bind = live.bind.read(cx).value().to_string();
            let port = live.port.read(cx).value().to_string();
            let collision = bind_port_collision_warning(&rows, Some(&exclude), &bind, &port);
            let inline = form_error.borrow().clone();
            dialog
                .title(title.clone())
                .overlay_closable(false)
                .on_close({
                    let track_view = track_source.clone();
                    move |_, window, cx| {
                        track_view.update(cx, |view, _| view.edit_form_open = None);
                        reclaim_shell_focus(window, cx);
                    }
                })
                .child(form.clone().element(
                    element_view.clone(),
                    cx,
                    collision,
                    inline,
                    true,
                    Some(budget.clone()),
                ))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(
                    // T-19: the shared footer closes with close_all_dialogs and
                    // would leave edit_form_open set; the Cancel click must
                    // release the row-under-edit protection itself.
                    DialogFooter::new()
                        .child(
                            Button::new("edit-dialog-cancel")
                                .small()
                                .label("Cancel")
                                .on_click({
                                    let track_view = track_source.clone();
                                    move |_, window, cx| {
                                        track_view.update(cx, |view, _| {
                                            view.edit_form_open = None;
                                        });
                                        window.close_all_dialogs(cx);
                                        reclaim_shell_focus(window, cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("edit-dialog-ok")
                                .small()
                                .label(footer_commit_label("edit"))
                                .on_click(move |_, window, cx| {
                                    if commit_click(window, cx) {
                                        window.close_all_dialogs(cx);
                                        reclaim_shell_focus(window, cx);
                                    }
                                }),
                        ),
                )
        });
    }

    fn open_remove_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.skip_delete_confirm() {
            self.commit_remove(&config.id, window, cx);
            return;
        }
        let view = cx.entity();
        let id = config.id.clone();
        let name = config.name.clone();

        let description = if self.engine.is_running(&config.id) {
            delete_running_confirm_text(&name)
        } else {
            delete_confirm_text(&name)
        };
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Delete")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Remove configuration")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_remove(&id, window, cx));
                    true
                })
        });
    }

    fn open_usage_reset_confirm(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = config.id.clone();
        let name = config.name.clone();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Reset")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Reset usage counters")
                .description(format!(
                    "Reset usage counters for \"{name}\" to zero? This cannot be undone."
                ))
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| view.commit_usage_reset(&id, window, cx));
                    true
                })
        });
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

        let commit_view = view.clone();
        let element_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let text = reader.text.read(cx).value().to_string();
            let group = reader.group.read(cx).value().to_string();
            let hint = reader.file_hint.clone();
            let original = reader.original_text.clone();
            let multi = reader.multi_files.clone();
            commit_view.update(cx, |view, cx| {
                view.commit_import_dialog(text, group, hint, Some(original), multi, window, cx)
            })
        });
        let commit_key = commit.clone();
        let cancel_view = view.clone();
        let close_view = view.clone();
        focus_shell_before_overlay(window, cx);
        // Snapshot before the builder runs: reading `AppView` inside the
        // dialog-builder closure re-enters the entity mid-update and panics.
        let in_flight = self.import_parse_in_flight;
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let cancel_view = cancel_view.clone();
            let close_view = close_view.clone();
            let footer = DialogFooter::new()
                .child(
                    Button::new("import-dialog-cancel")
                        .small()
                        .label("Cancel")
                        .on_click(move |_, window, cx| {
                            cancel_view.update(cx, |view, cx| view.cancel_import_parse(cx));
                            window.close_all_dialogs(cx);
                            reclaim_shell_focus(window, cx);
                        }),
                )
                .child(
                    Button::new("import-dialog-ok")
                        .small()
                        .label(if in_flight {
                            "Importing…"
                        } else {
                            footer_commit_label("import")
                        })
                        .disabled(in_flight)
                        .on_click(move |_, window, cx| {
                            if commit_click(window, cx) {
                                window.close_all_dialogs(cx);
                                reclaim_shell_focus(window, cx);
                            }
                        }),
                );
            dialog
                .title("Import proxy configurations")
                .child(form.clone().element(element_view.clone(), cx))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .on_close(move |_, window, cx| {
                    close_view.update(cx, |view, cx| view.cancel_import_parse(cx));
                    reclaim_shell_focus(window, cx);
                })
                .footer(footer)
        });
    }

    fn open_import_from_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        match Self::paste_intake_refusal(&text) {
            None => self.open_import_dialog(&text, DEFAULT_GROUP, None, window, cx),
            Some(reason) => {
                window.push_notification(Notification::warning(reason), cx);
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

    fn handle_launch_intake(
        &mut self,
        intake: OpenWithKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Boot calls this inside `cx.open_window`'s closure, before the window
        // adopts its `Root`; the intake dialog would panic in `Root::update`.
        // The intake decision is taken now, the dialog opens on the next frame.
        self.after_root(window, cx, move |view, window, cx| match intake {
            OpenWithKind::BareHttp(url) => view.open_bare_url_chooser(url, window, cx),
            OpenWithKind::Batch(text) => {
                view.open_import_dialog(&text, DEFAULT_GROUP, None, window, cx)
            }
        });
    }

    fn open_bare_url_chooser(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let proxy_view = view.clone();
        let proxy_url = url.clone();
        let sub_view = view.clone();
        let sub_url = url.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let proxy_view = proxy_view.clone();
            let proxy_url = proxy_url.clone();
            let sub_view = sub_view.clone();
            let sub_url = sub_url.clone();
            dialog
                .title("Add HTTP link")
                .child(
                    div()
                        .id("bare-url-chooser")
                        .test_support()
                        .child(format!(
                            "\"{url}\" can be a subscription feed or a proxy — choose how to add it:"
                        ))
                        .child(
                            {
                                let view = proxy_view.clone();
                                let url = proxy_url.clone();
                                Button::new("bare-url-import-proxy")
                                    .label("Import as proxy here")
                                    .on_click(move |_, window, cx| {
                                        window.close_all_dialogs(cx);
                                        reclaim_shell_focus(window, cx);
                                        view.update(cx, |view, cx| {
                                            view.open_import_dialog(
                                                &url,
                                                DEFAULT_GROUP,
                                                None,
                                                window,
                                                cx,
                                            );
                                        });
                                    })
                            },
                        )
                        .child(
                            {
                                let view = sub_view.clone();
                                let url = sub_url.clone();
                                Button::new("bare-url-add-subscription")
                                    .label("Add as subscription")
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |view, cx| {
                                            view.commit_bare_url_subscription(
                                                url.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    })
                            },
                        ),
                )

                .button_props(DialogButtonProps::default())
                .footer(dialog_footer(
                    "bare-url-close",
                    "bare-url-cancel",
                    footer_commit_label("bare-url"),
                    Rc::new(|_, _| true),
                ))
        });
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

    fn reopen_subscriptions_if_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.has_active_dialog(cx) && !window.has_active_sheet(cx) {
            self.subs_form = None;
            return;
        }
        let Some(form) = &self.subs_form else {
            return;
        };
        let label = form.label.read(cx).value().to_string();
        let url = form.url.read(cx).value().to_string();
        let group = form.group.read(cx).value().to_string();
        let auto = *form.auto_refresh.borrow();
        let interval = *form.interval_secs.borrow();
        window.close_all_dialogs(cx);
        window.close_sheet(cx);
        self.open_subscriptions_dialog_restored(&label, &url, &group, auto, interval, window, cx);
        reclaim_shell_focus(window, cx);
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

    fn open_preset_structured_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Preset name"));
        let action_cell = Rc::new(RefCell::new("direct".to_string()));
        let view = cx.entity();
        let name_reader = name_state.clone();
        let action_reader = action_cell.clone();
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = name_reader.read(cx).value().to_string();
            let action = action_reader.borrow().clone();
            commit_view.update(cx, |view, cx| {
                view.commit_preset_create_structured(name, action, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let cell = action_cell.clone();
            let current = cell.borrow().clone();
            let selected = ["direct", "proxy", "block"]
                .iter()
                .position(|value| *value == current);
            dialog
                .title("New structured preset")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(dialog_field(
                            "field-structured-preset-name",
                            "Name",
                            name_state.clone(),
                        ))
                        .child(
                            div()
                                .id("field-structured-preset-action")
                                .test_support()
                                .child({
                                    let mut group =
                                        RadioGroup::horizontal("structured-preset-action-options")
                                            .selected_index(selected)
                                            .on_change(move |ix, _, _| {
                                                let actions = ["direct", "proxy", "block"];
                                                if let Some(action) = actions.get(*ix) {
                                                    *cell.borrow_mut() = action.to_string();
                                                }
                                            });
                                    for label in ["Direct", "Proxy", "Block"] {
                                        group = group.child(
                                            Radio::new(label)
                                                .label(label.to_string())
                                                .tooltip(format!("Catch-all rule: {label}")),
                                        );
                                    }
                                    group
                                }),
                        ),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "structured-preset-ok",
                    "structured-preset-cancel",
                    footer_commit_label("presets"),
                    commit_click,
                ))
        });
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

    fn open_preset_remote_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Preset name"));
        let url_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/preset.json"));
        let view = cx.entity();
        let name_reader = name_state.clone();
        let url_reader = url_state.clone();
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = name_reader.read(cx).value().to_string();
            let url = url_reader.read(cx).value().to_string();
            if name.trim().is_empty() && url.trim().is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.commit_preset_create_remote(name, url, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title("New remote preset")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(dialog_field(
                            "field-remote-preset-name",
                            "Name",
                            name_state.clone(),
                        ))
                        .child(dialog_field(
                            "field-remote-preset-url",
                            "Preset URL",
                            url_state.clone(),
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "remote-preset-ok",
                    "remote-preset-cancel",
                    footer_commit_label("presets"),
                    commit_click,
                ))
        });
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

    fn open_preset_edit_dialog(
        &mut self,
        preset: store::RoutePreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rules_text = preset
            .rules()
            .map(|rules| serde_json::Value::Array(rules).to_string())
            .unwrap_or_default();
        let title = format!("Edit preset {}", preset.name);
        if preset.locked {
            // T-10: a locked preset renders a readonly editor — view-only
            // rows, no inputs, no save/copy-edit path.
            let id = preset.id.clone();
            let name = preset.name.clone();
            let json_text = rules_text;
            let advanced_view = cx.entity();
            open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
                dialog
                    .title(title.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id(format!("preset-locked-hint-{id}"))
                                    .test_support()
                                    .text_sm()
                                    .child("Locked — import a copy to modify"),
                            )
                            .child(
                                div()
                                    .id(format!("preset-locked-name-{id}"))
                                    .test_support()
                                    .text_sm()
                                    .child(format!("Name: {name}")),
                            )
                            .child(
                                div()
                                    .id(format!("preset-locked-json-{id}"))
                                    .test_support()
                                    .text_sm()
                                    .child(json_text.clone()),
                            )
                            .child(preset_advanced_section(
                                advanced_view.clone(),
                                &preset,
                                None,
                                &[],
                            )),
                    )
                    .button_props(DialogButtonProps::default())
                    .footer(dialog_footer(
                        "preset-locked-close",
                        "preset-locked-close",
                        "Close",
                        Rc::new(|_, _| true),
                    ))
            });
            return;
        }
        let exits: Vec<String> = self
            .store
            .preset_attachments(&preset.id)
            .unwrap_or_default()
            .iter()
            .map(|row| crate::engine::outbound_tag(&row.id))
            .collect();
        let form = PresetForm::restored(window, cx, &preset.name, &rules_text);
        let view = cx.entity();
        let reader = form.clone();
        let id = preset.id.clone();
        let builder_view = view.clone();
        let builder_preset = preset.clone();
        let builder_form = form.clone();
        let builder_exits = exits.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = reader.name.read(cx).value().to_string();
            let rules = reader.rules.read(cx).value().to_string();
            let boxes = SimpleRuleBoxes::from_form(&reader, cx);
            view.update(cx, |view, cx| {
                view.commit_preset_edit_with_boxes(&id, name, rules, boxes, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            dialog
                .title(title.clone())
                .child(form.clone().element(cx))
                .child(preset_advanced_section(
                    builder_view.clone(),
                    &builder_preset,
                    Some(builder_form.clone()),
                    builder_exits.as_slice(),
                ))
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "preset-edit-ok",
                    "preset-edit-cancel",
                    footer_commit_label("preset-edit"),
                    commit_click,
                ))
        });
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

    fn open_preset_detach_delete_confirm(
        &mut self,
        id: &str,
        name: &str,
        attached: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let preset_id = id.to_string();
        let description = preset_delete_confirm_description(name, attached);
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let preset_id = preset_id.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Delete")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Delete routing preset")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.commit_preset_detach_delete(&preset_id, window, cx)
                    });
                    true
                })
        });
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

    fn open_rule_set_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Rule set name"));
        let url_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://host.example/set.srs"));
        let view = cx.entity();
        let name_reader = name_state.clone();
        let url_reader = url_state.clone();
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = name_reader.read(cx).value().to_string();
            let url = url_reader.read(cx).value().to_string();
            if name.trim().is_empty() && url.trim().is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.begin_rule_set_download(name, url, window, cx)
            })
        });
        let commit_key = commit.clone();
        let commit_click = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit_click.clone();
            dialog
                .title("Add rule set by URL")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(dialog_field(
                            "field-rule-set-name",
                            "Name",
                            name_state.clone(),
                        ))
                        .child(dialog_field(
                            "field-rule-set-url",
                            "Rule set URL",
                            url_state.clone(),
                        ))
                        .child(Label::new(
                            "The download runs only after you confirm it below — each download is one explicit confirmation",
                        )),
                )
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "rule-set-add-dialog-ok",
                    "rule-set-add-dialog-cancel",
                    footer_commit_label("rule-set-add-dialog"),
                    commit_click,
                ))
        });
    }

    /// The Add-by-URL commit: validate the name, run the same policy gate as
    /// subscriptions (internal hosts block, plaintext http warn at the
    /// confirm), allocate a collision-free staged name, then request the
    /// explicit confirm. Nothing is fetched here.
    fn begin_rule_set_download(
        &mut self,
        name: String,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = match crate::store::Store::validated_rule_set_name(&name) {
            Ok(name) => name,
            Err(error) => {
                self.report_save_error(error, window, cx);
                return false;
            }
        };
        match store::subscription_fetch_policy(url.trim()) {
            store::SubscriptionFetchPolicy::Block { reason } => {
                window.push_notification(
                    Notification::error(format!("Invalid rule set URL: {reason}")),
                    cx,
                );
                return false;
            }
            store::SubscriptionFetchPolicy::Warn { .. } | store::SubscriptionFetchPolicy::Allow => {
            }
        }
        let dir = crate::store::rule_sets_dir();
        let taken: HashSet<String> = match self.store.list_rule_sets() {
            Ok(rows) => rows.into_iter().map(|row| row.name).collect(),
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load rule sets: {error}")),
                    cx,
                );
                return false;
            }
        };
        let mut candidate = name.clone();
        let mut suffix = 1;
        loop {
            let file_name = crate::store::rule_set_file_name(&candidate);
            if !taken.contains(&candidate) && !dir.join(&file_name).exists() {
                let grant = RuleSetDownloadGrant {
                    name: candidate,
                    file_name,
                    url: url.trim().to_string(),
                    dir,
                };
                self.open_rule_set_download_confirm(grant, window, cx);
                return true;
            }
            suffix += 1;
            if suffix > 100 {
                break;
            }
            candidate = format!("{name}-{suffix}");
        }
        window.push_notification(
            Notification::error(format!(
                "no free staging name for \"{name}\" — remove stale rule sets first"
            )),
            cx,
        );
        false
    }

    /// The exact one-confirmation-per-download callout. Blocked URLs never get
    /// here; http warns through the same policy verdicts subscriptions use.
    fn open_rule_set_download_confirm(
        &mut self,
        grant: RuleSetDownloadGrant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match store::subscription_fetch_policy(grant.url.trim()) {
            store::SubscriptionFetchPolicy::Block { reason } => {
                window.push_notification(
                    Notification::error(format!("Invalid rule set URL: {reason}")),
                    cx,
                );
                return;
            }
            store::SubscriptionFetchPolicy::Warn { reason } => {
                window.push_notification(Notification::warning(reason.to_string()), cx);
            }
            store::SubscriptionFetchPolicy::Allow => {}
        }
        let view = cx.entity();
        let warn = match store::subscription_fetch_policy(grant.url.trim()) {
            store::SubscriptionFetchPolicy::Warn { reason } => format!("\n{reason}"),
            _ => String::new(),
        };
        let overwrites = grant.dir.join(&grant.file_name).exists();
        let description = format!(
            "Download this rule set? {}\n{}\
             Staged file: {}\\{}",
            grant.url,
            warn,
            grant.dir.display(),
            grant.file_name,
        );
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let grant = grant.clone();
            let view = view.clone();
            let description = description.clone();
            let mut props = DialogButtonProps::default()
                .show_cancel(true)
                .ok_text("Download");
            if overwrites {
                props = props.ok_variant(ButtonVariant::Danger);
            }
            alert
                .confirm()
                .button_props(props)
                .title("Download rule set?")
                .description(description)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.start_rule_set_download(grant.clone(), window, cx);
                    });
                    true
                })
        });
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

    fn open_presets_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_presets_dialog_restored("", "", &SimpleRuleBoxes::default(), window, cx);
    }

    fn open_presets_dialog_restored(
        &mut self,
        name: &str,
        rules: &str,
        boxes: &SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = PresetForm::restored_with_boxes(window, cx, name, rules, boxes);
        self.preset_form = Some(form.clone());
        let view = cx.entity();
        let reader = form.clone();
        let presets = match self.store.list_presets() {
            Ok(presets) => presets,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load routing presets: {error}")),
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
        let mut attached: HashMap<String, Vec<String>> = HashMap::new();
        for row in &rows {
            if let Some(preset_id) = row.preset_id.as_deref() {
                attached
                    .entry(preset_id.to_string())
                    .or_default()
                    .push(format!("\"{}\"", row.name));
            }
        }
        let rule_sets = match self.store.list_rule_sets() {
            Ok(rule_sets) => rule_sets,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load rule sets: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        let mut rule_set_used: HashMap<String, Vec<String>> = HashMap::new();
        for preset in &presets {
            if let Ok(names) = self.store.rule_sets_in_use(&preset.id) {
                for name in names {
                    rule_set_used
                        .entry(name)
                        .or_default()
                        .push(preset.name.clone());
                }
            }
        }
        let rule_set_view = view.clone();
        let rule_set_add_view = view.clone();
        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let name = reader.name.read(cx).value().to_string();
            let rules = reader.rules.read(cx).value().to_string();
            let boxes = SimpleRuleBoxes::from_form(&reader, cx);
            if name.trim().is_empty() && rules.trim().is_empty() && boxes.is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.commit_preset_create_with_boxes(name, rules, boxes, window, cx)
            })
        });
        let commit_key = commit.clone();
        let close_view = view.clone();
        focus_shell_before_overlay(window, cx);
        window.open_dialog(cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let view_for_rows = view.clone();
            let close_view = close_view.clone();
            let mut body = div().flex().flex_col().gap_2();
            // T-04: the preset inventory left Settings — it lives here, in the
            // manager the Routing menu and the inspector pane link to.
            body = body.child(
                div()
                    .id("presets-summary")
                    .test_support()
                    .text_sm()
                    .child(preset_summary_text(Some(presets.len()))),
            );
            if presets.is_empty() {
                body = body.child(
                    "No routing presets yet — paste a throne://route link in Import, or create one below",
                );
            }
            for preset in &presets {
                body = body.child(preset_row(
                    view_for_rows.clone(),
                    preset.clone(),
                    attached.get(&preset.id).cloned().unwrap_or_default(),
                ));
            }

            // T-12: the Rule sets sub-list — explicit confirm-per-download
            // staging lives here, never anywhere else.
            body = body.child(
                div()
                    .id("rule-sets-section-title")
                    .test_support()
                    .text_sm()
                    .child("Rule sets"),
            );
            if rule_sets.is_empty() {
                body = body.child(Label::new(
                    "No rule sets staged — Add by URL downloads an .srs file into data\\rule-sets after you confirm each download",
                ));
            }
            for rule_set in &rule_sets {
                body = body.child(rule_set_row(
                    rule_set_view.clone(),
                    rule_set.clone(),
                    rule_set_used
                        .get(&rule_set.name)
                        .cloned()
                        .unwrap_or_default(),
                ));
            }
            {
                let value = rule_set_add_view.clone();
                body = body.child(
                    div().flex().flex_row().items_center().gap_2().child(
                        Button::new("rule-set-add")
                            .label("Add by URL")
                            .tooltip("Download an .srs rule set from an https URL (you confirm each download)")
                            .on_click(move |_, window, cx| {
                                value.update(cx, |view, cx| {
                                    view.open_rule_set_add_dialog(window, cx);
                                });
                            }),
                    ),
                );
            }

            {
                let structured_view = view_for_rows.clone();
                let remote_view = view_for_rows.clone();
                body = body.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("preset-new-structured")
                                .label("New Structured")
                                .tooltip("Name + Direct|Proxy|Block catch-all rule")
                                .on_click(move |_, window, cx| {
                                    structured_view.update(cx, |view, cx| {
                                        view.open_preset_structured_dialog(window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("preset-new-remote")
                                .label("New Remote")
                                .tooltip("Name + preset URL (explicit fetch fills the rules)")
                                .on_click(move |_, window, cx| {
                                    remote_view.update(cx, |view, cx| {
                                        view.open_preset_remote_dialog(window, cx);
                                    });
                                }),
                        ),
                );
            }
            body = body.child(form.clone().element(cx));
            dialog
                .title("Routing presets")
                .child(body)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .on_close(move |_, window, cx| {
                    close_view.update(cx, |view, _| {
                        view.preset_form = None;
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(dialog_footer(
                    "presets-dialog-ok",
                    "presets-dialog-cancel",

                    footer_commit_label("presets"),
                    commit_click,
                ))
        });
    }

    fn reopen_presets_if_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.has_active_dialog(cx) {
            self.preset_form = None;
            return;
        }
        let Some(form) = &self.preset_form else {
            return;
        };
        let name = form.name.read(cx).value().to_string();
        let rules = form.rules.read(cx).value().to_string();
        let boxes = SimpleRuleBoxes::from_form(form, cx);
        window.close_all_dialogs(cx);
        reclaim_shell_focus(window, cx);
        self.open_presets_dialog_restored(&name, &rules, &boxes, window, cx);
    }

    fn open_preset_attach_dialog(
        &mut self,
        config: ProxyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let config_id = config.id.clone();
        let config_name = config.name.clone();
        let current = config.preset_id.clone();
        let presets = match self.store.list_presets() {
            Ok(presets) => presets,
            Err(error) => {
                window.push_notification(
                    Notification::error(format!("Cannot load routing presets: {error}")),
                    cx,
                );
                Vec::new()
            }
        };
        // Read-only monitor, computed once per open: rows of the selected
        // row's group, ranked by their latest health samples. No probing, no
        // sampling, no persistence — purely the ordering view of what the
        // session already knows.
        let monitor = {
            let group_rows: Vec<ProxyConfig> = self
                .store
                .list()
                .unwrap_or_default()
                .into_iter()
                .filter(|row| row.group == config.group)
                .collect();
            crate::health::best_rows_monitor_lines(&config.group, &group_rows, &self.health_store)
        };
        let commit: DialogCommit = Rc::new(|_, _| true);
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, _| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();
            let mut body = div().flex().flex_col().gap_2();
            if let Some((monitor_title, monitor_lines)) = &monitor {
                let mut section = div()
                    .id("preset-attach-monitor")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("preset-attach-monitor-title")
                            .test_support()
                            .text_sm()
                            .child(monitor_title.clone()),
                    );
                for (index, line) in monitor_lines.iter().enumerate() {
                    section = section.child(
                        div()
                            .id(format!("preset-attach-monitor-{index}"))
                            .test_support()
                            .text_sm()
                            .child(line.clone()),
                    );
                }
                body = body.child(section);
            }
            if !presets.is_empty() {
                body = body.child(
                    div()
                        .id("preset-attach-exits")
                        .test_support()
                        .text_sm()
                        .child(format!(
                            "Route exits for \"{config_name}\": {}",
                            crate::preset::preset_exit_suggestions(&[crate::engine::outbound_tag(
                                &config_id
                            )])
                            .join(" · ")
                        )),
                );
            }
            if presets.is_empty() {
                body = body.child(
                    "No routing presets yet — import one first (throne://route link or preset JSON)",
                );
            }
            for preset in &presets {
                let attach_view = view.clone();
                let attach_config = config_id.clone();
                let attach_preset = preset.id.clone();
                let mut line = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Label::new(preset.name.clone()));
                if current.as_deref() == Some(preset.id.as_str()) {
                    line = line.child("attached");
                } else {
                    line = line.child(
                        Button::new(format!("preset-attach-{}", preset.id))
                            .label("Attach")
                            .on_click(move |_, window, cx| {
                                let config = attach_config.clone();
                                let preset = attach_preset.clone();
                                attach_view.update(cx, |view, cx| {
                                    view.commit_preset_attach(&config, Some(preset), window, cx);
                                });
                            }),
                    );
                }
                body = body.child(line);
            }
            if current.is_some() {
                let detach_view = view.clone();
                let detach_config = config_id.clone();
                body = body.child(Button::new("preset-detach").label("Detach").on_click(
                    move |_, window, cx| {
                        let config = detach_config.clone();
                        detach_view.update(cx, |view, cx| {
                            view.commit_preset_attach(&config, None, window, cx);
                        });
                    },
                ));
            }
            dialog
                .title(format!("Routing preset for \"{config_name}\""))
                .child(body)
                .button_props(
                    DialogButtonProps::default().on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "preset-attach-ok",
                    "preset-attach-cancel",
                    footer_commit_label("preset-attach"),
                    commit_click,
                ))
        });
    }

    fn open_sub_full_url_confirm(
        &mut self,
        label: &str,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let full = url.to_string();
        let description = format!(
            "The full URL for \"{label}\" carries its token in plaintext — anyone holding it can \
             use the subscription. Prefer the redacted copy. Copy the full URL anyway?"
        );
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let full = full.clone();
            let description = description.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Copy URL")
                        .ok_variant(ButtonVariant::Danger),
                )
                .title("Copy full subscription URL?")
                .description(description)
                .on_ok(move |_, window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(full.clone()));
                    window.push_notification(
                        Notification::warning("Copied FULL subscription URL (carries the token)"),
                        cx,
                    );
                    true
                })
        });
    }

    fn open_subscriptions_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_subscriptions_dialog_restored(
            "",
            "",
            DEFAULT_GROUP,
            true,
            store::DEFAULT_AUTO_REFRESH_SECS,
            window,
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn open_subscriptions_dialog_restored(
        &mut self,
        label: &str,
        url: &str,
        group: &str,
        auto: bool,
        interval_secs: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = SubForm::restored(window, cx, label, url, group, auto, interval_secs);
        self.subs_form = Some(form.clone());
        let view = cx.entity();
        let reader = form.clone();
        let subs = match self.store.list_subscriptions() {
            Ok(subs) => subs,

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

        let expanded = self.expanded_subs.clone();
        let member_rows = rows.clone();

        let commit_view = view.clone();
        let commit: DialogCommit = Rc::new(move |window, cx| {
            let label = reader.label.read(cx).value().to_string();
            let url = reader.url.read(cx).value().to_string();
            let group = reader.group.read(cx).value().to_string();
            let auto = *reader.auto_refresh.borrow();
            let interval = *reader.interval_secs.borrow();
            if label.trim().is_empty() && url.trim().is_empty() {
                return true;
            }
            commit_view.update(cx, |view, cx| {
                view.commit_subscription_add(label, url, group, auto, interval, window, cx)
            })
        });

        let auto_all = self.store.subs_auto_refresh_enabled();
        let now = now_unix();
        let due_count = subs
            .iter()
            .filter(|sub| due_for_refresh_for(sub, now))
            .count();

        let batch_progress = self.refresh_batch_progress();
        let refreshing_rows = self.inflight_refreshes.clone();
        focus_shell_before_overlay(window, cx);
        let kill_view = view.clone();
        let due_view = view.clone();

        let close_view = view.clone();

        window.open_sheet(cx, move |sheet: Sheet, _, cx| {
            let commit_click = commit.clone();
            let view_for_rows = view.clone();
            let close_subs = close_view.clone();
            let kill_buttons = kill_view.clone();
            let due_buttons = due_view.clone();
            let batch_progress = batch_progress;
            let refreshing_rows = refreshing_rows.clone();
            let mut body = div().flex().flex_col().gap_2();

            let kill_now = auto_all;
            body = body.child(
                div()
                    .id("subs-auto-header")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("subs-auto-all")
                            .checked(kill_now)
                            .label("Auto-refresh all")

                            .tooltip(
                                "Automatic background refresh of all subscriptions. Manual Refresh always works.",
                            )
                            .accessibility_label(
                                "Auto-refresh all subscriptions automatically. Manual Refresh always works.",
                            )
                            .on_change(move |next: &bool, window, cx| {
                                let next = *next;
                                kill_buttons.update(cx, |view, cx| {
                                    view.commit_subs_global_auto(next, window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("subs-refresh-all-due")
                            .label(format!("Refresh all due ({due_count})"))
                            .on_click(move |_, window, cx| {
                                due_buttons.update(cx, |view, cx| {
                                    view.refresh_all_due(window, cx);
                                });
                            }),
                    ),
            );
            if subs.is_empty() {
                body = body.child("No subscriptions yet — paste a subscription URL below");
            }

            if let Some((done, total)) = batch_progress {
                let percent =
                    (done as f32 / total.max(1) as f32 * 100.0).clamp(0.0, 100.0);
                body = body.child(
                    div()
                        .id("subs-refresh-progress")
                        .test_support()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Progress::new("subs-refresh-progress-bar")
                                .small()
                                .value(percent)
                                .accessibility_label(format!(
                                    "Refreshing subscriptions: {done} of {total}"
                                )),
                        )
                        .child(format!("Refreshing {done} of {total} subscriptions")),
                );
            }
            for sub in &subs {
                body = body.child(subscription_row(
                    view_for_rows.clone(),
                    sub.clone(),
                    counts.get(&sub.id).copied().unwrap_or(0),
                    expanded.contains(&sub.id),
                    subscription_member_names(&member_rows, &sub.id),
                    refreshing_rows.contains(&sub.id),
                ));
            }
            body = body.child(form.clone().element(view_for_rows.clone(), cx));
            sheet
                .title("Subscriptions")
                .child(body)
                .size(px(520.))
                .on_close(move |_, window, cx| {
                    close_subs.update(cx, |view, _| {
                        view.subs_form = None;
                    });
                    reclaim_shell_focus(window, cx);
                })
                .footer(sheet_footer(
                    "subscriptions-dialog-ok",
                    "subscriptions-dialog-cancel",

                    footer_commit_label("subscriptions"),
                    commit_click,
                ))
        });
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

        let interval_cell: Rc<RefCell<i64>> = Rc::new(RefCell::new(sub.interval_secs));
        let read_interval = interval_cell.clone();
        let edit_interval = interval_cell.clone();

        let repaint_view = view.clone();

        let commit: DialogCommit = Rc::new(move |window, cx| {
            let label = read_label.read(cx).value().to_string();
            let url = read_url.read(cx).value().to_string();
            let group = read_group.read(cx).value().to_string();
            let interval = *read_interval.borrow();
            view.update(cx, |view, cx| {
                view.commit_subscription_edit(&id, label, url, group, interval, window, cx)
            })
        });
        let commit_key = commit.clone();
        open_app_dialog(window, cx, move |dialog: Dialog, _, cx| {
            let commit_key = commit_key.clone();
            let commit_click = commit.clone();

            let repaint_view = repaint_view.clone();

            let url_now = url_state.read(cx).value().to_string();
            let edit_warning = http_subscription_warning(&url_now).map(|warning| {
                div()
                    .id("sub-edit-http-warning")
                    .test_support()
                    .child(warning)
            });

            let mut form = v_form()
                .small()
                .child(dialog_field(
                    "field-sub-edit-label",
                    "Label",
                    label_state.clone(),
                ))
                .child(dialog_field(
                    "field-sub-edit-url",
                    "Subscription URL",
                    url_state.clone(),
                ));
            if let Some(warning) = edit_warning {
                form = form.child(field().label_indent(false).child(warning));
            }
            form = form
                .child(dialog_field(
                    "field-sub-edit-group",
                    "Group",
                    group_state.clone(),
                ))
                .child(field().label_indent(false).child(
                    div().id("field-sub-edit-interval").test_support().child({
                        let repaint: DialogRepaint = Rc::new(move |_, cx| {
                            repaint_view.update(cx, |_, _| {});
                        });
                        interval_picker_row("sub-interval-edit", edit_interval.clone(), repaint)
                    }),
                ));
            dialog
                .title(title.clone())
                .child(form)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| commit_key(window, cx)),
                )
                .footer(dialog_footer(
                    "subscription-edit-dialog-ok",
                    "subscription-edit-dialog-cancel",
                    footer_commit_label("subscription-edit"),
                    commit_click,
                ))
        });
    }

    fn open_subscription_remove_confirm(
        &mut self,
        sub: ProxySubscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let id = sub.id.clone();
        let label = sub.label.clone();
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let id = id.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Remove")
                        .ok_variant(ButtonVariant::Danger),
                )
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

    fn open_update_all_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let subs = self.store.list_subscriptions().unwrap_or_default();
        let flagged: HashSet<String> = self
            .store
            .group_skip_flags()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let (included, skipped) = update_all_targets(subs, &flagged);
        if included.is_empty() {
            window.push_notification(
                Notification::info(if skipped.is_empty() {
                    "No subscriptions to update".to_string()
                } else {
                    format!(
                        "No subscriptions to update — {} skipped (auto-refresh off for their group)",
                        skipped.len()
                    )
                }),
                cx,
            );
            return;
        }
        let view = cx.entity();
        let ids: Vec<String> = included.iter().map(|sub| sub.id.clone()).collect();
        let description = update_all_confirm_text(included.len(), skipped.len());
        open_app_alert_dialog(window, cx, move |alert, _, _| {
            let view = view.clone();
            let ids = ids.clone();
            alert
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("Update all"),
                )
                .title("Update all subscriptions?")
                .description(description.clone())
                .on_ok(move |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.begin_refresh_batch(&ids);

                        for id in &ids {
                            if let Ok(Some(sub)) = view.store.get_subscription(id) {
                                view.start_subscription_refresh(sub, true, cx);
                            }
                        }
                        view.reopen_subscriptions_if_open(window, cx);
                    });
                    true
                })
        });
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
