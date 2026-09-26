//! Ticket 21: embed the Windows exe icon (`assets/icon.ico`, wired through
//! `manyproxy.rc`). Windows-only; other targets skip it (the `compile` call
//! itself is a no-op there).

fn main() {
    println!("cargo:rerun-if-changed=manyproxy.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Ticket 33: stamp the git hash for About (`option_env!` in main.rs).
    // Best-effort only: no git binary / no repo / any failure falls back to
    // "unknown" and never fails the build.
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Ticket 34: `.git/HEAD` only names the ref (its bytes never change on
    // commit), so follow it to the ref file — otherwise the About hash goes
    // stale after the first build. Best-effort; never fails the build.
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(target) = head.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=.git/{target}");
        }
    }
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MANYPROXY_GIT_HASH={hash}");
    embed_resource::compile("manyproxy.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
