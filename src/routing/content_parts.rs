//! OpenAI-style multimodal `message.content` → genai / Vertex parts.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use genai::chat::{Binary, ContentPart};
use serde_json::{json, Value};

/// Map a gateway message `content` value to genai `ContentPart`s.
pub fn content_to_genai_parts(content: &Value) -> Vec<ContentPart> {
    match content {
        Value::String(s) => vec![ContentPart::Text(s.clone())],
        Value::Array(parts) => parts.iter().filter_map(part_to_genai).collect(),
        other => vec![ContentPart::Text(other.to_string())],
    }
}

fn part_to_genai(part: &Value) -> Option<ContentPart> {
    let ty = part.get("type")?.as_str()?;
    match ty {
        "text" => Some(ContentPart::Text(part["text"].as_str()?.to_string())),
        "image_url" => {
            let url = part["image_url"]["url"].as_str()?;
            let (mime, data) = parse_data_url(url)?;
            Some(ContentPart::Binary(Binary::from_base64(mime, data, None)))
        }
        "inline_data" | "inlineData" => {
            let mime = part
                .get("inline_data")
                .or_else(|| part.get("inlineData"))
                .and_then(|o| o.get("mimeType").or_else(|| o.get("mime_type")))
                .and_then(|m| m.as_str())?;
            let data = part
                .get("inline_data")
                .or_else(|| part.get("inlineData"))
                .and_then(|o| o.get("data"))
                .and_then(|d| d.as_str())?;
            Some(ContentPart::Binary(Binary::from_base64(
                mime,
                data.to_string(),
                None,
            )))
        }
        _ => None,
    }
}

/// Map a gateway message `content` value to Vertex `parts` JSON objects.
pub fn content_to_vertex_parts(content: &Value) -> Vec<Value> {
    match content {
        Value::String(s) => vec![json!({ "text": s })],
        Value::Array(parts) => parts.iter().filter_map(part_to_vertex).collect(),
        other => vec![json!({ "text": other.to_string() })],
    }
}

fn part_to_vertex(part: &Value) -> Option<Value> {
    let ty = part.get("type")?.as_str()?;
    match ty {
        "text" => Some(json!({ "text": part["text"].as_str()? })),
        "image_url" => {
            let url = part["image_url"]["url"].as_str()?;
            let (mime, data) = parse_data_url(url)?;
            Some(json!({ "inlineData": { "mimeType": mime, "data": data } }))
        }
        "inline_data" | "inlineData" => {
            let obj = part.get("inline_data").or_else(|| part.get("inlineData"))?;
            let mime = obj
                .get("mimeType")
                .or_else(|| obj.get("mime_type"))
                .and_then(|m| m.as_str())?;
            let data = obj.get("data").and_then(|d| d.as_str())?;
            Some(json!({ "inlineData": { "mimeType": mime, "data": data } }))
        }
        _ => None,
    }
}

/// Parse `data:{mime};base64,{payload}` URLs used by OpenAI-compatible clients.
fn parse_data_url(url: &str) -> Option<(&str, String)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if !meta.ends_with(";base64") {
        return None;
    }
    let mime = meta.strip_suffix(";base64")?;
    Some((mime, data.to_string()))
}

/// Build an OpenAI-style multimodal content array from text + optional inline bytes.
pub fn multimodal_content(text: &str, mime: &str, bytes: &[u8]) -> Value {
    let data = B64.encode(bytes);
    json!([
        { "type": "text", "text": text },
        { "type": "inline_data", "inline_data": { "mimeType": mime, "data": data } }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_content_becomes_single_text_part() {
        let parts = content_to_genai_parts(&json!("hello"));
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], ContentPart::Text(t) if t == "hello"));
    }

    #[test]
    fn inline_pdf_round_trips_to_vertex() {
        let content = multimodal_content("extract", "application/pdf", b"%PDF");
        let vparts = content_to_vertex_parts(&content);
        assert_eq!(vparts.len(), 2);
        assert_eq!(vparts[1]["inlineData"]["mimeType"], "application/pdf");
    }
}
