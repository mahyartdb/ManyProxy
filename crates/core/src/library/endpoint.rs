//! Pure endpoint syntax, collision and suggestion logic for the endpoint
//! editor. No gpui and no store writes: the editor validates a draft with this
//! module and only then touches `store.update`.

use std::net::IpAddr;

use crate::*;

/// A parsed endpoint, ready to write into a row's `bind_ip` and `port`.
/// `bind` is always unbracketed, even when the input was `[::1]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointValue {
    pub bind: String,
    pub port: u16,
}

/// The failing field plus the core message. The UI localizes on `field`; the
/// message stays as the honest fallback for logs and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointError {
    pub field: &'static str,
    pub message: String,
}

impl EndpointError {
    fn bind() -> Self {
        Self {
            field: "bind",
            message: "Bind must be an IP address or localhost.".to_string(),
        }
    }

    fn port() -> Self {
        Self {
            field: "port",
            message: "Port must be a whole number 1-65535.".to_string(),
        }
    }
}

/// A row that already holds the same normalized bind and port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointCollision {
    pub row_id: String,
    pub name: String,
    pub bind: String,
    pub port: u16,
}

/// Syntax only: bind is an IPv4/IPv6 literal or `localhost`, port is a whole
/// number 1-65535. Privileged and reserved ports are a save-gate rule, so the
/// caller runs `validate_listen_for_save` next and keeps this parser honest for
/// tests that prove port 1 parses.
pub fn parse_endpoint(bind: &str, port: &str) -> Result<EndpointValue, EndpointError> {
    let bind = parse_bind(bind)?;
    let port = parse_port(port)?;
    Ok(EndpointValue { bind, port })
}

fn parse_bind(raw: &str) -> Result<String, EndpointError> {
    let trimmed = raw.trim();
    let literal = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    if literal.eq_ignore_ascii_case("localhost") {
        return Ok("localhost".to_string());
    }
    if literal.parse::<IpAddr>().is_ok() {
        // Canonicalize so `::0001` collides with `::1` and brackets never
        // reach `bind_ip`.
        return Ok(literal
            .parse::<IpAddr>()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| literal.to_string()));
    }
    Err(EndpointError::bind())
}

fn parse_port(raw: &str) -> Result<u16, EndpointError> {
    let trimmed = raw.trim();
    // `parse_port_field` accepts `+N`; the editor does not. Digits first, then
    // reuse the store's 1-65535 rule.
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(EndpointError::port());
    }
    match parse_port_field(trimmed) {
        Ok(Some(port)) => Ok(port),
        _ => Err(EndpointError::port()),
    }
}

/// Another row with the same normalized bind and port. Loopback aliases
/// (`""`, `localhost`, `127.0.0.1`, `::1`, `::ffff:127.0.0.1`) collide.
pub fn collision(
    rows: &[ProxyConfig],
    exclude_id: &str,
    value: &EndpointValue,
) -> Option<EndpointCollision> {
    let key = normalize_bind_key(&value.bind);
    rows.iter()
        .find(|row| {
            row.id != exclude_id
                && row.port == value.port
                && normalize_bind_key(&row.bind_ip) == key
        })
        .map(|row| EndpointCollision {
            row_id: row.id.clone(),
            name: row.name.clone(),
            bind: row.bind_ip.clone(),
            port: row.port,
        })
}

/// The first applicable free port at or above `start` for the same normalized
/// bind. Ports on other rows, the privileged range and the engine-reserved
/// ranges are skipped, so a suggestion can always be applied.
pub fn next_free_port(
    rows: &[ProxyConfig],
    exclude_id: &str,
    bind: &str,
    start: u16,
) -> Option<u16> {
    let key = normalize_bind_key(bind);
    let used: std::collections::HashSet<u16> = rows
        .iter()
        .filter(|row| row.id != exclude_id && normalize_bind_key(&row.bind_ip) == key)
        .map(|row| row.port)
        .collect();
    let mut candidate = u32::from(start.max(1024));
    while candidate <= u32::from(u16::MAX) {
        let port = candidate as u16;
        if !used.contains(&port) && validate_listen_for_save("", bind, port).is_ok() {
            return Some(port);
        }
        candidate += 1;
    }
    None
}

