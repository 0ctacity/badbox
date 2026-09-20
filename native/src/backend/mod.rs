//! Structural matching boundary used by the Badbox execution engine.

use crate::{model::Language, rule_ir::Rule};
use anyhow::Result;
use std::time::Duration;

pub mod ast_grep;

#[derive(Default)]
pub struct SelectionTimings {
    pub matching: Duration,
    pub ownership: Duration,
}

pub struct Selection {
    pub matches: Vec<Vec<crate::model::OwnedMatch>>,
    pub timings: SelectionTimings,
    pub selector_executions: usize,
}

pub trait StructuralBackend {
    type CompiledRule;
    type ExecutionKey: Clone + Ord;
    type ParsedFile;

    fn compile(rule: Rule) -> Result<Self::CompiledRule>;
    fn rule(compiled: &Self::CompiledRule) -> &Rule;
    fn execution_key(compiled: &Self::CompiledRule) -> &Self::ExecutionKey;
    fn parse(source: &str, language: Language) -> Result<Self::ParsedFile>;
    fn select_many(
        file: &Self::ParsedFile,
        rules: &[&Self::CompiledRule],
        profile: bool,
    ) -> Selection;
}
