pub(crate) const ENGINE_POLL_TICK_SECS: u64 = 5;
pub(crate) const DUE_SUBSCRIPTION_TICK_SECS: u64 = 300;
pub(crate) const REPROBE_TICK_SECS: u64 = 60;
pub(crate) const TRAFFIC_TICK_SECS: u64 = 5;
pub(crate) const TRAFFIC_FAST_TICK_SECS: u64 = 1;
pub(crate) const TRAY_PUMP_MILLIS: u64 = 500;
pub(crate) const STARTUP_REFRESH_DEFER_SECS: u64 = 3;
pub(crate) const APP_TITLE: &str = "ManyProxy";
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const GIT_HASH: &str = match option_env!("MANYPROXY_GIT_HASH") {
    Some(hash) => hash,
    None => "unknown (dev build)",
};
pub(crate) const SHORTCUT_LINES: &[(&str, &str)] = &[
    ("Add configuration", "Ctrl-N"),
    ("Import configurations", "Ctrl-O"),
    ("Paste clipboard into Import", "Ctrl-V"),
    ("Start selected", "F5"),
    ("Stop selected", "Shift-F5"),
    ("Test selected", "Ctrl-T"),
    ("Stop Testing", "Ctrl-."),
    ("Edit selected", "Ctrl-E"),
    ("Delete selected", "Delete"),
    ("Focus search", "Ctrl-F"),
    ("Inspector details", "Ctrl-1"),
    ("Inspector health", "Ctrl-2"),
    ("Inspector presets", "Ctrl-3"),
    ("Uncheck all", "Esc"),
    ("Copy URLs", "Ctrl-U"),
    ("Command palette", "Ctrl-K"),
];
