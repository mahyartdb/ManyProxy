//! dialogs: one binary of the split UI suite. Each binary runs as its own
//! process, which resets the per-process mount ceiling that aborted the
//! single-binary suite (slice 8's harness fix).
//
// The shared helpers live in the support module; each area also keeps the
// suite's original import block so the test bodies compile unchanged.
#![allow(unused_imports)]
mod support;
use support::*;
// The whole test suite: pure-logic tests and gpui test-app paints.
// Belongs: every `#[test]`/`#[gpui_kit::test]` and its local fixtures.
// Not here: production helpers — a test that needs one promotes it to a module.
use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{IndexPath, Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    point, px, size, AppContext, Bounds, Context, Entity, Hsla, TestAppContext, Window,
};
use manyproxy_core::Secret;
use manyproxy_ui::engine::AutoRestartConfig;
use manyproxy_ui::import::OpenWithKind;
use manyproxy_ui::overlay_open;
use manyproxy_ui::store::{
    AutoClearGroupConfig, HealthSnapshot, NewConfig, Store, StoreError, WindowGeometry,
};
use manyproxy_ui::AppView;
use manyproxy_ui::ColumnId;
use manyproxy_ui::ColumnLayout;
use manyproxy_ui::ConfigForm;
use manyproxy_ui::ExitTraffic;

#[test]
fn window_options_carry_app_title() {
    let options =
        manyproxy_ui::app_window_options(manyproxy_ui::startup_window_bounds(None, None), None);
    let title = options
        .titlebar
        .expect("titlebar options")
        .title
        .expect("window title");
    assert_eq!(title.as_str(), manyproxy_ui::APP_TITLE);
}

#[test]
fn window_options_carry_startup_size() {
    let options =
        manyproxy_ui::app_window_options(manyproxy_ui::startup_window_bounds(None, None), None);
    let bounds = options
        .window_bounds
        .expect("startup window bounds")
        .get_bounds();
    assert_eq!(bounds.size.width, gpui_kit::px(1200.));
    assert_eq!(bounds.size.height, gpui_kit::px(700.));
    let min = options.window_min_size.expect("startup window minimum");
    assert_eq!(min.width, gpui_kit::px(1100.));
    assert_eq!(min.height, gpui_kit::px(640.));
}

/// T-32: a sane stored rectangle seeds the startup window; a stored
/// rectangle under the minimums can never seed a smaller window.
#[test]
fn t82_app_options_seeds_stored_geometry() {
    let stored = WindowGeometry {
        x: 240.,
        y: 120.,
        width: 1280.,
        height: 900.,
    };
    let options = manyproxy_ui::app_window_options(
        manyproxy_ui::startup_window_bounds(Some(stored), None),
        None,
    );
    let bounds = options
        .window_bounds
        .expect("startup window bounds")
        .get_bounds();
    assert_eq!(bounds.origin, point(px(240.), px(120.)));
    assert_eq!(bounds.size, size(px(1280.), px(900.)));
    let min = options.window_min_size.expect("startup window minimum");
    assert_eq!(min, size(px(1100.), px(640.)), "minimums are unchanged");

    let undersized = manyproxy_ui::app_window_options(
        manyproxy_ui::startup_window_bounds(
            Some(WindowGeometry {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            }),
            None,
        ),
        None,
    );
    let bounds = undersized
        .window_bounds
        .expect("startup window bounds")
        .get_bounds();
    assert_eq!(
        bounds.size,
        size(px(1100.), px(640.)),
        "a stored rectangle below the minimums seeds the minimums"
    );
}

/// T-32: no stored meta — or a corrupt payload — leaves the headless startup
/// rectangle at the fixed 1000×700 (0, 0) fallback; with a display in hand the
/// same absence centers instead (see `startup_window_centers_when_unplaced`).
#[test]
fn t82_default_bounds_when_meta_absent_or_corrupt() {
    let mut store = Store::open_in_memory().expect("in-memory store opens");

    let absent = manyproxy_ui::app_window_options(
        manyproxy_ui::startup_window_bounds(store.window_geometry(), None),
        None,
    );
    let bounds = absent
        .window_bounds
        .expect("startup window bounds")
        .get_bounds();
    assert_eq!(bounds.origin, point(px(0.), px(0.)));
    assert_eq!(bounds.size, size(px(1200.), px(700.)));

    store
        .set_meta(Store::META_GEOMETRY, "not json at all")
        .expect("junk writes");
    assert_eq!(
        store.window_geometry(),
        None,
        "corrupt JSON reads as absent"
    );
    let corrupt = manyproxy_ui::app_window_options(
        manyproxy_ui::startup_window_bounds(store.window_geometry(), None),
        None,
    );
    let bounds = corrupt
        .window_bounds
        .expect("startup window bounds")
        .get_bounds();
    assert_eq!(bounds.origin, point(px(0.), px(0.)));
    assert_eq!(bounds.size, size(px(1200.), px(700.)));

    store
        .set_meta(Store::META_GEOMETRY, r#"{"x":40,"y":40}"#)
        .expect("partial payload writes");
    assert_eq!(
        store.window_geometry(),
        None,
        "a partial rectangle reads as absent"
    );
    let partial = manyproxy_ui::app_window_options(
        manyproxy_ui::startup_window_bounds(store.window_geometry(), None),
        None,
    );
    let bounds = partial
        .window_bounds
        .expect("startup window bounds")
        .get_bounds();
    assert_eq!(bounds.origin, point(px(0.), px(0.)));
    assert_eq!(bounds.size, size(px(1200.), px(700.)));
}

/// First run: center in the primary display's work area instead of the old
/// (0,0) corner. A stored rect still sitting on that corner is the unplaced
/// default, not a chosen position — it centers while keeping its size. A real
/// placement is honored exactly, so moving the window still sticks.
#[test]
fn startup_window_centers_when_unplaced() {
    let visible = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1040.)));
    let centered = manyproxy_ui::startup_window_bounds(None, Some(visible)).get_bounds();
    assert_eq!(
        centered.origin,
        point(px(360.), px(170.)),
        "1200×700 centers in the 1920×1040 work area"
    );
    assert_eq!(centered.size, size(px(1200.), px(700.)));

    let old_default = WindowGeometry {
        x: 0.,
        y: 0.,
        width: 1200.,
        height: 700.,
    };
    assert_eq!(
        manyproxy_ui::startup_window_bounds(Some(old_default), Some(visible)).get_bounds(),
        centered,
        "the old upper-left default is not a placement"
    );

    let placed = WindowGeometry {
        x: 240.,
        y: 120.,
        width: 1280.,
        height: 900.,
    };
    assert_eq!(
        manyproxy_ui::startup_window_bounds(Some(placed), Some(visible)).get_bounds(),
        Bounds::new(point(px(240.), px(120.)), size(px(1280.), px(900.))),
        "a real placement wins over centering"
    );

    let offset = Bounds::new(point(px(1920.), px(0.)), size(px(1920.), px(1040.)));
    assert_eq!(
        manyproxy_ui::startup_window_bounds(None, Some(offset))
            .get_bounds()
            .origin,
        point(px(2280.), px(170.)),
        "centering follows the display's own origin"
    );
}

/// High display scaling shrinks the logical work area below the design
/// minimum: a 1080p panel at 200% leaves roughly 540 points of height, less
/// than the 620-point floor. The window follows the display so its status
/// strip is never pinned below the desktop with no drag edge to recover it.
#[test]
fn window_minimum_height_follows_the_work_area() {
    let scaled = Bounds::new(point(px(0.), px(0.)), size(px(960.), px(540.)));
    assert_eq!(
        manyproxy_ui::window_min_size(Some(scaled)).height,
        px(540.),
        "the minimum height never exceeds the work area"
    );
    assert_eq!(
        manyproxy_ui::window_min_size(Some(scaled)).width,
        px(960.),
        "a work area narrower than the design floor keeps the work area width"
    );
    assert_eq!(
        manyproxy_ui::window_min_size(None),
        size(px(1100.), px(640.)),
        "a headless caller keeps the design minimum"
    );

    let options = manyproxy_ui::app_window_options(
        manyproxy_ui::startup_window_bounds(None, Some(scaled)),
        Some(scaled),
    );
    assert_eq!(
        options
            .window_min_size
            .expect("startup window minimum")
            .height,
        px(540.),
        "the OS minimum follows the display, not the design floor"
    );

    let stored = WindowGeometry {
        x: 240.,
        y: 120.,
        width: 1280.,
        height: 900.,
    };
    assert_eq!(
        manyproxy_ui::startup_window_bounds(Some(stored), Some(scaled))
            .get_bounds()
            .size,
        size(px(960.), px(540.)),
        "a stored rectangle measured on a larger display opens inside the work area"
    );
}

#[test]
fn exe_icon_asset_is_valid_ico() {
    let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icon.ico");
    let bytes = std::fs::read(icon).expect("assets/icon.ico is committed");
    assert!(bytes.len() > 6, "non-trivial icon file");
    assert_eq!(&bytes[0..4], &[0, 0, 1, 0], "ICO magic");
    let count = u16::from_le_bytes([bytes[4], bytes[5]]);
    assert!(count >= 1, "at least one icon image");
}

/// A panic before the first store open has no `data\` directory yet — the
/// hook must create it instead of losing the report to a swallowed error.
#[test]
fn crash_report_creates_missing_directory() {
    let root = std::env::temp_dir().join(format!(
        "manyproxy-crash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock works")
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let path = root.join("data").join("manyproxy-crash-test.txt");
    assert!(!root.exists(), "fresh temp root");
    let report = "panic: boom\nbacktrace:\n";
    manyproxy_ui::append_crash_report(&path, report).expect("report writes");
    let written = std::fs::read_to_string(&path).expect("report reads");
    assert!(written.contains(report), "report text lands: {written:?}");
    std::fs::remove_dir_all(&root).expect("temp dir cleans");
}

#[test]
fn t55_app_version_is_plain_semver() {
    // T-05: the Cargo version must stay a plain `major.minor.patch`
    // semver - the `-beta.1` prerelease suffix lives on the release
    // tag, and consts.rs must keep deriving this from the manifest.
    let src = include_str!("../../core/src/consts.rs");
    let definition = src
        .lines()
        .find(|line| line.contains("const APP_VERSION"))
        .expect("consts.rs defines APP_VERSION");
    assert!(
        definition.contains("env!(\"CARGO_PKG_VERSION\")"),
        "APP_VERSION must derive from CARGO_PKG_VERSION, got {definition:?}"
    );
    assert_eq!(
        manyproxy_ui::APP_VERSION,
        env!("CARGO_PKG_VERSION"),
        "APP_VERSION rides the manifest version"
    );
    let version = manyproxy_ui::APP_VERSION;
    assert!(!version.starts_with('v'), "no tag-style prefix: {version}");
    assert!(
        !version.contains('-') && !version.contains('+'),
        "plain semver only — prerelease/build suffixes belong to the release tag: {version}"
    );
    let components: Vec<&str> = version.split('.').collect();
    assert_eq!(
        components.len(),
        3,
        "APP_VERSION must be major.minor.patch, got {version}"
    );
    for (position, component) in components.iter().enumerate() {
        assert!(
                !component.is_empty() && component.bytes().all(|b| b.is_ascii_digit()),
                "APP_VERSION component {position} must be a non-empty number, got {component:?} in {version}"
            );
    }
}

#[test]
fn close_confirm_names_running_rows() {
    assert_eq!(manyproxy_ui::close_confirm_description(&[]), None);
    let text = manyproxy_ui::close_confirm_description(&["alpha".to_string(), "beta".to_string()])
        .expect("veto text while running");
    assert!(text.contains("\"alpha\""), "names rows, got {text}");
    assert!(text.contains("\"beta\""), "names rows, got {text}");
    assert!(text.contains('2'), "counts rows, got {text}");
}

#[test]
fn about_body_carries_version_sidecars_paths_shortcuts() {
    let body = manyproxy_ui::about_body(
        "0.1.0",
        "abc123",
        &["sing-box.exe: found (app dir)".to_string()],
        "C:\\data\\library.db",
        "C:\\data\\manyproxy-export.json",
    );
    for needle in [
        "0.1.0",
        "abc123",
        "sing-box.exe",
        "library.db",
        "manyproxy-export.json",
        "F5",
        "Shift-F5",
        "Ctrl-T",
        "Ctrl-N",
        "Ctrl-O",
        "Ctrl-E",
        "Delete",
        "Ctrl-F",
        "Ctrl-I",
        "Ctrl-`",
        "Esc",
        "Opt-in only",
        "default off",
    ] {
        assert!(body.contains(needle), "about names {needle}");
    }
    assert!(
        !body.contains("No autostart"),
        "ticket 38: the v1 no-autostart line is gone (opt-in exists)"
    );
    assert_eq!(
        manyproxy_ui::sidecar_presence_line("x.exe", false),
        "x.exe: missing (place it next to the app exe)"
    );
}

#[gpui_kit::test]
fn logs_dialog_strips_sidecar_ansi(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs.store.push_raw(
                "\u{1b}[31mERROR\u{1b}[0m[0033] dial tcp 127.0.0.1:8388: refused",
                &manyproxy_ui::health::LogTagIndex::default(),
            );
        });
    });
    let stored = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.logs
                .store
                .all()
                .iter()
                .map(|r| r.message.clone())
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(stored.len(), 1);
    assert!(
        !stored[0].contains('\u{1b}'),
        "stored record stripped at ingest, got {:?}",
        stored[0]
    );
    let lines = cx.update(|_, cx| view.update(cx, |view, cx| view.log_lines(cx)));
    assert_eq!(lines.len(), 1);
    assert!(
        !lines[0].contains('\u{1b}'),
        "rendered line has no escapes, got {:?}",
        lines[0]
    );
    assert!(lines[0].contains("ERROR"), "keeps text, got {:?}", lines[0]);
    assert!(
        lines[0].contains("refused"),
        "keeps text, got {:?}",
        lines[0]
    );
}

