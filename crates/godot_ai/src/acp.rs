//! ACP (Agent Client Protocol) agent descriptors.
//! Mirrors SolidCraft's agent table: install command + spawn args + model support.
//! Full JSON-RPC session (initialize/auth/prompt/cancel) is module 02; here we
//! provide discovery, `Refresh`/`Test` (spawn + handshake + shutdown), and config gen.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpAgent {
    pub id: String,
    pub label: String,
    pub install_cmd: String,
    pub spawn: Vec<String>,
    pub supports_models: bool,
    pub supports_modes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Opencode,
    Grok,
    Copilot,
    Custom,
}

impl AcpAgent {
    pub fn builtin(kind: AgentKind) -> Self {
        match kind {
            AgentKind::ClaudeCode => Self {
                id: "claude-code".into(),
                label: "Claude Code".into(),
                install_cmd: "npm install -g @agentclientprotocol/claude-agent-acp".into(),
                spawn: vec!["claude-agent-acp".into()],
                supports_models: true,
                supports_modes: true,
            },
            AgentKind::Codex => Self {
                id: "codex".into(),
                label: "Codex".into(),
                install_cmd: "npm install -g @agentclientprotocol/codex-acp".into(),
                spawn: vec!["codex-acp".into()],
                supports_models: true,
                supports_modes: false,
            },
            AgentKind::Opencode => Self {
                id: "opencode".into(),
                label: "OpenCode".into(),
                install_cmd: "npm install -g opencode-ai".into(),
                spawn: vec!["opencode".into(), "acp".into()],
                supports_models: true,
                supports_modes: false,
            },
            AgentKind::Grok => Self {
                id: "grok".into(),
                label: "XAI Grok".into(),
                install_cmd: "see https://docs.x.ai/build/overview, then `grok login`".into(),
                spawn: vec!["grok".into(), "agent".into(), "stdio".into()],
                supports_models: true,
                supports_modes: false,
            },
            AgentKind::Copilot => Self {
                id: "copilot".into(),
                label: "GitHub Copilot CLI".into(),
                install_cmd: "npm install -g @github/copilot".into(),
                spawn: vec!["copilot".into(), "--acp".into()],
                supports_models: true,
                supports_modes: false,
            },
            AgentKind::Custom => Self {
                id: "custom".into(),
                label: "Custom command".into(),
                install_cmd: "<your program speaking ACP on stdin/stdout>".into(),
                spawn: vec![],
                supports_models: true,
                supports_modes: false,
            },
        }
    }

    pub fn all_builtin() -> Vec<Self> {
        [
            AgentKind::ClaudeCode,
            AgentKind::Codex,
            AgentKind::Opencode,
            AgentKind::Grok,
            AgentKind::Copilot,
            AgentKind::Custom,
        ]
        .map(Self::builtin)
        .to_vec()
    }

    /// Is the agent binary on PATH? (SolidCraft AI → Refresh equivalent.)
    pub fn is_installed(&self) -> bool {
        if self.spawn.is_empty() {
            return false;
        }
        which(&self.spawn[0]).is_some()
    }
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| {
                let p = dir.join(bin);
                #[cfg(windows)]
                let p = if p.extension().is_none() { p.with_extension("exe") } else { p };
                p
            })
            .find(|p| p.is_file())
    })
}
