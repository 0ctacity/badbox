//! ast-grep implementation. No ast-grep AST types cross this module boundary.

use super::{Selection, SelectionTimings, StructuralBackend};
use crate::{
    model::{ByteRange, Language, OwnedMatch, RawOwner},
    rule_ir::{Rule, StructuralSelection},
};
use anyhow::{Result, ensure};
use ast_grep_core::{
    AstGrep, Matcher, Node, Pattern,
    matcher::{KindMatcher, MatcherExt},
    tree_sitter::StrDoc,
};
use ast_grep_language::{LanguageExt, SupportLang};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub struct AstGrepBackend;

enum CompiledSelector {
    Pattern(Box<Pattern>),
    Kind(KindMatcher),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExecutionKey {
    language: Language,
    selector_kind: u8,
    selector: String,
    owners: Vec<String>,
}

pub struct CompiledRule {
    rule: Rule,
    selector: CompiledSelector,
    owners: Vec<KindMatcher>,
    potential_kinds: Option<Vec<usize>>,
    execution_key: ExecutionKey,
}

pub struct ParsedFile(AstGrep<StrDoc<SupportLang>>);

fn backend_language(language: Language) -> SupportLang {
    match language {
        Language::Rust => SupportLang::Rust,
        Language::Go => SupportLang::Go,
    }
}

impl StructuralBackend for AstGrepBackend {
    type CompiledRule = CompiledRule;
    type ExecutionKey = ExecutionKey;
    type ParsedFile = ParsedFile;

    fn compile(rule: Rule) -> Result<CompiledRule> {
        let language = backend_language(rule.language);
        let (selector, selector_kind, selector_text) = match &rule.selection {
            StructuralSelection::Pattern(pattern) => {
                ensure!(!pattern.trim().is_empty(), "pattern must not be empty");
                (
                    CompiledSelector::Pattern(Box::new(Pattern::try_new(pattern, language)?)),
                    0,
                    pattern.clone(),
                )
            }
            StructuralSelection::Kind(kind) => (
                CompiledSelector::Kind(KindMatcher::try_new(kind, language)?),
                1,
                kind.clone(),
            ),
        };
        let potential_kinds = match &selector {
            CompiledSelector::Pattern(pattern) => pattern.potential_kinds(),
            CompiledSelector::Kind(kind) => kind.potential_kinds(),
        }
        .map(|kinds| kinds.iter().collect());
        let owners = rule
            .scope
            .nearest_ancestor_kinds()
            .iter()
            .map(|kind| KindMatcher::try_new(kind, language))
            .collect::<Result<Vec<_>, _>>()?;
        let mut owner_key = rule.scope.nearest_ancestor_kinds().to_vec();
        owner_key.sort();
        owner_key.dedup();
        let execution_key = ExecutionKey {
            language: rule.language,
            selector_kind,
            selector: selector_text,
            owners: owner_key,
        };
        Ok(CompiledRule {
            rule,
            selector,
            owners,
            potential_kinds,
            execution_key,
        })
    }

    fn rule(compiled: &CompiledRule) -> &Rule {
        &compiled.rule
    }

    fn execution_key(compiled: &CompiledRule) -> &ExecutionKey {
        &compiled.execution_key
    }

    fn parse(source: &str, language: Language) -> Result<ParsedFile> {
        let ast = backend_language(language).ast_grep(source);
        ensure!(
            !ast.root().get_inner_node().has_error(),
            "syntax errors; file skipped (no partial findings)"
        );
        Ok(ParsedFile(ast))
    }

    fn select_many(file: &ParsedFile, rules: &[&CompiledRule], profile: bool) -> Selection {
        let started = Instant::now();
        let mut ownership = Duration::ZERO;
        let mut matches = vec![Vec::new(); rules.len()];
        let mut by_kind: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut any_kind = Vec::new();
        for (index, rule) in rules.iter().enumerate() {
            if let Some(kinds) = &rule.potential_kinds {
                for kind in kinds {
                    by_kind.entry(*kind).or_default().push(index);
                }
            } else {
                any_kind.push(index);
            }
        }
        for candidate in file.0.root().dfs() {
            let kind = usize::from(candidate.kind_id());
            let candidates = by_kind.get(&kind).into_iter().flatten().chain(&any_kind);
            for &index in candidates {
                let rule = rules[index];
                let Some(matched) = match_candidate(rule, candidate.clone()) else {
                    continue;
                };
                let owner_started = profile.then(Instant::now);
                let owner = matched
                    .ancestors()
                    .find(|ancestor| rule.owners.iter().any(|kind| ancestor.matches(kind)));
                let Some(owner) = owner else {
                    if let Some(owner_started) = owner_started {
                        ownership += owner_started.elapsed();
                    }
                    continue;
                };
                matches[index].push(OwnedMatch {
                    range: byte_range(&matched),
                    owner: RawOwner {
                        range: byte_range(&owner),
                    },
                });
                if let Some(owner_started) = owner_started {
                    ownership += owner_started.elapsed();
                }
            }
        }
        let total = if profile {
            started.elapsed()
        } else {
            Duration::ZERO
        };
        Selection {
            matches,
            timings: SelectionTimings {
                matching: total.saturating_sub(ownership),
                ownership,
            },
        }
    }
}

fn match_candidate<'tree>(
    rule: &CompiledRule,
    candidate: Node<'tree, StrDoc<SupportLang>>,
) -> Option<Node<'tree, StrDoc<SupportLang>>> {
    let matched = match &rule.selector {
        CompiledSelector::Pattern(pattern) => pattern.match_node(candidate),
        CompiledSelector::Kind(kind) => kind.match_node(candidate),
    }?;
    Some(matched.into())
}

fn byte_range(node: &Node<'_, StrDoc<SupportLang>>) -> ByteRange {
    let range = node.range();
    ByteRange {
        start: range.start,
        end: range.end,
    }
}
