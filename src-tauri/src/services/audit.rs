use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub is_mock: bool,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

pub struct AuditService;

impl AuditService {
    pub fn mock_event(event_type: &str, payload: Value) -> AuditEvent {
        AuditEvent {
            is_mock: true,
            event_type: event_type.to_string(),
            payload,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}
