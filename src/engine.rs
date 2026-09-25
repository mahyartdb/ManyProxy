//! Ticket 05: standard multi-engine (one combined sidecar config).
//!
//! One generated engine config serves N active library rows: one `mixed`
//! (SOCKS5+HTTP) inbound per active config on its loopback-default bind, N
//! outbounds/`endpoints` built from the stored protocol params, and
//! `route.rules[]` mapping each inbound to exactly its own outbound
//! (`{"action":"route","outbound":...}` form) with a `direct` catch-all
//! (`route.final`). WireGuard clients become `endpoints[]` (sing-box ≥1.11).
//!
//! The engine runs as a supervised sidecar child process. Unit and headless
//! UI tests never touch a real binary or remote server: [`EngineBackend`] is
//! the trait seam, [`FakeEngineBackend`] is the mock driver.
//!
//! Hard prohibitions (spec + contracts): never set the system proxy, never
//! TUN, never DNS rewrite, no admin for loopback use. The generator emits
//! none of those keys and [`generate`] rejects them structurally.

#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::rc::Rc;

use crate::store::ProxyConfig;

/// Base name of the sidecar binary (resolved in the app dir, then `PATH`).
pub const ENGINE_BINARY_BASE: &str = "sing-box";

/// Canonical sing-box outbound/endpoint type for the ticket-05 standard set.
/// Anything else (anytls, naive, socks, http, unknown) is an
/// [`EngineError::UnsupportedProtocol`] owned by a later ticket.
fn canonical_type(protocol: &str) -> Option<&'static str> {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "shadowsocks" | "ss" => Some("shadowsocks"),
        "trojan" => Some("trojan"),
        "vmess" => Some("vmess"),
        "vless" => Some("vless"),
        "hysteria" => Some("hysteria"),
        "hysteria2" | "hy2" => Some("hysteria2"),
        "tuic" => Some("tuic"),
        "wireguard" | "wg" => Some("wireguard"),
        _ => None,
    }
}

/// Inbound tag for a library row (unique: ids are never reused).
pub fn inbound_tag(id: &str) -> String {
    format!("in-{}", sanitize_tag(id))
}

/// Outbound/endpoint tag for a library row.
pub fn outbound_tag(id: &str) -> String {
    format!("out-{}", sanitize_tag(id))
}

fn sanitize_tag(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Failures of the activation seam. Messages name configs, binds, ports, and
/// protocols — never secrets (no params values, keys, or passwords).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// Two active configs want the same local endpoint.
    PortConflict {
        bind: String,
        port: u16,
        first: String,
        second: String,
    },
    /// Protocol belongs to a later (exotic/custom-core) ticket.
    UnsupportedProtocol { name: String, protocol: String },
    /// Stored params are not a JSON object (never echoes the params).
    InvalidParamsJson { name: String },
    /// Stored params miss a required sing-box field (names the field only).
    BadParams { name: String, field: &'static str },
    /// Nothing to run.
    NoActiveConfigs,
    /// Generated JSON fails the structural self-check.
    InvalidGenerated { detail: String },
    /// The engine's own `check` rejected the generated config.
    CheckFailed { detail: String },
    /// No `sing-box` binary in the app dir or `PATH` (never downloads).
    MissingBinary { searched: String },
    /// The sidecar process could not be spawned.
    SpawnFailed { detail: String },
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::PortConflict {
                bind,
                port,
                first,
                second,
            } => write!(
                f,
                "port in use: {bind}:{port} already serves \"{first}\"; cannot start \"{second}\""
            ),
            EngineError::UnsupportedProtocol { name, protocol } => write!(
                f,
                "unsupported protocol for \"{name}\": {protocol} (standard set only)"
            ),
            EngineError::InvalidParamsJson { name } => {
                write!(f, "bad stored params for \"{name}\": params are not a JSON object")
            }
            EngineError::BadParams { name, field } => {
                write!(f, "bad stored params for \"{name}\": missing \"{field}\"")
            }
            EngineError::NoActiveConfigs => write!(f, "no active configurations"),
            EngineError::InvalidGenerated { detail } => {
                write!(f, "invalid generated engine config ({detail})")
            }
            EngineError::CheckFailed { detail } => {
                write!(f, "engine config check failed ({detail})")
            }
            EngineError::MissingBinary { searched } => write!(
                f,
                "engine binary not found (looked for {searched}); install sing-box — ManyProxy never downloads executables silently"
            ),
            EngineError::SpawnFailed { detail } => {
                write!(f, "engine failed to start ({detail})")
            }
        }
    }
}

