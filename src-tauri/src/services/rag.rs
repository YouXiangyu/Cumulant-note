use crate::services::ai::MimoProvider;
use crate::services::chunking::chunk_markdown;
use crate::services::index::IndexService;
use crate::services::markdown::parse_markdown;
use crate::services::rag_trace::{RagTraceRun, RagTraceService};
use crate::services::retrieval::{
    format_context, retrieve_with_scope, summaries_json, RagScope, RetrievedChunk,
};
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, INBOX_DIR, INTERNAL_DIR, LEDGER_FILE,
};
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagIndexRun {
    pub id: i64,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub scanned_count: i64,
    pub indexed_count: i64,
    pub skipped_count: i64,
    pub deleted_count: i64,
    pub chunk_count: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagIndexStatus {
    pub schema_version: i64,
    pub document_count: i64,
    pub chunk_count: i64,
    pub last_run: Option<RagIndexRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagCitation {
    pub id: String,
    pub relative_path: String,
    pub title: String,
    pub heading_path: Vec<String>,
    pub snippet: String,
    pub score: f64,
    pub channel: String,
    pub start_line: i64,
    pub end_line: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagAnswer {
    pub query_id: i64,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub is_mock: bool,
    pub answer: String,
    pub fallback_reason: Option<String>,
    pub retrieved_count: usize,
    pub citations: Vec<RagCitation>,
    pub trace: RagTraceRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagConversationSummary {
    pub id: i64,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagMessage {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub query_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RagConversation {
    pub id: i64,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<RagMessage>,
}

#[derive(Debug, Default)]
struct RebuildStats {
    scanned_count: i64,
    indexed_count: i64,
    skipped_count: i64,
    deleted_count: i64,
    chunk_count: i64,
}

pub struct RagService;

impl RagService {
    pub fn rebuild_index(vault_path: &str) -> ServiceResult<RagIndexRun> {
        let (root, mut connection) = open_connection(vault_path)?;
        let started_at = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO rag_index_runs (status, started_at)
             VALUES ('running', ?1)",
            params![started_at],
        )?;
        let run_id = connection.last_insert_rowid();

        match rebuild_index_inner(&root, &mut connection) {
            Ok(stats) => {
                let finished_at = Utc::now().to_rfc3339();
                connection.execute(
                    "UPDATE rag_index_runs
                     SET status = 'ok',
                         finished_at = ?1,
                         scanned_count = ?2,
                         indexed_count = ?3,
                         skipped_count = ?4,
                         deleted_count = ?5,
                         chunk_count = ?6
                     WHERE id = ?7",
                    params![
                        finished_at,
                        stats.scanned_count,
                        stats.indexed_count,
                        stats.skipped_count,
                        stats.deleted_count,
                        stats.chunk_count,
                        run_id
                    ],
                )?;
                run_by_id(&connection, run_id)
            }
            Err(error) => {
                let finished_at = Utc::now().to_rfc3339();
                let message = error.to_string();
                let _ = connection.execute(
                    "UPDATE rag_index_runs
                     SET status = 'failed', finished_at = ?1, error = ?2
                     WHERE id = ?3",
                    params![finished_at, message, run_id],
                );
                Err(error)
            }
        }
    }

    pub fn status(vault_path: &str) -> ServiceResult<RagIndexStatus> {
        let (_, connection) = open_connection(vault_path)?;
        let schema_version = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let document_count = connection.query_row(
            "SELECT COUNT(*) FROM rag_documents WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;
        let chunk_count = connection.query_row(
            "SELECT COUNT(*) FROM rag_chunks WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;
        let last_run = connection
            .query_row(
                "SELECT id FROM rag_index_runs ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|id| run_by_id(&connection, id))
            .transpose()?;
        Ok(RagIndexStatus {
            schema_version,
            document_count,
            chunk_count,
            last_run,
        })
    }

    pub fn list_conversations(
        vault_path: &str,
        limit: Option<usize>,
    ) -> ServiceResult<Vec<RagConversationSummary>> {
        let (_, connection) = open_connection(vault_path)?;
        let limit = limit.unwrap_or(50).clamp(1, 100) as i64;
        let mut statement = connection.prepare(
            "SELECT id, title, created_at, updated_at
             FROM rag_conversations
             ORDER BY updated_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], conversation_summary_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_conversation(
        vault_path: &str,
        title: Option<String>,
    ) -> ServiceResult<RagConversationSummary> {
        let (_, connection) = open_connection(vault_path)?;
        let title = title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("新会话");
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO rag_conversations (title, created_at, updated_at)
             VALUES (?1, ?2, ?2)",
            params![title, now],
        )?;
        conversation_summary_by_id(&connection, connection.last_insert_rowid())
    }

    pub fn get_conversation(
        vault_path: &str,
        conversation_id: i64,
    ) -> ServiceResult<RagConversation> {
        let (_, connection) = open_connection(vault_path)?;
        conversation_by_id(&connection, conversation_id)
    }

    pub fn ask(
        vault_path: &str,
        question: &str,
        top_k: Option<usize>,
        conversation_id: Option<i64>,
        scope: Option<RagScope>,
    ) -> ServiceResult<RagAnswer> {
        Self::ask_with_options(vault_path, question, top_k, conversation_id, scope, true)
    }

    pub fn ask_with_options(
        vault_path: &str,
        question: &str,
        top_k: Option<usize>,
        conversation_id: Option<i64>,
        scope: Option<RagScope>,
        allow_network: bool,
    ) -> ServiceResult<RagAnswer> {
        let trimmed_question = question.trim();
        if trimmed_question.is_empty() {
            return Err(ServiceError::InvalidState(
                "question must not be empty".to_string(),
            ));
        }
        let (_, connection) = open_connection(vault_path)?;
        if let Some(id) = conversation_id {
            ensure_conversation_exists(&connection, id)?;
        }
        let scope = scope.unwrap_or_default();
        scope.validate()?;
        let top_k = top_k.unwrap_or(6).clamp(1, 12);
        let scope_json = serde_json::to_value(&scope)?;
        let trace_run_id = RagTraceService::start_run(
            &connection,
            json!({
                "topK": top_k,
                "conversationId": conversation_id,
                "scope": scope_json.clone(),
                "allowNetwork": allow_network
            }),
        )?;
        connection.execute(
            "INSERT INTO rag_queries (question, rewritten_question, status, top_k, created_at, trace_run_id)
             VALUES (?1, ?1, 'running', ?2, ?3, ?4)",
            params![trimmed_question, top_k as i64, Utc::now().to_rfc3339(), trace_run_id],
        )?;
        let query_id = connection.last_insert_rowid();
        RagTraceService::set_query_id(&connection, trace_run_id, query_id)?;

        let root_node = RagTraceService::add_node(
            &connection,
            trace_run_id,
            None,
            "query",
            "normalize_question",
            json!({"question": trimmed_question}),
            json!({"rewrittenQuestion": trimmed_question, "topK": top_k, "scope": scope_json.clone()}),
            "ok",
            None,
        )?;

        let retrieval = retrieve_with_scope(&connection, trimmed_question, top_k, Some(&scope))?;
        RagTraceService::add_node(
            &connection,
            trace_run_id,
            Some(root_node),
            "retrieval",
            "multi_channel_retrieval",
            json!({"channels": ["keyword", "local_semantic_placeholder"], "scope": scope_json}),
            json!({
                "summaries": summaries_json(&retrieval.summaries),
                "rawReturnedCount": retrieval.results.len()
            }),
            "ok",
            None,
        )?;
        RagTraceService::add_node(
            &connection,
            trace_run_id,
            Some(root_node),
            "postprocess",
            "deduplicate_normalize_topk",
            json!({"requestedTopK": top_k}),
            json!({
                "returnedCount": retrieval.results.len(),
                "citations": retrieval.results.iter().map(|item| item.citation_id.clone()).collect::<Vec<_>>()
            }),
            "ok",
            None,
        )?;

        let formatted_context = format_context(&retrieval.results);
        RagTraceService::add_node(
            &connection,
            trace_run_id,
            Some(root_node),
            "context",
            "default_context_formatter",
            json!({"citationCount": retrieval.results.len()}),
            json!({"contextLength": formatted_context.chars().count()}),
            "ok",
            None,
        )?;

        let ai_answer = MimoProvider::answer_rag(
            vault_path,
            trimmed_question,
            &formatted_context,
            allow_network,
        )?;
        RagTraceService::add_node(
            &connection,
            trace_run_id,
            Some(root_node),
            "llm",
            "mimo_rag_answer",
            json!({"provider": "mimo", "allowNetwork": allow_network}),
            json!({
                "status": ai_answer.status,
                "isMock": ai_answer.is_mock,
                "answerLength": ai_answer.answer.chars().count()
            }),
            if ai_answer.status == "ok" {
                "ok"
            } else {
                "fallback"
            },
            ai_answer.error.as_deref(),
        )?;

        connection.execute(
            "UPDATE rag_queries
             SET answer = ?1, status = ?2, fallback_reason = ?3
             WHERE id = ?4",
            params![
                ai_answer.answer,
                ai_answer.status,
                ai_answer.error,
                query_id
            ],
        )?;
        if let Some(id) = conversation_id {
            persist_conversation_turn(
                &connection,
                id,
                trimmed_question,
                &ai_answer.answer,
                query_id,
            )?;
        }
        RagTraceService::finish_run(
            &connection,
            trace_run_id,
            if ai_answer.is_mock { "fallback" } else { "ok" },
        )?;
        let trace = RagTraceService::latest(&connection)?
            .ok_or_else(|| ServiceError::InvalidState("RAG trace was not persisted".to_string()))?;

        Ok(RagAnswer {
            query_id,
            provider: ai_answer.provider,
            model: ai_answer.model,
            status: ai_answer.status,
            is_mock: ai_answer.is_mock,
            answer: ai_answer.answer,
            fallback_reason: ai_answer.error,
            retrieved_count: retrieval.results.len(),
            citations: retrieval.results.iter().map(citation_from_chunk).collect(),
            trace,
        })
    }

    pub fn latest_trace(vault_path: &str) -> ServiceResult<Option<RagTraceRun>> {
        let (_, connection) = open_connection(vault_path)?;
        RagTraceService::latest(&connection)
    }
}

fn open_connection(vault_path: &str) -> ServiceResult<(PathBuf, Connection)> {
    let root = canonical_vault_root(vault_path)?;
    let opened = IndexService::open_or_create(&root)?;
    Ok((root, Connection::open(opened.path)?))
}

fn conversation_summary_by_id(
    connection: &Connection,
    conversation_id: i64,
) -> ServiceResult<RagConversationSummary> {
    connection
        .query_row(
            "SELECT id, title, created_at, updated_at
             FROM rag_conversations
             WHERE id = ?1",
            params![conversation_id],
            conversation_summary_from_row,
        )
        .optional()?
        .ok_or_else(|| {
            ServiceError::InvalidState(format!("RAG conversation {conversation_id} does not exist"))
        })
}

fn conversation_by_id(
    connection: &Connection,
    conversation_id: i64,
) -> ServiceResult<RagConversation> {
    let summary = conversation_summary_by_id(connection, conversation_id)?;
    Ok(RagConversation {
        id: summary.id,
        title: summary.title,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
        messages: messages_for_conversation(connection, conversation_id)?,
    })
}

fn ensure_conversation_exists(connection: &Connection, conversation_id: i64) -> ServiceResult<()> {
    conversation_summary_by_id(connection, conversation_id).map(|_| ())
}

fn messages_for_conversation(
    connection: &Connection,
    conversation_id: i64,
) -> ServiceResult<Vec<RagMessage>> {
    let mut statement = connection.prepare(
        "SELECT id, conversation_id, role, content, query_id, created_at
         FROM rag_messages
         WHERE conversation_id = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = statement.query_map(params![conversation_id], message_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn persist_conversation_turn(
    connection: &Connection,
    conversation_id: i64,
    user_content: &str,
    assistant_content: &str,
    query_id: i64,
) -> ServiceResult<()> {
    let user_created_at = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO rag_messages (conversation_id, role, content, query_id, created_at)
         VALUES (?1, 'user', ?2, NULL, ?3)",
        params![conversation_id, user_content, user_created_at],
    )?;
    let assistant_created_at = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO rag_messages (conversation_id, role, content, query_id, created_at)
         VALUES (?1, 'assistant', ?2, ?3, ?4)",
        params![
            conversation_id,
            assistant_content,
            query_id,
            assistant_created_at
        ],
    )?;
    connection.execute(
        "UPDATE rag_conversations
         SET updated_at = ?1
         WHERE id = ?2",
        params![assistant_created_at, conversation_id],
    )?;
    Ok(())
}

fn conversation_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RagConversationSummary> {
    Ok(RagConversationSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RagMessage> {
    Ok(RagMessage {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        query_id: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn rebuild_index_inner(root: &Path, connection: &mut Connection) -> ServiceResult<RebuildStats> {
    let files = scan_supported_files(root)?;
    let mut stats = RebuildStats {
        scanned_count: files.len() as i64,
        ..RebuildStats::default()
    };
    let mut seen = HashSet::new();
    let transaction = connection.transaction()?;

    for path in files {
        let relative_path = relative_path(root, &path)?;
        seen.insert(relative_path.clone());
        let metadata = fs::metadata(&path)?;
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        let size_bytes = metadata.len() as i64;
        let raw = fs::read_to_string(&path)?;
        let (title, indexable_content) = parse_indexable_content(&relative_path, &raw)?;
        let content_hash = stable_hash(&indexable_content);
        let existing = transaction
            .query_row(
                "SELECT id, content_hash, modified_at, chunk_count
                 FROM rag_documents
                 WHERE relative_path = ?1 AND status = 'active'",
                params![relative_path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((_id, previous_hash, previous_modified_at, chunk_count)) = existing {
            if previous_hash == content_hash
                && previous_modified_at == modified_at
                && chunk_count > 0
            {
                stats.skipped_count += 1;
                stats.chunk_count += chunk_count;
                continue;
            }
        }

        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO rag_documents
             (relative_path, title, content_hash, modified_at, size_bytes, status, chunk_count, indexed_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', 0, ?6, NULL)
             ON CONFLICT(relative_path) DO UPDATE SET
                title = excluded.title,
                content_hash = excluded.content_hash,
                modified_at = excluded.modified_at,
                size_bytes = excluded.size_bytes,
                status = 'active',
                indexed_at = excluded.indexed_at,
                deleted_at = NULL",
            params![relative_path, title, content_hash, modified_at, size_bytes, now],
        )?;
        let document_id = transaction.query_row(
            "SELECT id FROM rag_documents WHERE relative_path = ?1",
            params![relative_path],
            |row| row.get::<_, i64>(0),
        )?;
        transaction.execute(
            "UPDATE rag_chunks SET status = 'stale', updated_at = ?1 WHERE document_id = ?2",
            params![now, document_id],
        )?;
        let chunks = chunk_markdown(&indexable_content);
        for chunk in &chunks {
            transaction.execute(
                "INSERT INTO rag_chunks
                 (document_id, relative_path, chunk_index, heading_path, content, snippet,
                  start_line, end_line, char_start, char_end, char_count, token_estimate,
                  content_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'active', ?14, ?14)",
                params![
                    document_id,
                    relative_path,
                    chunk.chunk_index as i64,
                    serde_json::to_string(&chunk.heading_path)?,
                    chunk.content,
                    chunk.snippet,
                    chunk.start_line as i64,
                    chunk.end_line as i64,
                    chunk.char_start as i64,
                    chunk.char_end as i64,
                    chunk.char_count as i64,
                    chunk.token_estimate as i64,
                    stable_hash(&format!("{}:{}", relative_path, chunk.content)),
                    now
                ],
            )?;
        }
        transaction.execute(
            "UPDATE rag_documents SET chunk_count = ?1 WHERE id = ?2",
            params![chunks.len() as i64, document_id],
        )?;
        stats.indexed_count += 1;
        stats.chunk_count += chunks.len() as i64;
    }

    stats.deleted_count = mark_deleted_documents(&transaction, &seen)?;
    transaction.commit()?;
    Ok(stats)
}

fn mark_deleted_documents(
    connection: &rusqlite::Transaction<'_>,
    seen: &HashSet<String>,
) -> ServiceResult<i64> {
    let mut statement = connection
        .prepare("SELECT id, relative_path FROM rag_documents WHERE status = 'active'")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let documents = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let now = Utc::now().to_rfc3339();
    let mut deleted = 0;
    for (id, relative_path) in documents {
        if seen.contains(&relative_path) {
            continue;
        }
        connection.execute(
            "UPDATE rag_documents
             SET status = 'deleted', deleted_at = ?1, chunk_count = 0
             WHERE id = ?2",
            params![now, id],
        )?;
        connection.execute(
            "UPDATE rag_chunks SET status = 'deleted', updated_at = ?1 WHERE document_id = ?2",
            params![now, id],
        )?;
        deleted += 1;
    }
    Ok(deleted)
}

fn parse_indexable_content(relative_path: &str, raw: &str) -> ServiceResult<(String, String)> {
    let title = Path::new(relative_path)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(relative_path)
        .to_string();
    if is_markdown_path(relative_path) {
        let parsed = match parse_markdown(raw) {
            Ok(parsed) => parsed,
            Err(_) => return Ok((title, raw.to_string())),
        };
        let title = parsed
            .frontmatter
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&title)
            .to_string();
        Ok((title, parsed.content))
    } else {
        Ok((title, raw.to_string()))
    }
}

fn scan_supported_files(root: &Path) -> ServiceResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    scan_dir(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn scan_dir(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> ServiceResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            scan_dir(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() || !is_supported_file(&path) {
            continue;
        }
        let relative = relative_path(root, &path)?;
        if relative == format!("{INBOX_DIR}/{LEDGER_FILE}") {
            continue;
        }
        files.push(path);
    }
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some(INTERNAL_DIR)
            | Some(".secrets")
            | Some(".git")
            | Some("node_modules")
            | Some("target")
            | Some("dist")
    )
}

fn is_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt"))
        .unwrap_or(false)
}

fn is_markdown_path(relative_path: &str) -> bool {
    relative_path
        .rsplit('.')
        .next()
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown"))
        .unwrap_or(false)
}

fn relative_path(root: &Path, path: &Path) -> ServiceResult<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ServiceError::EscapedVault(path.to_string_lossy().to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    normalize_relative_path(&relative)
}

fn stable_hash(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn run_by_id(connection: &Connection, id: i64) -> ServiceResult<RagIndexRun> {
    connection
        .query_row(
            "SELECT id, status, started_at, finished_at, scanned_count, indexed_count,
                    skipped_count, deleted_count, chunk_count, error
             FROM rag_index_runs
             WHERE id = ?1",
            params![id],
            |row| {
                Ok(RagIndexRun {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    started_at: row.get(2)?,
                    finished_at: row.get(3)?,
                    scanned_count: row.get(4)?,
                    indexed_count: row.get(5)?,
                    skipped_count: row.get(6)?,
                    deleted_count: row.get(7)?,
                    chunk_count: row.get(8)?,
                    error: row.get(9)?,
                })
            },
        )
        .map_err(Into::into)
}

fn citation_from_chunk(chunk: &RetrievedChunk) -> RagCitation {
    RagCitation {
        id: chunk.citation_id.clone(),
        relative_path: chunk.relative_path.clone(),
        title: chunk.title.clone(),
        heading_path: chunk.heading_path.clone(),
        snippet: chunk.snippet.clone(),
        score: chunk.score,
        channel: chunk.channel.clone(),
        start_line: chunk.start_line,
        end_line: chunk.end_line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn rebuild_indexes_markdown_and_skips_inbox_ledger() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        fs::write(
            temp.path().join("100-School").join("ai.md"),
            "---\ntitle: 人工智能\n---\n# 机器学习\n使用数据训练模型。",
        )
        .unwrap();
        fs::write(
            temp.path().join(INBOX_DIR).join(LEDGER_FILE),
            "- [[../100-School/ai.md]]\n",
        )
        .unwrap();

        let run = RagService::rebuild_index(temp.path().to_str().unwrap()).unwrap();
        let status = RagService::status(temp.path().to_str().unwrap()).unwrap();

        assert_eq!(run.status, "ok");
        assert_eq!(status.document_count, 1);
        assert!(status.chunk_count >= 1);
    }

    #[test]
    fn ask_without_network_returns_local_citation_and_trace() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        fs::write(
            temp.path().join("100-School").join("ai.md"),
            "# 机器学习\n机器学习模型使用数据训练。",
        )
        .unwrap();
        RagService::rebuild_index(temp.path().to_str().unwrap()).unwrap();

        let answer = RagService::ask_with_options(
            temp.path().to_str().unwrap(),
            "机器学习模型",
            Some(4),
            None,
            None,
            false,
        )
        .unwrap();

        assert!(answer.is_mock);
        assert_eq!(answer.status, "forced_mock");
        assert_eq!(answer.citations[0].id, "S1");
        assert_eq!(answer.citations[0].relative_path, "100-School/ai.md");
        assert!(!answer.trace.nodes.is_empty());
    }

    #[test]
    fn conversations_create_list_and_get_empty_messages() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let first = RagService::create_conversation(temp.path().to_str().unwrap(), None).unwrap();
        let second = RagService::create_conversation(
            temp.path().to_str().unwrap(),
            Some("  Project Q&A  ".to_string()),
        )
        .unwrap();
        let list = RagService::list_conversations(temp.path().to_str().unwrap(), Some(10)).unwrap();
        let detail = RagService::get_conversation(temp.path().to_str().unwrap(), first.id).unwrap();

        assert_eq!(first.title, "新会话");
        assert_eq!(second.title, "Project Q&A");
        assert_eq!(list[0].id, second.id);
        assert_eq!(detail.id, first.id);
        assert!(detail.messages.is_empty());
    }

    #[test]
    fn ask_with_conversation_persists_user_and_assistant_messages() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("Projects")).unwrap();
        fs::write(
            temp.path().join("Projects").join("alpha.md"),
            "# Alpha\nalpha planning context",
        )
        .unwrap();
        RagService::rebuild_index(temp.path().to_str().unwrap()).unwrap();
        let conversation =
            RagService::create_conversation(temp.path().to_str().unwrap(), None).unwrap();

        let answer = RagService::ask_with_options(
            temp.path().to_str().unwrap(),
            "alpha",
            Some(4),
            Some(conversation.id),
            None,
            false,
        )
        .unwrap();
        let detail =
            RagService::get_conversation(temp.path().to_str().unwrap(), conversation.id).unwrap();

        assert_eq!(detail.messages.len(), 2);
        assert_eq!(detail.messages[0].role, "user");
        assert_eq!(detail.messages[0].content, "alpha");
        assert_eq!(detail.messages[0].query_id, None);
        assert_eq!(detail.messages[1].role, "assistant");
        assert_eq!(detail.messages[1].content, answer.answer);
        assert_eq!(detail.messages[1].query_id, Some(answer.query_id));
        assert_eq!(detail.updated_at, detail.messages[1].created_at);
    }

    #[test]
    fn ask_with_invalid_conversation_id_returns_error_before_messages() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        let error = RagService::ask_with_options(
            temp.path().to_str().unwrap(),
            "alpha",
            Some(4),
            Some(999),
            None,
            false,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("conversation 999 does not exist"));
        let list = RagService::list_conversations(temp.path().to_str().unwrap(), None).unwrap();
        assert!(list.is_empty());
    }
}
