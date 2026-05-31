use crate::services::budget::{BudgetLedgerInput, BudgetService};
use crate::services::vault::{
    canonical_vault_root, normalize_relative_path, resolve_existing_file,
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

impl MimoProvider {
    pub fn extract_file(
        vault_path: &str,
        input: MimoExtractInput,
    ) -> ServiceResult<MimoExtractResult> {
        let relative_path = normalize_relative_path(&input.relative_path)?;
        let path = resolve_existing_file(vault_path, &relative_path)?;
        if input.force_mock {
            return Ok(mock_extract(&relative_path, "forced mock"));
        }
        let Some(key) = lookup_mimo_key(vault_path)? else {
            return Ok(mock_extract(
                &relative_path,
                "missing MIMO_API_KEY or .secrets/mimo_api_key.txt",
            ));
        };
        match call_mimo_file(&key, &path, MIMO_EXTRACT_MODEL, extract_prompt()) {
            Ok(text) => {
                let _ = BudgetService::record_usage(
                    vault_path,
                    BudgetLedgerInput {
                        scope: None,
                        provider: "mimo".to_string(),
                        model: MIMO_EXTRACT_MODEL.to_string(),
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        total_tokens: 0,
                        cost_cents: 0,
                        reason: Some(format!("extract:{relative_path}")),
                    },
                );
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
            Err(error) => Ok(mock_extract(&relative_path, &format!("api_error: {error}"))),
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
            return Ok(mock_decision(&source, "forced mock"));
        }
        let Some(key) = lookup_mimo_key(vault_path)? else {
            return Ok(mock_decision(
                &source,
                "missing MIMO_API_KEY or .secrets/mimo_api_key.txt",
            ));
        };
        let prompt = organize_prompt(&source, extracted_text.unwrap_or(""));
        match call_mimo_text(&key, MIMO_ORGANIZE_MODEL, &prompt) {
            Ok(text) => {
                let mut decision = mock_decision(&source, "mimo response parsed conservatively");
                decision.provider = "mimo".to_string();
                decision.model = MIMO_ORGANIZE_MODEL.to_string();
                decision.status = "ok".to_string();
                decision.is_mock = false;
                decision.summary = text.chars().take(500).collect();
                Ok(decision)
            }
            Err(error) => Ok(mock_decision(&source, &format!("api_error: {error}"))),
        }
    }
}

fn lookup_mimo_key(vault_path: &str) -> ServiceResult<Option<String>> {
    if let Ok(value) = env::var("MIMO_API_KEY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    let root = canonical_vault_root(vault_path)?;
    let local_key = root.join(".secrets").join("mimo_api_key.txt");
    if local_key.exists() {
        let value = fs::read_to_string(local_key)?;
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(None)
}

fn call_mimo_file(key: &str, path: &Path, model: &str, prompt: &str) -> ServiceResult<String> {
    let data = fs::read(path)?;
    let mime = mime_for_path(path);
    let data_url = format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(data)
    );
    let media_type = if mime.starts_with("image/") {
        "image_url"
    } else {
        "input_audio"
    };
    let media_payload = if media_type == "image_url" {
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
    Ok(body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

fn mock_extract(relative_path: &str, reason: &str) -> MimoExtractResult {
    MimoExtractResult {
        provider: "mock".to_string(),
        model: MIMO_EXTRACT_MODEL.to_string(),
        status: "fallback".to_string(),
        is_mock: true,
        relative_path: relative_path.to_string(),
        text: format!("Mock extracted text for {relative_path}."),
        error: Some(reason.to_string()),
    }
}

fn mock_decision(source: &str, reason: &str) -> OrganizeDecision {
    let file_name = Path::new(source)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "整理结果.md".to_string());
    OrganizeDecision {
        provider: "mock".to_string(),
        model: MIMO_ORGANIZE_MODEL.to_string(),
        status: "fallback".to_string(),
        is_mock: true,
        source_relative_path: source.to_string(),
        target_relative_path: format!("100-Inbox-整理/{file_name}"),
        tags: vec!["inbox".to_string()],
        summary: "Mock organization decision.".to_string(),
        reason: reason.to_string(),
        confidence: 0.35,
        todo_candidates: Vec::new(),
        schedule_candidates: Vec::new(),
        error: Some(reason.to_string()),
    }
}

fn extract_prompt() -> &'static str {
    "请从该文件中抽取可整理为 Markdown 的文本内容。不要输出推理过程。"
}

fn organize_prompt(source: &str, text: &str) -> String {
    format!(
        "使用“文档管理 + 知识标签”范式整理 {source}。返回目标路径、标签、摘要、理由、TODO候选和日程候选。内容：\n{text}"
    )
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        _ => "audio/mpeg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::{VaultService, INBOX_DIR};

    #[test]
    fn mimo_extract_has_mock_fallback_without_network() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        std::fs::write(temp.path().join(INBOX_DIR).join("audio.mp3"), b"not real audio").unwrap();
        let result = MimoProvider::extract_file(
            temp.path().to_str().unwrap(),
            MimoExtractInput {
                relative_path: "000-收集箱/audio.mp3".to_string(),
                force_mock: true,
            },
        )
        .unwrap();
        assert!(result.is_mock);
        assert_eq!(result.model, MIMO_EXTRACT_MODEL);
        assert!(result.text.contains("audio.mp3"));
    }
}
