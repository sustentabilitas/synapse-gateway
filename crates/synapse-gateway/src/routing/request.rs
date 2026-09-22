//! OpenAI-compatible chat request body + native extension block.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub response_format: Option<ResponseFormat>,
    #[serde(default)]
    pub routing_strategy: Option<String>,
    #[serde(default)]
    pub vertex: Option<VertexExt>,
    /// TypeSafe System One (Jev) extension block. Present (with non-empty
    /// `questions`) routes the request to the route's `typesafe` legs; on
    /// retryable Jev failure the remaining legs answer as a normal chat
    /// completion.
    #[serde(default)]
    pub jev: Option<JevExt>,
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(flatten, default)]
    pub passthrough: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub content: Value, // string, array of parts, or null on a tool-call turn
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>, // assistant turn: [{id,type,function:{name,arguments}}]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>, // role:"tool" result correlation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>, // optional name (tool result or system message)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub kind: String, // "text" | "json_object" | "json_schema"
    #[serde(default)]
    pub json_schema: Option<Value>,
}

/// TypeSafe System One (Jev) extension block on a chat request: typed
/// questions evaluated against a state. The questions map is forwarded to Jev
/// verbatim (name → `{type, instructions, criteria?}`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JevExt {
    /// name → question definition. Non-empty routes the request to the Jev lane.
    pub questions: Map<String, Value>,
    /// Optional state override. Default: the request's `messages` serialized
    /// to JSON. State and questions share Jev's ~32k-token budget.
    #[serde(default)]
    pub state: Option<String>,
    /// Optional hybrid extraction spec: judge `candidates` with `noul`
    /// questions, then extract per survivor on the route's chat legs.
    #[serde(default)]
    pub extract: Option<ExtractSpec>,
}

/// One extraction candidate: the `text` gated by a `noul` question. When the
/// question's answer meets the extract floor, `text` is substituted into the
/// extraction prompt under `key`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractCandidate {
    pub key: String,
    /// Name of a `noul` question in `jev.questions` that gates this candidate.
    pub question: String,
    /// Candidate content fed to the extraction prompt as `{{text}}`.
    pub text: String,
}

/// Hybrid extraction spec (optional part of the `jev` extension block):
/// judge candidates with Jev, then run a schema-pinned chat extraction per
/// survivor. See `docs/superpowers/specs/2026-09-22-jev-hybrid-extraction-design.md`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractSpec {
    /// Minimum `noul` value (0..1] for a candidate to be extracted.
    pub floor: f64,
    pub candidates: Vec<ExtractCandidate>,
    /// Extraction system-message template; must contain `{{text}}`.
    pub prompt: String,
    /// Output schema for the extraction call (response_format on standard
    /// legs, vertex.response_schema on the native Vertex lane).
    pub response_schema: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VertexExt {
    #[serde(default)]
    pub cached_content: Option<String>,
    #[serde(default)]
    pub media_uris: Option<Vec<String>>,
    #[serde(default)]
    pub response_schema: Option<Value>,
    /// Raw Vertex `generationConfig.thinkingConfig` passthrough (e.g.
    /// `{"thinkingLevel":"low"}` for Gemini 3, or `{"thinkingBudget":N}` for
    /// Gemini 2.5). Threaded verbatim so the gateway stays model-agnostic.
    #[serde(default)]
    pub thinking_config: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_openai_body() {
        let body = serde_json::json!({
            "model": "gemini-pro",
            "messages": [{"role": "user", "content": "hi"}],
            "temperature": 0.2
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        assert_eq!(req.model, "gemini-pro");
        assert_eq!(req.messages.len(), 1);
        assert!(req.vertex.is_none());
        assert!(req.passthrough.is_empty());
    }

    #[test]
    fn captures_vertex_extension_and_passthrough() {
        let body = serde_json::json!({
            "model": "gemini-pro",
            "messages": [{"role": "user", "content": "hi"}],
            "top_k": 40,
            "vertex": { "cached_content": "cachedContents/abc" }
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        assert_eq!(
            req.vertex.unwrap().cached_content.as_deref(),
            Some("cachedContents/abc")
        );
        assert_eq!(req.passthrough.get("top_k"), Some(&serde_json::json!(40)));
    }

    #[test]
    fn captures_jev_extension() {
        let body = serde_json::json!({
            "model": "ticket-triage",
            "messages": [{"role": "user", "content": "hi"}],
            "jev": {
                "questions": {
                    "urgency": {"type": "noul", "instructions": "Is this urgent?"}
                },
                "state": "override"
            }
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        let jev = req.jev.unwrap();
        assert!(jev.questions.contains_key("urgency"));
        assert_eq!(jev.state.as_deref(), Some("override"));
    }

    #[test]
    fn captures_vertex_thinking_config() {
        let body = serde_json::json!({
            "model": "gemini-3-pro",
            "messages": [{"role": "user", "content": "hi"}],
            "vertex": { "thinking_config": { "thinkingLevel": "low" } }
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        assert_eq!(
            req.vertex.unwrap().thinking_config,
            Some(serde_json::json!({ "thinkingLevel": "low" }))
        );
    }

    #[test]
    fn parses_tools_and_tool_messages() {
        let body = serde_json::json!({
            "model": "gemini-pro",
            "messages": [
                {"role": "user", "content": "weather in SF?"},
                {"role": "assistant", "content": null,
                 "tool_calls": [{"id": "call_0", "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}}]},
                {"role": "tool", "tool_call_id": "call_0", "content": "21C"}
            ],
            "tools": [{"type": "function", "function": {"name": "get_weather",
                "description": "Lookup", "parameters": {"type": "object"}}}],
            "tool_choice": "auto"
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        assert_eq!(req.tools.as_ref().unwrap().len(), 1);
        assert_eq!(req.tool_choice, Some(serde_json::json!("auto")));
        let asst = &req.messages[1];
        assert!(asst.tool_calls.is_some());
        let tool = &req.messages[2];
        assert_eq!(tool.tool_call_id.as_deref(), Some("call_0"));
    }

    #[test]
    fn captures_jev_extract_spec() {
        let body = serde_json::json!({
            "model": "verify-orgs",
            "messages": [{"role": "user", "content": "hi"}],
            "jev": {
                "questions": {
                    "c0_match": {"type": "noul", "instructions": "Candidate 0 is the org's own site."}
                },
                "extract": {
                    "floor": 0.7,
                    "candidates": [
                        {"key": "c0", "question": "c0_match", "text": "excerpt 0"}
                    ],
                    "prompt": "Extract.\n\nCandidate {{key}}:\n{{text}}",
                    "response_schema": {"type": "object"}
                }
            }
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        let extract = req.jev.unwrap().extract.unwrap();
        assert_eq!(extract.floor, 0.7);
        assert_eq!(extract.candidates.len(), 1);
        assert_eq!(extract.candidates[0].key, "c0");
        assert_eq!(extract.candidates[0].question, "c0_match");
        assert_eq!(extract.candidates[0].text, "excerpt 0");
        assert!(extract.prompt.contains("{{text}}"));
        assert_eq!(
            extract.response_schema,
            serde_json::json!({"type": "object"})
        );
    }

    #[test]
    fn jev_block_without_extract_parses() {
        let body = serde_json::json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
            "jev": {"questions": {"q": {"type": "noul", "instructions": "x"}}}
        });
        let req: ChatRequest = serde_json::from_value(body).unwrap();
        assert!(req.jev.unwrap().extract.is_none());
    }
}
