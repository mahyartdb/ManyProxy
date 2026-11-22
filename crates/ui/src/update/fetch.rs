use crate::*;

#[derive(Debug, Clone)]
pub struct FetchedSubscription {
    pub body: String,
    pub userinfo: Option<store::SubscriptionQuota>,
}

/// T-12: everything one confirmed `.srs` download needs, resolved when the
/// user clicks **Download** on the per-file confirm dialog. The type is
/// constructed only in the confirm handler — staging a download without the
/// on-screen explicit confirm is not expressible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSetDownloadGrant {
    pub name: String,
    pub file_name: String,
    pub url: String,
    pub dir: PathBuf,
}

/// Shared T-08 subscription fetch pipeline: policy gates before any network at
/// every hop, `ManyProxy/<version>` UA, no-follow redirect loop, 20 s timeout,
/// 8 MiB cap. T-12's `.srs` staging fetch routes through this exact pipeline so
/// rule-set downloads get byte-identical transport policy (parity is literal).
pub struct FetchedHttp {
    pub body: Vec<u8>,
    pub userinfo: Option<store::SubscriptionQuota>,
}

pub async fn fetch_http_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<FetchedHttp, String> {
    use futures::AsyncReadExt as _;
    const MAX_BYTES: usize = store::SUBSCRIPTION_MAX_BYTES;
    const TIMEOUT_SECS: u64 = store::SUBSCRIPTION_TIMEOUT_SECS;
    const MAX_REDIRECTS: usize = store::SUBSCRIPTION_MAX_REDIRECTS;
    let mut current = url.trim().to_string();
    for _ in 0..=MAX_REDIRECTS {
        if let store::SubscriptionFetchPolicy::Block { reason } =
            store::subscription_fetch_policy(&current)
        {
            return Err(reason.to_string());
        }
        if let Some(host) = store::subscription_url_host(&current) {
            let resolved = resolve_subscription_addresses(host, resolver).await;
            if let store::SubscriptionFetchPolicy::Block { reason } =
                store::subscription_fetch_policy_resolved(&current, &resolved)
            {
                return Err(reason.to_string());
            }
        }
        let request = http_client::http::Request::builder()
            .uri(current.as_str())
            .header("user-agent", user_agent_header())
            .follow_redirects(http_client::RedirectPolicy::NoFollow)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .body(AsyncBody::empty())
            .map_err(|_| "subscription request failed".to_string())?;
        let mut response = client
            .send(request)
            .await
            .map_err(|_| "subscription fetch failed".to_string())?;
        if is_fetch_redirect(response.status()) {
            match fetch_redirect_location(response.headers(), &current) {
                Some(next) => {
                    current = next;
                    continue;
                }
                None => return Err("subscription fetch failed (bad redirect)".to_string()),
            }
        }
        if !response.status().is_success() {
            return Err(format!(
                "subscription fetch failed (status {})",
                response.status().as_u16()
            ));
        }

        let userinfo = response
            .headers()
            .get("subscription-userinfo")
            .and_then(|value| value.to_str().ok())
            .and_then(store::parse_subscription_userinfo);
        let mut buf = Vec::new();
        let mut chunk = [0u8; 65_536];
        loop {
            let n = response
                .body_mut()
                .read(&mut chunk)
                .await
                .map_err(|_| "subscription fetch failed".to_string())?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_BYTES {
                return Err("subscription body too large".to_string());
            }
        }
        return Ok(FetchedHttp {
            body: buf,
            userinfo,
        });
    }
    Err("subscription fetch failed (too many redirects)".to_string())
}

pub async fn fetch_subscription_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<FetchedSubscription, String> {
    let fetched = fetch_http_body(client, url, resolver).await?;
    String::from_utf8(fetched.body)
        .map(|body| FetchedSubscription {
            body,
            userinfo: fetched.userinfo,
        })
        .map_err(|_| "subscription body is not UTF-8".to_string())
}

/// T-12: the confirmed rule-set download rides the same pipeline; `.srs` is
/// binary, so the body stays bytes (never UTF-8-validated like subscriptions).
pub async fn fetch_rule_set_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<Vec<u8>, String> {
    Ok(fetch_http_body(client, url, resolver).await?.body)
}

pub type SubscriptionHostResolver = fn(&str) -> Result<Vec<std::net::IpAddr>, String>;

pub fn resolve_subscription_host(host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    use std::net::ToSocketAddrs as _;
    (host, 0u16)
        .to_socket_addrs()
        .map(|addrs| addrs.map(|addr| addr.ip()).collect())
        .map_err(|_| "subscription host does not resolve".to_string())
}

pub async fn resolve_subscription_addresses(
    host: String,
    resolver: SubscriptionHostResolver,
) -> Vec<std::net::IpAddr> {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let resolved = resolve_host_within(&host, resolver, SUBSCRIPTION_RESOLVE_TIMEOUT);
        let _ = tx.send(resolved);
    });
    rx.await.unwrap_or_default()
}

