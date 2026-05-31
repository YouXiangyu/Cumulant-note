use crate::services::vault::{canonical_vault_root, resolve_ledger_target, INBOX_DIR, LEDGER_FILE};
use crate::services::ServiceResult;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerItem {
    pub line_number: usize,
    pub source_line: String,
    pub raw_target: String,
    pub target_relative_path: String,
    pub display_name: String,
    pub exists: bool,
}

pub struct LedgerService;

impl LedgerService {
    pub fn parse_inbox_ledger(vault_path: &str) -> ServiceResult<Vec<LedgerItem>> {
        let root = canonical_vault_root(vault_path)?;
        let ledger_path = root.join(INBOX_DIR).join(LEDGER_FILE);
        if !ledger_path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(ledger_path)?;
        parse_ledger_content(&root, &content)
    }
}

pub fn parse_ledger_content(root: &Path, content: &str) -> ServiceResult<Vec<LedgerItem>> {
    let link_re = Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("valid wikilink regex");
    let mut items = Vec::new();

    for (index, line) in content.lines().enumerate() {
        for capture in link_re.captures_iter(line) {
            let raw_target = capture
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .trim();
            let alias = capture
                .get(2)
                .map(|m| m.as_str().trim())
                .filter(|s| !s.is_empty());
            let target_relative_path = resolve_ledger_target(root, INBOX_DIR, raw_target)?;
            let display_name = alias
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| display_name_from_target(&target_relative_path));
            let exists = root.join(&target_relative_path).exists();
            items.push(LedgerItem {
                line_number: index + 1,
                source_line: line.to_string(),
                raw_target: raw_target.to_string(),
                target_relative_path,
                display_name,
                exists,
            });
        }
    }

    Ok(items)
}

fn display_name_from_target(target: &str) -> String {
    Path::new(target)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| target.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_wikilink_from_inbox_ledger() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("100-School")).unwrap();
        std::fs::write(temp.path().join("100-School").join("笔记.md"), "done").unwrap();

        let items =
            parse_ledger_content(temp.path(), "- 2026-05-31 [[../100-School/笔记.md]]\n").unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].target_relative_path, "100-School/笔记.md");
        assert_eq!(items[0].display_name, "笔记.md");
        assert!(items[0].exists);
    }

    #[test]
    fn rejects_wikilink_that_escapes_vault() {
        let temp = tempfile::tempdir().unwrap();
        let err = parse_ledger_content(temp.path(), "[[../../outside.md]]").unwrap_err();
        assert!(err.to_string().contains("outside"));
    }
}
