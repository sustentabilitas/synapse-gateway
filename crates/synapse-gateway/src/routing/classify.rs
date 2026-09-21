//! Pure lane detection: standard vs native-Vertex.

use crate::routing::request::ChatRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Standard,
    NativeVertex,
    /// TypeSafe System One (Jev): the request carries a `jev` extension block
    /// with typed questions to evaluate against the request state.
    Jev,
}

/// Native-Vertex triggers on the `vertex` extension block. Also consulted by
/// the Jev lane, where these triggers describe the native-Vertex fallback
/// (the `jev` block itself takes lane precedence).
pub(crate) fn vertex_triggers(req: &ChatRequest) -> bool {
    let v = match &req.vertex {
        Some(v) => v,
        None => return false,
    };
    v.cached_content.is_some()
        || v.response_schema.is_some()
        || v.thinking_config.is_some()
        || v.media_uris
            .as_ref()
            .is_some_and(|uris| uris.iter().any(|u| u.starts_with("gs://")))
}

/// Classify by inspecting the request for lane triggers.
/// Pure and allocation-free; safe to call on the hot path.
pub fn classify(req: &ChatRequest) -> Lane {
    if req.jev.as_ref().is_some_and(|j| !j.questions.is_empty()) {
        return Lane::Jev;
    }
    if vertex_triggers(req) {
        Lane::NativeVertex
    } else {
        Lane::Standard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::request::{ChatRequest, VertexExt};

    fn base() -> ChatRequest {
        serde_json::from_value(serde_json::json!({
            "model": "m", "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap()
    }

    #[test]
    fn no_vertex_block_is_standard() {
        assert_eq!(classify(&base()), Lane::Standard);
    }

    #[test]
    fn cached_content_is_native() {
        let req = ChatRequest {
            vertex: Some(VertexExt {
                cached_content: Some("cachedContents/x".into()),
                ..Default::default()
            }),
            ..base()
        };
        assert_eq!(classify(&req), Lane::NativeVertex);
    }

    #[test]
    fn response_schema_is_native() {
        let req = ChatRequest {
            vertex: Some(VertexExt {
                response_schema: Some(serde_json::json!({"type": "object"})),
                ..Default::default()
            }),
            ..base()
        };
        assert_eq!(classify(&req), Lane::NativeVertex);
    }

    #[test]
    fn gs_media_uri_is_native_but_https_is_not() {
        let gs = ChatRequest {
            vertex: Some(VertexExt {
                media_uris: Some(vec!["gs://b/v.mp4".into()]),
                ..Default::default()
            }),
            ..base()
        };
        assert_eq!(classify(&gs), Lane::NativeVertex);
        let https = ChatRequest {
            vertex: Some(VertexExt {
                media_uris: Some(vec!["https://x/v.mp4".into()]),
                ..Default::default()
            }),
            ..base()
        };
        assert_eq!(classify(&https), Lane::Standard);
    }

    #[test]
    fn jev_questions_take_lane_precedence() {
        let req = ChatRequest {
            jev: Some(crate::routing::request::JevExt {
                questions: serde_json::json!({"q": {"type": "noul"}})
                    .as_object()
                    .unwrap()
                    .clone(),
                ..Default::default()
            }),
            vertex: Some(crate::routing::request::VertexExt {
                response_schema: Some(serde_json::json!({"type": "object"})),
                ..Default::default()
            }),
            ..base()
        };
        // The vertex block stays in force for the fallback; the lane is Jev.
        assert_eq!(classify(&req), Lane::Jev);
        assert!(super::vertex_triggers(&req));
    }

    #[test]
    fn empty_jev_questions_do_not_claim_the_lane() {
        let req = ChatRequest {
            jev: Some(crate::routing::request::JevExt::default()),
            vertex: Some(crate::routing::request::VertexExt {
                response_schema: Some(serde_json::json!({"type": "object"})),
                ..Default::default()
            }),
            ..base()
        };
        assert_eq!(classify(&req), Lane::NativeVertex);
    }

    #[test]
    fn thinking_config_is_native() {
        let req = ChatRequest {
            vertex: Some(VertexExt {
                thinking_config: Some(serde_json::json!({ "thinkingBudget": 0 })),
                ..Default::default()
            }),
            ..base()
        };
        assert_eq!(classify(&req), Lane::NativeVertex);
    }
}
