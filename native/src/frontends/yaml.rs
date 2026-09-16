//! YAML is an input frontend. Its serialized shape does not enter the engine.

use crate::{
    model::{Language, Severity},
    rule_ir::{Aggregation, Evidence, Rule, Scope, StructuralSelection, Threshold},
};
use anyhow::{Result, ensure};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlRule {
    version: u32,
    id: String,
    language: Language,
    summary: String,
    #[serde(default)]
    message: Option<String>,
    severity: Severity,
    select: YamlSelector,
    owner: YamlOwner,
    aggregate: YamlAggregation,
    threshold: YamlThreshold,
    evidence: YamlEvidence,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum YamlAggregation {
    Count,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum YamlSelector {
    Pattern(YamlPatternSelector),
    Kind(YamlKindSelector),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlPatternSelector {
    pattern: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlKindSelector {
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlOwner {
    nearest: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlThreshold {
    gt: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlEvidence {
    subject: String,
}

pub fn compile(source: &str) -> Result<Rule> {
    let yaml: YamlRule = serde_yaml_ng::from_str(source)?;
    ensure!(
        yaml.version == 1,
        "unsupported rule version: {}",
        yaml.version
    );
    ensure!(
        valid_id(&yaml.id),
        "invalid rule ID: expected lowercase namespace/name"
    );
    ensure!(
        !yaml.owner.nearest.is_empty(),
        "owner.nearest must not be empty"
    );
    ensure!(!yaml.summary.trim().is_empty(), "summary must not be empty");
    ensure!(
        !yaml.evidence.subject.trim().is_empty(),
        "evidence.subject must not be empty"
    );

    let message = yaml.message.unwrap_or_else(|| yaml.summary.clone());
    Ok(Rule {
        id: yaml.id,
        language: yaml.language,
        summary: yaml.summary,
        severity: yaml.severity,
        message,
        parameters: Default::default(),
        threshold_parameter: None,
        selection: match yaml.select {
            YamlSelector::Pattern(selector) => StructuralSelection::Pattern(selector.pattern),
            YamlSelector::Kind(selector) => StructuralSelection::Kind(selector.kind),
        },
        scope: Scope::NearestAncestor(yaml.owner.nearest),
        aggregation: match yaml.aggregate {
            YamlAggregation::Count => Aggregation::Count,
        },
        threshold: Threshold::GreaterThan(yaml.threshold.gt),
        evidence: Evidence {
            subject: yaml.evidence.subject,
        },
    })
}

fn valid_id(id: &str) -> bool {
    let parts = id.split('/').collect::<Vec<_>>();
    parts.len() == 2
        && parts.iter().all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}
