use crate::services::archive_map::{ArchiveMapService, ArchiveMapSnapshot};
use crate::services::budget::{BudgetLedgerInput, BudgetService};
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, resolve_existing_file, INBOX_DIR,
};
use crate::services::{ServiceError, ServiceResult};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::Path;

const MIMO_BASE_URL: &str = "https://token-plan-cn.xiaomimimo.com/v1";
const MIMO_EXTRACT_MODEL: &str = "mimo-v2.5";
const MIMO_ORGANIZE_MODEL: &str = "mimo-v2.5-pro";
const MIMO_RAG_MODEL: &str = "mimo-v2.5-pro";
const MIN_AUTO_CONFIDENCE: f32 = 0.6;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MockAiDecision {
    pub provider: String,
    pub model: String,
    pub target_path: Option<String>,
    pub confidence: f32,
    pub reason: String,
}

pub trait AiProvider {
    fn name(&self) -> &'static str;
    fn organize_preview(&self, relative_path: &str) -> MockAiDecision;
}

pub struct MockAiProvider;

impl AiProvider for MockAiProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn organize_preview(&self, relative_path: &str) -> MockAiDecision {
        MockAiDecision {
            provider: self.name().to_string(),
            model: "mock-organizer-v0".to_string(),
            target_path: None,
            confidence: 0.0,
            reason: format!(
                "v0.1 only records the interface; no AI decision or file move is performed for {relative_path}."
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MimoExtractInput {
    pub relative_path: String,
    pub force_mock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MimoExtractResult {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub is_mock: bool,
    pub relative_path: String,
    pub text: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MimoStatus {
    pub provider: String,
    pub extract_model: String,
    pub organize_model: String,
    pub has_key: bool,
    pub key_source: Option<String>,
    pub key_has_bom: bool,
    pub key_needs_cleanup: bool,
    pub key_message: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MimoKeyInput {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeDecision {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub is_mock: bool,
    pub source_relative_path: String,
    pub target_relative_path: String,
    pub tags: Vec<String>,
    pub summary: String,
    pub reason: String,
    pub confidence: f32,
    pub todo_candidates: Vec<Value>,
    pub schedule_candidates: Vec<Value>,
    pub error: Option<String>,
}

pub struct MimoProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MimoRagAnswer {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub is_mock: bool,
    pub answer: String,
    pub error: Option<String>,
}

impl MimoProvider {
    pub fn status(vault_path: &str) -> ServiceResult<MimoStatus> {
        let key = lookup_mimo_key(vault_path)?;
        let has_key = key.value.is_some();
        let status = if has_key && key.had_bom {
            "key_warning"
        } else if has_key {
            "key_found"
        } else {
            "missing_key"
        };
        Ok(MimoStatus {
            provider: "mimo".to_string(),
            extract_model: MIMO_EXTRACT_MODEL.to_string(),
            organize_model: MIMO_ORGANIZE_MODEL.to_string(),
            has_key,
            key_source: key.source,
            key_has_bom: key.had_bom,
            key_needs_cleanup: key.had_bom,
            key_message: key.message,
            status: status.to_string(),
        })
    }

    pub fn save_api_key(vault_path: &str, input: MimoKeyInput) -> ServiceResult<MimoStatus> {
        let (cleaned, _) = normalize_mimo_key(&input.api_key);
        if cleaned.is_empty() {
            return Err(ServiceError::InvalidState(
                "MIMO_API_KEY must not be empty".to_string(),
            ));
        }
        let root = canonical_vault_root(vault_path)?;
        let secrets_dir = root.join(".secrets");
        fs::create_dir_all(&secrets_dir)?;
        fs::write(secrets_dir.join("mimo_api_key.txt"), cleaned.as_bytes())?;
        Self::status(vault_path)
    }

    pub fn extract_file(
        vault_path: &str,
        input: MimoExtractInput,
    ) -> ServiceResult<MimoExtractResult> {
        let relative_path = normalize_relative_path(&input.relative_path)?;
        let path = resolve_existing_file(vault_path, &relative_path)?;
        if input.force_mock {
            return Ok(fallback_extract(
                &relative_path,
                "forced_mock",
                "forced mock",
            ));
        }
        if !is_supported_extract_file(&path) {
            return Ok(fallback_extract(
                &relative_path,
                "unsupported_file_type",
                "unsupported v0.3 extract file type",
            ));
        }
        if is_text_file(&path) {
            return Ok(MimoExtractResult {
                provider: "local".to_string(),
                model: "text-read-v0.3".to_string(),
                status: "ok".to_string(),
                is_mock: false,
                relative_path,
                text: fs::read_to_string(path)?,
                error: None,
            });
        }
        let budget = BudgetService::status(vault_path)?;
        if !budget.can_run {
            return Ok(fallback_extract(
                &relative_path,
                "budget_blocked",
                "budget is paused or exhausted",
            ));
        }
        let key = lookup_mimo_key(vault_path)?;
        let Some(key) = key.value else {
            return Ok(fallback_extract(
                &relative_path,
                "missing_key",
                "missing MIMO_API_KEY or vault .secrets key",
            ));
        };
        match call_mimo_file(&key, &path, MIMO_EXTRACT_MODEL, extract_prompt()) {
            Ok(text) => {
                record_ai_usage(vault_path, MIMO_EXTRACT_MODEL, "extract", "ok");
                Ok(MimoExtractResult {
                    provider: "mimo".to_string(),
                    model: MIMO_EXTRACT_MODEL.to_string(),
                    status: "ok".to_string(),
                    is_mock: false,
                    relative_path,
                    text,
                    error: None,
                })
            }
            Err(error) => {
                let status = classify_mimo_error(&error);
                record_ai_usage(vault_path, MIMO_EXTRACT_MODEL, "extract", status);
                Ok(fallback_extract(&relative_path, status, &error.to_string()))
            }
        }
    }

    pub fn organize_decision(
        vault_path: &str,
        source_relative_path: &str,
        extracted_text: Option<&str>,
        force_mock: bool,
    ) -> ServiceResult<OrganizeDecision> {
        let source = normalize_relative_path(source_relative_path)?;
        if force_mock {
            return Ok(fallback_decision(&source, "forced_mock", "forced mock"));
        }
        let budget = BudgetService::status(vault_path)?;
        if !budget.can_run {
            return Ok(fallback_decision(
                &source,
                "budget_blocked",
                "budget is paused or exhausted",
            ));
        }
        let key = lookup_mimo_key(vault_path)?;
        let Some(key) = key.value else {
            return Ok(fallback_decision(
                &source,
                "missing_key",
                "missing MIMO_API_KEY or vault .secrets key",
            ));
        };
        let archive_map = ArchiveMapService::latest_or_rebuild(vault_path)?;
        let prompt = organize_prompt(&source, extracted_text.unwrap_or(""), &archive_map.markdown);
        match call_mimo_text(&key, MIMO_ORGANIZE_MODEL, &prompt) {
            Ok(text) => {
                match parse_organize_response(vault_path, &source, &text, Some(&archive_map)) {
                    Ok(mut decision) => {
                        record_ai_usage(
                            vault_path,
                            MIMO_ORGANIZE_MODEL,
                            "organize",
                            &decision.status,
                        );
                        decision.provider = "mimo".to_string();
                        decision.model = MIMO_ORGANIZE_MODEL.to_string();
                        Ok(decision)
                    }
                    Err(error) => {
                        record_ai_usage(vault_path, MIMO_ORGANIZE_MODEL, "organize", "parse_error");
                        Ok(fallback_decision(
                            &source,
                            "parse_error",
                            &error.to_string(),
                        ))
                    }
                }
            }
            Err(error) => {
                let status = classify_mimo_error(&error);
                record_ai_usage(vault_path, MIMO_ORGANIZE_MODEL, "organize", status);
                Ok(fallback_decision(&source, status, &error.to_string()))
            }
        }
    }

    pub fn answer_rag(
        vault_path: &str,
        question: &str,
        formatted_context: &str,
        allow_network: bool,
    ) -> ServiceResult<MimoRagAnswer> {
        if formatted_context.trim().is_empty() {
            return Ok(fallback_rag_answer(
                "no_context",
                "no local RAG context was retrieved",
                question,
            ));
        }
        if !allow_network {
            return Ok(fallback_rag_answer(
                "forced_mock",
                "network call disabled for verification",
                question,
            ));
        }
        let budget = BudgetService::status(vault_path)?;
        if !budget.can_run {
            return Ok(fallback_rag_answer(
                "budget_blocked",
                "budget is paused or exhausted",
                question,
            ));
        }
        let key = lookup_mimo_key(vault_path)?;
        let Some(key) = key.value else {
            return Ok(fallback_rag_answer(
                "missing_key",
                "missing MIMO_API_KEY or vault .secrets key",
                question,
            ));
        };
        let prompt = rag_prompt(question, formatted_context);
        match call_mimo_text(&key, MIMO_RAG_MODEL, &prompt) {
            Ok(answer) => {
                record_ai_usage(vault_path, MIMO_RAG_MODEL, "rag", "ok");
                Ok(MimoRagAnswer {
                    provider: "mimo".to_string(),
                    model: MIMO_RAG_MODEL.to_string(),
                    status: "ok".to_string(),
                    is_mock: false,
                    answer,
                    error: None,
                })
            }
            Err(error) => {
                let status = classify_mimo_error(&error);
                record_ai_usage(vault_path, MIMO_RAG_MODEL, "rag", status);
                Ok(fallback_rag_answer(status, &error.to_string(), question))
            }
        }
    }
}

#[derive(Debug)]
struct KeyLookup {
    value: Option<String>,
    source: Option<String>,
    had_bom: bool,
    message: Option<String>,
}

fn lookup_mimo_key(vault_path: &str) -> ServiceResult<KeyLookup> {
    if let Ok(value) = env::var("MIMO_API_KEY") {
        let (trimmed, had_bom) = normalize_mimo_key(&value);
        if !trimmed.is_empty() {
            return Ok(KeyLookup {
                value: Some(trimmed),
                source: Some("env:MIMO_API_KEY".to_string()),
                had_bom,
                message: key_message(had_bom, "env:MIMO_API_KEY"),
            });
        }
    }
    let root = canonical_vault_root(vault_path)?;
    let local_key = root.join(".secrets").join("mimo_api_key.txt");
    if local_key.exists() {
        let value = fs::read_to_string(local_key)?;
        let (trimmed, had_bom) = normalize_mimo_key(&value);
        if !trimmed.is_empty() {
            return Ok(KeyLookup {
                value: Some(trimmed),
                source: Some("vault:.secrets/mimo_api_key.txt".to_string()),
                had_bom,
                message: key_message(had_bom, "vault:.secrets/mimo_api_key.txt"),
            });
        }
    }
    if let Ok(current_dir) = env::current_dir() {
        let project_key = current_dir.join(".secrets").join("mimo_api_key.txt");
        if project_key.exists() {
            let value = fs::read_to_string(project_key)?;
            let (trimmed, had_bom) = normalize_mimo_key(&value);
            if !trimmed.is_empty() {
                return Ok(KeyLookup {
                    value: Some(trimmed),
                    source: Some("cwd:.secrets/mimo_api_key.txt".to_string()),
                    had_bom,
                    message: key_message(had_bom, "cwd:.secrets/mimo_api_key.txt"),
                });
            }
        }
    }
    Ok(KeyLookup {
        value: None,
        source: None,
        had_bom: false,
        message: None,
    })
}

fn normalize_mimo_key(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    let had_bom = trimmed.starts_with('\u{feff}');
    let cleaned = trimmed.trim_start_matches('\u{feff}').trim().to_string();
    (cleaned, had_bom)
}

fn key_message(had_bom: bool, source: &str) -> Option<String> {
    had_bom.then(|| {
        format!("{source} contains a UTF-8 BOM; runtime calls use a sanitized key. Save the key again to clean the file.")
    })
}

fn call_mimo_file(key: &str, path: &Path, model: &str, prompt: &str) -> ServiceResult<String> {
    let data = fs::read(path)?;
    let mime = mime_for_path(path)
        .ok_or_else(|| ServiceError::UnsupportedFileType(path.to_string_lossy().to_string()))?;
    let data_url = format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(data)
    );
    let media_payload = if mime.starts_with("image/") {
        json!({"type": "image_url", "image_url": {"url": data_url}})
    } else {
        json!({"type": "input_audio", "input_audio": {"data": data_url}})
    };
    call_mimo_payload(
        key,
        json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": [media_payload, {"type": "text", "text": prompt}]
            }],
            "stream": false,
            "thinking": {"type": "disabled"}
        }),
    )
}

fn call_mimo_text(key: &str, model: &str, prompt: &str) -> ServiceResult<String> {
    call_mimo_payload(
        key,
        json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "thinking": {"type": "disabled"}
        }),
    )
}

