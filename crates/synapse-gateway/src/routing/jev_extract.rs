//! Hybrid extraction: validation and pure decision logic for the `jev.extract`
//! spec. Upstream orchestration lives in `jev_hybrid.rs`; types in `request.rs`.

use crate::routing::request::ChatRequest;

/// Validate a hybrid extraction request. `has_chat_legs` comes from the route
/// table (non-`typesafe` legs exist). Returns the human-readable 400 message.
pub fn validate_extract(req: &ChatRequest, has_chat_legs: bool) -> Result<(), String> {
    let spec = match req.jev.as_ref().and_then(|j| j.extract.as_ref()) {
        Some(spec) => spec,
        None => return Ok(()),
    };
    if req.stream == Some(true) {
        return Err(
            "jev extract does not support stream: true (hybrid extraction is unary)".into(),
        );
    }
    if !has_chat_legs {
        return Err(format!(
            "route '{}' has no chat legs for jev extraction",
            req.model
        ));
    }
    if !(spec.floor > 0.0 && spec.floor <= 1.0) {
        return Err(format!(
            "jev extract floor must be in (0, 1], got {}",
            spec.floor
        ));
    }
    if spec.prompt.is_empty() || !spec.prompt.contains("{{text}}") {
        return Err("jev extract prompt must be non-empty and contain {{text}}".into());
    }
    if !spec.response_schema.is_object() {
        return Err("jev extract response_schema must be a JSON object".into());
    }
    let questions = &req.jev.as_ref().expect("checked above").questions;
    for candidate in &spec.candidates {
        match questions.get(&candidate.question) {
            Some(q) if q["type"].as_str() == Some("noul") => {}
            Some(_) => {
                return Err(format!(
                    "jev extract candidate '{}' is gated by '{}', which is not a noul question",
                    candidate.key, candidate.question
                ))
            }
            None => {
                return Err(format!(
                    "jev extract candidate '{}' is gated by unknown question '{}'",
                    candidate.key, candidate.question
                ))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::request::{ChatRequest, ExtractCandidate, ExtractSpec};
    use serde_json::{Map, Value};

    fn hybrid_req(floor: f64, prompt: &str, schema: Value) -> ChatRequest {
        let mut questions = Map::new();
        questions.insert(
            "c0_match".into(),
            serde_json::json!({"type": "noul", "instructions": "x"}),
        );
        ChatRequest {
            model: "m".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            stream: None,
            response_format: None,
            routing_strategy: None,
            vertex: None,
            jev: Some(crate::routing::request::JevExt {
                questions,
                state: None,
                extract: Some(ExtractSpec {
                    floor,
                    candidates: vec![ExtractCandidate {
                        key: "c0".into(),
                        question: "c0_match".into(),
                        text: "t".into(),
                    }],
                    prompt: prompt.into(),
                    response_schema: schema,
                }),
            }),
            tools: None,
            tool_choice: None,
            passthrough: Map::new(),
        }
    }

    #[test]
    fn rejects_streaming() {
        let mut req = hybrid_req(0.7, "{{text}}", serde_json::json!({"type": "object"}));
        req.stream = Some(true);
        assert!(validate_extract(&req, true).unwrap_err().contains("stream"));
    }

    #[test]
    fn rejects_route_without_chat_legs() {
        let req = hybrid_req(0.7, "{{text}}", serde_json::json!({"type": "object"}));
        assert!(validate_extract(&req, false)
            .unwrap_err()
            .contains("no chat legs"));
    }

    #[test]
    fn rejects_bad_floor() {
        for floor in [0.0, 1.1, -0.5] {
            let req = hybrid_req(floor, "{{text}}", serde_json::json!({"type": "object"}));
            assert!(
                validate_extract(&req, true).unwrap_err().contains("floor"),
                "floor {floor}"
            );
        }
    }

    #[test]
    fn rejects_prompt_without_text_placeholder() {
        let req = hybrid_req(0.7, "no placeholder", serde_json::json!({"type": "object"}));
        assert!(validate_extract(&req, true)
            .unwrap_err()
            .contains("{{text}}"));
    }

    #[test]
    fn rejects_non_object_schema() {
        let req = hybrid_req(0.7, "{{text}}", serde_json::json!("not-an-object"));
        assert!(validate_extract(&req, true)
            .unwrap_err()
            .contains("response_schema"));
    }

    #[test]
    fn rejects_candidate_gated_by_non_noul_question() {
        let mut req = hybrid_req(0.7, "{{text}}", serde_json::json!({"type": "object"}));
        req.jev.as_mut().unwrap().questions.insert(
            "c1_choice".into(),
            serde_json::json!({"type": "choice", "instructions": "x"}),
        );
        req.jev
            .as_mut()
            .unwrap()
            .extract
            .as_mut()
            .unwrap()
            .candidates
            .push(ExtractCandidate {
                key: "c1".into(),
                question: "c1_choice".into(),
                text: "t".into(),
            });
        assert!(validate_extract(&req, true)
            .unwrap_err()
            .contains("not a noul question"));
    }

    #[test]
    fn rejects_candidate_gated_by_unknown_question() {
        let mut req = hybrid_req(0.7, "{{text}}", serde_json::json!({"type": "object"}));
        req.jev
            .as_mut()
            .unwrap()
            .extract
            .as_mut()
            .unwrap()
            .candidates[0]
            .question = "nope".into();
        assert!(validate_extract(&req, true)
            .unwrap_err()
            .contains("unknown question"));
    }

    #[test]
    fn accepts_a_valid_spec() {
        let req = hybrid_req(
            0.7,
            "Extract {{key}}: {{text}}",
            serde_json::json!({"type": "object"}),
        );
        assert!(validate_extract(&req, true).is_ok());
    }

    #[test]
    fn no_extract_spec_is_ok() {
        let mut req = hybrid_req(0.7, "{{text}}", serde_json::json!({"type": "object"}));
        req.jev.as_mut().unwrap().extract = None;
        assert!(validate_extract(&req, false).is_ok());
    }
}
