use crate::services::ServiceResult;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagTraceRun {
    pub id: i64,
    pub query_id: Option<i64>,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub metadata: Value,
    pub nodes: Vec<RagTraceNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagTraceNode {
    pub id: i64,
    pub run_id: i64,
    pub parent_id: Option<i64>,
    pub node_type: String,
    pub name: String,
    pub input: Value,
    pub output: Value,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
}

pub struct RagTraceService;

impl RagTraceService {
    pub fn start_run(connection: &Connection, metadata: Value) -> ServiceResult<i64> {
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO rag_trace_runs (status, started_at, metadata_json)
             VALUES ('running', ?1, ?2)",
            params![now, metadata.to_string()],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn set_query_id(connection: &Connection, run_id: i64, query_id: i64) -> ServiceResult<()> {
        connection.execute(
            "UPDATE rag_trace_runs SET query_id = ?1 WHERE id = ?2",
            params![query_id, run_id],
        )?;
        Ok(())
    }

    pub fn finish_run(connection: &Connection, run_id: i64, status: &str) -> ServiceResult<()> {
        let now = Utc::now();
        let started_at = connection
            .query_row(
                "SELECT started_at FROM rag_trace_runs WHERE id = ?1",
                params![run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let duration_ms = started_at
            .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
            .map(|started| {
                now.signed_duration_since(started.with_timezone(&Utc))
                    .num_milliseconds()
            });
        connection.execute(
            "UPDATE rag_trace_runs
             SET status = ?1, finished_at = ?2, duration_ms = ?3
             WHERE id = ?4",
            params![status, now.to_rfc3339(), duration_ms, run_id],
        )?;
        Ok(())
    }

    pub fn add_node(
        connection: &Connection,
        run_id: i64,
        parent_id: Option<i64>,
        node_type: &str,
        name: &str,
        input: Value,
        output: Value,
        status: &str,
        error: Option<&str>,
    ) -> ServiceResult<i64> {
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO rag_trace_nodes
             (run_id, parent_id, node_type, name, input_json, output_json, status, started_at, finished_at, duration_ms, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 0, ?9)",
            params![
                run_id,
                parent_id,
                node_type,
                name,
                input.to_string(),
                output.to_string(),
                status,
                now,
                error.map(ToOwned::to_owned)
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn latest(connection: &Connection) -> ServiceResult<Option<RagTraceRun>> {
        let run = connection
            .query_row(
                "SELECT id, query_id, status, started_at, finished_at, duration_ms, metadata_json
                 FROM rag_trace_runs
                 ORDER BY id DESC
                 LIMIT 1",
                [],
                |row| {
                    let metadata_json: String = row.get(6)?;
                    Ok(RagTraceRun {
                        id: row.get(0)?,
                        query_id: row.get(1)?,
                        status: row.get(2)?,
                        started_at: row.get(3)?,
                        finished_at: row.get(4)?,
                        duration_ms: row.get(5)?,
                        metadata: serde_json::from_str(&metadata_json)
                            .unwrap_or_else(|_| json!({})),
                        nodes: Vec::new(),
                    })
                },
            )
            .optional()?;
        let Some(mut run) = run else {
            return Ok(None);
        };
        run.nodes = Self::nodes_for_run(connection, run.id)?;
        Ok(Some(run))
    }

    pub fn nodes_for_run(connection: &Connection, run_id: i64) -> ServiceResult<Vec<RagTraceNode>> {
        let mut statement = connection.prepare(
            "SELECT id, run_id, parent_id, node_type, name, input_json, output_json, status,
                    started_at, finished_at, duration_ms, error
             FROM rag_trace_nodes
             WHERE run_id = ?1
             ORDER BY id ASC",
        )?;
        let rows = statement.query_map(params![run_id], |row| {
            let input_json: String = row.get(5)?;
            let output_json: String = row.get(6)?;
            Ok(RagTraceNode {
                id: row.get(0)?,
                run_id: row.get(1)?,
                parent_id: row.get(2)?,
                node_type: row.get(3)?,
                name: row.get(4)?,
                input: serde_json::from_str(&input_json).unwrap_or_else(|_| json!({})),
                output: serde_json::from_str(&output_json).unwrap_or_else(|_| json!({})),
                status: row.get(7)?,
                started_at: row.get(8)?,
                finished_at: row.get(9)?,
                duration_ms: row.get(10)?,
                error: row.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
