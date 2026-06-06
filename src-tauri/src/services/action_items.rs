use crate::services::audit::AuditService;
use crate::services::candidates::ActionCandidate;
use crate::services::index::open_index_for_vault;
use crate::services::vault::normalize_relative_path;
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub id: i64,
    pub source_candidate_id: Option<i64>,
    pub source_relative_path: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub payload: Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleItem {
    pub id: i64,
    pub source_candidate_id: Option<i64>,
    pub source_relative_path: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub all_day: bool,
    pub timezone: Option<String>,
    pub location: Option<String>,
    pub payload: Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidatePromotionResult {
    pub candidate: ActionCandidate,
    pub todo_item: Option<TodoItem>,
    pub schedule_item: Option<ScheduleItem>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItemSearchQuery {
    pub query: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItemSearchResult {
    pub items: Vec<ActionItemRecord>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItemRecord {
    pub id: i64,
    pub kind: String,
    pub source_candidate_id: Option<i64>,
    pub source_relative_path: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub all_day: Option<bool>,
    pub timezone: Option<String>,
    pub location: Option<String>,
    pub payload: Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
}

impl From<TodoItem> for ActionItemRecord {
    fn from(item: TodoItem) -> Self {
        Self {
            id: item.id,
            kind: "todo".to_string(),
            source_candidate_id: item.source_candidate_id,
            source_relative_path: item.source_relative_path,
            title: item.title,
            notes: item.notes,
            due_at: item.due_at,
            starts_at: None,
            ends_at: None,
            all_day: None,
            timezone: None,
            location: None,
            payload: item.payload,
            status: item.status,
            created_at: item.created_at,
            updated_at: item.updated_at,
            completed_at: item.completed_at,
            cancelled_at: item.cancelled_at,
        }
    }
}

impl From<ScheduleItem> for ActionItemRecord {
    fn from(item: ScheduleItem) -> Self {
        Self {
            id: item.id,
            kind: "schedule".to_string(),
            source_candidate_id: item.source_candidate_id,
            source_relative_path: item.source_relative_path,
            title: item.title,
            notes: item.notes,
            due_at: None,
            starts_at: item.starts_at,
            ends_at: item.ends_at,
            all_day: Some(item.all_day),
            timezone: item.timezone,
            location: item.location,
            payload: item.payload,
            status: item.status,
            created_at: item.created_at,
            updated_at: item.updated_at,
            completed_at: item.completed_at,
            cancelled_at: item.cancelled_at,
        }
    }
}

pub struct ActionItemService;

impl ActionItemService {
    pub fn promote_candidate(
        vault_path: &str,
        candidate_id: i64,
    ) -> ServiceResult<CandidatePromotionResult> {
        if candidate_id <= 0 {
            return Err(ServiceError::InvalidState(
                "candidate_id must be positive".to_string(),
            ));
        }

        let mut connection = open_index_for_vault(vault_path)?;
        let transaction = connection.transaction()?;
        let candidate = get_candidate_for_promotion(&transaction, candidate_id)?;
        if candidate.status == "rejected" {
            return Err(ServiceError::InvalidState(format!(
                "candidate {candidate_id} has been rejected"
            )));
        }

        let now = Utc::now().to_rfc3339();
        let status;
        let mut todo_item = None;
        let mut schedule_item = None;

        match candidate.candidate_type.as_str() {
            "todo" => {
                if let Some(existing) = find_todo_by_candidate(&transaction, candidate.id)? {
                    todo_item = Some(existing);
                    status = "existing".to_string();
                } else {
                    let item = insert_todo_from_candidate(&transaction, &candidate, &now)?;
                    todo_item = Some(item);
                    status = "created".to_string();
                }
            }
            "schedule" => {
                if let Some(existing) = find_schedule_by_candidate(&transaction, candidate.id)? {
                    schedule_item = Some(existing);
                    status = "existing".to_string();
                } else {
                    let item = insert_schedule_from_candidate(&transaction, &candidate, &now)?;
                    schedule_item = Some(item);
                    status = "created".to_string();
                }
            }
            other => {
                return Err(ServiceError::InvalidState(format!(
                    "unsupported candidate type for promotion: {other}"
                )));
            }
        }

        transaction.execute(
            "UPDATE action_candidates
             SET status = 'confirmed',
                 updated_at = ?1,
                 confirmed_at = COALESCE(confirmed_at, ?1)
             WHERE id = ?2",
            params![now, candidate.id],
        )?;
        transaction.commit()?;

        let confirmed_candidate = get_candidate(vault_path, candidate.id)?;
        let _ = AuditService::record(
            vault_path,
            "action_candidate_promoted",
            json!({
                "candidateId": candidate.id,
                "candidateType": candidate.candidate_type,
                "status": status,
                "todoItemId": todo_item.as_ref().map(|item| item.id),
                "scheduleItemId": schedule_item.as_ref().map(|item| item.id)
            }),
        );

        Ok(CandidatePromotionResult {
            candidate: confirmed_candidate,
            todo_item,
            schedule_item,
            status,
        })
    }

    pub fn list_todo_items(
        vault_path: &str,
        include_completed: bool,
    ) -> ServiceResult<Vec<TodoItem>> {
        let connection = open_index_for_vault(vault_path)?;
        let sql = if include_completed {
            "SELECT id, source_candidate_id, source_relative_path, title, notes, due_at,
                    payload_json, status, created_at, updated_at, completed_at, cancelled_at
             FROM todo_items
             ORDER BY
                CASE status WHEN 'open' THEN 0 WHEN 'completed' THEN 1 ELSE 2 END,
                COALESCE(due_at, created_at),
                id"
        } else {
            "SELECT id, source_candidate_id, source_relative_path, title, notes, due_at,
                    payload_json, status, created_at, updated_at, completed_at, cancelled_at
             FROM todo_items
             WHERE status = 'open'
             ORDER BY COALESCE(due_at, created_at), id"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], row_to_todo_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_schedule_items(
        vault_path: &str,
        include_completed: bool,
    ) -> ServiceResult<Vec<ScheduleItem>> {
        let connection = open_index_for_vault(vault_path)?;
        let sql = if include_completed {
            "SELECT id, source_candidate_id, source_relative_path, title, notes, starts_at,
                    ends_at, all_day, timezone, location, payload_json, status, created_at,
                    updated_at, completed_at, cancelled_at
             FROM schedule_items
             ORDER BY
                CASE status WHEN 'scheduled' THEN 0 WHEN 'completed' THEN 1 ELSE 2 END,
                COALESCE(starts_at, created_at),
                id"
        } else {
            "SELECT id, source_candidate_id, source_relative_path, title, notes, starts_at,
                    ends_at, all_day, timezone, location, payload_json, status, created_at,
                    updated_at, completed_at, cancelled_at
             FROM schedule_items
             WHERE status = 'scheduled'
             ORDER BY COALESCE(starts_at, created_at), id"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], row_to_schedule_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn search_items(
        vault_path: &str,
        query: ActionItemSearchQuery,
    ) -> ServiceResult<ActionItemSearchResult> {
        let kind_filter = normalize_kind_filter(query.kind.as_deref())?;
        let status_filter = normalize_status_filter(query.status.as_deref())?;
        let text_filter = query
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_lowercase());
        let limit = query.limit.unwrap_or(20).clamp(1, 100);
        let offset = query.offset.unwrap_or(0);

        let mut records = Vec::new();
        if kind_filter != "schedule" {
            records.extend(
                Self::list_todo_items(vault_path, true)?
                    .into_iter()
                    .map(ActionItemRecord::from),
            );
        }
        if kind_filter != "todo" {
            records.extend(
                Self::list_schedule_items(vault_path, true)?
                    .into_iter()
                    .map(ActionItemRecord::from),
            );
        }

        records.retain(|item| {
            action_item_matches_status(item, &status_filter)
                && action_item_matches_query(item, text_filter.as_deref())
        });
        records.sort_by(action_item_sort);

        let total = records.len();
        let items = records
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let consumed = offset.saturating_add(items.len());
        let next_offset = (consumed < total).then_some(consumed);

        Ok(ActionItemSearchResult {
            items,
            total,
            limit,
            offset,
            next_offset,
        })
    }

    pub fn set_todo_status(vault_path: &str, id: i64, status: &str) -> ServiceResult<TodoItem> {
        validate_todo_status(status)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let completed_at = (status == "completed").then_some(now.clone());
        let cancelled_at = (status == "cancelled").then_some(now.clone());
        let changed = connection.execute(
            "UPDATE todo_items
             SET status = ?1,
                 updated_at = ?2,
                 completed_at = ?3,
                 cancelled_at = ?4
             WHERE id = ?5",
            params![status, now, completed_at, cancelled_at, id],
        )?;
        if changed == 0 {
            return Err(ServiceError::InvalidState(format!(
                "todo item {id} does not exist"
            )));
        }
        let item = get_todo_item(&connection, id)?;
        let _ = AuditService::record(
            vault_path,
            "todo_item_status_changed",
            json!({"todoItemId": id, "status": status}),
        );
        Ok(item)
    }

    pub fn set_schedule_status(
        vault_path: &str,
        id: i64,
        status: &str,
    ) -> ServiceResult<ScheduleItem> {
        validate_schedule_status(status)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        let completed_at = (status == "completed").then_some(now.clone());
        let cancelled_at = (status == "cancelled").then_some(now.clone());
        let changed = connection.execute(
            "UPDATE schedule_items
             SET status = ?1,
                 updated_at = ?2,
                 completed_at = ?3,
                 cancelled_at = ?4
             WHERE id = ?5",
            params![status, now, completed_at, cancelled_at, id],
        )?;
        if changed == 0 {
            return Err(ServiceError::InvalidState(format!(
                "schedule item {id} does not exist"
            )));
        }
        let item = get_schedule_item(&connection, id)?;
        let _ = AuditService::record(
            vault_path,
            "schedule_item_status_changed",
            json!({"scheduleItemId": id, "status": status}),
        );
        Ok(item)
    }
}

fn get_candidate_for_promotion(
    connection: &rusqlite::Transaction<'_>,
    id: i64,
) -> ServiceResult<ActionCandidate> {
    connection
        .query_row(
            "SELECT id, candidate_type, source_relative_path, title, payload_json,
                    status, created_at, updated_at, confirmed_at
             FROM action_candidates
             WHERE id = ?1",
            params![id],
            row_to_candidate,
        )
        .map_err(Into::into)
}

fn get_candidate(vault_path: &str, id: i64) -> ServiceResult<ActionCandidate> {
    let connection = open_index_for_vault(vault_path)?;
    connection
        .query_row(
            "SELECT id, candidate_type, source_relative_path, title, payload_json,
                    status, created_at, updated_at, confirmed_at
             FROM action_candidates
             WHERE id = ?1",
            params![id],
            row_to_candidate,
        )
        .map_err(Into::into)
}

fn row_to_candidate(row: &Row<'_>) -> rusqlite::Result<ActionCandidate> {
    let payload_json: String = row.get(4)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Object(Map::new()));
    Ok(ActionCandidate {
        id: row.get(0)?,
        candidate_type: row.get(1)?,
        source_relative_path: row.get(2)?,
        title: row.get(3)?,
        payload,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        confirmed_at: row.get(8)?,
    })
}

fn find_todo_by_candidate(
    connection: &rusqlite::Transaction<'_>,
    candidate_id: i64,
) -> ServiceResult<Option<TodoItem>> {
    connection
        .query_row(
            "SELECT id, source_candidate_id, source_relative_path, title, notes, due_at,
                    payload_json, status, created_at, updated_at, completed_at, cancelled_at
             FROM todo_items
             WHERE source_candidate_id = ?1",
            params![candidate_id],
            row_to_todo_item,
        )
        .optional()
        .map_err(Into::into)
}

fn find_schedule_by_candidate(
    connection: &rusqlite::Transaction<'_>,
    candidate_id: i64,
) -> ServiceResult<Option<ScheduleItem>> {
    connection
        .query_row(
            "SELECT id, source_candidate_id, source_relative_path, title, notes, starts_at,
                    ends_at, all_day, timezone, location, payload_json, status, created_at,
                    updated_at, completed_at, cancelled_at
             FROM schedule_items
             WHERE source_candidate_id = ?1",
            params![candidate_id],
            row_to_schedule_item,
        )
        .optional()
        .map_err(Into::into)
}

fn insert_todo_from_candidate(
    connection: &rusqlite::Transaction<'_>,
    candidate: &ActionCandidate,
    now: &str,
) -> ServiceResult<TodoItem> {
    let source_relative_path = candidate
        .source_relative_path
        .as_deref()
        .map(normalize_relative_path)
        .transpose()?;
    let notes = payload_string(&candidate.payload, &["notes", "excerpt", "summary", "text"]);
    let due_at = payload_string(&candidate.payload, &["dueAt", "due_at", "date"]);
    let payload_json = serde_json::to_string(&candidate.payload)?;
    connection.execute(
        "INSERT INTO todo_items
            (source_candidate_id, source_relative_path, title, notes, due_at, payload_json,
             status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7, ?7)",
        params![
            candidate.id,
            source_relative_path,
            candidate.title,
            notes,
            due_at,
            payload_json,
            now
        ],
    )?;
    get_todo_item(connection, connection.last_insert_rowid())
}

fn insert_schedule_from_candidate(
    connection: &rusqlite::Transaction<'_>,
    candidate: &ActionCandidate,
    now: &str,
) -> ServiceResult<ScheduleItem> {
    let source_relative_path = candidate
        .source_relative_path
        .as_deref()
        .map(normalize_relative_path)
        .transpose()?;
    let notes = payload_string(&candidate.payload, &["notes", "excerpt", "summary", "text"]);
    let starts_at = payload_string(
        &candidate.payload,
        &[
            "startsAt",
            "startAt",
            "starts_at",
            "start_at",
            "dueAt",
            "date",
        ],
    );
    let ends_at = payload_string(
        &candidate.payload,
        &["endsAt", "endAt", "ends_at", "end_at"],
    );
    let timezone = payload_string(&candidate.payload, &["timezone", "timeZone"]);
    let location = payload_string(&candidate.payload, &["location", "place"]);
    let all_day = payload_bool(&candidate.payload, &["allDay", "all_day"]);
    let payload_json = serde_json::to_string(&candidate.payload)?;
    connection.execute(
        "INSERT INTO schedule_items
            (source_candidate_id, source_relative_path, title, notes, starts_at, ends_at,
             all_day, timezone, location, payload_json, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'scheduled', ?11, ?11)",
        params![
            candidate.id,
            source_relative_path,
            candidate.title,
            notes,
            starts_at,
            ends_at,
            if all_day { 1 } else { 0 },
            timezone,
            location,
            payload_json,
            now
        ],
    )?;
    get_schedule_item(connection, connection.last_insert_rowid())
}

fn get_todo_item(connection: &rusqlite::Connection, id: i64) -> ServiceResult<TodoItem> {
    connection
        .query_row(
            "SELECT id, source_candidate_id, source_relative_path, title, notes, due_at,
                    payload_json, status, created_at, updated_at, completed_at, cancelled_at
             FROM todo_items
             WHERE id = ?1",
            params![id],
            row_to_todo_item,
        )
        .map_err(Into::into)
}

fn get_schedule_item(connection: &rusqlite::Connection, id: i64) -> ServiceResult<ScheduleItem> {
    connection
        .query_row(
            "SELECT id, source_candidate_id, source_relative_path, title, notes, starts_at,
                    ends_at, all_day, timezone, location, payload_json, status, created_at,
                    updated_at, completed_at, cancelled_at
             FROM schedule_items
             WHERE id = ?1",
            params![id],
            row_to_schedule_item,
        )
        .map_err(Into::into)
}

fn row_to_todo_item(row: &Row<'_>) -> rusqlite::Result<TodoItem> {
    let payload_json: String = row.get(6)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Object(Map::new()));
    Ok(TodoItem {
        id: row.get(0)?,
        source_candidate_id: row.get(1)?,
        source_relative_path: row.get(2)?,
        title: row.get(3)?,
        notes: row.get(4)?,
        due_at: row.get(5)?,
        payload,
        status: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        completed_at: row.get(10)?,
        cancelled_at: row.get(11)?,
    })
}

fn row_to_schedule_item(row: &Row<'_>) -> rusqlite::Result<ScheduleItem> {
    let payload_json: String = row.get(10)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Object(Map::new()));
    let all_day: i64 = row.get(7)?;
    Ok(ScheduleItem {
        id: row.get(0)?,
        source_candidate_id: row.get(1)?,
        source_relative_path: row.get(2)?,
        title: row.get(3)?,
        notes: row.get(4)?,
        starts_at: row.get(5)?,
        ends_at: row.get(6)?,
        all_day: all_day != 0,
        timezone: row.get(8)?,
        location: row.get(9)?,
        payload,
        status: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        completed_at: row.get(14)?,
        cancelled_at: row.get(15)?,
    })
}

fn payload_string(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn payload_bool(payload: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn normalize_kind_filter(kind: Option<&str>) -> ServiceResult<String> {
    let value = kind.unwrap_or("all").trim();
    if value.is_empty() || value == "all" {
        return Ok("all".to_string());
    }
    match value {
        "todo" | "schedule" => Ok(value.to_string()),
        _ => Err(ServiceError::InvalidState(format!(
            "unsupported action item kind filter: {value}"
        ))),
    }
}

fn normalize_status_filter(status: Option<&str>) -> ServiceResult<String> {
    let value = status.unwrap_or("active").trim();
    if value.is_empty() {
        return Ok("active".to_string());
    }
    match value {
        "active" | "all" | "open" | "scheduled" | "completed" | "cancelled" | "archived" => {
            Ok(value.to_string())
        }
        _ => Err(ServiceError::InvalidState(format!(
            "unsupported action item status filter: {value}"
        ))),
    }
}

fn action_item_matches_status(item: &ActionItemRecord, status_filter: &str) -> bool {
    match status_filter {
        "all" => true,
        "active" => item.status == "open" || item.status == "scheduled",
        status => item.status == status,
    }
}

fn action_item_matches_query(item: &ActionItemRecord, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    let payload_text = serde_json::to_string(&item.payload).unwrap_or_default();
    let matches = [
        Some(item.kind.as_str()),
        Some(item.title.as_str()),
        item.notes.as_deref(),
        item.source_relative_path.as_deref(),
        item.due_at.as_deref(),
        item.starts_at.as_deref(),
        item.ends_at.as_deref(),
        item.timezone.as_deref(),
        item.location.as_deref(),
        Some(item.status.as_str()),
        Some(payload_text.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(query));
    matches
}

fn action_item_sort(left: &ActionItemRecord, right: &ActionItemRecord) -> std::cmp::Ordering {
    action_status_rank(&left.status)
        .cmp(&action_status_rank(&right.status))
        .then_with(|| action_item_time_key(left).cmp(&action_item_time_key(right)))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| left.id.cmp(&right.id))
}

fn action_status_rank(status: &str) -> u8 {
    match status {
        "open" | "scheduled" => 0,
        "completed" => 1,
        "cancelled" => 2,
        "archived" => 3,
        _ => 4,
    }
}

fn action_item_time_key(item: &ActionItemRecord) -> &str {
    item.starts_at
        .as_deref()
        .or(item.due_at.as_deref())
        .or(Some(item.created_at.as_str()))
        .unwrap_or("")
}

fn validate_todo_status(status: &str) -> ServiceResult<()> {
    match status {
        "open" | "completed" | "cancelled" | "archived" => Ok(()),
        _ => Err(ServiceError::InvalidState(format!(
            "unsupported todo item status: {status}"
        ))),
    }
}

fn validate_schedule_status(status: &str) -> ServiceResult<()> {
    match status {
        "scheduled" | "completed" | "cancelled" | "archived" => Ok(()),
        _ => Err(ServiceError::InvalidState(format!(
            "unsupported schedule item status: {status}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::candidates::{CandidateInput, CandidateService};
    use crate::services::vault::VaultService;

    #[test]
    fn promotes_todo_candidate_idempotently_and_updates_status() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();

        let candidate = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: Some("000-收集箱/task.md".to_string()),
                title: "提交作业".to_string(),
                payload: Some(json!({
                    "notes": "来自课堂笔记",
                    "dueAt": "2026-06-10",
                    "confidence": 0.87
                })),
            },
        )
        .unwrap();

        let first = ActionItemService::promote_candidate(vault_path, candidate.id).unwrap();
        let second = ActionItemService::promote_candidate(vault_path, candidate.id).unwrap();

        let first_item = first.todo_item.unwrap();
        let second_item = second.todo_item.unwrap();
        assert_eq!(first.status, "created");
        assert_eq!(second.status, "existing");
        assert_eq!(first_item.id, second_item.id);
        assert_eq!(first_item.status, "open");
        assert_eq!(first_item.due_at.as_deref(), Some("2026-06-10"));
        assert_eq!(
            first.candidate.source_relative_path.as_deref(),
            Some("000-收集箱/task.md")
        );
        assert_eq!(first.candidate.status, "confirmed");

        let items = ActionItemService::list_todo_items(vault_path, false).unwrap();
        assert_eq!(items.len(), 1);
        let completed =
            ActionItemService::set_todo_status(vault_path, first_item.id, "completed").unwrap();
        assert_eq!(completed.status, "completed");
        assert!(completed.completed_at.is_some());
        assert!(ActionItemService::list_todo_items(vault_path, false)
            .unwrap()
            .is_empty());
        assert_eq!(
            ActionItemService::list_todo_items(vault_path, true)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn promotes_schedule_candidate_and_preserves_payload_fields() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();

        let candidate = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "schedule".to_string(),
                source_relative_path: Some("000-收集箱/meeting.md".to_string()),
                title: "项目例会".to_string(),
                payload: Some(json!({
                    "startsAt": "2026-06-12T09:00:00+08:00",
                    "endsAt": "2026-06-12T10:00:00+08:00",
                    "allDay": false,
                    "timezone": "Asia/Shanghai",
                    "location": "线上"
                })),
            },
        )
        .unwrap();

        let promoted = ActionItemService::promote_candidate(vault_path, candidate.id).unwrap();
        let item = promoted.schedule_item.unwrap();
        assert_eq!(item.status, "scheduled");
        assert_eq!(item.starts_at.as_deref(), Some("2026-06-12T09:00:00+08:00"));
        assert_eq!(item.location.as_deref(), Some("线上"));
        assert_eq!(item.payload["timezone"], "Asia/Shanghai");

        let cancelled =
            ActionItemService::set_schedule_status(vault_path, item.id, "cancelled").unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(cancelled.cancelled_at.is_some());
    }

    #[test]
    fn rejected_or_unsupported_candidates_are_not_promoted() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();

        let rejected = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: None,
                title: "已忽略".to_string(),
                payload: None,
            },
        )
        .unwrap();
        CandidateService::reject(vault_path, rejected.id).unwrap();
        assert!(ActionItemService::promote_candidate(vault_path, rejected.id).is_err());

        let unsupported = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "person".to_string(),
                source_relative_path: None,
                title: "联系人".to_string(),
                payload: None,
            },
        )
        .unwrap();
        assert!(ActionItemService::promote_candidate(vault_path, unsupported.id).is_err());
    }

    #[test]
    fn search_items_filters_status_kind_text_and_pages_results() {
        let temp = tempfile::tempdir().unwrap();
        let vault_path = temp.path().to_str().unwrap();
        VaultService::init(vault_path).unwrap();

        let todo_candidate = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: Some("100-School/homework.md".to_string()),
                title: "提交线代作业".to_string(),
                payload: Some(json!({
                    "notes": "周三前交给助教",
                    "dueAt": "2026-06-10"
                })),
            },
        )
        .unwrap();
        let schedule_candidate = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "schedule".to_string(),
                source_relative_path: Some("200-Projects/brain/meeting.md".to_string()),
                title: "项目例会".to_string(),
                payload: Some(json!({
                    "startsAt": "2026-06-11T09:00:00+08:00",
                    "location": "线上会议室",
                    "notes": "讨论 Archive Map"
                })),
            },
        )
        .unwrap();
        let second_todo_candidate = CandidateService::create(
            vault_path,
            CandidateInput {
                candidate_type: "todo".to_string(),
                source_relative_path: Some("000-收集箱/life.md".to_string()),
                title: "购买牛奶".to_string(),
                payload: Some(json!({"notes": "晚饭后"})),
            },
        )
        .unwrap();

        let promoted_todo =
            ActionItemService::promote_candidate(vault_path, todo_candidate.id).unwrap();
        ActionItemService::promote_candidate(vault_path, schedule_candidate.id).unwrap();
        ActionItemService::promote_candidate(vault_path, second_todo_candidate.id).unwrap();

        let schedule_results = ActionItemService::search_items(
            vault_path,
            ActionItemSearchQuery {
                query: Some("archive".to_string()),
                kind: Some("schedule".to_string()),
                status: Some("active".to_string()),
                limit: Some(10),
                offset: Some(0),
            },
        )
        .unwrap();
        assert_eq!(schedule_results.total, 1);
        assert_eq!(schedule_results.items[0].kind, "schedule");
        assert_eq!(schedule_results.items[0].title, "项目例会");

        let paged = ActionItemService::search_items(
            vault_path,
            ActionItemSearchQuery {
                query: None,
                kind: Some("all".to_string()),
                status: Some("all".to_string()),
                limit: Some(1),
                offset: Some(0),
            },
        )
        .unwrap();
        assert_eq!(paged.total, 3);
        assert_eq!(paged.items.len(), 1);
        assert_eq!(paged.next_offset, Some(1));

        let todo_id = promoted_todo.todo_item.unwrap().id;
        ActionItemService::set_todo_status(vault_path, todo_id, "completed").unwrap();
        let completed = ActionItemService::search_items(
            vault_path,
            ActionItemSearchQuery {
                query: Some("线代".to_string()),
                kind: Some("todo".to_string()),
                status: Some("completed".to_string()),
                limit: Some(10),
                offset: Some(0),
            },
        )
        .unwrap();
        assert_eq!(completed.total, 1);
        assert_eq!(completed.items[0].status, "completed");

        assert!(ActionItemService::search_items(
            vault_path,
            ActionItemSearchQuery {
                query: None,
                kind: Some("person".to_string()),
                status: Some("all".to_string()),
                limit: Some(10),
                offset: Some(0),
            },
        )
        .is_err());
    }
}
