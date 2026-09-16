use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_MAX_FINDINGS: u32 = 10_000;
pub const FINDING_RECORD_WIDTH: u32 = 5;

fn default_max_findings() -> u32 {
    DEFAULT_MAX_FINDINGS
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Go,
    Rust,
}

impl Language {
    pub fn for_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "rs" => Some(Self::Rust),
            "go" => Some(Self::Go),
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
    pub threshold: Option<u32>,
    #[serde(default)]
    pub profile: bool,
    #[serde(default = "default_max_findings")]
    pub max_findings: u32,
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
    pub scanned_files: usize,
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
    pub worker_threads: usize,
}
