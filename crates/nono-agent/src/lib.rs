//! Agent gateway contracts.
//!
//! The pet only ever talks to a gateway; the gateway owns an adapter.  M1 ships
//! the protocol and a mock adapter so the wiring can be exercised without any
//! network or API key.  M2 adds Ollama / OpenAI-compatible / Claude adapters and
//! streams replies into the speech bubble.

use serde::{Deserialize, Serialize};

pub const AGENT_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent is not configured")]
    NotConfigured,
    #[error("agent request failed: {0}")]
    Transport(String),
    #[error("agent returned an invalid reply: {0}")]
    Protocol(String),
}

/// Request as documented in the design doc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRequest {
    pub role: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

impl AgentRequest {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: "user".into(), text: text.into(), context_id: None }
    }
}

/// Reply.  `emotion` and `suggest_anim` are hints the pet turns into
/// expressions/animations; adapters should keep them optional.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReply {
    pub reply_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest_anim: Option<String>,
    #[serde(default)]
    pub done: bool,
}

impl AgentReply {
    pub fn text(reply_text: impl Into<String>) -> Self {
        Self { reply_text: reply_text.into(), emotion: None, suggest_anim: None, done: true }
    }
}

/// One adapter per backend.  Implementations must be side-effect free from the
/// caller's perspective and never panic the UI thread; M2 will run them on a
/// worker thread and feed chunks through the event bus.
pub trait AgentAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn send(&self, request: &AgentRequest) -> Result<AgentReply, AgentError>;
}

/// Canned adapter used for wiring tests and for demoing the bubble without keys.
#[derive(Debug, Default)]
pub struct MockAdapter {
    pub reply: Option<String>,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AgentAdapter for MockAdapter {
    fn name(&self) -> &str {
        "mock"
    }

    fn send(&self, request: &AgentRequest) -> Result<AgentReply, AgentError> {
        if request.text.trim().is_empty() {
            return Err(AgentError::Protocol("empty prompt".into()));
        }
        let reply = self
            .reply
            .clone()
            .unwrap_or_else(|| format!("收到「{}」！我是超能 NONO，还在学习中。", request.text));
        Ok(AgentReply { reply_text: reply, emotion: Some("happy".into()), suggest_anim: None, done: true })
    }
}

/// Single point of contact for the rest of the application.
pub struct AgentGateway {
    adapter: Box<dyn AgentAdapter>,
}

impl std::fmt::Debug for AgentGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentGateway").field("adapter", &self.adapter.name()).finish()
    }
}

impl AgentGateway {
    pub fn new(adapter: impl AgentAdapter + 'static) -> Self {
        Self { adapter: Box::new(adapter) }
    }

    pub fn adapter_name(&self) -> &str {
        self.adapter.name()
    }

    pub fn ask(&self, text: impl Into<String>) -> Result<AgentReply, AgentError> {
        self.adapter.send(&AgentRequest::user(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_matches_the_design_document() {
        let request = serde_json::to_string(&AgentRequest::user("你好")).unwrap();
        assert_eq!(request, r#"{"role":"user","text":"你好"}"#);

        let reply: AgentReply = serde_json::from_str(
            r#"{"reply_text":"哈喽！","emotion":"happy","suggest_anim":"talk","done":true}"#,
        )
        .unwrap();
        assert_eq!(reply.emotion.as_deref(), Some("happy"));
        assert!(reply.done);
    }

    #[test]
    fn mock_adapter_answers() {
        let gateway = AgentGateway::new(MockAdapter::new());
        let reply = gateway.ask("你好").unwrap();
        assert!(reply.reply_text.contains("你好"));
        assert_eq!(gateway.adapter_name(), "mock");
    }
}