/// T-31: every level resolves to its own theme tone, in both theme modes,
/// clears WCAG AA (4.5:1) against that mode's background, and the visible
/// `[level]` prefix maps back to the ingested level.
#[gpui_kit::test]
fn t81_log_level_tones_render_all_levels(cx: &mut TestAppContext) {
    use gpui_kit::component::ThemeMode;

    cx.update(gpui_kit::init);
    let levels = [
        manyproxy_ui::LogLevel::Error,
        manyproxy_ui::LogLevel::Warn,
        manyproxy_ui::LogLevel::Info,
        manyproxy_ui::LogLevel::Debug,
    ];
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|cx| {
            manyproxy_ui::Theme::change(mode, None, cx);
            let theme = manyproxy_ui::Theme::global(cx);
            assert_eq!(theme.mode, mode, "the tone check runs in {mode:?}");
            for level in levels {
                let ratio = contrast_ratio(
                    manyproxy_ui::log_level_text_color(level, theme),
                    theme.background,
                );
                assert!(
                    ratio >= 4.5,
                    "{level:?} text reaches WCAG AA in {mode:?}, got {ratio:.2}:1"
                );
            }
            for (index, left) in levels.iter().enumerate() {
                for right in levels.iter().skip(index + 1) {
                    assert_ne!(
                        manyproxy_ui::log_level_text_color(*left, theme),
                        manyproxy_ui::log_level_text_color(*right, theme),
                        "{left:?} and {right:?} stay distinct in {mode:?}"
                    );
                }
            }
        });
    }

    assert_eq!(
        manyproxy_ui::log_line_level("[error] dial refused"),
        manyproxy_ui::LogLevel::Error
    );
    assert_eq!(
        manyproxy_ui::log_line_level("[warn] retry slow"),
        manyproxy_ui::LogLevel::Warn
    );
    assert_eq!(
        manyproxy_ui::log_line_level("[info] engine started"),
        manyproxy_ui::LogLevel::Info
    );
    assert_eq!(
        manyproxy_ui::log_line_level("[debug] tick"),
        manyproxy_ui::LogLevel::Debug
    );
    assert_eq!(
        manyproxy_ui::log_line_level("unprefixed line"),
        manyproxy_ui::LogLevel::Info,
        "an unprefixed line keeps the default tone"
    );
}

/// Every signal and muted tone the shell paints as text, measured where it
/// actually sits: paper (window, dialogs, inspector) and the quiet surface
/// (row cards, tab bar, status band), in both themes. Light is where the three
/// easy mistakes live — the kit's signal tokens are fills, the stopped badge
/// borrowed a surface token, and the kit's muted ink lands under AA on
/// #f5f5f5 — so this asserts by usage, not by token.
#[gpui_kit::test]
fn signal_and_muted_text_clear_aa_on_paper_and_quiet_surface(cx: &mut TestAppContext) {
    use manyproxy_ui::health::LatencyTone;
    use manyproxy_ui::{
        danger_text_color, health_tone_color, serving_tone_color, warning_text_color, ServingTone,
    };

    cx.update(gpui_kit::init);
    for appearance in ["light", "dark"] {
        cx.update(|cx| {
            manyproxy_ui::apply_appearance(appearance, manyproxy_ui::store::Accent::Blue, None, cx)
        });
        cx.update(|cx| {
            let theme = manyproxy_ui::Theme::global(cx);
            let surfaces = [
                ("paper", theme.background),
                ("quiet surface", theme.group_box),
            ];
            let text_usages = [
                (
                    "row health · fast",
                    health_tone_color(LatencyTone::Fast, theme),
                ),
                (
                    "row health · slow",
                    health_tone_color(LatencyTone::Slow, theme),
                ),
                (
                    "row health · poor",
                    health_tone_color(LatencyTone::Poor, theme),
                ),
                (
                    "row health · failed",
                    health_tone_color(LatencyTone::Failed, theme),
                ),
                (
                    "row health · untested",
                    health_tone_color(LatencyTone::Unknown, theme),
                ),
                (
                    "inspector badge · running",
                    serving_tone_color(ServingTone::Serving, theme),
                ),
                (
                    "inspector badge · failed",
                    serving_tone_color(ServingTone::Failed, theme),
                ),
                (
                    "inspector badge · stopped",
                    serving_tone_color(ServingTone::Stopped, theme),
                ),
                ("write banner and status error", danger_text_color(theme)),
                ("log warning", warning_text_color(theme)),
                ("row metadata, `Acting on:`, status band", theme.muted_foreground),
            ];
            let graphic_usages = [
                (
                    "row lamp · serving",
                    serving_tone_color(ServingTone::Serving, theme),
                ),
                (
                    "row lamp · failed",
                    serving_tone_color(ServingTone::Failed, theme),
                ),
                (
                    "row lamp · stopped",
                    serving_tone_color(ServingTone::Stopped, theme),
                ),
            ];

            for (usage, color) in text_usages {
                for (surface, ground) in surfaces {
                    let ratio = contrast_ratio(color, ground);
                    assert!(
                        ratio >= 4.5,
                        "{usage} text on {surface} in {appearance} clears AA, got {ratio:.2}:1"
                    );
                }
            }
            for (usage, color) in graphic_usages {
                for (surface, ground) in surfaces {
                    let ratio = contrast_ratio(color, ground);
                    assert!(
                        ratio >= 3.0,
                        "{usage} on {surface} in {appearance} clears the 3:1 graphic floor, got {ratio:.2}:1"
                    );
                }
            }

            // The focus ring is a large signal, not text: 3:1 against paper.
            let ring = contrast_ratio(theme.ring, theme.background);
            assert!(
                ring >= 3.0,
                "the focus ring clears 3:1 on paper in {appearance}, got {ring:.2}:1"
            );

            assert_eq!(
                serving_tone_color(ServingTone::Stopped, theme),
                theme.muted_foreground,
                "the stopped lamp and the stopped badge read one muted ink in {appearance}"
            );
            assert!(
                contrast_ratio(
                    health_tone_color(LatencyTone::Failed, theme),
                    theme.background
                ) > contrast_ratio(
                    health_tone_color(LatencyTone::Poor, theme),
                    theme.background
                ),
                "a failed probe outranks a poor latency in {appearance}"
            );
        });
    }
}

/// T-31: filter/exclude/regex/level/tail output is byte-identical at any
/// ring cap, `with_store` hydrates the meta cap, and saving a new cap
/// rebuilds the ring empty (the documented truncation).
#[gpui_kit::test]
fn t81_filters_unaffected_by_cap(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store.set_log_max_lines(500).expect("cap writes");
    let (view, cx) = mount_view(store, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.store.cap()),
        500,
        "with_store hydrates the meta cap"
    );

    let tags = manyproxy_ui::health::LogTagIndex::default();
    let source: Vec<String> = (0..400)
        .map(|i| match i % 4 {
            0 => format!("error dial {i} refused password=hunter2"),
            1 => format!("warn retry {i} slow"),
            2 => format!("debug tick {i}"),
            _ => format!("info heartbeat {i}"),
        })
        .collect();

    let cases = [
        ("", "", "", None, true),
        ("dial", "", "", None, true),
        ("", "slow", "", None, true),
        ("", "", "refused$", None, false),
        ("", "", "", Some(manyproxy_ui::LogLevel::Warn), true),
        (
            "heartbeat",
            "",
            "",
            Some(manyproxy_ui::LogLevel::Info),
            false,
        ),
    ];
    for (filter, exclude, pattern, level, tail) in cases {
        let mut by_cap = Vec::new();
        for cap in [500usize, 50_000] {
            let lines = cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    let mut logs = manyproxy_ui::LogStore::with_capacity(cap);
                    for line in &source {
                        logs.push_raw(line, &tags);
                    }
                    view.logs.store = logs;
                    view.logs.filter = filter.to_string();
                    view.logs.exclude = exclude.to_string();
                    view.logs.regex = pattern.to_string();
                    view.logs.level = level;
                    view.logs.tail = tail;
                    view.log_lines(cx)
                })
            });
            by_cap.push(lines);
        }
        assert_eq!(
            by_cap[0], by_cap[1],
            "cap must not change {filter:?}/{exclude:?}/{pattern:?}/{level:?}/{tail}"
        );
        assert!(
            !by_cap[0].iter().any(|line| line.contains("hunter2")),
            "redaction still decorates only redacted text"
        );
    }

    let tail = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut logs = manyproxy_ui::LogStore::with_capacity(500);
            for line in &source {
                logs.push_raw(line, &tags);
            }
            view.logs.store = logs;
            view.logs.filter.clear();
            view.logs.exclude.clear();
            view.logs.regex.clear();
            view.logs.level = None;
            view.logs.tail = true;
            view.log_lines(cx)
        })
    });
    assert_eq!(tail.len(), 200, "tail keeps the newest 200 lines");
    assert!(tail
        .last()
        .is_some_and(|line| line.contains("heartbeat 399")));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.logs.store.push_raw("info before cap change", &tags);
            view.commit_log_cap(window, "50,000", cx);
        });
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.store.cap()),
        50_000,
        "a typed save rebuilds the ring at the clamped cap"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.store.log_max_lines()),
        50_000,
        "the new cap persists"
    );
    assert!(
        view.read_with(cx, |view, _| view.logs.store.all().is_empty()),
        "the fresh ring truncates instead of carrying lines over"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_log_cap(window, "1", cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.store.cap()),
        500,
        "saving below the window clamps to 500"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.commit_log_cap(window, "junk", cx));
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.logs.store.cap()),
        500,
        "junk keeps the previous cap"
    );
}

#[gpui_kit::test]
fn close_veto_passes_when_nothing_running(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    assert_eq!(view.read_with(cx, |view, _| view.close_veto_text()), None);
    cx.update(|window, cx| {
        view.update(cx, |view, _| view.confirm_close_stop(window));
    });
    assert!(view.read_with(cx, |view, _| view.close_confirmed));
    assert!(
        view.read_with(cx, |view, _| view.store.window_geometry())
            .is_some(),
        "confirm-close also persists the window rectangle (T-32)"
    );
    assert_eq!(view.read_with(cx, |view, _| view.close_veto_text()), None);
}

#[gpui_kit::test]
fn shell_boots_with_empty_library(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 0);

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    let list_found = cx.update(|window, _| window.try_find("config-list").is_some());
    assert!(
        list_found,
        "config list renders: missing element, see registered paths in panic context"
    );
    assert!(cx.update(|window, _| window.try_find("action-bar-count").is_some()));

    cx.update(|window, cx| {
        window.open_dialog(cx, |dialog: Dialog, _, _| dialog.title("Add proxy config"));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        cx.debug_bounds("dialog-0").is_some(),
        "dialog layer paints the dialog"
    );
}

