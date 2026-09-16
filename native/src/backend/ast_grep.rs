//! ast-grep implementation. No ast-grep AST types cross this module boundary.

use super::{Selection, SelectionTimings, StructuralBackend};
use crate::{
    model::{ByteRange, Language, OwnedMatch, RawOwner},
    rule_ir::{Rule, StructuralSelection},
};
use anyhow::{Result, ensure};
use ast_grep_core::{
    AstGrep, Matcher, Node, Pattern,
    language::Language as AstGrepLanguage,
    matcher::{KindMatcher, MatcherExt, PatternBuilder, PatternError},
    meta_var::MetaVariable,
    tree_sitter::{LanguageExt, StrDoc, TSLanguage, TSRange},
};
use ast_grep_language::SupportLang;
use std::{
    borrow::Cow,
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

pub struct ParsedFile(AstGrep<StrDoc<BackendLanguage>>);

#[derive(Clone, Copy, Debug)]
enum BackendLanguage {
    BuiltIn(SupportLang),
    PowerShell,
    Zig,
}

impl AstGrepLanguage for BackendLanguage {
    fn kind_to_id(&self, kind: &str) -> u16 {
        match self {
            Self::BuiltIn(language) => language.kind_to_id(kind),
            Self::PowerShell | Self::Zig => self.get_ts_language().id_for_node_kind(kind, true),
        }
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        match self {
            Self::BuiltIn(language) => language.field_to_id(field),
            Self::PowerShell | Self::Zig => self
                .get_ts_language()
                .field_id_for_name(field)
                .map(|id| id.get()),
        }
    }

    fn meta_var_char(&self) -> char {
        match self {
            Self::BuiltIn(language) => language.meta_var_char(),
            Self::PowerShell => '#',
            Self::Zig => '$',
        }
    }

    fn expando_char(&self) -> char {
        match self {
            Self::BuiltIn(language) => language.expando_char(),
            Self::PowerShell => '$',
            Self::Zig => '_',
        }
    }

    fn pre_process_pattern<'query>(&self, query: &'query str) -> Cow<'query, str> {
        match self {
            Self::BuiltIn(language) => language.pre_process_pattern(query),
            Self::PowerShell => preprocess_powershell_pattern(query),
            Self::Zig => preprocess_expando_pattern(self.expando_char(), query),
        }
    }

    fn extract_meta_var(&self, source: &str) -> Option<MetaVariable> {
        match self {
            Self::PowerShell => extract_powershell_meta_var(source),
            Self::BuiltIn(language) => language.extract_meta_var(source),
            Self::Zig => extract_expando_meta_var(source, self.expando_char()),
        }
    }

    fn build_pattern(&self, builder: &PatternBuilder<'_>) -> Result<Pattern, PatternError> {
        match self {
            Self::BuiltIn(language) => language.build_pattern(builder),
            Self::PowerShell => {
                let pattern = builder.build(|source| StrDoc::try_new(source, *self))?;
                if pattern.has_error() {
                    return Err(PatternError::Parse(
                        "invalid PowerShell structural pattern".to_owned(),
                    ));
                }
                Ok(pattern)
            }
            Self::Zig => builder.build(|source| StrDoc::try_new(source, *self)),
        }
    }
}

impl LanguageExt for BackendLanguage {
    fn get_ts_language(&self) -> TSLanguage {
        match self {
            Self::BuiltIn(language) => language.get_ts_language(),
            Self::PowerShell => tree_sitter_powershell::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
        }
    }

    fn injectable_languages(&self) -> Option<&'static [&'static str]> {
        match self {
            Self::BuiltIn(language) => language.injectable_languages(),
            Self::PowerShell | Self::Zig => None,
        }
    }

    fn extract_injections<L: LanguageExt>(
        &self,
        root: Node<'_, StrDoc<L>>,
    ) -> Vec<(String, Vec<TSRange>)> {
        match self {
            Self::BuiltIn(language) => language.extract_injections(root),
            Self::PowerShell | Self::Zig => Vec::new(),
        }
    }
}

const POWERSHELL_CAPTURE_PREFIX: &str = "$__BADBOX_CAPTURE_";