fn call_mimo_payload(key: &str, payload: Value) -> ServiceResult<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| ServiceError::InvalidState(format!("mimo client error: {error}")))?;
    let response = client
        .post(format!("{MIMO_BASE_URL}/chat/completions"))
        .header("api-key", key)
        .json(&payload)
        .send()
        .map_err(|error| ServiceError::InvalidState(format!("mimo network error: {error}")))?;
    if !response.status().is_success() {
        return Err(ServiceError::InvalidState(format!(
            "mimo api returned status {}",
            response.status()
        )));
    }
    let body: Value = response
        .json()
        .map_err(|error| ServiceError::InvalidState(format!("mimo json error: {error}")))?;
    let content = body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        Err(ServiceError::InvalidState(
            "mimo response content is empty".to_string(),
        ))
    } else {
        Ok(content)
    }
}

fn parse_organize_response(
    vault_path: &str,
    source: &str,
    text: &str,
    archive_map: Option<&ArchiveMapSnapshot>,
) -> ServiceResult<OrganizeDecision> {
    let raw_json = extract_json_object(text).ok_or_else(|| {
        ServiceError::InvalidState("mimo organize response did not contain JSON".to_string())
    })?;
    let value: Value = serde_json::from_str(raw_json)?;
    let target_raw = string_field(&value, &["targetRelativePath", "target_relative_path"])
        .ok_or_else(|| ServiceError::InvalidState("missing targetRelativePath".to_string()))?;
    let target = normalize_relative_path(&target_raw)?;
    let mut status = "ok".to_string();
    let mut error = None;

    if target.starts_with(&format!("{INBOX_DIR}/")) {
        status = "pending".to_string();
        error = Some("target_inside_inbox".to_string());
    }
    let root = canonical_vault_root(vault_path)?;
    if root.join(&target).exists() {
        status = "pending".to_string();
        error = Some("target_conflict".to_string());
    }
    if status == "ok" {
        match target_parent_dir(&target) {
            Some(parent)
                if archive_map
                    .map(|map| ArchiveMapService::contains_directory(map, &parent))
                    .unwrap_or(false) => {}
            Some(_) => {
                status = "pending".to_string();
                error = Some("target_not_in_archive_map".to_string());
            }
            None => {
                status = "pending".to_string();
                error = Some("target_missing_archive_directory".to_string());
            }
        }
    }

    let confidence = number_field(&value, &["confidence"])
        .unwrap_or(0.0)
        .clamp(0.0, 1.0) as f32;
    if confidence < MIN_AUTO_CONFIDENCE && status == "ok" {
        status = "pending".to_string();
        error = Some("low_confidence".to_string());
    }

    Ok(OrganizeDecision {
        provider: "mimo".to_string(),
        model: MIMO_ORGANIZE_MODEL.to_string(),
        status,
        is_mock: false,
        source_relative_path: source.to_string(),
        target_relative_path: target,
        tags: string_array_field(&value, &["tags"]),
        summary: string_field(&value, &["summary"]).unwrap_or_default(),
        reason: string_field(&value, &["reason"]).unwrap_or_default(),
        confidence,
        todo_candidates: value_array_field(&value, &["todoCandidates", "todo_candidates"]),
        schedule_candidates: value_array_field(
            &value,
            &["scheduleCandidates", "schedule_candidates"],
        ),
        error,
    })
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (start <= end).then_some(&text[start..=end])
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn number_field(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_f64))
}

