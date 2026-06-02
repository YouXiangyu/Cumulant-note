use crate::services::index::open_index_for_vault;
use crate::services::ServiceResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const SETTINGS_KEY: &str = "app_settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    pub organization_template: String,
    pub ai_decision_model: String,
    pub extraction_model: String,
    pub sticky_notes_path: String,
    pub auto_save_interval_seconds: i64,
    pub queue_concurrency: i64,
    pub retry_limit: i64,
    pub cooldown_minutes: i64,
    pub enable_global_shortcut: bool,
    pub global_shortcut: String,
    pub prewarm_windows: i64,
    pub active_window_limit: i64,
    pub budget_monthly_cents: i64,
    pub budget_hard_stop_cents: i64,
    pub conflict_default_action: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsInput {
    pub language: Option<String>,
    pub organization_template: Option<String>,
    pub ai_decision_model: Option<String>,
    pub extraction_model: Option<String>,
    pub sticky_notes_path: Option<String>,
    pub auto_save_interval_seconds: Option<i64>,
    pub queue_concurrency: Option<i64>,
    pub retry_limit: Option<i64>,
    pub cooldown_minutes: Option<i64>,
    pub enable_global_shortcut: Option<bool>,
    pub global_shortcut: Option<String>,
    pub prewarm_windows: Option<i64>,
    pub active_window_limit: Option<i64>,
    pub budget_monthly_cents: Option<i64>,
    pub budget_hard_stop_cents: Option<i64>,
    pub conflict_default_action: Option<String>,
}

pub struct SettingsService;

impl SettingsService {
    pub fn get(vault_path: &str) -> ServiceResult<AppSettings> {
        let connection = open_index_for_vault(vault_path)?;
        let raw = connection
            .query_row(
                "SELECT value FROM vault_meta WHERE key = ?1",
                params![SETTINGS_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(raw) = raw {
            Ok(serde_json::from_str(&raw).unwrap_or_else(|_| default_settings()))
        } else {
            Ok(default_settings())
        }
    }

    pub fn save(vault_path: &str, input: AppSettingsInput) -> ServiceResult<AppSettings> {
        let current = Self::get(vault_path).unwrap_or_else(|_| default_settings());
        let settings = AppSettings {
            language: input.language.unwrap_or(current.language),
            organization_template: input
                .organization_template
                .unwrap_or(current.organization_template),
            ai_decision_model: input.ai_decision_model.unwrap_or(current.ai_decision_model),
            extraction_model: input.extraction_model.unwrap_or(current.extraction_model),
            sticky_notes_path: input.sticky_notes_path.unwrap_or(current.sticky_notes_path),
            auto_save_interval_seconds: input
                .auto_save_interval_seconds
                .unwrap_or(current.auto_save_interval_seconds)
                .clamp(1, 3600),
            queue_concurrency: input
                .queue_concurrency
                .unwrap_or(current.queue_concurrency)
                .clamp(1, 1),
            retry_limit: input
                .retry_limit
                .unwrap_or(current.retry_limit)
                .clamp(0, 10),
            cooldown_minutes: input
                .cooldown_minutes
                .unwrap_or(current.cooldown_minutes)
                .clamp(0, 1440),
            enable_global_shortcut: input
                .enable_global_shortcut
                .unwrap_or(current.enable_global_shortcut),
            global_shortcut: input.global_shortcut.unwrap_or(current.global_shortcut),
            prewarm_windows: input
                .prewarm_windows
                .unwrap_or(current.prewarm_windows)
                .clamp(0, 4),
            active_window_limit: input
                .active_window_limit
                .unwrap_or(current.active_window_limit)
                .clamp(1, 16),
            budget_monthly_cents: input
                .budget_monthly_cents
                .unwrap_or(current.budget_monthly_cents)
                .max(0),
            budget_hard_stop_cents: input
                .budget_hard_stop_cents
                .unwrap_or(current.budget_hard_stop_cents)
                .max(0),
            conflict_default_action: input
                .conflict_default_action
                .unwrap_or(current.conflict_default_action),
            updated_at: Utc::now().to_rfc3339(),
        };
        let connection = open_index_for_vault(vault_path)?;
        connection.execute(
            "INSERT INTO vault_meta (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![SETTINGS_KEY, serde_json::to_string(&settings)?, settings.updated_at],
        )?;
        Ok(settings)
    }
}

fn default_settings() -> AppSettings {
    AppSettings {
        language: "zh-CN".to_string(),
        organization_template: "文档管理 + 知识标签".to_string(),
        ai_decision_model: "mimo-v2.5-pro".to_string(),
        extraction_model: "mimo-v2.5".to_string(),
        sticky_notes_path: "000-收集箱".to_string(),
        auto_save_interval_seconds: 20,
        queue_concurrency: 1,
        retry_limit: 3,
        cooldown_minutes: 10,
        enable_global_shortcut: true,
        global_shortcut: "Ctrl+Space".to_string(),
        prewarm_windows: 2,
        active_window_limit: 8,
        budget_monthly_cents: 2000,
        budget_hard_stop_cents: 2500,
        conflict_default_action: "rename".to_string(),
        updated_at: Utc::now().to_rfc3339(),
    }
}
