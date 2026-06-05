use crate::services::vault::{normalize_relative_path, INBOX_DIR, INTERNAL_DIR, LEDGER_FILE};
use crate::services::{ServiceError, ServiceResult};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RagScope {
    AllVault,
    CurrentFile {
        #[serde(rename = "relativePath")]
        relative_path: String,
    },
    ProjectPrefix {
        #[serde(rename = "relativePathPrefix")]
        relative_path_prefix: String,
    },
}

impl Default for RagScope {
    fn default() -> Self {
        Self::AllVault
    }
}

impl RagScope {
    pub fn validate(&self) -> ServiceResult<()> {
        ScopeFilter::from_scope(Some(self)).map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievedChunk {
    pub chunk_id: i64,
    pub document_id: i64,
    pub relative_path: String,
    pub title: String,
    pub heading_path: Vec<String>,
    pub content: String,
    pub snippet: String,
    pub score: f64,
    pub channel: String,
    pub start_line: i64,
    pub end_line: i64,
    pub citation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchChannelSummary {
    pub channel: String,
    pub candidate_count: usize,
    pub returned_count: usize,
    pub max_score: f64,
}

#[derive(Debug, Clone)]
pub struct RetrievalOutput {
    pub results: Vec<RetrievedChunk>,
    pub summaries: Vec<SearchChannelSummary>,
}

#[derive(Debug, Clone)]
struct IndexedChunk {
    chunk_id: i64,
    document_id: i64,
    relative_path: String,
    title: String,
    heading_path: Vec<String>,
    content: String,
    snippet: String,
    start_line: i64,
    end_line: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScopeFilter {
    AllVault,
    CurrentFile(String),
    ProjectPrefix(String),
}

impl ScopeFilter {
    fn from_scope(scope: Option<&RagScope>) -> ServiceResult<Self> {
        match scope {
            None | Some(RagScope::AllVault) => Ok(Self::AllVault),
            Some(RagScope::CurrentFile { relative_path }) => Ok(Self::CurrentFile(
                validate_scope_path("currentFile", relative_path)?,
            )),
            Some(RagScope::ProjectPrefix {
                relative_path_prefix,
            }) => Ok(Self::ProjectPrefix(validate_scope_path(
                "projectPrefix",
                relative_path_prefix,
            )?)),
        }
    }

    fn matches(&self, relative_path: &str) -> bool {
        match self {
            Self::AllVault => true,
            Self::CurrentFile(path) => relative_path == path,
            Self::ProjectPrefix(prefix) => {
                relative_path == prefix || relative_path.starts_with(&format!("{prefix}/"))
            }
        }
    }
}

pub fn retrieve(
    connection: &Connection,
    question: &str,
    top_k: usize,
) -> ServiceResult<RetrievalOutput> {
    retrieve_with_scope(connection, question, top_k, None)
}

pub fn retrieve_with_scope(
    connection: &Connection,
    question: &str,
    top_k: usize,
    scope: Option<&RagScope>,
) -> ServiceResult<RetrievalOutput> {
    let filter = ScopeFilter::from_scope(scope)?;
    let chunks = fetch_active_chunks(connection, &filter)?;
    let keyword = rank_channel(
        "keyword",
        &chunks,
        question,
        top_k.saturating_mul(4).max(top_k),
        keyword_score,
    );
    let semantic = rank_channel(
        "local_semantic_placeholder",
        &chunks,
        question,
        top_k.saturating_mul(4).max(top_k),
        local_semantic_placeholder_score,
    );
    let summaries = vec![
        summarize("keyword", chunks.len(), &keyword),
        summarize("local_semantic_placeholder", chunks.len(), &semantic),
    ];
    let mut results = merge_and_rank(keyword.into_iter().chain(semantic), top_k);
    for (index, result) in results.iter_mut().enumerate() {
        result.citation_id = format!("S{}", index + 1);
    }
    Ok(RetrievalOutput { results, summaries })
}

pub fn format_context(results: &[RetrievedChunk]) -> String {
    results
        .iter()
        .map(|item| {
            let heading = if item.heading_path.is_empty() {
                item.title.clone()
            } else {
                item.heading_path.join(" > ")
            };
            format!(
                "[{}] {}\n标题路径: {}\n行: {}-{}\n内容:\n{}",
                item.citation_id,
                item.relative_path,
                heading,
                item.start_line,
                item.end_line,
                item.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub fn summaries_json(summaries: &[SearchChannelSummary]) -> Value {
    json!(summaries)
}

fn fetch_active_chunks(
    connection: &Connection,
    filter: &ScopeFilter,
) -> ServiceResult<Vec<IndexedChunk>> {
    let mut statement = connection.prepare(
        "SELECT
            c.id,
            c.document_id,
            c.relative_path,
            d.title,
            c.heading_path,
            c.content,
            c.snippet,
            c.start_line,
            c.end_line
         FROM rag_chunks c
         JOIN rag_documents d ON d.id = c.document_id
         WHERE c.status = 'active' AND d.status = 'active'",
    )?;
    let rows = statement.query_map([], |row| {
        let heading_json: String = row.get(4)?;
        let heading_path = serde_json::from_str::<Vec<String>>(&heading_json).unwrap_or_default();
        Ok(IndexedChunk {
            chunk_id: row.get(0)?,
            document_id: row.get(1)?,
            relative_path: row.get(2)?,
            title: row.get(3)?,
            heading_path,
            content: row.get(5)?,
            snippet: row.get(6)?,
            start_line: row.get(7)?,
            end_line: row.get(8)?,
        })
    })?;
    let mut chunks = Vec::new();
    for row in rows {
        let chunk = row?;
        if filter.matches(&chunk.relative_path) {
            chunks.push(chunk);
        }
    }
    Ok(chunks)
}

fn validate_scope_path(kind: &str, raw_path: &str) -> ServiceResult<String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::InvalidRelativePath(format!(
            "{kind} scope path must not be empty"
        )));
    }
    if trimmed.contains('\\') {
        return Err(ServiceError::InvalidRelativePath(trimmed.to_string()));
    }
    let normalized = normalize_relative_path(trimmed)?;
    if normalized == format!("{INBOX_DIR}/{LEDGER_FILE}") {
        return Err(ServiceError::InvalidRelativePath(normalized));
    }
    if normalized
        .split('/')
        .any(|component| is_blocked_scope_component(component))
    {
        return Err(ServiceError::InvalidRelativePath(normalized));
    }
    Ok(normalized)
}

fn is_blocked_scope_component(component: &str) -> bool {
    [
        INTERNAL_DIR,
        ".secrets",
        ".git",
        "node_modules",
        "target",
        "dist",
    ]
    .iter()
    .any(|blocked| component.eq_ignore_ascii_case(blocked))
}

fn rank_channel(
    channel: &str,
    chunks: &[IndexedChunk],
    question: &str,
    limit: usize,
    score_fn: fn(&IndexedChunk, &str) -> f64,
) -> Vec<RetrievedChunk> {
    let mut results = chunks
        .iter()
        .filter_map(|chunk| {
            let score = score_fn(chunk, question);
            (score > 0.0).then(|| RetrievedChunk {
                chunk_id: chunk.chunk_id,
                document_id: chunk.document_id,
                relative_path: chunk.relative_path.clone(),
                title: chunk.title.clone(),
                heading_path: chunk.heading_path.clone(),
                content: chunk.content.clone(),
                snippet: chunk.snippet.clone(),
                score,
                channel: channel.to_string(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                citation_id: String::new(),
            })
        })
        .collect::<Vec<_>>();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });
    results.truncate(limit);
    normalize_scores(&mut results);
    results
}

fn summarize(
    channel: &str,
    candidate_count: usize,
    results: &[RetrievedChunk],
) -> SearchChannelSummary {
    SearchChannelSummary {
        channel: channel.to_string(),
        candidate_count,
        returned_count: results.len(),
        max_score: results.first().map(|item| item.score).unwrap_or(0.0),
    }
}

fn merge_and_rank<I>(items: I, top_k: usize) -> Vec<RetrievedChunk>
where
    I: IntoIterator<Item = RetrievedChunk>,
{
    let mut merged: HashMap<i64, RetrievedChunk> = HashMap::new();
    for item in items {
        merged
            .entry(item.chunk_id)
            .and_modify(|existing| {
                existing.score += item.score;
                if !existing.channel.contains(&item.channel) {
                    existing.channel = format!("{}+{}", existing.channel, item.channel);
                }
            })
            .or_insert(item);
    }
    let mut results = merged.into_values().collect::<Vec<_>>();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.relative_path.cmp(&b.relative_path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
    results.truncate(top_k);
    results
}

fn normalize_scores(results: &mut [RetrievedChunk]) {
    let max_score = results
        .iter()
        .map(|item| item.score)
        .fold(0.0_f64, f64::max);
    if max_score <= 0.0 {
        return;
    }
    for item in results {
        item.score = item.score / max_score;
    }
}

fn keyword_score(chunk: &IndexedChunk, question: &str) -> f64 {
    let query = question.trim().to_lowercase();
    if query.is_empty() {
        return 0.0;
    }
    let text = searchable_text(chunk).to_lowercase();
    let mut score = 0.0;
    if text.contains(&query) {
        score += 6.0;
    }
    let content_tokens = tokenize(&text);
    for token in unique_tokens(question) {
        let count = content_tokens
            .iter()
            .filter(|candidate| **candidate == token)
            .count();
        score += count as f64;
    }
    if heading_text(chunk).to_lowercase().contains(&query) {
        score += 2.0;
    }
    score
}

fn local_semantic_placeholder_score(chunk: &IndexedChunk, question: &str) -> f64 {
    let query_tokens = unique_tokens(question);
    if query_tokens.is_empty() {
        return 0.0;
    }
    let text_tokens = unique_tokens(&searchable_text(chunk));
    let overlap = query_tokens
        .iter()
        .filter(|token| text_tokens.contains(*token))
        .count();
    if overlap == 0 {
        return 0.0;
    }
    let heading_tokens = unique_tokens(&heading_text(chunk));
    let heading_overlap = query_tokens
        .iter()
        .filter(|token| heading_tokens.contains(*token))
        .count();
    overlap as f64 / query_tokens.len() as f64 + heading_overlap as f64 * 0.25
}

fn searchable_text(chunk: &IndexedChunk) -> String {
    format!(
        "{}\n{}\n{}",
        chunk.title,
        heading_text(chunk),
        chunk.content
    )
}

fn heading_text(chunk: &IndexedChunk) -> String {
    chunk.heading_path.join(" ")
}

fn unique_tokens(text: &str) -> HashSet<String> {
    tokenize(text).into_iter().collect()
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for char in text.chars() {
        if char.is_ascii_alphanumeric() {
            current.push(char.to_ascii_lowercase());
            continue;
        }
        if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
        if !char.is_ascii()
            && !char.is_whitespace()
            && !char.is_ascii_punctuation()
            && (char.is_alphabetic() || char.is_numeric())
        {
            tokens.push(char.to_lowercase().collect());
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
        .into_iter()
        .filter(|token| token.chars().count() > 1 || !token.is_ascii())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::index::IndexService;
    use rusqlite::params;

    fn insert_indexed_chunk(connection: &Connection, relative_path: &str, content: &str) {
        let now = "2026-06-02T00:00:00Z";
        connection
            .execute(
                "INSERT INTO rag_documents
                 (relative_path, title, content_hash, modified_at, size_bytes, status, chunk_count, indexed_at)
                 VALUES (?1, ?2, ?3, 0, 0, 'active', 1, ?4)",
                params![
                    relative_path,
                    relative_path,
                    format!("doc-hash:{relative_path}"),
                    now
                ],
            )
            .unwrap();
        let document_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO rag_chunks
                 (document_id, relative_path, chunk_index, heading_path, content, snippet, start_line, end_line,
                  char_start, char_end, char_count, token_estimate, content_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, 0, ?3, ?4, ?5, 1, 3, 0, 20, 20, 5, ?6, 'active', ?7, ?7)",
                params![
                    document_id,
                    relative_path,
                    serde_json::to_string(&Vec::<String>::new()).unwrap(),
                    content,
                    content,
                    format!("chunk-hash:{relative_path}"),
                    now
                ],
            )
            .unwrap();
    }

    fn result_paths(output: &RetrievalOutput) -> Vec<String> {
        output
            .results
            .iter()
            .map(|item| item.relative_path.clone())
            .collect()
    }

    #[test]
    fn keyword_and_placeholder_channels_return_ranked_citations() {
        let temp = tempfile::tempdir().unwrap();
        let db = IndexService::open_or_create(temp.path()).unwrap();
        let connection = Connection::open(db.path).unwrap();
        let now = "2026-06-02T00:00:00Z";
        connection
            .execute(
                "INSERT INTO rag_documents
                 (relative_path, title, content_hash, modified_at, size_bytes, status, chunk_count, indexed_at)
                 VALUES (?1, ?2, ?3, 0, 0, 'active', 1, ?4)",
                params!["100-School/ai.md", "人工智能", "doc-hash", now],
            )
            .unwrap();
        let document_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO rag_chunks
                 (document_id, relative_path, chunk_index, heading_path, content, snippet, start_line, end_line,
                  char_start, char_end, char_count, token_estimate, content_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, 0, ?3, ?4, ?5, 1, 3, 0, 20, 20, 5, ?6, 'active', ?7, ?7)",
                params![
                    document_id,
                    "100-School/ai.md",
                    serde_json::to_string(&vec!["人工智能"]).unwrap(),
                    "机器学习 使用 数据 训练 模型",
                    "机器学习 使用 数据",
                    "chunk-hash",
                    now
                ],
            )
            .unwrap();

        let output = retrieve(&connection, "机器学习模型", 3).unwrap();

        assert_eq!(output.results.len(), 1);
        assert_eq!(output.results[0].citation_id, "S1");
        assert!(output.results[0].score > 0.0);
        assert_eq!(output.summaries.len(), 2);
    }

    #[test]
    fn all_vault_scope_matches_existing_retrieval() {
        let temp = tempfile::tempdir().unwrap();
        let db = IndexService::open_or_create(temp.path()).unwrap();
        let connection = Connection::open(db.path).unwrap();
        insert_indexed_chunk(&connection, "100-School/a.md", "alpha shared topic");
        insert_indexed_chunk(&connection, "200-Life/b.md", "alpha shared topic");

        let default_output = retrieve(&connection, "alpha", 10).unwrap();
        let scoped_output =
            retrieve_with_scope(&connection, "alpha", 10, Some(&RagScope::AllVault)).unwrap();

        assert_eq!(result_paths(&default_output), result_paths(&scoped_output));
        assert_eq!(scoped_output.results.len(), 2);
    }

    #[test]
    fn current_file_scope_only_matches_exact_relative_path() {
        let temp = tempfile::tempdir().unwrap();
        let db = IndexService::open_or_create(temp.path()).unwrap();
        let connection = Connection::open(db.path).unwrap();
        insert_indexed_chunk(&connection, "Projects/App/a.md", "alpha scoped note");
        insert_indexed_chunk(&connection, "Projects/App/b.md", "alpha sibling note");

        let output = retrieve_with_scope(
            &connection,
            "alpha",
            10,
            Some(&RagScope::CurrentFile {
                relative_path: "Projects/App/a.md".to_string(),
            }),
        )
        .unwrap();

        assert_eq!(result_paths(&output), vec!["Projects/App/a.md"]);
    }

    #[test]
    fn project_prefix_scope_matches_directory_without_similar_prefixes() {
        let temp = tempfile::tempdir().unwrap();
        let db = IndexService::open_or_create(temp.path()).unwrap();
        let connection = Connection::open(db.path).unwrap();
        insert_indexed_chunk(&connection, "Project/note.md", "alpha project note");
        insert_indexed_chunk(
            &connection,
            "Project/Sub/deep.md",
            "alpha nested project note",
        );
        insert_indexed_chunk(&connection, "ProjectX/note.md", "alpha wrong project note");

        let output = retrieve_with_scope(
            &connection,
            "alpha",
            10,
            Some(&RagScope::ProjectPrefix {
                relative_path_prefix: "Project".to_string(),
            }),
        )
        .unwrap();

        let paths = result_paths(&output);
        assert!(paths.contains(&"Project/note.md".to_string()));
        assert!(paths.contains(&"Project/Sub/deep.md".to_string()));
        assert!(!paths.contains(&"ProjectX/note.md".to_string()));
    }

    #[test]
    fn scope_rejects_traversal_internal_tool_and_backslash_paths() {
        let temp = tempfile::tempdir().unwrap();
        let db = IndexService::open_or_create(temp.path()).unwrap();
        let connection = Connection::open(db.path).unwrap();
        let invalid_scopes = vec![
            RagScope::CurrentFile {
                relative_path: "../secret.md".to_string(),
            },
            RagScope::CurrentFile {
                relative_path: "/outside.md".to_string(),
            },
            RagScope::CurrentFile {
                relative_path: "Project\\note.md".to_string(),
            },
            RagScope::CurrentFile {
                relative_path: ".thebrain/rules/a.md".to_string(),
            },
            RagScope::CurrentFile {
                relative_path: ".secrets/key.txt".to_string(),
            },
            RagScope::ProjectPrefix {
                relative_path_prefix: "notes/node_modules".to_string(),
            },
            RagScope::ProjectPrefix {
                relative_path_prefix: "target".to_string(),
            },
            RagScope::ProjectPrefix {
                relative_path_prefix: "dist".to_string(),
            },
            RagScope::CurrentFile {
                relative_path: format!("{INBOX_DIR}/{LEDGER_FILE}"),
            },
            RagScope::ProjectPrefix {
                relative_path_prefix: " ".to_string(),
            },
        ];

        for scope in invalid_scopes {
            assert!(retrieve_with_scope(&connection, "alpha", 10, Some(&scope)).is_err());
        }
    }
}
