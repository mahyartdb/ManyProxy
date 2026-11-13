use crate::*;

use gpui_kit::*;

/// The context the shortcuts below live in; the shell sets it as its
/// `key_context`, so a focused text input still wins.
pub(crate) const APP_KEY_CONTEXT: &str = "manyproxy-library";

/// The list's own key context, reached from the app's. A binding that names
/// this descendant matches deeper than one that names the app alone, so it
/// outranks the kit's list bindings.
pub(crate) const LIST_CONTEXT: &str = "manyproxy-library > List";

/// Binds one table action's keystrokes. The keystrokes live in the action
/// table, so a shortcut cannot exist here and nowhere else.
fn bind<A: Action + Clone>(out: &mut Vec<KeyBinding>, id: &str, event: A) {
    for &(keystroke, _) in action(id).shortcuts {
        out.push(KeyBinding::new(
            keystroke,
            event.clone(),
            Some(APP_KEY_CONTEXT),
        ));
    }
}

pub(crate) fn bind_app_keys(cx: &mut App) {
    let mut keys: Vec<KeyBinding> = Vec::new();
    bind(&mut keys, "add", AddConfig);
    bind(&mut keys, "import", ImportConfigs);
    bind(&mut keys, "paste-import", PasteImport);
    bind(&mut keys, "start", StartSelected);
    bind(&mut keys, "stop", StopSelected);
    bind(&mut keys, "test", TestSelected);
    bind(&mut keys, "stop-testing", StopTesting);
    bind(&mut keys, "clear-selection", ClearSelection);
    bind(&mut keys, "open-palette", OpenPalette);
    bind(&mut keys, "copy-urls", CopyUrls);
    bind(&mut keys, "update-current-group", UpdateCurrentGroup);
    bind(&mut keys, "edit-endpoint", EditEndpoint);
    bind(&mut keys, "edit", EditSelected);
    bind(&mut keys, "delete", DeleteSelected);
    // Multi-select growth is app-level: the kit's List owns plain Up/Down, and
    // these modifiers never reach its bindings.
    keys.push(KeyBinding::new(
        "shift-up",
        SelectPreviousRow,
        Some(APP_KEY_CONTEXT),
    ));
    keys.push(KeyBinding::new(
        "shift-down",
        SelectNextRow,
        Some(APP_KEY_CONTEXT),
    ));
    // The plain arrows move the focus and the selection with it. The binding
    // names the list as a descendant of the app, so it outranks the kit's own
    // arrow bindings: the list never moves on its own.
    keys.push(KeyBinding::new("up", FocusPreviousRow, Some(LIST_CONTEXT)));
    keys.push(KeyBinding::new("down", FocusNextRow, Some(LIST_CONTEXT)));
    keys.push(KeyBinding::new(
        "home",
        FocusFirstRow,
        Some(APP_KEY_CONTEXT),
    ));
    keys.push(KeyBinding::new("end", FocusLastRow, Some(APP_KEY_CONTEXT)));
    keys.push(KeyBinding::new(
        "space",
        ToggleFocused,
        Some(APP_KEY_CONTEXT),
    ));
    keys.push(KeyBinding::new(
        "ctrl-a",
        SelectAllRows,
        Some(APP_KEY_CONTEXT),
    ));
    // The kit's List binds Escape to Cancel at `List`; a focused list would
    // otherwise swallow the app's clear-selection. The descendant context
    // outranks the kit's, so Escape clears wherever the table has the keyboard.
    keys.push(KeyBinding::new(
        "escape",
        ClearSelection,
        Some(LIST_CONTEXT),
    ));
    // The search field is not inside the list context, so it needs the same
    // layered handler one scope up.
    keys.push(KeyBinding::new(
        "escape",
        ClearSelection,
        Some(APP_KEY_CONTEXT),
    ));
    bind(&mut keys, "focus-search", FocusSearch);
    bind(&mut keys, "inspector-details", InspectorDetails);
    bind(&mut keys, "inspector-health", InspectorHealth);
    bind(&mut keys, "inspector-logs", InspectorLogs);
    bind(&mut keys, "inspector-links", InspectorLinks);
    bind(&mut keys, "inspector-traffic", InspectorTraffic);
    bind(&mut keys, "inspector-toggle", InspectorToggle);
    cx.bind_keys(keys);
}
