fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");

    println!("cargo:rerun-if-changed=.git/HEAD");

    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(target) = head.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=.git/{target}");
        }
    }
    // `git describe` output changes when tags appear, so watch the tag refs too.
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-changed=.git/packed-refs");
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MANYPROXY_GIT_HASH={hash}");

    let (version, version_source) = resolve_app_version();
    println!("cargo:rustc-env=MANYPROXY_APP_VERSION={version}");
    println!("cargo:warning=ManyProxy app version {version} (from {version_source})");

    let resource = write_version_resource(&version);
    if let Err(error) = embed_resource::compile(&resource, embed_resource::NONE).manifest_optional()
    {
        panic!("ticket 21: cannot embed the generated version resource ({error})");
    }
}

/// Resolve the app version without ever failing the build.
///
/// Order: a tag-shaped `GITHUB_REF_NAME` (GitHub Actions sets it on tag pushes),
/// then the closest reachable git tag, then `CARGO_PKG_VERSION`, then a
/// hardcoded dev fallback. Every probe is best-effort: a missing git binary or a
/// non-repo directory falls through silently to the next source.
fn resolve_app_version() -> (String, &'static str) {
    let ref_is_tag = std::env::var("GITHUB_REF_TYPE")
        .map(|kind| kind.trim() == "tag")
        .unwrap_or(true);
    let ref_name = std::env::var("GITHUB_REF_NAME").ok();
    if ref_is_tag {
        if let Some(version) = ref_name.as_deref().and_then(tag_version) {
            return (version, "GITHUB_REF_NAME");
        }
    }
    if let Some(version) = git_describe_tag().as_deref().and_then(tag_version) {
        return (version, "git describe --tags --abbrev=0");
    }
    if let Some(version) = std::env::var("CARGO_PKG_VERSION")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        return (version, "CARGO_PKG_VERSION");
    }
    ("0.0.0-dev".to_owned(), "hardcoded fallback")
}

fn git_describe_tag() -> Option<String> {
    if !std::path::Path::new(".git").exists() {
        return None;
    }
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tag = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!tag.is_empty()).then_some(tag)
}

/// Accept only tag-shaped strings (`v0.1.2`, `v0.0.0-test.1`) and normalize them
/// to a version string (leading `v` dropped). Branch names and paths fail the
/// shape test, so they fall through to the next version source.
fn tag_version(raw: &str) -> Option<String> {
    let version = raw.trim().strip_prefix('v')?;
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
    {
        return None;
    }
    Some(version.to_owned())
}

/// Four numeric Windows version components. The part before any `-`/`+` suffix
/// splits on `.`; missing or non-numeric components become 0 and values beyond
/// 16 bits clamp, because VERSIONINFO numeric fields are WORDs.
fn version_quad(version: &str) -> [u16; 4] {
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let mut quad = [0u16; 4];
    for (index, component) in core.split('.').take(4).enumerate() {
        let digits: String = component
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        quad[index] = digits
            .parse::<u32>()
            .map(|value| value.min(u32::from(u16::MAX)) as u16)
            .unwrap_or(0);
    }
    quad
}

/// Generate the resource script that gets compiled and linked: the window icon
/// plus a VERSIONINFO block carrying the resolved version. Written into
/// OUT_DIR (rewritten only when the content changes) so the crate tree stays
/// clean and the values can never drift from `MANYPROXY_APP_VERSION`.
fn write_version_resource(version: &str) -> std::path::PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set for build scripts");
    let icon = std::path::Path::new(&manifest_dir)
        .join("assets")
        .join("icon.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let [major, minor, patch, build] = version_quad(version);
    let template = r#"// Generated by build.rs - edit build.rs, not this file.
1 ICON "@ICON@"

1 VERSIONINFO
FILEVERSION @QUAD@
PRODUCTVERSION @QUAD@
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", "ManyProxy"
            VALUE "FileDescription", "ManyProxy - local proxy library"
            VALUE "FileVersion", "@VERSION@"
            VALUE "InternalName", "manyproxy"
            VALUE "OriginalFilename", "manyproxy.exe"
            VALUE "ProductName", "ManyProxy"
            VALUE "ProductVersion", "@VERSION@"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#;
    let contents = template
        .replace("@ICON@", &icon)
        .replace("@QUAD@", &format!("{major},{minor},{patch},{build}"))
        .replace("@VERSION@", version);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set for build scripts");
    let resource = std::path::Path::new(&out_dir).join("manyproxy.rc");
    let unchanged = std::fs::read_to_string(&resource)
        .map(|existing| existing == contents)
        .unwrap_or(false);
    if !unchanged {
        std::fs::write(&resource, contents)
            .expect("ticket 21: OUT_DIR must be writable for the version resource");
    }
    resource
}
