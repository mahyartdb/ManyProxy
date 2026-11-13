use crate::*;

pub fn flow_choices() -> Vec<PanelChoice> {
    let default = t!("choice-default-no-flow");
    panel_choices(
        "vless-flow-option",
        &[
            ("", default.as_str()),
            ("xtls-rprx-vision", "xtls-rprx-vision"),
        ],
    )
}

pub fn packet_encoding_choices() -> Vec<PanelChoice> {
    let default = t!("choice-default");
    panel_choices(
        "vless-packet-encoding-option",
        &[
            ("", default.as_str()),
            ("packetaddr", "packetaddr"),
            ("xudp", "xudp"),
        ],
    )
}

pub fn network_choices() -> Vec<PanelChoice> {
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

pub fn fp_choices() -> Vec<PanelChoice> {
    let default = t!("choice-default-no-fingerprint");
    panel_choices(
        "s2-fp-option",
        &[
            ("", default.as_str()),
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

pub fn vmess_security_choices() -> Vec<PanelChoice> {
    use crate::store::VMESS_CIPHERS;
    let auto_default = t!("choice-auto-default");
    let mut items: Vec<(&str, &str)> = Vec::with_capacity(VMESS_CIPHERS.len());
    for cipher in VMESS_CIPHERS {
        let title = if cipher == "auto" {
            auto_default.as_str()
        } else {
            cipher
        };
        items.push((cipher, title));
    }
    panel_choices("vmess-security-option", &items)
}

pub fn ss_method_choices() -> Vec<PanelChoice> {
    use crate::store::SS_METHODS;
    let default = t!("choice-select-method");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for method in SS_METHODS {
        items.push((method, method));
    }
    panel_choices("ss-method-option", &items)
}

pub fn ss_plugin_choices() -> Vec<PanelChoice> {
    let default = t!("choice-no-plugin");
    panel_choices(
        "ss-plugin-option",
        &[
            ("", default.as_str()),
            ("obfs-local", "obfs-local"),
            ("v2ray-plugin", "v2ray-plugin"),
        ],
    )
}

pub fn awg_version_choices() -> Vec<PanelChoice> {
    use crate::store::AWG_VERSIONS;
    let default = t!("choice-select-version");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for version in AWG_VERSIONS {
        items.push((version, version));
    }
    panel_choices("awg-version-option", &items)
}

pub fn hy_auth_type_choices() -> Vec<PanelChoice> {
    use crate::store::HY_AUTH_TYPES;
    let default = t!("choice-default");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for marker in HY_AUTH_TYPES {
        items.push((marker, marker));
    }
    panel_choices("hy-auth-type-option", &items)
}

pub fn tuic_cc_choices() -> Vec<PanelChoice> {
    use crate::store::TUIC_CC_MODES;
    let default = t!("choice-select");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for mode in TUIC_CC_MODES {
        items.push((mode, mode));
    }
    panel_choices("tuic-cc-option", &items)
}

pub fn tuic_relay_choices() -> Vec<PanelChoice> {
    use crate::store::TUIC_RELAY_MODES;
    let default = t!("choice-select");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for mode in TUIC_RELAY_MODES {
        items.push((mode, mode));
    }
    panel_choices("tuic-relay-option", &items)
}

pub fn socks_version_choices() -> Vec<PanelChoice> {
    use crate::store::SOCKS_VERSIONS;
    let default = t!("choice-select-version-5");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for version in SOCKS_VERSIONS {
        items.push((version, version));
    }
    panel_choices("socks-version-option", &items)
}

pub fn socks_network_choices() -> Vec<PanelChoice> {
    panel_choices("socks-network-option", &[("tcp", "tcp"), ("udp", "udp")])
}

pub fn naive_cc_choices() -> Vec<PanelChoice> {
    use crate::store::NAIVE_CC_MODES;
    let default = t!("choice-select");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for mode in NAIVE_CC_MODES {
        items.push((mode, mode));
    }
    panel_choices("naive-cc-option", &items)
}

pub fn ssh_key_pick_error(path: &str) -> String {
    t!("ssh-key-unreadable", path = path)
}

pub fn snell_version_choices() -> Vec<PanelChoice> {
    use crate::store::SNELL_VERSIONS;
    let default = t!("choice-select-version-4");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for version in SNELL_VERSIONS {
        items.push((version, version));
    }
    panel_choices("snell-version-option", &items)
}

pub fn shadowtls_version_choices() -> Vec<PanelChoice> {
    use crate::store::SHADOWTLS_VERSIONS;
    let default = t!("choice-select-version-1");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for version in SHADOWTLS_VERSIONS {
        items.push((version, version));
    }
    panel_choices("shadowtls-version-option", &items)
}

pub fn xray_network_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_NETWORKS;
    let default = t!("choice-select-network");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for network in XRAY_NETWORKS {
        items.push((network, network));
    }
    panel_choices("xray-network-option", &items)
}

pub fn xray_flow_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_FLOWS;
    let default = t!("choice-default-no-flow");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for flow in XRAY_FLOWS {
        items.push((flow, flow));
    }
    panel_choices("xray-flow-option", &items)
}

pub fn xray_mux_choices() -> Vec<PanelChoice> {
    use crate::store::XRAY_MUX_MODES;
    let default = t!("choice-keep-default");
    let mut items: Vec<(&str, &str)> = vec![("", default.as_str())];
    for mode in XRAY_MUX_MODES {
        items.push((mode, mode));
    }
    panel_choices("xray-mux-option", &items)
}
