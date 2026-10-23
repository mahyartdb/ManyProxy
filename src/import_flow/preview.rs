use crate::*;

#[cfg(test)]
pub(crate) fn import_preview_counts(text: &str, file_hint: Option<&str>) -> (usize, usize) {
    if text.trim().is_empty() {
        return (0, 0);
    }
    let batch = match effective_hint_for_text(file_hint, text) {
        Some(name) => import_file_content(&name, text),
        None => import_text(text),
    };
    (batch.configs.len(), batch.failures.len())
}

pub(crate) fn preview_batch_for_form(
    text: &str,
    file_hint: Option<String>,
    original: Option<String>,
    multi: Option<Vec<(String, String)>>,
) -> crate::import::BatchResult {
    if text.trim().is_empty() {
        return crate::import::BatchResult::default();
    }
    let resolved =
        resolve_import_inputs(text.to_string(), String::new(), file_hint, original, multi);
    parse_import_inputs(&resolved)
}
