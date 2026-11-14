//! Shared helpers for the split UI suite. Imported by every binary; nothing
//! here runs a test itself. The original suite's import block is kept so the
//! helper bodies compile unchanged. Items are pub so the binaries' glob
//! import reaches them.
#![allow(dead_code)]
// The whole test suite: pure-logic tests and gpui test-app paints.
// Belongs: every `#[test]`/`#[gpui_kit::test]` and its local fixtures.
// Not here: production helpers — a test that needs one promotes it to a module.
pub use std::cell::RefCell;
pub use std::rc::Rc;

pub use gpui_kit::component::dialog::Dialog;
pub use gpui_kit::component::{IndexPath, Root, WindowExt};
pub use gpui_kit::test::TestWindowExt;
pub use gpui_kit::{
    point, px, size, AppContext, Bounds, Context, Entity, Hsla, TestAppContext, Window,
};
pub use manyproxy_core::Secret;
pub use manyproxy_ui::engine::AutoRestartConfig;
pub use manyproxy_ui::import::OpenWithKind;
pub use manyproxy_ui::overlay_open;
pub use manyproxy_ui::store::{
    AutoClearGroupConfig, HealthSnapshot, NewConfig, Store, StoreError, WindowGeometry,
};
pub use manyproxy_ui::AppView;
pub use manyproxy_ui::ColumnId;
pub use manyproxy_ui::ColumnLayout;
pub use manyproxy_ui::ConfigForm;
pub use manyproxy_ui::ExitTraffic;

pub fn seed_input(name: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: Secret::default(),
        group: String::new(),
        bind_ip: String::new(),
        port: String::new(),
    }
}

pub fn test_facts(delegate: &manyproxy_ui::LibraryDelegate) -> manyproxy_ui::QueryFacts<'_> {
    manyproxy_ui::QueryFacts {
        running: &delegate.running,
        health: &delegate.health,
        crashed: &delegate.crashed,
        latency: &delegate.latency,
    }
}

/// The pure query over a delegate's mirrors, for tests that exercise matching
/// without mounting a window.
pub fn test_visible(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<usize> {
    manyproxy_ui::filter(
        &delegate.rows,
        &delegate.search_index,
        &test_facts(delegate),
        &manyproxy_ui::LibraryScope::All,
        query,
    )
}

/// The app's filter-then-sort order, reproduced without an `AppView`.
pub fn test_sorted(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<usize> {
    let mut visible = test_visible(delegate, query);
    if !delegate.sort.is_none() {
        if delegate.sort.key == manyproxy_ui::store::LibrarySortKey::TypeSecurity {
            visible.sort_by(|&a, &b| {
                manyproxy_ui::compare_type_security(&delegate.rows[a], &delegate.rows[b])
            });
        } else {
            let context = manyproxy_ui::SortContext {
                latency: &delegate.latency,
                data_traffic: &delegate.data_traffic,
                totals: &delegate.traffic,
                out_ips: &delegate.out_ips,
                upstreams: &delegate.upstream_cache,
            };
            visible.sort_by(|&a, &b| {
                manyproxy_ui::compare_library_rows(
                    &context,
                    delegate.sort,
                    &delegate.rows[a],
                    &delegate.rows[b],
                )
            });
        }
    }
    visible
}

pub fn test_names(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<String> {
    test_sorted(delegate, query)
        .into_iter()
        .map(|index| delegate.rows[index].name.clone())
        .collect()
}

/// WCAG 2.1 relative luminance of `color` composited over `background`.
pub fn relative_luminance(color: Hsla, background: Hsla) -> f32 {
    let front = color.to_rgb();
    let back = background.to_rgb();
    let channel = |value: f32| {
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let alpha = front.a.clamp(0., 1.);
    let mix = |front: f32, back: f32| front.mul_add(alpha, back * (1. - alpha));
    0.2126 * channel(mix(front.r, back.r))
        + 0.7152 * channel(mix(front.g, back.g))
        + 0.0722 * channel(mix(front.b, back.b))
}

/// WCAG 2.1 contrast ratio of a text tone against the surface behind it.
pub fn contrast_ratio(color: Hsla, background: Hsla) -> f32 {
    let text = relative_luminance(color, background);
    let surface = relative_luminance(background, background);
    let (lighter, darker) = if text >= surface {
        (text, surface)
    } else {
        (surface, text)
    };
    (lighter + 0.05) / (darker + 0.05)
}

pub fn paint_tall_dialog(cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1000.), px(1800.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
}

pub fn t07_open_add_on(view: &Entity<AppView>, cx: &mut VisualTestContext, protocol: &str) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of(protocol);
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
}

pub fn tick12_seed(name: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: String::new(),
        bind_ip: String::new(),
        port: port.to_string(),
    }
}

pub fn stub_clash_server(bodies: Vec<String>, secret: &str) -> (u16, std::thread::JoinHandle<()>) {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
    let port = listener.local_addr().expect("addr works").port();
    let secret = secret.to_string();
    let server = std::thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept works");
            let mut head = [0u8; 2048];
            let n = stream.read(&mut head).unwrap_or(0);
            let authed = String::from_utf8_lossy(&head[..n])
                .contains(&format!("Authorization: Bearer {secret}"));
            let (status, payload) = if authed {
                ("200 OK", body)
            } else {
                (
                    "401 Unauthorized",
                    r#"{"error":"unauthorized"}"#.to_string(),
                )
            };
            stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                            payload.len()
                        )
                        .as_bytes(),
                    )
                    .expect("reply works");
        }
    });
    (port, server)
}

pub fn traffic_body(row: &str, rx: u64, tx: u64) -> String {
    serde_json::json!({
        "connections": [
            {"download": rx, "id": "conn-1",
             "metadata": {"destinationIP": "93.184.216.34", "destinationPort": "80",
                          "host": "example.com", "network": "tcp",
                          "sourceIP": "127.0.0.1", "sourcePort": "51241",
                          "type": format!("mixed/in-{row}")},
             "upload": tx},
            {"download": 500, "id": "probe-1",
             "metadata": {"destinationIP": "93.184.216.34", "destinationPort": "80",
                          "host": "www.gstatic.com", "network": "tcp",
                          "sourceIP": "127.0.0.1", "sourcePort": "51242",
                          "type": format!("mixed/in-{row}")},
             "upload": 60},
        ],
        "downloadTotal": 1500, "uploadTotal": 110,
    })
    .to_string()
}

