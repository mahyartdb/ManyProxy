//! Ticket 21: embed the Windows exe icon (`assets/icon.ico`, wired through
//! `manyproxy.rc`). Windows-only; other targets skip it (the `compile` call
//! itself is a no-op there).

fn main() {
    println!("cargo:rerun-if-changed=manyproxy.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("manyproxy.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