fn preprocess_powershell_pattern(query: &str) -> Cow<'_, str> {
    let mut output = String::with_capacity(query.len());
    let mut characters = query.char_indices().peekable();
    let mut quote = None;
    while let Some((_, character)) = characters.next() {
        if character == '`' && quote != Some('\'') {
            output.push(character);
            if let Some((_, escaped)) = characters.next() {
                output.push(escaped);
            }
            continue;
        }
        if let Some(active_quote) = quote {
            output.push(character);
            if character == active_quote {
                if active_quote == '\'' && characters.peek().is_some_and(|(_, next)| *next == '\'')
                {
                    if let Some((_, escaped_quote)) = characters.next() {
                        output.push(escaped_quote);
                    }
                } else {
                    quote = None;
                }
            }
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            output.push(character);
            continue;
        }
        if character != '#' {
            output.push(character);
            continue;
        }
        let Some(&(_, first)) = characters.peek() else {
            output.push(character);
            continue;
        };
        if !first.is_ascii_alphabetic() && first != '_' {
            output.push(character);
            continue;
        }
        let mut name = String::new();
        while let Some(&(_, next)) = characters.peek() {
            if !next.is_ascii_alphanumeric() && next != '_' {
                break;
            }
            name.push(next);
            characters.next();
        }
        if name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            output.push_str(POWERSHELL_CAPTURE_PREFIX);
            output.push_str(&name);
        } else {
            output.push('(');
        }
    }
    Cow::Owned(output)
}

fn extract_expando_meta_var(source: &str, expando: char) -> Option<MetaVariable> {
    let ellipsis = std::iter::repeat_n(expando, 3).collect::<String>();
    if source == ellipsis {
        return Some(MetaVariable::Multiple);
    }
    if let Some(name) = source.strip_prefix(&ellipsis) {
        if !valid_meta_var_name(name) {
            return None;
        }
        return if name.starts_with('_') {
            Some(MetaVariable::Multiple)
        } else {
            Some(MetaVariable::MultiCapture(name.to_owned()))
        };
    }
    let name = source.strip_prefix(expando)?;
    let (name, named) = match name.strip_prefix(expando) {
        Some(name) => (name, false),
        None => (name, true),
    };
    if !valid_meta_var_name(name) {
        return None;
    }
    if name.starts_with('_') {
        Some(MetaVariable::Dropped(named))
    } else {
        Some(MetaVariable::Capture(name.to_owned(), named))
    }
}

fn valid_meta_var_name(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn extract_powershell_meta_var(source: &str) -> Option<MetaVariable> {
    let name = source.strip_prefix(POWERSHELL_CAPTURE_PREFIX)?;
    if name == "_" {
        Some(MetaVariable::Dropped(true))
    } else {
        Some(MetaVariable::Capture(name.to_owned(), true))
    }
}

fn preprocess_expando_pattern(expando: char, query: &str) -> Cow<'_, str> {
    let mut output = String::with_capacity(query.len());
    let mut dollar_count = 0;
    for character in query.chars() {
        if character == '$' {
            dollar_count += 1;
            continue;
        }
        let replace = matches!(character, 'A'..='Z' | '_') || dollar_count == 3;
        output.extend(std::iter::repeat_n(
            if replace { expando } else { '$' },
            dollar_count,
        ));
        dollar_count = 0;
        output.push(character);
    }
    output.extend(std::iter::repeat_n(
        if dollar_count == 3 { expando } else { '$' },
        dollar_count,
    ));
    Cow::Owned(output)
}

fn backend_language(language: Language) -> BackendLanguage {
    let built_in = match language {
        Language::Bash => SupportLang::Bash,
        Language::C => SupportLang::C,
        Language::Cpp => SupportLang::Cpp,
        Language::CSharp => SupportLang::CSharp,
        Language::Css => SupportLang::Css,
        Language::Dart => SupportLang::Dart,
        Language::Elixir => SupportLang::Elixir,
        Language::Rust => SupportLang::Rust,
        Language::Go => SupportLang::Go,
        Language::Haskell => SupportLang::Haskell,
        Language::Hcl => SupportLang::Hcl,
        Language::Html => SupportLang::Html,
        Language::Java => SupportLang::Java,
        Language::JavaScript => SupportLang::JavaScript,
        Language::Json => SupportLang::Json,
        Language::Kotlin => SupportLang::Kotlin,
        Language::Lua => SupportLang::Lua,
        Language::Markdown => SupportLang::Markdown,
        Language::Nix => SupportLang::Nix,
        Language::Php => SupportLang::Php,
        Language::PowerShell => return BackendLanguage::PowerShell,
        Language::Python => SupportLang::Python,
        Language::Ruby => SupportLang::Ruby,
        Language::Scala => SupportLang::Scala,
        Language::Solidity => SupportLang::Solidity,
        Language::Swift => SupportLang::Swift,
        Language::Tsx => SupportLang::Tsx,
        Language::TypeScript => SupportLang::TypeScript,
        Language::Yaml => SupportLang::Yaml,
        Language::Zig => return BackendLanguage::Zig,
    };
    BackendLanguage::BuiltIn(built_in)
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
                if rule.language == Language::PowerShell {
                    ensure!(
                        !pattern.contains(POWERSHELL_CAPTURE_PREFIX),
                        "PowerShell pattern uses Badbox's reserved capture marker"
                    );
                }
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
    candidate: Node<'tree, StrDoc<BackendLanguage>>,
) -> Option<Node<'tree, StrDoc<BackendLanguage>>> {
    let matched = match &rule.selector {
        CompiledSelector::Pattern(pattern) => pattern.match_node(candidate),
        CompiledSelector::Kind(kind) => kind.match_node(candidate),
    }?;
    Some(matched.into())
}

