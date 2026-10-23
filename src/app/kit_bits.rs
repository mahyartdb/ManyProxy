use crate::*;

pub(crate) fn shortcut_kbd(keystroke: &str, fallback: &str) -> AnyElement {
    match Keystroke::parse(keystroke) {
        Ok(stroke) => Kbd::new(stroke).into_any_element(),
        Err(_) => fallback.to_string().into_any_element(),
    }
}

pub(crate) type EchoSelect<'a> = (
    &'a Entity<SelectState<PanelChoiceList>>,
    &'a [&'a str],
    &'a str,
);
