use crate::*;

/// The passive check runs at launch and waits a week between attempts. The
/// timestamp is the same `last_update_check` meta a manual check writes, so
/// either path resets the other's wait.
pub const PASSIVE_UPDATE_CHECK_INTERVAL_SECS: i64 = 7 * 24 * 60 * 60;

pub fn passive_update_check_due(last_check: Option<i64>, now: i64, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    match last_check {
        None => true,
        // A clock that moved backwards is not an invitation to fetch early.
        Some(last) => now.saturating_sub(last) >= PASSIVE_UPDATE_CHECK_INTERVAL_SECS,
    }
}

/// The gate and the request in one place, with the decision passed in so a test
/// can prove an empty `None` path never reaches the client.
pub async fn fetch_passive_update(
    due: bool,
    client: Arc<dyn HttpClient>,
    url: &str,
    resolver: SubscriptionHostResolver,
) -> Option<Result<Option<FetchedRelease>, String>> {
    if !due {
        return None;
    }
    Some(fetch_release_feed(client, url, resolver).await)
}
