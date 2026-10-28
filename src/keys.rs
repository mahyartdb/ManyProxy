use crate::*;

use gpui_kit::*;

/// The context the shortcuts below live in; the shell sets it as its
/// `key_context`, so a focused text input still wins.
pub(crate) const APP_KEY_CONTEXT: &str = "manyproxy-library";

/// Binds one table action's keystrokes. The keystrokes live in the action
/// table, so a shortcut cannot exist here and nowhere else.
fn bind<A: Action + Clone>(out: &mut Vec<KeyBinding>, id: &str, event: A) {
    for &(keystroke, _) in action(id).shortcuts {
        out.push(KeyBinding::new(keystroke, event.clone(), Some(APP_KEY_CONTEXT)));
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
    bind(&mut keys, "clear-checked", ClearChecked);
    bind(&mut keys, "open-palette", OpenPalette);
    bind(&mut keys, "copy-urls", CopyUrls);
    bind(&mut keys, "update-current-group", UpdateCurrentGroup);
    bind(&mut keys, "edit", EditSelected);
    bind(&mut keys, "delete", DeleteSelected);
    bind(&mut keys, "focus-search", FocusSearch);
    bind(&mut keys, "inspector-details", InspectorDetails);
    bind(&mut keys, "inspector-health", InspectorHealth);
    bind(&mut keys, "inspector-logs", InspectorLogs);
    bind(&mut keys, "inspector-links", InspectorLinks);
    bind(&mut keys, "inspector-traffic", InspectorTraffic);
    cx.bind_keys(keys);
}