pub fn bug18_probe_body(row: &str, gstatic: Option<(u64, u64)>, custom: (u64, u64)) -> String {
    let mut connections = vec![serde_json::json!({
        "download": 1000, "id": "data-1",
        "metadata": {"destinationIP": "93.184.216.34", "destinationPort": "80",
                     "host": "example.com", "network": "tcp",
                     "sourceIP": "127.0.0.1", "sourcePort": "51241",
                     "type": format!("mixed/in-{row}")},
        "upload": 50
    })];
    if let Some((rx, tx)) = gstatic {
        connections.push(serde_json::json!({
            "download": rx, "id": "probe-gstatic",
            "metadata": {"destinationIP": "93.184.216.34", "destinationPort": "80",
                         "host": "www.gstatic.com", "network": "tcp",
                         "sourceIP": "127.0.0.1", "sourcePort": "51242",
                         "type": format!("mixed/in-{row}")},
            "upload": tx
        }));
    }
    connections.push(serde_json::json!({
        "download": custom.0, "id": "probe-custom",
        "metadata": {"destinationIP": "203.0.113.9", "destinationPort": "80",
                     "host": "203.0.113.9", "network": "tcp",
                     "sourceIP": "127.0.0.1", "sourcePort": "51243",
                     "type": format!("mixed/in-{row}")},
        "upload": custom.1
    }));
    serde_json::json!({"connections": connections, "downloadTotal": 9_999, "uploadTotal": 999})
        .to_string()
}

pub fn traffic_body_multi(flows: &[(&str, u64, u64)]) -> String {
    let mut connections: Vec<serde_json::Value> = flows
        .iter()
        .enumerate()
        .map(|(ix, (row, rx, tx))| {
            serde_json::json!({
                "download": rx, "id": format!("conn-{ix}"),
                "metadata": {"destinationIP": "93.184.216.34", "destinationPort": "80",
                             "host": format!("flow-{ix}.example.com"), "network": "tcp",
                             "sourceIP": "127.0.0.1", "sourcePort": "51241",
                             "type": format!("mixed/in-{row}")},
                "upload": tx
            })
        })
        .collect();
    connections.push(serde_json::json!({
        "download": 500, "id": "probe-9",
        "metadata": {"destinationIP": "93.184.216.34", "destinationPort": "80",
                     "host": "www.gstatic.com", "network": "tcp",
                     "sourceIP": "127.0.0.1", "sourcePort": "51242",
                     "type": format!("mixed/in-{}", flows[0].0)},
        "upload": 60
    }));
    serde_json::json!({
        "connections": connections,
        "downloadTotal": 1500, "uploadTotal": 110,
    })
    .to_string()
}

/// Tall enough that the seeded Connections row lands inside the detail
/// viewport (the default 1000x700 frame clips it below the fold).
pub fn t74_paint(cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1400.), px(1200.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
}

/// T-24 hermetic Connections pane: one claimed exit row, seeded snapshot,
/// no engines and no network, ready for click simulation.
pub fn t74_conn_setup<'a>(
    cx: &'a mut TestAppContext,
    dest: &str,
) -> (Entity<AppView>, &'a mut VisualTestContext, String) {
    use manyproxy_ui::health::ClashConn;

    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let alpha = store
        .add(tick12_seed("Alpha", "10856"))
        .expect("seed works");
    let row_id = alpha.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.conn.snapshot = vec![(
                "stock".to_string(),
                ClashConn {
                    id: "conn-0".to_string(),
                    row_id: Some(row_id.clone()),
                    rx_bytes: 1000,
                    tx_bytes: 50,
                    dest: dest.to_string(),
                    network: "tcp".to_string(),
                },
            )];
            view.conn.delta = std::collections::HashMap::new();
            // Slice 6: connections live in the console now, not the inspector.
            view.console.open = true;
            view.console.tab = manyproxy_ui::ConsoleTab::Connections;
            view.console.scope = manyproxy_ui::ConsoleScope::All;
            view.conn.sort_desc = true;
            view.select_visible_row(0, cx);
            view.open_inspector();
            cx.notify();
        });
    });
    t74_paint(cx);
    cx.run_until_parked();
    let endpoint = view.read_with(cx, |view, cx| {
        let rows = view.libraries.read(cx).delegate().rows.clone();
        let selected = view.selected_id_set(cx);
        let built = manyproxy_ui::build_conn_rows(
            &view.conn.snapshot,
            &view.conn.delta,
            &selected,
            view.console.scope == manyproxy_ui::ConsoleScope::Selected,
            view.conn.sort_desc,
        );
        manyproxy_ui::conn_exit_of(&rows, built[0].row_id.as_deref()).1
    });
    (view, cx, endpoint)
}

pub fn seed_lan_input(name: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: Secret::default(),
        group: String::new(),
        bind_ip: "0.0.0.0".to_string(),
        port: String::new(),
    }
}

pub use gpui_kit::VisualTestContext;

pub fn guided_add_form(name: &str, window: &mut Window, cx: &mut Context<AppView>) -> ConfigForm {
    let form = ConfigForm::blank(window, cx);
    form.name.update(cx, |state, cx| {
        state.set_value(name, window, cx);
    });
    form.guided_server.update(cx, |state, cx| {
        state.set_value("198.51.100.9", window, cx);
    });
    form.guided_port.update(cx, |state, cx| {
        state.set_value("443", window, cx);
    });
    form.guided_identity.update(cx, |state, cx| {
        state.set_value("11111111-2222-4333-8444-555555555555", window, cx);
    });
    form
}

pub fn mount_view(
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

/// T-08: mount through the production memory fallback, carrying the error from
/// a real failed `Store::open_file`. This is the same constructor `AppView::new`
/// calls when the file store will not open.
#[cfg(windows)]
pub fn mount_fallback_view(
    error: StoreError,
    path: std::path::PathBuf,
    cx: &mut TestAppContext,
) -> (Entity<AppView>, &mut VisualTestContext) {
    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::fell_back_to_memory(window, cx, error, &path));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");
    (view, cx)
}

/// T-08: a real folder inside TEMP, writable unless `deny_write` says
/// otherwise. The denial is a deny-write ACE for the current user — the
/// cheapest faithful simulation on Windows: reads keep working,
/// `create_dir_all` keeps working (the folder exists), and creating a file —
/// which is exactly what the write probe does — is refused. The ACE is removed
/// on drop, before the folder is removed.
#[cfg(windows)]
pub struct T08TempFolder {
    pub dir: std::path::PathBuf,
    pub denied: bool,
}

