use crate::services::vault::{canonical_vault_root, INBOX_DIR, LEDGER_FILE};
use crate::services::ServiceResult;
use serde::Serialize;
use std::fs;

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
        for entry in fs::read_dir(&inbox)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name == LEDGER_FILE {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs());
            items.push(InboxItem {
                relative_path: format!("{INBOX_DIR}/{name}"),
                name,
                is_dir: metadata.is_dir(),
                size_bytes: metadata.is_file().then_some(metadata.len()),
                modified_at,
            });
        }
        items.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(items)
    }
}
