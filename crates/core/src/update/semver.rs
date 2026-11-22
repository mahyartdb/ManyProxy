use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseCompare {
    UpToDate,
    Newer,
    Older,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemverIdent {
    /// Normalized decimal digits with no leading zeros. Arbitrary length, so
    /// numeric order is `(length, lexicographic)`.
    Numeric(String),
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
/// A missing `minor`/`patch` pads with 0; leading zeros in core components are
/// rejected (rule 9).
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
    let major = parse_core_component(parts.next()?)?;
    let minor = match parts.next() {
        Some(raw) => parse_core_component(raw)?,
        None => 0,
    };
    let patch = match parts.next() {
        Some(raw) => parse_core_component(raw)?,
        None => 0,
    };
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
        let digits = ident.bytes().all(|b| b.is_ascii_digit());
        // Rule 9 forbids leading zeros in numeric identifiers. Keeping such a
        // tag parseable as alphanumeric means `01` never equals `1` without
        // discarding the whole release tag.
        let leading_zero = ident.len() > 1 && ident.starts_with('0');
        if digits && !leading_zero {
            idents.push(SemverIdent::Numeric(ident.to_string()));
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

fn parse_core_component(raw: &str) -> Option<u64> {
    if raw.is_empty() || (raw.len() > 1 && raw.starts_with('0')) {
        return None;
    }
    if !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u64>().ok()
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

pub fn compare_pre_identifiers(a: &[SemverIdent], b: &[SemverIdent]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = match (left, right) {
            (SemverIdent::Numeric(left), SemverIdent::Numeric(right)) => {
                left.len().cmp(&right.len()).then_with(|| left.cmp(right))
            }
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

/// The running binary only sees prerelease tags when its own tag carries a
/// prerelease suffix; `+build` metadata containing a dash does not count.
pub fn running_is_prerelease(running: &str) -> bool {
    parse_release_tag(running).is_some_and(|parsed| !parsed.idents.is_empty())
}

pub fn newest_tag_among(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn huge_numeric_prerelease_identifiers_keep_numeric_order() {
        let smaller = parse_release_tag("1.0.0-18446744073709551616")
            .expect("an identifier above u64::MAX still parses");
        let larger = parse_release_tag("1.0.0-18446744073709551617")
            .expect("an identifier above u64::MAX still parses");
        assert_eq!(compare_parsed_tags(&smaller, &larger), Ordering::Less);
        assert_eq!(compare_parsed_tags(&larger, &smaller), Ordering::Greater);
        assert_eq!(compare_parsed_tags(&smaller, &smaller), Ordering::Equal);

        let shorter = parse_release_tag("1.0.0-99999999999999999999999").expect("parses");
        let longer = parse_release_tag("1.0.0-100000000000000000000000").expect("parses");
        assert_eq!(compare_parsed_tags(&shorter, &longer), Ordering::Less);
    }

    #[test]
    fn leading_zeros_reject_core_and_never_equal_a_canonical_prerelease() {
        for tag in ["01.2.3", "1.02.3", "1.2.03", "v00.2.3"] {
            assert!(
                parse_release_tag(tag).is_none(),
                "{tag} has a leading zero in a core component"
            );
        }

        let canonical = parse_release_tag("1.2.3-rc.1").expect("canonical tag parses");
        let zeroed = parse_release_tag("1.2.3-rc.01").expect("the tag stays parseable");
        assert_ne!(compare_parsed_tags(&zeroed, &canonical), Ordering::Equal);
    }

    #[test]
    fn build_metadata_never_marks_the_running_version_prerelease() {
        assert!(!running_is_prerelease("1.2.3"));
        assert!(!running_is_prerelease("1.2.3+build-5"));
        assert!(running_is_prerelease("1.2.3-rc.1+build-5"));
        assert_eq!(
            compare_release_to_running("v1.2.3+build-5", "1.2.3"),
            ReleaseCompare::UpToDate
        );
    }

    #[test]
    fn two_component_tags_pad_the_missing_patch() {
        assert_eq!(
            parse_release_tag("v1.3"),
            Some(ParsedTag {
                major: 1,
                minor: 3,
                patch: 0,
                idents: Vec::new(),
            })
        );
        assert_eq!(
            compare_release_to_running("v1.3", "1.3.0"),
            ReleaseCompare::UpToDate
        );
        assert_eq!(
            compare_release_to_running("v1.4", "1.3.9"),
            ReleaseCompare::Newer
        );
        assert_eq!(
            compare_release_to_running("v1.2", "1.3.0"),
            ReleaseCompare::Older
        );
    }
}