#[cfg(windows)]
impl T08TempFolder {
    pub fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "manyproxy-t08-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("T-08 temp folder created");
        Self { dir, denied: false }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.dir
    }

    pub fn deny_write(&mut self) {
        if self.denied {
            return;
        }
        let principal = format!(
            "{}:(W)",
            std::env::var("USERNAME").expect("USERNAME is set")
        );
        Self::icacls(&self.dir, "/deny", &principal);
        self.denied = true;
    }

    pub fn allow_write(&mut self) {
        if !self.denied {
            return;
        }
        let principal = std::env::var("USERNAME").expect("USERNAME is set");
        Self::icacls(&self.dir, "/remove:d", &principal);
        self.denied = false;
    }

    pub fn icacls(dir: &std::path::Path, flag: &str, principal: &str) {
        let output = std::process::Command::new("icacls")
            .arg(dir)
            .arg(flag)
            .arg(principal)
            .output()
            .unwrap_or_else(|error| {
                panic!("icacls {flag} {principal} on {}: {error}", dir.display())
            });
        assert!(
            output.status.success(),
            "icacls {flag} {principal} on {} failed: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[cfg(windows)]
impl Drop for T08TempFolder {
    fn drop(&mut self) {
        if self.denied {
            if let Ok(principal) = std::env::var("USERNAME") {
                let _ = std::process::Command::new("icacls")
                    .arg(&self.dir)
                    .arg("/remove:d")
                    .arg(principal)
                    .status();
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn notification_count(cx: &mut VisualTestContext) -> usize {
    cx.update(|window, cx| window.notifications(cx).len())
}

/// T-03: the inspector carries the SELECTED row's editors, so a test that
/// needs them selects the row first — the same way a click does.
pub fn select_library_row(view: &Entity<AppView>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        window.click(format!("row-name-text-{id}"), cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
    let selected = view.read_with(cx, |view, _| view.selection_focus.clone());
    assert_eq!(selected.as_deref(), Some(id), "the row is selected");
}

/// F19: the inspector starts closed and opens from a row pick. Tests that set
/// the selection by hand skip that pick, so they open it the way the app would.
pub fn open_inspector(view: &Entity<AppView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.open_inspector();
        cx.notify();
    });
    cx.run_until_parked();
}

pub fn ticket43_seed(name: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: String::new(),
        bind_ip: String::new(),
        port: port.to_string(),
    }
}

pub fn t73_port_row(
    id: &str,
    name: &str,
    protocol: &str,
    group: &str,
    port: u16,
) -> manyproxy_ui::store::ProxyConfig {
    manyproxy_ui::store::ProxyConfig {
        id: id.to_string(),
        name: name.to_string(),
        protocol: protocol.to_string(),
        params: r#"{"server":"t73.example.com","server_port":443}"#.to_string().into(),
        group: group.to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    }
}

pub fn t73_visible_ports(
    delegate: &manyproxy_ui::LibraryDelegate,
    query: &manyproxy_ui::FilterQuery,
) -> Vec<u16> {
    test_visible(delegate, query)
        .into_iter()
        .map(|index| delegate.rows[index].port)
        .collect()
}

pub fn t73_filtered(
    rows: Vec<manyproxy_ui::store::ProxyConfig>,
    query: &str,
) -> (Vec<u16>, manyproxy_ui::FilterQuery) {
    let mut delegate = manyproxy_ui::LibraryDelegate::new();
    delegate.set_rows(rows);
    let mut filter_query = manyproxy_ui::FilterQuery::default();
    filter_query.set_search(query);
    (t73_visible_ports(&delegate, &filter_query), filter_query)
}

pub fn t73_rows() -> Vec<manyproxy_ui::store::ProxyConfig> {
    vec![
        t73_port_row("a", "Alpha", "vless", "g1", 10801),
        t73_port_row("b", "Beta", "vless", "g1", 10802),
        t73_port_row("c", "Gamma", "vmess", "g2", 10910),
        t73_port_row("d", "Delta", "trojan", "g2", 20000),
    ]
}

/// T-05: `Copy proxies` yields one TAB-separated line per running checked
/// row — two spreadsheet columns, the name and the URL.
///
/// T-05: the copy toast is honest about what it skipped.

pub fn paint_dialog_open(cx: &mut VisualTestContext) {
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

pub fn paint_frame(cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
}

pub fn assert_painted(cx: &mut VisualTestContext, id: &str) {
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

pub fn assert_absent(cx: &mut VisualTestContext, id: String) {
    assert!(
        cx.update(|window, _| window.try_find(id.clone()).is_none()),
        "{id} is not in the completed frame"
    );
}

/// The accessible name of a painted element. K12 pins the visible status/sort
/// grammar there, so a label assertion is also a text assertion.
pub fn label_of(id: &str, cx: &mut VisualTestContext) -> String {
    cx.update(|window, _| {
        window
            .try_find(id.to_string())
            .unwrap_or_else(|| panic!("{id} renders"))
            .label()
            .map(str::to_string)
            .unwrap_or_else(|| panic!("{id} names itself"))
    })
}

pub fn row19_seed(name: &str, host: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: format!(
            "{{\"type\":\"vless\",\"server\":\"{host}\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\"}}"
        )
        .into(),
        group: "travel".to_string(),
        bind_ip: "127.0.0.1".to_string(),
        port: port.to_string(),
    }
}

pub fn facet_seed(name: &str, protocol: &str, group: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: protocol.to_string(),
        params: format!(
            "{{\"type\":\"{protocol}\",\"server\":\"seed.example.com\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\"}}"
        )
        .into(),
        group: group.to_string(),
        bind_ip: "127.0.0.1".to_string(),
        port: port.to_string(),
    }
}

pub fn t26_engine_seed(name: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: String::new(),
        bind_ip: String::new(),
        port: port.to_string(),
    }
}

/// Paint the frame at an explicit size. The repo's `paint_frame` pins the
/// 1000x700 frame; the geometry pins need the wide frame and the minimum.
pub fn paint_frame_at(cx: &mut VisualTestContext, width: f32, height: f32) {
    cx.simulate_resize(size(px(width), px(height)));
    // The resize lands before the first draw, so the frame is laid out at
    // the size it asks for.
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
}

pub fn t30_engine_seed(name: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: String::new(),
        bind_ip: String::new(),
        port: port.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Slice 3 — the filter row
// ---------------------------------------------------------------------------

// K20/D1: the inspector switcher sizes to its five labels instead of five
// fixed slots, at one fixed text size — `Details · Health · Logs · Links ·
// Traffic` fit whole, so nothing scales them. Checked at the default drawer
// width, on the wide side panel, and at the 240px panel floor.

pub fn t37_route_link(name: &str) -> String {
    use base64::Engine as _;
    let payload = base64::engine::general_purpose::STANDARD.encode(format!(
            "{{\"name\":\"{name}\",\"rules\":[{{\"action\":\"block\",\"domain_suffix\":[\"ads.example\"]}}]}}"
        ));
    format!("throne://route/{payload}")
}

/// B17: the one settings column is taller than the dialog, so a section below
/// the fold has to be scrolled to before it can be painted or clicked. Rewind,
/// then walk down until the target's top sits inside the body.
pub fn scroll_settings_into_view(cx: &mut VisualTestContext, id: &str) {
    pub fn wheel(cx: &mut VisualTestContext, delta: f32) {
        cx.update(|window, cx| {
            window.scroll(
                "settings-body",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(delta))),
                cx,
            );
        });
        cx.run_until_parked();
        paint_dialog_open(cx);
    }
    wheel(cx, 100_000.);
    for _ in 0..40 {
        let (top, body_top, body_bottom) = cx.update(|window, _| {
            (
                window
                    .try_find(id.to_string())
                    .map(|snapshot| snapshot.bounds().origin.y),
                window.find("settings-body").bounds().origin.y,
                window.find("settings-body").bounds().bottom(),
            )
        });
        match top {
            Some(top) if top >= body_top && top <= body_bottom - px(48.) => return,
            Some(_) => wheel(cx, -120.),
            None => return,
        }
    }
    panic!("{id} never scrolled into the settings column's viewport");
}

/// B17: the six spine headers, top to bottom. A settings section below the
/// scroll viewport is not painted, so a test walks the column and unions what
/// each step shows.
pub const SETTINGS_HEADERS: [&str; 6] = [
    "settings-section-general",
    "settings-section-probing",
    "settings-section-updates",
    "settings-section-engine",
    "settings-section-maintenance",
    "settings-section-shortcuts",
];

/// B17: the headers painted right now, in the column's order.
pub fn settings_headers_painted(cx: &mut VisualTestContext) -> Vec<&'static str> {
    SETTINGS_HEADERS
        .into_iter()
        .filter(|id| cx.update(|window, _| window.try_find(id.to_string()).is_some()))
        .collect()
}

/// B17: scroll the one settings column from top to bottom, returning the
/// headers seen at each step. Every step is in the column's order.
pub fn settings_spine_walk(cx: &mut VisualTestContext) -> Vec<Vec<&'static str>> {
    let mut steps = Vec::new();
    let mut seen: Vec<&'static str> = Vec::new();
    for step in 0..12 {
        if step > 0 {
            cx.update(|window, cx| {
                window.scroll(
                    "settings-body",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-320.))),
                    cx,
                );
            });
            cx.run_until_parked();
            paint_dialog_open(cx);
        }
        let painted = settings_headers_painted(cx);
        let first_new = painted.iter().find(|id| !seen.contains(id)).copied();
        let fresh: Vec<&'static str> = painted
            .iter()
            .copied()
            .filter(|id| !seen.contains(id))
            .collect();
        seen.extend(fresh);
        steps.push(painted);
        // Stop once the column has nothing new to show and the tail is up.
        if first_new.is_none()
            && cx.update(|window, _| {
                window
                    .try_find("settings-section-shortcuts".to_string())
                    .is_some()
            })
        {
            break;
        }
    }
    steps
}