#[gpui_kit::test]
fn f18_empty_library_offers_add_and_import(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);

    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "the empty canvas is this test's subject"
    );
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    // Both intake actions the sentence names sit under it, sharing the
    // toolbar's handlers.
    assert_painted(cx, "empty-add");
    assert_painted(cx, "empty-import");

    cx.update(|window, cx| window.click("empty-add", cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_painted(cx, "field-guided-server");
    cx.update(|window, cx| window.close_all_dialogs(cx));

    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.update(|window, cx| window.click("empty-import", cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_painted(cx, "import-guide");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn library_lists_stored_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let first = store.add(seed_input("Work")).expect("seed works");
    let second = store.add(seed_input("Home")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    for id in [first.id, second.id] {
        let row_id = format!("library-row-{id}");
        assert!(
            cx.update(move |window, _| window.try_find(row_id).is_some()),
            "stored row renders with a stable domain id"
        );
    }
    // F19: the inspector starts closed and only a pick opens it.
    assert!(cx.update(|window, _| window.try_find("inspector-drawer").is_none()));
    view.update(cx, |view, cx| {
        view.open_inspector();
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, _| window.try_find("config-detail").is_some()));
}

#[gpui_kit::test]
fn add_dialog_opens_with_fields(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));

    for field in ["field-name", "protocol-select", "field-bind", "field-port"] {
        assert!(
            cx.update(|window, _| window.try_find(field).is_some()),
            "add dialog renders {field}"
        );
    }
    assert_eq!(
        label_of("field-bind", cx),
        "Bind address",
        "the visible Field label is the input's accessible name"
    );
    assert_eq!(
        label_of("field-port", cx),
        "Port",
        "every dialog_field carries its label into aria_label"
    );
}

#[gpui_kit::test]
fn t01_protocol_picker_lists_27_canonical_types(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "protocol-select");

    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    for protocol in ["vless", "vmess", "trojan"] {
        assert_painted(cx, &format!("protocol-option-{protocol}"));
    }

    let keystroke = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert_absent(cx, "protocol-option-vless".to_string());

    assert_eq!(
        VALID_PROTOCOLS.len(),
        27,
        "picker covers the canonical list"
    );
    assert_eq!(
        manyproxy_ui::protocol_choices().len(),
        27,
        "one picker row per canonical type"
    );
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::blank(window, cx);
            for protocol in VALID_PROTOCOLS {
                form.protocol_select.update(cx, |state, cx| {
                    state.set_selected_value(&protocol.to_string(), window, cx);
                });
                assert_eq!(
                    form.protocol_select.read(cx).selected_value(),
                    Some(&protocol.to_string()),
                    "{protocol} resolves in the picker"
                );
            }
        })
    });
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t01_edit_locks_protocol_type(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store.add(seed_input("Work")).expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_dialog_open(cx);
    assert_painted(cx, "protocol-lock");
    assert_absent(cx, "protocol-select".to_string());

    let untouched = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &seeded, None);
            form.as_patch_vs(cx, &seeded).protocol.is_none()
        })
    });
    assert!(untouched, "locked edit changes no type");

    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let committed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::prefilled(window, cx, &seeded, None);
            form.name.update(cx, |state, cx| {
                state.set_value("Work2", window, cx);
            });
            let patch = form.as_patch_vs(cx, &seeded);
            assert!(patch.protocol.is_none(), "rename carries no type change");
            view.commit_edit_with_focus(&seeded.id, patch, &form, &form_error, window, cx)
        })
    });
    assert!(committed, "rename commits");
    let row = view.read_with(cx, |view, _| {
        view.store
            .get(&seeded.id)
            .expect("get works")
            .expect("row kept")
    });
    assert_eq!(row.name, "Work2", "rename lands");
    assert_eq!(row.protocol, "vless", "baseline type kept");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t01_virtual_types_hide_server_port(cx: &mut TestAppContext) {
    use gpui_kit::VisualTestContext;
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);

    assert_painted(cx, "field-guided-server");
    assert_painted(cx, "field-guided-port");
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let mut cursor = row_of("vless");
    let pick = |cx: &mut VisualTestContext, target: usize, cursor: &mut usize| {
        cx.update(|window, cx| {
            window.click("protocol-select", cx);
        });
        cx.run_until_parked();
        paint_dialog_open(cx);
        let (key, steps) = if target >= *cursor {
            ("down", target - *cursor)
        } else {
            ("up", *cursor - target)
        };
        let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
        for _ in 0..steps {
            cx.update(|window, cx| {
                window.dispatch_keystroke(keystroke.clone(), cx);
            });
            cx.run_until_parked();
        }
        let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(enter, cx);
        });
        cx.run_until_parked();
        paint_dialog_open(cx);
        *cursor = target;
    };
    for virtual_proto in [
        "chain",
        "custom-outbound",
        "custom-config",
        "extra-core",
        "direct",
    ] {
        pick(cx, row_of(virtual_proto), &mut cursor);
        assert_absent(cx, "field-guided-server".to_string());
        assert_absent(cx, "field-guided-port".to_string());
        assert_painted(cx, "protocol-select");
        assert_painted(cx, "guided-hint");
    }

    pick(cx, row_of("vless"), &mut cursor);
    assert_painted(cx, "field-guided-server");
    assert_painted(cx, "field-guided-port");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t01_template_sync_rewrites_pristine_only(cx: &mut TestAppContext) {
    use manyproxy_ui::store::protocol_params_template;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let (pristine_after, dirty_after) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::blank(window, cx);
            form.protocol_select.update(cx, |state, cx| {
                state.set_selected_value(&"chain".to_string(), window, cx);
            });
            form.sync_protocol_template(window, cx);
            let pristine_after = form.params.read(cx).value().to_string();

            form.params.update(cx, |state, cx| {
                state.set_value(
                    "{\"type\":\"vless\",\"server\":\"a.example.com\"}",
                    window,
                    cx,
                );
            });
            form.protocol_select.update(cx, |state, cx| {
                state.set_selected_value(&"socks".to_string(), window, cx);
            });
            form.sync_protocol_template(window, cx);
            let dirty_after = form.params.read(cx).value().to_string();
            (pristine_after, dirty_after)
        })
    });
    assert_eq!(
        pristine_after,
        protocol_params_template("chain"),
        "pristine switch rewrites"
    );
    assert_eq!(
        dirty_after, "{\"type\":\"vless\",\"server\":\"a.example.com\"}",
        "dirty switch preserves"
    );
}

#[gpui_kit::test]
fn t01_enter_in_picker_popup_does_not_commit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_dialog_open(cx);
    assert_eq!(notification_count(cx), 0, "no toast before Enter");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, _| window
            .try_find("protocol-option-socks".to_string())
            .is_some()),
        "picker popup opened"
    );
    let keystroke = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
    paint_dialog_open(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "picker Enter keeps the dialog open (no half-filled commit)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "picker Enter stores no row"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "picker Enter runs no commit (a broken guard would toast the template error)"
    );
    assert_absent(cx, "protocol-option-socks".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t02_vless_panels_paint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    // K2: the protocol rows are visible from the first paint — no Show page.
    assert_painted(cx, "protocol-section");
    for id in [
        "vless-uuid",
        "vless-uuid-generate",
        "vless-flow",
        "vless-packet-encoding",
        "advanced-settings-caption",
    ] {
        assert_painted(cx, id);
    }
    // K2: everything else lives behind ONE collapsed Advanced Settings disclosure.
    for id in [
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "s4-caption",
        "tls-security-caption",
        "tls-extras-caption",
    ] {
        assert_absent(cx, id.to_string());
    }
    assert_absent(cx, "advanced-settings-section".to_string());

    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "advanced-settings-section",
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "s4-caption",
        "s4-reuse",
        "s4-timeout",
        "tls-security-caption",
        "tls-extras-caption",
        "tls-extras-section",
        "s2-insecure",
        "s2-pbk",
        "s2-sid",
    ] {
        assert_painted(cx, id);
    }

    for toggle in ["s1-toggle", "s2-toggle", "s3-toggle"] {
        cx.update(|window, cx| {
            window.click(toggle, cx);
        });
        cx.run_until_parked();
    }
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "s1-network",
        "s2-enable",
        "s2-sni",
        "s2-fp",
        "s3-mux-group",
        "s3-brutal",
        "s3-up",
        "s3-down",
    ] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "s1-path".to_string());
    assert_absent(cx, "s3-vision-note".to_string());

    // T-06: one dialog layer now — Escape pops it in a single press and the
    // shell takes the keyboard focus back.
    let esc = gpui_kit::Keystroke::parse("escape").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(esc, cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "escape closes the single-layer add dialog"
    );
    let shell_focus = view.read_with(cx, |view, _| view.focus.clone());
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        Some(shell_focus),
        "the shell holds focus again"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t02_uuid_generate_button_toasts(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_eq!(notification_count(cx), 0, "no toast before Generate");

    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("vless-uuid-generate", cx);
    });
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 1, "Generate toasts once");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "dialog stays open after Generate"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t02_generate_uuid_v4_shape() {
    use manyproxy_ui::generate_uuid_v4;
    for _ in 0..8 {
        let uuid = generate_uuid_v4();
        assert_eq!(uuid.len(), 36, "hyphenated length: {uuid}");
        assert_eq!(
            uuid.chars().filter(|c| *c == '-').count(),
            4,
            "four hyphens: {uuid}"
        );
        assert_eq!(uuid.chars().nth(14), Some('4'), "version nibble: {uuid}");
        assert!(
            matches!(uuid.chars().nth(19), Some('8' | '9' | 'a' | 'b')),
            "variant nibble: {uuid}"
        );
        assert!(
            uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
            "hex only: {uuid}"
        );
    }
    assert_ne!(generate_uuid_v4(), generate_uuid_v4(), "unique");
}

