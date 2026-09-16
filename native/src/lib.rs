mod backend;
mod evaluator;
mod frontends;
mod model;
mod rule_ir;

use anyhow::{Context, Result, anyhow, ensure};
use backend::{StructuralBackend, ast_grep::AstGrepBackend};
use ignore::WalkBuilder;
use model::{
    CompactFinding, Diagnostic, FINDING_RECORD_WIDTH, Language, PerformanceProfile, RuleMetadata,
    ScanMetadata, ScanOptions, ScanOutput,
};
use napi::{
    Env, Task,
    bindgen_prelude::{AsyncTask, Uint32Array},
};
use napi_derive::napi;
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const RULE_CACHE_ENTRIES: usize = 256;
const PARSE_CACHE_ENTRIES: usize = 256;
const PARSE_CACHE_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCAN_THREADS: usize = 4;

type CompiledRule = <AstGrepBackend as StructuralBackend>::CompiledRule;
type ExecutionKey = <AstGrepBackend as StructuralBackend>::ExecutionKey;
type ParsedFile = <AstGrepBackend as StructuralBackend>::ParsedFile;

struct RuleCacheEntry {
    path: PathBuf,
    source: String,
    rule: Arc<CompiledRule>,
}

#[derive(Default)]
struct RuleCache {
    entries: VecDeque<RuleCacheEntry>,
}

struct ParseCacheEntry {
    path: PathBuf,
    source: String,
    language: Language,
    parsed: Arc<ParsedFile>,
}

#[derive(Default)]
struct ParseCache {
    entries: VecDeque<ParseCacheEntry>,
    source_bytes: usize,
}

static RULE_CACHE: OnceLock<Mutex<RuleCache>> = OnceLock::new();
static PARSE_CACHE: OnceLock<Mutex<ParseCache>> = OnceLock::new();
static SCAN_POOL: OnceLock<std::result::Result<ThreadPool, String>> = OnceLock::new();

fn scan_pool() -> Result<&'static ThreadPool> {
    SCAN_POOL
        .get_or_init(|| {
            ThreadPoolBuilder::new()
                .num_threads(MAX_SCAN_THREADS)
                .thread_name(|index| format!("badbox-scan-{index}"))
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!("creating scan thread pool: {error}"))
}

#[derive(Default)]
struct CacheCounts {
    hits: usize,
    misses: usize,
}

struct LoadedRules {
    rules: Vec<Arc<CompiledRule>>,
    cache: CacheCounts,
}

struct ExecutionGroup {
    rules: Vec<PlannedRule>,
}

struct PlannedRule {
    index: u32,
    rule: Arc<CompiledRule>,
}

type ExecutionPlans = BTreeMap<Language, Vec<ExecutionGroup>>;

#[derive(Default)]
struct FileTimings {
    read: Duration,
    parse: Duration,
    matching: Duration,
    ownership: Duration,
    evaluation: Duration,
    aggregation: Duration,
    output_build: Duration,
}

#[derive(Default)]
struct FileScan {
    scanned: bool,
    findings: Vec<CompactFinding>,
    finding_count: usize,
    diagnostics: Vec<Diagnostic>,
    timings: FileTimings,
    selector_executions: usize,
    rule_evaluations: usize,
    parse_cache: CacheCounts,
}

fn cached_rule(path: &str) -> Result<(Arc<CompiledRule>, bool)> {
    let canonical = Path::new(path)
        .canonicalize()
        .with_context(|| format!("rule {path}"))?;
    let source = fs::read_to_string(&canonical).with_context(|| format!("rule {path}"))?;
    let cache = RULE_CACHE.get_or_init(|| Mutex::new(RuleCache::default()));
    {
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow!("rule cache lock is poisoned"))?;
        if let Some(index) = cache
            .entries
            .iter()
            .position(|entry| entry.path == canonical && entry.source == source)
        {
            let entry = cache.entries.remove(index).expect("cache index exists");
            let rule = Arc::clone(&entry.rule);
            cache.entries.push_back(entry);
            return Ok((rule, true));
        }
    }

    let rule = frontends::yaml::compile(&source).with_context(|| format!("rule {path}"))?;
    let rule = Arc::new(AstGrepBackend::compile(rule).with_context(|| format!("rule {path}"))?);
    let mut cache = cache
        .lock()
        .map_err(|_| anyhow!("rule cache lock is poisoned"))?;
    cache.entries.retain(|entry| entry.path != canonical);
    while cache.entries.len() >= RULE_CACHE_ENTRIES {
        cache.entries.pop_front();
    }
    cache.entries.push_back(RuleCacheEntry {
        path: canonical,
        source,
        rule: Arc::clone(&rule),
    });
    Ok((rule, false))
}