pub fn t64_seed_in_group(name: &str, group: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: group.to_string(),
        bind_ip: String::new(),
        port: port.to_string(),
    }
}

pub fn restore_seed(name: &str, bind: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: String::new(),
        bind_ip: bind.to_string(),
        port: port.to_string(),
    }
}

/// Functions that run inside `cx.open_window`'s closure, before the window
/// adopts its `Root` layer, so none of them may reach an overlay directly:
/// `Root::update` panics on a window whose first layer is not a
/// `gpui_component::Root`. They hand the overlay to a `*_after_root`
/// helper, or — when the same path also serves ordinary post-Root use — to
/// `notify_now_or_after_root`, which decides at runtime. That helper is the
/// only root-checked direct push, so this list stays complete without
/// exceptions: `reload` boots via `restore_on_launch` ->
/// `commit_restore_subset` and is listed here for that reason. Its
/// Root-READ calls (`has_active_dialog` / `has_active_sheet` /
/// `close_sheet`, which panic through `Root::read` rather than
/// `Root::update`) are guarded by the same `root_installed` check, so they
/// cannot fire before the Root exists either.
pub const BOOT_FUNCTIONS: [&str; 9] = [
    "fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {\n        match Store::open_file",
    "fn fell_back_to_memory(",
    "fn with_store(window: &mut Window",
    "fn create_tray(&mut self",
    "fn restore_on_launch(&mut self",
    "fn commit_restore_subset(",
    "fn open_restore_lan_confirm(",
    "fn handle_launch_intake(",
    "fn reload(&mut self",
];

/// Matched by method name, so any receiver spelling (`window.`, `handle.`,
/// a chained call) is caught, not just the one this file happens to use.
pub const OVERLAY_CALLS: [&str; 3] = [
    ".push_notification(",
    ".open_dialog(",
    ".open_alert_dialog(",
];

/// Direct overlay calls inside `markers`, ignoring literals and comments.
/// Empty means every listed function defers. Each marker must name exactly
/// one function, so a rename cannot silently leave the scan on a body it
/// did not mean to check.
pub fn direct_overlay_calls(source: &str, markers: &[&str]) -> Vec<String> {
    // Normalize line endings first: a checkout with CRLF (Windows
    // `core.autocrlf`) would otherwise break the multi-line markers,
    // which are written with `\n`.
    let normalized = source.replace("\r\n", "\n");
    let stripped = strip_literals_and_comments(&normalized);
    let mut found = Vec::new();
    for marker in markers {
        assert_eq!(
            stripped.matches(marker).count(),
            1,
            "marker names exactly one function: {marker}"
        );
        let body = function_body(&stripped, marker);
        for call in OVERLAY_CALLS {
            if body.contains(call) {
                found.push(format!("{marker} -> {call}"));
            }
        }
    }
    found
}