fn string_array_field(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn value_array_field(value: &Value, keys: &[&str]) -> Vec<Value> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .cloned()
        .unwrap_or_default()
}

fn target_parent_dir(target: &str) -> Option<String> {
    Path::new(target)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty() && path != ".")
        .and_then(|path| normalize_relative_path(&path).ok())
}

fn fallback_extract(relative_path: &str, status: &str, reason: &str) -> MimoExtractResult {
    MimoExtractResult {
        provider: "mock".to_string(),
        model: MIMO_EXTRACT_MODEL.to_string(),
        status: status.to_string(),
        is_mock: true,
        relative_path: relative_path.to_string(),
        text: format!("Fallback extracted text for {relative_path}."),
        error: Some(reason.to_string()),
    }
}

fn fallback_decision(source: &str, status: &str, reason: &str) -> OrganizeDecision {
    let file_name = Path::new(source)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "organized.md".to_string());
    OrganizeDecision {
        provider: "mock".to_string(),
        model: MIMO_ORGANIZE_MODEL.to_string(),
        status: status.to_string(),
        is_mock: true,
        source_relative_path: source.to_string(),
        target_relative_path: format!("100-Organized/{file_name}"),
        tags: vec!["inbox".to_string()],
        summary: "Fallback organization decision.".to_string(),
        reason: reason.to_string(),
        confidence: 0.35,
        todo_candidates: Vec::new(),
        schedule_candidates: Vec::new(),
        error: Some(reason.to_string()),
    }
}

