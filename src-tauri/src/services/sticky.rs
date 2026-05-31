use crate::services::index::open_index_for_vault;
use crate::services::markdown::MarkdownService;
use crate::services::vault::INBOX_DIR;
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickyNoteInput {
    pub id: Option<i64>,
    pub title: String,
    pub body: String,
    pub color: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickyNote {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub color: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub pinned: bool,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub struct StickyService;

impl StickyService {
    pub fn list(vault_path: &str, include_archived: bool) -> ServiceResult<Vec<StickyNote>> {
        let connection = open_index_for_vault(vault_path)?;
        let sql = if include_archived {
            "SELECT id, title, body, color, x, y, width, height, pinned, archived, created_at, updated_at
             FROM sticky_notes ORDER BY updated_at DESC"
        } else {
            "SELECT id, title, body, color, x, y, width, height, pinned, archived, created_at, updated_at
             FROM sticky_notes WHERE archived = 0 ORDER BY updated_at DESC"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], row_to_sticky)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn save(vault_path: &str, input: StickyNoteInput) -> ServiceResult<StickyNote> {
        if input.title.trim().is_empty() {
            return Err(ServiceError::InvalidState(
                "sticky title must not be empty".to_string(),
            ));
        }
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let id = if let Some(id) = input.id {
            let changed = connection.execute(
                "UPDATE sticky_notes
                 SET title = ?1, body = ?2, color = ?3, x = ?4, y = ?5, width = ?6, height = ?7,
                     pinned = ?8, archived = ?9, updated_at = ?10
                 WHERE id = ?11",
                params![
                    input.title,
                    input.body,
                    input.color,
                    input.x,
                    input.y,
                    input.width,
                    input.height,
                    if input.pinned { 1 } else { 0 },
                    if input.archived { 1 } else { 0 },
                    now,
                    id
                ],
            )?;
            if changed == 0 {
                return Err(ServiceError::InvalidState(format!(
                    "sticky note {id} does not exist"
                )));
            }
            id
        } else {
            connection.execute(
                "INSERT INTO sticky_notes
                    (title, body, color, x, y, width, height, pinned, archived, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    input.title,
                    input.body,
                    input.color,
                    input.x,
                    input.y,
                    input.width,
                    input.height,
                    if input.pinned { 1 } else { 0 },
                    if input.archived { 1 } else { 0 },
                    now
                ],
            )?;
            connection.last_insert_rowid()
        };
        Self::get(vault_path, id)
    }

    pub fn delete(vault_path: &str, id: i64) -> ServiceResult<()> {
        let connection = open_index_for_vault(vault_path)?;
        connection.execute("DELETE FROM sticky_notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn autosave_to_inbox(vault_path: &str, id: i64) -> ServiceResult<String> {
        let note = Self::get(vault_path, id)?;
        let relative_path = format!("{INBOX_DIR}/便利贴-{}.md", note.id);
        MarkdownService::save(
            vault_path,
            &relative_path,
            &note.body,
            Some(json!({
                "title": note.title,
                "source_type": "sticky_note",
                "status": "inbox"
            })),
        )?;
        Ok(relative_path)
    }

    fn get(vault_path: &str, id: i64) -> ServiceResult<StickyNote> {
        let connection = open_index_for_vault(vault_path)?;
        connection
            .query_row(
                "SELECT id, title, body, color, x, y, width, height, pinned, archived, created_at, updated_at
                 FROM sticky_notes WHERE id = ?1",
                params![id],
                row_to_sticky,
            )
            .optional()?
            .ok_or_else(|| ServiceError::InvalidState(format!("sticky note {id} does not exist")))
    }
}

fn row_to_sticky(row: &Row<'_>) -> rusqlite::Result<StickyNote> {
    Ok(StickyNote {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        color: row.get(3)?,
        x: row.get(4)?,
        y: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        pinned: row.get::<_, i64>(8)? != 0,
        archived: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn sticky_notes_persist_and_autosave_to_inbox_markdown() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let note = StickyService::save(
            temp.path().to_str().unwrap(),
            StickyNoteInput {
                id: None,
                title: "便签".to_string(),
                body: "内容".to_string(),
                color: "#fff59d".to_string(),
                x: 1,
                y: 2,
                width: 300,
                height: 200,
                pinned: false,
                archived: false,
            },
        )
        .unwrap();
        let relative =
            StickyService::autosave_to_inbox(temp.path().to_str().unwrap(), note.id).unwrap();
        assert_eq!(relative, "000-收集箱/便利贴-1.md");
        assert!(temp.path().join(relative).exists());
    }
}