/// One row of the per-endpoint isolation mapping: traffic arriving on
/// `inbound_tag` exits only via `outbound_tag`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationEntry {
    pub config_id: String,
    pub inbound_tag: String,
    pub outbound_tag: String,
}

/// A generated combined engine config plus its isolation mapping.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct GeneratedConfig {
    /// Compact JSON fed to `check` and the sidecar's `-c` file.
    pub json: String,
    pub inbound_tags: Vec<String>,
    pub outbound_tags: Vec<String>,
    pub isolation: Vec<IsolationEntry>,
}

impl GeneratedConfig {
    /// Parsed view of [`GeneratedConfig::json`] (tests assert structure).
    #[cfg(test)]
    pub fn value(&self) -> serde_json::Value {
        serde_json::from_str(&self.json).expect("generated config is valid JSON")
    }
}

/// Build one combined engine config from the active library rows.
///
/// Order: conflict check → per-row outbound/endpoint mapping → route table →
/// structural self-check. Pure: no processes, no network, no secrets in
/// errors.
pub fn generate(configs: &[ProxyConfig]) -> Result<GeneratedConfig, EngineError> {
    if configs.is_empty() {
        return Err(EngineError::NoActiveConfigs);
    }
    let mut taken: HashMap<(String, u16), &str> = HashMap::new();
    for config in configs {
        let key = (config.bind_ip.to_ascii_lowercase(), config.port);
        if let Some(first) = taken.get(&key) {
            return Err(EngineError::PortConflict {
                bind: config.bind_ip.clone(),
                port: config.port,
                first: (*first).to_string(),
                second: config.name.clone(),
            });
        }
        taken.insert(key, config.name.as_str());
    }

    let mut inbounds = Vec::with_capacity(configs.len());
    let mut outbounds: Vec<serde_json::Value> = Vec::with_capacity(configs.len() + 1);
    let mut endpoints = Vec::new();
    let mut rules = Vec::with_capacity(configs.len());
    let mut isolation = Vec::with_capacity(configs.len());
    let mut outbound_tags = Vec::with_capacity(configs.len());

    for config in configs {
        let in_tag = inbound_tag(&config.id);
        let out_tag = outbound_tag(&config.id);
        let canonical =
            canonical_type(&config.protocol).ok_or_else(|| EngineError::UnsupportedProtocol {
                name: config.name.clone(),
                protocol: config.protocol.clone(),
            })?;
        let mut params = serde_json::from_str::<serde_json::Value>(&config.params)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or_else(|| EngineError::InvalidParamsJson {
                name: config.name.clone(),
            })?;
        let server = params
            .get("server")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|server| !server.is_empty())
            .ok_or(EngineError::BadParams {
                name: config.name.clone(),
                field: "server",
            })?;
        let _ = server;
        let server_port = match params.get("server_port") {
            Some(serde_json::Value::Number(number)) => number
                .as_u64()
                .filter(|port| (1..=65_535).contains(port))
                .map(|port| port as u16),
            Some(serde_json::Value::String(text)) => {
                text.trim().parse::<u16>().ok().filter(|port| *port > 0)
            }
            _ => None,
        }
        .ok_or(EngineError::BadParams {
            name: config.name.clone(),
            field: "server_port",
        })?;
        params.insert(
            "server_port".to_string(),
            serde_json::Value::Number(server_port.into()),
        );
        params.insert(
            "type".to_string(),
            serde_json::Value::String(canonical.to_string()),
        );
        params.insert(
            "tag".to_string(),
            serde_json::Value::String(out_tag.clone()),
        );
        let node = serde_json::Value::Object(params);
        if canonical == "wireguard" {
            endpoints.push(node);
        } else {
            outbounds.push(node);
        }
        inbounds.push(serde_json::json!({
            "type": "mixed",
            "tag": in_tag,
            "listen": config.bind_ip,
            "listen_port": config.port,
        }));
        rules.push(serde_json::json!({
            "inbound": in_tag,
            "action": "route",
            "outbound": out_tag,
        }));
        outbound_tags.push(out_tag.clone());
        isolation.push(IsolationEntry {
            config_id: config.id.clone(),
            inbound_tag: in_tag,
            outbound_tag: out_tag,
        });
    }
    outbounds.push(serde_json::json!({"type": "direct", "tag": "direct"}));

    let document = serde_json::json!({
        "log": {"level": "warn"},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "endpoints": endpoints,
        "route": {"rules": rules, "final": "direct"},
    });
    validate_structure(&document)?;
    Ok(GeneratedConfig {
        json: serde_json::to_string(&document).expect("engine config serializes"),
        inbound_tags: isolation
            .iter()
            .map(|entry| entry.inbound_tag.clone())
            .collect(),
        outbound_tags,
        isolation,
    })
}

