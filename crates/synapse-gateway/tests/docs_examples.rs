//! Loads every titled `routes.toml`, `pricing.toml`, `guardrails.toml` and
//! `ai_task_types.toml` example in the docs site through the gateway's own
//! parsers and startup validation, so a published config sample that would
//! fail to parse or validate breaks the build. Unknown keys are not caught:
//! the config structs do not use `deny_unknown_fields`.

use std::fs;
use std::path::{Path, PathBuf};

use synapse::ai_task_type::AiTaskTypeTable;
use synapse::guard::{GuardEngine, GuardrailsConfig};
use synapse::pricing::PricingTable;
use synapse::routing::table::RouteTable;
use tap::Tap;

const DOCS_ROOTS: [&str; 2] = [
    "../../docs/docs",
    "../../docs/i18n/es/docusaurus-plugin-content-docs/current",
];

#[derive(Debug)]
struct Snippet {
    file: PathBuf,
    line: usize,
    title: String,
    body: String,
}

fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
                .tap_mut(|paths| paths.sort())
                .into_iter()
                .flat_map(|path| {
                    if path.is_dir() {
                        markdown_files(&path)
                    } else if matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("md" | "mdx")
                    ) {
                        vec![path]
                    } else {
                        vec![]
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn snippets(file: &Path) -> Vec<Snippet> {
    let text = fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            let meta = line.trim_start().strip_prefix("```toml")?;
            let title = meta
                .split("title=\"")
                .nth(1)?
                .split('"')
                .next()?
                .to_string();
            // Leading newlines keep TOML error line numbers equal to the markdown file's.
            let body = "\n".repeat(i + 1)
                + &lines[i + 1..]
                    .iter()
                    .take_while(|l| !l.trim_start().starts_with("```"))
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n");
            Some(Snippet {
                file: file.to_path_buf(),
                line: i + 1,
                title,
                body,
            })
        })
        .collect()
}

fn parse(snippet: &Snippet) -> Option<anyhow::Result<()>> {
    match snippet.title.rsplit('/').next()? {
        "routes.toml" => Some(RouteTable::from_toml_str(&snippet.body).map(drop)),
        "pricing.toml" => Some(PricingTable::from_toml_str(&snippet.body).map(drop)),
        "guardrails.toml" => Some(
            GuardrailsConfig::from_toml_str(&snippet.body)
                .and_then(|config| GuardEngine::from_config(&config))
                .map(drop),
        ),
        "ai_task_types.toml" => Some(AiTaskTypeTable::from_toml_str(&snippet.body).map(drop)),
        _ => None,
    }
}

#[test]
fn every_titled_config_example_in_the_docs_parses() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let all: Vec<Snippet> = DOCS_ROOTS
        .iter()
        .map(|root| manifest.join(root))
        .flat_map(|dir| markdown_files(&dir))
        .flat_map(|file| snippets(&file))
        .collect();
    let checked: Vec<(&Snippet, anyhow::Result<()>)> = all
        .iter()
        .filter_map(|s| parse(s).map(|result| (s, result)))
        .collect();
    assert!(
        !checked.is_empty(),
        "no titled config examples found under docs/"
    );
    let failures: Vec<String> = checked
        .iter()
        .filter_map(|(s, result)| {
            result
                .as_ref()
                .err()
                .map(|e| format!("{}:{} ({}): {e:#}", s.file.display(), s.line, s.title))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "docs config examples failed to parse:\n{}",
        failures.join("\n")
    );
}
