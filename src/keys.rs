use gpui_kit::*;

use crate::{
    AddConfig, ClearChecked, CopyUrls, DeleteSelected, EditSelected, FocusSearch, ImportConfigs,
    InspectorConnections, InspectorDetails, InspectorHealth, InspectorLogs, InspectorPresets,
    InspectorTraffic, OpenPalette, PasteImport, StartSelected, StopSelected, StopTesting,
    TestSelected, UpdateCurrentGroup,
};

pub(crate) const APP_KEY_CONTEXT: &str = "manyproxy-library";
pub(crate) fn bind_app_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-n", AddConfig, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-o", ImportConfigs, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-v", PasteImport, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("f5", StartSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("shift-f5", StopSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-t", TestSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("escape", ClearChecked, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-e", EditSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("delete", DeleteSelected, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-f", FocusSearch, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-1", InspectorDetails, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-2", InspectorHealth, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-3", InspectorPresets, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-4", InspectorLogs, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-5", InspectorConnections, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-6", InspectorTraffic, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-k", OpenPalette, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-u", CopyUrls, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-r", UpdateCurrentGroup, Some(APP_KEY_CONTEXT)),
        KeyBinding::new("ctrl-.", StopTesting, Some(APP_KEY_CONTEXT)),
    ]);
}