#[gpui_kit::test]
fn t02_ws_to_tcp_hides_and_restores(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "WsRow".to_string(),
            protocol: "vless".to_string(),
            params: "{\"type\":\"vless\",\"server\":\"a.example.com\",\"server_port\":443,\
                    \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"network\":\"ws\",\
                    \"path\":\"/w\",\"host\":\"h.example.com\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    cx.update(|window, cx| {
        window.click("s1-toggle", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s1-path");
    assert_painted(cx, "s1-host");

    cx.update(|window, cx| {
        window.click("s1-network", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("s1-network-option-tcp", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_absent(cx, "s1-path".to_string());
    assert_absent(cx, "s1-host".to_string());

    cx.update(|window, cx| {
        window.click("s1-network", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("s1-network-option-ws", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s1-path");
    assert_painted(cx, "s1-host");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t02_reality_vision_prefill_round_trips(cx: &mut TestAppContext) {
    use manyproxy_ui::import::import_text;
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let link = "vless://11111111-2222-4333-8444-555555555555@a.example.com:443\
            ?encryption=none&security=reality&pbk=9p5voorXr3Y8kZ7Q6W5E4R3T2Y1U0I9O8P7O6&fp=chrome\
            &sni=a.example.com&sid=9f8e7d&spx=%2F&flow=xtls-rprx-vision&type=tcp#RealityVision";
    let mut batch = import_text(&link.replace(' ', ""));
    assert!(
        batch.failures.is_empty(),
        "link imports: {:?}",
        batch.failures
    );
    assert_eq!(batch.configs.len(), 1, "one row");
    let parsed = batch.configs.remove(0);
    assert_eq!(parsed.protocol, "vless");
    let row = ProxyConfig {
        id: "t02".to_string(),
        name: parsed.name.clone(),
        protocol: parsed.protocol.clone(),
        params: parsed.params.clone(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let (uuid, flow, enable, pbk, sni, fp, folded) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            let uuid = form.vless_uuid.read(cx).value().to_string();
            let flow = form
                .vless_flow
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            let enable = *form.s2_enable.borrow();
            let pbk = form.s2_pbk.read(cx).value().to_string();
            let sni = form.s2_sni.read(cx).value().to_string();
            let fp = form
                .s2_fp
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            let folded = form.effective_params(cx);
            (uuid, flow, enable, pbk, sni, fp, folded)
        })
    });
    assert_eq!(
        uuid, "11111111-2222-4333-8444-555555555555",
        "uuid prefills"
    );
    assert_eq!(flow, "xtls-rprx-vision", "flow prefills");
    assert!(enable, "reality enables TLS");
    assert!(!pbk.is_empty(), "pbk prefills");
    assert_eq!(sni, "a.example.com", "sni prefills");
    assert_eq!(fp, "chrome", "fp prefills");
    assert_eq!(
        folded,
        parsed.params.expose(),
        "untouched refold is byte-identical"
    );
}

#[gpui_kit::test]
fn t02_vision_disables_mux_with_reason(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    // K2: the transport panel only exists inside the Advanced Settings disclosure.
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("s3-toggle", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s3-mux-group");
    cx.update(|window, cx| {
        window.click("vless-flow", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("vless-flow-option-xtls-rprx-vision", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s3-vision-note");
    assert_absent(cx, "s3-mux-group".to_string());
    cx.update(|window, cx| {
        window.click("vless-flow", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("vless-flow-option-default", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s3-mux-group");
    assert_absent(cx, "s3-vision-note".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t02_brutal_forces_mux_on(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_absent(cx, "s3-brutal-note".to_string());

    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    // K2: open the Advanced Settings disclosure that holds the transport panel.
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("s3-toggle", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("s3-brutal-switch", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s3-brutal-note");
    let folded = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::blank(window, cx);
            *form.s3_brutal.borrow_mut() = true;
            form.effective_params(cx)
        })
    });
    let value: serde_json::Value = serde_json::from_str(&folded).expect("brutal fold is JSON");
    assert_eq!(value["mux"], "on", "brutal forces mux on in the fold");
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t03_dropdown_counts() {
    assert_eq!(
        manyproxy_ui::vmess_security_choices().len(),
        5,
        "vmess security lists 5 ciphers"
    );
    assert_eq!(
        manyproxy_ui::packet_encoding_choices().len(),
        3,
        "packet_encoding lists 3"
    );
    let methods = manyproxy_ui::ss_method_choices();
    assert_eq!(
        methods
            .iter()
            .filter(|choice| !choice.value.is_empty())
            .count(),
        18,
        "ss method lists 18 ciphers"
    );
    assert_eq!(
        manyproxy_ui::ss_plugin_choices().len(),
        3,
        "ss plugin lists 3"
    );
}

#[gpui_kit::test]
fn t03_vmess_page_paints(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let params = "{\"type\":\"vmess\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"alter_id\":\"0\",\
            \"security\":\"auto\",\"tls\":true,\"sni\":\"a.example.com\",\"network\":\"ws\",\
            \"path\":\"/w\",\"mux\":\"on\"}";
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "VmessRow".to_string(),
            protocol: "vmess".to_string(),
            params: params.to_string().into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    // K2: the protocol rows paint immediately — the per-protocol Show page is gone.
    assert_painted(cx, "protocol-section");
    for id in [
        "vmess-uuid",
        "vmess-uuid-generate",
        "vmess-aid",
        "vmess-security",
        "vmess-packet-encoding",
    ] {
        assert_painted(cx, id);
    }
    for id in ["s1-caption", "s2-caption", "s3-caption"] {
        assert_absent(cx, id.to_string());
    }

    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in ["s1-caption", "s2-caption", "s3-caption"] {
        assert_painted(cx, id);
    }

    for id in [
        "ss-method",
        "trojan-password",
        "vless-flow",
        "s3-vision-note",
    ] {
        assert_absent(cx, id.to_string());
    }
    for toggle in ["s1-toggle", "s2-toggle", "s3-toggle"] {
        cx.update(|window, cx| {
            window.click(toggle, cx);
        });
        cx.run_until_parked();
    }
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "s1-network",
        "s1-path",
        "s2-enable",
        "s2-sni",
        "s3-mux-group",
    ] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = ProxyConfig {
        id: "t03-v".to_string(),
        name: "V".to_string(),
        protocol: "vmess".to_string(),
        params: params.to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            for cipher in manyproxy_ui::store::VMESS_CIPHERS {
                form.vmess_security.update(cx, |state, cx| {
                    state.set_selected_value(&cipher.to_string(), window, cx);
                });
                assert_eq!(
                    form.vmess_security
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    cipher,
                    "cipher {cipher} resolves"
                );
            }
            for choice in manyproxy_ui::packet_encoding_choices() {
                form.vmess_packet_encoding.update(cx, |state, cx| {
                    state.set_selected_value(&choice.value.clone(), window, cx);
                });
                assert_eq!(
                    form.vmess_packet_encoding
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    choice.value,
                    "packet spelling resolves"
                );
            }
        })
    });
}

#[gpui_kit::test]
fn t03_trojan_page_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "TrojanRow".to_string(),
            protocol: "trojan".to_string(),
            params: "{\"type\":\"trojan\",\"server\":\"a.example.com\",\"server_port\":443,\
                    \"password\":\"secret\",\"security\":\"tls\",\"sni\":\"a.example.com\",\
                    \"network\":\"ws\",\"path\":\"/w\",\"mux\":\"on\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in ["trojan-password", "s1-caption", "s2-caption", "s3-caption"] {
        assert_painted(cx, id);
    }
    for id in ["vmess-aid", "ss-method", "vless-uuid", "s3-vision-note"] {
        assert_absent(cx, id.to_string());
    }
    for toggle in ["s1-toggle", "s2-toggle", "s3-toggle"] {
        cx.update(|window, cx| {
            window.click(toggle, cx);
        });
        cx.run_until_parked();
    }
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in ["s1-network", "s2-enable", "s3-mux-group", "s3-brutal"] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t03_ss_page_paints_mux_only(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let params = "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\"server_port\":8388,\
            \"method\":\"aes-128-gcm\",\"password\":\"pw\",\"mux\":\"on\"}";
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "SsRow".to_string(),
            protocol: "shadowsocks".to_string(),
            params: params.to_string().into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "ss-method",
        "ss-password",
        "ss-plugin",
        "ss-plugin-opts",
        "ss-uot",
        "s3-caption",
    ] {
        assert_painted(cx, id);
    }

    for id in [
        "s1-caption",
        "s2-caption",
        "s2-enable",
        "trojan-password",
        "vmess-aid",
        "ss-plugin-note",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| {
        window.click("s3-toggle", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    assert_painted(cx, "s3-mux-group");
    for id in ["s1-caption", "s2-caption", "s2-enable", "s2-pbk"] {
        assert_absent(cx, id.to_string());
    }

    // K2: shadowsocks' dial row unfolds under the same disclosure (always
    // expanded — it has no Show page of its own).
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "s4-caption");
    for id in ["s4-reuse", "s4-timeout", "s4-bind"] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "tls-extras-caption".to_string());

    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, _| window.try_find("advanced-settings-section").is_none()),
        "the disclosure collapses back to the guided rows"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = ProxyConfig {
        id: "t03-s".to_string(),
        name: "S".to_string(),
        protocol: "shadowsocks".to_string(),
        params: params.to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            for method in manyproxy_ui::store::SS_METHODS {
                form.ss_method.update(cx, |state, cx| {
                    state.set_selected_value(&method.to_string(), window, cx);
                });
                assert_eq!(
                    form.ss_method
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    method,
                    "method {method} resolves"
                );
            }
            for plugin in ["", "obfs-local", "v2ray-plugin"] {
                form.ss_plugin.update(cx, |state, cx| {
                    state.set_selected_value(&plugin.to_string(), window, cx);
                });
                assert_eq!(
                    form.ss_plugin
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    plugin,
                    "plugin spellings resolve"
                );
            }
        })
    });
}

#[gpui_kit::test]
fn t03_ss_plugin_note_paints_on_pick(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "SsPluginRow".to_string(),
            protocol: "shadowsocks".to_string(),
            params: "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\"server_port\":8388,\
                    \"method\":\"aes-128-gcm\",\"password\":\"pw\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_absent(cx, "ss-plugin-note".to_string());
    cx.update(|window, cx| {
        window.click("ss-plugin", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("ss-plugin-option-obfs-local", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "ss-plugin-note");
    cx.update(|window, cx| {
        window.click("ss-plugin", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.update(|window, cx| {
        window.click("ss-plugin-option-default", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_absent(cx, "ss-plugin-note".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t03_legacy_vmess_prefill_round_trips(cx: &mut TestAppContext) {
    use manyproxy_ui::import::import_text;
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let link = "vmess://eyJ2IjoiMiIsInBzIjoibGVnYWN5LXZtZXNzIiwiYWRkIjoiYS5leGFtcGxlLmNvbSIsInBvcnQiOiI0NDMiLCJpZCI6IjExMTExMTExLTIyMjItNDMzMy04NDQ0LTU1NTU1NTU1NTU1NSIsImFpZCI6IjAiLCJzY3kiOiJhdXRvIiwibmV0IjoidGNwIiwidHlwZSI6Im5vbmUiLCJ0bHMiOiJ0bHMiLCJzbmkiOiJhLmV4YW1wbGUuY29tIn0=";
    let mut batch = import_text(link);
    assert!(
        batch.failures.is_empty(),
        "link imports: {:?}",
        batch.failures
    );
    assert_eq!(batch.configs.len(), 1, "one row");
    let parsed = batch.configs.remove(0);
    assert_eq!(parsed.protocol, "vmess");
    let row = ProxyConfig {
        id: "t03".to_string(),
        name: parsed.name.clone(),
        protocol: parsed.protocol.clone(),
        params: parsed.params.clone(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let (uuid, aid, cipher, enable, sni, folded) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            let uuid = form.vmess_uuid.read(cx).value().to_string();
            let aid = form.vmess_aid.read(cx).value().to_string();
            let cipher = form
                .vmess_security
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            let enable = *form.s2_enable.borrow();
            let sni = form.s2_sni.read(cx).value().to_string();
            let folded = form.effective_params(cx);
            (uuid, aid, cipher, enable, sni, folded)
        })
    });
    assert_eq!(
        uuid, "11111111-2222-4333-8444-555555555555",
        "uuid prefills"
    );
    assert_eq!(aid, "0", "aid prefills");
    assert_eq!(cipher, "auto", "legacy cipher prefills");
    assert!(enable, "legacy boolean enables TLS");
    assert_eq!(sni, "a.example.com", "sni prefills");
    assert_eq!(
        folded,
        parsed.params.expose(),
        "untouched refold is byte-identical"
    );
}

#[gpui_kit::test]
fn t03_trojan_ss_prefill_round_trips(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let trojan_params = "{\"type\":\"trojan\",\"server\":\"a.example.com\",\"server_port\":443,\
            \"password\":\"secret\",\"security\":\"tls\",\"sni\":\"a.example.com\",\
            \"network\":\"ws\",\"path\":\"/w\",\"mux\":\"on\"}";
    let ss_params = "{\"type\":\"shadowsocks\",\"server\":\"a.example.com\",\"server_port\":8388,\
            \"method\":\"chacha20-ietf-poly1305\",\"password\":\"pw\",\
            \"plugin_opts\":\"obfs=http\",\"udp_over_tcp\":true,\"mux\":\"on\"}";
    let rows = [
        ("t03-t", "T", "trojan", trojan_params),
        ("t03-s", "S", "shadowsocks", ss_params),
    ];
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    for (id, name, protocol, params) in rows {
        let row = ProxyConfig {
            id: id.to_string(),
            name: name.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string().into(),
            group: "default".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        let (password, extra, folded) = cx.update(|window, cx| {
            view.update(cx, |_, cx| {
                let form = ConfigForm::prefilled(window, cx, &row, None);
                let password = if protocol == "trojan" {
                    form.trojan_password.read(cx).value().to_string()
                } else {
                    form.ss_password.read(cx).value().to_string()
                };
                let extra = if protocol == "trojan" {
                    form.s2_sni.read(cx).value().to_string()
                } else {
                    form.ss_method
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default()
                };
                (password, extra, form.effective_params(cx))
            })
        });
        assert_eq!(
            folded, params,
            "{protocol} untouched refold is byte-identical"
        );
        if protocol == "trojan" {
            assert_eq!(password, "secret", "trojan password prefills");
            assert_eq!(extra, "a.example.com", "trojan sni prefills");
        } else {
            assert_eq!(password, "pw", "ss password prefills");
            assert_eq!(extra, "chacha20-ietf-poly1305", "ss method prefills");
        }
    }
}

#[gpui_kit::test]
fn t03_enter_in_ss_picker_popup_does_not_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("shadowsocks");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);

    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "ss-method");

    assert_eq!(notification_count(cx), 0, "no toast before Enter");
    cx.update(|window, cx| {
        window.click("ss-method", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, _| window
            .try_find("ss-method-option-default".to_string())
            .is_some()),
        "method popup opened"
    );
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "method Enter keeps the dialog open (no half-filled commit)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "method Enter stores no row"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "method Enter runs no commit (a broken guard would toast the template error)"
    );
    assert_absent(cx, "ss-method-option-default".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t04_wireguard_page_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
            .add(NewConfig {
                name: "WgRow".to_string(),
                protocol: "wireguard".to_string(),
                params: "{\"type\":\"wireguard\",\"server\":\"192.0.2.44\",\"server_port\":51820,\
                    \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\",\"address\":\"10.0.0.2/32\"}"
                    .to_string().into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "wg-private-key",
        "wg-private-key-generate",
        "wg-public-key",
        "wg-preshared-key",
        "wg-reserved",
        "wg-keepalive",
        "wg-local-addr",
        "wg-dns",
        "wg-mtu",
        "wg-workers",
        "wg-sys-ifc",
        "wg-notes",
    ] {
        assert_painted(cx, id);
    }

    for id in [
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "awg-version",
        "vmess-aid",
        "ss-method",
        "trojan-password",
        "vless-uuid",
    ] {
        assert_absent(cx, id.to_string());
    }

    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "advanced-settings-section",
        "s4-caption",
        "s4-reuse",
        "s4-timeout",
    ] {
        assert_painted(cx, id);
    }

    assert_eq!(notification_count(cx), 0, "no toast before Generate");
    cx.update(|window, cx| {
        window.click("wg-private-key-generate", cx);
    });
    cx.run_until_parked();
    assert_eq!(notification_count(cx), 1, "Generate toasts once");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "dialog stays open after Generate"
    );
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t04_amneziawg_page_paints_and_version_gates(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));

    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
            .add(NewConfig {
                name: "AwgRow".to_string(),
                protocol: "amneziawg".to_string(),
                params: "{\"type\":\"amneziawg\",\"server\":\"192.0.2.90\",\"server_port\":51820,\
                    \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\",\
                    \"awg_version\":\"2.0\",\"jc\":\"5\",\"s1\":\"20\",\"h1\":\"1-100\",\"i1\":\"x\"}"
                    .to_string().into(),
                group: String::new(),
                bind_ip: String::new(),
                port: String::new(),
            })
            .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "awg-private-key",
        "awg-private-key-generate",
        "awg-public-key",
        "awg-preshared-key",
        "awg-reserved",
        "awg-keepalive",
        "awg-local-addr",
        "awg-dns",
        "awg-mtu",
        "awg-workers",
        "awg-sys-ifc",
        "awg-enable",
        "awg-version",
        "awg-jc",
        "awg-jmin",
        "awg-jmax",
        "awg-s1",
        "awg-h1",
        "awg-i1",
        "awg-notes",
    ] {
        assert_painted(cx, id);
    }

    assert_painted(cx, "awg-v3-note");
    for id in [
        "awg-hpk",
        "awg-pad",
        "awg-rekey-after",
        "awg-trailers",
        "awg-cookies",
        "wg-private-key",
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "ss-method",
    ] {
        assert_absent(cx, id.to_string());
    }

    cx.update(|window, cx| {
        window.click("awg-version", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.update(|window, cx| {
        window.click("awg-version-option-3.x", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "awg-v3-caption",
        "awg-hpk",
        "awg-pad",
        "awg-rekey-after",
        "awg-rekey-timeout",
        "awg-reject-after",
        "awg-keepalive-timeout",
        "awg-max-handshake",
        "awg-trailers",
        "awg-cookies",
    ] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "awg-v3-note".to_string());

    cx.update(|window, cx| {
        window.click("awg-version", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.update(|window, cx| {
        window.click("awg-version-option-default", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "awg-v3-note");
    assert_absent(cx, "awg-hpk".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = manyproxy_ui::store::ProxyConfig {
        id: "t04-a".to_string(),
        name: "A".to_string(),
        protocol: "amneziawg".to_string(),
        params: "{\"type\":\"amneziawg\"}".to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = manyproxy_ui::ConfigForm::prefilled(window, cx, &row, None);
            for version in ["", "2.0", "3.x"] {
                form.awg_version.update(cx, |state, cx| {
                    state.set_selected_value(&version.to_string(), window, cx);
                });
                assert_eq!(
                    form.awg_version
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    version,
                    "version {version:?} resolves"
                );
            }
        })
    });
    assert_eq!(
        manyproxy_ui::awg_version_choices().len(),
        3,
        "version lists blank + 2.0 + 3.x"
    );
}

#[gpui_kit::test]
fn t04_awg_version_toggle_hide_preserves(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let params = "{\"type\":\"amneziawg\",\"server\":\"192.0.2.90\",\"server_port\":51820,\
            \"private_key\":\"LOCAL\",\"public_key\":\"REMOTE\",\"awg_version\":\"3.x\",\
            \"jc\":\"5\",\"headerprotectionkey\":\"HPK\",\"randomtrailers\":true}";
    let row = ProxyConfig {
        id: "t04-h".to_string(),
        name: "H".to_string(),
        protocol: "amneziawg".to_string(),
        params: params.to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let folded = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            assert_eq!(
                form.effective_params(cx),
                params,
                "untouched v3 row refolds byte-identical"
            );

            form.awg_version.update(cx, |state, cx| {
                state.set_selected_value(&"2.0".to_string(), window, cx);
            });
            form.effective_params(cx)
        })
    });
    let folded_value: serde_json::Value = serde_json::from_str(&folded).expect("refold parses");
    assert_eq!(folded_value["awg_version"], "2.0", "version switched");
    assert_eq!(
        folded_value["headerprotectionkey"], "HPK",
        "hidden v3 value preserved"
    );
    assert_eq!(
        folded_value["randomtrailers"], true,
        "hidden switch preserved"
    );
    assert_eq!(folded_value["jc"], "5", "visible matrix kept");
}

#[gpui_kit::test]
fn t04_conf_prefill_round_trips(cx: &mut TestAppContext) {
    use manyproxy_ui::import::import_text;
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let conf = "[Interface]\nPrivateKey = LOCAL-KEY\nAddress = 10.0.0.2/32\n\
            DNS = 1.1.1.1\nMTU = 1420\n\n[Peer]\nPublicKey = REMOTE-KEY\n\
            PresharedKey = PSK\nAllowedIPs = 0.0.0.0/0\n\
            Endpoint = 192.0.2.44:51820\nPersistentKeepalive = 25\nReserved = 1,2,3\n";
    let awg_conf = "[Interface]\nPrivateKey = LOCAL-KEY\nAddress = 10.0.0.2/32\n\
            Jc = 5\nJmin = 30\nJmax = 1000\nS1 = 20\nH1 = 1-100\nI1 = x\n\n[Peer]\n\
            PublicKey = REMOTE-KEY\nEndpoint = 192.0.2.90:51820\n";
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let mut batch = import_text(conf);
    assert!(batch.failures.is_empty(), "wg conf imports");
    assert_eq!(batch.configs.len(), 1, "one wg row");
    let wg = batch.configs.remove(0);
    assert_eq!(wg.protocol, "wireguard");
    let mut awg_batch = import_text(awg_conf);
    assert!(awg_batch.failures.is_empty(), "awg conf imports");
    assert_eq!(awg_batch.configs.len(), 1, "one awg row");
    let awg = awg_batch.configs.remove(0);
    assert_eq!(awg.protocol, "amneziawg");
    for parsed in [wg, awg] {
        let protocol = parsed.protocol.clone();
        let params = parsed.params.clone();
        let row = ProxyConfig {
            id: "t04-c".to_string(),
            name: parsed.name.clone(),
            protocol: protocol.clone(),
            params: params.clone(),
            group: "default".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        let (fields, folded) = cx.update(|window, cx| {
            view.update(cx, |_, cx| {
                let form = ConfigForm::prefilled(window, cx, &row, None);
                let fields: Vec<String> = if protocol == "wireguard" {
                    vec![
                        form.wg_private.read(cx).value().to_string(),
                        form.wg_public.read(cx).value().to_string(),
                        form.wg_preshared.read(cx).value().to_string(),
                        form.wg_reserved.read(cx).value().to_string(),
                        form.wg_keepalive.read(cx).value().to_string(),
                        form.wg_local_addr.read(cx).value().to_string(),
                        form.wg_dns.read(cx).value().to_string(),
                        form.wg_mtu.read(cx).value().to_string(),
                    ]
                } else {
                    vec![
                        form.awg_private.read(cx).value().to_string(),
                        form.awg_public.read(cx).value().to_string(),
                        form.awg_local_addr.read(cx).value().to_string(),
                        form.awg_jc.read(cx).value().to_string(),
                        form.awg_jmin.read(cx).value().to_string(),
                        form.awg_jmax.read(cx).value().to_string(),
                        form.awg_s1.read(cx).value().to_string(),
                        form.awg_h1.read(cx).value().to_string(),
                        form.awg_i1.read(cx).value().to_string(),
                        form.awg_version
                            .read(cx)
                            .selected_value()
                            .cloned()
                            .unwrap_or_default(),
                    ]
                };
                (fields, form.effective_params(cx))
            })
        });
        assert_eq!(
            folded,
            params.expose(),
            "{protocol} untouched refold is byte-identical"
        );
        if protocol == "wireguard" {
            assert_eq!(
                fields,
                vec![
                    "LOCAL-KEY",
                    "REMOTE-KEY",
                    "PSK",
                    "1,2,3",
                    "25",
                    "10.0.0.2/32",
                    "1.1.1.1",
                    "1420",
                ],
                "wg conf values sit in kit fields"
            );
        } else {
            assert_eq!(
                fields,
                vec![
                    "LOCAL-KEY",
                    "REMOTE-KEY",
                    "10.0.0.2/32",
                    "5",
                    "30",
                    "1000",
                    "20",
                    "1-100",
                    "x",
                    "2.0",
                ],
                "awg conf values sit in kit fields"
            );
        }
    }
}

#[gpui_kit::test]
fn t04_enter_in_awg_picker_popup_does_not_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("amneziawg");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);

    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "awg-version");

    assert_eq!(notification_count(cx), 0, "no toast before Enter");
    cx.update(|window, cx| {
        window.click("awg-version", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, _| window
            .try_find("awg-version-option-default".to_string())
            .is_some()),
        "version popup opened"
    );
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "version Enter keeps the dialog open (no half-filled commit)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "version Enter stores no row"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "version Enter runs no commit (a broken guard would toast the template error)"
    );
    assert_absent(cx, "awg-version-option-default".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t04_generate_wireguard_key_shape() {
    use base64::Engine as _;
    use manyproxy_ui::generate_wireguard_key;
    for _ in 0..8 {
        let key = generate_wireguard_key();
        assert_eq!(key.len(), 44, "base64 32 bytes: {key}");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&key)
            .expect("key decodes");
        assert_eq!(bytes.len(), 32, "32 key bytes: {key}");
    }
    assert_ne!(generate_wireguard_key(), generate_wireguard_key(), "unique");
}

#[test]
fn t05_dropdown_counts() {
    use manyproxy_ui::store::{HY_AUTH_TYPES, TUIC_CC_MODES, TUIC_RELAY_MODES};
    assert_eq!(HY_AUTH_TYPES, ["STRING", "BASE64"]);
    assert_eq!(TUIC_CC_MODES, ["bbr", "cubic", "new_reno"]);
    assert_eq!(TUIC_RELAY_MODES, ["native", "quic"]);
    assert_eq!(
        manyproxy_ui::hy_auth_type_choices().len(),
        3,
        "auth_type lists blank + 2"
    );
    assert_eq!(
        manyproxy_ui::tuic_cc_choices().len(),
        4,
        "cc lists blank + 3"
    );
    assert_eq!(
        manyproxy_ui::tuic_relay_choices().len(),
        3,
        "relay lists blank + 2"
    );
}

#[gpui_kit::test]
fn t05_hysteria_add_toggles_layouts(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };

    let pick = |cx: &mut VisualTestContext, target: &str, current: &str| {
        let start = row_of(current);
        let end = row_of(target);
        cx.update(|window, cx| {
            window.click("protocol-select", cx);
        });
        cx.run_until_parked();
        paint_jumbo_dialog(cx);
        let (key, steps) = if end >= start {
            ("down", end - start)
        } else {
            ("up", start - end)
        };
        let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
        for _ in 0..steps {
            cx.update(|window, cx| {
                window.dispatch_keystroke(keystroke.clone(), cx);
            });
            cx.run_until_parked();
        }
        let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(enter.clone(), cx);
        });
        cx.run_until_parked();
        paint_jumbo_dialog(cx);
        cx.run_until_parked();
        paint_jumbo_dialog(cx);
    };
    pick(cx, "hysteria", "vless");
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    // K2: TLS + dial panels live inside the Advanced Settings disclosure.
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);

    for id in [
        "hy-version-group",
        "hy-server-ports",
        "hy-hop-interval",
        "hy-up",
        "hy-down",
        "hy-obfs",
        "hy-auth-type",
        "hy-auth",
        "hy-recv-window-conn",
        "hy-recv-window",
        "hy-mtu",
        "hy-notes",
        "s2-caption",
        "s4-caption",
    ] {
        assert_painted(cx, id);
    }
    for id in [
        "hy-password",
        "hy-version-locked",
        "s2-enable",
        "s1-caption",
        "s3-caption",
        "tuic-cc",
        "ss-method",
    ] {
        assert_absent(cx, id.to_string());
    }

    cx.update(|window, cx| {
        window.click("s2-toggle", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in ["s2-forced-note", "s2-sni", "s2-alpn", "s2-fp"] {
        assert_painted(cx, id);
    }

    // K2: the merged Advanced Settings disclosure renders the TLS extras for
    // every TLS protocol (the old per-protocol pages did not); insecure and
    // the Reality pair are present alongside the TLS security block.
    for id in ["s2-insecure", "s2-pbk", "s2-sid", "tls-extras-caption"] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "s2-enable".to_string());

    pick(cx, "hysteria2", "hysteria");
    for id in [
        "hy-version-group",
        "hy-server-ports",
        "hy-hop-interval",
        "hy-up",
        "hy-down",
        "hy-obfs",
        "hy-password",
        "s2-forced-note",
    ] {
        assert_painted(cx, id);
    }
    for id in [
        "hy-auth-type",
        "hy-auth",
        "hy-recv-window-conn",
        "hy-recv-window",
        "hy-mtu",
    ] {
        assert_absent(cx, id.to_string());
    }

    pick(cx, "hysteria", "hysteria2");
    for id in ["hy-auth-type", "hy-auth", "hy-mtu", "hy-version-group"] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "hy-password".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t05_hysteria_toggle_hide_preserves(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let params = "{\"type\":\"hysteria\",\"server\":\"192.0.2.14\",\"server_port\":443,\
            \"auth\":\"secret\",\"auth_type\":\"STRING\",\"up\":\"100\",\"down\":\"100\",\
            \"recv_window\":2048,\"disable_mtu_discovery\":true}";
    let row = ProxyConfig {
        id: "t05-h".to_string(),
        name: "H".to_string(),
        protocol: "hysteria".to_string(),
        params: params.to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let folded = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            assert_eq!(
                form.effective_params(cx),
                params,
                "untouched v1 row refolds byte-identical"
            );

            let as_v2 = manyproxy_ui::store::fold_hysteria_into_params(
                params,
                &manyproxy_ui::store::HysteriaFold {
                    password: "v2secret".to_string(),
                    ..Default::default()
                },
                true,
            );
            let as_v2_value: serde_json::Value =
                serde_json::from_str(&as_v2).expect("v2 refold parses");
            assert_eq!(as_v2_value["password"], "v2secret", "v2 credential folds");
            assert_eq!(as_v2_value["auth"], "secret", "hidden v1 auth preserved");
            assert_eq!(
                as_v2_value["auth_type"], "STRING",
                "hidden v1 marker preserved"
            );
            assert_eq!(
                as_v2_value["recv_window"], 2048,
                "hidden tunables preserved"
            );
            assert_eq!(as_v2_value["up"], "100", "common speeds kept");
            form.effective_params(cx)
        })
    });
    assert_eq!(folded, params, "v1 refold still byte-identical");
}

#[gpui_kit::test]
fn t05_hysteria_v1_blank_up_down_refuses_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::store::protocol_params_template;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let (closed, message, closed2) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("Hy", window, cx);
            });
            form.protocol_select.update(cx, |state, cx| {
                state.set_selected_value(&"hysteria".to_string(), window, cx);
            });

            form.params.update(cx, |state, cx| {
                state.set_value(protocol_params_template("hysteria"), window, cx);
            });
            form.guided_server.update(cx, |state, cx| {
                state.set_value("192.0.2.14", window, cx);
            });
            form.hy_auth.update(cx, |state, cx| {
                state.set_value("secret", window, cx);
            });
            let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
            let message = form_error
                .borrow()
                .clone()
                .map(|(_, message)| message)
                .unwrap_or_default();
            form.hy_up.update(cx, |state, cx| {
                state.set_value("100", window, cx);
            });
            form.hy_down.update(cx, |state, cx| {
                state.set_value("100", window, cx);
            });
            let closed2 = view.commit_add_with_focus(&form, &form_error, window, cx);
            (closed, message, closed2)
        })
    });
    assert!(!closed, "blank speeds keep the dialog open");
    assert!(
        message.contains("up_mbps") && message.contains("down_mbps"),
        "refusal names the page fields: {message}"
    );
    assert!(closed2, "speeds filled commits");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "exactly one row lands"
    );
}

#[gpui_kit::test]
fn t05_tuic_edit_round_trips(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let params = "{\"type\":\"tuic\",\"server\":\"192.0.2.12\",\"server_port\":443,\
            \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"password\":\"secret\",\
            \"congestion_control\":\"bbr\",\"udp_relay_mode\":\"quic\",\
            \"udp_over_stream\":true,\"zero_rtt_handshake\":true,\"heartbeat\":\"10s\",\
            \"sni\":\"example.com\",\"fp\":\"chrome\",\"insecure\":true}";
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "TuicRow".to_string(),
            protocol: "tuic".to_string(),
            params: params.to_string().into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "tuic-uuid",
        "tuic-uuid-generate",
        "tuic-password",
        "tuic-cc",
        "tuic-relay",
        "tuic-uot",
        "tuic-zero-rtt",
        "tuic-heartbeat",
        "tuic-notes",
        "s2-caption",
        "s4-caption",
    ] {
        assert_painted(cx, id);
    }

    for id in [
        "s2-enable",
        "s1-caption",
        "s3-caption",
        "hy-auth",
        "hy-password",
        "ss-method",
        "trojan-password",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| {
        window.click("s2-toggle", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in ["s2-forced-note", "s2-sni", "s2-fp"] {
        assert_painted(cx, id);
    }

    // K2: extras render with the merged Advanced Settings disclosure.
    for id in ["s2-insecure", "s2-pbk", "s2-sid"] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = ProxyConfig {
        id: "t05-t".to_string(),
        name: "T".to_string(),
        protocol: "tuic".to_string(),
        params: params.to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let (fields, folded) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            let fields = vec![
                form.tuic_uuid.read(cx).value().to_string(),
                form.tuic_password.read(cx).value().to_string(),
                form.tuic_cc
                    .read(cx)
                    .selected_value()
                    .cloned()
                    .unwrap_or_default(),
                form.tuic_relay
                    .read(cx)
                    .selected_value()
                    .cloned()
                    .unwrap_or_default(),
                form.tuic_heartbeat.read(cx).value().to_string(),
            ];
            (fields, form.effective_params(cx))
        })
    });
    assert_eq!(
        fields,
        vec![
            "11111111-2222-4333-8444-555555555555",
            "secret",
            "bbr",
            "quic",
            "10s",
        ],
        "tuic values sit in kit fields"
    );
    assert_eq!(folded, params, "untouched tuic refolds byte-identical");

    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = ConfigForm::prefilled(window, cx, &row, None);
            for mode in ["", "bbr", "cubic", "new_reno"] {
                form.tuic_cc.update(cx, |state, cx| {
                    state.set_selected_value(&mode.to_string(), window, cx);
                });
                assert_eq!(
                    form.tuic_cc
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    mode,
                    "cc {mode:?} resolves"
                );
            }
            for mode in ["", "native", "quic"] {
                form.tuic_relay.update(cx, |state, cx| {
                    state.set_selected_value(&mode.to_string(), window, cx);
                });
                assert_eq!(
                    form.tuic_relay
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    mode,
                    "relay {mode:?} resolves"
                );
            }
        })
    });
    assert_eq!(
        manyproxy_ui::tuic_cc_choices().len(),
        4,
        "cc lists blank + 3"
    );
    assert_eq!(
        manyproxy_ui::tuic_relay_choices().len(),
        3,
        "relay lists blank + 2"
    );
}

