//! Ticket 02: ProxyConfig library store.
//!
//! rusqlite-backed persistence for the config library. Every connection gets
//! the sqlite-skill PRAGMAs (WAL, busy_timeout, FK, synchronous, temp_store),
//! all queries are parameterized, and unit tests run on `:memory:` databases.
//! The store owns all validation so UI dialogs pass raw strings straight
//! through and render the returned field-level errors.

use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Default loopback bind for new configs (spec: loopback default, LAN only
/// behind an explicit warning at activation time).
pub const DEFAULT_BIND: &str = "127.0.0.1";
/// Default group for new configs.
pub const DEFAULT_GROUP: &str = "default";
/// Default protocol when the add form leaves it blank.
pub const DEFAULT_PROTOCOL: &str = "vless";
/// Inclusive auto-port range for configs created without an explicit port.
pub const AUTO_PORT_START: u16 = 10801;
pub const AUTO_PORT_END: u16 = 10999;

/// A stored proxy configuration (ticket 02 owns identity, naming, grouping,
/// enable flag, and local bind; protocol params are opaque until the
/// import/custom-core tickets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    /// Stable string id, never reused (AUTOINCREMENT rowid rendered as text).
    pub id: String,
    pub name: String,
    pub protocol: String,
    /// Opaque protocol params; schema-checked by a later ticket.
    pub params: String,
    pub group: String,
    pub enabled: bool,
    pub bind_ip: String,
    pub port: u16,
}

/// Raw input for creating a config. All strings are unvalidated; empty
/// protocol/group/bind fall back to defaults, empty port means auto-assign.
#[derive(Debug, Clone)]
pub struct NewConfig {
    pub name: String,
    pub protocol: String,
    pub params: String,
    pub group: String,
    pub bind_ip: String,
    /// Raw port text; `""` (or whitespace) means auto-assign.
    pub port: String,
}

/// Raw patch for editing a config. `None` means unchanged; `Some("")` port
/// means auto-assign a fresh port.
#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub params: Option<String>,
    pub group: Option<String>,
    pub bind_ip: Option<String>,
    pub port: Option<String>,
}

/// One field-level validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

impl FieldError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

/// Store-level failures.
#[derive(Debug)]
pub enum StoreError {
    /// One or more fields rejected; nothing was written.
    Invalid(Vec<FieldError>),
    /// No config with this id.
    NotFound(String),
    Sql(rusqlite::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Invalid(errors) => {
                let parts: Vec<String> = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect();
                write!(f, "invalid config ({})", parts.join("; "))
            }
            StoreError::NotFound(id) => write!(f, "config not found: {id}"),
            StoreError::Sql(error) => write!(f, "database error: {error}"),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        StoreError::Sql(error)
    }
}

/// Parse raw port text from a form field. Empty means auto-assign (`None`).
/// `Some(0)` never validates: it is rejected as out of range.
pub fn parse_port_field(raw: &str) -> Result<Option<u16>, FieldError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.parse::<u32>() {
        Ok(port) if (1..=65_535).contains(&port) => Ok(Some(port as u16)),
        _ => Err(FieldError::new(
            "port",
            format!("port must be 1-65535, got '{trimmed}'"),
        )),
    }
}

/// Both proxy URL forms for a local endpoint (contract: copy action copies
/// both the `socks5://` and `http://` forms).
pub fn proxy_urls(bind_ip: &str, port: u16) -> (String, String) {
    (
        format!("socks5://{bind_ip}:{port}"),
        format!("http://{bind_ip}:{port}"),
    )
}

/// Confirm-dialog description for removing a config. Names the config.
pub fn delete_confirm_text(name: &str) -> String {
    format!("Remove \"{name}\"? The stored configuration is deleted. Nothing else changes.")
}

/// Map one result row to a ProxyConfig. All columns are bound by position
/// with explicit types (STRICT table guarantees the shapes).
fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyConfig> {
    let id: i64 = row.get(0)?;
    let enabled: i64 = row.get(5)?;
    let port: i64 = row.get(7)?;
    Ok(ProxyConfig {
        id: id.to_string(),
        name: row.get(1)?,
        protocol: row.get(2)?,
        params: row.get(3)?,
        group: row.get(4)?,
        enabled: enabled != 0,
        bind_ip: row.get(6)?,
        port: port as u16,
    })
}

/// Fully validated + defaulted record content, ready for INSERT/UPDATE.
struct ValidatedConfig {
    name: String,
    protocol: String,
    params: String,
    group: String,
    bind_ip: String,
    port: u16,
}

