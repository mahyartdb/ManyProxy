//! The one protocol table: every name a row's `protocol` can spell and the
//! alias each folds into.
//! Belongs: canonical names, alias parsing, and which protocols a probe can
//! reach.
//! Not here: how a protocol is translated for a core (`engine`), how a share
//! link scheme becomes a protocol (`import`).

/// A row's protocol, by its canonical name. `ProxyConfig::protocol` stays a
/// `String`; this is what every layer agrees it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Hysteria,
    Tuic,
    Wireguard,
    Amneziawg,
    Socks,
    Snell,
    Shadowtls,
    AnyTls,
    Ssh,
    Noise,
    Http,
    Https,
    Naive,
    Xray,
    CustomOutbound,
    CustomConfig,
    ExtraCore,
    Chain,
    Direct,
    Mieru,
    Juicity,
    TrustTunnel,
    OpenVpn,
    Tailscale,
    Masque,
}

impl Protocol {
    /// Trims, lowercases and folds one alias into its protocol. `None` for
    /// anything no layer has ever stored.
    pub fn parse(raw: &str) -> Option<Protocol> {
        let lowered = raw.trim().to_ascii_lowercase();
        Protocol::ALL.iter().copied().find(|protocol| {
            protocol.as_str() == lowered || protocol.aliases().contains(&lowered.as_str())
        })
    }

    pub fn as_str(self) -> &'static str {
        CANONICAL[self as usize].0
    }

    /// Every other spelling of this protocol. Empty for most.
    fn aliases(self) -> &'static [&'static str] {
        CANONICAL[self as usize].1
    }

    pub const ALL: &'static [Protocol] = &[
        Protocol::Vless,
        Protocol::Vmess,
        Protocol::Trojan,
        Protocol::Shadowsocks,
        Protocol::Hysteria2,
        Protocol::Hysteria,
        Protocol::Tuic,
        Protocol::Wireguard,
        Protocol::Amneziawg,
        Protocol::Socks,
        Protocol::Snell,
        Protocol::Shadowtls,
        Protocol::AnyTls,
        Protocol::Ssh,
        Protocol::Noise,
        Protocol::Http,
        Protocol::Https,
        Protocol::Naive,
        Protocol::Xray,
        Protocol::CustomOutbound,
        Protocol::CustomConfig,
        Protocol::ExtraCore,
        Protocol::Chain,
        Protocol::Direct,
        Protocol::Mieru,
        Protocol::Juicity,
        Protocol::TrustTunnel,
        Protocol::OpenVpn,
        Protocol::Tailscale,
        Protocol::Masque,
    ];
}

/// One row per variant, in variant order: the canonical name and every other
/// spelling. `Protocol::as_str` indexes this by the variant's position, so a
/// variant is never reordered or inserted without its row moving with it;
/// `every_canonical_name_round_trips` fails when the two drift.
const CANONICAL: &[(&str, &[&str])] = &[
    ("vless", &[]),
    ("vmess", &[]),
    ("trojan", &[]),
    ("shadowsocks", &["ss"]),
    ("hysteria2", &["hy2"]),
    ("hysteria", &[]),
    ("tuic", &[]),
    ("wireguard", &["wg"]),
    ("amneziawg", &["awg", "amnezia-wg"]),
    ("socks", &["socks5"]),
    ("snell", &[]),
    ("shadowtls", &["shadow-tls", "shadow_tls"]),
    ("anytls", &[]),
    ("ssh", &[]),
    ("noise", &[]),
    ("http", &[]),
    ("https", &[]),
    ("naive", &[]),
    ("xray", &["xray-vless", "xray_vless", "vless-xray"]),
    (
        "custom-outbound",
        &[
            "custom",
            "custom-out",
            "custom_outbound",
            "custom-outbound-json",
        ],
    ),
    (
        "custom-config",
        &[
            "custom_config",
            "custom-full",
            "custom_full",
            "custom-full-config",
        ],
    ),
    ("extra-core", &["extra", "extra_core", "extracore"]),
    ("chain", &["chained", "chain-outbound", "chain_outbound"]),
    ("direct", &[]),
    ("mieru", &[]),
    ("juicity", &[]),
    ("trusttunnel", &[]),
    ("openvpn", &[]),
    ("tailscale", &[]),
    ("masque", &[]),
];