fn load_rules(paths: &[String]) -> Result<LoadedRules> {
    ensure!(!paths.is_empty(), "rulePaths must not be empty");
    let mut ids = BTreeSet::new();
    let mut rules = Vec::with_capacity(paths.len());
    let mut counts = CacheCounts::default();
    for path in paths {
        let (rule, hit) = cached_rule(path)?;
        ensure!(
            ids.insert(AstGrepBackend::rule(&rule).id.clone()),
            "duplicate rule ID: {}",
            AstGrepBackend::rule(&rule).id
        );
        if hit {
            counts.hits += 1;
        } else {
            counts.misses += 1;
        }
        rules.push(rule);
    }
    Ok(LoadedRules {
        rules,
        cache: counts,
    })
}

fn execution_plans(rules: &[Arc<CompiledRule>]) -> ExecutionPlans {
    let mut plans: BTreeMap<Language, BTreeMap<ExecutionKey, Vec<PlannedRule>>> = BTreeMap::new();
    for (index, rule) in rules.iter().enumerate() {
        plans
            .entry(AstGrepBackend::rule(rule).language)
            .or_default()
            .entry(AstGrepBackend::execution_key(rule).clone())
            .or_default()
            .push(PlannedRule {
                index: u32::try_from(index).expect("rule count is limited by u32 input"),
                rule: Arc::clone(rule),
            });
    }
    plans
        .into_iter()
        .map(|(language, groups)| {
            (
                language,
                groups
                    .into_values()
                    .map(|rules| ExecutionGroup { rules })
                    .collect(),
            )
        })
        .collect()
}

fn discover(paths: &[String]) -> Result<BTreeSet<PathBuf>> {
    ensure!(!paths.is_empty(), "paths must not be empty");
    let mut files = BTreeSet::new();
    for path in paths {
        ensure!(!path.trim().is_empty(), "scan path must not be empty");
        let root = Path::new(path)
            .canonicalize()
            .with_context(|| format!("scan path {path}"))?;
        let walker = WalkBuilder::new(&root)
            .require_git(false)
            .follow_links(false)
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry.file_type().is_some_and(|kind| kind.is_dir())
                    || !matches!(
                        entry.file_name().to_str(),
                        Some("node_modules" | "target" | "vendor" | "dist" | "zig-out")
                    )
            })
            .build();
        for entry in walker {
            let entry = entry.with_context(|| format!("walking {}", root.display()))?;
            if let Some(error) = entry.error() {
                anyhow::bail!("walking {}: {error}", entry.path().display());
            }
            if entry.file_type().is_some_and(|kind| kind.is_file())
                && Language::for_path(entry.path()).is_some()
            {
                files.insert(entry.path().canonicalize()?);
            }
        }
    }
    Ok(files)
}

fn parsed_file(path: &Path, source: &str, language: Language) -> Result<(Arc<ParsedFile>, bool)> {
    let cache = PARSE_CACHE.get_or_init(|| Mutex::new(ParseCache::default()));
    {
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow!("parse cache lock is poisoned"))?;
        if let Some(index) = cache.entries.iter().position(|entry| {
            entry.path == path && entry.language == language && entry.source == source
        }) {
            let entry = cache.entries.remove(index).expect("cache index exists");
            let parsed = Arc::clone(&entry.parsed);
            cache.entries.push_back(entry);
            return Ok((parsed, true));
        }
    }

    let parsed = Arc::new(AstGrepBackend::parse(source, language)?);
    if source.len() <= PARSE_CACHE_SOURCE_BYTES {
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow!("parse cache lock is poisoned"))?;
        let mut retained = VecDeque::with_capacity(cache.entries.len());
        while let Some(entry) = cache.entries.pop_front() {
            if entry.path == path {
                cache.source_bytes = cache.source_bytes.saturating_sub(entry.source.len());
            } else {
                retained.push_back(entry);
            }
        }
        cache.entries = retained;
        while cache.entries.len() >= PARSE_CACHE_ENTRIES
            || cache.source_bytes + source.len() > PARSE_CACHE_SOURCE_BYTES
        {
            let Some(entry) = cache.entries.pop_front() else {
                break;
            };
            cache.source_bytes = cache.source_bytes.saturating_sub(entry.source.len());
        }
        cache.source_bytes += source.len();
        cache.entries.push_back(ParseCacheEntry {
            path: path.to_owned(),
            source: source.to_owned(),
            language,
            parsed: Arc::clone(&parsed),
        });
    }
    Ok((parsed, false))
}

