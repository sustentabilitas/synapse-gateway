//! Jev router: pure decision logic for `strategy = "jev"` routes. Jev rates
//! each request's difficulty against the route's tiers; this module turns the
//! answers into an ordered, effort-stamped leg plan and a client-facing report.
//! Spec: `docs/superpowers/specs/2026-09-28-jev-router-design.md`.

use crate::routing::table::{escalation_order, ChainLeg, Tier};

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