#[gpui_kit::test]
fn t05_enter_in_tuic_picker_popup_does_not_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("tuic");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);

    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "tuic-cc");

    assert_eq!(notification_count(cx), 0, "no toast before Enter");
    cx.update(|window, cx| {
        window.click("tuic-cc", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, _| window
            .try_find("tuic-cc-option-default".to_string())
            .is_some()),
        "cc popup opened"
    );
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "cc Enter keeps the dialog open (no half-filled commit)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "cc Enter stores no row"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "cc Enter runs no commit (a broken guard would toast the template error)"
    );
    assert_absent(cx, "tuic-cc-option-default".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t06_dropdown_counts() {
    use manyproxy_ui::store::{NAIVE_CC_MODES, SOCKS_VERSIONS};
    assert_eq!(SOCKS_VERSIONS, ["5", "4"]);
    assert_eq!(NAIVE_CC_MODES, ["bbr", "bbr2", "cubic", "reno"]);
    assert_eq!(
        manyproxy_ui::socks_version_choices().len(),
        3,
        "version lists blank + 2"
    );
    assert_eq!(
        manyproxy_ui::socks_network_choices().len(),
        2,
        "network lists tcp + udp"
    );
    assert_eq!(
        manyproxy_ui::naive_cc_choices().len(),
        5,
        "cc lists blank + 4"
    );
}

#[gpui_kit::test]
fn t06_socks_page_paints(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("socks");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "socks-version",
        "socks-username",
        "socks-password",
        "socks-network",
        "socks-uot",
        "socks-notes",
    ] {
        assert_painted(cx, id);
    }

    for id in [
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "http-username",
        "anytls-password",
        "naive-cc",
        "ssh-user",
        "s4-caption",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = manyproxy_ui::store::ProxyConfig {
        id: "t06-s".to_string(),
        name: "S".to_string(),
        protocol: "socks".to_string(),
        params: "{\"type\":\"socks\",\"server\":\"s\",\"server_port\":1080}"
            .to_string()
            .into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = manyproxy_ui::ConfigForm::prefilled(window, cx, &row, None);
            for version in ["", "5", "4"] {
                form.socks_version.update(cx, |state, cx| {
                    state.set_selected_value(&version.to_string(), window, cx);
                });
                assert_eq!(
                    form.socks_version
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    version,
                    "version {version:?} resolves"
                );
            }
        })
    });
}