/// True when a health probe can reach this protocol's row at its own
/// `bind`/`port`. The probe is one loopback dial that does not depend on the
/// protocol. False only for the protocols `engine::validate_activation`
/// refuses, which have no endpoint. `extra-core` and `custom-config` rows get
/// no listener from the engine, but the process they hand off to is expected
/// to serve `bind`/`port`, so probing it reports whether it does.
pub fn is_probeable_protocol(protocol: &str) -> bool {
    Protocol::parse(protocol).is_some_and(|protocol| {
        !matches!(
            protocol,
            Protocol::Mieru
                | Protocol::Juicity
                | Protocol::TrustTunnel
                | Protocol::OpenVpn
                | Protocol::Tailscale
                | Protocol::Masque
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineError;
    use crate::store::ProxyConfig;

    /// The alias table the enum replaced, spelled with the name the majority
    /// of the old tables agreed on. Each place a table disagreed is named.
    const DECIDED: &[(&str, Protocol)] = &[
        ("vless", Protocol::Vless),
        ("vmess", Protocol::Vmess),
        ("trojan", Protocol::Trojan),
        ("shadowsocks", Protocol::Shadowsocks),
        ("ss", Protocol::Shadowsocks),
        ("hysteria2", Protocol::Hysteria2),
        // The store's table lacked this one; the engine, the prober and both
        // importer tables had it.
        ("hy2", Protocol::Hysteria2),
        ("hysteria", Protocol::Hysteria),
        ("tuic", Protocol::Tuic),
        ("wireguard", Protocol::Wireguard),
        ("wg", Protocol::Wireguard),
        ("amneziawg", Protocol::Amneziawg),
        ("awg", Protocol::Amneziawg),
        ("amnezia-wg", Protocol::Amneziawg),
        ("socks", Protocol::Socks),
        ("socks5", Protocol::Socks),
        ("snell", Protocol::Snell),
        ("shadowtls", Protocol::Shadowtls),
        ("shadow-tls", Protocol::Shadowtls),
        ("shadow_tls", Protocol::Shadowtls),
        ("anytls", Protocol::AnyTls),
        ("ssh", Protocol::Ssh),
        ("noise", Protocol::Noise),
        ("http", Protocol::Http),
        // The engine's table folded this into `http` for node generation;
        // the store and both importer tables kept it its own protocol.
        ("https", Protocol::Https),
        ("naive", Protocol::Naive),
        ("xray", Protocol::Xray),
        ("xray-vless", Protocol::Xray),
        ("xray_vless", Protocol::Xray),
        ("vless-xray", Protocol::Xray),
        ("custom", Protocol::CustomOutbound),
        ("custom-out", Protocol::CustomOutbound),
        ("custom-outbound", Protocol::CustomOutbound),
        ("custom_outbound", Protocol::CustomOutbound),
        ("custom-outbound-json", Protocol::CustomOutbound),
        ("custom-config", Protocol::CustomConfig),
        ("custom_config", Protocol::CustomConfig),
        ("custom-full", Protocol::CustomConfig),
        ("custom_full", Protocol::CustomConfig),
        ("custom-full-config", Protocol::CustomConfig),
        ("extra", Protocol::ExtraCore),
        ("extra-core", Protocol::ExtraCore),
        ("extra_core", Protocol::ExtraCore),
        ("extracore", Protocol::ExtraCore),
        ("chain", Protocol::Chain),
        ("chained", Protocol::Chain),
        ("chain-outbound", Protocol::Chain),
        ("chain_outbound", Protocol::Chain),
        ("direct", Protocol::Direct),
        ("mieru", Protocol::Mieru),
        ("juicity", Protocol::Juicity),
        ("trusttunnel", Protocol::TrustTunnel),
        ("openvpn", Protocol::OpenVpn),
        ("tailscale", Protocol::Tailscale),
        ("masque", Protocol::Masque),
    ];

    #[test]
    fn every_alias_parses_to_the_decided_name() {
        for (alias, expected) in DECIDED {
            let parsed = Protocol::parse(alias);
            assert_eq!(parsed, Some(*expected), "alias \"{alias}\"");
            assert_eq!(
                parsed.map(Protocol::as_str),
                Some(expected.as_str()),
                "alias \"{alias}\""
            );
        }
        assert_eq!(DECIDED.len(), 55, "30 canonical names and 25 aliases");
        for (raw, why) in [
            ("vles", "a typo"),
            ("", "empty"),
            ("  ", "blank"),
            ("h y s t e r i a 2", "inner spaces"),
            ("v2rayn", "a link scheme, not a protocol"),
            ("xhttp", "a clash type, not a protocol"),
            ("naive+https", "a link scheme, not a protocol"),
            ("hysteria2+realm", "a link scheme, not a protocol"),
        ] {
            assert_eq!(Protocol::parse(raw), None, "{why}: {raw:?}");
        }
    }

    #[test]
    fn every_canonical_name_round_trips() {
        assert_eq!(
            Protocol::ALL.len(),
            30,
            "the 27 startable or importable kinds plus three"
        );
        assert_eq!(
            CANONICAL.len(),
            Protocol::ALL.len(),
            "as_str indexes the table by variant position"
        );
        for (index, protocol) in Protocol::ALL.iter().enumerate() {
            let name = protocol.as_str();
            assert_eq!(*protocol as usize, index, "{name} sits at its table row");
            assert_eq!(CANONICAL[index].0, name, "{name}");
            assert_eq!(Protocol::parse(name), Some(*protocol), "{name}");
        }
        let mut names: Vec<&str> = Protocol::ALL.iter().map(|p| p.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "canonical names are unique");
    }

    #[test]
    fn every_alias_is_listed_under_its_protocol() {
        for protocol in Protocol::ALL {
            for alias in protocol.aliases() {
                assert_eq!(
                    Protocol::parse(alias),
                    Some(*protocol),
                    "alias {alias:?} is listed under {}",
                    protocol.as_str()
                );
            }
        }
    }

    #[test]
    fn parse_trims_and_lowercases() {
        for (raw, expected) in [
            (" VLESS ", Protocol::Vless),
            ("vLess", Protocol::Vless),
            ("SS", Protocol::Shadowsocks),
            ("\tHy2\n", Protocol::Hysteria2),
            ("Custom-Out ", Protocol::CustomOutbound),
            ("EXTRA_CORE", Protocol::ExtraCore),
        ] {
            assert_eq!(Protocol::parse(raw), Some(expected), "{raw:?}");
        }
    }

    /// The protocols Start refuses: no translator arm serves them, so they
    /// have no listener for a probe to dial.
    const UNSERVABLE: &[Protocol] = &[
        Protocol::Mieru,
        Protocol::Juicity,
        Protocol::TrustTunnel,
        Protocol::OpenVpn,
        Protocol::Tailscale,
        Protocol::Masque,
    ];

    /// What Start does with one row of `protocol`, judged by the engine. `Ok`
    /// when it would start, `Err(UnsupportedProtocol)` when the protocol
    /// itself is refused. Any other refusal means the fixture is wrong, not
    /// the protocol, and panics rather than passing as "unservable".
    fn engine_verdict(protocol: &str) -> Result<(), EngineError> {
        use serde_json::json;
        const UUID: &str = "11111111-2222-4333-8444-555555555555";
        let params = match protocol {
            "vless" => {
                json!({"server": "198.51.100.9", "server_port": 443, "uuid": UUID, "encryption": "none"})
            }
            "vmess" => json!({"server": "198.51.100.9", "server_port": 443, "uuid": UUID}),
            "trojan" => json!({"server": "198.51.100.9", "server_port": 443, "password": "p"}),
            "shadowsocks" => {
                json!({"server": "198.51.100.9", "server_port": 8388, "method": "aes-128-gcm", "password": "p"})
            }
            "hysteria2" => json!({"server": "192.0.2.13", "server_port": 443, "password": "pw"}),
            "hysteria" => {
                json!({"type": "hysteria", "server": "192.0.2.14", "server_port": 443, "auth": "secret", "up": "100", "down": "50 Mbps"})
            }
            "tuic" => {
                json!({"type": "tuic", "server": "192.0.2.12", "server_port": 443, "uuid": UUID, "password": "pass1"})
            }
            "wireguard" => {
                json!({"type": "wireguard", "server": "192.0.2.44", "server_port": 51820, "private_key": "LOCAL-PRIVATE", "public_key": "REMOTE-PUBLIC"})
            }
            "amneziawg" => {
                json!({"type": "amneziawg", "server": "192.0.2.90", "server_port": 51820, "private_key": "LOCAL-PRIVATE", "public_key": "REMOTE-PUBLIC", "awg_version": "2.0"})
            }
            "socks" => json!({"server": "198.51.100.9", "server_port": 1080}),
            "snell" => json!({"server": "198.51.100.9", "server_port": 443, "psk": "p"}),
            "shadowtls" => json!({"server": "198.51.100.9", "server_port": 443, "password": "p"}),
            "anytls" => json!({"server": "198.51.100.9", "server_port": 443, "password": "p"}),
            "ssh" => {
                json!({"server": "198.51.100.9", "server_port": 22, "user": "u", "password": "p"})
            }
            "noise" => {
                json!({"type": "noise", "server": "192.0.2.44", "server_port": 51820, "password": "NOISE-KEY", "key": "NOISE-KEY"})
            }
            "http" => json!({"server": "198.51.100.9", "server_port": 8080}),
            "https" => json!({"server": "198.51.100.9", "server_port": 443}),
            "naive" => {
                json!({"server": "198.51.100.9", "server_port": 443, "username": "u", "password": "p"})
            }
            "xray" => {
                json!({"server": "198.51.100.9", "server_port": 443, "uuid": UUID, "flow": "xtls-rprx-vision", "network": "tcp", "security": "tls"})
            }
            "custom-outbound" => {
                json!({"type": "vless", "server": "203.0.113.9", "server_port": 443, "uuid": UUID})
            }
            "custom-config" => json!({"outbounds": [{"type": "direct", "tag": "a"}]}),
            "extra-core" => json!({"exe": "user-core.exe", "config": {"listen": "127.0.0.1"}}),
            "chain" => json!({"nodes": [{"type": "direct"}]}),
            "direct" => json!({}),
            _ => json!({"server": "198.51.100.9", "server_port": 443}),
        };
        let row = ProxyConfig {
            id: protocol.to_string(),
            name: protocol.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string().into(),
            group: "default".to_string(),
            enabled: true,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        match crate::engine::validate_activation(&[row]) {
            Ok(_) => Ok(()),
            Err(error @ EngineError::UnsupportedProtocol { .. }) => Err(error),
            Err(other) => panic!("fixture for {protocol} is refused for another reason: {other}"),
        }
    }

    #[test]
    fn probeable_equals_servable() {
        let mut refused = Vec::new();
        for protocol in Protocol::ALL {
            let name = protocol.as_str();
            let served = engine_verdict(name).is_ok();
            let probed = is_probeable_protocol(name);
            assert_eq!(
                probed, served,
                "{name}: probeable ({probed}) must equal served ({served})"
            );
            if !served {
                refused.push(*protocol);
            }
        }
        assert_eq!(
            refused, UNSERVABLE,
            "exactly these protocols are refused, and only they are unprobeable"
        );
        for alias in [
            "ss",
            "hy2",
            "socks5",
            "awg",
            "shadow-tls",
            "xray-vless",
            "custom",
            "custom_config",
            "extracore",
            "chained",
        ] {
            assert!(is_probeable_protocol(alias), "alias {alias} is probeable");
        }
        for (raw, why) in [
            ("dial-up", "not a protocol"),
            ("mieru", "importable, never startable"),
            ("masque", "in the enum, never startable"),
        ] {
            assert!(!is_probeable_protocol(raw), "{why}: {raw}");
        }
    }

    #[test]
    fn picker_protocols_parse_and_the_three_unservable_are_named() {
        // The picker lists the 27 importable protocols. Twenty-four start;
        // `mieru`, `juicity` and `trusttunnel` are importable and storable
        // but have no translator arm, so Start refuses them (plan finding F5).
        let picker = crate::store::VALID_PROTOCOLS;
        assert_eq!(picker.len(), 27);
        let mut unservable_in_picker = Vec::new();
        for name in picker {
            let protocol = Protocol::parse(name).unwrap_or_else(|| panic!("{name} parses"));
            assert_eq!(protocol.as_str(), *name, "{name} is already canonical");
            if !engine_verdict(name).is_ok() {
                unservable_in_picker.push(protocol);
            }
        }
        assert_eq!(
            unservable_in_picker,
            [Protocol::Mieru, Protocol::Juicity, Protocol::TrustTunnel],
            "exactly these picker protocols cannot start"
        );
        // Every protocol outside the picker is one of the three new kinds,
        // which the importer does not offer yet.
        for protocol in Protocol::ALL {
            let name = protocol.as_str();
            if !picker.contains(&name) {
                assert!(
                    matches!(
                        protocol,
                        Protocol::OpenVpn | Protocol::Tailscale | Protocol::Masque
                    ),
                    "{name} is outside the picker"
                );
            }
        }
    }

    #[test]
    fn predicates_agree_with_parse() {
        use crate::engine::{
            is_awg_protocol, is_chain_protocol, is_custom_config_protocol,
            is_custom_outbound_protocol, is_direct_protocol, is_egress_only_protocol,
            is_extra_core_protocol, is_noise_protocol, is_ssh_protocol, is_xray_protocol,
        };
        type Predicate = fn(&str) -> bool;
        let predicates: &[(&str, Predicate, Protocol)] = &[
            ("is_xray_protocol", is_xray_protocol, Protocol::Xray),
            (
                "is_custom_outbound_protocol",
                is_custom_outbound_protocol,
                Protocol::CustomOutbound,
            ),
            (
                "is_custom_config_protocol",
                is_custom_config_protocol,
                Protocol::CustomConfig,
            ),
            (
                "is_extra_core_protocol",
                is_extra_core_protocol,
                Protocol::ExtraCore,
            ),
            ("is_chain_protocol", is_chain_protocol, Protocol::Chain),
            ("is_direct_protocol", is_direct_protocol, Protocol::Direct),
            ("is_awg_protocol", is_awg_protocol, Protocol::Amneziawg),
            ("is_noise_protocol", is_noise_protocol, Protocol::Noise),
            ("is_ssh_protocol", is_ssh_protocol, Protocol::Ssh),
        ];
        for protocol in Protocol::ALL {
            for (name, predicate, wanted) in predicates {
                let spellings: Vec<String> = std::iter::once(protocol.as_str().to_string())
                    .chain(protocol.aliases().iter().map(|a| a.to_string()))
                    .collect();
                for spelling in spellings {
                    assert_eq!(
                        predicate(&spelling),
                        *protocol == *wanted,
                        "{name}({spelling:?}) must equal parsed == {}",
                        wanted.as_str()
                    );
                }
            }
        }
        for protocol in [
            Protocol::Xray,
            Protocol::CustomOutbound,
            Protocol::CustomConfig,
            Protocol::ExtraCore,
        ] {
            assert!(
                is_egress_only_protocol(protocol.as_str()),
                "{} is egress-only",
                protocol.as_str()
            );
        }
        let others = Protocol::ALL
            .iter()
            .filter(|p| !is_egress_only_protocol(p.as_str()))
            .count();
        assert_eq!(
            others,
            Protocol::ALL.len() - 4,
            "exactly four are egress-only"
        );
    }
}
