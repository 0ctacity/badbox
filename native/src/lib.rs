mod backend;
mod evaluator;
mod frontends;
mod model;
mod rule_ir;

use anyhow::{Context, Result, anyhow, bail, ensure};
use backend::{StructuralBackend, ast_grep::AstGrepBackend};
use ignore::WalkBuilder;
use model::{
    CompactFinding, Diagnostic, FINDING_RECORD_WIDTH, Language, PerformanceProfile, RuleMetadata,
    ScalarParameter, ScanMetadata, ScanOptions, ScanOutput,
};
use napi::{
    Env, Task,
    bindgen_prelude::{AsyncTask, Uint32Array},
};
use napi_derive::napi;
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use rule_ir::ParameterValue;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs,
    mem::size_of,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const RULE_CACHE_ENTRIES: usize = 256;
const PARSE_CACHE_ENTRIES: usize = 256;
const PARSE_CACHE_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const RESULT_CACHE_ENTRIES: usize = 16 * 1024;
const RESULT_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SCAN_THREADS: usize = 4;

type Fingerprint = [u8; 32];

type CompiledRule = <AstGrepBackend as StructuralBackend>::CompiledRule;
type ExecutionKey = <AstGrepBackend as StructuralBackend>::ExecutionKey;
type ParsedFile = <AstGrepBackend as StructuralBackend>::ParsedFile;

struct RuleCacheEntry {
    path: PathBuf,
    source: String,
    fingerprint: Fingerprint,
    rules: Vec<Arc<CompiledRule>>,
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct ResultRevision {
    source: Fingerprint,
    plan: Fingerprint,
    language: Language,
    max_findings: usize,
}

#[derive(Clone)]
struct CachedFileScan {
    scanned: bool,
    findings: Vec<CompactFinding>,
    finding_count: usize,
    diagnostics: Vec<Diagnostic>,
}

impl CachedFileScan {
    fn from_file_scan(scan: &FileScan) -> Self {
        Self {
            scanned: scan.scanned,
            findings: scan.findings.clone(),
            finding_count: scan.finding_count,
            diagnostics: scan.diagnostics.clone(),
        }
    }

    fn estimated_bytes(&self, path: &Path) -> usize {
        size_of::<ResultCacheEntry>()
            + path.as_os_str().as_encoded_bytes().len()
            + self.findings.len() * size_of::<CompactFinding>()
            + self
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.file.len() + diagnostic.message.len())
                .sum::<usize>()
    }

    fn into_file_scan(mut self, file_id: u32, read: Duration) -> FileScan {
        for finding in &mut self.findings {
            finding.file_id = file_id;
        }
        FileScan {
            scanned: self.scanned,
            findings: self.findings,
            finding_count: self.finding_count,
            diagnostics: self.diagnostics,
            timings: FileTimings {
                read,
                ..FileTimings::default()
            },
            result_cache: CacheCounts { hits: 1, misses: 0 },
            ..FileScan::default()
        }
    }
}

struct ResultCacheEntry {
    revision: ResultRevision,
    result: CachedFileScan,
    estimated_bytes: usize,
    last_used: u64,
}

struct ResultCacheCandidate {
    path: PathBuf,
    revision: ResultRevision,
    result: CachedFileScan,
    estimated_bytes: usize,
}

#[derive(Default)]
struct ResultCache {
    entries: HashMap<PathBuf, ResultCacheEntry>,
    estimated_bytes: usize,
    clock: u64,
}

impl ResultCache {
    fn get(&mut self, path: &Path, revision: ResultRevision) -> Option<CachedFileScan> {
        let entry = self.entries.get_mut(path)?;
        if entry.revision != revision {
            return None;
        }
        self.clock = self.clock.wrapping_add(1);
        entry.last_used = self.clock;
        Some(entry.result.clone())
    }

