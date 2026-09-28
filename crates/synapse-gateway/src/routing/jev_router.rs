//! Jev router: pure decision logic for `strategy = "jev"` routes. Jev rates
//! each request's difficulty against the route's tiers; this module turns the
//! answers into an ordered, effort-stamped leg plan and a client-facing report.
//! Spec: `docs/superpowers/specs/2026-09-28-jev-router-design.md`.

use serde_json::{json, Value};

use crate::routing::request::{ChatRequest, Message};
use crate::routing::table::{escalation_order, ChainLeg, Tier};

/// Character budget for the whole Jev `state` (≈ 6k tokens).
pub const STATE_BUDGET: usize = 24_000;
/// Cap on `latest_user_message`; longer messages keep their head and tail.
pub const LATEST_CAP: usize = 16_000;
/// Cap on `system_prompt`.
pub const SYSTEM_CAP: usize = 2_000;
/// Cap on each `recent_history` entry.
pub const HISTORY_ENTRY_CAP: usize = 2_000;

/// How a request's legs were planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingMode {
    /// Jev chose the tier.
    Jev,
    /// A plain `legs` route.
    #[default]
    Static,
    /// A `jev` route the client asked to serve without Jev.
    StaticOverride,
}

impl RoutingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jev => "jev",
            Self::Static => "static",
            Self::StaticOverride => "static-override",
        }
    }
}

/// The mode for one request: the route's strategy unless `routing_strategy`
/// overrides it. `Err` carries the 400 message.
pub fn resolve_mode(is_jev_route: bool, requested: Option<&str>) -> Result<RoutingMode, String> {
    match (is_jev_route, requested) {
        (true, None | Some("jev")) => Ok(RoutingMode::Jev),
        (true, Some("static")) => Ok(RoutingMode::StaticOverride),
        (false, None | Some("static")) => Ok(RoutingMode::Static),
        (false, Some("jev")) => Err("routing_strategy \"jev\" requires a route with tiers".into()),
        (_, Some(other)) => Err(format!(
            "unknown routing_strategy '{other}' (expected \"static\" or \"jev\")"
        )),
    }
}

/// The bounded `state` Jev rates: the latest user message, recent history,
/// system prompt, and tool/image flags. Never logged (customer data).
pub fn build_state(req: &ChatRequest) -> Value {
    let latest = req.messages.iter().rposition(|m| m.role == "user");
    let latest_text = latest
        .map(|i| head_tail(&message_text(&req.messages[i].content), LATEST_CAP))
        .unwrap_or_default();
    let system = req
        .messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| truncate(&message_text(&m.content), SYSTEM_CAP))
        .unwrap_or_default();
    let remaining =
        STATE_BUDGET.saturating_sub(latest_text.chars().count() + system.chars().count());
    json!({
        "latest_user_message": latest_text,
        "recent_history": recent_history(&req.messages, latest, remaining),
        "system_prompt": system,
        "has_tools": req.tools.as_ref().is_some_and(|t| !t.is_empty()),
        "has_images": has_images(req),
    })
}

/// Non-system messages other than the latest user message, filled newest-first
/// until `budget` characters, returned in chronological order.
fn recent_history(messages: &[Message], latest: Option<usize>, budget: usize) -> Vec<Value> {
    messages
        .iter()
        .enumerate()
        .rev()
        .filter(|(i, m)| m.role != "system" && Some(*i) != latest)
        .map(|(_, m)| {
            (
                m.role.as_str(),
                truncate(&message_text(&m.content), HISTORY_ENTRY_CAP),
            )
        })
        .scan(0usize, |used, (role, text)| {
            *used += text.chars().count();
            (*used <= budget).then_some((role, text))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect()
}

fn message_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(part_text)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn part_text(part: &Value) -> String {
    match part.get("type").and_then(Value::as_str) {
        Some("text") => part
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some("image_url" | "image") => "[image]".into(),
        Some("input_audio" | "audio") => "[audio]".into(),
        Some("file") => "[file]".into(),
        _ => String::new(),
    }
}

fn is_image_part(part: &Value) -> bool {
    matches!(
        part.get("type").and_then(Value::as_str),
        Some("image_url" | "image")
    )
}

fn has_images(req: &ChatRequest) -> bool {
    req.messages.iter().any(|m| {
        m.content
            .as_array()
            .is_some_and(|parts| parts.iter().any(is_image_part))
    }) || req
        .vertex
        .as_ref()
        .and_then(|v| v.media_uris.as_ref())
        .is_some_and(|uris| !uris.is_empty())
}

fn truncate(s: &str, cap: usize) -> String {
    s.chars().take(cap).collect()
}

/// `s` unchanged when within `cap` characters; otherwise its first and last
/// `cap / 2` characters joined by an ellipsis line.
fn head_tail(s: &str, cap: usize) -> String {
    let n = s.chars().count();
    match n <= cap {
        true => s.to_string(),
        false => format!(
            "{}\n…\n{}",
            s.chars().take(cap / 2).collect::<String>(),
            s.chars().skip(n - cap / 2).collect::<String>()
        ),
    }
}

/// How planned legs get their reasoning effort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortPolicy {
    /// Each tier's configured effort, one step harder when `bump`.
    Tier { bump: bool },
    /// The client set its own effort; the planner sets none.
    Client,
}

/// One leg of a plan and the index of the tier it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedLeg {
    pub leg: ChainLeg,
    pub tier: usize,
}