/// `source` with the contents of string/char/raw literals and of line and
/// block comments blanked to spaces. Newlines survive, so markers spanning
/// lines still match and the brace count sees only real code. `'` lifetimes
/// (`'a`, `'static`) stay: only a closed `'x'` / `'\n'` is a char literal.
pub fn strip_literals_and_comments(source: &str) -> String {
    pub fn blanked(ch: char) -> char {
        if ch == '\n' {
            '\n'
        } else {
            ' '
        }
    }

    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(blanked(chars[i]));
                i += 1;
            }
            continue;
        }
        if ch == '/' && chars.get(i + 1) == Some(&'*') {
            let mut depth = 0usize;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blanked(chars[i]));
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"..", r#".."#, br##".."##.
        let raw_prefix = if ch == 'r' {
            Some(i + 1)
        } else if matches!(ch, 'b' | 'c') && chars.get(i + 1) == Some(&'r') {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_prefix {
            let mut hashes = 0usize;
            while chars.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                j += 1;
                while j < chars.len() {
                    if chars[j] == '"' {
                        let mut close = j + 1;
                        let mut seen = 0usize;
                        while seen < hashes && chars.get(close) == Some(&'#') {
                            seen += 1;
                            close += 1;
                        }
                        if seen == hashes {
                            j = close;
                            break;
                        }
                    }
                    j += 1;
                }
                for ch in &chars[i..j.min(chars.len())] {
                    out.push(blanked(*ch));
                }
                i = j;
                continue;
            }
        }
        if ch == '"' {
            out.push(' ');
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '\\' => {
                        out.push_str("  ");
                        i += 2;
                    }
                    '"' => {
                        out.push(' ');
                        i += 1;
                        break;
                    }
                    other => {
                        out.push(blanked(other));
                        i += 1;
                    }
                }
            }
            continue;
        }
        let char_literal = matches!(
            (chars.get(i + 1), chars.get(i + 2)),
            (Some('\\'), _) | (Some(_), Some('\''))
        );
        if ch == '\'' && char_literal {
            out.push(' ');
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '\\' => {
                        out.push_str("  ");
                        i += 2;
                    }
                    '\'' => {
                        out.push(' ');
                        i += 1;
                        break;
                    }
                    other => {
                        out.push(blanked(other));
                        i += 1;
                    }
                }
            }
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// The brace-matched body of the function starting at `marker`. Expects the
/// literal- and comment-free text from `strip_literals_and_comments`.
pub fn function_body<'a>(source: &'a str, marker: &str) -> &'a str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing function: {marker}"));
    let open = start + source[start..].find('{').expect("function body opens");
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced body: {marker}");
}

pub fn t48_recording_client(
    handler: impl Fn(String) -> gpui_kit::http_client::http::Response<gpui_kit::http_client::AsyncBody>
        + Send
        + Sync
        + 'static,
) -> (
    std::sync::Arc<dyn gpui_kit::http_client::HttpClient>,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    use gpui_kit::http_client::FakeHttpClient;
    let handler = std::sync::Arc::new(handler);
    let fetched = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = fetched.clone();
    let client = FakeHttpClient::create(move |req| {
        let seen = seen.clone();
        let handler = handler.clone();
        let uri = req.uri().to_string();
        async move {
            seen.lock().expect("record works").push(uri.clone());
            Ok(handler(uri))
        }
    });
    (client, fetched)
}

pub fn t48_response(
    status: u16,
    location: Option<&str>,
    body: &str,
) -> gpui_kit::http_client::http::Response<gpui_kit::http_client::AsyncBody> {
    use gpui_kit::http_client::AsyncBody;
    let mut builder = gpui_kit::http_client::http::Response::builder().status(status);
    if let Some(location) = location {
        builder = builder.header("location", location);
    }
    builder
        .body(AsyncBody::from(body.to_string()))
        .expect("response builds")
}

pub fn t48_public_resolver(_host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    Ok(vec!["93.184.216.34".parse().expect("ip parses")])
}

pub fn t48_internal_resolver(_host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    Ok(vec!["127.0.0.1".parse().expect("ip parses")])
}

pub fn t48_dead_resolver(_host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    Err("no such host".to_string())
}

pub type T56HeaderLog = std::sync::Arc<std::sync::Mutex<Vec<Vec<(String, String)>>>>;

pub fn t56_recording_feed_client(
    handler: impl Fn(String) -> gpui_kit::http_client::http::Response<gpui_kit::http_client::AsyncBody>
        + Send
        + Sync
        + 'static,
) -> (
    std::sync::Arc<dyn gpui_kit::http_client::HttpClient>,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    T56HeaderLog,
) {
    use gpui_kit::http_client::FakeHttpClient;
    let handler = std::sync::Arc::new(handler);
    let uris = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let header_log: T56HeaderLog = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = uris.clone();
    let seen_headers = header_log.clone();
    let client = FakeHttpClient::create(move |req| {
        let seen = seen.clone();
        let seen_headers = seen_headers.clone();
        let handler = handler.clone();
        let uri = req.uri().to_string();
        let headers = req
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or("").to_string(),
                )
            })
            .collect::<Vec<_>>();
        async move {
            seen.lock().expect("record works").push(uri.clone());
            seen_headers.lock().expect("record works").push(headers);
            Ok(handler(uri))
        }
    });
    (client, uris, header_log)
}

pub fn t56_release_json(tag: &str, prerelease: bool, html_url: &str, name: &str) -> String {
    format!(
        r#"{{"tag_name":"{tag}","prerelease":{prerelease},"html_url":"{html_url}","name":"{name}"}}"#
    )
}

pub fn t56_newer_release() -> manyproxy_ui::FetchedRelease {
    manyproxy_ui::FetchedRelease {
        tag_name: "v999.0.1".to_string(),
        prerelease: false,
        html_url: Some("https://github.com/mahyartdb/ManyProxy/releases/tag/v999.0.1".to_string()),
        release_name: Some("ManyProxy v999.0.1".to_string()),
    }
}

pub fn t56_public_resolver(_host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    Ok(vec!["93.184.216.34".parse().expect("ip parses")])
}

pub fn t56_no_hwid_header(log: &[Vec<(String, String)>]) -> bool {
    log.iter()
        .flatten()
        .all(|(name, _)| !name.to_ascii_lowercase().contains("hwid"))
}

// T-07: the outbound fetch layer identifies itself as exactly
// `ManyProxy/<version>` on every request of every hop, and never sends
// any device-identification header (Throne's x-hwid / x-device-*
// family is banned capability — pinned here, not merely documented).

pub fn t57_expected_user_agent() -> String {
    format!("ManyProxy/{}", manyproxy_ui::APP_VERSION)
}

pub fn t57_assert_user_agent_on_every_capture(log: &[Vec<(String, String)>]) {
    assert!(
        !log.is_empty(),
        "at least one outgoing request must have been captured"
    );
    for (hop, headers) in log.iter().enumerate() {
        let user_agents: Vec<String> = headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
            .map(|(_, value)| value.clone())
            .collect();
        assert!(
            user_agents.len() == 1,
            "every request carries exactly one user-agent, hop {hop} sent {user_agents:?}"
        );
        assert_eq!(
            user_agents[0],
            t57_expected_user_agent(),
            "hop {hop} announces exactly ManyProxy/<version>"
        );
    }
}

