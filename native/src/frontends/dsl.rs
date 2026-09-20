//! Tiny DSL frontend. Syntax types stop here; the engine consumes Badbox Rule IR.

use crate::{
    model::{Language, Severity},
    rule_ir::{
        Aggregation, Condition, Evidence, ParameterValue, Relation, RelationTarget, Rule, Scope,
        StructuralSelection, TextOperator, Threshold,
    },
};
use anyhow::{Result, anyhow, bail, ensure};
use tiny_dsl::{ComparisonValue, Group, Selection, WhereClause};

pub fn compile(source: &str) -> Result<Vec<Rule>> {
    let document = tiny_dsl::parse(source)?;
    document.rules.into_iter().map(lower_rule).collect()
}

fn lower_rule(rule: tiny_dsl::Rule) -> Result<Rule> {
    let language = language(&rule.language)?;
    let conditions = lower_conditions(&rule.selection, &rule.where_clauses, language, &rule.id)?;
    let parameters = rule
        .parameters
        .iter()
        .map(|parameter| {
            let value = match &parameter.value {
                ComparisonValue::Integer(value) => ParameterValue::Integer(*value),
                ComparisonValue::Boolean(value) => ParameterValue::Boolean(*value),
                ComparisonValue::String(value) => ParameterValue::String(value.clone()),
                ComparisonValue::Parameter(_) => unreachable!("parser rejects parameter defaults"),
            };
            (parameter.name.clone(), value)
        })
        .collect();
    let (threshold, threshold_parameter) = match rule.condition.value {
        ComparisonValue::Integer(value) => (value, None),
        ComparisonValue::Parameter(name) => rule
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .and_then(|parameter| match parameter.value {
                ComparisonValue::Integer(value) => Some(value),
                _ => None,
            })
            .map(|value| (value, Some(name.clone())))
            .ok_or_else(|| anyhow!("rule {} has invalid count parameter {name}", rule.id))?,
        _ => bail!("rule {} count threshold must be an integer", rule.id),
    };
    let selection = lower_selection(&rule.selection, language)?;
    let owners = match rule.group {
        Group::Callable => callable_kinds(language)
            .ok_or_else(|| {
                anyhow!(
                    "rule {} uses callable grouping, which is not mapped for {} yet; use nearest node(...) instead",
                    rule.id,
                    rule.language
                )
            })?
            .iter()
            .map(|kind| (*kind).to_owned())
            .collect(),
        Group::Node(kinds) => kinds,
    };
    Ok(Rule {
        id: rule.id,
        language,
        summary: rule.summary,
        severity: match rule.report.severity {
            tiny_dsl::Severity::Info => Severity::Info,
            tiny_dsl::Severity::Warning => Severity::Warning,
            tiny_dsl::Severity::Error => Severity::Error,
        },
        message: rule.report.message,
        parameters,
        threshold_parameter,
        selection,
        conditions,
        scope: Scope::NearestAncestor(owners),
        aggregation: Aggregation::Count,
        threshold: Threshold::GreaterThan(threshold),
        evidence: Evidence {
            subject: rule.report.evidence,
        },
    })
}

fn lower_selection(selection: &Selection, language: Language) -> Result<StructuralSelection> {
    Ok(match selection {
        Selection::Node(kind) => StructuralSelection::Kind(kind.clone()),
        Selection::Code { captures, template } => {
            StructuralSelection::Pattern(lower_captures(template, captures, language)?)
        }
    })
}

fn lower_conditions(
    selection: &Selection,
    clauses: &[WhereClause],
    language: Language,
    rule_id: &str,
) -> Result<Vec<Condition>> {
    let captures = match selection {
        Selection::Code { captures, .. } => captures
            .iter()
            .map(|capture| (capture.name.as_str(), capture.multiple))
            .collect::<std::collections::HashMap<_, _>>(),
        Selection::Node(_) => std::collections::HashMap::new(),
    };
    clauses
        .iter()
        .map(|clause| match clause {
            WhereClause::Text {
                capture,
                operator,
                values,
            } => {
                let multiple = captures.get(capture.as_str()).ok_or_else(|| {
                    anyhow!("rule {rule_id} filters undeclared capture {capture}")
                })?;
                ensure!(
                    !multiple,
                    "rule {rule_id} cannot apply text conditions to multiple capture {capture}"
                );
                Ok(Condition::Text {
                    capture: capture.to_ascii_uppercase(),
                    operator: match operator {
                        tiny_dsl::TextOperator::Equal => TextOperator::Equal,
                        tiny_dsl::TextOperator::NotEqual => TextOperator::NotEqual,
                        tiny_dsl::TextOperator::In => TextOperator::In,
                        tiny_dsl::TextOperator::NotIn => TextOperator::NotIn,
                        tiny_dsl::TextOperator::Matches => TextOperator::Matches,
                    },
                    values: values.clone(),
                })
            }
            WhereClause::Relation {
                target,
                relation,
                require_all,
                selections,
            } => {
                let target = match target {
                    tiny_dsl::RelationTarget::Match => RelationTarget::Match,
                    tiny_dsl::RelationTarget::Group => RelationTarget::Group,
                };
                let relation = match relation {
                    tiny_dsl::Relation::Has => Relation::Has,
                    tiny_dsl::Relation::Lacks => Relation::Lacks,
                    tiny_dsl::Relation::Inside => Relation::Inside,
                    tiny_dsl::Relation::Follows | tiny_dsl::Relation::Precedes => {
                        bail!(
                            "rule {rule_id} uses a relational condition that does not execute yet"
                        )
                    }
                };
                ensure!(
                    matches!(
                        (target, relation),
                        (RelationTarget::Match, Relation::Inside)
                            | (RelationTarget::Group, Relation::Has | Relation::Lacks)
                    ),
                    "rule {rule_id} uses a relational condition that does not execute yet"
                );
                Ok(Condition::Relation {
                    target,
                    relation,
                    require_all: *require_all,
                    selections: selections
                        .iter()
                        .map(|selection| lower_selection(selection, language))
                        .collect::<Result<_>>()?,
                })
            }
        })
        .collect()
}

