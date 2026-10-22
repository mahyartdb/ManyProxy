use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupRowView {
    pub(crate) name: String,
    pub(crate) count: usize,
    pub(crate) kind: String,
    pub(crate) quota: Option<String>,
    pub(crate) skip: bool,
    pub(crate) auto_clear: bool,
    pub(crate) sub_ids: Vec<String>,
}

pub(crate) fn group_row_views(
    counts: &[(String, usize)],
    subs: &[ProxySubscription],
    skip: &dyn Fn(&str) -> bool,
    auto_clear: &dyn Fn(&str) -> bool,
    now: i64,
) -> Vec<GroupRowView> {
    use std::collections::BTreeMap;
    let mut counts_map: BTreeMap<&str, usize> = BTreeMap::new();
    for (group, count) in counts {
        counts_map.insert(group.as_str(), *count);
    }
    let mut feeding: BTreeMap<&str, Vec<&ProxySubscription>> = BTreeMap::new();
    for sub in subs {
        feeding.entry(sub.group.as_str()).or_default().push(sub);
    }
    let mut names: Vec<&str> = counts_map.keys().copied().collect();
    for name in feeding.keys() {
        if !counts_map.contains_key(name) {
            names.push(name);
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let feeding = feeding.get(name).cloned().unwrap_or_default();
            let count = counts_map.get(name).copied().unwrap_or(0);
            let kind = if feeding.is_empty() {
                "Basic".to_string()
            } else {
                format!("Subscription ({})", feeding.len())
            };
            let mut quota = group_quota_line(&feeding, now);
            let skipped = skip(name);
            if skipped {
                let marker = "auto-refresh skipped".to_string();
                quota = Some(match quota {
                    Some(line) => format!("{line} · {marker}"),
                    None => marker,
                });
            }
            GroupRowView {
                name: name.to_string(),
                count,
                kind,
                quota,
                skip: skipped,
                auto_clear: auto_clear(name),
                sub_ids: feeding.iter().map(|sub| sub.id.clone()).collect(),
            }
        })
        .collect()
}

pub(crate) fn group_quota_line(subs: &[&ProxySubscription], now: i64) -> Option<String> {
    if subs.is_empty() {
        return None;
    }
    let last = subs.iter().filter_map(|sub| sub.last_refresh_at).max();
    let mut segments = vec![format!("Last update {}", format_last_refresh(last, now))];
    let mut used: i64 = 0;
    let mut used_known = false;
    let mut total: i64 = 0;
    let mut total_known = false;
    let mut expire: Option<i64> = None;
    for sub in subs {
        if let Some(value) = sub.quota_used {
            used = used.saturating_add(value.max(0));
            used_known = true;
        }
        if let Some(value) = sub.quota_total {
            total = total.saturating_add(value.max(0));
            total_known = true;
        }
        expire = match (expire, sub.quota_expire) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
    }
    if used_known {
        segments.push(format!("Used {}", format_bytes(used.max(0) as u64)));
    }
    if total_known {
        segments.push(format!(
            "Remain {}",
            format_bytes(total.saturating_sub(used).max(0) as u64)
        ));
    }
    if let Some(expire) = expire {
        segments.push(format!("Expire {}", format_expire(expire, now)));
    }
    Some(segments.join(" | "))
}