pub fn t57_assert_no_device_identifier_headers(log: &[Vec<(String, String)>]) {
    for (hop, headers) in log.iter().enumerate() {
        for (name, _) in headers {
            let lowered = name.to_ascii_lowercase();
            assert!(
                !lowered.contains("hwid"),
                "no hwid-named header may ride outbound fetches, hop {hop} sent {name:?}"
            );
            assert!(
                !lowered.starts_with("x-device"),
                "no x-device header may ride outbound fetches, hop {hop} sent {name:?}"
            );
        }
    }
}

pub fn assert_within_frame(cx: &mut VisualTestContext, id: &str) {
    let bounds = cx
        .update(|window, _| window.try_find(id.to_owned()))
        .unwrap_or_else(|| panic!("{id} renders: missing element in completed frame"))
        .bounds();
    assert!(
        bounds.origin.x >= px(0.) && bounds.origin.y >= px(0.),
        "{id} starts inside the frame: {bounds:?}"
    );
    assert!(
        bounds.origin.x + bounds.size.width <= px(1000.) + px(1.),
        "{id} ends inside the frame width: {bounds:?}"
    );
    assert!(
        bounds.origin.y + bounds.size.height <= px(700.) + px(1.),
        "{id} ends inside the frame height: {bounds:?}"
    );
}

pub fn paint_frame_min(cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1100.), px(640.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
}

pub fn assert_within_frame_min(cx: &mut VisualTestContext, id: &str) {
    let bounds = cx
        .update(|window, _| window.try_find(id.to_owned()))
        .unwrap_or_else(|| panic!("{id} renders: missing element in completed frame"))
        .bounds();
    assert!(
        bounds.origin.x >= px(0.) && bounds.origin.y >= px(0.),
        "{id} starts inside the minimum frame: {bounds:?}"
    );
    assert!(
        bounds.origin.x + bounds.size.width <= px(1100.) + px(1.),
        "{id} ends inside the minimum frame width: {bounds:?}"
    );
    assert!(
        bounds.origin.y + bounds.size.height <= px(640.) + px(1.),
        "{id} ends inside the minimum frame height: {bounds:?}"
    );
}

pub fn t19_auto_clear_seed(name: &str, port: &str, group: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: group.to_string(),
        bind_ip: String::new(),
        port: port.to_string(),
    }
}

pub fn t19_dead_snapshot(id: &str, error: &str) -> HealthSnapshot {
    HealthSnapshot {
        config_id: id.to_string(),
        at: 1,
        kind: "url-test".to_string(),
        latency_ms: None,
        error: Some(error.to_string()),
    }
}

/// Every key the app binds in its own contexts, in the canonical form
/// `KeyBinding::unparse` writes: lower-case modifiers, then the key. One key
/// can be bound more than once (Escape clears from both the shell and the
/// list), but the reference lists each shortcut once.
pub fn bound_key_strings(cx: &mut TestAppContext) -> Vec<String> {
    cx.update(|cx| {
        let keymap = cx.key_bindings();
        let map = keymap.borrow();
        let mut keys: Vec<String> = Vec::new();
        for binding in map.bindings().filter(|binding| {
            binding
                .predicate()
                .map(|predicate| {
                    predicate
                        .to_string()
                        .starts_with(manyproxy_ui::keys::APP_KEY_CONTEXT)
                })
                .unwrap_or(false)
        }) {
            let key = binding
                .keystrokes()
                .iter()
                .map(|keystroke| keystroke.unparse())
                .collect::<Vec<_>>()
                .join(" ");
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    })
}

/// T-04: the reference form of a displayed key: `Ctrl-N` → `ctrl-n`,
/// `Esc` → `escape`.
pub fn canonical_shortcut(display: &str) -> String {
    let lower = display.to_lowercase();
    match lower.as_str() {
        "esc" => "escape".to_string(),
        other => other.to_string(),
    }
}

pub const BUG16_LINK: &str = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one";

/// Serializes the one test that stages real rule-set files: locks the
/// process-global `MANYPROXY_RULE_SETS_DIR` override to a test staging
/// root for the guard's lifetime and clears it on drop.
pub fn t62_rule_sets_env(dir: &std::path::Path) -> T62RuleSetsEnvGuard {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let lease = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("MANYPROXY_RULE_SETS_DIR", dir);
    T62RuleSetsEnvGuard { _lease: lease }
}

pub struct T62RuleSetsEnvGuard {
    pub _lease: std::sync::MutexGuard<'static, ()>,
}

impl Drop for T62RuleSetsEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("MANYPROXY_RULE_SETS_DIR");
    }
}

pub fn t62_staged_rows(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
) -> Vec<manyproxy_ui::store::RuleSetRow> {
    view.read_with(cx, |view, _| {
        view.store.list_rule_sets().expect("list works")
    })
}

/// Runs the Add-by-URL commit (the add dialog's only meaningful action)
/// and reports whether the gate let the flow through to the confirm.
pub fn t62_begin(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
    name: &str,
    url: &str,
) -> bool {
    let outcome = Rc::new(RefCell::new(false));
    let recorded = outcome.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            recorded.replace(view.begin_rule_set_download(
                name.to_string(),
                url.to_string(),
                window,
                cx,
            ));
        });
    });
    let value = *outcome.borrow();
    value
}

pub fn t65_seed(name: &str, port: &str) -> NewConfig {
    NewConfig {
        name: name.to_string(),
        protocol: "vless".to_string(),
        params: "{\"type\":\"vless\",\"server\":\"198.51.100.9\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"security\":\"tls\",\"sni\":\"198.51.100.9\",\"network\":\"tcp\"}"
            .to_string()
            .into(),
        group: "travel".to_string(),
        bind_ip: "127.0.0.1".to_string(),
        port: port.to_string(),
    }
}

pub fn t65_mount_with_failed_probe<'a>(
    cx: &'a mut TestAppContext,
    ports: [&str; 3],
) -> (
    Entity<AppView>,
    &'a mut VisualTestContext,
    String,
    String,
    manyproxy_ui::store::ProxyConfig,
) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let failed = store.add(t65_seed("Alpha", ports[0])).expect("seed works");
    let untested = store.add(t65_seed("Beta", ports[1])).expect("seed works");
    let gamma = store.add(t65_seed("Gamma", ports[2])).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            let (prober, handle) = FakeHealthProber::shared();
            handle.script_failure(&failed.id, "upstream refused");
            view.prober = Box::new(prober);
        });
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.start_all_engines(window, cx));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.select_visible_row(0, cx);
            // F19: setting the selection by hand skips the row pick that opens
            // the inspector.
            view.open_inspector();
        });
    });
    let selected = view.read_with(cx, |view, cx| {
        view.selected_config(cx).map(|config| config.id)
    });
    assert_eq!(
        selected.as_deref(),
        Some(failed.id.as_str()),
        "row 0 selects the failed probe's row for a targeted test"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.test_selected_health(window, cx));
    });
    cx.run_until_parked();
    paint_frame(cx);
    (view, cx, failed.id, untested.id, gamma)
}