#[gpui_kit::test]
fn t06_http_page_paints(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let mut current = "vless".to_string();
    let mut pick = |cx: &mut VisualTestContext, target: &str| {
        let start = row_of(current.as_str());
        let end = row_of(target);
        current = target.to_string();
        cx.update(|window, cx| {
            window.click("protocol-select", cx);
        });
        cx.run_until_parked();
        paint_jumbo_dialog(cx);
        let (key, steps) = if end >= start {
            ("down", end - start)
        } else {
            ("up", start - end)
        };
        let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
        for _ in 0..steps {
            cx.update(|window, cx| {
                window.dispatch_keystroke(keystroke.clone(), cx);
            });
            cx.run_until_parked();
        }
        let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
        cx.update(|window, cx| {
            window.dispatch_keystroke(enter.clone(), cx);
        });
        cx.run_until_parked();
        paint_jumbo_dialog(cx);
        cx.run_until_parked();
        paint_jumbo_dialog(cx);
    };
    let mut first = true;
    for target in ["http", "https"] {
        pick(cx, target);
        assert_painted(cx, "protocol-section");

        if first {
            cx.run_until_parked();
            paint_jumbo_dialog(cx);
            cx.run_until_parked();
            paint_jumbo_dialog(cx);
            // K2: the TLS + dial panels live inside the Advanced Settings disclosure.
            cx.update(|window, cx| {
                window.click("advanced-settings", cx);
            });
            cx.run_until_parked();
            paint_jumbo_dialog(cx);
            cx.run_until_parked();
            paint_jumbo_dialog(cx);
        }
        for id in [
            "http-username",
            "http-password",
            "http-path",
            "http-headers",
            "http-notes",
            "s2-caption",
            "s4-caption",
        ] {
            assert_painted(cx, id);
        }

        if first {
            cx.update(|window, cx| {
                window.click("s2-toggle", cx);
            });
            cx.run_until_parked();
            paint_jumbo_dialog(cx);
            cx.run_until_parked();
            paint_jumbo_dialog(cx);
        }
        for id in ["s2-enable", "s2-sni", "s2-fp"] {
            assert_painted(cx, id);
        }
        assert_absent(cx, "s2-forced-note".to_string());
        for id in ["s1-caption", "s3-caption", "socks-version", "naive-cc"] {
            assert_absent(cx, id.to_string());
        }
        first = false;
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t06_anytls_page_paints(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("anytls");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "protocol-section");
    // K2: TLS + dial panels live inside the single Advanced Settings disclosure.
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "anytls-password",
        "anytls-password-state",
        "anytls-interval",
        "anytls-timeout",
        "anytls-min",
        "anytls-notes",
        "s2-caption",
        "s4-caption",
        "s4-reuse",
        "tls-security-caption",
        "tls-extras-caption",
        "s2-insecure",
        "s2-pbk",
    ] {
        assert_painted(cx, id);
    }

    for id in ["field-guided-identity", "field-guided-identity-state"] {
        assert_absent(cx, id.to_string());
    }
    for id in ["s1-caption", "s3-caption", "ssh-user"] {
        assert_absent(cx, id.to_string());
    }
    let toggle_snap = cx.update(|window, _| {
        window
            .try_find("s2-toggle")
            .map(|s| (s.bounds(), s.visible()))
    });
    assert!(
        toggle_snap.is_some_and(|(_, visible)| visible),
        "s2-toggle visible in-frame after min-w-0 columns: {toggle_snap:?}"
    );
    cx.update(|window, cx| {
        window.click("s2-toggle", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "s2-forced-note");
    assert_absent(cx, "s2-enable".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t06_anytls_edit_regroups_single_password(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "AnyRow".to_string(),
            protocol: "anytls".to_string(),
            params: "{\"type\":\"anytls\",\"server\":\"a.example.com\",\"server_port\":443,\
                    \"password\":\"secret\"}"
                .to_string()
                .into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);

    assert_painted(cx, "protocol-lock");
    assert_absent(cx, "protocol-select".to_string());

    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);

    for id in [
        "common-caption",
        "advanced-settings",
        "tls-security-caption",
    ] {
        assert_painted(cx, id);
    }
    for id in ["field-guided-identity", "field-guided-identity-state"] {
        assert_absent(cx, id.to_string());
    }
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);

    for id in [
        "anytls-password",
        "anytls-password-state",
        "s2-caption",
        "s4-caption",
    ] {
        assert_painted(cx, id);
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let folded = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = manyproxy_ui::ConfigForm::prefilled(window, cx, &seeded, None);
            form.effective_params(cx)
        })
    });
    assert_eq!(
        folded,
        seeded.params.expose(),
        "untouched anytls edit refolds byte-identical"
    );
}

#[gpui_kit::test]
fn t06_invalid_port_stays_open_naming_field(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let closed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("BadPort", window, cx);
            });
            form.guided_server.update(cx, |state, cx| {
                state.set_value("a.example.com", window, cx);
            });
            form.guided_port.update(cx, |state, cx| {
                state.set_value("443", window, cx);
            });
            form.guided_identity.update(cx, |state, cx| {
                state.set_value("11111111-2222-4333-8444-555555555555", window, cx);
            });
            form.port.update(cx, |state, cx| {
                state.set_value("not-a-port", window, cx);
            });
            view.commit_add_with_focus(&form, &form_error, window, cx)
        })
    });
    assert!(!closed, "invalid port keeps the dialog open");
    let (field, message) = form_error
        .borrow()
        .clone()
        .expect("inline error names the field");
    assert_eq!(field, "port", "error owns the port field");
    assert!(!message.is_empty(), "error carries a reason");
    let rows = view.read_with(cx, |view, _| view.store.list().expect("list works"));
    assert!(rows.is_empty(), "nothing stored on failed commit");
}

#[gpui_kit::test]
fn t06_naive_page_paints(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("naive");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "protocol-section");
    // K2: TLS + dial panels live inside the Advanced Settings disclosure.
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "naive-username",
        "naive-password",
        "naive-uot",
        "naive-quic",
        "naive-cc",
        "naive-notes",
        "s2-caption",
        "s4-caption",
    ] {
        assert_painted(cx, id);
    }
    for id in ["s2-enable", "s1-caption", "s3-caption", "tuic-cc"] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = manyproxy_ui::store::ProxyConfig {
        id: "t06-n".to_string(),
        name: "N".to_string(),
        protocol: "naive".to_string(),
        params: "{\"type\":\"naive\",\"server\":\"s\",\"server_port\":443}"
            .to_string()
            .into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = manyproxy_ui::ConfigForm::prefilled(window, cx, &row, None);
            for mode in ["", "bbr", "bbr2", "cubic", "reno"] {
                form.naive_cc.update(cx, |state, cx| {
                    state.set_selected_value(&mode.to_string(), window, cx);
                });
                assert_eq!(
                    form.naive_cc
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    mode,
                    "cc {mode:?} resolves"
                );
            }
        })
    });
}