fn fallback_rag_answer(status: &str, reason: &str, question: &str) -> MimoRagAnswer {
    MimoRagAnswer {
        provider: "mock".to_string(),
        model: MIMO_RAG_MODEL.to_string(),
        status: status.to_string(),
        is_mock: true,
        answer: format!(
            "未生成 MiMo 答案（{reason}）。已保留本地检索引用，可根据引用内容继续确认。问题：{question}"
        ),
        error: Some(reason.to_string()),
    }
}

fn record_ai_usage(vault_path: &str, model: &str, reason: &str, status: &str) {
    let _ = BudgetService::record_usage(
        vault_path,
        BudgetLedgerInput {
            scope: None,
            provider: "mimo".to_string(),
            model: model.to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost_cents: 0,
            reason: Some(format!("{reason}:{status}")),
        },
    );
}

fn classify_mimo_error(error: &ServiceError) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("network") || message.contains("timed out") {
        "network_error"
    } else if message.contains("401") || message.contains("403") || message.contains("auth") {
        "auth_error"
    } else if message.contains("json") || message.contains("empty") {
        "api_parse_error"
    } else {
        "api_error"
    }
}

fn extract_prompt() -> &'static str {
    "Extract the readable content from this file as concise Markdown. Do not include chain-of-thought."
}

