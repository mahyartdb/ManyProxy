/// The latency facet's buckets. Boundaries are inclusive: a 100 ms probe
/// belongs to every bucket from `Ms100` up, matching the labels users read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LatencyBucket {
    #[default]
    Any,
    Ms100,
    Ms300,
    Ms1000,
}

impl LatencyBucket {
    pub const ALL: [LatencyBucket; 4] = [
        LatencyBucket::Any,
        LatencyBucket::Ms100,
        LatencyBucket::Ms300,
        LatencyBucket::Ms1000,
    ];

    pub fn token(self) -> &'static str {
        match self {
            LatencyBucket::Any => "any",
            LatencyBucket::Ms100 => "100",
            LatencyBucket::Ms300 => "300",
            LatencyBucket::Ms1000 => "1000",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LatencyBucket::Any => "Any",
            LatencyBucket::Ms100 => "≤100 ms",
            LatencyBucket::Ms300 => "≤300 ms",
            LatencyBucket::Ms1000 => "≤1000 ms",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|bucket| bucket.token() == token)
    }

    pub fn max_ms(self) -> Option<u64> {
        match self {
            LatencyBucket::Any => None,
            LatencyBucket::Ms100 => Some(100),
            LatencyBucket::Ms300 => Some(300),
            LatencyBucket::Ms1000 => Some(1000),
        }
    }
}

/// The latency facet's full state. The toggle stands on its own: `Any` with
/// unprobed excluded means "has a measurement", so the control is never a
/// no-op the user has to guess about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyFacet {
    pub bucket: LatencyBucket,
    pub include_unprobed: bool,
}

impl Default for LatencyFacet {
    fn default() -> Self {
        Self {
            bucket: LatencyBucket::Any,
            include_unprobed: true,
        }
    }
}

impl LatencyFacet {
    pub fn matches(&self, latency_ms: Option<u64>) -> bool {
        let Some(ms) = latency_ms else {
            return self.include_unprobed;
        };
        self.bucket.max_ms().is_none_or(|max| ms <= max)
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"bucket\":\"{}\",\"include_unprobed\":{}}}",
            self.bucket.token(),
            self.include_unprobed
        )
    }

    /// Tolerant read: a missing or malformed payload is the pre-feature view
    /// (Any + unprobed included). An unknown bucket is treated as malformed
    /// rather than silently hiding rows under a future token this build cannot
    /// evaluate.
    pub fn from_json(raw: &str) -> Self {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw.trim()) else {
            return Self::default();
        };
        let Some(bucket) = map
            .get("bucket")
            .and_then(|value| value.as_str())
            .and_then(LatencyBucket::from_token)
        else {
            return Self::default();
        };
        let include_unprobed = map
            .get("include_unprobed")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        Self {
            bucket,
            include_unprobed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_are_inclusive_and_unprobed_follows_the_toggle() {
        let at_100 = LatencyFacet {
            bucket: LatencyBucket::Ms100,
            include_unprobed: true,
        };
        assert!(
            at_100.matches(Some(100)),
            "the boundary belongs in the bucket"
        );
        assert!(!at_100.matches(Some(101)));
        assert!(
            at_100.matches(None),
            "unprobed included when the toggle is on"
        );

        let probed_only = LatencyFacet {
            include_unprobed: false,
            ..at_100
        };
        assert!(!probed_only.matches(None));
        assert!(probed_only.matches(Some(1)));

        let any = LatencyFacet::default();
        assert!(any.matches(None));
        assert!(any.matches(Some(u64::MAX)));
        assert!(
            !LatencyFacet {
                include_unprobed: false,
                ..any
            }
            .matches(None),
            "excluding unprobed still narrows the Any bucket"
        );
    }

    #[test]
    fn latency_facet_round_trips_and_tolerates_junk() {
        let facet = LatencyFacet {
            bucket: LatencyBucket::Ms300,
            include_unprobed: false,
        };
        assert_eq!(LatencyFacet::from_json(&facet.to_json()), facet);
        assert_eq!(LatencyFacet::from_json("not json"), LatencyFacet::default());
        assert_eq!(
            LatencyFacet::from_json(r#"{"bucket":"999","include_unprobed":false}"#),
            LatencyFacet::default(),
            "an unknown bucket resets the whole facet instead of hiding rows"
        );
    }

    #[test]
    fn every_bucket_has_a_unique_token_and_label() {
        for bucket in LatencyBucket::ALL {
            assert!(LatencyBucket::from_token(bucket.token()).is_some());
            assert!(!bucket.label().is_empty());
        }
    }
}
