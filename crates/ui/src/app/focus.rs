use crate::*;

pub fn overlay_open(window: &mut Window, cx: &mut App) -> bool {
    window.has_active_dialog(cx) || window.has_active_sheet(cx)
}

/// T-06: the copy a refused shortcut shows, so the key never looks dead.
pub fn overlay_refusal_note() -> String {
    t!("overlay-refusal-note")
}

/// T-06: a shortcut that cannot run because an overlay owns the keyboard says
/// so out loud instead of looking like a dead key. The note names the key, so
/// "Ctrl+N" reads as refused, not as a generic scold.
pub fn refuse_shortcut_under_overlay(window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(overlay_refusal_note()), cx);
}

pub fn refusal_note_for(stroke: &str) -> String {
    t!("overlay-refusal-for-key", key = stroke)
}

/// T-06 (gpui 0.3.12): a dialog renders in a Root plugin overlay that is a
/// sibling of the app shell, so the shell's key context is not on the dispatch
/// path while one is open and an app shortcut matches no binding at all — it
/// falls through as an unbound keystroke instead of reaching the shell's
/// refusal handler. This observer runs after binding resolution and speaks for
/// exactly that fall-through: a keystroke that spells one of the action
/// table's shortcuts (menu/palette verbs) while an overlay owns focus. The
/// keymap-only strokes outside the action table (space, ctrl-a, arrows) fall
/// through silently under an overlay. Keys the overlay consumed (its own
/// Escape, text input) carry a handler or an action and never reach here.
/// The shortcut list is hoisted once — this runs on every keystroke, and the
/// app is Windows-only (tray-icon, winreg, DPAPI), so the host OS is pinned.
pub fn observe_refused_shortcuts(cx: &mut App) -> Subscription {
    let bound = bound_shortcuts();
    cx.observe_keystrokes(move |event, window, cx| {
        refuse_app_shortcut_with(event, window, cx, &bound)
    })
}

pub(crate) fn refuse_app_shortcut_with(
    event: &KeystrokeEvent,
    window: &mut Window,
    cx: &mut App,
    bound: &[(&'static str, &'static str)],
) {
    if event.action.is_some() {
        return;
    }
    if !overlay_open(window, cx) {
        return;
    }
    // The keystroke must spell one of the app's own shortcuts; any other
    // unbound key under an overlay is that surface's business. The toast
    // shows the display chip ("Ctrl+N"), not the binding syntax.
    let stroke = event.keystroke.unparse(OperatingSystem::Windows);
    if let Some(note) = refusal_note_for_bound_stroke(&stroke, bound) {
        window.push_notification(Notification::warning(note), cx);
    }
}

/// The stroke→display-chip→note chain, kept in one function so the chip
/// half of `bound` is pinned by a test — passing the raw stroke here would
/// silently produce "ctrl-n" in the toast and still compile.
pub fn refusal_note_for_bound_stroke(
    stroke: &str,
    bound: &[(&'static str, &'static str)],
) -> Option<String> {
    bound
        .iter()
        .find(|(candidate, _)| *candidate == stroke)
        .map(|(_, chip)| refusal_note_for(chip))
}

fn bound_shortcuts() -> Vec<(&'static str, &'static str)> {
    ACTIONS
        .iter()
        .flat_map(|def| def.shortcuts.iter().copied())
        .collect()
}

/// T-06: the shell's focus handle, published once per window, so every
/// overlay-dismiss path can hand keyboard focus back without threading a
/// `FocusHandle` through each dialog builder and footer closure.
pub struct ShellFocus {
    pub window: WindowId,
    pub handle: FocusHandle,
}

impl Global for ShellFocus {}

/// T-06: the end of every overlay-dismiss path. The shell reclaims keyboard
/// focus only when no overlay is open and no element holds focus - a reclaim
/// that fires while the user has already clicked elsewhere, or while a second
/// overlay has just opened, steals the keystroke they were aiming.
pub fn reclaim_shell_focus(window: &mut Window, cx: &mut App) {
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
pub fn focus_shell_before_overlay(window: &mut Window, cx: &mut App) {
    if window.focused(cx).is_none() {
        focus_shell(window, cx);
    }
}

pub fn focus_shell(window: &mut Window, cx: &mut App) {
    let Some(shell) = cx.try_global::<ShellFocus>() else {
        return;
    };
    if shell.window != window.window_handle().window_id() {
        return;
    }
    let handle = shell.handle.clone();
    window.focus(&handle, cx);
}
