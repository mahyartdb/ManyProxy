use crate::*;

/// Same fetch policy as subscriptions — validated at save time, before any
/// update check ever runs: https-only allow, plaintext http warns, internal
/// hosts are refused outright.
pub(crate) fn normalize_update_feed_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("feed URL must not be empty".to_string());
    }
    match store::subscription_fetch_policy(trimmed) {
        store::SubscriptionFetchPolicy::Allow | store::SubscriptionFetchPolicy::Warn { .. } => {
            Ok(trimmed.to_string())
        }
        store::SubscriptionFetchPolicy::Block { reason } => Err(reason.to_string()),
    }
}

pub const UPDATE_FEED_URL_DEFAULT: &str =
    "https://api.github.com/repos/QMahyar/ManyProxy/releases/latest";

/// T-06: the update check is a passive JSON GET — 10 s timeout, one MiB cap,
/// at most five hand-followed hops. It reads the feed and nothing else;
/// release assets are never fetched, hashed, or executed by the app.
pub(crate) const UPDATE_FEED_TIMEOUT_SECS: u64 = 10;

pub(crate) const UPDATE_FEED_MAX_REDIRECTS: usize = store::SUBSCRIPTION_MAX_REDIRECTS;

pub(crate) const UPDATE_FEED_MAX_BYTES: usize = 1024 * 1024;

pub const UPDATE_FEED_EMPTY_MESSAGE: &str = "No releases published yet.";

/// One release entry read out of the JSON feed. The app keeps parsed
/// metadata only — assets are never touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedRelease {
    pub tag_name: String,
    pub prerelease: bool,
    pub html_url: Option<String>,
    pub release_name: Option<String>,
}

pub(crate) fn json_str_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .map(str::to_string)
}

pub(crate) fn release_entry(value: &serde_json::Value) -> Option<FetchedRelease> {
    if !value.is_object() {
        return None;
    }
    Some(FetchedRelease {
        tag_name: json_str_field(value, "tag_name")?,
        prerelease: value
            .get("prerelease")
            .and_then(|field| field.as_bool())
            .unwrap_or(false),
        html_url: json_str_field(value, "html_url"),
        release_name: json_str_field(value, "name"),
    })
}

/// Parses the feed body. `Ok(None)` means nothing a compare can act on —
/// everything published is prerelease-only while the running build is stable,
/// or no release carries a comparable tag. HTTP 404 stays with the fetch fn.
pub fn parse_release_feed(
    body: &str,
    list_endpoint: bool,
    running: &str,
) -> Result<Option<FetchedRelease>, String> {
    const MALFORMED: &str = "update feed is not valid JSON";
    let value: serde_json::Value =
        serde_json::from_str(body.trim()).map_err(|_| MALFORMED.to_string())?;
    let allow_prerelease = running_is_prerelease(running);
    if list_endpoint {
        let entries = value.as_array().ok_or_else(|| MALFORMED.to_string())?;
        let mut candidates = Vec::new();
        for entry in entries {
            if let Some(release) = release_entry(entry) {
                candidates.push(release);
            }
        }
        Ok(newest_tag_among(&candidates, allow_prerelease))
    } else {
        let feed = release_entry(&value).ok_or_else(|| MALFORMED.to_string())?;
        if feed.prerelease && !allow_prerelease {
            return Ok(None);
        }
        if parse_release_tag(&feed.tag_name).is_none() {
            return Ok(None);
        }
        Ok(Some(feed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckOutcome {
    UpToDate,
    NoReleases,
    Newer(FetchedRelease),
}

/// Darkens the feed result into a dialog outcome: 404 is the exact
/// "No releases published yet." state (not an error class), a filtered
/// release set is up-to-date, and every other failure is a plain error.
pub fn update_check_outcome(
    fetched: Result<Option<FetchedRelease>, String>,
) -> Result<UpdateCheckOutcome, String> {
    match fetched {
        Ok(Some(release)) => {
            let candidate = compare_release_to_running(&release.tag_name, APP_VERSION);
            Ok(match candidate {
                ReleaseCompare::Newer => UpdateCheckOutcome::Newer(release),
                ReleaseCompare::UpToDate | ReleaseCompare::Older => UpdateCheckOutcome::UpToDate,
            })
        }
        Ok(None) => Ok(UpdateCheckOutcome::UpToDate),
        Err(why) if why == UPDATE_FEED_EMPTY_MESSAGE => Ok(UpdateCheckOutcome::NoReleases),
        Err(why) => Err(why),
    }
}

/// (message element id, message text, optional release title, optional https
/// releases page) — the single source the dialog renders from, so tests pin
/// the exact strings without pulling them out of a painted frame.
pub(crate) fn update_check_presentation(
    outcome: &UpdateCheckOutcome,
    running: &str,
) -> (&'static str, String, Option<String>, Option<String>) {
    match outcome {
        UpdateCheckOutcome::UpToDate => (
            "update-up-to-date",
            format!("ManyProxy {running} is up to date."),
            None,
            None,
        ),
        UpdateCheckOutcome::NoReleases => (
            "update-no-releases",
            UPDATE_FEED_EMPTY_MESSAGE.to_string(),
            None,
            None,
        ),
        UpdateCheckOutcome::Newer(release) => {
            let tag = release.tag_name.trim().to_string();
            let name = release
                .release_name
                .clone()
                .filter(|name| !name.trim().is_empty() && name.trim() != tag);
            let url = release
                .html_url
                .clone()
                .filter(|value| value.trim().starts_with("https://"));
            (
                "update-new-version",
                format!("New version available: {tag} (running {running})"),
                name,
                url,
            )
        }
    }
}

/// Stable binaries stay on `releases/latest`; a prerelease build rides the
/// releases list so prerelease tags become visible (the channel rule).
pub fn effective_update_feed_url(override_url: Option<&str>, running: &str) -> String {
    if let Some(url) = override_url.filter(|value| !value.trim().is_empty()) {
        return url.trim().to_string();
    }
    let default = UPDATE_FEED_URL_DEFAULT.trim();
    if running_is_prerelease(running) {
        if let Some(prefix) = default.strip_suffix("/releases/latest") {
            return format!("{prefix}/releases");
        }
    }
    default.to_string()
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    pub(crate) fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        lp_operation: *const u16,
        lp_file: *const u16,
        lp_parameters: *const u16,
        lp_directory: *const u16,
        n_show_cmd: i32,
    ) -> isize;
}

/// Opens the releases page in the user's own browser on their explicit
/// click — https-only, no prefetch, no app-initiated navigation.
pub(crate) fn open_release_in_browser(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return Err("only https release pages can open in a browser".to_string());
    }
    open_url_shell_execute(trimmed)
}

#[cfg(windows)]
pub(crate) fn open_url_shell_execute(url: &str) -> Result<(), String> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt as _;
    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(once(0))
            .collect()
    }
    let operation = wide("open");
    let file = wide(url);
    let instance = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    };
    if instance > 32 {
        Ok(())
    } else {
        Err(format!(
            "cannot open the browser to {url} (shell returned {instance})"
        ))
    }
}

#[cfg(not(windows))]
pub(crate) fn open_url_shell_execute(_url: &str) -> Result<(), String> {
    Err("opening the release page needs the Windows shell (ManyProxy is Windows-only)".to_string())
}