    fn insert_batch(&mut self, candidates: Vec<ResultCacheCandidate>) {
        for candidate in candidates {
            self.clock = self.clock.wrapping_add(1);
            if let Some(previous) = self.entries.remove(&candidate.path) {
                self.estimated_bytes = self
                    .estimated_bytes
                    .saturating_sub(previous.estimated_bytes);
            }
            self.estimated_bytes += candidate.estimated_bytes;
            self.entries.insert(
                candidate.path,
                ResultCacheEntry {
                    revision: candidate.revision,
                    result: candidate.result,
                    estimated_bytes: candidate.estimated_bytes,
                    last_used: self.clock,
                },
            );
        }
        self.trim();
    }

    fn trim(&mut self) {
        if self.entries.len() <= RESULT_CACHE_ENTRIES && self.estimated_bytes <= RESULT_CACHE_BYTES
        {
            return;
        }
        let mut oldest = self
            .entries
            .iter()
            .map(|(path, entry)| (entry.last_used, path.clone()))
            .collect::<Vec<_>>();
        oldest.sort_by_key(|(last_used, _)| *last_used);
        for (_, path) in oldest {
            if self.entries.len() <= RESULT_CACHE_ENTRIES
                && self.estimated_bytes <= RESULT_CACHE_BYTES
            {
                break;
            }
            if let Some(entry) = self.entries.remove(&path) {
                self.estimated_bytes = self.estimated_bytes.saturating_sub(entry.estimated_bytes);
            }
        }
    }
}

static RULE_CACHE: OnceLock<Mutex<RuleCache>> = OnceLock::new();
static PARSE_CACHE: OnceLock<Mutex<ParseCache>> = OnceLock::new();
static RESULT_CACHE: OnceLock<Mutex<ResultCache>> = OnceLock::new();
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
    rules: Vec<LoadedRule>,
    cache: CacheCounts,
}

struct LoadedRule {
    rule: Arc<CompiledRule>,
    fingerprint: Fingerprint,
}

struct ResolvedRule {
    rule: Arc<CompiledRule>,
    fingerprint: Fingerprint,
    threshold: u32,
}

struct ExecutionGroup {
    rules: Vec<PlannedRule>,
}

struct LanguagePlan {
    groups: Vec<ExecutionGroup>,
    fingerprint: Fingerprint,
}

struct PlannedRule {
    index: u32,
    rule: Arc<CompiledRule>,
    threshold: u32,
}

type ExecutionPlans = BTreeMap<Language, LanguagePlan>;

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

#[derive(Clone, Copy)]
struct FileScanSettings {
    plan_fingerprint: Fingerprint,
    max_findings: usize,
    profile: bool,
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
    result_cache: CacheCounts,
    result_cache_candidate: Option<ResultCacheCandidate>,
}

fn cached_rules(path: &Path) -> Result<(Vec<Arc<CompiledRule>>, Fingerprint, bool)> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("rule {}", path.display()))?;
    let source =
        fs::read_to_string(&canonical).with_context(|| format!("rule {}", path.display()))?;
    let fingerprint = *blake3::hash(source.as_bytes()).as_bytes();
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
            let rules = entry.rules.iter().map(Arc::clone).collect();
            let fingerprint = entry.fingerprint;
            cache.entries.push_back(entry);
            return Ok((rules, fingerprint, true));
        }
    }

    let extension = canonical.extension().and_then(|value| value.to_str());
    let rules = match extension {
        Some("badbox") => frontends::dsl::compile(&source),
        Some("yaml" | "yml") => frontends::yaml::compile(&source).map(|rule| vec![rule]),
        _ => bail!("unsupported rule file {}", canonical.display()),
    }
    .with_context(|| format!("rule {}", path.display()))?
    .into_iter()
    .map(|rule| {
        AstGrepBackend::compile(rule)
            .map(Arc::new)
            .with_context(|| format!("rule {}", path.display()))
    })
    .collect::<Result<Vec<_>>>()?;
    ensure!(
        !rules.is_empty(),
        "rule file {} does not contain any rules",
        path.display()
    );
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
        fingerprint,
        rules: rules.iter().map(Arc::clone).collect(),
    });
    Ok((rules, fingerprint, false))
}

