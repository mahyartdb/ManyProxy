pub fn generate_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now();
        let mut first = DefaultHasher::new();
        ("manyproxy-uuid-v1", std::process::id(), now, n).hash(&mut first);
        let mut second = DefaultHasher::new();
        ("manyproxy-uuid-v2", n, now, std::process::id()).hash(&mut second);
        let (a, b) = (first.finish().to_be_bytes(), second.finish().to_be_bytes());
        bytes[..8].copy_from_slice(&a);
        bytes[8..].copy_from_slice(&b);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let hex = |byte: u8| {
        [
            HEX[(byte >> 4) as usize] as char,
            HEX[(byte & 0x0f) as usize] as char,
        ]
    };
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.extend(hex(*byte));
    }
    out
}

pub fn generate_wireguard_key() -> String {
    use base64::Engine as _;
    let mut bytes = [0u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now();
        for (slot, tag) in bytes.chunks_mut(8).zip([
            "manyproxy-wg-1",
            "manyproxy-wg-2",
            "manyproxy-wg-3",
            "manyproxy-wg-4",
        ]) {
            let mut hasher = DefaultHasher::new();
            (tag, std::process::id(), now, n).hash(&mut hasher);
            slot.copy_from_slice(&hasher.finish().to_be_bytes());
        }
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