/// The display form: `host:port`, with an IPv6 literal bracketed.
pub fn format_endpoint(bind: &str, port: u16) -> String {
    let bind = bind.trim();
    if bind.contains(':') && !bind.starts_with('[') {
        format!("[{bind}]:{port}")
    } else {
        format!("{bind}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, name: &str, bind: &str, port: u16) -> ProxyConfig {
        ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: "vless".to_string(),
            params: Secret::new("{}"),
            group: "work".to_string(),
            enabled: true,
            bind_ip: bind.to_string(),
            port,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        }
    }

    #[test]
    fn endpoint_parses_ipv4_ipv6_and_localhost() {
        assert_eq!(
            parse_endpoint("127.0.0.1", "10801"),
            Ok(EndpointValue {
                bind: "127.0.0.1".to_string(),
                port: 10801
            })
        );
        assert_eq!(
            parse_endpoint("0.0.0.0", "10801").map(|value| value.bind),
            Ok("0.0.0.0".to_string())
        );
        assert_eq!(
            parse_endpoint("::1", "10801").map(|value| value.bind),
            Ok("::1".to_string())
        );
        assert_eq!(
            parse_endpoint("[::1]", "10801").map(|value| value.bind),
            Ok("::1".to_string())
        );
        assert_eq!(
            parse_endpoint("localhost", "10801").map(|value| value.bind),
            Ok("localhost".to_string())
        );
        assert_eq!(format_endpoint("::1", 10801), "[::1]:10801");
        assert_eq!(format_endpoint("127.0.0.1", 10801), "127.0.0.1:10801");
    }

    #[test]
    fn endpoint_rejects_empty_bind_empty_port_zero_and_non_numeric() {
        assert_eq!(parse_endpoint("", "10801").unwrap_err().field, "bind");
        assert_eq!(parse_endpoint("  ", "10801").unwrap_err().field, "bind");
        for port in ["", "0", "auto", "+10801", "-1", "10 801", "0x2a"] {
            let error = parse_endpoint("127.0.0.1", port).unwrap_err();
            assert_eq!(error.field, "port", "port {port:?}");
        }
    }

    #[test]
    fn endpoint_rejects_a_dns_name_the_store_cannot_bind() {
        let error = parse_endpoint("example.local", "10801").unwrap_err();
        assert_eq!(error.field, "bind");
        assert_eq!(error.message, "Bind must be an IP address or localhost.");
    }

    #[test]
    fn endpoint_accepts_port_1_and_65535_syntactically_but_the_save_gate_rejects_1() {
        assert_eq!(
            parse_endpoint("127.0.0.1", "1").map(|value| value.port),
            Ok(1)
        );
        assert_eq!(
            parse_endpoint("127.0.0.1", "65535").map(|value| value.port),
            Ok(65535)
        );
        assert!(validate_listen_for_save("Alpha", "127.0.0.1", 1).is_err());
        assert!(validate_listen_for_save("Alpha", "127.0.0.1", 65535).is_ok());
    }

    #[test]
    fn endpoint_collision_normalizes_loopback_aliases() {
        let rows = vec![row("a", "Alpha", "localhost", 10801)];
        let value = |bind: &str| EndpointValue {
            bind: bind.to_string(),
            port: 10801,
        };
        assert_eq!(
            collision(&rows, "b", &value("127.0.0.1")).map(|found| found.name),
            Some("Alpha".to_string())
        );
        assert_eq!(
            collision(&rows, "b", &value("::1")).map(|found| found.name),
            Some("Alpha".to_string())
        );
        assert_eq!(
            collision(&rows, "b", &value("")).map(|found| found.name),
            Some("Alpha".to_string())
        );
        assert!(collision(&rows, "b", &value("0.0.0.0")).is_none());
        assert!(collision(&rows, "a", &value("127.0.0.1")).is_none());
    }

    #[test]
    fn next_free_port_skips_rows_and_reserved_ranges() {
        let rows = vec![
            row("a", "Alpha", "127.0.0.1", 10801),
            row("b", "Beta", "localhost", 10803),
            row("c", "Gamma", "0.0.0.0", 10804),
        ];
        assert_eq!(next_free_port(&rows, "c", "127.0.0.1", 10801), Some(10802));
        // The first applicable port skips the privileged range.
        assert_eq!(next_free_port(&rows, "a", "127.0.0.1", 1), Some(1024));
        // 11801-11999 and 18501-18999 are the engine's reserved ranges.
        assert_eq!(next_free_port(&rows, "a", "127.0.0.1", 11801), Some(12000));
        assert_eq!(next_free_port(&rows, "a", "127.0.0.1", 18501), Some(19000));
        assert_eq!(next_free_port(&rows, "a", "127.0.0.1", 65535), Some(65535));
        let full = vec![row("top", "Top", "127.0.0.1", 65535)];
        assert_eq!(next_free_port(&full, "a", "127.0.0.1", 65535), None);
    }
}
