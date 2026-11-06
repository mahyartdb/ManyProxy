//! Fluent-backed user-facing copy.
//!
//! English is the only shipped locale. The bundle cache, locale negotiation,
//! and missing-key log exist so adding a translation later is `.ftl` authoring
//! rather than another plumbing pass. Tests resolve to `en` so the suite's
//! literal-copy assertions keep their meaning.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

pub(crate) use fluent_bundle::FluentArgs as Args;

/// Reserved store key for a future language picker. Nothing writes it yet;
/// reading it here means the resolution order is already in place.
pub(crate) const LOCALE_META_KEY: &str = "locale";

/// The only locale with an embedded resource. Also the final fallback.
pub(crate) const DEFAULT_LOCALE: &str = "en";

const EN_FTL: &str = include_str!("../../locales/en/main.ftl");

type Bundle = FluentBundle<FluentResource>;

struct Cache {
    bundles: HashMap<String, Bundle>,
    current: String,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| {
        Mutex::new(Cache {
            bundles: HashMap::new(),
            current: DEFAULT_LOCALE.to_string(),
        })
    })
}

/// A poisoned cache would only mean one panicking render; recovering the guard
/// keeps copy rendering instead of cascading the panic into every later frame.
fn lock_cache() -> MutexGuard<'static, Cache> {
    match cache().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn source_for(locale: &str) -> Option<&'static str> {
    match locale {
        "en" => Some(EN_FTL),
        _ => None,
    }
}

/// Reduce a system tag to one we can serve. `en-US` lands on `en`; a locale
/// with no resource falls all the way back to English.
fn negotiate(tag: &str) -> String {
    let normalized = normalize(tag);
    if source_for(&normalized).is_some() {
        return normalized;
    }
    let primary = normalized
        .split('-')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(DEFAULT_LOCALE);
    if source_for(primary).is_some() {
        return primary.to_string();
    }
    DEFAULT_LOCALE.to_string()
}

fn normalize(tag: &str) -> String {
    let tag = tag.trim().replace('_', "-");
    match tag.parse::<LanguageIdentifier>() {
        Ok(id) => id.to_string(),
        Err(_) => DEFAULT_LOCALE.to_string(),
    }
}

/// Store override first, then the OS locale, then English. Kept pure so the
/// precedence is testable without touching the process-wide current locale.
fn resolve_locale_with(stored: Option<&str>, system: Option<&str>) -> String {
    let candidate = stored
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .or_else(|| system.map(str::trim).filter(|tag| !tag.is_empty()))
        .unwrap_or(DEFAULT_LOCALE);
    negotiate(candidate)
}

/// Tests force English: the suite asserts literal English copy, and a machine
/// whose OS locale is not English must not change those assertions.
pub(crate) fn resolve_locale(stored: Option<&str>) -> String {
    #[cfg(test)]
    {
        let _ = stored;
        DEFAULT_LOCALE.to_string()
    }
    #[cfg(not(test))]
    {
        resolve_locale_with(stored, sys_locale::get_locale().as_deref())
    }
}

pub(crate) fn set_locale(tag: &str) {
    let negotiated = negotiate(tag);
    lock_cache().current = negotiated;
}

#[cfg(test)]
pub(crate) fn current_locale() -> String {
    lock_cache().current.clone()
}

impl Cache {
    fn bundle(&mut self, locale: &str) -> Option<&Bundle> {
        if !self.bundles.contains_key(locale) {
            let source = source_for(locale)?;
            let langid: LanguageIdentifier = locale.parse().ok()?;
            let resource = match FluentResource::try_new(source.to_string()) {
                Ok(resource) => resource,
                Err((_, errors)) => {
                    if log_once(&format!("parse:{locale}")) {
                        eprintln!("i18n: failed to parse {locale}: {errors:?}");
                    }
                    return None;
                }
            };
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            // Fluent wraps placeholders in bidi isolates by default. English
            // copy must stay byte-identical, so isolation is off.
            bundle.set_use_isolating(false);
            bundle.add_resource(resource).ok()?;
            self.bundles.insert(locale.to_string(), bundle);
        }
        self.bundles.get(locale)
    }

    fn format(&mut self, locale: &str, key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
        let bundle = self.bundle(locale)?;
        let message = bundle.get_message(key)?;
        let pattern = message.value()?;
        let mut errors = Vec::new();
        let text = bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned();
        if !errors.is_empty() && log_once(&format!("format:{locale}:{key}")) {
            eprintln!("i18n: failed to format {locale}:{key}: {errors:?}");
        }
        Some(text)
    }
}

/// Log a missing key once. A missing translation should be visible in dev but
/// must not flood a repainting window.
fn log_once(tag: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    match seen.lock() {
        Ok(mut seen) => seen.insert(tag.to_string()),
        Err(poisoned) => poisoned.into_inner().insert(tag.to_string()),
    }
}

pub(crate) fn translate(key: &str, args: Option<&FluentArgs<'_>>) -> String {
    let mut cache = lock_cache();
    let current = cache.current.clone();
    if let Some(text) = cache.format(&current, key, args) {
        return text;
    }
    if current != DEFAULT_LOCALE {
        if let Some(text) = cache.format(DEFAULT_LOCALE, key, args) {
            if log_once(&format!("missing:{current}:{key}")) {
                eprintln!("i18n: no {current} copy for {key}; using en");
            }
            return text;
        }
    }
    if log_once(&format!("missing:{DEFAULT_LOCALE}:{key}")) {
        eprintln!("i18n: no copy for {key}");
    }
    key.to_string()
}

/// `t!("key")` or `t!("key", name = value, count = rows.len())`.
///
/// Values become Fluent arguments, so a sentence with a row name is one
/// message rather than concatenated fragments.
macro_rules! t {
    ($key:literal $(,)?) => {
        $crate::i18n::translate($key, None)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::i18n::Args::new();
        $( args.set(stringify!($name), $value); )+
        $crate::i18n::translate($key, Some(&args))
    }};
}

pub(crate) use t;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_copy_is_byte_identical_and_args_interpolate() {
        set_locale("en");
        assert_eq!(translate("app-title", None), "ManyProxy");

        let mut args = FluentArgs::new();
        args.set("folder", "C:\\data");
        assert_eq!(
            translate("write-banner-cannot-write", Some(&args)),
            "Changes will not be saved — cannot write to the data folder (C:\\data)."
        );
        assert!(!translate("write-banner-cannot-write", Some(&args)).contains('\u{2068}'));
    }

    #[test]
    fn missing_keys_fall_back_to_the_key_and_log_once() {
        set_locale("en");
        let key = "test.definitely-missing";
        assert_eq!(translate(key, None), key);
        assert!(!log_once(&format!("missing:{DEFAULT_LOCALE}:{key}")));
    }

    #[test]
    fn resolution_prefers_store_then_system_then_english() {
        assert_eq!(resolve_locale_with(Some("en"), Some("fa-IR")), "en");
        assert_eq!(resolve_locale_with(Some("  "), Some("en-GB")), "en");
        assert_eq!(resolve_locale_with(None, Some("fa-IR")), "en");
        assert_eq!(resolve_locale_with(None, None), "en");
        assert_eq!(resolve_locale_with(Some("not a locale"), None), "en");
    }

    #[test]
    fn unsupported_locales_render_english() {
        set_locale("fa-IR");
        assert_eq!(translate("app-title", None), "ManyProxy");
        set_locale("en");
        assert_eq!(current_locale(), "en");
    }
}