/// Structural self-check run before the engine's own `check`: unique tags and
/// binds, every rule resolves to exactly one outbound/endpoint, `final`
/// resolves to `direct`, and none of the prohibited keys (`tun`, `dns`,
/// `set_system_proxy`) appear anywhere in the generated shape.
fn validate_structure(document: &serde_json::Value) -> Result<(), EngineError> {
    let invalid = |detail: &str| EngineError::InvalidGenerated {
        detail: detail.to_string(),
    };
    if let Some(top) = document.as_object() {
        for forbidden in ["tun", "dns"] {
            if top.contains_key(forbidden) {
                return Err(invalid(&format!("forbidden section \"{forbidden}\"")));
            }
        }
    }
    let inbounds = document
        .get("inbounds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"inbounds\""))?;
    let outbounds = document
        .get("outbounds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"outbounds\""))?;
    let endpoints = document
        .get("endpoints")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"endpoints\""))?;
    let mut tags = HashSet::new();
    let mut binds = HashSet::new();
    for inbound in inbounds {
        let obj = inbound.as_object().ok_or_else(|| invalid("bad inbound"))?;
        if obj.get("type").and_then(serde_json::Value::as_str) != Some("mixed") {
            return Err(invalid("inbound is not \"mixed\""));
        }
        let tag = obj
            .get("tag")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("inbound without tag"))?;
        if !tags.insert(("in", tag.to_string())) {
            return Err(invalid(&format!("duplicate inbound tag \"{tag}\"")));
        }
        let listen = obj
            .get("listen")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("inbound without listen"))?;
        let port = obj
            .get("listen_port")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| invalid("inbound without listen_port"))?;
        if !binds.insert((listen.to_ascii_lowercase(), port)) {
            return Err(invalid("duplicate inbound bind"));
        }
        if obj.contains_key("set_system_proxy") {
            return Err(invalid("forbidden key \"set_system_proxy\""));
        }
    }
    let mut exits = HashSet::new();
    for node in outbounds.iter().chain(endpoints.iter()) {
        let obj = node.as_object().ok_or_else(|| invalid("bad outbound"))?;
        let tag = obj
            .get("tag")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("outbound without tag"))?;
        if !tags.insert(("out", tag.to_string())) {
            return Err(invalid(&format!("duplicate outbound tag \"{tag}\"")));
        }
        exits.insert(tag.to_string());
        if obj.contains_key("set_system_proxy") {
            return Err(invalid("forbidden key \"set_system_proxy\""));
        }
    }
    let route = document
        .get("route")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid("missing \"route\""))?;
    let inbound_names: HashSet<&str> = inbounds
        .iter()
        .filter_map(|node| node.get("tag").and_then(serde_json::Value::as_str))
        .collect();
    let rules = route
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid("missing \"route.rules\""))?;
    for rule in rules {
        let obj = rule.as_object().ok_or_else(|| invalid("bad route rule"))?;
        let inbound = obj
            .get("inbound")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("rule without inbound"))?;
        let outbound = obj
            .get("outbound")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("rule without outbound"))?;
        if obj.get("action").and_then(serde_json::Value::as_str) != Some("route") {
            return Err(invalid("rule without route action"));
        }
        if !inbound_names.contains(inbound) {
            return Err(invalid(&format!("rule references unknown \"{inbound}\"")));
        }
        if !exits.contains(outbound) {
            return Err(invalid(&format!("rule references unknown \"{outbound}\"")));
        }
    }
    let final_outbound = route
        .get("final")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("missing \"route.final\""))?;
    if final_outbound != "direct" || !exits.contains(final_outbound) {
        return Err(invalid("route.final must be the direct outbound"));
    }
    Ok(())
}

/// Sidecar driver seam: the real backend shells out to `sing-box`, the fake
/// backend records configs in memory. Errors are plain strings so the fake
/// and the real backend share one failure shape.
pub trait EngineBackend {
    /// Run the engine's own `check` on the generated JSON (no processes kept).
    fn check_config(&mut self, config_json: &str) -> Result<(), String>;
    /// (Re)start the sidecar with the generated JSON.
    fn start(&mut self, config_json: &str) -> Result<(), String>;
    /// Stop the sidecar if running.
    fn stop(&mut self);
    /// True while a sidecar process (real or fake) is up.
    #[cfg_attr(not(test), allow(dead_code))]
    fn is_running(&self) -> bool;
    /// `Some(cause)` once when a previously running sidecar has exited.
    fn poll_exit(&mut self) -> Option<String>;
}

