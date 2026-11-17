use crate::*;

/// `Keystroke::parse` takes a `'static str`, so the chip builder does too;
/// every caller names its keystroke as a literal.
pub fn shortcut_kbd(keystroke: &'static str, fallback: &str) -> AnyElement {
    match Keystroke::parse(keystroke) {
        Ok(stroke) => Kbd::new(stroke).into_any_element(),
        Err(_) => fallback.to_string().into_any_element(),
    }
}