fn scan_file(
    file: &Path,
    file_id: u32,
    language: Language,
    plan: &[ExecutionGroup],
    threshold: Option<u32>,
    max_findings: usize,
    profile: bool,
) -> Result<FileScan> {
    let file_name = file.to_str().context("scan path is not valid UTF-8")?;
    let read_started = Instant::now();
    let source = fs::read_to_string(file).with_context(|| format!("reading {file_name}"))?;
    let read = if profile {
        read_started.elapsed()
    } else {
        Duration::default()
    };
    let parse_started = Instant::now();
    let parsed = parsed_file(file, &source, language);
    let parse = if profile {
        parse_started.elapsed()
    } else {
        Duration::default()
    };
    let (parsed, cache_hit) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(FileScan {
                diagnostics: vec![Diagnostic {
                    file: file_name.to_owned(),
                    message: error.to_string(),
                }],
                timings: FileTimings {
                    read,
                    parse,
                    ..FileTimings::default()
                },
                parse_cache: CacheCounts { hits: 0, misses: 1 },
                ..FileScan::default()
            });
        }
    };
    let representatives = plan
        .iter()
        .map(|group| group.rules[0].rule.as_ref())
        .collect::<Vec<_>>();
    let selection = AstGrepBackend::select_many(&parsed, &representatives, profile);
    let mut findings = Vec::new();
    let mut evaluation = Duration::ZERO;
    let mut aggregation = Duration::ZERO;
    let mut output_build = Duration::ZERO;
    let mut finding_count = 0;
    let mut rule_evaluations = 0;
    for (group, matches) in plan.iter().zip(&selection.matches) {
        for planned in &group.rules {
            let rule = AstGrepBackend::rule(&planned.rule);
            let started = Instant::now();
            let remaining = max_findings.saturating_sub(findings.len());
            let result = evaluator::evaluate(
                rule,
                matches,
                evaluator::EvaluationOptions {
                    file_id,
                    rule_id: planned.index,
                    threshold: threshold.unwrap_or(rule.threshold.greater_than()),
                    max_findings: remaining,
                    profile,
                },
            )?;
            finding_count += result.finding_count;
            findings.extend(result.findings);
            aggregation += result.aggregation;
            output_build += result.output_build;
            if profile {
                evaluation += started.elapsed();
            }
            rule_evaluations += 1;
        }
    }
    Ok(FileScan {
        scanned: true,
        findings,
        finding_count,
        timings: FileTimings {
            read,
            parse,
            matching: selection.timings.matching,
            ownership: selection.timings.ownership,
            evaluation,
            aggregation,
            output_build,
        },
        selector_executions: plan.len(),
        rule_evaluations,
        parse_cache: if cache_hit {
            CacheCounts { hits: 1, misses: 0 }
        } else {
            CacheCounts { hits: 0, misses: 1 }
        },
        ..FileScan::default()
    })
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn scan(options: ScanOptions) -> Result<ScanOutput> {
    let rule_started = Instant::now();
    let loaded = load_rules(&options.rule_paths)?;
    let rule_load = if options.profile {
        rule_started.elapsed()
    } else {
        Duration::default()
    };
    let rules = loaded
        .rules
        .iter()
        .map(|compiled| {
            let rule = AstGrepBackend::rule(compiled);
            RuleMetadata {
                id: rule.id.clone(),
                language: rule.language,
                summary: rule.summary.clone(),
                severity: rule.severity,
                threshold: options.threshold.unwrap_or(rule.threshold.greater_than()),
                evidence_subject: rule.evidence.subject.clone(),
            }
        })
        .collect();
    let plans = execution_plans(&loaded.rules);
    let discovery_started = Instant::now();
    let files = discover(&options.paths)?;
    let discovery = if options.profile {
        discovery_started.elapsed()
    } else {
        Duration::default()
    };
    let languages = files
        .iter()
        .filter_map(|file| Language::for_path(file))
        .collect::<BTreeSet<_>>();
    let file_list = files
        .into_iter()
        .filter(|file| {
            Language::for_path(file).is_some_and(|language| plans.contains_key(&language))
        })
        .collect::<Vec<_>>();
    let relevant_files = file_list.len();
    ensure!(
        file_list.len() <= u32::MAX as usize,
        "scan contains more than u32::MAX relevant files"
    );
    let file_names = file_list
        .iter()
        .map(|path| {
            path.to_str()
                .context("scan path is not valid UTF-8")
                .map(str::to_owned)
        })
        .collect::<Result<Vec<_>>>()?;
    let pool = scan_pool()?;
    let mut performance = PerformanceProfile {
        rule_load_ms: milliseconds(rule_load),
        discovery_ms: milliseconds(discovery),
        rule_cache_hits: loaded.cache.hits,
        rule_cache_misses: loaded.cache.misses,
        worker_threads: pool.current_num_threads().min(relevant_files.max(1)),
        ..PerformanceProfile::default()
    };
    let mut metadata = ScanMetadata {
        record_width: FINDING_RECORD_WIDTH,
        languages: languages.into_iter().collect(),
        files: file_names,
        rules,
        scanned_files: 0,
        finding_count: 0,
        truncated: false,
        diagnostics: Vec::new(),
        performance: None,
    };
    let mut findings = Vec::new();
    for (chunk_index, chunk) in file_list.chunks(MAX_SCAN_THREADS).enumerate() {
        let first_file_id = chunk_index * MAX_SCAN_THREADS;
        let scans = pool.install(|| {
            chunk
                .par_iter()
                .enumerate()
                .map(|(index, file)| {
                    let language = Language::for_path(file).expect("relevant file has a language");
                    let plan = plans
                        .get(&language)
                        .expect("relevant file has an execution plan");
                    scan_file(
                        file,
                        u32::try_from(first_file_id + index)
                            .expect("file count was checked against u32::MAX"),
                        language,
                        plan,
                        options.threshold,
                        options.max_findings as usize,
                        options.profile,
                    )
                })
                .collect::<Result<Vec<_>>>()
        })?;
        let merge_started = options.profile.then(Instant::now);
        for mut file in scans {
            metadata.scanned_files += usize::from(file.scanned);
            metadata.finding_count += file.finding_count;
            let remaining = (options.max_findings as usize).saturating_sub(findings.len());
            findings.extend(file.findings.drain(..remaining.min(file.findings.len())));
            metadata.diagnostics.extend(file.diagnostics);
            performance.read_ms += milliseconds(file.timings.read);
            performance.parse_ms += milliseconds(file.timings.parse);
            performance.matching_ms += milliseconds(file.timings.matching);
            performance.ownership_ms += milliseconds(file.timings.ownership);
            performance.evaluation_ms += milliseconds(file.timings.evaluation);
            performance.aggregation_ms += milliseconds(file.timings.aggregation);
            performance.output_build_ms += milliseconds(file.timings.output_build);
            performance.selector_executions += file.selector_executions;
            performance.rule_evaluations += file.rule_evaluations;
            performance.parse_cache_hits += file.parse_cache.hits;
            performance.parse_cache_misses += file.parse_cache.misses;
        }
        if let Some(merge_started) = merge_started {
            performance.result_merge_ms += milliseconds(merge_started.elapsed());
        }
    }
    metadata.truncated = metadata.finding_count > findings.len();
    findings.sort_by_key(|finding| (finding.file_id, finding.owner_start, finding.rule_id));
    metadata
        .diagnostics
        .sort_by(|a, b| (&a.file, &a.message).cmp(&(&b.file, &b.message)));
    if options.profile {
        metadata.performance = Some(performance);
    }
    let mut records = Vec::with_capacity(findings.len() * FINDING_RECORD_WIDTH as usize);
    for finding in findings {
        finding.append_to(&mut records);
    }
    Ok(ScanOutput {
        metadata,
        findings: records,
    })
}

