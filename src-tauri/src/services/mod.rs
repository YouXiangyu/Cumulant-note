pub mod ai;
pub mod audit;
pub mod budget;
pub mod candidates;
pub mod chunking;
pub mod conflict_rules;
pub mod dedupe;
pub mod importer;
pub mod inbox;
pub mod index;
pub mod ledger;
pub mod listener;
pub mod markdown;
pub mod movement;
pub mod organizer;
pub mod queue;
pub mod rag;
pub mod rag_trace;
pub mod retrieval;
pub mod settings;
pub mod sticky;
pub mod usage;
pub mod vault;
pub mod worker;

use thiserror::Error;

pub type ServiceResult<T> = Result<T, ServiceError>;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid vault path: {0}")]
    InvalidVault(String),
    #[error("path is outside the selected vault: {0}")]
    EscapedVault(String),
    #[error("invalid relative path: {0}")]
    InvalidRelativePath(String),
    #[error("unsupported file type: {0}")]
    UnsupportedFileType(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
}
