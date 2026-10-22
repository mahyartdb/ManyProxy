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
pub(crate) struct FetchedHttp {
    pub(crate) body: Vec<u8>,
    pub(crate) userinfo: Option<store::SubscriptionQuota>,
}

pub(crate) async fn fetch_http_body(
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

pub(crate) async fn fetch_subscription_body(
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
pub(crate) async fn fetch_rule_set_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Result<Vec<u8>, String> {
    Ok(fetch_http_body(client, url, resolver).await?.body)
}

pub(crate) type SubscriptionHostResolver = fn(&str) -> Result<Vec<std::net::IpAddr>, String>;

pub(crate) fn resolve_subscription_host(host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    use std::net::ToSocketAddrs as _;
    (host, 0u16)
        .to_socket_addrs()
        .map(|addrs| addrs.map(|addr| addr.ip()).collect())
        .map_err(|_| "subscription host does not resolve".to_string())
}

pub(crate) async fn resolve_subscription_addresses(
    host: String,
    resolver: SubscriptionHostResolver,
) -> Vec<std::net::IpAddr> {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(resolver(&host).unwrap_or_default());
    });
    rx.await.unwrap_or_default()
}

/// Every outbound fetch-layer request carries `User-Agent: ManyProxy/<APP_VERSION>`.
/// A static, per-release identifier with zero device identity: Throne's
/// `x-hwid`/`x-device-*` header family is device fingerprinting, and this
/// function must never gain any hostname/MAC/ID component.
pub(crate) fn user_agent_header() -> String {
    format!("ManyProxy/{}", APP_VERSION)
}

pub(crate) fn is_fetch_redirect(status: http_client::http::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

pub(crate) fn resolve_fetch_redirect(current_url: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.is_empty() {
        return None;
    }
    if location.starts_with('/') {
        let scheme_end = current_url.find("://")?;
        let after_scheme = &current_url[scheme_end + 3..];
        let authority_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let origin = &current_url[..scheme_end + 3 + authority_end];
        if origin.ends_with("://") {
            return None;
        }
        return Some(format!("{origin}{location}"));
    }
    if location
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || location
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return Some(location.to_string());
    }
    None
}

pub(crate) fn fetch_redirect_location(
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
pub(crate) fn is_releases_list_url(url: &str) -> bool {
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
