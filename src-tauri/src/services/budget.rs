use crate::services::index::open_index_for_vault;
use crate::services::{ServiceError, ServiceResult};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSettings {
    pub scope: String,
    pub monthly_limit_cents: Option<i64>,
    pub daily_limit_cents: Option<i64>,
    pub paused: bool,
    pub retry_limit: i64,
    pub cooldown_seconds: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSettingsInput {
    pub scope: Option<String>,
    pub monthly_limit_cents: Option<i64>,
    pub daily_limit_cents: Option<i64>,
    pub paused: bool,
    pub retry_limit: i64,
    pub cooldown_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLedgerInput {
    pub scope: Option<String>,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cost_cents: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatus {
    pub settings: BudgetSettings,
    pub spent_today_cents: i64,
    pub spent_month_cents: i64,
    pub total_tokens_today: i64,
    pub budget_exhausted: bool,
    pub can_run: bool,
}

pub struct BudgetService;

impl BudgetService {
    pub fn get_settings(vault_path: &str) -> ServiceResult<BudgetSettings> {
        let connection = open_index_for_vault(vault_path)?;
        let settings = connection
            .query_row(
                "SELECT scope, monthly_limit_cents, daily_limit_cents, paused, retry_limit, cooldown_seconds, updated_at
                 FROM budget_settings WHERE scope = 'global'",
                [],
                row_to_settings,
            )
            .optional()?;
        Ok(settings.unwrap_or_else(default_settings))
    }

    pub fn save_settings(
        vault_path: &str,
        input: BudgetSettingsInput,
    ) -> ServiceResult<BudgetSettings> {
        validate_optional_nonnegative("monthly_limit_cents", input.monthly_limit_cents)?;
        validate_optional_nonnegative("daily_limit_cents", input.daily_limit_cents)?;
        if input.retry_limit < 0 || input.cooldown_seconds < 0 {
            return Err(ServiceError::InvalidState(
                "budget limits must not be negative".to_string(),
            ));
        }
        let scope = normalize_scope(input.scope)?;
        let connection = open_index_for_vault(vault_path)?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO budget_settings
                (scope, monthly_limit_cents, daily_limit_cents, paused, retry_limit, cooldown_seconds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(scope) DO UPDATE SET
                monthly_limit_cents = excluded.monthly_limit_cents,
                daily_limit_cents = excluded.daily_limit_cents,
                paused = excluded.paused,
                retry_limit = excluded.retry_limit,
                cooldown_seconds = excluded.cooldown_seconds,
                updated_at = excluded.updated_at",
            params![
                scope,
                input.monthly_limit_cents,
                input.daily_limit_cents,
                if input.paused { 1 } else { 0 },
                input.retry_limit,
                input.cooldown_seconds,
                now
            ],
        )?;
        Self::get_settings(vault_path)
    }

    pub fn record_usage(vault_path: &str, input: BudgetLedgerInput) -> ServiceResult<BudgetStatus> {
        validate_nonnegative("prompt_tokens", input.prompt_tokens)?;
        validate_nonnegative("completion_tokens", input.completion_tokens)?;
        validate_nonnegative("total_tokens", input.total_tokens)?;
        validate_nonnegative("cost_cents", input.cost_cents)?;
        let scope = normalize_scope(input.scope)?;
        let connection = open_index_for_vault(vault_path)?;
        connection.execute(
            "INSERT INTO budget_ledger
                (scope, provider, model, prompt_tokens, completion_tokens, total_tokens, cost_cents, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                scope,
                input.provider,
                input.model,
                input.prompt_tokens,
                input.completion_tokens,
                input.total_tokens,
                input.cost_cents,
                input.reason,
                Utc::now().to_rfc3339()
            ],
        )?;
        Self::status(vault_path)
    }

    pub fn status(vault_path: &str) -> ServiceResult<BudgetStatus> {
        let settings = Self::get_settings(vault_path)?;
        let connection = open_index_for_vault(vault_path)?;
        let today_prefix = Utc::now().format("%Y-%m-%d").to_string();
        let month_prefix = Utc::now().format("%Y-%m").to_string();
        let (spent_today_cents, total_tokens_today): (i64, i64) = connection.query_row(
            "SELECT COALESCE(SUM(cost_cents), 0), COALESCE(SUM(total_tokens), 0)
             FROM budget_ledger WHERE scope = 'global' AND created_at LIKE ?1",
            params![format!("{today_prefix}%")],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let spent_month_cents: i64 = connection.query_row(
            "SELECT COALESCE(SUM(cost_cents), 0)
             FROM budget_ledger WHERE scope = 'global' AND created_at LIKE ?1",
            params![format!("{month_prefix}%")],
            |row| row.get(0),
        )?;
        let exhausted = settings
            .daily_limit_cents
            .is_some_and(|limit| spent_today_cents >= limit)
            || settings
                .monthly_limit_cents
                .is_some_and(|limit| spent_month_cents >= limit);
        let can_run = !exhausted && !settings.paused;
        Ok(BudgetStatus {
            settings,
            spent_today_cents,
            spent_month_cents,
            total_tokens_today,
            budget_exhausted: exhausted,
            can_run,
        })
    }
}

fn normalize_scope(scope: Option<String>) -> ServiceResult<String> {
    let scope = scope.unwrap_or_else(|| "global".to_string());
    let scope = scope.trim();
    if scope.is_empty() {
        Err(ServiceError::InvalidState(
            "budget scope must not be empty".to_string(),
        ))
    } else {
        Ok(scope.to_string())
    }
}

fn validate_optional_nonnegative(label: &str, value: Option<i64>) -> ServiceResult<()> {
    if let Some(value) = value {
        validate_nonnegative(label, value)?;
    }
    Ok(())
}

fn validate_nonnegative(label: &str, value: i64) -> ServiceResult<()> {
    if value < 0 {
        Err(ServiceError::InvalidState(format!(
            "{label} must not be negative"
        )))
    } else {
        Ok(())
    }
}

fn default_settings() -> BudgetSettings {
    BudgetSettings {
        scope: "global".to_string(),
        monthly_limit_cents: None,
        daily_limit_cents: None,
        paused: false,
        retry_limit: 3,
        cooldown_seconds: 0,
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn row_to_settings(row: &Row<'_>) -> rusqlite::Result<BudgetSettings> {
    Ok(BudgetSettings {
        scope: row.get(0)?,
        monthly_limit_cents: row.get(1)?,
        daily_limit_cents: row.get(2)?,
        paused: row.get::<_, i64>(3)? != 0,
        retry_limit: row.get(4)?,
        cooldown_seconds: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;

    #[test]
    fn budget_limits_can_exhaust_queue_runs() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        BudgetService::save_settings(
            temp.path().to_str().unwrap(),
            BudgetSettingsInput {
                scope: None,
                monthly_limit_cents: Some(10),
                daily_limit_cents: Some(5),
                paused: false,
                retry_limit: 2,
                cooldown_seconds: 60,
            },
        )
        .unwrap();
        let status = BudgetService::record_usage(
            temp.path().to_str().unwrap(),
            BudgetLedgerInput {
                scope: None,
                provider: "mock".to_string(),
                model: "mock".to_string(),
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                cost_cents: 5,
                reason: Some("test".to_string()),
            },
        )
        .unwrap();
        assert!(status.budget_exhausted);
        assert_eq!(status.settings.retry_limit, 2);
    }

    #[test]
    fn paused_budget_blocks_runs_without_spend() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        BudgetService::save_settings(
            temp.path().to_str().unwrap(),
            BudgetSettingsInput {
                scope: None,
                monthly_limit_cents: None,
                daily_limit_cents: None,
                paused: true,
                retry_limit: 3,
                cooldown_seconds: 0,
            },
        )
        .unwrap();

        let status = BudgetService::status(temp.path().to_str().unwrap()).unwrap();
        assert!(!status.budget_exhausted);
        assert!(!status.can_run);
    }
}
