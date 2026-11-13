pub const ENGINE_POLL_TICK_SECS: u64 = 5;
pub const DUE_SUBSCRIPTION_TICK_SECS: u64 = 300;
pub const REPROBE_TICK_SECS: u64 = 60;
pub const TRAFFIC_TICK_SECS: u64 = 5;
pub const TRAFFIC_FAST_TICK_SECS: u64 = 1;
pub const TRAY_PUMP_MILLIS: u64 = 500;
pub const STARTUP_REFRESH_DEFER_SECS: u64 = 3;
pub const APP_TITLE: &str = "ManyProxy";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HASH: &str = match option_env!("MANYPROXY_GIT_HASH") {
    Some(hash) => hash,
    None => "unknown (dev build)",
};
/// T-04: the complete shortcut reference. Every entry here mirrors one
/// `KeyBinding` in `bind_app_keys` (`src/keys.rs`) — the Settings → Shortcuts
/// section and the reference below read from this ONE list, so the two can
/// never drift apart. The order follows the binding table.
pub const SHORTCUT_LINES: &[(&str, &str)] = &[
    ("Add configuration", "Ctrl-N"),
    ("Import configurations", "Ctrl-O"),
    ("Paste clipboard into Import", "Ctrl-V"),
    ("Start selected", "F5"),
    ("Stop selected", "Shift-F5"),
    ("Test selected", "Ctrl-T"),
    ("Stop Testing", "Ctrl-."),
    ("Edit endpoint", "Ctrl-E"),
    ("Edit config", "Ctrl-Shift-E"),
    ("Delete selected", "Delete"),
    ("Extend selection up", "Shift-Up"),
    ("Extend selection down", "Shift-Down"),
    ("Move focus up", "Up"),
    ("Move focus down", "Down"),
    ("Move focus to first row", "Home"),
    ("Move focus to last row", "End"),
    ("Toggle focused row", "Space"),
    ("Select all visible", "Ctrl-A"),
    ("Focus search", "Ctrl-F"),
    ("Toggle inspector", "Ctrl-I"),
    ("Toggle console", "Ctrl-`"),
    ("Clear selection", "Esc"),
    ("Copy URLs", "Ctrl-U"),
    ("Command palette", "Ctrl-K"),
    ("Update the current scope", "Ctrl-R"),
];