/// Binary file name for this platform.
pub fn engine_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{ENGINE_BINARY_BASE}.exe")
    } else {
        ENGINE_BINARY_BASE.to_string()
    }
}

/// Resolve the sidecar binary: app directory first, then `PATH`. Never
/// downloads anything; the [`EngineError::MissingBinary`] names every
/// directory searched.
pub fn resolve_engine_binary() -> Result<PathBuf, EngineError> {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let path_env = std::env::var("PATH").unwrap_or_default();
    resolve_engine_binary_in(app_dir.as_deref(), &path_env)
}

/// Testable core of [`resolve_engine_binary`] (explicit search roots).
pub fn resolve_engine_binary_in(
    app_dir: Option<&Path>,
    path_env: &str,
) -> Result<PathBuf, EngineError> {
    let file = engine_binary_file_name();
    let mut searched = Vec::new();
    if let Some(dir) = app_dir {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(dir.to_path_buf());
    }
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
        searched.push(dir);
    }
    let searched = searched
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    Err(EngineError::MissingBinary { searched })
}

fn truncate_detail(detail: &str) -> String {
    const LIMIT: usize = 1000;
    let mut text: String = detail.chars().take(LIMIT).collect();
    if detail.chars().count() > LIMIT {
        text.push('…');
    }
    text
}

/// Production driver: `sing-box check` validates, `sing-box run` serves. The
/// generated JSON lives in a temp `-c` file that is removed on stop so no
/// secrets linger on disk.
pub struct RealEngineBackend {
    child: Option<std::process::Child>,
    config_path: PathBuf,
}

impl RealEngineBackend {
    pub fn new() -> Self {
        Self {
            child: None,
            config_path: std::env::temp_dir().join("manyproxy-singbox.json"),
        }
    }

    fn write_config(&self, config_json: &str) -> Result<PathBuf, String> {
        std::fs::write(&self.config_path, config_json)
            .map_err(|error| format!("cannot write engine config ({error})"))?;
        Ok(self.config_path.clone())
    }
}

impl Default for RealEngineBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineBackend for RealEngineBackend {
    fn check_config(&mut self, config_json: &str) -> Result<(), String> {
        let binary = resolve_engine_binary().map_err(|error| error.to_string())?;
        let path = self.write_config(config_json)?;
        let output = std::process::Command::new(&binary)
            .arg("check")
            .arg("-c")
            .arg(&path)
            .output()
            .map_err(|error| format!("engine check could not run ({error})"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(truncate_detail(&String::from_utf8_lossy(&output.stderr)))
        }
    }

    fn start(&mut self, config_json: &str) -> Result<(), String> {
        let binary = resolve_engine_binary().map_err(|error| error.to_string())?;
        let path = self.write_config(config_json)?;
        self.stop();
        let child = std::process::Command::new(&binary)
            .arg("run")
            .arg("-c")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("engine failed to start ({error})"))?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.config_path);
    }

    fn is_running(&self) -> bool {
        self.child.is_some()
    }

    fn poll_exit(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.try_wait().ok()??;
        self.child = None;
        Some(format!("engine exited ({status})"))
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeState {
    running: bool,
    last_config: Option<String>,
    check_calls: usize,
    fail_check: Option<String>,
    exited: Option<String>,
}

/// Read/write handle to a [`FakeEngineBackend`]'s recorded state (tests
/// assert generated configs and inject check failures / crashes).
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeHandle {
    state: Rc<RefCell<FakeState>>,
}

#[cfg(test)]
impl FakeHandle {
    /// Last config JSON handed to `start`, if any.
    pub fn last_config(&self) -> Option<String> {
        self.state.borrow().last_config.clone()
    }

    /// Whether the fake sidecar is up.
    pub fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    /// How many times `check_config` ran.
    pub fn check_calls(&self) -> usize {
        self.state.borrow().check_calls
    }

    /// Make the next (and subsequent, until cleared) `check_config` fail.
    pub fn fail_check(&self, detail: &str) {
        self.state.borrow_mut().fail_check = Some(detail.to_string());
    }

    /// Simulate a sidecar crash; the next [`EngineController::poll`] reports it.
    pub fn inject_crash(&self, cause: &str) {
        let mut state = self.state.borrow_mut();
        state.running = false;
        state.exited = Some(cause.to_string());
    }
}

/// In-memory mock driver: records configs, never spawns processes, never
/// touches the network. All unit and headless UI tests run against this.
#[cfg(test)]
pub struct FakeEngineBackend {
    state: Rc<RefCell<FakeState>>,
}

#[cfg(test)]
impl FakeEngineBackend {
    pub fn shared() -> (Self, FakeHandle) {
        let state = Rc::new(RefCell::new(FakeState::default()));
        (
            Self {
                state: state.clone(),
            },
            FakeHandle { state },
        )
    }
}

#[cfg(test)]
impl Default for FakeEngineBackend {
    fn default() -> Self {
        Self::shared().0
    }
}

#[cfg(test)]
impl EngineBackend for FakeEngineBackend {
    fn check_config(&mut self, _config_json: &str) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.check_calls += 1;
        state.fail_check.clone().map_or(Ok(()), Err)
    }

    fn start(&mut self, config_json: &str) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.running = true;
        state.exited = None;
        state.last_config = Some(config_json.to_string());
        Ok(())
    }

    fn stop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.running = false;
    }

    fn is_running(&self) -> bool {
        self.state.borrow().running
    }

    fn poll_exit(&mut self) -> Option<String> {
        self.state.borrow_mut().exited.take()
    }
}

