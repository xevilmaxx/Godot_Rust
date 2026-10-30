//! godot_ai — SolidCraft-style AI integration.
//!
//! Two channels, same as SolidCraft (`docs/agent-chat.md`, `docs/mcp.md`):
//! 1. **ACP agent chat**: spawn an installed agent (`opencode acp`, `claude-agent-acp`,
//!    `codex-acp`, `grok agent stdio`, …) over stdio JSON-RPC; model dropdown comes
//!    from the agent's model list or the endpoint's `/models`.
//! 2. **MCP server**: expose engine commands (`list_commands`, `execute`, `inspect`,
//!    `set_parameter`, `export`) so *any* MCP client (Claude Code, etc.) can drive
//!    the editor headlessly — `godot_rs_editor mcp`.
//!
//! Open-source reference implementations fetched per user request:
//! - `github.com/tangency-labs/solidcraft` (MIT/Apache-2.0): `crates/mcp`, agent-chat panel
//! - `github.com/storytold/artcraft` (MIT/Apache-2.0): provider/model catalog pattern

pub mod acp;
pub mod control;
pub mod mcp;
pub mod models;

pub use acp::{AcpAgent, AgentKind};
pub use control::ControlChannel;
pub use mcp::{McpServer, ToolDef};
pub use models::{ModelDescriptor, ModelRegistry};
