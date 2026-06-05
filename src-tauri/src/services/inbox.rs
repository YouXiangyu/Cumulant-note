use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, INBOX_DIR, LEDGER_FILE,
};
use crate::services::ServiceResult;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<u64>,
}

pub struct InboxService;

impl InboxService {
    pub fn list(vault_path: &str) -> ServiceResult<Vec<InboxItem>> {
        let root = canonical_vault_root(vault_path)?;
        let inbox = root.join(INBOX_DIR);
        if !inbox.exists() {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();
        collect_inbox_items(&root, &inbox, &mut items)?;
        items.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(items)
    }
}

fn collect_inbox_items(
    root: &Path,
    directory: &Path,
    items: &mut Vec<InboxItem>,
) -> ServiceResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_name(&name) {
            continue;
        }

        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_inbox_items(root, &path, items)?;
            continue;
        }
        if !metadata.is_file() || !is_supported_inbox_file(&path) {
            continue;
        }

        let relative_path = path
            .strip_prefix(root)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .map_err(|_| {
                crate::services::ServiceError::EscapedVault(path.to_string_lossy().to_string())
            })?;
        let relative_path = normalize_relative_path(&relative_path)?;
        if relative_path == format!("{INBOX_DIR}/{LEDGER_FILE}") {
            continue;
        }
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        items.push(InboxItem {
            name,
            relative_path,
            is_dir: false,
            size_bytes: Some(metadata.len()),
            modified_at,
        });
    }
    Ok(())
}

fn should_skip_name(name: &str) -> bool {
    name == LEDGER_FILE
        || name.starts_with('.')
        || matches!(
            name,
            ".thebrain" | ".secrets" | ".git" | "node_modules" | "target" | "dist"
        )
}

fn is_supported_inbox_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "md" | "markdown" | "txt" | "mp3" | "wav" | "m4a" | "aac" | "png" | "jpg" | "jpeg"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn inbox_list_recurses_supported_files_and_skips_unsupported() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let inbox = temp.path().join(INBOX_DIR);
        fs::create_dir_all(inbox.join("nested").join("deep")).unwrap();
        fs::write(inbox.join("root.md"), "root").unwrap();
        fs::write(inbox.join("nested").join("deep").join("note.txt"), "deep").unwrap();
        fs::write(inbox.join("nested").join("slides.pdf"), "unsupported").unwrap();
        fs::write(inbox.join(LEDGER_FILE), "ledger").unwrap();

        let items = InboxService::list(temp.path().to_str().unwrap()).unwrap();
        let paths = items
            .into_iter()
            .map(|item| item.relative_path)
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                format!("{INBOX_DIR}/nested/deep/note.txt"),
                format!("{INBOX_DIR}/root.md"),
            ]
        );
    }
}
