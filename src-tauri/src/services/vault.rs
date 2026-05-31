use crate::services::{ServiceError, ServiceResult};
use serde::Serialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::index::IndexService;

pub const INBOX_DIR: &str = "000-收集箱";
pub const LEDGER_FILE: &str = "收集箱-已整理.md";
pub const INTERNAL_DIR: &str = ".thebrain";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInitResult {
    pub vault_path: String,
    pub created: Vec<String>,
    pub preserved: Vec<String>,
    pub index_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTreeNode {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub children: Vec<VaultTreeNode>,
}

pub struct VaultService;

impl VaultService {
    pub fn init(vault_path: &str) -> ServiceResult<VaultInitResult> {
        let root = canonical_vault_root(vault_path)?;
        let mut created = Vec::new();
        let mut preserved = Vec::new();

        ensure_dir(
            &root.join(INBOX_DIR),
            INBOX_DIR,
            &mut created,
            &mut preserved,
        )?;
        ensure_dir(
            &root.join(INTERNAL_DIR),
            INTERNAL_DIR,
            &mut created,
            &mut preserved,
        )?;

        let ledger = root.join(INBOX_DIR).join(LEDGER_FILE);
        let ledger_rel = format!("{INBOX_DIR}/{LEDGER_FILE}");
        if ledger.exists() {
            preserved.push(ledger_rel);
        } else {
            fs::write(&ledger, "# 收集箱-已整理\n\n")?;
            created.push(ledger_rel);
        }

        let index_path = IndexService::open_or_create(&root)?.path;
        Ok(VaultInitResult {
            vault_path: root.to_string_lossy().to_string(),
            created,
            preserved,
            index_path,
        })
    }

    pub fn list_tree(vault_path: &str) -> ServiceResult<Vec<VaultTreeNode>> {
        let root = canonical_vault_root(vault_path)?;
        let mut nodes = Vec::new();
        for entry in sorted_entries(&root)? {
            if entry.file_name() == Some(OsStr::new(INTERNAL_DIR)) {
                continue;
            }
            nodes.push(tree_node(&root, &entry, 0)?);
        }
        Ok(nodes)
    }
}

pub fn canonical_vault_root(vault_path: &str) -> ServiceResult<PathBuf> {
    let root = PathBuf::from(vault_path);
    let canonical = root
        .canonicalize()
        .map_err(|_| ServiceError::InvalidVault(vault_path.to_string()))?;
    if !canonical.is_dir() {
        return Err(ServiceError::InvalidVault(vault_path.to_string()));
    }
    Ok(canonical)
}

pub fn resolve_existing_file(vault_path: &str, relative_path: &str) -> ServiceResult<PathBuf> {
    let root = canonical_vault_root(vault_path)?;
    let joined = resolve_relative_path(&root, relative_path)?;
    let canonical = joined.canonicalize()?;
    ensure_within_vault(&root, &canonical)?;
    if !canonical.is_file() {
        return Err(ServiceError::InvalidRelativePath(relative_path.to_string()));
    }
    Ok(canonical)
}

pub fn resolve_writable_file(vault_path: &str, relative_path: &str) -> ServiceResult<PathBuf> {
    let root = canonical_vault_root(vault_path)?;
    let joined = resolve_relative_path(&root, relative_path)?;
    let parent = joined
        .parent()
        .ok_or_else(|| ServiceError::InvalidRelativePath(relative_path.to_string()))?;
    let canonical_parent = parent.canonicalize()?;
    ensure_within_vault(&root, &canonical_parent)?;
    Ok(joined)
}

pub fn resolve_relative_path(root: &Path, relative_path: &str) -> ServiceResult<PathBuf> {
    let normalized = normalize_relative_path(relative_path)?;
    Ok(root.join(normalized))
}

pub fn normalize_relative_path(relative_path: &str) -> ServiceResult<String> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return Err(ServiceError::InvalidRelativePath(relative_path.to_string()));
    }

    let mut cleaned = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => cleaned.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ServiceError::InvalidRelativePath(relative_path.to_string()));
            }
        }
    }

    if cleaned.as_os_str().is_empty() {
        return Err(ServiceError::InvalidRelativePath(relative_path.to_string()));
    }

    Ok(cleaned.to_string_lossy().replace('\\', "/"))
}

