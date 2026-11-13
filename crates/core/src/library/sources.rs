//! Pure source health for the nav and the source strip.
//!
//! The store already records the last refresh, its error and the quota. This
//! module turns those fields into the one status word the nav dot paints and
//! the quota parts the strip formats, so the two surfaces cannot disagree.

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

/// The quota numbers a source reported. The strip turns these into localized
/// `Used X of Y` and `Expires …` segments; the core keeps the raw parts so it
/// never owns UI copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceQuotaLine {
    pub used: Option<i64>,
    pub total: Option<i64>,
    pub expire_at: Option<i64>,
}

pub fn source_quota_line(subscription: &Subscription) -> Option<SourceQuotaLine> {
    if subscription.quota_used.is_none()
        && subscription.quota_total.is_none()
        && subscription.quota_expire.is_none()
    {
        return None;
    }
    Some(SourceQuotaLine {
        used: subscription.quota_used,
        total: subscription.quota_total,
        expire_at: subscription.quota_expire,
    })
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
    fn source_quota_line_keeps_raw_parts_and_skips_silent_sources() {
        let mut sub = subscription(Some(10_000), 3_600);
        assert_eq!(source_quota_line(&sub), None, "no quota, no line");

        sub.quota_used = Some(1_024);
        sub.quota_total = Some(4_096);
        sub.quota_expire = Some(96_400);
        assert_eq!(
            source_quota_line(&sub),
            Some(SourceQuotaLine {
                used: Some(1_024),
                total: Some(4_096),
                expire_at: Some(96_400),
            })
        );

        let mut expire_only = subscription(None, 3_600);
        expire_only.quota_expire = Some(96_400);
        assert_eq!(
            source_quota_line(&expire_only),
            Some(SourceQuotaLine {
                used: None,
                total: None,
                expire_at: Some(96_400),
            })
        );
    }
}