/// A lookup that has not answered by now is treated as unresolved, which the
/// fetch policy turns into a refusal; a black-holed resolver must never hold
/// a subscription refresh open.
const SUBSCRIPTION_RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn resolve_host_within(
    host: &str,
    resolver: SubscriptionHostResolver,
    timeout: std::time::Duration,
) -> Vec<std::net::IpAddr> {
    let (tx, rx) = std::sync::mpsc::channel();
    if resolver_worker()
        .send((host.to_string(), resolver, tx))
        .is_err()
    {
        return Vec::new();
    }
    rx.recv_timeout(timeout).unwrap_or_default()
}

type ResolveRequest = (
    String,
    SubscriptionHostResolver,
    std::sync::mpsc::Sender<Vec<std::net::IpAddr>>,
);

/// std's resolver cannot be cancelled, so the thread inside a hung lookup
/// would never exit. One shared worker owns every lookup and the per-call
/// waiter stops at its deadline, so a stall costs no threads.
fn resolver_worker() -> &'static std::sync::mpsc::Sender<ResolveRequest> {
    static WORKER: std::sync::OnceLock<std::sync::mpsc::Sender<ResolveRequest>> =
        std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<ResolveRequest>();
        std::thread::spawn(move || {
            while let Ok((host, resolver, reply)) = rx.recv() {
                let addrs = resolver(&host).unwrap_or_default();
                let _ = reply.send(addrs);
            }
        });
        tx
    })
}

/// Every outbound fetch-layer request carries `User-Agent: ManyProxy/<APP_VERSION>`.
/// A static, per-release identifier with zero device identity: Throne's
/// `x-hwid`/`x-device-*` header family is device fingerprinting, and this
/// function must never gain any hostname/MAC/ID component.
pub fn user_agent_header() -> String {
    format!("ManyProxy/{}", APP_VERSION)
}

pub fn is_fetch_redirect(status: http_client::http::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

pub fn resolve_fetch_redirect(current_url: &str, location: &str) -> Option<String> {
    let base = parse_http_url(current_url.trim())?;
    // HTTP never sends the fragment, and `http::Uri` rejects one on the next
    // request, so the reference is resolved without it.
    let reference = location.trim().split('#').next().unwrap_or("");
    if reference.is_empty() {
        return None;
    }

    if let Some(scheme) = reference_scheme(reference) {
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return None;
        }
        // Keep the server's original spelling for absolute references.
        return parse_http_url(reference).map(|_| reference.to_string());
    }

    if let Some(network_path) = reference.strip_prefix("//") {
        let (authority, path, query) = split_authority(network_path)?;
        let path = remove_dot_segments(path);
        return Some(build_url(base.scheme, authority, &path, query));
    }

    if reference.starts_with('/') {
        let (path, query) = split_path_query(reference);
        let path = remove_dot_segments(path);
        return Some(build_url(base.scheme, base.authority, &path, query));
    }

    if let Some(query) = reference.strip_prefix('?') {
        return Some(build_url(
            base.scheme,
            base.authority,
            base.path,
            Some(query),
        ));
    }

    let (path, query) = split_path_query(reference);
    let base_dir = match base.path.rfind('/') {
        Some(slash) => &base.path[..=slash],
        None => "/",
    };
    let merged = format!("{base_dir}{path}");
    let path = remove_dot_segments(&merged);
    Some(build_url(base.scheme, base.authority, &path, query))
}

struct HttpUrl<'a> {
    scheme: &'a str,
    authority: &'a str,
    path: &'a str,
}

fn parse_http_url(url: &str) -> Option<HttpUrl<'_>> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let rest = rest.split('#').next().unwrap_or("");
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() {
        return None;
    }
    let path = rest[end..].split('?').next().unwrap_or("");
    Some(HttpUrl {
        scheme,
        authority,
        path,
    })
}

fn reference_scheme(reference: &str) -> Option<&str> {
    let scheme = &reference[..reference.find(':')?];
    let mut chars = scheme.chars();
    if !chars.next()?.is_ascii_alphabetic() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    Some(scheme)
}

fn split_authority(value: &str) -> Option<(&str, &str, Option<&str>)> {
    let end = value.find(['/', '?']).unwrap_or(value.len());
    let authority = &value[..end];
    if authority.is_empty() {
        return None;
    }
    let (path, query) = split_path_query(&value[end..]);
    Some((authority, path, query))
}

fn split_path_query(value: &str) -> (&str, Option<&str>) {
    match value.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (value, None),
    }
}

fn build_url(scheme: &str, authority: &str, path: &str, query: Option<&str>) -> String {
    match query {
        Some(query) => format!("{scheme}://{authority}{path}?{query}"),
        None => format!("{scheme}://{authority}{path}"),
    }
}

/// RFC 3986 §5.2.4. Extra empty segments normalize away; percent-encoded
/// dots are not segments and stay literal.
fn remove_dot_segments(path: &str) -> String {
    let absolute = path.starts_with('/');
    let trailing_slash = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let mut resolved = String::new();
    if absolute {
        resolved.push('/');
    }
    resolved.push_str(&segments.join("/"));
    if trailing_slash && !resolved.ends_with('/') {
        resolved.push('/');
    }
    resolved
}