pub fn resolve_ledger_target(
    root: &Path,
    base_relative_dir: &str,
    target: &str,
) -> ServiceResult<String> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut parts: Vec<String> = base_relative_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    let target_path = Path::new(target);
    if target_path.is_absolute() {
        return Err(ServiceError::EscapedVault(target.to_string()));
    }

    for component in target_path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(ServiceError::EscapedVault(target.to_string()));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ServiceError::EscapedVault(target.to_string()));
            }
        }
    }

    if parts.is_empty() {
        return Err(ServiceError::EscapedVault(target.to_string()));
    }

    let candidate = canonical_root.join(parts.iter().collect::<PathBuf>());
    if let Ok(canonical) = candidate.canonicalize() {
        ensure_within_vault(&canonical_root, &canonical)?;
    }
    Ok(parts.join("/"))
}

pub fn ensure_markdown_path(relative_path: &str) -> ServiceResult<()> {
    let suffix = Path::new(relative_path)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if suffix == "md" || suffix == "markdown" {
        Ok(())
    } else {
        Err(ServiceError::UnsupportedFileType(relative_path.to_string()))
    }
}

fn ensure_dir(
    path: &Path,
    relative: &str,
    created: &mut Vec<String>,
    preserved: &mut Vec<String>,
) -> ServiceResult<()> {
    if path.exists() {
        if !path.is_dir() {
            return Err(ServiceError::InvalidVault(relative.to_string()));
        }
        preserved.push(relative.to_string());
    } else {
        fs::create_dir_all(path)?;
        created.push(relative.to_string());
    }
    Ok(())
}

fn ensure_within_vault(root: &Path, candidate: &Path) -> ServiceResult<()> {
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(ServiceError::EscapedVault(
            candidate.to_string_lossy().to_string(),
        ))
    }
}

fn sorted_entries(path: &Path) -> ServiceResult<Vec<PathBuf>> {
    let mut entries = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    Ok(entries)
}

fn tree_node(root: &Path, path: &Path, depth: usize) -> ServiceResult<VaultTreeNode> {
    let metadata = fs::symlink_metadata(path)?;
    let is_dir = metadata.is_dir();
    let relative_path = path
        .strip_prefix(root)
        .map_err(|_| ServiceError::EscapedVault(path.to_string_lossy().to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| relative_path.clone());

    let mut children = Vec::new();
    if is_dir && depth < 5 {
        for child in sorted_entries(path)? {
            if child.file_name() == Some(OsStr::new(INTERNAL_DIR)) {
                continue;
            }
            children.push(tree_node(root, &child, depth + 1)?);
        }
    }

    Ok(VaultTreeNode {
        name,
        relative_path,
        is_dir,
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent_and_preserves_ledger() {
        let temp = tempfile::tempdir().unwrap();
        let inbox = temp.path().join(INBOX_DIR);
        fs::create_dir_all(&inbox).unwrap();
        let ledger = inbox.join(LEDGER_FILE);
        fs::write(&ledger, "existing ledger").unwrap();

        let first = VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let second = VaultService::init(temp.path().to_str().unwrap()).unwrap();

        assert!(temp.path().join(INTERNAL_DIR).is_dir());
        assert_eq!(fs::read_to_string(ledger).unwrap(), "existing ledger");
        assert!(first
            .preserved
            .contains(&format!("{INBOX_DIR}/{LEDGER_FILE}")));
        assert!(second.preserved.contains(&INBOX_DIR.to_string()));
    }

    #[test]
    fn rejects_parent_dir_command_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = canonical_vault_root(temp.path().to_str().unwrap()).unwrap();
        let err = resolve_relative_path(&root, "../outside.md").unwrap_err();
        assert!(matches!(err, ServiceError::InvalidRelativePath(_)));
    }
}
