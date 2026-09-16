//! Backend- and serialization-independent Badbox rule representation.

use crate::model::{Language, Severity};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Rule {
    pub id: String,
    pub language: Language,
    pub summary: String,
    pub severity: Severity,
    pub message: String,
    pub parameters: BTreeMap<String, ParameterValue>,
    pub threshold_parameter: Option<String>,
    pub selection: StructuralSelection,
    pub scope: Scope,
    pub aggregation: Aggregation,
    pub threshold: Threshold,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterValue {
    Integer(u32),
    Boolean(bool),
    String(String),
}

#[derive(Debug)]
pub enum StructuralSelection {
    Pattern(String),
    Kind(String),
}

#[derive(Debug)]
pub enum Scope {
    NearestAncestor(Vec<String>),
}

impl Scope {
    pub fn nearest_ancestor_kinds(&self) -> &[String] {
        match self {
            Self::NearestAncestor(kinds) => kinds,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Aggregation {
    Count,
}

#[derive(Clone, Copy, Debug)]
pub enum Threshold {
    GreaterThan(u32),
}

impl Threshold {
    pub fn greater_than(self) -> u32 {
        match self {
            Self::GreaterThan(value) => value,
        }
    }
}

#[derive(Debug)]
pub struct Evidence {
    pub subject: String,
}
