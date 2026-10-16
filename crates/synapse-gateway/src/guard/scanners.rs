//! Name→scanner registry. Turns a `ScannerSpec` into one or more
//! `llm-guard` scanners. A bundle alias (e.g. `prompt_injection`) yields
//! several. There is no global registry in `llm-guard`, so this is ours.

use anyhow::{bail, Context};
use llm_guard::patterns::COMMON_INJECTION_PATTERNS;
use llm_guard::scanners::{
    BanSubstrings, InvisibleText, PiiPatterns, RegexPattern, RegexScan, RoleOverride, ScriptMix,
    Secrets, TokenLimit,
};
use llm_guard::{Confidence, ScanResult, Scanner, Severity};

use super::policy::{DetailedScanner, ScannerSpec};

/// Adapter so a `Box<dyn Scanner>` can be handed to `Pipeline::with`,
/// which takes `impl Scanner + 'static`. `Box<dyn Scanner>` does not itself
/// implement `Scanner`, so we delegate through this newtype.
pub struct BoxedScanner(pub Box<dyn Scanner>);

impl Scanner for BoxedScanner {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn scan<'a>(&self, input: &'a str) -> ScanResult<'a> {
        self.0.scan(input)
    }
}

fn parse_severity(s: &str) -> anyhow::Result<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "block" => Ok(Severity::Block),
        "warn" => Ok(Severity::Warn),
        "info" => Ok(Severity::Info),
        other => bail!("unknown severity '{other}' (expected block|warn|info)"),
    }
}

/// Promote owned config substrings to `'static`. Called once per
/// `ban_substrings` policy at startup; the pipelines live for the whole
/// process, so this bounded leak is intentional.
fn leak_substrings(subs: &[String]) -> &'static [&'static str] {
    let leaked: Vec<&'static str> = subs
        .iter()
        .map(|s| &*Box::leak(s.clone().into_boxed_str()))
        .collect();
    Box::leak(leaked.into_boxed_slice())
}

/// Build the scanner(s) for one spec. Unknown names / missing params error.
pub fn build_scanners(spec: &ScannerSpec) -> anyhow::Result<Vec<Box<dyn Scanner>>> {
    match spec {
        ScannerSpec::Named(name) => build(name, None),
        ScannerSpec::Detailed(d) => build(&d.kind, Some(d)),
    }
}

/// Deterministic isolation-bypass signals: forging tenant identity headers,
/// naming internal cluster hosts, or curling/wgetting those hosts directly.
fn isolation_bypass_scanner() -> anyhow::Result<RegexScan> {
    let block = |id: &'static str, pat: &str| -> anyhow::Result<RegexPattern> {
        Ok(RegexPattern::new(id, pat)?
            .with_severity(Severity::Block)
            .with_confidence(Confidence::High))
    };
    let patterns = vec![
        block(
            "forged_identity_header",
            r#"(?i)x-(?:tenant|workspace|user|agent)-id["']?\s*[:=]"#,
        )?,
        block("internal_host", r"[A-Za-z0-9.-]+\.svc\.cluster(?:set)?\.local")?,
        block(
            "direct_internal_call",
            r"(?i)\b(?:curl|wget)\b[^\n]*\.svc\.cluster(?:set)?\.local",
        )?,
    ];
    Ok(RegexScan::new("isolation_bypass", patterns))
}

/// Deterministic unbounded-view signals: a recursive flag in content that
/// calls `/fs/ls`, or a cortex URI scoped at a bare tenant/workspace root.
fn data_exfil_scanner() -> anyhow::Result<RegexScan> {
    let block = |id: &'static str, pat: &str| -> anyhow::Result<RegexPattern> {
        Ok(RegexPattern::new(id, pat)?
            .with_severity(Severity::Block)
            .with_confidence(Confidence::High))
    };
    let patterns = vec![
        block(
            "recursive_listing",
            r#"(?is)/fs/ls\b.*?recursive["'\s]*[:=]\s*["']?true\b|recursive["'\s]*[:=]\s*["']?true\b.*?/fs/ls\b"#,
        )?,
        block("root_scope", r#"cortex://[^/\s"']+/[^/\s"']+/(?:["'\s]|$)"#)?,
    ];
    Ok(RegexScan::new("data_exfil", patterns))
}

