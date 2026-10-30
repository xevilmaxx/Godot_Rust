//! Control channel: desktop app listens on 127.0.0.1:<port> with a per-launch
//! token (SolidCraft pattern), so headless MCP sessions can bridge to the live editor.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlChannel {
    pub host: String,
    pub port: u16,
    pub token: String,
}

impl ControlChannel {
    pub fn local(port: u16) -> Self {
        Self {
            host: "127.0.0.1".into(),
            port,
            // v0: fixed dev token; module 03 generates fresh token per launch,
            // readable only by current user (0700 file), like SolidCraft.
            token: "dev-token-change-me".into(),
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