pub fn fetch_redirect_location(
    headers: &http_client::http::HeaderMap,
    current_url: &str,
) -> Option<String> {
    let location = headers
        .get(http_client::http::header::LOCATION)?
        .to_str()
        .ok()?;
    resolve_fetch_redirect(current_url, location)
}

/// A feed URL targets the releases *list* when the path (sans query or
/// fragment) ends in a `releases` segment — GitHub exposes
/// `/repos/<owner>/<repo>/releases` (list) vs `/releases/latest` (single).
pub fn is_releases_list_url(url: &str) -> bool {
    let path = url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    path.rsplit('/').next().unwrap_or("") == "releases"
}

/// Fetches the JSON-only release feed through the subscription fetch policy —
/// the same SSRF guards (https-only allow, plaintext http warns, internal
/// hosts never fetched), redirects followed by hand.
pub async fn fetch_release_feed(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<Option<FetchedRelease>, String> {
    use futures::AsyncReadExt as _;
    const MAX_BYTES: usize = UPDATE_FEED_MAX_BYTES;
    let list_endpoint = is_releases_list_url(url);
    let mut current = url.trim().to_string();
    for _ in 0..=UPDATE_FEED_MAX_REDIRECTS {
        if let store::SubscriptionFetchPolicy::Block { reason } =
            store::subscription_fetch_policy(&current)
        {
            return Err(reason.to_string());
        }
        if let Some(host) = store::subscription_url_host(&current) {
            let resolved = resolve_subscription_addresses(host, resolver).await;
            if let store::SubscriptionFetchPolicy::Block { reason } =
                store::subscription_fetch_policy_resolved(&current, &resolved)
            {
                return Err(reason.to_string());
            }
        }
        let request = http_client::http::Request::builder()
            .uri(current.as_str())
            .header("user-agent", user_agent_header())
            .follow_redirects(http_client::RedirectPolicy::NoFollow)
            .timeout(std::time::Duration::from_secs(UPDATE_FEED_TIMEOUT_SECS))
            .body(AsyncBody::empty())
            .map_err(|_| "update feed request failed".to_string())?;
        let mut response = client
            .send(request)
            .await
            .map_err(|_| "update feed fetch failed".to_string())?;
        if is_fetch_redirect(response.status()) {
            match fetch_redirect_location(response.headers(), &current) {
                Some(next) => {
                    current = next;
                    continue;
                }
                None => return Err("update feed fetch failed (bad redirect)".to_string()),
            }
        }
        if response.status().as_u16() == 404 {
            // No releases published and still-private repos both land here.
            return Err(UPDATE_FEED_EMPTY_MESSAGE.to_string());
        }
        if !response.status().is_success() {
            return Err(format!(
                "update feed fetch failed (status {})",
                response.status().as_u16()
            ));
        }

        let mut buf = Vec::new();
        let mut chunk = [0u8; 65_536];
        loop {
            let n = response
                .body_mut()
                .read(&mut chunk)
                .await
                .map_err(|_| "update feed fetch failed".to_string())?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_BYTES {
                return Err("update feed body too large".to_string());
            }
        }
        let body =
            String::from_utf8(buf).map_err(|_| "update feed body is not UTF-8".to_string())?;
        return parse_release_feed(&body, list_endpoint, APP_VERSION);
    }
    Err("update feed fetch failed (too many redirects)".to_string())
}

#[cfg(test)]
mod tests {
    use super::{resolve_fetch_redirect, resolve_host_within};

    #[test]
    fn redirects_resolve_dot_relative_and_protocol_relative_locations() {
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a/sub.txt", "./latest"),
            Some("https://example.com/a/latest".to_string())
        );
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a/sub.txt", "latest"),
            Some("https://example.com/a/latest".to_string())
        );
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a/b", "../c"),
            Some("https://example.com/c".to_string())
        );
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a", "//cdn.host/file"),
            Some("https://cdn.host/file".to_string())
        );
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a?x=1", "?y=2"),
            Some("https://example.com/a?y=2".to_string())
        );
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a", "/b/d/../c?y=2"),
            Some("https://example.com/b/c?y=2".to_string())
        );
        assert_eq!(
            resolve_fetch_redirect("https://example.com/a", "javascript:alert(1)"),
            None
        );
    }

    #[test]
    fn hanging_resolver_stops_at_the_resolve_deadline() {
        fn hanging(_host: &str) -> Result<Vec<std::net::IpAddr>, String> {
            std::thread::sleep(std::time::Duration::from_millis(300));
            Ok(vec!["93.184.216.34".parse().expect("ip parses")])
        }
        let started = std::time::Instant::now();
        let resolved = resolve_host_within(
            "slow.example.com",
            hanging,
            std::time::Duration::from_millis(50),
        );
        assert!(
            resolved.is_empty(),
            "the deadline abandons the lookup: {resolved:?}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_millis(250),
            "bounded wait, took {:?}",
            started.elapsed()
        );
    }
}