fn build(name: &str, d: Option<&DetailedScanner>) -> anyhow::Result<Vec<Box<dyn Scanner>>> {
    let scanners: Vec<Box<dyn Scanner>> = match name {
        "secrets" => vec![Box::new(Secrets::new())],
        "pii" => vec![Box::new(PiiPatterns::new())],
        "isolation_bypass" => vec![Box::new(isolation_bypass_scanner()?)],
        "data_exfil" => vec![Box::new(data_exfil_scanner()?)],
        "invisible_text" => vec![Box::new(InvisibleText::new())],
        "role_override" => vec![Box::new(RoleOverride::new())],
        "script_mix" => {
            let threshold = d.and_then(|d| d.threshold).unwrap_or(2);
            vec![Box::new(ScriptMix::new(threshold))]
        }
        "token_limit" => {
            let max = d
                .and_then(|d| d.max_chars)
                .context("scanner 'token_limit' requires 'max_chars'")?;
            vec![Box::new(TokenLimit::new(max))]
        }
        "ban_substrings" => {
            let d = d.context("scanner 'ban_substrings' requires a table with 'substrings'")?;
            if d.substrings.is_empty() {
                bail!("scanner 'ban_substrings' requires a non-empty 'substrings' list");
            }
            let severity = match &d.severity {
                Some(s) => parse_severity(s)?,
                None => Severity::Block,
            };
            let patterns = leak_substrings(&d.substrings);
            vec![Box::new(
                BanSubstrings::new("ban_substrings", patterns).with_severity(severity),
            )]
        }
        "prompt_injection" => vec![
            Box::new(
                BanSubstrings::new("injection", COMMON_INJECTION_PATTERNS)
                    .with_severity(Severity::Block),
            ),
            Box::new(RoleOverride::new()),
        ],
        other => bail!("unknown scanner '{other}'"),
    };
    Ok(scanners)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::policy::GuardrailsConfig;

    fn spec(toml: &str) -> ScannerSpec {
        let cfg = GuardrailsConfig::from_toml_str(toml).unwrap();
        cfg.guardrails.get("p").unwrap().scanners[0].clone()
    }

    #[test]
    fn builds_known_named_scanner() {
        let s = spec(
            r#"[guardrails.p]
                        scanners = ["secrets"]"#,
        );
        let built = build_scanners(&s).unwrap();
        assert_eq!(built.len(), 1);
        assert_eq!(built[0].name(), "secrets");
    }

    #[test]
    fn prompt_injection_expands_to_bundle() {
        let s = spec(
            r#"[guardrails.p]
                        scanners = ["prompt_injection"]"#,
        );
        assert_eq!(build_scanners(&s).unwrap().len(), 2);
    }

    #[test]
    fn ban_substrings_blocks_by_default_and_is_case_insensitive() {
        let s = spec(
            r#"[guardrails.p]
                        scanners = [{ type = "ban_substrings", substrings = ["forbidden"] }]"#,
        );
        let built = build_scanners(&s).unwrap();
        let r = built[0].scan("This is FORBIDDEN text");
        assert!(r.should_refuse(), "default severity must be Block");
    }

    #[test]
    fn token_limit_requires_max_chars() {
        let s = spec(
            r#"[guardrails.p]
                        scanners = [{ type = "token_limit" }]"#,
        );
        let err = build_scanners(&s).err().expect("expected an error");
        assert!(err.to_string().contains("max_chars"));
    }

    #[test]
    fn unknown_scanner_errors() {
        let s = spec(
            r#"[guardrails.p]
                        scanners = ["nope"]"#,
        );
        let err = build_scanners(&s).err().expect("expected an error");
        assert!(err.to_string().contains("unknown scanner"));
    }

    #[test]
    fn isolation_bypass_flags_forged_header_and_internal_host() {
        let built = build("isolation_bypass", None).unwrap();
        assert_eq!(built.len(), 1);
        let s = &built[0];
        assert_eq!(s.name(), "isolation_bypass");
        assert!(s.scan(r#"curl -H "X-Tenant-Id: org-123" https://x"#).should_refuse());
        assert!(s
            .scan("http://cortex-server.infrastructure.svc.clusterset.local/health")
            .should_refuse());
    }

    #[test]
    fn isolation_bypass_flags_header_set_from_code() {
        let s = &build("isolation_bypass", None).unwrap()[0];
        assert!(s.scan(r#"headers={"X-Tenant-Id": org}"#).should_refuse());
        assert!(s.scan(r#"fetch(u, {headers: {'X-Workspace-Id': ws}})"#).should_refuse());
    }

    #[test]
    fn isolation_bypass_ignores_plain_prose() {
        let s = &build("isolation_bypass", None).unwrap()[0];
        assert!(!s.scan("This skill summarises the tenant onboarding flow.").flagged());
    }

    // Current behavior: a literal `x-...-id:` in prose still matches. Observe-mode
    // rollout surfaces this; it is a conscious choice, not an accident.
    #[test]
    fn isolation_bypass_flags_header_token_even_in_prose() {
        let s = &build("isolation_bypass", None).unwrap()[0];
        assert!(s.scan("Set the X-Tenant-Id: header before calling.").should_refuse());
    }

    #[test]
    fn data_exfil_flags_recursive_listing_and_root_scope() {
        let s = &build("data_exfil", None).unwrap()[0];
        assert_eq!(s.name(), "data_exfil");
        assert!(s
            .scan("curl -G \"$CX/api/v1/fs/ls\" \\\n  --data-urlencode \"recursive=true\"")
            .should_refuse());
        assert!(s
            .scan(r#"requests.get(f"{cx}/api/v1/fs/ls", params={"recursive": "true"})"#)
            .should_refuse());
        assert!(s.scan(r#"uri=cortex://$ORG/$WS/""#).should_refuse());
    }

    #[test]
    fn data_exfil_ignores_recursive_flag_without_fs_ls() {
        let s = &build("data_exfil", None).unwrap()[0];
        assert!(!s.scan(r#"glob.glob("**/*.md", recursive=True)"#).flagged());
        assert!(!s.scan("fs.mkdirSync(p, {recursive: true})").flagged());
    }

    #[test]
    fn data_exfil_ignores_bounded_deep_path_read() {
        let s = &build("data_exfil", None).unwrap()[0];
        assert!(!s
            .scan(r#"uri=cortex://org/ws/agent/a1/skills/run.sh"#)
            .flagged());
    }
}