fn byte_range(node: &Node<'_, StrDoc<BackendLanguage>>) -> ByteRange {
    let range = node.range();
    ByteRange {
        start: range.start,
        end: range.end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern_match_count(source: &str, pattern: &str) -> usize {
        let language = BackendLanguage::PowerShell;
        let ast = language.ast_grep(source);
        let pattern = Pattern::try_new(pattern, language).expect("pattern compiles");
        ast.root().find_all(&pattern).count()
    }

    #[test]
    fn all_ast_grep_builtin_languages_parse() {
        let cases = [
            (Language::Bash, "echo hi\n"),
            (Language::C, "int value;\n"),
            (Language::Cpp, "int value;\n"),
            (Language::CSharp, "class Example {}\n"),
            (Language::Css, ".example { color: red; }\n"),
            (Language::Dart, "void main() {}\n"),
            (Language::Elixir, "value = 1\n"),
            (Language::Go, "package example\n"),
            (Language::Haskell, "value = 1\n"),
            (Language::Hcl, "value = 1\n"),
            (Language::Html, "<div></div>\n"),
            (Language::Java, "class Example {}\n"),
            (Language::JavaScript, "const value = 1;\n"),
            (Language::Json, "{\"value\": 1}\n"),
            (Language::Kotlin, "val value = 1\n"),
            (Language::Lua, "local value = 1\n"),
            (Language::Markdown, "# Title\n"),
            (Language::Nix, "{ value = 1; }\n"),
            (Language::Php, "<?php $value = 1;\n"),
            (
                Language::PowerShell,
                "function Example { Write-Output $value }\n",
            ),
            (Language::Python, "value = 1\n"),
            (Language::Ruby, "value = 1\n"),
            (Language::Rust, "fn main() {}\n"),
            (Language::Scala, "val value = 1\n"),
            (Language::Solidity, "contract Example {}\n"),
            (Language::Swift, "let value = 1\n"),
            (Language::Tsx, "const value = <div />;\n"),
            (Language::TypeScript, "const value: number = 1;\n"),
            (Language::Yaml, "value: 1\n"),
            (Language::Zig, "pub fn main() void {}\n"),
        ];

        assert_eq!(cases.len(), Language::ALL.len());
        for (language, source) in cases {
            AstGrepBackend::parse(source, language)
                .unwrap_or_else(|error| panic!("{language:?} did not parse: {error:#}"));
        }
    }

    #[test]
    fn powershell_patterns_keep_source_variables_literal_and_use_hash_captures() {
        let source = r##"
function Example {
    Write-Output $message
    Write-Output $other
    Write-Output $MESSAGE
    Write-Output "#VALUE"
    Write-Output '#OTHER'
}
"##;

        assert_eq!(pattern_match_count(source, "Write-Output $message"), 1);
        assert_eq!(pattern_match_count(source, "Write-Output $MESSAGE"), 1);
        assert_eq!(pattern_match_count(source, "Write-Output \"#VALUE\""), 1);
        assert_eq!(pattern_match_count(source, "Write-Output '#OTHER'"), 1);
        assert_eq!(pattern_match_count(source, "Write-Output #VALUE"), 5);
    }

    #[test]
    fn powershell_repeated_capture_requires_the_same_syntax() {
        let source = r#"
function Example {
    Compare-Values $left $left
    Compare-Values $left $right
}
"#;

        assert_eq!(
            pattern_match_count(source, "Compare-Values #VALUE #VALUE"),
            1
        );
    }

    #[test]
    fn powershell_rejects_lowercase_hash_capture_names() {
        let language = BackendLanguage::PowerShell;
        assert!(Pattern::try_new("Write-Output #value", language).is_err());
    }

    #[test]
    fn powershell_rejects_the_internal_capture_marker_as_literal_syntax() {
        use crate::rule_ir::{Aggregation, Evidence, Scope, Threshold};

        let rule = Rule {
            id: "powershell/internal-marker".to_owned(),
            language: Language::PowerShell,
            summary: "test".to_owned(),
            severity: crate::model::Severity::Info,
            message: "test".to_owned(),
            parameters: Default::default(),
            threshold_parameter: None,
            selection: StructuralSelection::Pattern(
                "Write-Output $__BADBOX_CAPTURE_VALUE".to_owned(),
            ),
            scope: Scope::NearestAncestor(vec!["function_statement".to_owned()]),
            aggregation: Aggregation::Count,
            threshold: Threshold::GreaterThan(0),
            evidence: Evidence {
                subject: "test".to_owned(),
            },
        };

        assert!(AstGrepBackend::compile(rule).is_err());
    }
}
