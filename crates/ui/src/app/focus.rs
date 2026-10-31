use crate::*;

pub(crate) fn overlay_open(window: &mut Window, cx: &mut App) -> bool {
    window.has_active_dialog(cx) || window.has_active_sheet(cx)
}

/// T-06: the copy a refused shortcut shows, so the key never looks dead.
pub(crate) const OVERLAY_REFUSAL_NOTE: &str = "Close the open dialog first";

/// T-06: a shortcut that cannot run because an overlay owns the keyboard says
/// so out loud instead of looking like a dead key.
pub(crate) fn refuse_shortcut_under_overlay(window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(OVERLAY_REFUSAL_NOTE), cx);
}

/// T-06: the shell's focus handle, published once per window, so every
/// overlay-dismiss path can hand keyboard focus back without threading a
/// `FocusHandle` through each dialog builder and footer closure.
pub(crate) struct ShellFocus {
    pub(crate) window: WindowId,
    pub(crate) handle: FocusHandle,
}

impl Global for ShellFocus {}

/// T-06: the end of every overlay-dismiss path. The shell reclaims keyboard
/// focus only when no overlay is open and no element holds focus - a reclaim
/// that fires while the user has already clicked elsewhere, or while a second
/// overlay has just opened, steals the keystroke they were aiming.
pub(crate) fn reclaim_shell_focus(window: &mut Window, cx: &mut App) {
    if overlay_open(window, cx) {
        return;
    }
    if window.focused(cx).is_some() {
        return;
    }
    focus_shell(window, cx);
}

/// T-06: before an overlay opens, the shell takes focus when nothing else
/// holds it. `Root::open_dialog` remembers `window.focused(cx)` and hands that
/// handle back when the layer closes, so this is what makes Esc, a backdrop
/// press, the close button and a footer commit return keyboard control to the
/// shell instead of leaving the window with a dead focus id.
pub(crate) fn focus_shell_before_overlay(window: &mut Window, cx: &mut App) {
    if window.focused(cx).is_none() {
        focus_shell(window, cx);
    }
}

pub(crate) fn focus_shell(window: &mut Window, cx: &mut App) {
    let Some(shell) = cx.try_global::<ShellFocus>() else {
        return;
    };
    if shell.window != window.window_handle().window_id() {
        return;
    }
    let handle = shell.handle.clone();
    window.focus(&handle, cx);
}