fn organize_prompt(source: &str, text: &str, archive_map_markdown: &str) -> String {
    format!(
        r#"You organize a local Markdown knowledge vault.
You must use the Archive Map below as the source of truth for formal archive directories.
Return only valid JSON with this exact shape:
{{
  "sourceRelativePath": "{source}",
  "targetRelativePath": "100-Topic/file.md",
  "confidence": 0.0,
  "reason": "short reason",
  "tags": ["tag"],
  "summary": "short summary",
  "todoCandidates": [{{"title":"todo title","excerpt":"source quote or note","confidence":0.0}}],
  "scheduleCandidates": [{{"title":"event title","date":"YYYY-MM-DD","excerpt":"source quote or note","confidence":0.0}}]
}}
Rules:
- targetRelativePath must be relative to the vault, must not use .., and should not remain inside {INBOX_DIR}.
- Prefer a target directory that already appears in the Archive Map.
- If no existing Archive Map directory fits, suggest a new directory only with a clear reason and confidence below 0.6.
- Low confidence, a new directory, a target conflict, or a classification boundary issue will require user confirmation.
- Use confidence below 0.6 when unsure.
- Do not include Markdown fences or explanatory prose.

Archive Map:
{archive_map_markdown}

Extracted content:
{text}"#
    )
}

fn rag_prompt(question: &str, formatted_context: &str) -> String {
    format!(
        r#"你是 TheBrain 的本地知识库问答助手。
只根据给定的本地上下文回答。回答必须包含引用编号，例如 [S1]。
如果上下文不足以回答，直接说明不足，不要编造。
不要输出思维链。

问题：
{question}

本地上下文：
{formatted_context}"#
    )
}

fn is_text_file(path: &Path) -> bool {
    matches!(extension(path).as_str(), "md" | "markdown" | "txt")
}

fn is_supported_extract_file(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "md" | "markdown" | "txt" | "mp3" | "wav" | "m4a" | "aac" | "png" | "jpg" | "jpeg"
    )
}

