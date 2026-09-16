//! Language-independent grouping and threshold evaluation over plain source facts.

use crate::{
    model::{CompactFinding, OwnedMatch},
    rule_ir::{Aggregation, Rule},
};
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub struct Evaluation {
    pub findings: Vec<CompactFinding>,
    pub finding_count: usize,
    pub aggregation: Duration,
    pub output_build: Duration,
}

pub struct EvaluationOptions {
    pub file_id: u32,
    pub rule_id: u32,
    pub threshold: u32,
    pub max_findings: usize,
    pub profile: bool,
}

pub fn evaluate(
    rule: &Rule,
    matches: &[OwnedMatch],
    options: EvaluationOptions,
) -> Result<Evaluation> {
    match rule.aggregation {
        Aggregation::Count => count(matches, options),
    }
}

fn count(matches: &[OwnedMatch], options: EvaluationOptions) -> Result<Evaluation> {
    let started = Instant::now();
    let mut groups: HashMap<u64, Vec<u64>> = HashMap::new();
    for matched in matches {
        groups
            .entry(pack_range(
                matched.owner.range.start,
                matched.owner.range.end,
            )?)
            .or_default()
            .push(pack_range(matched.range.start, matched.range.end)?);
    }
    let mut findings = Vec::with_capacity(groups.len().min(options.max_findings));
    let mut finding_count = 0;
    let mut output_build = Duration::ZERO;
    for (owner, mut ranges) in groups {
        ranges.sort_unstable();
        ranges.dedup();
        let observed = ranges.len();
        if observed <= options.threshold as usize {
            continue;
        }
        finding_count += 1;
        if findings.len() >= options.max_findings {
            continue;
        }
        let output_started = options.profile.then(Instant::now);
        let (owner_start, owner_end) = unpack_range(owner);
        findings.push(CompactFinding {
            file_id: options.file_id,
            rule_id: options.rule_id,
            owner_start,
            owner_end,
            observed: u32::try_from(observed).context("observed count exceeds u32")?,
        });
        if let Some(output_started) = output_started {
            output_build += output_started.elapsed();
        }
    }
    let total = if options.profile {
        started.elapsed()
    } else {
        Duration::ZERO
    };
    Ok(Evaluation {
        findings,
        finding_count,
        aggregation: total.saturating_sub(output_build),
        output_build,
    })
}

fn pack_range(start: usize, end: usize) -> Result<u64> {
    let start = u32::try_from(start).context("range start exceeds u32")?;
    let end = u32::try_from(end).context("range end exceeds u32")?;
    Ok((u64::from(start) << 32) | u64::from(end))
}

fn unpack_range(range: u64) -> (u32, u32) {
    ((range >> 32) as u32, range as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ByteRange, RawOwner};

    fn matched(owner: (usize, usize), selected: (usize, usize)) -> OwnedMatch {
        OwnedMatch {
            owner: RawOwner {
                range: ByteRange {
                    start: owner.0,
                    end: owner.1,
                },
            },
            range: ByteRange {
                start: selected.0,
                end: selected.1,
            },
        }
    }

    #[test]
    fn count_deduplicates_selected_ranges_per_owner() {
        let matches = [
            matched((10, 100), (20, 30)),
            matched((10, 100), (20, 30)),
            matched((10, 100), (40, 50)),
        ];
        let evaluation = count(
            &matches,
            EvaluationOptions {
                file_id: 3,
                rule_id: 7,
                threshold: 1,
                max_findings: usize::MAX,
                profile: false,
            },
        )
        .expect("evaluation succeeds");

        assert_eq!(evaluation.finding_count, 1);
        assert_eq!(evaluation.findings.len(), 1);
        let finding = evaluation.findings[0];
        assert_eq!(finding.file_id, 3);
        assert_eq!(finding.rule_id, 7);
        assert_eq!(finding.owner_start, 10);
        assert_eq!(finding.owner_end, 100);
        assert_eq!(finding.observed, 2);
    }
}