// ---- T-27: header sort pickers -------------------------------------

pub use manyproxy_ui::store::ProxyConfig;
pub use manyproxy_ui::{LibraryDelegate, LibrarySortKey, SortDirection};
pub use std::collections::HashMap;

pub fn t77_row(id: &str, name: &str, protocol: &str, group: &str, port: u16) -> ProxyConfig {
    ProxyConfig {
        id: id.to_string(),
        name: name.to_string(),
        protocol: protocol.to_string(),
        params: "{}".to_string().into(),
        group: group.to_string(),
        enabled: true,
        bind_ip: "127.0.0.1".to_string(),
        port,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    }
}

pub fn t77_names(delegate: &LibraryDelegate) -> Vec<String> {
    t77_names_q(delegate, &manyproxy_ui::FilterQuery::default())
}

pub fn t77_names_q(delegate: &LibraryDelegate, query: &manyproxy_ui::FilterQuery) -> Vec<String> {
    test_sorted(delegate, query)
        .into_iter()
        .map(|index| delegate.rows[index].name.clone())
        .collect()
}

pub fn t77_map_u64(pairs: &[(&str, u64)]) -> HashMap<String, u64> {
    pairs
        .iter()
        .map(|(id, value)| (id.to_string(), *value))
        .collect()
}

pub fn t77_map_pair(pairs: &[(&str, u64, u64)]) -> HashMap<String, (u64, u64)> {
    pairs
        .iter()
        .map(|(id, rx, tx)| (id.to_string(), (*rx, *tx)))
        .collect()
}

pub fn t77_map_text(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(id, value)| (id.to_string(), value.to_string()))
        .collect()
}

pub fn t77_sort(key: LibrarySortKey, direction: SortDirection) -> manyproxy_ui::store::LibrarySort {
    manyproxy_ui::store::LibrarySort::new(key, direction)
}

// ---- T-28: share-dialog QR ------------------------------------------

pub const T78_KNOWN_PAYLOAD: &str = "socks5://127.0.0.1:10801";
/// Pinned fingerprint of the known payload's grid (quiet zone included) at EC M.
pub const T78_KNOWN_FINGERPRINT: u64 = 0xDD10_7000_ADAB_B569;

pub fn t78_row(bind_ip: &str, port: u16) -> ProxyConfig {
    ProxyConfig {
        id: format!("t78-{port}"),
        name: "Lan".to_string(),
        protocol: "vless".to_string(),
        params: "{}".to_string().into(),
        group: String::new(),
        enabled: true,
        bind_ip: bind_ip.to_string(),
        port,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    }
}

pub fn t78_new(bind_ip: &str, port: &str) -> NewConfig {
    NewConfig {
        bind_ip: bind_ip.to_string(),
        ..tick12_seed("Lan", port)
    }
}

pub fn socks_qr_side() -> usize {
    manyproxy_ui::qr_grid(T78_KNOWN_PAYLOAD)
        .expect("known payload encodes")
        .side
}

// ---- T-29: top-9 exits + Other --------------------------------------

pub fn t79_exit(id: &str, name: &str, rx: u64, tx: u64) -> ExitTraffic {
    ExitTraffic {
        row_id: id.to_string(),
        name: name.to_string(),
        bind: "127.0.0.1:10801".to_string(),
        rx_bytes: rx,
        tx_bytes: tx,
    }
}

// --- T-30: shard CPU/RAM sampler (pure math, ring, render) ---------------

pub fn shard_pair(label: &str, pid: u32) -> (String, u32) {
    (label.to_string(), pid)
}

pub fn sampled(cpu_100ns: u64, ram_bytes: u64) -> impl Fn(u32) -> manyproxy_ui::SampleRead {
    move |_pid| {
        manyproxy_ui::SampleRead::Sampled(manyproxy_ui::ProcessSample {
            cpu_100ns,
            ram_bytes,
        })
    }
}

// T-33: the bind picker and the Settings address list.

pub fn t83_bind_hosts_of(view: &Entity<AppView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| view.update(cx, |view, _| view.store.bind_hosts()))
}

// ---------------------------------------------------------------------------
// Slice 5 - one endpoint draft, two hosts, one apply path.

pub fn endpoint_open(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
    id: &str,
    host: manyproxy_ui::app::endpoint_editor::EndpointHost,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_endpoint_draft(id, host, window, cx);
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
}

/// The draft's two raw field values. Reading the inputs, not the stored row,
/// is what proves Apply acts on what the user typed.
pub fn endpoint_values(view: &Entity<AppView>, cx: &mut VisualTestContext) -> (String, String) {
    view.read_with(cx, |view, cx| {
        let draft = view
            .endpoint_draft
            .as_ref()
            .expect("an endpoint draft is open");
        (
            draft.bind.read(cx).value().to_string(),
            draft.port.read(cx).value().to_string(),
        )
    })
}

pub fn endpoint_set(view: &Entity<AppView>, cx: &mut VisualTestContext, field: &str, text: &str) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let draft = view
                .endpoint_draft
                .as_ref()
                .expect("an endpoint draft is open");
            let input = if field == "bind" {
                draft.bind.clone()
            } else {
                draft.port.clone()
            };
            input.update(cx, |state, cx| state.set_value(text, window, cx));
        });
    });
    cx.run_until_parked();
    paint_frame(cx);
}

pub fn endpoint_can_apply(view: &Entity<AppView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| {
        view.endpoint_check(cx)
            .is_some_and(|check| check.can_apply())
    })
}

pub fn endpoint_error(view: &Entity<AppView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| {
        view.endpoint_check(cx).and_then(|check| check.error)
    })
}

pub fn endpoint_failed_reason(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
) -> Option<String> {
    view.read_with(cx, |view, _| {
        match view.endpoint_draft.as_ref().map(|draft| &draft.phase) {
            Some(manyproxy_ui::app::endpoint_editor::EndpointPhase::Failed(reason)) => {
                Some(reason.clone())
            }
            _ => None,
        }
    })
}

pub fn endpoint_apply_clicked(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click("endpoint-apply", cx));
    cx.run_until_parked();
    paint_frame(cx);
}

pub fn endpoint_saved(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
    id: &str,
) -> (String, u16) {
    view.read_with(cx, |view, _| {
        let row = view
            .store
            .get(id)
            .expect("store reads")
            .expect("row survives");
        (row.bind_ip, row.port)
    })
}