/// Legs in fallback order from tier `start` (see [`escalation_order`]), each
/// stamped with an effort per `policy`. Tiers with no legs contribute nothing.
pub fn order_legs(tiers: &[Tier], start: usize, policy: EffortPolicy) -> Vec<PlannedLeg> {
    escalation_order(tiers.len(), start)
        .into_iter()
        .flat_map(|i| {
            let effort = match policy {
                EffortPolicy::Client => None,
                EffortPolicy::Tier { bump: true } => Some(tiers[i].effort.bump()),
                EffortPolicy::Tier { bump: false } => Some(tiers[i].effort),
            };
            tiers[i].legs.iter().map(move |l| PlannedLeg {
                leg: ChainLeg {
                    effort,
                    ..l.clone()
                },
                tier: i,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::request::ChatRequest;
    use serde_json::json;

    fn req(body: serde_json::Value) -> ChatRequest {
        serde_json::from_value(body).unwrap()
    }

    #[test]
    fn resolve_mode_follows_route_unless_overridden() {
        assert_eq!(resolve_mode(true, None), Ok(RoutingMode::Jev));
        assert_eq!(resolve_mode(true, Some("jev")), Ok(RoutingMode::Jev));
        assert_eq!(
            resolve_mode(true, Some("static")),
            Ok(RoutingMode::StaticOverride)
        );
        assert_eq!(resolve_mode(false, None), Ok(RoutingMode::Static));
        assert_eq!(resolve_mode(false, Some("static")), Ok(RoutingMode::Static));
        assert!(resolve_mode(false, Some("jev"))
            .unwrap_err()
            .contains("requires a route with tiers"));
        assert!(resolve_mode(true, Some("fastest"))
            .unwrap_err()
            .contains("unknown routing_strategy 'fastest'"));
    }

    #[test]
    fn routing_mode_labels() {
        assert_eq!(RoutingMode::Jev.as_str(), "jev");
        assert_eq!(RoutingMode::Static.as_str(), "static");
        assert_eq!(RoutingMode::StaticOverride.as_str(), "static-override");
        assert_eq!(RoutingMode::default(), RoutingMode::Static);
    }

    #[test]
    fn state_carries_latest_user_message_system_prompt_and_flags() {
        let s = build_state(&req(json!({
            "model": "auto",
            "messages": [
                {"role": "system", "content": "You are terse."},
                {"role": "user", "content": "first"},
                {"role": "assistant", "content": "answer"},
                {"role": "user", "content": [
                    {"type": "text", "text": "what is in"},
                    {"type": "image_url", "image_url": {"url": "https://x/y.png"}}
                ]}
            ],
            "tools": [{"type": "function", "function": {"name": "f"}}]
        })));
        assert_eq!(s["latest_user_message"], "what is in\n[image]");
        assert_eq!(s["system_prompt"], "You are terse.");
        assert_eq!(s["has_tools"], true);
        assert_eq!(s["has_images"], true);
        assert_eq!(
            s["recent_history"],
            json!([
                {"role": "user", "content": "first"},
                {"role": "assistant", "content": "answer"}
            ])
        );
    }

    #[test]
    fn non_text_parts_become_placeholders_and_system_prompt_is_capped() {
        let s = build_state(&req(json!({
            "model": "auto",
            "messages": [
                {"role": "system", "content": "s".repeat(SYSTEM_CAP + 500)},
                {"role": "user", "content": [
                    {"type": "input_audio", "input_audio": {"data": "..."}},
                    {"type": "file", "file": {"file_id": "f"}},
                    {"type": "unknown"}
                ]}
            ],
            "tools": []
        })));
        assert_eq!(s["latest_user_message"], "[audio]\n[file]");
        assert_eq!(
            s["system_prompt"].as_str().unwrap().chars().count(),
            SYSTEM_CAP
        );
        assert_eq!(s["has_tools"], false);
        assert_eq!(s["has_images"], false);
    }

    #[test]
    fn long_latest_message_keeps_head_and_tail() {
        let long = format!("{}{}", "a".repeat(10_000), "z".repeat(10_000));
        let s = build_state(&req(json!({
            "model": "auto",
            "messages": [{"role": "user", "content": long}]
        })));
        let latest = s["latest_user_message"].as_str().unwrap();
        assert!(latest.starts_with(&"a".repeat(8_000)));
        assert!(latest.ends_with(&"z".repeat(8_000)));
        assert!(latest.contains("\n…\n"));
        assert_eq!(latest.chars().count(), LATEST_CAP + "\n…\n".chars().count());
    }

    #[test]
    fn history_fills_the_budget_newest_first_in_chronological_order() {
        let messages: Vec<serde_json::Value> = (0..30)
            .map(|i| {
                json!({
                    "role": if i % 2 == 0 { "user" } else { "assistant" },
                    "content": format!("{i}:{}", "x".repeat(1_990))
                })
            })
            .chain(std::iter::once(
                json!({"role": "user", "content": "latest"}),
            ))
            .collect();
        let s = build_state(&req(json!({"model": "auto", "messages": messages})));
        let history = s["recent_history"].as_array().unwrap();
        let total: usize = history
            .iter()
            .map(|h| h["content"].as_str().unwrap().chars().count())
            .sum();
        assert!(total <= STATE_BUDGET, "history {total} exceeds budget");
        assert!(history.len() < 30, "older messages were dropped");
        assert!(history.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .starts_with("29:"));
        let first_idx: usize = history[0]["content"]
            .as_str()
            .unwrap()
            .split(':')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(first_idx, 30 - history.len());
    }

    #[test]
    fn vertex_media_uris_count_as_images_and_no_user_message_is_empty_string() {
        let s = build_state(&req(json!({
            "model": "auto",
            "messages": [{"role": "system", "content": "sys"}],
            "vertex": {"media_uris": ["gs://b/v.mp4"]}
        })));
        assert_eq!(s["has_images"], true);
        assert_eq!(s["latest_user_message"], "");
        assert_eq!(s["has_tools"], false);
    }
}
