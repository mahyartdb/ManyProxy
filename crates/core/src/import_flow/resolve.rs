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
            return match typed_prefix(&resolved.text, files) {
                Some(typed) => import_text_and_multi_file_contents(typed, &refs),
                // The text no longer ends with the picked files (edited after
                // the merge): import the files alone rather than parsing their
                // bytes a second time as typed text.
                None => import_multi_file_contents(&refs),
            };
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

/// Recovers the text the user typed before `merge_browse_results` appended
/// the picked file contents. `None` means the text does not end with those
/// file contents, so the caller must not assume the files are separable.
fn typed_prefix<'a>(text: &'a str, files: &[(String, String)]) -> Option<&'a str> {
    let mut rest = text;
    for (_, content) in files.iter().rev() {
        rest = rest.strip_suffix(content.as_str())?;
        // Merge inserts one separator newline before a file; a newline that
        // belongs to the typed text or the previous file stays.
        if let Some(before_newline) = rest.strip_suffix('\n') {
            if !before_newline.trim().is_empty() {
                rest = before_newline;
            }
        }
    }
    Some(rest)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "11111111-2222-4333-8444-555555555555";
    const WG_A: &str = "[Interface]\nPrivateKey = QUJD\n\n[Peer]\nPublicKey = cHVia2V5\nEndpoint = 203.0.113.1:51820\n";
    const WG_B: &str = "[Interface]\nPrivateKey = WFl6\n\n[Peer]\nPublicKey = cHVia2V5Mg\nEndpoint = 203.0.113.2:51820\n";

    fn resolve_browsed(typed: &str, files: &[(&str, &str)]) -> ResolvedImport {
        let files: Vec<(String, String)> = files
            .iter()
            .map(|(name, content)| (name.to_string(), content.to_string()))
            .collect();
        let (merged, hint, multi) = merge_browse_results(typed, &files);
        resolve_import_inputs(merged, String::new(), hint, None, multi)
    }

    #[test]
    fn multi_file_import_keeps_typed_links_once() {
        let typed = format!("vless://{UUID}@typed.example.com:443#typed");
        let files = [
            ("a.conf", WG_A),
            ("b.txt", "trojan://pw@file.example.com:443#from-file"),
        ];
        let batch = parse_import_inputs(&resolve_browsed(&format!("{typed}\n"), &files));

        assert!(batch.failures.is_empty(), "failures: {:?}", batch.failures);
        assert_eq!(
            batch.configs.len(),
            3,
            "typed text is not dropped or doubled"
        );
        let mut names: Vec<&str> = batch
            .configs
            .iter()
            .map(|config| config.name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["a-p1", "from-file", "typed"]);
    }

    #[test]
    fn multi_file_import_keeps_per_file_stems_without_typed_text() {
        let batch =
            parse_import_inputs(&resolve_browsed("", &[("a.conf", WG_A), ("b.conf", WG_B)]));

        assert!(batch.failures.is_empty(), "failures: {:?}", batch.failures);
        let names: Vec<&str> = batch
            .configs
            .iter()
            .map(|config| config.name.as_str())
            .collect();
        assert_eq!(names, ["a-p1", "b-p1"]);
    }

    #[test]
    fn multi_file_import_indices_count_typed_and_file_items() {
        let typed = "bogus://typed";
        let one = format!("vless://{UUID}@a.example.com:443#one");
        let files = [("a.txt", one.as_str()), ("b.txt", "bogus://file")];
        let batch = parse_import_inputs(&resolve_browsed(typed, &files));

        assert_eq!(batch.configs.len(), 1);
        let indices: Vec<usize> = batch.failures.iter().map(|failure| failure.index).collect();
        assert_eq!(
            indices,
            [0, 2],
            "typed item first, then both files in order"
        );
        let text = format_import_failures(&batch.failures);
        assert!(text.contains("item 1 (bogus://typed)"), "got {text}");
        assert!(text.contains("item 3 (bogus://file)"), "got {text}");
    }

    #[test]
    fn typed_prefix_round_trips_merge_browse_results() {
        let files = vec![
            ("a.txt".to_string(), "alpha\n".to_string()),
            ("b.txt".to_string(), "beta".to_string()),
        ];
        let (merged, _, _) = merge_browse_results("one\n\ntwo\n", &files);
        assert_eq!(typed_prefix(&merged, &files), Some("one\n\ntwo\n"));
    }
}