fn load_rules(paths: &[String]) -> Result<LoadedRules> {
    ensure!(!paths.is_empty(), "rulePaths must not be empty");
    let paths = expand_rule_paths(paths)?;
    let mut ids = BTreeSet::new();
    let mut rules = Vec::new();
    let mut counts = CacheCounts::default();
    for path in paths {
        let (compiled, fingerprint, hit) = cached_rules(&path)?;
        for rule in compiled {
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
            rules.push(LoadedRule { rule, fingerprint });
        }
    }
    Ok(LoadedRules {
        rules,
        cache: counts,
    })
}

fn expand_rule_paths(paths: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        let path = Path::new(path);
        ensure!(path.exists(), "rule {path:?} does not exist");
        if path.is_file() {
            files.push(path.to_path_buf());
            continue;
        }
        ensure!(path.is_dir(), "rule {path:?} is not a file or directory");
        let mut pack_files = BTreeSet::new();
        for entry in WalkBuilder::new(path).hidden(false).build() {
            let entry = entry.with_context(|| format!("rule pack {}", path.display()))?;
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            if matches!(
                entry.path().extension().and_then(|value| value.to_str()),
                Some("badbox" | "yaml" | "yml")
            ) {
                pack_files.insert(entry.into_path());
            }
        }
        files.extend(pack_files);
    }
    ensure!(
        !files.is_empty(),
        "rulePaths did not contain any rule files"
    );
    Ok(files)
}

fn resolve_rules(
    rules: &[LoadedRule],
    overrides: &BTreeMap<String, ScalarParameter>,
    threshold_override: Option<u32>,
) -> Result<Vec<ResolvedRule>> {
    let mut by_id = BTreeMap::new();
    let mut parameters = rules
        .iter()
        .enumerate()
        .map(|(index, loaded)| {
            by_id.insert(AstGrepBackend::rule(&loaded.rule).id.as_str(), index);
            AstGrepBackend::rule(&loaded.rule).parameters.clone()
        })
        .collect::<Vec<_>>();
    for (qualified, value) in overrides {
        let Some((rule_id, name)) = qualified.rsplit_once('.') else {
            bail!("invalid parameter {qualified}: expected namespace/rule.parameter");
        };
        let Some(&index) = by_id.get(rule_id) else {
            bail!("unknown parameter {qualified}");
        };
        let Some(expected) = parameters[index].get(name) else {
            bail!("unknown parameter {qualified}");
        };
        let value = match (expected, value) {
            (ParameterValue::Integer(_), ScalarParameter::Integer(value)) => {
                ParameterValue::Integer(*value)
            }
            (ParameterValue::Boolean(_), ScalarParameter::Boolean(value)) => {
                ParameterValue::Boolean(*value)
            }
            (ParameterValue::String(_), ScalarParameter::String(value)) => {
                ParameterValue::String(value.clone())
            }
            (ParameterValue::Integer(_), _) => bail!("parameter {qualified} must be an integer"),
            (ParameterValue::Boolean(_), _) => bail!("parameter {qualified} must be a boolean"),
            (ParameterValue::String(_), _) => bail!("parameter {qualified} must be a string"),
        };
        parameters[index].insert(name.to_owned(), value);
    }

    rules
        .iter()
        .zip(parameters)
        .map(|(loaded, parameters)| {
            let rule = AstGrepBackend::rule(&loaded.rule);
            let threshold = if let Some(threshold) = threshold_override {
                threshold
            } else if let Some(name) = &rule.threshold_parameter {
                match parameters.get(name) {
                    Some(ParameterValue::Integer(value)) => *value,
                    _ => bail!("rule {} has invalid count parameter {name}", rule.id),
                }
            } else {
                rule.threshold.greater_than()
            };
            let mut fingerprint = blake3::Hasher::new();
            fingerprint.update(&loaded.fingerprint);
            fingerprint.update(&threshold.to_le_bytes());
            for (name, value) in parameters {
                fingerprint.update(name.as_bytes());
                match value {
                    ParameterValue::Integer(value) => {
                        fingerprint.update(&[0]);
                        fingerprint.update(&value.to_le_bytes());
                    }
                    ParameterValue::Boolean(value) => {
                        fingerprint.update(&[1, u8::from(value)]);
                    }
                    ParameterValue::String(value) => {
                        fingerprint.update(&[2]);
                        fingerprint.update(value.as_bytes());
                    }
                }
            }
            Ok(ResolvedRule {
                rule: Arc::clone(&loaded.rule),
                fingerprint: *fingerprint.finalize().as_bytes(),
                threshold,
            })
        })
        .collect()
}