/// Sidecar lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Stopped,
    Running,
    Crashed { cause: String },
}

/// Supervised sidecar owner: one combined config for the active set,
/// regenerated and re-checked on every membership change. A failed apply
/// never clobbers a previously running sidecar: `check` runs before the old
/// process is stopped.
pub struct EngineController {
    backend: Box<dyn EngineBackend>,
    active: Vec<ProxyConfig>,
    status: EngineStatus,
    last_error: Option<String>,
    last_config_json: Option<String>,
}

impl EngineController {
    pub fn new(backend: Box<dyn EngineBackend>) -> Self {
        Self {
            backend,
            active: Vec::new(),
            status: EngineStatus::Stopped,
            last_error: None,
            last_config_json: None,
        }
    }

    /// Production controller (real `sing-box` sidecar, lazily resolved).
    pub fn production() -> Self {
        Self::new(Box::new(RealEngineBackend::new()))
    }

    /// Test controller plus a handle to the mock driver.
    #[cfg(test)]
    pub fn fake() -> (Self, FakeHandle) {
        let (backend, handle) = FakeEngineBackend::shared();
        (Self::new(Box::new(backend)), handle)
    }

    /// Start one config (idempotent); restarts the sidecar with the union.
    pub fn start_one(&mut self, config: &ProxyConfig) -> Result<(), EngineError> {
        let mut candidate = self.active.clone();
        if let Some(slot) = candidate.iter_mut().find(|row| row.id == config.id) {
            *slot = config.clone();
        } else {
            candidate.push(config.clone());
        }
        // Pure pre-check: a bad candidate never disturbs the running sidecar.
        generate(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.active = candidate;
        self.apply()
    }

    /// Stop one config; returns true when it was active. Stops the sidecar
    /// when the set becomes empty.
    pub fn stop_one(&mut self, id: &str) -> bool {
        let before = self.active.len();
        self.active.retain(|row| row.id != id);
        if self.active.len() == before {
            return false;
        }
        let _ = self.apply();
        true
    }

    /// Replace the active set (start-all path); validates before touching the
    /// running sidecar.
    pub fn start_all(&mut self, configs: &[ProxyConfig]) -> Result<(), EngineError> {
        let candidate = configs.to_vec();
        if candidate.is_empty() {
            self.active.clear();
            return self.apply();
        }
        generate(&candidate).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        self.active = candidate;
        self.apply()
    }

    /// Stop everything and take the sidecar down.
    pub fn stop_all(&mut self) {
        self.active.clear();
        let _ = self.apply();
    }

    fn apply(&mut self) -> Result<(), EngineError> {
        if self.active.is_empty() {
            self.backend.stop();
            self.status = EngineStatus::Stopped;
            self.last_config_json = None;
            return Ok(());
        }
        let generated = generate(&self.active).inspect_err(|error| {
            self.last_error = Some(error.to_string());
        })?;
        if let Err(detail) = self.backend.check_config(&generated.json) {
            let error = EngineError::CheckFailed { detail };
            self.last_error = Some(error.to_string());
            return Err(error);
        }
        self.backend.stop();
        if let Err(detail) = self.backend.start(&generated.json) {
            let error = EngineError::SpawnFailed { detail };
            self.last_error = Some(error.to_string());
            self.status = EngineStatus::Stopped;
            return Err(error);
        }
        self.status = EngineStatus::Running;
        self.last_error = None;
        self.last_config_json = Some(generated.json);
        Ok(())
    }

    /// Crash detection: call on UI actions; reports the named cause once.
    pub fn poll(&mut self) -> Option<String> {
        let cause = self.backend.poll_exit()?;
        if self.active.is_empty() {
            return None;
        }
        self.status = EngineStatus::Crashed {
            cause: cause.clone(),
        };
        self.last_error = Some(format!("engine crashed ({cause})"));
        Some(cause)
    }

    /// Ids of the active set.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn running_ids(&self) -> Vec<String> {
        self.active.iter().map(|row| row.id.clone()).collect()
    }