fn mime_for_path(path: &Path) -> Option<&'static str> {
    match extension(path).as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "wav" => Some("audio/wav"),
        "m4a" => Some("audio/mp4"),
        "aac" => Some("audio/aac"),
        "mp3" => Some("audio/mpeg"),
        _ => None,
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn text_extract_reads_local_file_without_key() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let relative = format!("{INBOX_DIR}/note.txt");
        fs::write(temp.path().join(INBOX_DIR).join("note.txt"), "hello text").unwrap();

        let result = MimoProvider::extract_file(
            temp.path().to_str().unwrap(),
            MimoExtractInput {
                relative_path: relative,
                force_mock: false,
            },
        )
        .unwrap();

        assert!(!result.is_mock);
        assert_eq!(result.provider, "local");
        assert_eq!(result.text, "hello text");
    }

    #[test]
    fn audio_extract_has_clear_fallback_without_key() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let relative = format!("{INBOX_DIR}/audio.mp3");
        fs::write(
            temp.path().join(INBOX_DIR).join("audio.mp3"),
            b"not real audio",
        )
        .unwrap();

        let result = MimoProvider::extract_file(
            temp.path().to_str().unwrap(),
            MimoExtractInput {
                relative_path: relative,
                force_mock: false,
            },
        )
        .unwrap();

        assert!(result.is_mock);
        assert_eq!(result.status, "missing_key");
        assert_eq!(result.model, MIMO_EXTRACT_MODEL);
    }

    #[test]
    fn normalize_mimo_key_strips_utf8_bom() {
        let (key, had_bom) = normalize_mimo_key("\u{feff}secret-key\n");

        assert!(had_bom);
        assert_eq!(key, "secret-key");
    }

    #[test]
    fn save_api_key_writes_clean_secret_file() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();

        MimoProvider::save_api_key(
            temp.path().to_str().unwrap(),
            MimoKeyInput {
                api_key: "\u{feff}secret-key".to_string(),
            },
        )
        .unwrap();

        let bytes = fs::read(temp.path().join(".secrets").join("mimo_api_key.txt")).unwrap();
        assert_eq!(bytes, b"secret-key");
    }

    #[test]
    fn organize_parser_accepts_controlled_json() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        let archive_map =
            crate::services::archive_map::ArchiveMapService::rebuild(temp.path().to_str().unwrap())
                .unwrap();
        let source = format!("{INBOX_DIR}/note.md");
        let decision = parse_organize_response(
            temp.path().to_str().unwrap(),
            &source,
            r#"```json
            {
              "sourceRelativePath": "ignored.md",
              "targetRelativePath": "100-School/note.md",
              "confidence": 0.72,
              "reason": "class note",
              "tags": ["school"],
              "summary": "summary",
              "todoCandidates": [{"title":"Review","confidence":0.7}],
              "scheduleCandidates": []
            }
            ```"#,
            Some(&archive_map),
        )
        .unwrap();

        assert_eq!(decision.status, "ok");
        assert_eq!(decision.source_relative_path, source);
        assert_eq!(decision.target_relative_path, "100-School/note.md");
        assert_eq!(decision.todo_candidates.len(), 1);
    }

    #[test]
    fn organize_parser_rejects_escaping_target() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let archive_map =
            crate::services::archive_map::ArchiveMapService::rebuild(temp.path().to_str().unwrap())
                .unwrap();
        let source = format!("{INBOX_DIR}/note.md");
        let error = parse_organize_response(
            temp.path().to_str().unwrap(),
            &source,
            r#"{"targetRelativePath":"../outside.md","confidence":0.9}"#,
            Some(&archive_map),
        )
        .unwrap_err();

        assert!(matches!(error, ServiceError::InvalidRelativePath(_)));
    }

    #[test]
    fn organize_parser_marks_new_directories_pending_without_archive_map_match() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        fs::create_dir_all(temp.path().join("100-School")).unwrap();
        let archive_map =
            crate::services::archive_map::ArchiveMapService::rebuild(temp.path().to_str().unwrap())
                .unwrap();
        let source = format!("{INBOX_DIR}/note.md");

        let decision = parse_organize_response(
            temp.path().to_str().unwrap(),
            &source,
            r#"{"targetRelativePath":"999-New/note.md","confidence":0.9,"reason":"new category"}"#,
            Some(&archive_map),
        )
        .unwrap();

        assert_eq!(decision.status, "pending");
        assert_eq!(decision.error.as_deref(), Some("target_not_in_archive_map"));
    }
}