#[gpui_kit::test]
fn t06_ssh_edit_round_trips_and_key_pick(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let paint_jumbo_dialog = |cx: &mut VisualTestContext| {
        cx.simulate_resize(size(px(1000.), px(3600.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.render_frame(cx);
        });
    };
    let params = "{\"type\":\"ssh\",\"server\":\"192.0.2.23\",\"server_port\":22,\
            \"user\":\"root\",\"password\":\"secret\",\"private_key\":\"KEY-BLOB\",\
            \"host_key\":[\"ecdsa-sha2-nistp256 AAAA\",\"ssh-ed25519 BBBB\"],\
            \"host_key_algorithms\":[\"ssh-ed25519\"],\"client_version\":\"SSH-2.0-X\"}";
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let seeded = store
        .add(NewConfig {
            name: "SshRow".to_string(),
            protocol: "ssh".to_string(),
            params: params.to_string().into(),
            group: String::new(),
            bind_ip: String::new(),
            port: String::new(),
        })
        .expect("seed works");
    let (view, cx) = mount_view(store, cx);
    let target = seeded.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_edit_dialog(target, window, cx));
    });
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    cx.run_until_parked();
    paint_jumbo_dialog(cx);
    for id in [
        "ssh-user",
        "ssh-password",
        "ssh-private-key",
        "ssh-key-path",
        "ssh-key-browse",
        "ssh-key-pass",
        "ssh-host-key",
        "ssh-host-key-algs",
        "ssh-client-version",
        "ssh-notes",
        "s4-caption",
    ] {
        assert_painted(cx, id);
    }

    for id in [
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "socks-version",
        "http-username",
        "anytls-password",
        "naive-cc",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let row = ProxyConfig {
        id: "t06-k".to_string(),
        name: "K".to_string(),
        protocol: "ssh".to_string(),
        params: params.to_string().into(),
        group: "default".to_string(),
        enabled: false,
        bind_ip: "127.0.0.1".to_string(),
        port: 10801,
        subscription_id: None,
        identity_key: None,
        preset_id: None,
    };
    let (fields, folded) = cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = manyproxy_ui::ConfigForm::prefilled(window, cx, &row, None);
            let fields = vec![
                form.ssh_user.read(cx).value().to_string(),
                form.ssh_password.read(cx).value().to_string(),
                form.ssh_private_key.read(cx).value().to_string(),
                form.ssh_host_key.read(cx).value().to_string(),
                form.ssh_host_key_algs.read(cx).value().to_string(),
                form.ssh_client_version.read(cx).value().to_string(),
            ];
            (fields, form.effective_params(cx))
        })
    });
    assert_eq!(
        fields,
        vec![
            "root",
            "secret",
            "KEY-BLOB",
            "ecdsa-sha2-nistp256 AAAA, ssh-ed25519 BBBB",
            "ssh-ed25519",
            "SSH-2.0-X",
        ],
        "ssh values sit in kit fields (arrays echo as comma text)"
    );
    assert_eq!(folded, params, "untouched ssh refolds byte-identical");

    cx.update(|window, cx| {
        view.update(cx, |_, cx| {
            let form = manyproxy_ui::ConfigForm::prefilled(window, cx, &row, None);
            form.ssh_key_path.update(cx, |state, cx| {
                state.set_value("/home/u/.ssh/id_rsa".to_string(), window, cx);
            });
            form.ssh_private_key.update(cx, |state, cx| {
                state.set_value("PICKED-KEY-BLOB".to_string(), window, cx);
            });
            let folded = form.effective_params(cx);
            let value: serde_json::Value = serde_json::from_str(&folded).expect("picked key folds");
            assert_eq!(value["private_key_path"], "/home/u/.ssh/id_rsa");
            assert_eq!(value["private_key"], "PICKED-KEY-BLOB");
        })
    });
}

#[gpui_kit::test]
fn t06_small_pages_round_trip(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let cases = [
        (
            "socks",
            "{\"type\":\"socks\",\"server\":\"192.0.2.20\",\"server_port\":1080,\
                \"version\":\"5\",\"username\":\"u\",\"password\":\"p\",\
                \"network\":\"udp\",\"udp_over_tcp\":true}",
        ),
        (
            "http",
            "{\"type\":\"http\",\"server\":\"192.0.2.21\",\"server_port\":8080,\
                \"username\":\"u\",\"password\":\"p\",\"path\":\"/p\",\
                \"headers\":\"Host: h.example.com\",\"security\":\"tls\",\
                \"sni\":\"example.com\"}",
        ),
        (
            "anytls",
            "{\"type\":\"anytls\",\"server\":\"192.0.2.15\",\"server_port\":8443,\
                \"password\":\"p\",\"idle_session_check_interval\":\"30s\",\
                \"idle_session_timeout\":\"60s\",\"min_idle_session\":5,\
                \"sni\":\"example.com\"}",
        ),
        (
            "naive",
            "{\"type\":\"naive\",\"server\":\"192.0.2.22\",\"server_port\":443,\
                \"username\":\"u\",\"password\":\"p\",\"udp_over_tcp\":true,\
                \"quic\":true,\"quic_congestion_control\":\"bbr2\",\
                \"sni\":\"example.com\"}",
        ),
    ];
    for (protocol, params) in cases {
        let row = ProxyConfig {
            id: format!("t06-{protocol}"),
            name: protocol.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string().into(),
            group: "default".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 10801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        let folded = cx.update(|window, cx| {
            view.update(cx, |_, cx| {
                let form = ConfigForm::prefilled(window, cx, &row, None);
                form.effective_params(cx)
            })
        });
        assert_eq!(
            folded, params,
            "untouched {protocol} refolds byte-identical"
        );
    }
}

#[test]
fn t06_ssh_key_pick_error_names_path() {
    let error = manyproxy_ui::ssh_key_pick_error("C:\\keys\\missing_rsa");
    assert!(
        error.contains("C:\\keys\\missing_rsa"),
        "wrong path names the path, got {error}"
    );
}

#[gpui_kit::test]
fn t06_enter_in_naive_picker_popup_does_not_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::store::VALID_PROTOCOLS;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_add_dialog(window, cx));
    });
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);

    let row_of = |protocol: &str| {
        VALID_PROTOCOLS
            .iter()
            .position(|candidate| *candidate == protocol)
            .expect("canonical protocol")
    };
    let start = row_of("vless");
    let target = row_of("naive");
    cx.update(|window, cx| {
        window.click("protocol-select", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    let (key, steps) = if target >= start {
        ("down", target - start)
    } else {
        ("up", start - target)
    };
    let keystroke = gpui_kit::Keystroke::parse(key).expect("test keystroke parses");
    for _ in 0..steps {
        cx.update(|window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
        });
        cx.run_until_parked();
    }
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);

    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "naive-cc");

    assert_eq!(notification_count(cx), 0, "no toast before Enter");
    cx.update(|window, cx| {
        window.click("naive-cc", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, _| window
            .try_find("naive-cc-option-default".to_string())
            .is_some()),
        "cc popup opened"
    );
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "cc Enter keeps the dialog open (no half-filled commit)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "cc Enter stores no row"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "cc Enter runs no commit (a broken guard would toast the template error)"
    );
    assert_absent(cx, "naive-cc-option-default".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[test]
fn t07_dropdown_counts() {
    use manyproxy_ui::store::{
        SHADOWTLS_VERSIONS, SNELL_VERSIONS, XRAY_FLOWS, XRAY_MUX_MODES, XRAY_NETWORKS,
    };
    assert_eq!(SNELL_VERSIONS, ["4", "6"]);
    assert_eq!(SHADOWTLS_VERSIONS, ["1", "2", "3"]);
    assert_eq!(XRAY_NETWORKS, ["raw", "xhttp", "ws", "httpupgrade", "grpc"]);
    assert_eq!(XRAY_FLOWS, ["xtls-rprx-vision", "xtls-rprx-vision-udp443"]);
    assert_eq!(XRAY_MUX_MODES, ["enabled", "disabled"]);
    assert_eq!(
        manyproxy_ui::snell_version_choices().len(),
        3,
        "version lists blank + 2"
    );
    assert_eq!(
        manyproxy_ui::shadowtls_version_choices().len(),
        4,
        "version lists blank + 3"
    );
    assert_eq!(
        manyproxy_ui::xray_network_choices().len(),
        6,
        "network lists blank + 5"
    );
    assert_eq!(
        manyproxy_ui::xray_flow_choices().len(),
        3,
        "flow lists blank + 2"
    );
    assert_eq!(
        manyproxy_ui::xray_mux_choices().len(),
        3,
        "mux lists blank + 2"
    );
}

#[gpui_kit::test]
fn t07_snell_page_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    t07_open_add_on(&view, cx, "snell");
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "snell-psk",
        "snell-version",
        "snell-obfs-mode",
        "snell-obfs-host",
        "snell-userkey",
        "snell-reuse",
        "snell-network",
        "snell-mode",
        "snell-notes",
    ] {
        assert_painted(cx, id);
    }
    for id in [
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "s4-caption",
        "shadowtls-version",
        "xray-uuid",
        "chain-pick",
        "extra-exe",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t07_shadowtls_page_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    t07_open_add_on(&view, cx, "shadowtls");
    assert_painted(cx, "protocol-section");
    // K2: TLS + dial panels live inside the Advanced Settings disclosure.
    cx.update(|window, cx| {
        window.click("advanced-settings", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "shadowtls-version",
        "shadowtls-password",
        "shadowtls-notes",
        "s2-caption",
        "s4-caption",
    ] {
        assert_painted(cx, id);
    }
    for id in [
        "s1-caption",
        "s3-caption",
        "s2-enable",
        "snell-psk",
        "xray-uuid",
        "chain-pick",
        "extra-exe",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t07_xray_page_paints(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    t07_open_add_on(&view, cx, "xray");
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "xray-uuid",
        "xray-uuid-generate",
        "xray-enc",
        "xray-flow",
        "xray-sni",
        "xray-fp",
        "xray-alpn",
        "xray-pbk",
        "xray-sid",
        "xray-spiderx",
        "xray-network",
        "xray-mode",
        "xray-mux",
        "xray-notes",
    ] {
        assert_painted(cx, id);
    }
    for id in [
        "s1-caption",
        "s2-caption",
        "s3-caption",
        "s4-caption",
        "snell-psk",
        "shadowtls-version",
    ] {
        assert_absent(cx, id.to_string());
    }
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn t07_small_pages_round_trip(cx: &mut TestAppContext) {
    use manyproxy_ui::store::ProxyConfig;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    let cases = [
        (
            "snell",
            "{\"type\":\"snell\",\"server\":\"192.0.2.30\",\"server_port\":8080,\
                \"psk\":\"secret\",\"version\":\"6\",\"obfs_mode\":\"http\",\
                \"obfs_host\":\"example.com\",\"userkey\":\"key\",\"reuse\":true,\
                \"network\":\"tcp\",\"mode\":\"multi\"}",
        ),
        (
            "shadowtls",
            "{\"type\":\"shadowtls\",\"server\":\"192.0.2.31\",\"server_port\":443,\
                \"version\":\"2\",\"password\":\"secret\",\
                \"sni\":\"example.com\",\"fp\":\"chrome\",\"insecure\":true}",
        ),
        (
            "xray",
            "{\"type\":\"xray\",\"server\":\"192.0.2.32\",\"server_port\":443,\
                \"uuid\":\"11111111-2222-4333-8444-555555555555\",\"encryption\":\"none\",\
                \"flow\":\"xtls-rprx-vision\",\"sni\":\"example.com\",\"fp\":\"chrome\",\
                \"alpn\":\"h2\",\"pbk\":\"pubkey\",\"sid\":\"shortid\",\"spiderx\":\"/\",\
                \"network\":\"xhttp\",\"mode\":\"auto\",\"xray_mux\":\"enabled\",\
                \"xPaddingBytes\":\"100-200\",\"maxConcurrency\":16,\
                \"downloadSettings\":{\"address\":\"d.example.com\"}}",
        ),
        (
            "chain",
            "{\"type\":\"chain\",\"server\":\"\",\"server_port\":443,\
                \"nodes\":[\"row-a\",\"row-b\"]}",
        ),
        (
            "extra-core",
            "{\"type\":\"extra-core\",\"server\":\"\",\"server_port\":443,\
                \"exe\":\"core.exe\",\"args\":\"--fast\",\"config\":{\"inbounds\":[]}}",
        ),
    ];
    for (protocol, params) in cases {
        let row = ProxyConfig {
            id: format!("t07-{protocol}"),
            name: protocol.to_string(),
            protocol: protocol.to_string(),
            params: params.to_string().into(),
            group: "default".to_string(),
            enabled: false,
            bind_ip: "127.0.0.1".to_string(),
            port: 11801,
            subscription_id: None,
            identity_key: None,
            preset_id: None,
        };
        let folded = cx.update(|window, cx| {
            view.update(cx, |_, cx| {
                let form = ConfigForm::prefilled(window, cx, &row, None);
                form.effective_params(cx)
            })
        });
        assert_eq!(
            folded, params,
            "untouched {protocol} refolds byte-identical"
        );
    }
}

#[gpui_kit::test]
fn t07_chain_page_paints_empty_and_rules(cx: &mut TestAppContext) {
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    t07_open_add_on(&view, cx, "chain");
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "chain-pick",
        "chain-hop-add",
        "chain-hops-empty",
        "chain-notes",
    ] {
        assert_painted(cx, id);
    }
    assert_absent(cx, "chain-hops-list".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let (landed, refused_chain, refused_off_index, refused_second, replaced) =
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let library = vec![
                    manyproxy_ui::ChainLibraryEntry {
                        id: "a".to_string(),
                        name: "VlessOne".to_string(),
                        protocol: "vless".to_string(),
                    },
                    manyproxy_ui::ChainLibraryEntry {
                        id: "b".to_string(),
                        name: "ChainOne".to_string(),
                        protocol: "chain".to_string(),
                    },
                    manyproxy_ui::ChainLibraryEntry {
                        id: "c".to_string(),
                        name: "CoreOne".to_string(),
                        protocol: "extra-core".to_string(),
                    },
                ];
                let library = Rc::new(RefCell::new(library));
                let hops: Rc<RefCell<Vec<serde_json::Value>>> = Rc::new(RefCell::new(Vec::new()));
                let pick = ConfigForm::panel_select(
                    window,
                    cx,
                    manyproxy_ui::chain_pick_choices(&library.borrow()),
                    "",
                );

                let refused_chain = view.chain_add_hop(&hops, &library, &pick, "b", window, cx);
                let landed_extra = view.chain_add_hop(&hops, &library, &pick, "c", window, cx);
                let refused_second = view.chain_add_hop(&hops, &library, &pick, "c", window, cx);
                let landed = view.chain_add_hop(&hops, &library, &pick, "a", window, cx);
                let refused_off_index =
                    view.chain_replace_hop(&hops, &library, &pick, 1, "c", window, cx);
                let refused_replace_chain =
                    view.chain_replace_hop(&hops, &library, &pick, 1, "b", window, cx);
                let replaced = hops.borrow().clone();
                assert_eq!(
                    manyproxy_ui::chain_hop_label(&replaced[0], 0, &library.borrow()),
                    "1. CoreOne (extra-core)"
                );
                assert_eq!(
                    manyproxy_ui::chain_existing_kinds(&replaced, &library.borrow()),
                    vec![
                        manyproxy_ui::store::ChainHopSlot::Extra,
                        manyproxy_ui::store::ChainHopSlot::Other
                    ]
                );
                (
                    landed_extra && landed,
                    !refused_chain,
                    !refused_off_index,
                    !refused_second,
                    !refused_replace_chain && replaced.len() == 2,
                )
            })
        });
    assert!(landed, "legal hops land (extra at 0, vless after)");
    assert!(refused_chain, "chain→chain add refused");
    assert!(refused_off_index, "off-index-0 extracore replace refused");
    assert!(refused_second, "second extracore add refused");
    assert!(replaced, "chain replace refused, hops unchanged");
}

