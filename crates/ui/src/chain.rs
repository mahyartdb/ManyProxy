use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLibraryEntry {
    pub id: String,
    pub name: String,
    pub protocol: String,
    /// The store's verdict on this row as a chain hop, with the refusal text
    /// when it can never be one. Classified once from the full row where the
    /// picker is built, so params-based rules (XHTTP, VLESS encryption) apply.
    pub rule: Result<ChainHopRule, String>,
}

pub fn chain_pick_choices(library: &[ChainLibraryEntry]) -> Vec<PanelChoice> {
    let mut owned: Vec<(String, String)> = Vec::with_capacity(library.len() + 1);
    owned.push((String::new(), t!("chain-hop-select")));
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

/// The rule each hop already in the chain falls under, in order. A reference
/// takes its row's rule from the library; hand-written JSON is classified by
/// the store the same way, so a replace can never leave a refused hop behind.
pub fn chain_existing_rules(
    hops: &[serde_json::Value],
    library: &[ChainLibraryEntry],
) -> Vec<ChainHopRule> {
    hops.iter()
        .map(|node| match node {
            serde_json::Value::String(id) => library
                .iter()
                .find(|entry| entry.id == id.trim())
                .and_then(|entry| entry.rule.clone().ok())
                .unwrap_or(ChainHopRule::NeverAHop),
            serde_json::Value::Object(map) => {
                chain_inline_hop_rule(map).unwrap_or(ChainHopRule::NeverAHop)
            }
            _ => ChainHopRule::NeverAHop,
        })
        .collect()
}

pub fn chain_hop_label(
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
            t!(
                "chain-hop-inline",
                index = (index + 1) as i64,
                marker = marker
            )
        }
        _ => t!("chain-hop-plain", index = (index + 1) as i64),
    }
}

pub struct WgPeerRefs<'a> {
    pub prefix: &'static str,
    pub private_key: &'a Entity<InputState>,
    pub public_key: &'a Entity<InputState>,
    pub pre_shared_key: &'a Entity<InputState>,
    pub reserved: &'a Entity<InputState>,
    pub keepalive: &'a Entity<InputState>,
    pub local_addr: &'a Entity<InputState>,
    pub dns: &'a Entity<InputState>,
    pub mtu: &'a Entity<InputState>,
    pub workers: &'a Entity<InputState>,
    pub sys_ifc: &'a Rc<RefCell<bool>>,
}
