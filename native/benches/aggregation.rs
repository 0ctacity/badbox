use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    hint::black_box,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Match {
    owner: (u32, u32),
    selected: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Summary {
    findings: usize,
    observed: usize,
}

struct Workload {
    name: &'static str,
    matches: Vec<Match>,
    threshold: usize,
    iterations: usize,
}

type Aggregate = fn(&[Match], usize) -> Summary;

fn btree_set(matches: &[Match], threshold: usize) -> Summary {
    let mut groups: BTreeMap<(u32, u32), BTreeSet<(u32, u32)>> = BTreeMap::new();
    for matched in matches {
        groups
            .entry(matched.owner)
            .or_default()
            .insert(matched.selected);
    }
    summarize(groups.into_values().map(|ranges| ranges.len()), threshold)
}

fn btree_vec(matches: &[Match], threshold: usize) -> Summary {
    let mut groups: BTreeMap<(u32, u32), Vec<(u32, u32)>> = BTreeMap::new();
    for matched in matches {
        groups
            .entry(matched.owner)
            .or_default()
            .push(matched.selected);
    }
    summarize(
        groups.into_values().map(|mut ranges| {
            ranges.sort_unstable();
            ranges.dedup();
            ranges.len()
        }),
        threshold,
    )
}

fn hash_set(matches: &[Match], threshold: usize) -> Summary {
    let mut groups: HashMap<(u32, u32), HashSet<(u32, u32)>> = HashMap::new();
    for matched in matches {
        groups
            .entry(matched.owner)
            .or_default()
            .insert(matched.selected);
    }
    summarize(groups.into_values().map(|ranges| ranges.len()), threshold)
}

fn hash_vec(matches: &[Match], threshold: usize) -> Summary {
    let mut groups: HashMap<(u32, u32), Vec<(u32, u32)>> = HashMap::new();
    for matched in matches {
        groups
            .entry(matched.owner)
            .or_default()
            .push(matched.selected);
    }
    summarize(
        groups.into_values().map(|mut ranges| {
            ranges.sort_unstable();
            ranges.dedup();
            ranges.len()
        }),
        threshold,
    )
}

fn packed_hash_vec(matches: &[Match], threshold: usize) -> Summary {
    let mut groups: HashMap<u64, Vec<u64>> = HashMap::new();
    for matched in matches {
        groups
            .entry(pack(matched.owner))
            .or_default()
            .push(pack(matched.selected));
    }
    summarize(
        groups.into_values().map(|mut ranges| {
            ranges.sort_unstable();
            ranges.dedup();
            ranges.len()
        }),
        threshold,
    )
}

fn pack(range: (u32, u32)) -> u64 {
    (u64::from(range.0) << 32) | u64::from(range.1)
}

fn summarize(counts: impl Iterator<Item = usize>, threshold: usize) -> Summary {
    counts.fold(
        Summary {
            findings: 0,
            observed: 0,
        },
        |mut summary, observed| {
            if observed > threshold {
                summary.findings += 1;
                summary.observed += observed;
            }
            summary
        },
    )
}

fn make_workload(
    name: &'static str,
    owners: u32,
    matches_per_owner: u32,
    duplicate_every: u32,
    iterations: usize,
) -> Workload {
    let mut matches = Vec::with_capacity((owners * matches_per_owner) as usize);
    for owner in 0..owners {
        let owner_start = owner * 4_096;
        for selected in 0..matches_per_owner {
            let selected_start = owner_start + selected * 8;
            let matched = Match {
                owner: (owner_start, owner_start + 4_000),
                selected: (selected_start, selected_start + 4),
            };
            matches.push(matched);
            if duplicate_every != 0 && selected % duplicate_every == 0 {
                matches.push(matched);
            }
        }
    }
    Workload {
        name,
        matches,
        threshold: 1,
        iterations,
    }
}

fn measure(aggregate: Aggregate, workload: &Workload) -> (Duration, Duration, Duration) {
    for _ in 0..10 {
        black_box(aggregate(black_box(&workload.matches), workload.threshold));
    }

    let mut samples = Vec::with_capacity(15);
    for _ in 0..15 {
        let started = Instant::now();
        for _ in 0..workload.iterations {
            black_box(aggregate(black_box(&workload.matches), workload.threshold));
        }
        samples.push(started.elapsed() / workload.iterations as u32);
    }
    samples.sort_unstable();
    (samples[7], samples[0], samples[14])
}

fn main() {
    let workloads = [
        make_workload("tiny", 4, 2, 4, 20_000),
        make_workload("sparse", 4_096, 2, 8, 100),
        make_workload("dense", 512, 128, 8, 10),
    ];
    let variants: [(&str, Aggregate); 5] = [
        ("btree-set-current", btree_set),
        ("btree-vec", btree_vec),
        ("hash-set", hash_set),
        ("hash-vec", hash_vec),
        ("packed-hash-vec", packed_hash_vec),
    ];

    println!("workload\tinput_matches\tvariant\tmedian_ns\tmin_ns\tmax_ns\trelative_to_current");
    for workload in &workloads {
        let expected = btree_set(&workload.matches, workload.threshold);
        for (_, aggregate) in variants {
            assert_eq!(aggregate(&workload.matches, workload.threshold), expected);
        }

        let current = measure(btree_set, workload);
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.3}",
            workload.name,
            workload.matches.len(),
            variants[0].0,
            current.0.as_nanos(),
            current.1.as_nanos(),
            current.2.as_nanos(),
            1.0
        );
        for (name, aggregate) in &variants[1..] {
            let elapsed = measure(*aggregate, workload);
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{:.3}",
                workload.name,
                workload.matches.len(),
                name,
                elapsed.0.as_nanos(),
                elapsed.1.as_nanos(),
                elapsed.2.as_nanos(),
                elapsed.0.as_secs_f64() / current.0.as_secs_f64()
            );
        }
    }
}