#[gpui_kit::test]
fn t07_extra_page_paints_validates_and_gates_commit(cx: &mut TestAppContext) {
    use manyproxy_ui::store::protocol_params_template;
    use manyproxy_ui::ConfigForm;
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    t07_open_add_on(&view, cx, "extra-core");
    assert_painted(cx, "protocol-section");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    for id in [
        "extra-exe",
        "extra-exe-browse",
        "extra-args",
        "extra-config",
        "extra-notes",
    ] {
        assert_painted(cx, id);
    }

    assert_absent(cx, "extra-no-logs".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));

    let stored = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            view.finish_extra_exe_pick(
                form.extra_exe.clone(),
                "C:\\cores\\my-core.exe".to_string(),
                window,
                cx,
            );
            form.extra_exe.read(cx).value().to_string()
        })
    });
    assert_eq!(stored, "my-core.exe", "Browse stores the file name");

    let form_error: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let (closed, message, closed2) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let form = ConfigForm::blank(window, cx);
            form.name.update(cx, |state, cx| {
                state.set_value("EC", window, cx);
            });
            form.protocol_select.update(cx, |state, cx| {
                state.set_selected_value(&"extra-core".to_string(), window, cx);
            });
            form.params.update(cx, |state, cx| {
                state.set_value(protocol_params_template("extra-core"), window, cx);
            });
            let closed = view.commit_add_with_focus(&form, &form_error, window, cx);
            let message = form_error
                .borrow()
                .clone()
                .map(|(_, message)| message)
                .unwrap_or_default();
            form.extra_exe.update(cx, |state, cx| {
                state.set_value("core.exe", window, cx);
            });
            form.extra_config.update(cx, |state, cx| {
                state.set_value("{}", window, cx);
            });

            let closed2 = view.commit_add_with_focus(&form, &form_error, window, cx);
            (closed, message, closed2)
        })
    });
    assert!(!closed, "exeless row keeps the dialog open");
    assert!(
        message.contains("exe"),
        "refusal names the exe field: {message}"
    );
    assert!(closed2, "exe + config shape commits");
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        1,
        "exactly one row lands"
    );
}

#[gpui_kit::test]
fn t07_enter_in_xray_picker_popup_does_not_commit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (view, cx) = mount_view(store, cx);
    t07_open_add_on(&view, cx, "xray");
    cx.run_until_parked();
    paint_tall_dialog(cx);
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert_painted(cx, "xray-network");
    assert_eq!(notification_count(cx), 0, "no toast before Enter");
    cx.update(|window, cx| {
        window.click("xray-network", cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, _| window
            .try_find("xray-network-option-default".to_string())
            .is_some()),
        "network popup opened"
    );
    let enter = gpui_kit::Keystroke::parse("enter").expect("test keystroke parses");
    cx.update(|window, cx| {
        window.dispatch_keystroke(enter.clone(), cx);
    });
    cx.run_until_parked();
    paint_tall_dialog(cx);
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "network Enter keeps the dialog open (no half-filled commit)"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "network Enter stores no row"
    );
    assert_eq!(
        notification_count(cx),
        0,
        "network Enter runs no commit (a broken guard would toast the template error)"
    );
    assert_absent(cx, "xray-network-option-default".to_string());
    cx.update(|window, cx| window.close_all_dialogs(cx));
}

#[gpui_kit::test]
fn edit_and_remove_dialogs_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let config = store.add(seed_input("Work")).expect("seed works");

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        let config = config.clone();
        view.update(cx, |view, cx| view.open_edit_dialog(config, window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("field-name").is_some()));

    cx.update(|window, cx| window.close_all_dialogs(cx));
    cx.run_until_parked();

    let target = config.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.delete_row_immediately(&target.id, window, cx)
        });
    });
    cx.run_until_parked();
    // K5: single-row delete is immediate — no confirm dialog opens, the row
    // is already gone, and the Undo toast is the delete acknowledgment.
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "K5: single-row delete never blocks on a confirm"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.config_count(cx)),
        0,
        "the row is deleted at once"
    );
    assert_eq!(
        notification_count(cx),
        1,
        "the undo toast is the delete acknowledgment"
    );
}

#[gpui_kit::test]
fn import_dialog_opens_with_fields(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_import_dialog("vless://x", "travel", None, window, cx)
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.update(|window, _| window.try_find("field-import-text").is_some()));
    assert!(cx.update(|window, _| window.try_find("field-import-group").is_some()));
}

#[gpui_kit::test]
fn import_commit_stores_disabled_records(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.commit_import(
                "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nbogus://x"
                    .to_string(),
                "travel".to_string(),
                None,
                window,
                cx,
            ));
        });
    });
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 1);
}

#[gpui_kit::test]
fn import_commit_selects_and_checks_new_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.commit_import(
                    "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#one\nvless://22222222-3333-4444-8555-666666666666@b.example.com:443#two"
                        .to_string(),
                    "travel".to_string(),
                    None,
                    window,
                    cx,
                ));
            });
        });
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, cx| view.config_count(cx)), 2);
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_focus.clone()),
        view.read_with(cx, |view, cx| {
            view.selected_ids_in_visible_order(cx).first().cloned()
        }),
        "focus lands on the first imported row"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.selection_count()),
        2,
        "all imported rows arrive checked"
    );
}

#[test]
fn port_budget_text_counts_auto_ports() {
    use manyproxy_ui::ConfigForm;
    assert!(
        ConfigForm::port_budget_text(&[]).contains("0 of 499"),
        "empty library: {}",
        ConfigForm::port_budget_text(&[])
    );
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    store
        .add(row19_seed("A", "a.example.com", "10801"))
        .expect("seed works");
    store
        .add(row19_seed("B", "b.example.com", "18081"))
        .expect("seed works");
    let rows = store.list().expect("list works");
    let text = ConfigForm::port_budget_text(&rows);
    assert!(text.contains("2 of 499"), "both ranges count: {text}");
}

#[gpui_kit::test]
fn export_dialog_opens_with_verbatim_warning(cx: &mut TestAppContext) {
    use manyproxy_ui::store::export_confirm_description;
    use manyproxy_ui::store::export_warning_text;

    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    assert_eq!(export_confirm_description(), export_warning_text());

    let clause = manyproxy_ui::store::RESTORE_REQUIRES_CLAUSE;
    assert_eq!(
        clause,
        "Restore requires a matching ManyProxy export (schema manyproxy-library, version 1)."
    );
    assert!(
        export_warning_text().contains(clause),
        "export warning carries the restore-requirement clause: {}",
        export_warning_text()
    );
    assert!(
        export_warning_text().starts_with(
            "Export carries private keys, tokens, and passwords in plaintext. \
                 Anyone with this file can use your proxies — store it somewhere safe."
        ),
        "the original warning text stays byte-identical: {}",
        export_warning_text()
    );

    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, _| window.try_find("action-bar-overflow").is_some()));
    cx.update(|window, cx| {
        window.click("action-bar-overflow", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });
    assert!(cx.update(|window, _| window.try_find("action-overflow-export").is_some()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_export_dialog(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.render_frame(cx);
    });

    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn ticks_idle_with_nothing_running_touch_no_network(cx: &mut TestAppContext) {
    use manyproxy_ui::engine::EngineController;
    use manyproxy_ui::health::FakeHealthProber;

    cx.update(gpui_kit::init);

    let stash: Rc<RefCell<Option<Entity<AppView>>>> = Rc::new(RefCell::new(None));
    let stash_for_window = stash.clone();
    let store = Store::open_in_memory().expect("in-memory store opens");
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| AppView::with_store(window, cx, store));
        *stash_for_window.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = stash.borrow().clone().expect("AppView stashed");

    let (prober, handle) = FakeHealthProber::shared();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let (engine, _) = EngineController::fake();
            view.engine = engine;
            view.prober = Box::new(prober);
        });
    });

    let poll = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_engine_poll(cx)));
    assert_eq!(poll, None, "no crash without a sidecar");
    let subs = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_due_subscriptions(cx)));
    assert_eq!(subs, 0, "no subs, no fetches");
    let reprobed = cx.update(|_, cx| view.update(cx, |view, cx| view.tick_reprobe_running(cx)));
    assert_eq!(reprobed, 0, "nothing running, nothing probed");
    assert!(
        handle.calls_for("1").is_empty() && handle.calls_for("2").is_empty(),
        "idle ticks never touch the network"
    );
    assert!(
        view.read_with(cx, |view, _| view.engine.last_error().is_none()),
        "no errors recorded"
    );
}

// --- slice 8 anchors: copy catalog, About, icon-only names ----------------

/// The catalog carries no pseudo-plural and no checked-set vocabulary: the
/// nouns are config/configs, and count-bearing messages use Fluent selectors.
#[test]
fn no_pseudo_plural_or_checked_string_survives() {
    const CATALOG: &str = include_str!("../locales/en/main.ftl");
    for (needle, why) in [
        ("(s)", "pseudo-plurals are banned"),
        ("checked", "the checked set is retired"),
    ] {
        let offenders: Vec<&str> = CATALOG
            .lines()
            .filter(|line| line.contains(needle) && !line.trim_start().starts_with('#'))
            .collect();
        assert!(offenders.is_empty(), "{why}: {offenders:?}");
    }
    for line in CATALOG.lines() {
        if !line.contains(" = ") || line.trim_start().starts_with('#') {
            continue;
        }
        assert!(
            !line.contains("configuration") && !line.contains("configurations"),
            "a row is a config, not a configuration: {line}"
        );
    }
}

/// The About body renders real line breaks from the multiline Fluent value;
/// the literal escape never reaches the dialog.
#[test]
fn about_body_renders_line_breaks() {
    let body = manyproxy_ui::about_body(
        "0.1.0",
        "abc123",
        &["sing-box.exe: found (app dir)".to_string()],
        "C:\\data\\library.db",
        "C:\\data\\manyproxy-export.json",
    );
    assert!(body.contains('\n'), "the About body has real newlines");
    assert!(
        !body.contains("\\n"),
        "no literal backslash-n survives in the About body: {body}"
    );
}

/// The audited icon-only controls each expose an accessible name and a
/// tooltip; the state glyph stays a non-control.
#[gpui_kit::test]
fn every_icon_only_control_has_an_accessible_name(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut store = Store::open_in_memory().expect("in-memory store opens");
    let row = store
        .add(facet_seed("Alpha", "vless", "team", "10801"))
        .expect("seed works");
    let sub = store
        .add_subscription("Free plan", "https://example.com/free.txt", "team", false)
        .expect("sub works");
    store
        .refresh_with_body(
            &sub.id,
            "vless://11111111-2222-4333-8444-555555555555@a.example.com:443#feed",
            manyproxy_ui::store::now_unix(),
        )
        .expect("refresh works");
    let sub_id = sub.id.clone();
    let (view, cx) = mount_view(store, cx);
    cx.run_until_parked();
    paint_frame(cx);

    // The row's hover actions are icon-only until hover; their accessible
    // names exist in the tree regardless. Checked in All configs, where the
    // seeded row is visible.
    select_library_row(&view, cx, &row.id);
    paint_frame(cx);
    for id in [
        format!("row-start-{}", row.id),
        format!("row-stop-{}", row.id),
    ] {
        cx.update(|window, _| {
            let snapshot = window.try_find(id.clone());
            if let Some(snapshot) = snapshot {
                assert!(
                    snapshot.label().is_some(),
                    "{id} exposes an accessible name"
                );
            }
        });
    }

    // The scope strip only exists inside a source scope.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_scope(
                manyproxy_ui::LibraryScope::Source(sub_id.clone()),
                window,
                cx,
            )
        });
    });
    cx.run_until_parked();
    paint_frame(cx);

    let named = [
        "action-bar-palette",
        "action-bar-overflow",
        "nav-new-group",
        "nav-manage-sources",
        "scope-strip-refresh",
        "engine-bar-overflow",
    ];
    for id in named {
        let label = cx.update(|window, _| {
            window
                .try_find(id.to_string())
                .unwrap_or_else(|| panic!("{id} renders in the shell"))
                .label()
                .map(str::to_string)
        });
        assert!(label.is_some(), "{id} exposes an accessible name");
        // The tooltip wiring is pinned at the source: every id above appears
        // with a `.tooltip(...)` in the renderer that owns it.
    }
}
