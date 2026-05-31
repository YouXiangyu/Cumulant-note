use super::ai::{AiProvider, MockAiDecision, MockAiProvider};

pub struct OrganizerService<P: AiProvider = MockAiProvider> {
    provider: P,
}

impl Default for OrganizerService<MockAiProvider> {
    fn default() -> Self {
        Self {
            provider: MockAiProvider,
        }
    }
}

impl<P: AiProvider> OrganizerService<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn preview_only(&self, relative_path: &str) -> MockAiDecision {
        self.provider.organize_preview(relative_path)
    }
}
