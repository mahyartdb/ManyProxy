use crate::*;

pub fn topbar_menu_row(
    id: &'static str,
    label: impl Into<SharedString>,
    accessible: impl Into<SharedString>,
    tip: impl Into<SharedString>,
    shortcut: Option<(&'static str, &'static str, &'static str)>,
) -> AnyElement {
    let label = label.into();
    let accessible = accessible.into();
    let tip = tip.into();
    // A6-5/K8: the label flexes and the chip is parked on the row's right
    // edge, so every chipped item in a menu reads as one aligned column and
    // the chip keeps its full width (never "Ctrl+…").
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
            .tooltip(move |_, cx| {
                let tip = tip.clone();
                cx.new(move |_| Tooltip::new(tip)).into()
            })
            .child(div().flex_1().min_w_0().child(label))
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
            .tooltip(move |_, cx| {
                let tip = tip.clone();
                cx.new(move |_| Tooltip::new(tip)).into()
            })
            .child(label)
            .into_any_element(),
    }
}

pub fn search_placeholder() -> String {
    t!("filter-search-placeholder")
}

/// T-23: the one hint that the `port=` grammar exists — a tooltip on the search
/// box, because the pinned placeholder is too narrow to carry it.
pub fn search_hint() -> String {
    t!("search-hint")
}

/// Store meta key for "Automatically restart crashed cores"; a missing or corrupt
/// value reads as enabled, matching the engine's default.
pub const META_AUTO_RESTART: &str = "auto_restart_enabled";

/// Store meta key for "Notify from the tray"; a missing or corrupt value reads
/// as enabled, because the notifications exist precisely for a hidden window.
pub const META_NOTIFY_FROM_TRAY: &str = "notify_from_tray_enabled";

/// Store meta key for "Check for updates weekly"; a missing or corrupt value
/// reads as enabled because the passive check ships on (plan decision 6).
pub const META_PASSIVE_UPDATE_CHECK: &str = "passive_update_check_enabled";

/// Bind failures are the ones where "start" did nothing because the endpoint's
/// address was taken or unusable; the user needs the log even when the window
/// is hidden.
pub fn is_port_bind_error(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::PortConflict { .. }
            | EngineError::RowPortUnavailable { .. }
            | EngineError::SharedPortUnavailable { .. }
            | EngineError::ReservedPort { .. }
            | EngineError::BadBind { .. }
    )
}

pub fn is_row_local_start_error(error: &EngineError) -> bool {
    crate::engine::is_row_local_error(error)
}

pub fn normalize_test_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(t!("test-url-empty"));
    }
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .ok_or_else(|| t!("test-url-scheme"))?;
    let host = rest.split('/').next().unwrap_or("").trim();
    if host.is_empty() {
        return Err(t!("test-url-host"));
    }
    Ok(trimmed.to_string())
}

pub fn effective_probe_url(row_override: Option<&str>, global: Option<&str>) -> String {
    row_override
        .filter(|url| !url.trim().is_empty())
        .or_else(|| global.filter(|url| !url.trim().is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_TEST_URL.to_string())
}

pub fn format_params_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| t!("params-invalid-json", error = error.to_string()))?;
    serde_json::to_string_pretty(&value)
        .map_err(|error| t!("params-format-failed", error = error.to_string()))
}

pub fn validate_params_json(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map(|_| ())
        .map_err(|error| t!("params-invalid-json", error = error.to_string()))
}

pub fn guided_identity_label(protocol_raw: &str) -> String {
    match guided_identity_fields(protocol_raw).and_then(|guided| guided.key) {
        Some(key) => t!("guided-identity-key", key = key),
        None => t!("guided-identity"),
    }
}

pub fn guided_hint_line(protocol_raw: &str) -> Option<String> {
    let name = protocol_raw.trim();
    let guided = guided_identity_fields(protocol_raw)?;
    if name.is_empty() {
        return None;
    }
    match guided.key {
        Some(_) => Some(t!("guided-needs", name = name, needs = guided.needs)),
        None if guided.needs == "custom JSON" => Some(t!("guided-custom-json", name = name)),
        None => Some(t!("guided-no-secret", name = name)),
    }
}

pub fn block_owns_identity(protocol_raw: &str) -> bool {
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

pub fn tls_panel_plan(protocol_raw: &str) -> Option<bool> {
    match protocol_raw.trim() {
        "vless" | "vmess" | "trojan" | "http" | "https" => Some(false),
        "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls" => Some(true),
        _ => None,
    }
}

pub fn has_network_panel(protocol_raw: &str) -> bool {
    matches!(protocol_raw.trim(), "vless" | "vmess" | "trojan")
}

pub fn has_dial_panel(protocol_raw: &str) -> bool {
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

pub fn has_advanced_dialog(protocol_raw: &str) -> bool {
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
    // K12/A5-1: words instead of the tiny `↓`/`↑` pair, which the UI video
    // showed degrading to a serif "T". `out` is what the endpoints sent
    // (tx), `in` is what came back (rx).
    Some(t!(
        "traffic-totals",
        tx = format_bytes(tx),
        rx = format_bytes(rx)
    ))
}

pub fn interval_picker_row(
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
        let label = format_interval_label(*preset);
        group = group.child(
            Radio::new(label.clone())
                .label(label.clone())
                .tooltip(t!("interval-tip", label = label.clone())),
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
                .child(Label::new(t!("interval-caption"))),
        )
        .child(
            div()
                .id(format!("{id_prefix}-group"))
                .test_support()
                .child(group),
        )
}

pub fn spawn_tick(
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
