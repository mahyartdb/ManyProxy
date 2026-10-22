use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseCompare {
    UpToDate,
    Newer,
    Older,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemverIdent {
    Numeric(u64),
    Alnum(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTag {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub idents: Vec<SemverIdent>,
}

/// Strips the leading `v`, splits `major.minor.patch` from the optional
/// `-prerelease` suffix, and ignores `+build` metadata per semver rule 10.
pub fn parse_release_tag(tag: &str) -> Option<ParsedTag> {
    let trimmed = tag.trim();
    let no_v = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let no_build = no_v.split('+').next().unwrap_or(no_v);
    let (core, suffix) = match no_build.split_once('-') {
        Some((core, suffix)) => (core, suffix),
        None => (no_build, ""),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if suffix.is_empty() {
        if no_build.contains('-') {
            return None;
        }
        return Some(ParsedTag {
            major,
            minor,
            patch,
            idents: Vec::new(),
        });
    }
    let mut idents = Vec::new();
    for ident in suffix.split('.') {
        if ident.is_empty() {
            return None;
        }
        if ident.bytes().all(|b| b.is_ascii_digit()) {
            idents.push(SemverIdent::Numeric(ident.parse::<u64>().ok()?));
        } else {
            idents.push(SemverIdent::Alnum(ident.to_string()));
        }
    }
    Some(ParsedTag {
        major,
        minor,
        patch,
        idents,
    })
}

/// Compares two parsed tags. Equal triples rank the plain release above its
/// own prereleases (rule 10); two prerelease tag sets compare per rule 11
/// (numeric < alphanumeric, larger set outranks the smaller).
pub fn compare_parsed_tags(a: &ParsedTag, b: &ParsedTag) -> std::cmp::Ordering {
    let triple = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if triple != std::cmp::Ordering::Equal {
        return triple;
    }
    match (a.idents.is_empty(), b.idents.is_empty()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => compare_pre_identifiers(&a.idents, &b.idents),
    }
}

pub(crate) fn compare_pre_identifiers(a: &[SemverIdent], b: &[SemverIdent]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = match (left, right) {
            (SemverIdent::Numeric(left), SemverIdent::Numeric(right)) => left.cmp(right),
            (SemverIdent::Numeric(_), SemverIdent::Alnum(_)) => std::cmp::Ordering::Less,
            (SemverIdent::Alnum(_), SemverIdent::Numeric(_)) => std::cmp::Ordering::Greater,
            (SemverIdent::Alnum(left), SemverIdent::Alnum(right)) => left.cmp(right),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

pub fn compare_release_to_running(tag_name: &str, running: &str) -> ReleaseCompare {
    let Some(candidate) = parse_release_tag(tag_name) else {
        return ReleaseCompare::UpToDate;
    };
    let Some(current) = parse_release_tag(running) else {
        return ReleaseCompare::UpToDate;
    };
    match compare_parsed_tags(&candidate, &current) {
        std::cmp::Ordering::Greater => ReleaseCompare::Newer,
        std::cmp::Ordering::Less => ReleaseCompare::Older,
        std::cmp::Ordering::Equal => ReleaseCompare::UpToDate,
    }
}

/// The running binary only sees prerelease tags when it itself carries a
/// prerelease suffix (the brief: `APP_VERSION` rides the channel).
pub(crate) fn running_is_prerelease(running: &str) -> bool {
    running.contains('-')
}

pub(crate) fn newest_tag_among(
    entries: &[FetchedRelease],
    allow_prerelease: bool,
) -> Option<FetchedRelease> {
    let mut best: Option<(ParsedTag, &FetchedRelease)> = None;
    for entry in entries {
        if entry.prerelease && !allow_prerelease {
            continue;
        }
        let Some(parsed) = parse_release_tag(&entry.tag_name) else {
            continue;
        };
        let replace = match &best {
            Some((top, _)) => compare_parsed_tags(&parsed, top) == std::cmp::Ordering::Greater,
            None => true,
        };
        if replace {
            best = Some((parsed, entry));
        }
    }
    best.map(|(_, entry)| entry.clone())
}
