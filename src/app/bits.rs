use crate::*;

pub(crate) fn topbar_menu_row(
    id: &'static str,
    label: &'static str,
    accessible: &'static str,
    tip: &'static str,
    shortcut: Option<(&'static str, &'static str, &'static str)>,
) -> AnyElement {
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
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip)).into())
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
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip)).into())
            .child(label)
            .into_any_element(),
    }
}

pub(crate) const SEARCH_PLACEHOLDER: &str = "Search configurations";

/// T-23: the one hint that the `port=` grammar exists — a tooltip on the search
/// box, because the pinned placeholder is too narrow to carry it.
pub(crate) const SEARCH_HINT: &str =
    "port=N or port=N:M filters by listen port (port=10801, port=10800:10900, port=10801:, port=:10900)";

/// Store meta key for "Automatically restart crashed cores"; a missing or corrupt
/// value reads as enabled, matching the engine's default.
pub(crate) const META_AUTO_RESTART: &str = "auto_restart_enabled";

pub(crate) fn is_row_local_start_error(error: &EngineError) -> bool {
    crate::engine::is_row_local_error(error)
}

pub(crate) fn normalize_test_url(raw: &str) -> Result<String, String> {
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

pub(crate) fn effective_probe_url(row_override: Option<&str>, global: Option<&str>) -> String {
    row_override
        .filter(|url| !url.trim().is_empty())
        .or_else(|| global.filter(|url| !url.trim().is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_TEST_URL.to_string())
}

pub(crate) fn format_params_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("params are not valid JSON: {error}"))?;
    serde_json::to_string_pretty(&value)
        .map_err(|error| format!("params cannot be formatted: {error}"))
}

pub(crate) fn validate_params_json(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map(|_| ())
        .map_err(|error| format!("params are not valid JSON: {error}"))
}

pub(crate) fn guided_identity_label(protocol_raw: &str) -> String {
    match guided_identity_fields(protocol_raw).and_then(|guided| guided.key) {
        Some(key) => format!("Identity ({key})"),
        None => "Identity".to_string(),
    }
}

pub(crate) fn guided_hint_line(protocol_raw: &str) -> Option<String> {
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

pub(crate) fn block_owns_identity(protocol_raw: &str) -> bool {
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

pub(crate) fn tls_panel_plan(protocol_raw: &str) -> Option<bool> {
    match protocol_raw.trim() {
        "vless" | "vmess" | "trojan" | "http" | "https" => Some(false),
        "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls" => Some(true),
        _ => None,
    }
}

pub(crate) fn has_network_panel(protocol_raw: &str) -> bool {
    matches!(protocol_raw.trim(), "vless" | "vmess" | "trojan")
}

pub(crate) fn has_dial_panel(protocol_raw: &str) -> bool {
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

pub(crate) fn has_advanced_dialog(protocol_raw: &str) -> bool {
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
    Some(format!(
        "out {} · in {}",
        format_bytes(tx),
        format_bytes(rx)
    ))
}

pub(crate) fn interval_picker_row(
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

pub(crate) fn spawn_tick(
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