pub fn endpoint_fake_engine(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
) -> manyproxy_ui::engine::FakeHandle {
    use manyproxy_ui::engine::EngineController;
    let (engine, handle) = EngineController::fake();
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.engine = engine);
    });
    handle
}

pub fn endpoint_served_ports(handle: &manyproxy_ui::engine::FakeHandle) -> Vec<u64> {
    let config = handle
        .last_config()
        .expect("the fake engine served a config");
    let value: serde_json::Value =
        serde_json::from_str(&config).expect("the served config is JSON");
    value["inbounds"]
        .as_array()
        .expect("inbounds is an array")
        .iter()
        .filter_map(|inbound| inbound["listen_port"].as_u64())
        .collect()
}

pub fn endpoint_press(view: &Entity<AppView>, cx: &mut VisualTestContext, key: &str) {
    let focus = view.read_with(cx, |view, cx| {
        view.endpoint_frame(cx).map(|frame| frame.focus)
    });
    cx.update(|window, cx| {
        if let Some(focus) = focus.as_ref() {
            window.focus(focus, cx);
        }
        let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

// ---------------------------------------------------------------------------
// K5 — toasts + non-blocking import/delete
// ---------------------------------------------------------------------------

/// A click on a library row with modifiers held. The kit's own test helpers
/// click with an empty modifier set, so multi-select needs its own dispatch.
pub fn click_row_with(cx: &mut VisualTestContext, id: &str, modifiers: gpui_kit::Modifiers) {
    use gpui_kit::gpui::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};
    cx.update(|window, cx| {
        window.render_frame(cx);
        let position = window
            .try_find(id.to_string())
            .unwrap_or_else(|| panic!("{id} renders"))
            .bounds()
            .center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

/// T-03: press on a column edge, move by `dx`, release. The move listener is
/// registered by the header strip's paint, so the frame must exist first.
pub fn drag_column_edge(cx: &mut VisualTestContext, id: &str, dx: f32) {
    use gpui_kit::gpui::{
        InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    };
    cx.update(|window, cx| {
        window.render_frame(cx);
        let start = window
            .try_find(id.to_string())
            .unwrap_or_else(|| panic!("{id} renders"))
            .bounds()
            .center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseMoveEvent {
                position: point(start.x + px(dx), start.y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: point(start.x + px(dx), start.y),
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

pub fn double_click_column_edge(cx: &mut VisualTestContext, id: &str) {
    use gpui_kit::gpui::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};
    cx.update(|window, cx| {
        window.render_frame(cx);
        let position = window
            .try_find(id.to_string())
            .unwrap_or_else(|| panic!("{id} renders"))
            .bounds()
            .center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 2,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 2,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

/// T-04: press a header and cross gpui's drag threshold. A header drag starts
/// only after the pointer moves; the second move in the tests below is the one
/// that carries the indicator onto a target.
pub fn begin_column_drag(cx: &mut VisualTestContext, source_id: &str) {
    use gpui_kit::gpui::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent};
    let start = cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(source_id.to_string())
            .unwrap_or_else(|| panic!("{source_id} renders"))
            .bounds()
            .center()
    });
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseMoveEvent {
                position: point(start.x + px(8.), start.y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

/// T-04: the point inside a header that selects the before/after landing slot.
pub fn column_drop_point(
    cx: &mut VisualTestContext,
    target_id: &str,
    after: bool,
) -> gpui_kit::gpui::Point<gpui_kit::gpui::Pixels> {
    cx.update(|window, _| {
        let bounds = window
            .try_find(target_id.to_string())
            .unwrap_or_else(|| panic!("{target_id} renders"))
            .bounds();
        let x = if after {
            bounds.right() - px(4.)
        } else {
            bounds.left() + px(4.)
        };
        point(x, bounds.center().y)
    })
}

pub fn move_column_drag(cx: &mut VisualTestContext, target_id: &str, after: bool) {
    use gpui_kit::gpui::{InputEvent as _, MouseButton, MouseMoveEvent};
    let position = column_drop_point(cx, target_id, after);
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

pub fn release_column_drag(cx: &mut VisualTestContext, target_id: &str, after: bool) {
    use gpui_kit::gpui::{InputEvent as _, MouseButton, MouseUpEvent};
    let position = column_drop_point(cx, target_id, after);
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
    cx.run_until_parked();
    paint_frame(cx);
}

/// T-04: the common one-target gesture: start, hover, assert the indicator,
/// release.
pub fn drag_column_header(
    cx: &mut VisualTestContext,
    source_id: &str,
    target_id: &str,
    after: bool,
    indicator: Option<&str>,
) {
    begin_column_drag(cx, source_id);
    move_column_drag(cx, target_id, after);
    if let Some(indicator) = indicator {
        assert_painted(cx, indicator);
    }
    release_column_drag(cx, target_id, after);
}

pub fn selected_ids(view: &Entity<AppView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| view.selection_ids())
}

pub fn row_aria_selected(cx: &mut VisualTestContext, id: &str) -> bool {
    cx.update(|window, _| {
        window
            .try_find(format!("library-row-{id}"))
            .unwrap_or_else(|| panic!("row {id} renders"))
            .selected()
            .unwrap_or(false)
    })
}

pub fn press_key(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    cx.update(|window, cx| window.dispatch_keystroke(keystroke, cx));
    cx.run_until_parked();
    paint_frame(cx);
}

pub fn focus_library(view: &Entity<AppView>, cx: &mut VisualTestContext) {
    use gpui_kit::Focusable as _;
    let handle = view.read_with(cx, |view, cx| view.libraries.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
}

// ---------------------------------------------------------------------------
// Slice 1 — the workbench shell
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Slice 2 — one selection model, flat rows and the batch bar
// ---------------------------------------------------------------------------

pub fn clipboard_text(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
            .to_string()
    })
}

/// A click near the bottom of the table, below the rows.
pub fn click_blank_table(cx: &mut VisualTestContext) {
    use gpui_kit::gpui::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};
    let position = cx.update(|window, _| {
        let bounds = window
            .try_find("library-scroll")
            .expect("the table paints")
            .bounds();
        point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height - px(8.),
        )
    });
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    });
}

// ---------------------------------------------------------------------------
// Slice 6 — the stacked inspector and the console dock
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Slice 7: the nav scopes.
// ---------------------------------------------------------------------------

pub fn nav_y(id: &str, cx: &mut VisualTestContext) -> gpui_kit::Pixels {
    cx.update(|window, _| {
        window
            .try_find(id.to_string())
            .unwrap_or_else(|| panic!("{id} renders"))
            .bounds()
            .origin
            .y
    })
}