impl ProxyConfig {
    /// Integer rowid behind the string id; -1 when the id is foreign
    /// (only used for UPDATE targets already loaded from this store).
    fn rowid(&self) -> i64 {
        self.id.parse().unwrap_or(-1)
    }
}

/// Non-empty after trimming, else push a `name` error and return None.
fn validated_name(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        errors.push(FieldError::new("name", "name must not be empty"));
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Blank input falls back to `default`; otherwise the trimmed value.
fn defaulted(raw: &str, default: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Blank means loopback default; otherwise the text must parse as an IP
/// literal (spec: loopback default; LAN binds gated at activation time).
fn validated_bind(raw: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(DEFAULT_BIND.to_string());
    }
    match trimmed.parse::<IpAddr>() {
        Ok(_) => Some(trimmed.to_string()),
        Err(_) => {
            errors.push(FieldError::new(
                "bind_ip",
                format!("bind address must be an IP literal, got '{trimmed}'"),
            ));
            None
        }
    }
}
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open a throwaway store for unit tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::open(Connection::open_in_memory()?)
    }

    /// Open (creating parents as needed) a file-backed store.
    pub fn open_file(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
                })?;
            }
        }
        Self::open(Connection::open(path)?)
    }

    /// Default file location: `%APPDATA%/ManyProxy/library.db` on Windows,
    /// `$HOME/.manyproxy/library.db` elsewhere. `MANYPROXY_DB` overrides.
    pub fn default_path() -> PathBuf {
        if let Ok(path) = std::env::var("MANYPROXY_DB") {
            if !path.trim().is_empty() {
                return PathBuf::from(path);
            }
        }
        #[cfg(windows)]
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("ManyProxy").join("library.db");
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".manyproxy").join("library.db")
    }

    fn open(conn: Connection) -> Result<Self, StoreError> {
        let store = Self { conn };
        store.apply_pragmas()?;
        store.create_schema()?;
        Ok(store)
    }

    /// sqlite-skill setup: must run on every connection.
    fn apply_pragmas(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
        )?;
        Ok(())
    }

    fn create_schema(&self) -> Result<(), StoreError> {
        // `grp` avoids the reserved word GROUP. Ports are intentionally NOT
        // unique: two stored configs may share a port; the conflict error
        // naming both parties is raised at activation time (ticket 03).
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proxy_configs (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL,
               protocol TEXT NOT NULL DEFAULT 'vless',
               params TEXT NOT NULL DEFAULT '{}',
               grp TEXT NOT NULL DEFAULT 'default',
               enabled INTEGER NOT NULL DEFAULT 0,
               bind_ip TEXT NOT NULL DEFAULT '127.0.0.1',
               port INTEGER NOT NULL
             ) STRICT;
             CREATE INDEX IF NOT EXISTS idx_proxy_configs_enabled
               ON proxy_configs(enabled);",
        )?;
        Ok(())
    }

    /// Create a disabled config; invalid input is rejected with field errors.
    pub fn add(&mut self, input: NewConfig) -> Result<ProxyConfig, StoreError> {
        let validated = self.validate_new(&input)?;
        self.conn.execute(
            "INSERT INTO proxy_configs
               (name, protocol, params, grp, enabled, bind_ip, port)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                validated.name,
                validated.protocol,
                validated.params,
                validated.group,
                validated.bind_ip,
                validated.port as i64,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(&id.to_string())?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// All configs in stable id order.
    pub fn list(&self) -> Result<Vec<ProxyConfig>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port
             FROM proxy_configs ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Fetch one config by id.
    pub fn get(&self, id: &str) -> Result<Option<ProxyConfig>, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(None),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, name, protocol, params, grp, enabled, bind_ip, port
             FROM proxy_configs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![rowid], map_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Edit a stopped-config record. Validation runs before any write, so
    /// invalid input leaves the record unchanged.
    pub fn update(&mut self, id: &str, patch: ConfigPatch) -> Result<ProxyConfig, StoreError> {
        let current = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let merged = self.validate_patch(&current, &patch)?;
        self.conn.execute(
            "UPDATE proxy_configs
             SET name = ?1, protocol = ?2, params = ?3, grp = ?4, bind_ip = ?5, port = ?6
             WHERE id = ?7",
            params![
                merged.name,
                merged.protocol,
                merged.params,
                merged.group,
                merged.bind_ip,
                merged.port as i64,
                current.rowid(),
            ],
        )?;
        self.get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// Remove a config. Returns false when the id does not exist.
    pub fn remove(&mut self, id: &str) -> Result<bool, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Ok(false),
        };
        let removed = self
            .conn
            .execute("DELETE FROM proxy_configs WHERE id = ?1", params![rowid])?;
        Ok(removed > 0)
    }

    /// Flip the enabled flag.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<ProxyConfig, StoreError> {
        let rowid: i64 = match id.parse() {
            Ok(rowid) => rowid,
            Err(_) => return Err(StoreError::NotFound(id.to_string())),
        };
        let changed = self.conn.execute(
            "UPDATE proxy_configs SET enabled = ?1 WHERE id = ?2",
            params![enabled as i64, rowid],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// First free port in the auto range, given the set of taken ports.
    fn auto_port(&self) -> Result<u16, StoreError> {
        let mut stmt = self.conn.prepare("SELECT port FROM proxy_configs")?;
        let taken: HashSet<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        for port in AUTO_PORT_START..=AUTO_PORT_END {
            if !taken.contains(&(port as i64)) {
                return Ok(port);
            }
        }
        Err(StoreError::Invalid(vec![FieldError::new(
            "port",
            "no automatic port left in the auto range",
        )]))
    }

    /// Validate fresh input, applying defaults and auto port assignment.
    fn validate_new(&self, input: &NewConfig) -> Result<ValidatedConfig, StoreError> {
        let mut errors = Vec::new();
        let name = validated_name(&input.name, &mut errors);
        let protocol = defaulted(&input.protocol, DEFAULT_PROTOCOL);
        let params = defaulted(&input.params, "{}");
        let group = defaulted(&input.group, DEFAULT_GROUP);
        let bind_ip = validated_bind(&input.bind_ip, &mut errors);
        let port = match parse_port_field(&input.port) {
            Ok(None) => match self.auto_port() {
                Ok(port) => Some(port),
                Err(StoreError::Invalid(mut auto_errors)) => {
                    errors.append(&mut auto_errors);
                    None
                }
                Err(error) => return Err(error),
            },
            Ok(port) => port,
            Err(error) => {
                errors.push(error);
                None
            }
        };
        if !errors.is_empty() {
            return Err(StoreError::Invalid(errors));
        }
        Ok(ValidatedConfig {
            name: name.unwrap_or_default(),
            protocol,
            params,
            group,
            bind_ip: bind_ip.unwrap_or_default(),
            port: port.unwrap_or(AUTO_PORT_START),
        })
    }

    /// Validate a patch against the current record; pure until the write.
    fn validate_patch(
        &self,
        current: &ProxyConfig,
        patch: &ConfigPatch,
    ) -> Result<ValidatedConfig, StoreError> {
        let mut errors = Vec::new();
        let name = match &patch.name {
            Some(raw) => validated_name(raw, &mut errors),
            None => Some(current.name.clone()),
        };
        let protocol = match &patch.protocol {
            Some(raw) => Some(defaulted(raw, DEFAULT_PROTOCOL)),
            None => Some(current.protocol.clone()),
        };
        let params = match &patch.params {
            Some(raw) => Some(defaulted(raw, "{}")),
            None => Some(current.params.clone()),
        };
        let group = match &patch.group {
            Some(raw) => Some(defaulted(raw, DEFAULT_GROUP)),
            None => Some(current.group.clone()),
        };
        let bind_ip = match &patch.bind_ip {
            Some(raw) => validated_bind(raw, &mut errors),
            None => Some(current.bind_ip.clone()),
        };
        let port = match &patch.port {
            Some(raw) => match parse_port_field(raw) {
                Ok(None) => match self.auto_port() {
                    Ok(port) => Some(port),
                    Err(StoreError::Invalid(mut auto_errors)) => {
                        errors.append(&mut auto_errors);
                        None
                    }
                    Err(error) => return Err(error),
                },
                Ok(port) => port,
                Err(error) => {
                    errors.push(error);
                    None
                }
            },
            None => Some(current.port),
        };
        if !errors.is_empty() {
            return Err(StoreError::Invalid(errors));
        }
        Ok(ValidatedConfig {
            name: name.unwrap_or_else(|| current.name.clone()),
            protocol: protocol.unwrap_or_else(|| current.protocol.clone()),
            params: params.unwrap_or_else(|| current.params.clone()),
            group: group.unwrap_or_else(|| current.group.clone()),
            bind_ip: bind_ip.unwrap_or_else(|| current.bind_ip.clone()),
            port: port.unwrap_or(current.port),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input(name: &str) -> NewConfig {
        NewConfig {
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: String::new(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        }
    }

    /// Issue 02, scenario 1: empty library + add with loopback bind and auto
    /// port yields a disabled record that persists (visible via list).
    #[test]
    fn add_config_with_defaults_persists_disabled() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        assert!(store.list().expect("list works").is_empty());

        let created = store.add(valid_input("Work")).expect("valid add works");

        assert_eq!(created.name, "Work");
        assert!(!created.enabled);
        assert_eq!(created.bind_ip, DEFAULT_BIND);
        assert_eq!(created.group, DEFAULT_GROUP);
        assert_eq!(created.protocol, DEFAULT_PROTOCOL);
        assert!(
            (AUTO_PORT_START..=AUTO_PORT_END).contains(&created.port),
            "auto port in range, got {}",
            created.port
        );
        assert!(!created.id.is_empty());

        let rows = store.list().expect("list works");
        assert_eq!(rows, vec![created]);
    }

    /// Invalid add input is rejected per field and writes nothing.
    #[test]
    fn add_invalid_reports_fields_and_writes_nothing() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let error = store
            .add(NewConfig {
                name: "   ".to_string(),
                protocol: String::new(),
                params: String::new(),
                group: String::new(),
                bind_ip: "not-an-ip".to_string(),
                port: "99999".to_string(),
            })
            .expect_err("invalid add must fail");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        let names: Vec<&str> = fields.iter().map(|e| e.field).collect();
        assert!(names.contains(&"name"), "name error, got {names:?}");
        assert!(names.contains(&"bind_ip"), "bind error, got {names:?}");
        assert!(names.contains(&"port"), "port error, got {names:?}");
        assert!(
            store.list().expect("list works").is_empty(),
            "failed add writes nothing"
        );
    }

    /// Issue 02, scenario 2: editing with an invalid port keeps the record.
    #[test]
    fn edit_invalid_port_leaves_record_unchanged() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");

        let error = store
            .update(
                &created.id,
                ConfigPatch {
                    port: Some("99999".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("invalid port edit must fail");
        let StoreError::Invalid(fields) = error else {
            panic!("expected Invalid, got {error}");
        };
        assert!(fields.iter().any(|e| e.field == "port"));

        let kept = store
            .get(&created.id)
            .expect("get works")
            .expect("record still present");
        assert_eq!(kept, created, "record unchanged after failed edit");
    }

    /// Invalid name edit is rejected and the record is unchanged.
    #[test]
    fn edit_invalid_name_leaves_record_unchanged() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");

        store
            .update(
                &created.id,
                ConfigPatch {
                    name: Some("  ".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("blank name edit must fail");

        let kept = store
            .get(&created.id)
            .expect("get works")
            .expect("record still present");
        assert_eq!(kept, created);
    }

    /// Valid edits apply; unknown ids report NotFound.
    #[test]
    fn edit_valid_fields_applies() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Home")).expect("valid add works");

        let updated = store
            .update(
                &created.id,
                ConfigPatch {
                    name: Some("Cabin".to_string()),
                    bind_ip: Some("127.0.0.2".to_string()),
                    port: Some("10821".to_string()),
                    ..Default::default()
                },
            )
            .expect("valid edit works");
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "Cabin");
        assert_eq!(updated.bind_ip, "127.0.0.2");
        assert_eq!(updated.port, 10821);

        let missing = store.update(
            "no-such-id",
            ConfigPatch {
                name: Some("X".to_string()),
                ..Default::default()
            },
        );
        assert!(
            matches!(missing, Err(StoreError::NotFound(_))),
            "unknown id is NotFound, got {missing:?}"
        );
    }

    /// Issue 02, scenario 3 (store half): remove deletes exactly one record.
    #[test]
    fn remove_deletes_only_target() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store.add(valid_input("One")).expect("add works");
        let second = store.add(valid_input("Two")).expect("add works");

        assert!(store.remove(&first.id).expect("remove works"));
        assert!(!store.remove(&first.id).expect("repeat remove works"));

        let rows = store.list().expect("list works");
        assert_eq!(rows, vec![second]);
    }

    /// Enabled flag flips; unknown ids report NotFound.
    #[test]
    fn set_enabled_flips_flag() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let created = store.add(valid_input("Work")).expect("add works");
        assert!(!created.enabled);

        let on = store.set_enabled(&created.id, true).expect("enable works");
        assert!(on.enabled);
        let off = store
            .set_enabled(&created.id, false)
            .expect("disable works");
        assert!(!off.enabled);

        assert!(matches!(
            store.set_enabled("no-such-id", true),
            Err(StoreError::NotFound(_))
        ));
    }

    /// Ids are stable and never reused after removal.
    #[test]
    fn ids_are_stable_and_never_reused() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store.add(valid_input("One")).expect("add works");
        let second = store.add(valid_input("Two")).expect("add works");
        assert_ne!(first.id, second.id);

        assert!(store.remove(&first.id).expect("remove works"));
        let third = store.add(valid_input("Three")).expect("add works");
        assert_ne!(third.id, first.id, "removed id is never reused");
        assert_ne!(third.id, second.id);

        let kept = store
            .get(&second.id)
            .expect("get works")
            .expect("second still present");
        assert_eq!(kept, second, "surviving record is untouched");
    }

    /// Auto ports do not collide across adds.
    #[test]
    fn auto_ports_are_unique() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let mut ports = HashSet::new();
        for index in 0..5 {
            let created = store
                .add(valid_input(&format!("Box-{index}")))
                .expect("add works");
            assert!(ports.insert(created.port), "port reused: {}", created.port);
        }
    }

    /// Stored configs may share a port; the conflict error naming both
    /// parties is raised at activation time (ticket 03), not at rest.
    #[test]
    fn duplicate_ports_allowed_at_rest() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let first = store
            .add(NewConfig {
                port: "10821".to_string(),
                ..valid_input("One")
            })
            .expect("add works");
        let second = store
            .add(NewConfig {
                port: "10821".to_string(),
                ..valid_input("Two")
            })
            .expect("duplicate port add works");
        assert_eq!(first.port, second.port);
    }

    /// Port field parsing covers the issue's 99999 example plus blanks.
    #[test]
    fn port_field_parsing() {
        assert_eq!(parse_port_field("").expect("blank is auto"), None);
        assert_eq!(parse_port_field("   ").expect("spaces are auto"), None);
        assert_eq!(
            parse_port_field("10821").expect("valid parses"),
            Some(10821)
        );
        assert_eq!(parse_port_field("1").expect("low edge parses"), Some(1));
        assert_eq!(
            parse_port_field("65535").expect("high edge parses"),
            Some(65535)
        );
        for raw in ["0", "99999", "abc", "10.5", "-1"] {
            let error = parse_port_field(raw).expect_err(&format!("{raw} is invalid"));
            assert_eq!(error.field, "port", "field name for {raw}");
        }
    }

    /// Bind validation: loopback default, any parseable IP allowed at rest.
    #[test]
    fn bind_validation() {
        let mut store = Store::open_in_memory().expect("in-memory store opens");
        let loopback = store.add(valid_input("A")).expect("default bind works");
        assert_eq!(loopback.bind_ip, DEFAULT_BIND);

        let lan = store
            .add(NewConfig {
                bind_ip: "0.0.0.0".to_string(),
                ..valid_input("B")
            })
            .expect("explicit bind stores");
        assert_eq!(lan.bind_ip, "0.0.0.0");
    }

    /// Issue 02, scenario 4 (store half): URL pair formats.
    #[test]
    fn proxy_url_pair_format() {
        let (socks, http) = proxy_urls("127.0.0.1", 10801);
        assert_eq!(socks, "socks5://127.0.0.1:10801");
        assert_eq!(http, "http://127.0.0.1:10801");
    }

    /// Delete confirm text names the config.
    #[test]
    fn delete_confirm_names_config() {
        let text = delete_confirm_text("Work VPN");
        assert!(
            text.contains("Work VPN"),
            "confirm names the config, got {text}"
        );
    }

    /// File-backed stores persist across reopen (restart scenario).
    #[test]
    fn file_store_persists_across_reopen() {
        let path =
            std::env::temp_dir().join(format!("manyproxy-ticket02-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let created = {
            let mut store = Store::open_file(&path).expect("file store opens");
            store.add(valid_input("Persisted")).expect("add works")
        };
        let reopened = Store::open_file(&path).expect("reopen works");
        let rows = reopened.list().expect("list works");
        assert_eq!(rows, vec![created]);
        let _ = std::fs::remove_file(&path);
    }

    /// IpAddr import is used (keeps the dependency honest if refactored).
    #[test]
    fn localhost_parses_as_loopback() {
        let ip: IpAddr = DEFAULT_BIND.parse().expect("default bind parses");
        assert!(ip.is_loopback());
    }
}