fn lower_captures(
    template: &str,
    captures: &[tiny_dsl::Capture],
    language: Language,
) -> Result<String> {
    if language == Language::PowerShell && captures.iter().any(|capture| capture.multiple) {
        bail!("PowerShell code patterns do not support multiple captures yet");
    }
    let captures = captures
        .iter()
        .map(|capture| (capture.name.as_str(), capture.multiple))
        .collect::<std::collections::HashMap<_, _>>();
    let mut output = String::with_capacity(template.len());
    let mut chars = template.char_indices().peekable();
    let mut quote = None;
    while let Some((start, character)) = chars.next() {
        if let Some(active) = quote {
            output.push(character);
            if character == '\\' {
                if let Some((_, escaped)) = chars.next() {
                    output.push(escaped);
                }
            } else if character == active {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            output.push(character);
            continue;
        }
        if character.is_ascii_alphabetic() || character == '_' {
            let mut end = start + character.len_utf8();
            while let Some(&(index, next)) = chars.peek() {
                if !next.is_ascii_alphanumeric() && next != '_' {
                    break;
                }
                chars.next();
                end = index + next.len_utf8();
            }
            let identifier = &template[start..end];
            if let Some(multiple) = captures.get(identifier) {
                let name = identifier.to_ascii_uppercase();
                if language == Language::PowerShell {
                    output.push('#');
                } else if *multiple {
                    output.push_str("$$$");
                } else {
                    output.push('$');
                }
                output.push_str(&name);
            } else {
                output.push_str(identifier);
            }
            continue;
        }
        output.push(character);
    }
    for capture in captures.keys() {
        let marker = if language == Language::PowerShell {
            format!("#{}", capture.to_ascii_uppercase())
        } else {
            format!("${}", capture.to_ascii_uppercase())
        };
        ensure!(
            output.contains(&marker),
            "declared capture {capture} is not used in the code pattern"
        );
    }
    Ok(output)
}

fn callable_kinds(language: Language) -> Option<&'static [&'static str]> {
    match language {
        Language::Rust => Some(&["function_item", "closure_expression"]),
        Language::Go => Some(&["function_declaration", "method_declaration", "func_literal"]),
        Language::PowerShell => Some(&["function_statement"]),
        Language::Zig => Some(&["function_declaration"]),
        _ => None,
    }
}

fn language(value: &str) -> Result<Language> {
    Ok(match value {
        "bash" => Language::Bash,
        "c" => Language::C,
        "cpp" => Language::Cpp,
        "csharp" => Language::CSharp,
        "css" => Language::Css,
        "dart" => Language::Dart,
        "elixir" => Language::Elixir,
        "go" => Language::Go,
        "haskell" => Language::Haskell,
        "hcl" => Language::Hcl,
        "html" => Language::Html,
        "java" => Language::Java,
        "javascript" => Language::JavaScript,
        "json" => Language::Json,
        "kotlin" => Language::Kotlin,
        "lua" => Language::Lua,
        "markdown" => Language::Markdown,
        "nix" => Language::Nix,
        "php" => Language::Php,
        "powershell" => Language::PowerShell,
        "python" => Language::Python,
        "ruby" => Language::Ruby,
        "rust" => Language::Rust,
        "scala" => Language::Scala,
        "solidity" => Language::Solidity,
        "swift" => Language::Swift,
        "tsx" => Language::Tsx,
        "typescript" => Language::TypeScript,
        "yaml" => Language::Yaml,
        "zig" => Language::Zig,
        _ => bail!("unsupported language {value}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_captures_lower_to_backend_metavariables() {
        let captures = [tiny_dsl::Capture {
            name: "value".to_owned(),
            multiple: false,
        }];
        assert_eq!(
            lower_captures("value.clone()", &captures, Language::Rust).unwrap(),
            "$VALUE.clone()"
        );
        assert_eq!(
            lower_captures("Invoke-Expression value", &captures, Language::PowerShell).unwrap(),
            "Invoke-Expression #VALUE"
        );
        assert_eq!(
            lower_captures("\"value\" + value", &captures, Language::Rust).unwrap(),
            "\"value\" + $VALUE"
        );
    }

    #[test]
    fn unsupported_relational_clauses_are_rejected_explicitly() {
        let error = compile(
            r#"#badbox 1
rule rust/guarded for rust {
  summary "guarded"
  find code(value) `value.clone()`
  group by nearest callable
  where match follows any { node return_expression }
  when count > 0
  report {
    severity info
    message "guard missing"
    evidence "clone call sites"
  }
}
"#,
        )
        .expect_err("follows execution is not implemented");
        assert!(error.to_string().contains("does not execute yet"));
    }
}
