//! OpenAI-compatible model registry (SolidCraft "OpenAI-compatible API" preset pattern).
//! Presets: Ollama, LM Studio, OpenAI, OpenCode Go (`/models` → model dropdown).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub id: String,
    pub label: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointPreset {
    pub id: String,
    pub label: String,
    pub base_url: String,
}

impl EndpointPreset {
    pub fn builtin() -> Vec<Self> {
        vec![
            Self { id: "ollama".into(), label: "Ollama".into(), base_url: "http://localhost:11434/v1".into() },
            Self { id: "lmstudio".into(), label: "LM Studio".into(), base_url: "http://localhost:1234/v1".into() },
            Self { id: "opencode-go".into(), label: "OpenCode Go".into(), base_url: "https://opencode.ai/zen/go/v1".into() },
            Self { id: "openai".into(), label: "OpenAI".into(), base_url: "https://api.openai.com/v1".into() },
        ]
    }
}

#[derive(Debug, Default)]
pub struct ModelRegistry {
    pub models: Vec<ModelDescriptor>,
    pub last_choice: Option<String>,
}

impl ModelRegistry {
    pub fn with_builtin_offline() -> Self {
        Self {
            models: vec![
                ModelDescriptor { id: "iris-default".into(), label: "Iris local (no network)".into(), endpoint: "local".into() },
            ],
            last_choice: Some("iris-default".into()),
        }
    }

    pub fn choose(&mut self, id: &str) -> bool {
        if self.models.iter().any(|m| m.id == id) {
            self.last_choice = Some(id.to_owned());
            true
        } else {
            false
        }
    }
}
