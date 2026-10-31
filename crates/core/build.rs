//! Stamps the git hash into `consts::GIT_HASH` for the About box.

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = root.join(".git");
    println!("cargo:rerun-if-changed={}", git.join("HEAD").display());
    if let Ok(head) = std::fs::read_to_string(git.join("HEAD")) {
        if let Some(target) = head.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed={}", git.join(target).display());
        }
    }
    println!("cargo:rerun-if-changed={}", git.join("refs/tags").display());
    println!(
        "cargo:rerun-if-changed={}",
        git.join("packed-refs").display()
    );
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "unknown (dev build)".to_string());
    println!("cargo:rustc-env=MANYPROXY_GIT_HASH={hash}");
}