fn execution_plans(rules: &[ResolvedRule]) -> ExecutionPlans {
    let mut plans: BTreeMap<Language, BTreeMap<ExecutionKey, Vec<PlannedRule>>> = BTreeMap::new();
    let mut fingerprints: BTreeMap<Language, blake3::Hasher> = BTreeMap::new();
    for (index, loaded) in rules.iter().enumerate() {
        let rule = &loaded.rule;
        let language = AstGrepBackend::rule(rule).language;
        let fingerprint = fingerprints.entry(language).or_default();
        fingerprint.update(&(index as u64).to_le_bytes());
        fingerprint.update(&loaded.fingerprint);
        plans
            .entry(language)
            .or_default()
            .entry(AstGrepBackend::execution_key(rule).clone())
            .or_default()
            .push(PlannedRule {
                index: u32::try_from(index).expect("rule count is limited by u32 input"),
                rule: Arc::clone(rule),
                threshold: loaded.threshold,
            });
    }
    plans
        .into_iter()
        .map(|(language, groups)| {
            (
                language,
                LanguagePlan {
                    groups: groups
                        .into_values()
                        .map(|rules| ExecutionGroup { rules })
                        .collect(),
                    fingerprint: *fingerprints
                        .remove(&language)
                        .expect("language plan has a fingerprint")
                        .finalize()
                        .as_bytes(),
                },
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

fn cached_file_result(path: &Path, revision: ResultRevision) -> Result<Option<CachedFileScan>> {
    let cache = RESULT_CACHE.get_or_init(|| Mutex::new(ResultCache::default()));
    let mut cache = cache
        .lock()
        .map_err(|_| anyhow!("result cache lock is poisoned"))?;
    Ok(cache.get(path, revision))
}

fn cache_candidate(path: &Path, revision: ResultRevision, scan: &FileScan) -> ResultCacheCandidate {
    let result = CachedFileScan::from_file_scan(scan);
    ResultCacheCandidate {
        path: path.to_owned(),
        revision,
        estimated_bytes: result.estimated_bytes(path),
        result,
    }
}

fn commit_result_cache(candidates: Vec<ResultCacheCandidate>) -> Result<()> {
    let cache = RESULT_CACHE.get_or_init(|| Mutex::new(ResultCache::default()));
    let mut cache = cache
        .lock()
        .map_err(|_| anyhow!("result cache lock is poisoned"))?;
    cache.insert_batch(candidates);
    Ok(())
}

fn scan_file(
    file: &Path,
    file_id: u32,
    language: Language,
    plan: &[ExecutionGroup],
    settings: FileScanSettings,
) -> Result<FileScan> {
    let file_name = file.to_str().context("scan path is not valid UTF-8")?;
    let read_started = Instant::now();
    let source = fs::read_to_string(file).with_context(|| format!("reading {file_name}"))?;
    let read = if settings.profile {
        read_started.elapsed()
    } else {
        Duration::default()
    };
    let revision = ResultRevision {
        source: *blake3::hash(source.as_bytes()).as_bytes(),
        plan: settings.plan_fingerprint,
        language,
        max_findings: settings.max_findings,
    };
    if let Some(cached) = cached_file_result(file, revision)? {
        return Ok(cached.into_file_scan(file_id, read));
    }
    let parse_started = Instant::now();
    let parsed = parsed_file(file, &source, language);
    let parse = if settings.profile {
        parse_started.elapsed()
    } else {
        Duration::default()
    };
    let (parsed, cache_hit) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            let mut scan = FileScan {
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
                result_cache: CacheCounts { hits: 0, misses: 1 },
                ..FileScan::default()
            };
            scan.result_cache_candidate = Some(cache_candidate(file, revision, &scan));
            return Ok(scan);
        }
    };
    let representatives = plan
        .iter()
        .map(|group| group.rules[0].rule.as_ref())
        .collect::<Vec<_>>();
    let selection = AstGrepBackend::select_many(&parsed, &representatives, settings.profile);
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
            let remaining = settings.max_findings.saturating_sub(findings.len());
            let result = evaluator::evaluate(
                rule,
                matches,
                evaluator::EvaluationOptions {
                    file_id,
                    rule_id: planned.index,
                    threshold: planned.threshold,
                    max_findings: remaining,
                    profile: settings.profile,
                },
            )?;
            finding_count += result.finding_count;
            findings.extend(result.findings);
            aggregation += result.aggregation;
            output_build += result.output_build;
            if settings.profile {
                evaluation += started.elapsed();
            }
            rule_evaluations += 1;
        }
    }
    let mut scan = FileScan {
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
        result_cache: CacheCounts { hits: 0, misses: 1 },
        ..FileScan::default()
    };
    scan.result_cache_candidate = Some(cache_candidate(file, revision, &scan));
    Ok(scan)
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn scan(options: ScanOptions) -> Result<ScanOutput> {
    let rule_started = Instant::now();
    let loaded = load_rules(&options.rule_paths)?;
    let resolved = resolve_rules(&loaded.rules, &options.parameters, options.threshold)?;
    let rule_load = if options.profile {
        rule_started.elapsed()
    } else {
        Duration::default()
    };
    let rules = resolved
        .iter()
        .map(|resolved| {
            let rule = AstGrepBackend::rule(&resolved.rule);
            RuleMetadata {
                id: rule.id.clone(),
                language: rule.language,
                summary: rule.summary.clone(),
                severity: rule.severity,
                message: rule.message.clone(),
                threshold: resolved.threshold,
                evidence_subject: rule.evidence.subject.clone(),
            }
        })
        .collect();
    let plans = execution_plans(&resolved);
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
    let mut result_cache_candidates = Vec::new();
    let mut result_cache_candidate_bytes = 0;
    for (chunk_index, chunk) in file_list.chunks(MAX_SCAN_THREADS).enumerate() {
        let first_file_id = chunk_index * MAX_SCAN_THREADS;
        let scans = pool.install(|| {
            chunk
                .par_iter()
                .enumerate()
                .map(|(index, file)| {
                    let language = Language::for_path(file).expect("relevant file has a language");
                    let language_plan = plans
                        .get(&language)
                        .expect("relevant file has an execution plan");
                    scan_file(
                        file,
                        u32::try_from(first_file_id + index)
                            .expect("file count was checked against u32::MAX"),
                        language,
                        &language_plan.groups,
                        FileScanSettings {
                            plan_fingerprint: language_plan.fingerprint,
                            max_findings: options.max_findings as usize,
                            profile: options.profile,
                        },
                    )
                })
                .collect::<Result<Vec<_>>>()
        })?;
        let merge_started = options.profile.then(Instant::now);
        for mut file in scans {
            if let Some(candidate) = file.result_cache_candidate.take()
                && result_cache_candidates.len() < RESULT_CACHE_ENTRIES
                && result_cache_candidate_bytes + candidate.estimated_bytes <= RESULT_CACHE_BYTES
            {
                result_cache_candidate_bytes += candidate.estimated_bytes;
                result_cache_candidates.push(candidate);
            }
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
            performance.result_cache_hits += file.result_cache.hits;
            performance.result_cache_misses += file.result_cache.misses;
        }
        if let Some(merge_started) = merge_started {
            performance.result_merge_ms += milliseconds(merge_started.elapsed());
        }
    }
    commit_result_cache(result_cache_candidates)?;
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
