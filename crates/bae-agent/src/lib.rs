//! Agent access to BAE (ARCHITECTURE §10): an MCP server, agent sessions, proposal
//! sublayers, permissions and an audit log.
//!
//! - [`Workspace`]: the open project plus the [`CommandRegistry`] agents can call.
//! - [`AgentIdentity`] / [`Permission`]: who an agent is and what it may write. By
//!   default an agent writes into its own proposal sublayer `agent/<name>`.
//! - [`AgentSession`]: runs commands for one agent, enforcing its permission inside
//!   the transaction (a command that would write another sublayer is rolled back).
//! - [`AuditLog`]: every tool call, with its outcome and the sublayers it wrote.
//! - [`proposal`]: `proposal.accept` / `proposal.reject` commands and proposal
//!   summaries for the Agent panel (workstream 11).
//! - [`render`]: the [`FrameRenderer`] seam for `render.frame` and
//!   `render.contact_sheet` (a placeholder until the compositor lands).
//! - [`BaeMcpServer`]: the MCP server. Tools are generated from the command
//!   registry (tool name = command ID with `.` replaced by `_`), plus query, render,
//!   batch and history tools ([`tools`]). [`protocol`] is a small synchronous
//!   JSON-RPC layer (no async runtime, to keep the app small); rmcp's client is used
//!   in the tests to check interoperability with the official SDK.
//!
//! This crate has no GPU, window or platform dependencies; the app injects a real
//! renderer and wires `bae mcp` (see `apps/bae`).

pub mod audit;
pub mod describe;
mod identity;
pub mod proposal;
pub mod protocol;
pub mod render;
mod server;
mod session;
pub mod timeargs;
pub mod tools;
mod workspace;

pub use audit::{AuditEntry, AuditLog, AuditOutcome};
pub use identity::{AgentIdentity, InvalidAgentName, Permission};
pub use render::{FrameRenderer, FrameRequest, PlaceholderRenderer, RenderError, RgbaImage};
pub use server::{
    BaeMcpServer, Content, GUIDE_INSTRUCTIONS_EN, GUIDE_INSTRUCTIONS_ZH, InstructionsLang,
    McpOptions, SaveErrorText, ServeError, ToolResult, instructions, run_stdio,
};
pub use session::{AgentError, AgentSession, ExecOutcome, Step};
pub use workspace::{SharedWorkspace, Workspace};

/// Register this crate's commands (`proposal.accept`, `proposal.reject`).
pub fn register_commands(registry: &mut bae_model::CommandRegistry) {
    proposal::register(registry);
}

#[doc(no_inline)]
pub use bae_model::CommandRegistry;
