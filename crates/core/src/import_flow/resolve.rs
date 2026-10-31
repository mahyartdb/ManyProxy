use crate::*;

pub struct ResolvedImport {
    pub text: String,
    pub group: String,
    pub file_hint: Option<String>,
    pub multi: Option<Vec<(String, String)>>,
}

pub fn resolve_import_inputs(
    text: String,
    group: String,
    file_hint: Option<String>,
    original: Option<String>,
    multi: Option<Vec<(String, String)>>,
) -> ResolvedImport {
    let edited = match original.as_deref() {
        Some(prefill) => text.trim() != prefill.trim(),
        None => false,
    };
    let multi = if edited { None } else { multi };
    let file_hint = if edited {
        None
    } else {
        effective_hint_for_text(file_hint.as_deref(), &text)
    };
    ResolvedImport {
        text,
        group,
        file_hint,
        multi,
    }
}

pub fn parse_import_inputs(resolved: &ResolvedImport) -> crate::import::BatchResult {
    if let Some(files) = &resolved.multi {
        if resolved.file_hint.is_none() {
            let refs: Vec<(&str, &str)> = files
                .iter()
                .map(|(name, content)| (name.as_str(), content.as_str()))
                .collect();
            return import_multi_file_contents(&refs);
        }
    }
    match &resolved.file_hint {
        Some(name) => import_file_content(name, &resolved.text),
        None => import_text(&resolved.text),
    }
}

pub enum ImportFileRead {
    Text(String),
    Unreadable,
    Refused(&'static str),
}

pub fn read_import_file(path: &std::path::Path) -> ImportFileRead {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return ImportFileRead::Unreadable,
    };
    if !import_file_size_ok(bytes.len() as u64) {
        return ImportFileRead::Refused("over 50MB");
    }
    if let Some(reason) = reject_file_bytes(&bytes) {
        return ImportFileRead::Refused(reason);
    }
    match String::from_utf8(bytes) {
        Ok(content) => ImportFileRead::Text(content),
        Err(_) => ImportFileRead::Unreadable,
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn first_import_failure_reason(text: &str, file_hint: Option<&str>) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let batch = match effective_hint_for_text(file_hint, text) {
        Some(name) => import_file_content(&name, text),
        None => import_text(text),
    };
    batch.failures.first().map(|failure| {
        format!(
            "item {} ({}): {}",
            failure.index + 1,
            failure.label,
            failure.reason
        )
    })
}

pub type BrowsedImport = (String, Option<String>, Option<Vec<(String, String)>>);

pub fn merge_browse_results(existing: &str, files: &[(String, String)]) -> BrowsedImport {
    let mut merged = existing.to_string();
    for (_, content) in files {
        if !merged.trim().is_empty() {
            merged.push('\n');
        }
        merged.push_str(content);
    }
    if files.len() == 1 {
        let hint = files.first().map(|(name, _)| name.clone());
        (merged, hint, None)
    } else if files.len() > 1 {
        (merged, None, Some(files.to_vec()))
    } else {
        (merged, None, None)
    }
}