    /// Number of active endpoints.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn running_count(&self) -> usize {
        self.active.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_running(&self, id: &str) -> bool {
        self.active.iter().any(|row| row.id == id) && self.backend.is_running()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn status(&self) -> &EngineStatus {
        &self.status
    }

    /// One-line status for the status bar.
    pub fn status_text(&self) -> String {
        match &self.status {
            EngineStatus::Stopped => "engine stopped".to_string(),
            EngineStatus::Running => {
                format!("engine running · {} endpoint(s)", self.active.len())
            }
            EngineStatus::Crashed { cause } => format!("engine crashed ({cause})"),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Current per-endpoint isolation mapping (each endpoint exits only via
    /// its own remote).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn isolation_map(&self) -> Vec<IsolationEntry> {
        self.active
            .iter()
            .map(|row| IsolationEntry {
                config_id: row.id.clone(),
                inbound_tag: inbound_tag(&row.id),
                outbound_tag: outbound_tag(&row.id),
            })
            .collect()
    }

    /// Active configs bound off loopback (the UI warns before serving these).
    pub fn lan_warnings(&self) -> Vec<String> {
        self.active
            .iter()
            .filter(|row| !is_loopback_bind(&row.bind_ip))
            .map(|row| format!("\"{}\" serves on {} (LAN-visible)", row.name, row.bind_ip))
            .collect()
    }
}

/// True for loopback literals (spec: default bind; anything else is an
/// explicit LAN exposure). Unparseable input warns rather than serves
/// silently.
pub fn is_loopback_bind(bind: &str) -> bool {
    bind.trim()
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Warning text for a non-loopback bind, if any.
pub fn lan_exposure_warning(name: &str, bind: &str) -> Option<String> {
    if is_loopback_bind(bind) {
        None
    } else {
        Some(format!(
            "\"{name}\" binds {bind}: LAN-visible; anyone on the network can use this proxy"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(
        id: &str,
        name: &str,
        protocol: &str,
        params: serde_json::Value,
        bind: &str,
        port: u16,
    ) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string(),
            group: "default".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
        }
    }

    fn ss_params(password: &str) -> serde_json::Value {
        json!({"type": "shadowsocks", "server": "203.0.113.7", "server_port": 8388,
               "method": "aes-128-gcm", "password": password})
    }

    fn vless_params() -> serde_json::Value {
        json!({"type": "vless", "server": "198.51.100.9", "server_port": 443,
               "uuid": "11111111-2222-4333-8444-555555555555", "encryption": "none",
               "security": "tls", "sni": "198.51.100.9", "network": "tcp"})
    }

    fn wg_params() -> serde_json::Value {
        json!({"type": "wireguard", "server": "192.0.2.44", "server_port": 51820,
               "private_key": "LOCAL-PRIVATE", "public_key": "REMOTE-PUBLIC"})
    }

    fn three_standard() -> Vec<ProxyConfig> {
        vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10801,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10802),
            fixture("3", "Gamma", "wireguard", wg_params(), "127.0.0.1", 10803),
        ]
    }

    /// Slice scenario 1 (structure): 3 stopped standard configs generate 3
    /// mixed loopback inbounds with unique tags and ports.
    #[test]
    fn three_configs_yield_three_mixed_inbounds() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let inbounds = value["inbounds"].as_array().expect("inbounds array");
        assert_eq!(inbounds.len(), 3);
        let mut tags = HashSet::new();
        let mut ports = HashSet::new();
        for inbound in inbounds {
            assert_eq!(inbound["type"], json!("mixed"));
            assert_eq!(inbound["listen"], json!("127.0.0.1"));
            assert!(tags.insert(inbound["tag"].as_str().unwrap().to_string()));
            assert!(ports.insert(inbound["listen_port"].as_u64().unwrap()));
        }
        assert_eq!(generated.inbound_tags.len(), 3);
    }

    /// Slice scenario 1 (isolation): every rule routes its own inbound to
    /// exactly its own outbound; unmatched traffic falls to direct.
    #[test]
    fn route_rules_isolate_each_endpoint_with_direct_final() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let rules = value["route"]["rules"].as_array().expect("rules array");
        assert_eq!(rules.len(), 3);
        for (entry, rule) in generated.isolation.iter().zip(rules.iter()) {
            assert_eq!(rule["inbound"], json!(entry.inbound_tag));
            assert_eq!(rule["outbound"], json!(entry.outbound_tag));
            assert_eq!(rule["action"], json!("route"));
        }
        assert_eq!(value["route"]["final"], json!("direct"));
        let outbound_tags: HashSet<String> = value["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .filter_map(|node| node["tag"].as_str().map(str::to_string))
            .collect();
        assert!(outbound_tags.contains("direct"));
        for entry in &generated.isolation {
            if value["endpoints"]
                .as_array()
                .expect("endpoints array")
                .iter()
                .any(|node| node["tag"] == json!(entry.outbound_tag))
            {
                continue;
            }
            assert!(
                outbound_tags.contains(&entry.outbound_tag),
                "outbound tag resolves: {}",
                entry.outbound_tag
            );
        }
    }

    /// Research mapping: WireGuard clients live in `endpoints[]`, never in
    /// `outbounds[]`.
    #[test]
    fn wireguard_becomes_endpoint_not_outbound() {
        let generated = generate(&three_standard()).expect("standard set generates");
        let value = generated.value();
        let endpoints = value["endpoints"].as_array().expect("endpoints array");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], json!("wireguard"));
        assert_eq!(endpoints[0]["tag"], json!(outbound_tag("3")));
        let outbound_tags: Vec<String> = value["outbounds"]
            .as_array()
            .expect("outbounds array")
            .iter()
            .filter_map(|node| node["tag"].as_str().map(str::to_string))
            .collect();
        assert!(!outbound_tags.contains(&outbound_tag("3")));
        assert_eq!(outbound_tags.len(), 3); // ss + vless + direct
        assert_eq!(generated.outbound_tags.len(), 3); // one tag per row (wg tag lives in endpoints)
        assert!(generated.outbound_tags.contains(&outbound_tag("1")));
    }

