use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainLibraryEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) protocol: String,
}

pub(crate) fn chain_pick_choices(library: &[ChainLibraryEntry]) -> Vec<PanelChoice> {
    let mut owned: Vec<(String, String)> = Vec::with_capacity(library.len() + 1);
    owned.push((String::new(), "Select a row…".to_string()));
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

pub(crate) fn chain_slot_kind(protocol: &str) -> ChainHopSlot {
    use crate::engine::{is_chain_protocol, is_extra_core_protocol};
    if protocol.trim().eq_ignore_ascii_case("autoselector") {
        return ChainHopSlot::Autoselector;
    }
    if is_chain_protocol(protocol) {
        ChainHopSlot::Chain
    } else if is_extra_core_protocol(protocol) {
        ChainHopSlot::Extra
    } else {
        ChainHopSlot::Other
    }
}

pub(crate) fn chain_existing_kinds(
    hops: &[serde_json::Value],
    library: &[ChainLibraryEntry],
) -> Vec<ChainHopSlot> {
    hops.iter()
        .map(|node| match node {
            serde_json::Value::String(id) => library
                .iter()
                .find(|entry| entry.id == id.trim())
                .map(|entry| chain_slot_kind(&entry.protocol))
                .unwrap_or(ChainHopSlot::Other),
            serde_json::Value::Object(map) => {
                let marker = map
                    .get("type")
                    .or_else(|| map.get("protocol"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                chain_slot_kind(marker)
            }
            _ => ChainHopSlot::Other,
        })
        .collect()
}

pub(crate) fn chain_hop_label(
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
            format!("{}. {marker} (inline)", index + 1)
        }
        _ => format!("{}. hop", index + 1),
    }
}

pub(crate) struct WgPeerRefs<'a> {
    pub(crate) prefix: &'static str,
    pub(crate) private_key: &'a Entity<InputState>,
    pub(crate) public_key: &'a Entity<InputState>,
    pub(crate) pre_shared_key: &'a Entity<InputState>,
    pub(crate) reserved: &'a Entity<InputState>,
    pub(crate) keepalive: &'a Entity<InputState>,
    pub(crate) local_addr: &'a Entity<InputState>,
    pub(crate) dns: &'a Entity<InputState>,
    pub(crate) mtu: &'a Entity<InputState>,
    pub(crate) workers: &'a Entity<InputState>,
    pub(crate) sys_ifc: &'a Rc<RefCell<bool>>,
}