pub struct InspectTask(String);

pub struct InspectOutput {
    metadata: String,
    findings: Vec<u32>,
}

#[napi(object)]
pub struct NativeScanResult {
    pub metadata: String,
    pub findings: Uint32Array,
}

impl Task for InspectTask {
    type Output = InspectOutput;
    type JsValue = NativeScanResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let run = || -> Result<InspectOutput> {
            let options: ScanOptions =
                serde_json::from_str(&self.0).context("invalid scan options")?;
            let profile = options.profile;
            let mut result = scan(options)?;
            if profile {
                let started = Instant::now();
                let _ = serde_json::to_string(&result.metadata)?;
                if let Some(performance) = &mut result.metadata.performance {
                    performance.serialization_ms = milliseconds(started.elapsed());
                }
            }
            Ok(InspectOutput {
                metadata: serde_json::to_string(&result.metadata)?,
                findings: result.findings,
            })
        };
        run().map_err(|error| napi::Error::from_reason(format!("{error:#}")))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(NativeScanResult {
            metadata: output.metadata,
            findings: output.findings.into(),
        })
    }
}

/// Run all file I/O, matching, ownership and aggregation on native workers.
#[napi]
pub fn inspect(request: String) -> AsyncTask<InspectTask> {
    AsyncTask::new(InspectTask(request))
}