    /// Exotic protocols stay out of this slice with a named error.
    #[test]
    fn unsupported_protocol_names_config_without_secrets() {
        let configs = vec![fixture(
            "9",
            "Odd",
            "anytls",
            json!({"server": "10.0.0.9", "server_port": 443, "password": "TOP-SECRET"}),
            "127.0.0.1",
            10809,
        )];
        let error = generate(&configs).expect_err("anytls is not standard");
        let text = error.to_string();
        assert!(text.contains("\"Odd\""), "names config: {text}");
        assert!(text.contains("anytls"), "names protocol: {text}");
        assert!(!text.contains("TOP-SECRET"), "no secrets: {text}");
    }

    /// Slice scenario 3: a taken port blocks start, naming both configs.
    #[test]
    fn duplicate_listen_port_names_both_configs() {
        let configs = vec![
            fixture(
                "1",
                "Alpha",
                "shadowsocks",
                ss_params("pass1"),
                "127.0.0.1",
                10821,
            ),
            fixture("2", "Beta", "vless", vless_params(), "127.0.0.1", 10821),
        ];
        let error = generate(&configs).expect_err("collision must fail");
        let text = error.to_string();
        assert!(text.contains("\"Alpha\""), "names holder: {text}");
        assert!(text.contains("\"Beta\""), "names newcomer: {text}");
        assert!(text.contains("10821"), "names port: {text}");
    }

    /// Prohibitions: no TUN, no DNS rewrite, no system-proxy keys.
    #[test]
    fn generated_json_has_no_tun_dns_or_system_proxy() {
        let generated = generate(&three_standard()).expect("standard set generates");
        for key in ["\"tun\"", "\"dns\"", "\"set_system_proxy\""] {
            assert!(
                !generated.json.contains(key),
                "forbidden key {key} in generated config"
            );
        }
        let top = generated.value();
        assert!(top.get("tun").is_none());
        assert!(top.get("dns").is_none());
    }

