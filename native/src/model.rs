use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const DEFAULT_MAX_FINDINGS: u32 = 10_000;
pub const FINDING_RECORD_WIDTH: u32 = 5;

fn default_max_findings() -> u32 {
    DEFAULT_MAX_FINDINGS
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Bash,
    C,
    Cpp,
    CSharp,
    Css,
    Dart,
    Elixir,
    Go,
    Haskell,
    Hcl,
    Html,
    Java,
    JavaScript,
    Json,
    Kotlin,
    Lua,
    Markdown,
    Nix,
    Php,
    PowerShell,
    Python,
    Ruby,
    Rust,
    Scala,
    Solidity,
    Swift,
    Tsx,
    TypeScript,
    Yaml,
    Zig,
}

impl Language {
    #[cfg(test)]
    pub const ALL: [Self; 30] = [
        Self::Bash,
        Self::C,
        Self::Cpp,
        Self::CSharp,
        Self::Css,
        Self::Dart,
        Self::Elixir,
        Self::Go,
        Self::Haskell,
        Self::Hcl,
        Self::Html,
        Self::Java,
        Self::JavaScript,
        Self::Json,
        Self::Kotlin,
        Self::Lua,
        Self::Markdown,
        Self::Nix,
        Self::Php,
        Self::PowerShell,
        Self::Python,
        Self::Ruby,
        Self::Rust,
        Self::Scala,
        Self::Solidity,
        Self::Swift,
        Self::Tsx,
        Self::TypeScript,
        Self::Yaml,
        Self::Zig,
    ];

    pub fn for_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "bash" | "bats" | "cgi" | "command" | "env" | "fcgi" | "ksh" | "sh" | "tmux"
            | "tool" | "zsh" => Some(Self::Bash),
            "c" | "h" => Some(Self::C),
            "cc" | "hpp" | "cpp" | "c++" | "hh" | "cxx" | "cu" | "ino" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "css" | "scss" => Some(Self::Css),
            "dart" => Some(Self::Dart),
            "ex" | "exs" => Some(Self::Elixir),
            "go" => Some(Self::Go),
            "hs" => Some(Self::Haskell),
            "hcl" | "nomad" | "tf" | "tfvars" | "workflow" => Some(Self::Hcl),
            "html" | "htm" | "xhtml" => Some(Self::Html),
            "java" => Some(Self::Java),
            "cjs" | "js" | "mjs" | "jsx" => Some(Self::JavaScript),
            "json" => Some(Self::Json),
            "kt" | "ktm" | "kts" => Some(Self::Kotlin),
            "lua" => Some(Self::Lua),
            "markdown" | "md" => Some(Self::Markdown),
            "nix" => Some(Self::Nix),
            "php" => Some(Self::Php),
            "ps1" | "psm1" | "psd1" => Some(Self::PowerShell),
            "py" | "py3" | "pyi" | "bzl" | "bazel" => Some(Self::Python),
            "rb" | "rbw" | "gemspec" => Some(Self::Ruby),
            "rs" => Some(Self::Rust),
            "scala" | "sc" | "sbt" => Some(Self::Scala),
            "sol" => Some(Self::Solidity),
            "swift" => Some(Self::Swift),
            "tsx" => Some(Self::Tsx),
            "ts" | "cts" | "mts" => Some(Self::TypeScript),
            "yaml" | "yml" => Some(Self::Yaml),
            "zig" => Some(Self::Zig),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanOptions {
    pub paths: Vec<String>,
    pub rule_paths: Vec<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, ScalarParameter>,
    pub threshold: Option<u32>,
    #[serde(default)]
    pub profile: bool,
    #[serde(default = "default_max_findings")]
    pub max_findings: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ScalarParameter {
    Integer(u32),
    Boolean(bool),
    String(String),
}

#[derive(Clone, Copy, Debug)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct RawOwner {
    pub range: ByteRange,
}

#[derive(Clone, Debug)]
pub struct OwnedMatch {
    pub owner: RawOwner,
    pub range: ByteRange,
}

#[derive(Clone, Copy, Debug)]
pub struct CompactFinding {
    pub file_id: u32,
    pub rule_id: u32,
    pub owner_start: u32,
    pub owner_end: u32,
    pub observed: u32,
}

impl CompactFinding {
    pub fn append_to(self, output: &mut Vec<u32>) {
        output.extend([
            self.file_id,
            self.rule_id,
            self.owner_start,
            self.owner_end,
            self.observed,
        ]);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMetadata {
    pub id: String,
    pub language: Language,
    pub summary: String,
    pub message: String,
    pub severity: Severity,
    pub threshold: u32,
    pub evidence_subject: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanMetadata {
    pub record_width: u32,
    pub languages: Vec<Language>,
    pub files: Vec<String>,
    pub rules: Vec<RuleMetadata>,
    pub checked_files: usize,
    pub finding_count: usize,
    pub truncated: bool,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<PerformanceProfile>,
}

pub struct ScanOutput {
    pub metadata: ScanMetadata,
    pub findings: Vec<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceProfile {
    pub rule_load_ms: f64,
    pub discovery_ms: f64,
    pub read_ms: f64,
    pub parse_ms: f64,
    pub matching_ms: f64,
    pub ownership_ms: f64,
    pub evaluation_ms: f64,
    pub aggregation_ms: f64,
    pub output_build_ms: f64,
    pub result_merge_ms: f64,
    pub serialization_ms: f64,
    pub js_decode_ms: f64,
    pub selector_executions: usize,
    pub rule_evaluations: usize,
    pub rule_cache_hits: usize,
    pub rule_cache_misses: usize,
    pub parse_cache_hits: usize,
    pub parse_cache_misses: usize,
    pub result_cache_hits: usize,
    pub result_cache_misses: usize,
    pub worker_threads: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_every_builtin_language_from_a_representative_extension() {
        let cases = [
            ("input.sh", Language::Bash),
            ("input.c", Language::C),
            ("input.cpp", Language::Cpp),
            ("input.cs", Language::CSharp),
            ("input.css", Language::Css),
            ("input.dart", Language::Dart),
            ("input.ex", Language::Elixir),
            ("input.go", Language::Go),
            ("input.hs", Language::Haskell),
            ("input.tf", Language::Hcl),
            ("input.html", Language::Html),
            ("input.java", Language::Java),
            ("input.js", Language::JavaScript),
            ("input.json", Language::Json),
            ("input.kt", Language::Kotlin),
            ("input.lua", Language::Lua),
            ("input.md", Language::Markdown),
            ("input.nix", Language::Nix),
            ("input.php", Language::Php),
            ("input.ps1", Language::PowerShell),
            ("input.py", Language::Python),
            ("input.rb", Language::Ruby),
            ("input.rs", Language::Rust),
            ("input.scala", Language::Scala),
            ("input.sol", Language::Solidity),
            ("input.swift", Language::Swift),
            ("input.tsx", Language::Tsx),
            ("input.ts", Language::TypeScript),
            ("input.yaml", Language::Yaml),
            ("input.zig", Language::Zig),
        ];

        assert_eq!(cases.len(), Language::ALL.len());
        for (path, expected) in cases {
            assert_eq!(
                Language::for_path(Path::new(path)),
                Some(expected),
                "{path}"
            );
        }
        for path in ["input.psm1", "input.psd1"] {
            assert_eq!(
                Language::for_path(Path::new(path)),
                Some(Language::PowerShell),
                "{path}"
            );
        }
    }
}
