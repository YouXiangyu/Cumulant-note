use crate::services::index::open_index_for_vault;
use crate::services::ServiceResult;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub is_mock: bool,
    pub provider: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cost_cents: i64,
    pub storage: String,
}

pub struct UsageService;

impl UsageService {
    pub fn summary(vault_path: &str) -> ServiceResult<UsageSummary> {
        let connection = open_index_for_vault(vault_path)?;
        let (prompt_tokens, completion_tokens, total_tokens, cost_cents): (i64, i64, i64, i64) =
            connection.query_row(
                "SELECT
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_cents), 0)
                 FROM ai_usage",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

        Ok(UsageSummary {
            is_mock: true,
            provider: "mock".to_string(),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cost_cents,
            storage: ".thebrain/index.sqlite".to_string(),
        })
    }
}