    /// Controller on the mock: start-all 3, stop 1 (its endpoint drops, the
    /// others keep their isolation mapping).
    #[test]
    fn controller_start_all_stop_one_restart_via_fake() {
        let (mut controller, handle) = EngineController::fake();
        let rows = three_standard();
        controller.start_all(&rows).expect("start-all works");
        assert_eq!(controller.running_count(), 3);
        assert_eq!(controller.status(), &EngineStatus::Running);
        assert!(handle.is_running());
        assert_eq!(handle.check_calls(), 1);

        assert!(controller.stop_one("2"));
        assert_eq!(controller.running_count(), 2);
        assert_eq!(
            controller.running_ids(),
            vec!["1".to_string(), "3".to_string()]
        );
        let live: serde_json::Value =
            serde_json::from_str(&handle.last_config().expect("regenerated")).expect("JSON");
        assert_eq!(live["inbounds"].as_array().expect("inbounds").len(), 2);
        assert_eq!(live["route"]["rules"].as_array().expect("rules").len(), 2);
        let isolation = controller.isolation_map();
        assert_eq!(isolation.len(), 2);
        assert!(isolation.iter().all(|entry| entry.config_id != "2"));

        assert!(!controller.stop_one("missing"));
        controller.stop_all();
        assert_eq!(controller.running_count(), 0);
        assert!(!handle.is_running());
        assert_eq!(controller.status(), &EngineStatus::Stopped);
    }

    /// A failed engine `check` blocks start and leaves nothing running.
    #[test]
    fn check_failure_blocks_start() {
        let (mut controller, handle) = EngineController::fake();
        handle.fail_check("boom: bad field");
        let error = controller
            .start_all(&three_standard())
            .expect_err("check failure blocks start");
        assert!(matches!(error, EngineError::CheckFailed { .. }));
        assert!(!handle.is_running());
        assert!(controller.last_error().is_some());
    }

    /// Crash detection surfaces the named cause exactly once.
    #[test]
    fn crash_poll_records_named_cause() {
        let (mut controller, handle) = EngineController::fake();
        controller.start_all(&three_standard()).expect("starts");
        handle.inject_crash("signal: segmentation fault");
        assert_eq!(
            controller.poll().as_deref(),
            Some("signal: segmentation fault")
        );
        assert!(matches!(controller.status(), EngineStatus::Crashed { .. }));
        assert!(controller.status_text().contains("segmentation fault"));
        assert!(controller.poll().is_none(), "cause reported once");
    }

    /// Binary resolution failure names the searched directories.
    #[test]
    fn missing_binary_error_lists_searched_dirs() {
        let error = resolve_engine_binary_in(Some(Path::new("/nonexistent-manyproxy-dir")), "")
            .expect_err("no binary anywhere");
        let text = error.to_string();
        assert!(text.contains("sing-box"), "names binary: {text}");
        assert!(
            text.contains("/nonexistent-manyproxy-dir"),
            "names searched dir: {text}"
        );
        assert!(!text.contains("password"), "no secrets: {text}");
    }

    /// Param errors name the config and field, never the secret values.
    #[test]
    fn bad_params_error_names_field_not_secrets() {
        let configs = vec![fixture(
            "7",
            "Leaky",
            "trojan",
            json!({"server": "10.1.1.7", "password": "HUNTER2"}),
            "127.0.0.1",
            10807,
        )];
        let error = generate(&configs).expect_err("server_port missing");
        let text = error.to_string();
        assert!(text.contains("\"Leaky\""), "names config: {text}");
        assert!(text.contains("server_port"), "names field: {text}");
        assert!(!text.contains("HUNTER2"), "no secrets: {text}");

        let broken = vec![fixture(
            "8",
            "Broken",
            "vless",
            json!({"server": "10.1.1.8", "server_port": 443, "uuid": "x"}),
            "127.0.0.1",
            10808,
        )];
        let mut tampered = broken;
        tampered[0].params = "{not json".to_string();
        let error = generate(&tampered).expect_err("invalid JSON rejected");
        assert!(!error.to_string().contains("not json"));
    }

    /// Custom binds are allowed but flagged; loopback stays quiet.
    #[test]
    fn lan_warning_only_for_non_loopback() {
        assert!(is_loopback_bind("127.0.0.1"));
        assert!(is_loopback_bind("::1"));
        assert!(!is_loopback_bind("0.0.0.0"));
        assert!(lan_exposure_warning("A", "127.0.0.1").is_none());
        let warning = lan_exposure_warning("B", "0.0.0.0").expect("LAN warns");
        assert!(warning.contains("\"B\""), "names config: {warning}");

        let (mut controller, _) = EngineController::fake();
        let mut rows = three_standard();
        rows[0].bind_ip = "0.0.0.0".to_string();
        controller.start_all(&rows).expect("custom bind starts");
        let warnings = controller.lan_warnings();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("\"Alpha\""));
    }

    /// Empty activation set is a typed error, not an empty engine config.
    #[test]
    fn empty_set_is_no_active_configs() {
        assert!(matches!(generate(&[]), Err(EngineError::NoActiveConfigs)));
    }
}
