use crate::*;

pub(crate) fn flow_choices() -> Vec<PanelChoice> {
    panel_choices(
        "vless-flow-option",
        &[
            ("", "Default (no flow)"),
            ("xtls-rprx-vision", "xtls-rprx-vision"),
        ],
    )
}

pub(crate) fn packet_encoding_choices() -> Vec<PanelChoice> {
    panel_choices(
        "vless-packet-encoding-option",
        &[
            ("", "Default"),
            ("packetaddr", "packetaddr"),
            ("xudp", "xudp"),
        ],
    )
}

pub(crate) fn network_choices() -> Vec<PanelChoice> {
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

pub(crate) fn fp_choices() -> Vec<PanelChoice> {
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

pub(crate) fn vmess_security_choices() -> Vec<PanelChoice> {
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

pub(crate) fn ss_method_choices() -> Vec<PanelChoice> {
    use crate::store::SS_METHODS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select method…")];
    for method in SS_METHODS {
        items.push((method, method));
    }
    panel_choices("ss-method-option", &items)
}

pub(crate) fn ss_plugin_choices() -> Vec<PanelChoice> {
    panel_choices(
        "ss-plugin-option",
        &[
            ("", "No plugin"),
            ("obfs-local", "obfs-local"),
            ("v2ray-plugin", "v2ray-plugin"),
        ],
    )
}

pub(crate) fn awg_version_choices() -> Vec<PanelChoice> {
    use crate::store::AWG_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version…")];
    for version in AWG_VERSIONS {
        items.push((version, version));
    }
    panel_choices("awg-version-option", &items)
}

pub(crate) fn hy_auth_type_choices() -> Vec<PanelChoice> {
    use crate::store::HY_AUTH_TYPES;
    let mut items: Vec<(&str, &str)> = vec![("", "Default")];
    for marker in HY_AUTH_TYPES {
        items.push((marker, marker));
    }
    panel_choices("hy-auth-type-option", &items)
}

pub(crate) fn tuic_cc_choices() -> Vec<PanelChoice> {
    use crate::store::TUIC_CC_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Select…")];
    for mode in TUIC_CC_MODES {
        items.push((mode, mode));
    }
    panel_choices("tuic-cc-option", &items)
}

pub(crate) fn tuic_relay_choices() -> Vec<PanelChoice> {
    use crate::store::TUIC_RELAY_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Select…")];
    for mode in TUIC_RELAY_MODES {
        items.push((mode, mode));
    }
    panel_choices("tuic-relay-option", &items)
}

pub(crate) fn socks_version_choices() -> Vec<PanelChoice> {
    use crate::store::SOCKS_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version… (blank = 5)")];
    for version in SOCKS_VERSIONS {
        items.push((version, version));
    }
    panel_choices("socks-version-option", &items)
}

pub(crate) fn socks_network_choices() -> Vec<PanelChoice> {
    panel_choices("socks-network-option", &[("tcp", "tcp"), ("udp", "udp")])
}

pub(crate) fn naive_cc_choices() -> Vec<PanelChoice> {
    use crate::store::NAIVE_CC_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Select…")];
    for mode in NAIVE_CC_MODES {
        items.push((mode, mode));
    }
    panel_choices("naive-cc-option", &items)
}

pub(crate) fn ssh_key_pick_error(path: &str) -> String {
    format!("SSH key file unreadable: {path} (check the path and pick again)")
}

pub(crate) fn snell_version_choices() -> Vec<PanelChoice> {
    use crate::store::SNELL_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version… (blank = 4)")];
    for version in SNELL_VERSIONS {
        items.push((version, version));
    }
    panel_choices("snell-version-option", &items)
}

pub(crate) fn shadowtls_version_choices() -> Vec<PanelChoice> {
    use crate::store::SHADOWTLS_VERSIONS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select version… (blank = 1)")];
    for version in SHADOWTLS_VERSIONS {
        items.push((version, version));
    }
    panel_choices("shadowtls-version-option", &items)
}

pub(crate) fn xray_network_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_NETWORKS;
    let mut items: Vec<(&str, &str)> = vec![("", "Select network…")];
    for network in XRAY_NETWORKS {
        items.push((network, network));
    }
    panel_choices("xray-network-option", &items)
}

pub(crate) fn xray_flow_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_FLOWS;
    let mut items: Vec<(&str, &str)> = vec![("", "Default (no flow)")];
    for flow in XRAY_FLOWS {
        items.push((flow, flow));
    }
    panel_choices("xray-flow-option", &items)
}

pub(crate) fn xray_mux_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_MUX_MODES;
    let mut items: Vec<(&str, &str)> = vec![("", "Keep default")];
    for mode in XRAY_MUX_MODES {
        items.push((mode, mode));
    }
    panel_choices("xray-mux-option", &items)
}
