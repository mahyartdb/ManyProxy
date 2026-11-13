//! Pure source health for the nav and the source strip.
//!
//! The store already records the last refresh, its error and the quota. This
//! module turns those fields into the one status word and the one quota line
//! the UI paints, so the nav dot and the strip can never disagree.

use crate::*;

/// The four states the source strip names. The nav dot paints the same state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Fresh,
    Stale,
    Failed,
    Never,
}

/// The read-only snapshot the nav and the strip share. `last_refresh_at` is
/// kept so both can format the relative time without another store read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceHealth {
    pub status: SourceStatus,
    pub last_refresh_at: Option<i64>,
}

impl SourceHealth {
    pub fn from_last_refresh(status: SourceStatus, last_refresh_at: Option<i64>) -> Self {
        Self {
            status,
            last_refresh_at,
        }
    }
}

/// Fresh means refreshed at least once, no error, and still inside the
/// subscription's interval. Failed wins over the clock: a refresh that carried
/// an error is not fresh even when it just happened.
pub fn source_health(subscription: &Subscription, now: i64) -> SourceHealth {
    let status = if subscription.last_error.is_some() {
        SourceStatus::Failed
    } else {
        match subscription.last_refresh_at {
            None => SourceStatus::Never,
            Some(last)
                if subscription.interval_secs > 0
                    && now.saturating_sub(last) < subscription.interval_secs =>
            {
                SourceStatus::Fresh
            }
            Some(_) => SourceStatus::Stale,
        }
    };
    SourceHealth::from_last_refresh(status, subscription.last_refresh_at)
}

/// The quota segment of the strip. `Used X of Y` needs both numbers; a known
/// used count alone still says something. The expiry is the second segment.
pub fn source_quota_line(subscription: &Subscription, now: i64) -> Option<String> {
    let mut segments = Vec::new();
    match (subscription.quota_used, subscription.quota_total) {
        (Some(used), Some(total)) => segments.push(format!(
            "Used {} of {}",
            format_bytes(used.max(0) as u64),
            format_bytes(total.max(0) as u64)
        )),
        (Some(used), None) => segments.push(format!("Used {}", format_bytes(used.max(0) as u64))),
        (None, Some(total)) => {
            segments.push(format!("{} available", format_bytes(total.max(0) as u64)))
        }
        (None, None) => {}
    }
    if let Some(expire) = subscription.quota_expire {
        segments.push(format!("Expires {}", format_expire(expire, now)));
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(last_refresh_at: Option<i64>, interval_secs: i64) -> Subscription {
        Subscription {
            id: "1".to_string(),
            label: "Feed".to_string(),
            url: Secret::new("https://example.com/sub.txt"),
            group: "Home".to_string(),
            auto_refresh: true,
            last_refresh_at,
            last_error: None,
            failures: Vec::new(),
            interval_secs,
            quota_used: None,
            quota_total: None,
            quota_expire: None,
        }
    }

    #[test]
    fn source_health_is_fresh_stale_failed_or_never() {
        let now = 10_000;
        assert_eq!(
            source_health(&subscription(Some(now - 10), 3_600), now).status,
            SourceStatus::Fresh
        );
        assert_eq!(
            source_health(&subscription(Some(now - 7_200), 3_600), now).status,
            SourceStatus::Stale
        );
        assert_eq!(
            source_health(&subscription(None, 3_600), now).status,
            SourceStatus::Never
        );
        let mut failed = subscription(Some(now - 10), 3_600);
        failed.last_error = Some("HTTP 403".to_string());
        assert_eq!(source_health(&failed, now).status, SourceStatus::Failed);
    }

    #[test]
    fn source_health_is_stale_past_the_interval() {
        let now = 10_000;
        assert_eq!(
            source_health(&subscription(Some(now - 3_599), 3_600), now).status,
            SourceStatus::Fresh,
            "one second inside the interval is fresh"
        );
        assert_eq!(
            source_health(&subscription(Some(now - 3_600), 3_600), now).status,
            SourceStatus::Stale,
            "the interval boundary is stale"
        );
        assert_eq!(
            source_health(&subscription(Some(now), 0), now).status,
            SourceStatus::Stale,
            "a zero interval has no fresh window"
        );
    }

    #[test]
    fn source_quota_line_names_used_total_and_expiry() {
        let now = 10_000;
        let mut sub = subscription(Some(now), 3_600);
        sub.quota_used = Some(1_024);
        sub.quota_total = Some(4_096);
        sub.quota_expire = Some(now + 86_400);
        let line = source_quota_line(&sub, now).expect("quota line");
        assert!(line.contains("Used 1.0 KB of 4.0 KB"), "{line}");
        assert!(line.contains("Expires 24h left"), "{line}");

        let mut used_only = subscription(None, 3_600);
        used_only.quota_used = Some(1_024);
        assert_eq!(
            source_quota_line(&used_only, now).as_deref(),
            Some("Used 1.0 KB")
        );
        assert_eq!(
            source_quota_line(&subscription(None, 3_600), now),
            None,
            "no quota, no line"
        );
    }
}
