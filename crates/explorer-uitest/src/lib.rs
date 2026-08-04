#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::format_push_string,
    clippy::struct_excessive_bools,
    reason = "report schemas favor direct formatting and CLI flags are independent switches"
)]

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    env,
    ffi::OsString,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result, anyhow, bail};
use globset::Glob;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const KNOWN_SUITES: [&str; 5] = ["quick", "full", "interop", "visual", "soak"];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub cases: Vec<TestCase>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestCase {
    pub id: String,
    pub description: String,
    pub suites: Vec<String>,
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub prerequisites: Vec<Prerequisite>,
    #[serde(default)]
    pub exclusive_resources: Vec<String>,
    #[serde(default)]
    pub covers: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub required_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prerequisite {
    Windows,
    InteractiveDesktop,
    Command { name: String },
    Path { path: String },
    Environment { name: String, value: Option<String> },
    MonitorCount { minimum: usize },
    PythonModule { name: String },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Requirement {
    pub id: String,
    pub change: String,
    pub capability: String,
    pub title: String,
    pub source: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Coverage {
    pub by_requirement: BTreeMap<String, Vec<String>>,
    pub by_case: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseStatus {
    Pass,
    Fail,
    Skip,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub description: String,
    pub status: CaseStatus,
    pub started_utc: String,
    pub duration_ms: u128,
    pub command: String,
    pub exit_code: Option<i32>,
    pub terminal_reason: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub evidence_directory: String,
    pub artifacts: Vec<String>,
    pub requirements: Vec<String>,
    pub rerun_command: String,
    pub process: ProcessReport,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProcessReport {
    pub launched_pid: Option<u32>,
    pub before_count: usize,
    pub after_count: usize,
    pub detected_residual_pids: Vec<u32>,
    pub residual_pids: Vec<u32>,
    pub cleanup_attempted: bool,
}

#[derive(Debug, Serialize)]
struct HostMetadata {
    windows_build: Option<String>,
    rustc: Option<String>,
    cargo: Option<String>,
    architecture: String,
}

#[derive(Debug, Serialize)]
struct RunReport {
    schema_version: u32,
    run_id: String,
    started_utc: String,
    workspace: String,
    git_revision: String,
    git_dirty: bool,
    host: HostMetadata,
    selected_suites: Vec<String>,
    selected_cases: Vec<String>,
    counts: BTreeMap<String, usize>,
    results: Vec<CaseResult>,
}

#[derive(Debug, Serialize)]
struct CoverageReport {
    schema_version: u32,
    discovered: usize,
    covered: usize,
    uncovered: Vec<String>,
    requirements: Vec<RequirementCoverage>,
}

#[derive(Debug, Serialize)]
struct RequirementCoverage {
    id: String,
    title: String,
    source: String,
    cases: Vec<String>,
    executed_results: BTreeMap<String, String>,
    best_result: String,
}

#[derive(Debug, Default)]
struct Cli {
    manifest: Option<PathBuf>,
    suites: BTreeSet<String>,
    cases: BTreeSet<String>,
    output: Option<PathBuf>,
    list: bool,
    validate_only: bool,
    fail_fast: bool,
    fail_on_skip: bool,
}

/// Runs the manifest-driven test runner using the current process arguments.
///
/// # Errors
///
/// Returns an error for invalid CLI/manifest/coverage, report I/O, or a failed selected case.
pub fn run_from_env() -> Result<()> {
    let cli = parse_cli(env::args_os().skip(1))?;
    let current_directory = env::current_dir()?;
    let workspace = discover_workspace_root(&current_directory)?;
    let manifest_path = cli
        .manifest
        .clone()
        .unwrap_or_else(|| workspace.join("uitest/manifest.json"));
    let manifest = read_manifest(&manifest_path)?;
    validate_manifest(&manifest)?;
    let requirements = scan_requirements(&workspace)?;
    // An explicitly selected case is a local/example run, not a repository
    // release gate. Validate its selectors without requiring unrelated OpenSpec
    // requirements to have UITEST coverage before the case can even start.
    let require_complete_coverage = !cli.list && cli.cases.is_empty();
    let coverage = build_coverage_with_gate(&manifest, &requirements, require_complete_coverage)?;
    let selected = select_cases(&manifest, &cli)?;

    if cli.list || cli.validate_only {
        println!(
            "manifest={} cases={} selected={} requirements={} covered={}",
            manifest_path.display(),
            manifest.cases.len(),
            selected.len(),
            requirements.len(),
            coverage.by_requirement.len()
        );
        for case in &selected {
            println!(
                "{:<34} suites={:<22} timeout={:>4}s requirements={:>3}  {}",
                case.id,
                case.suites.join(","),
                case.timeout_seconds,
                coverage.by_case.get(&case.id).map_or(0, Vec::len),
                case.description
            );
        }
        if cli.validate_only {
            println!(
                "OpenSpec coverage gate passed: {} requirements",
                requirements.len()
            );
        }
        return Ok(());
    }

    execute_run(
        &workspace,
        &manifest_path,
        &requirements,
        &coverage,
        &selected,
        &cli,
    )
}

fn parse_cli<I>(arguments: I) -> Result<Cli>
where
    I: IntoIterator<Item = OsString>,
{
    let mut cli = Cli::default();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let argument = argument.to_string_lossy();
        match argument.as_ref() {
            "--manifest" => cli.manifest = Some(next_path(&mut arguments, "--manifest")?),
            "--suite" => {
                let value = next_string(&mut arguments, "--suite")?;
                cli.suites.extend(split_values(&value));
            }
            "--case" => {
                let value = next_string(&mut arguments, "--case")?;
                cli.cases.extend(split_values(&value));
            }
            "--output" => cli.output = Some(next_path(&mut arguments, "--output")?),
            "--list" => cli.list = true,
            "--validate-only" => cli.validate_only = true,
            "--fail-fast" => cli.fail_fast = true,
            "--fail-on-skip" => cli.fail_on_skip = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            unknown => bail!("unknown argument {unknown}; use --help"),
        }
    }
    Ok(cli)
}

fn next_string<I>(arguments: &mut I, flag: &str) -> Result<String>
where
    I: Iterator<Item = OsString>,
{
    arguments
        .next()
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn next_path<I>(arguments: &mut I, flag: &str) -> Result<PathBuf>
where
    I: Iterator<Item = OsString>,
{
    Ok(PathBuf::from(next_string(arguments, flag)?))
}

fn split_values(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
}

fn print_help() {
    println!(
        "explorer-uitest [--suite quick,full,interop,visual,soak] [--case ID] \
         [--list|--validate-only] [--output PATH] [--fail-fast] [--fail-on-skip]"
    );
}

fn discover_workspace_root(start: &Path) -> Result<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join("Cargo.toml").is_file() && candidate.join("openspec/changes").is_dir() {
            return Ok(candidate.to_path_buf());
        }
    }
    bail!("unable to find workspace containing Cargo.toml and openspec/changes")
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    let bytes = fs::read(path).with_context(|| format!("read manifest {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse manifest {}", path.display()))
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.schema_version != 1 {
        bail!(
            "unsupported manifest schema_version {}",
            manifest.schema_version
        );
    }
    if manifest.cases.is_empty() {
        bail!("manifest contains no cases");
    }
    let mut ids = HashSet::new();
    for case in &manifest.cases {
        if case.id.is_empty()
            || !case.id.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
        {
            bail!("case id must use lowercase ASCII kebab-case: {}", case.id);
        }
        if !ids.insert(&case.id) {
            bail!("duplicate case id {}", case.id);
        }
        if case.description.trim().is_empty() || case.program.trim().is_empty() {
            bail!("case {} requires description and program", case.id);
        }
        if case.timeout_seconds == 0 {
            bail!("case {} timeout_seconds must be positive", case.id);
        }
        if case.suites.is_empty() {
            bail!("case {} has no suite", case.id);
        }
        for suite in &case.suites {
            if !KNOWN_SUITES.contains(&suite.as_str()) {
                bail!("case {} uses unknown suite {suite}", case.id);
            }
        }
        for resource in &case.exclusive_resources {
            if !matches!(
                resource.as_str(),
                "gui" | "cursor" | "clipboard" | "ole" | "explorer"
            ) {
                bail!(
                    "case {} uses unknown exclusive resource {resource}",
                    case.id
                );
            }
        }
        for (name, value) in &case.environment {
            if name.is_empty() || name.contains('=') || value.contains('\0') {
                bail!("case {} has invalid environment entry", case.id);
            }
        }
        for artifact in &case.required_artifacts {
            let path = Path::new(artifact);
            if path.is_absolute() || artifact.contains("..") {
                bail!("case {} artifact must stay relative: {artifact}", case.id);
            }
        }
        for prerequisite in &case.prerequisites {
            if let Prerequisite::MonitorCount { minimum } = prerequisite
                && *minimum == 0
            {
                bail!("case {} monitor_count minimum must be positive", case.id);
            }
            if let Prerequisite::PythonModule { name } = prerequisite
                && (name.is_empty()
                    || !name.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "._".contains(character)
                    }))
            {
                bail!("case {} has invalid python module prerequisite", case.id);
            }
        }
    }
    Ok(())
}

/// Scans active `OpenSpec` changes and returns stable requirement identities.
///
/// # Errors
///
/// Returns an error when specs cannot be read or contain invalid/duplicate identities.
pub fn scan_requirements(workspace: &Path) -> Result<Vec<Requirement>> {
    let changes = workspace.join("openspec/changes");
    let mut requirements = Vec::new();
    for change in sorted_directories(&changes)? {
        let change_name = file_name(&change)?;
        if matches!(change_name.as_str(), "archive" | "archived") || change_name.starts_with('.') {
            continue;
        }
        let tasks = change.join("tasks.md");
        if tasks.is_file() {
            let task_text = fs::read_to_string(&tasks)
                .with_context(|| format!("read OpenSpec tasks {}", tasks.display()))?;
            let implementation_started = task_text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("- [x]") || line.starts_with("- [X]")
            });
            if !implementation_started {
                // A freshly proposed change is not part of the implemented regression surface
                // yet. Its own first implementation task registers truthful coverage before
                // the gate starts enforcing the change's requirements.
                continue;
            }
        }
        let specs = change.join("specs");
        if !specs.is_dir() {
            continue;
        }
        for capability in sorted_directories(&specs)? {
            let capability_name = file_name(&capability)?;
            let spec = capability.join("spec.md");
            if !spec.is_file() {
                continue;
            }
            let text = fs::read_to_string(&spec)
                .with_context(|| format!("read OpenSpec {}", spec.display()))?;
            for (index, line) in text.lines().enumerate() {
                if let Some(title) = line.strip_prefix("### Requirement:") {
                    let title = title.trim();
                    if title.is_empty() {
                        bail!(
                            "empty requirement title at {}:{}",
                            spec.display(),
                            index + 1
                        );
                    }
                    requirements.push(Requirement {
                        id: format!(
                            "{change_name}/{capability_name}/{}",
                            normalize_identity(title)
                        ),
                        change: change_name.clone(),
                        capability: capability_name.clone(),
                        title: title.to_owned(),
                        source: spec
                            .strip_prefix(workspace)
                            .unwrap_or(&spec)
                            .to_string_lossy()
                            .replace('\\', "/"),
                        line: index + 1,
                    });
                }
            }
        }
    }
    requirements.sort_by(|left, right| left.id.cmp(&right.id));
    let mut seen = HashSet::new();
    for requirement in &requirements {
        if !seen.insert(&requirement.id) {
            bail!(
                "duplicate normalized requirement identity {}",
                requirement.id
            );
        }
    }
    Ok(requirements)
}

fn sorted_directories(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut directories = fs::read_dir(path)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| entry.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    Ok(directories)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("path has no file name: {}", path.display()))
}

fn normalize_identity(title: &str) -> String {
    let mut result = String::new();
    let mut pending_separator = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_separator && !result.is_empty() {
                result.push('-');
            }
            result.push(character);
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    result
}

/// Expands manifest selectors into a bidirectional requirement coverage map.
///
/// # Errors
///
/// Returns an error when a selector matches nothing or a requirement remains uncovered.
pub fn build_coverage(manifest: &Manifest, requirements: &[Requirement]) -> Result<Coverage> {
    build_coverage_with_gate(manifest, requirements, true)
}

fn build_coverage_with_gate(
    manifest: &Manifest,
    requirements: &[Requirement],
    require_complete_coverage: bool,
) -> Result<Coverage> {
    let requirement_ids = requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect::<Vec<_>>();
    let mut by_requirement = BTreeMap::<String, Vec<String>>::new();
    let mut by_case = BTreeMap::<String, Vec<String>>::new();
    for case in &manifest.cases {
        let mut matched = BTreeSet::new();
        for selector in &case.covers {
            let selector_matches = if let Some(prefix) = selector.strip_suffix('*') {
                requirement_ids
                    .iter()
                    .copied()
                    .filter(|id| id.starts_with(prefix))
                    .collect::<Vec<_>>()
            } else {
                requirement_ids
                    .iter()
                    .copied()
                    .filter(|id| *id == selector.as_str())
                    .collect::<Vec<_>>()
            };
            if selector_matches.is_empty() {
                bail!(
                    "case {} coverage selector matched nothing: {selector}",
                    case.id
                );
            }
            matched.extend(selector_matches.into_iter().map(ToOwned::to_owned));
        }
        let matched = matched.into_iter().collect::<Vec<_>>();
        for requirement in &matched {
            by_requirement
                .entry(requirement.clone())
                .or_default()
                .push(case.id.clone());
        }
        by_case.insert(case.id.clone(), matched);
    }
    let uncovered = requirement_ids
        .iter()
        .filter(|id| !by_requirement.contains_key(**id))
        .copied()
        .collect::<Vec<_>>();
    if require_complete_coverage && !uncovered.is_empty() {
        bail!(
            "OpenSpec coverage gate failed; {} uncovered requirement(s):\n{}",
            uncovered.len(),
            uncovered.join("\n")
        );
    }
    Ok(Coverage {
        by_requirement,
        by_case,
    })
}

fn select_cases<'a>(manifest: &'a Manifest, cli: &Cli) -> Result<Vec<&'a TestCase>> {
    if !cli.cases.is_empty() {
        let selected = manifest
            .cases
            .iter()
            .filter(|case| cli.cases.contains(&case.id))
            .collect::<Vec<_>>();
        let found = selected
            .iter()
            .map(|case| case.id.as_str())
            .collect::<HashSet<_>>();
        let missing = cli
            .cases
            .iter()
            .filter(|id| !found.contains(id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bail!("unknown case(s): {}", missing.join(", "));
        }
        return Ok(selected);
    }
    let requested = if cli.suites.is_empty() {
        BTreeSet::from(["quick".to_owned()])
    } else {
        cli.suites.clone()
    };
    for suite in &requested {
        if !KNOWN_SUITES.contains(&suite.as_str()) {
            bail!("unknown suite {suite}");
        }
    }
    let mut expanded = requested.clone();
    if requested.iter().any(|suite| suite != "quick") {
        expanded.insert("quick".to_owned());
    }
    let selected = manifest
        .cases
        .iter()
        .filter(|case| case.suites.iter().any(|suite| expanded.contains(suite)))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        bail!("suite selection contains no cases");
    }
    Ok(selected)
}

fn execute_run(
    workspace: &Path,
    manifest_path: &Path,
    requirements: &[Requirement],
    coverage: &Coverage,
    selected: &[&TestCase],
    cli: &Cli,
) -> Result<()> {
    let run_id = format!("{}-{}", unix_seconds(), Uuid::new_v4().simple());
    let output = cli
        .output
        .clone()
        .unwrap_or_else(|| workspace.join("target/uitest-runs").join(&run_id));
    let output = if output.is_absolute() {
        output
    } else {
        workspace.join(output)
    };
    if output.is_dir() && fs::read_dir(&output)?.next().is_some() {
        bail!(
            "UITest output directory must be empty to prevent log corruption: {}",
            output.display()
        );
    }
    let fixtures = output.join("fixtures");
    let evidence = output.join("evidence");
    let logs = output.join("logs");
    fs::create_dir_all(&fixtures)?;
    fs::create_dir_all(&evidence)?;
    fs::create_dir_all(&logs)?;
    let started_utc = utc_stamp();
    let manifest_display = manifest_path
        .strip_prefix(workspace)
        .unwrap_or(manifest_path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut results = Vec::new();
    for case in selected {
        let result = execute_case(
            workspace,
            &output,
            &fixtures,
            &evidence,
            &logs,
            &manifest_display,
            coverage,
            case,
        )?;
        println!(
            "[{:<7}] {:<34} {:>7} ms  {}",
            status_name(result.status),
            result.id,
            result.duration_ms,
            result.terminal_reason.as_deref().unwrap_or("")
        );
        let failed = case_is_failure(&result, cli.fail_on_skip);
        results.push(result);
        if cli.fail_fast && failed {
            break;
        }
    }
    let selected_suites = if cli.suites.is_empty() {
        vec!["quick".to_owned()]
    } else {
        cli.suites.iter().cloned().collect()
    };
    let report = RunReport {
        schema_version: 2,
        run_id,
        started_utc,
        workspace: workspace.display().to_string(),
        git_revision: command_text(workspace, "git", &["rev-parse", "--short=12", "HEAD"])
            .unwrap_or_else(|| "unknown".to_owned()),
        git_dirty: command_text(workspace, "git", &["status", "--porcelain"])
            .is_some_and(|output| !output.trim().is_empty()),
        host: host_metadata(workspace),
        selected_suites,
        selected_cases: selected.iter().map(|case| case.id.clone()).collect(),
        counts: result_counts(&results),
        results,
    };
    write_reports(&output, requirements, coverage, &report)?;
    println!("UITest report: {}", output.display());
    let failed = report
        .results
        .iter()
        .any(|result| case_is_failure(result, cli.fail_on_skip));
    if failed {
        bail!("one or more UITest cases failed; see {}", output.display());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_case(
    workspace: &Path,
    output: &Path,
    fixtures: &Path,
    evidence_root: &Path,
    logs: &Path,
    manifest_display: &str,
    coverage: &Coverage,
    case: &TestCase,
) -> Result<CaseResult> {
    let started = Instant::now();
    let started_utc = utc_stamp();
    let evidence = evidence_root.join(&case.id);
    fs::create_dir_all(&evidence)?;
    let stdout = logs.join(format!("{}.stdout.log", case.id));
    let stderr = logs.join(format!("{}.stderr.log", case.id));
    let templates = Templates {
        workspace,
        output,
        fixtures,
        evidence: &evidence,
    };
    let program = expand_template(&case.program, &templates);
    let arguments = case
        .arguments
        .iter()
        .map(|argument| expand_template(argument, &templates))
        .collect::<Vec<_>>();
    let command = display_command(&program, &arguments);
    let requirements = coverage.by_case.get(&case.id).cloned().unwrap_or_default();
    let rerun_command = format!(
        "cargo run -p explorer-uitest -- --manifest {} --case {}",
        quote_argument(manifest_display),
        case.id
    );
    if let Some(reason) = missing_prerequisite(case, &templates) {
        return Ok(CaseResult {
            id: case.id.clone(),
            description: case.description.clone(),
            status: CaseStatus::Skip,
            started_utc,
            duration_ms: started.elapsed().as_millis(),
            command,
            exit_code: None,
            terminal_reason: Some(reason),
            stdout: relative_string(output, &stdout),
            stderr: relative_string(output, &stderr),
            evidence_directory: relative_string(output, &evidence),
            artifacts: Vec::new(),
            requirements,
            rerun_command,
            process: ProcessReport::default(),
        });
    }
    let before = process_census();
    let stdout_file = File::create(&stdout)?;
    let stderr_file = File::create(&stderr)?;
    let mut child = Command::new(&program)
        .args(&arguments)
        .current_dir(workspace)
        .env("EXPLORER_UITEST_RUN_ROOT", output)
        .env("EXPLORER_UITEST_FIXTURE_ROOT", fixtures)
        .env("EXPLORER_UITEST_EVIDENCE_DIR", &evidence)
        .envs(
            case.environment
                .iter()
                .map(|(name, value)| (name, expand_template(value, &templates))),
        )
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn();
    let launched_pid = child.as_ref().ok().map(std::process::Child::id);
    let (status, exit_code, terminal_reason) = match child.as_mut() {
        Ok(child) => match wait_with_timeout(child, Duration::from_secs(case.timeout_seconds))? {
            WaitOutcome::Exited(exit) => {
                let success = exit.success();
                (
                    if success {
                        CaseStatus::Pass
                    } else {
                        CaseStatus::Fail
                    },
                    exit.code(),
                    (!success).then(|| format!("process exited with {exit}")),
                )
            }
            WaitOutcome::TimedOut => (
                CaseStatus::Timeout,
                None,
                Some(format!("timeout after {} seconds", case.timeout_seconds)),
            ),
        },
        Err(error) => (
            CaseStatus::Error,
            None,
            Some(format!("failed to start {program}: {error}")),
        ),
    };
    let mut artifacts = Vec::new();
    let mut status = status;
    let mut terminal_reason = terminal_reason;
    for artifact in &case.required_artifacts {
        let matches = collect_artifacts(&evidence, artifact)?;
        if matches.is_empty() && status == CaseStatus::Pass {
            status = CaseStatus::Fail;
            terminal_reason = Some(format!("required artifact missing: {artifact}"));
        } else {
            artifacts.extend(matches.iter().map(|path| relative_string(output, path)));
        }
    }
    artifacts.sort();
    artifacts.dedup();
    let mut after = process_census();
    let detected_residual_pids =
        launched_pid.map_or_else(Vec::new, |pid| descendants_of(pid, &after));
    let cleanup_attempted = !detected_residual_pids.is_empty();
    for pid in &detected_residual_pids {
        terminate_process_tree(*pid);
    }
    if cleanup_attempted {
        thread::sleep(Duration::from_millis(100));
        after = process_census();
    }
    let residual_pids = launched_pid.map_or_else(Vec::new, |pid| descendants_of(pid, &after));
    Ok(CaseResult {
        id: case.id.clone(),
        description: case.description.clone(),
        status,
        started_utc,
        duration_ms: started.elapsed().as_millis(),
        command,
        exit_code,
        terminal_reason,
        stdout: relative_string(output, &stdout),
        stderr: relative_string(output, &stderr),
        evidence_directory: relative_string(output, &evidence),
        artifacts,
        requirements,
        rerun_command,
        process: ProcessReport {
            launched_pid,
            before_count: before.len(),
            after_count: after.len(),
            detected_residual_pids,
            residual_pids,
            cleanup_attempted,
        },
    })
}

fn collect_artifacts(root: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let matcher = Glob::new(&pattern.replace('\\', "/"))
        .with_context(|| format!("invalid required artifact glob {pattern}"))?
        .compile_matcher();
    let mut matched_paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(root)
                && matcher.is_match(relative.to_string_lossy().replace('\\', "/"))
            {
                matched_paths.push(path);
            }
        }
    }
    matched_paths.sort();
    Ok(matched_paths)
}

fn process_census() -> BTreeMap<u32, u32> {
    #[cfg(windows)]
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct ProcessRow {
            process_id: u32,
            parent_process_id: u32,
        }
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                "@(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId) | ConvertTo-Json -Compress",
            ])
            .output();
        if let Ok(output) = output
            && output.status.success()
            && let Ok(rows) = serde_json::from_slice::<Vec<ProcessRow>>(&output.stdout)
        {
            return rows
                .into_iter()
                .map(|row| (row.process_id, row.parent_process_id))
                .collect();
        }
    }
    BTreeMap::new()
}

fn descendants_of(root: u32, census: &BTreeMap<u32, u32>) -> Vec<u32> {
    let mut parents = BTreeSet::from([root]);
    let mut descendants = BTreeSet::new();
    loop {
        let discovered = census
            .iter()
            .filter(|(pid, parent)| parents.contains(parent) && **pid != root)
            .map(|(pid, _)| *pid)
            .filter(|pid| descendants.insert(*pid))
            .collect::<Vec<_>>();
        if discovered.is_empty() {
            break;
        }
        parents.extend(discovered);
    }
    descendants.into_iter().collect()
}

fn host_metadata(workspace: &Path) -> HostMetadata {
    let windows_build = if cfg!(windows) {
        command_text(
            workspace,
            "powershell.exe",
            &[
                "-NoProfile",
                "-Command",
                "[Environment]::OSVersion.Version.Build",
            ],
        )
    } else {
        None
    };
    HostMetadata {
        windows_build,
        rustc: command_text(workspace, "rustc", &["--version"]),
        cargo: command_text(workspace, "cargo", &["--version"]),
        architecture: env::consts::ARCH.to_owned(),
    }
}

struct Templates<'a> {
    workspace: &'a Path,
    output: &'a Path,
    fixtures: &'a Path,
    evidence: &'a Path,
}

fn expand_template(value: &str, templates: &Templates<'_>) -> String {
    value
        .replace(
            "{workspace_root}",
            &templates.workspace.display().to_string(),
        )
        .replace("{output_root}", &templates.output.display().to_string())
        .replace("{fixture_root}", &templates.fixtures.display().to_string())
        .replace("{evidence_dir}", &templates.evidence.display().to_string())
}

fn missing_prerequisite(case: &TestCase, templates: &Templates<'_>) -> Option<String> {
    for prerequisite in &case.prerequisites {
        let missing = match prerequisite {
            Prerequisite::Windows => (!cfg!(windows)).then(|| "requires Windows".to_owned()),
            Prerequisite::InteractiveDesktop => {
                let session = env::var("SESSIONNAME").unwrap_or_default();
                session
                    .eq_ignore_ascii_case("services")
                    .then(|| "requires an interactive desktop session".to_owned())
            }
            Prerequisite::Command { name } => {
                (!command_exists(name)).then(|| format!("missing command {name}"))
            }
            Prerequisite::Path { path } => {
                let path = expand_template(path, templates);
                (!Path::new(&path).exists()).then(|| format!("missing path {path}"))
            }
            Prerequisite::Environment { name, value } => match env::var(name) {
                Ok(actual) if value.as_ref().is_none_or(|expected| expected == &actual) => None,
                _ => Some(match value {
                    Some(value) => format!("requires environment {name}={value}"),
                    None => format!("requires environment {name}"),
                }),
            },
            Prerequisite::MonitorCount { minimum } => monitor_count().and_then(|actual| {
                (actual < *minimum)
                    .then(|| format!("requires at least {minimum} monitors; detected {actual}"))
            }),
            Prerequisite::PythonModule { name } => {
                (!python_module_exists(name)).then(|| format!("missing Python module {name}"))
            }
        };
        if missing.is_some() {
            return missing;
        }
    }
    None
}

fn monitor_count() -> Option<usize> {
    if !cfg!(windows) {
        return Some(0);
    }
    command_text(
        Path::new("."),
        "powershell.exe",
        &[
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Screen]::AllScreens.Count",
        ],
    )
    .and_then(|value| value.parse().ok())
}

fn python_module_exists(name: &str) -> bool {
    Command::new("python.exe")
        .args(["-c", &format!("import {name}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn command_exists(name: &str) -> bool {
    Command::new("where.exe")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

enum WaitOutcome {
    Exited(ExitStatus),
    TimedOut,
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> Result<WaitOutcome> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(WaitOutcome::Exited(status));
        }
        if started.elapsed() >= timeout {
            terminate_process_tree(child.id());
            let _ = child.wait();
            return Ok(WaitOutcome::TimedOut);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn terminate_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    let _ = pid;
}

fn write_reports(
    output: &Path,
    requirements: &[Requirement],
    coverage: &Coverage,
    report: &RunReport,
) -> Result<()> {
    atomic_json(&output.join("report.json"), report)?;
    let result_map = report
        .results
        .iter()
        .map(|result| (result.id.as_str(), status_name(result.status)))
        .collect::<HashMap<_, _>>();
    let requirement_reports = requirements
        .iter()
        .map(|requirement| {
            let cases = coverage
                .by_requirement
                .get(&requirement.id)
                .cloned()
                .unwrap_or_default();
            let executed_results = cases
                .iter()
                .filter_map(|case| {
                    result_map
                        .get(case.as_str())
                        .map(|status| (case.clone(), (*status).to_owned()))
                })
                .collect::<BTreeMap<_, _>>();
            let best_result =
                best_requirement_result(executed_results.values().map(String::as_str));
            RequirementCoverage {
                id: requirement.id.clone(),
                title: requirement.title.clone(),
                source: format!("{}:{}", requirement.source, requirement.line),
                cases,
                executed_results,
                best_result,
            }
        })
        .collect::<Vec<_>>();
    let coverage_report = CoverageReport {
        schema_version: 1,
        discovered: requirements.len(),
        covered: coverage.by_requirement.len(),
        uncovered: requirements
            .iter()
            .filter(|requirement| !coverage.by_requirement.contains_key(&requirement.id))
            .map(|requirement| requirement.id.clone())
            .collect(),
        requirements: requirement_reports,
    };
    atomic_json(&output.join("coverage.json"), &coverage_report)?;
    atomic_write(&output.join("junit.xml"), junit_xml(report).as_bytes())?;
    atomic_write(
        &output.join("summary.md"),
        markdown_summary(report).as_bytes(),
    )?;
    Ok(())
}

fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

fn junit_xml(report: &RunReport) -> String {
    let failures = report
        .results
        .iter()
        .filter(|result| {
            matches!(
                result.status,
                CaseStatus::Fail | CaseStatus::Timeout | CaseStatus::Error
            )
        })
        .count();
    let skipped = report
        .results
        .iter()
        .filter(|result| result.status == CaseStatus::Skip)
        .count();
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"explorer-uitest\" tests=\"{}\" failures=\"{failures}\" skipped=\"{skipped}\">\n",
        report.results.len()
    );
    for result in &report.results {
        xml.push_str(&format!(
            "  <testcase name=\"{}\" time=\"{:.3}\">\n",
            xml_escape(&result.id),
            result.duration_ms as f64 / 1000.0
        ));
        match result.status {
            CaseStatus::Skip => xml.push_str(&format!(
                "    <skipped message=\"{}\"/>\n",
                xml_escape(
                    result
                        .terminal_reason
                        .as_deref()
                        .unwrap_or("prerequisite unavailable")
                )
            )),
            CaseStatus::Fail | CaseStatus::Timeout | CaseStatus::Error => xml.push_str(&format!(
                "    <failure type=\"{}\" message=\"{}\"/>\n",
                status_name(result.status),
                xml_escape(result.terminal_reason.as_deref().unwrap_or("failed"))
            )),
            CaseStatus::Pass => {}
        }
        xml.push_str(&format!(
            "    <system-out>{}</system-out>\n    <system-err>{}</system-err>\n  </testcase>\n",
            xml_escape(&result.stdout),
            xml_escape(&result.stderr)
        ));
    }
    xml.push_str("</testsuite>\n");
    xml
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn markdown_summary(report: &RunReport) -> String {
    let mut output = format!(
        "# Explorer UITest report\n\n- Run: `{}`\n- Started UTC: `{}`\n- Git: `{}`{}\n- Cases: {}\n\n| Status | Case | Duration ms | Reason |\n|---|---|---:|---|\n",
        report.run_id,
        report.started_utc,
        report.git_revision,
        if report.git_dirty { " (dirty)" } else { "" },
        report.results.len()
    );
    for result in &report.results {
        output.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            status_name(result.status),
            result.id,
            result.duration_ms,
            result
                .terminal_reason
                .as_deref()
                .unwrap_or("")
                .replace('|', "\\|")
        ));
    }
    output.push_str("\n## Failed or skipped reruns\n\n");
    for result in report
        .results
        .iter()
        .filter(|result| result.status != CaseStatus::Pass)
    {
        output.push_str(&format!("- `{}`: `{}`\n", result.id, result.rerun_command));
    }
    output
}

fn result_counts(results: &[CaseResult]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for result in results {
        *counts
            .entry(status_name(result.status).to_owned())
            .or_insert(0) += 1;
    }
    counts
}

fn best_requirement_result<'a>(results: impl Iterator<Item = &'a str>) -> String {
    let results = results.collect::<HashSet<_>>();
    for status in ["PASS", "FAIL", "TIMEOUT", "ERROR", "SKIP"] {
        if results.contains(status) {
            return status.to_owned();
        }
    }
    "NOT_RUN".to_owned()
}

fn status_name(status: CaseStatus) -> &'static str {
    match status {
        CaseStatus::Pass => "PASS",
        CaseStatus::Fail => "FAIL",
        CaseStatus::Skip => "SKIP",
        CaseStatus::Timeout => "TIMEOUT",
        CaseStatus::Error => "ERROR",
    }
}

fn case_is_failure(result: &CaseResult, fail_on_skip: bool) -> bool {
    matches!(
        result.status,
        CaseStatus::Fail | CaseStatus::Timeout | CaseStatus::Error
    ) || fail_on_skip && result.status == CaseStatus::Skip
        || !result.process.residual_pids.is_empty()
}

fn command_text(workspace: &Path, program: &str, arguments: &[&str]) -> Option<String> {
    Command::new(program)
        .args(arguments)
        .current_dir(workspace)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn display_command(program: &str, arguments: &[String]) -> String {
    std::iter::once(program)
        .chain(arguments.iter().map(String::as_str))
        .map(quote_argument)
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_argument(argument: &str) -> String {
    if argument.is_empty() || argument.chars().any(char::is_whitespace) {
        format!("\"{}\"", argument.replace('"', "\\\""))
    } else {
        argument.to_owned()
    }
}

fn relative_string(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn utc_stamp() -> String {
    format!("unix:{}", unix_seconds())
}

/// Returns whether `target` resolves beneath, and is not equal to, `root`.
///
/// # Errors
///
/// Returns an error if either path cannot be canonicalized.
pub fn is_within_owned_root(root: &Path, target: &Path) -> Result<bool> {
    let root = fs::canonicalize(root)?;
    let target = fs::canonicalize(target)?;
    Ok(target.starts_with(&root) && target != root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct VisualRect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    fn normalized_rect(rect: VisualRect, space: (f64, f64)) -> [f64; 4] {
        [
            rect.x / space.0,
            rect.y / space.1,
            rect.width / space.0,
            rect.height / space.1,
        ]
    }

    fn rect_matches(reference: VisualRect, actual: VisualRect, tolerance: f64) -> bool {
        let fields = [
            (reference.x, actual.x),
            (reference.y, actual.y),
            (reference.width, actual.width),
            (reference.height, actual.height),
        ];
        fields.into_iter().all(|(expected, observed)| {
            let allowed = if expected.abs() < 10.0 {
                1.0
            } else {
                expected.abs() * tolerance
            };
            (expected - observed).abs() <= allowed
        })
    }

    #[test]
    fn visual_comparator_contract() {
        let reference = VisualRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        assert!(rect_matches(
            reference,
            VisualRect {
                width: 109.0,
                ..reference
            },
            0.10
        ));
        assert!(!rect_matches(
            reference,
            VisualRect {
                x: 20.0,
                ..reference
            },
            0.10
        ));
        let small = VisualRect {
            width: 5.0,
            ..reference
        };
        assert!(rect_matches(
            small,
            VisualRect {
                width: 6.0,
                ..small
            },
            0.10
        ));

        let icon_reference = normalized_rect(
            VisualRect {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
            },
            (100.0, 100.0),
        );
        let icon_actual = normalized_rect(
            VisualRect {
                x: 20.0,
                y: 20.0,
                width: 40.0,
                height: 40.0,
            },
            (200.0, 200.0),
        );
        assert!(
            icon_reference
                .iter()
                .zip(icon_actual)
                .all(|(reference, actual)| (reference - actual).abs() <= f64::EPSILON)
        );

        let reference_image = vec![[255_u8; 4]; 100];
        let mut actual_image = reference_image.clone();
        actual_image[0] = [0, 0, 0, 255];
        let changed = reference_image
            .iter()
            .zip(&actual_image)
            .filter(|(left, right)| left != right)
            .count();
        assert_eq!(changed, 1);
        assert_eq!(reference_image.len(), actual_image.len());
    }

    fn manifest(case: TestCase) -> Manifest {
        Manifest {
            schema_version: 1,
            cases: vec![case],
        }
    }

    fn case() -> TestCase {
        TestCase {
            id: "workspace-tests".to_owned(),
            description: "tests".to_owned(),
            suites: vec!["quick".to_owned()],
            program: "cargo".to_owned(),
            arguments: vec!["test".to_owned()],
            timeout_seconds: 60,
            prerequisites: vec![],
            exclusive_resources: vec![],
            covers: vec!["change/capability/*".to_owned()],
            environment: BTreeMap::new(),
            required_artifacts: vec![],
        }
    }

    #[test]
    fn identity_normalization_preserves_unicode_and_collapses_punctuation() {
        assert_eq!(
            normalize_identity("F2 rename SHALL 工作"),
            "f2-rename-shall-工作"
        );
        assert_eq!(normalize_identity("  A / B -- C  "), "a-b-c");
    }

    #[test]
    fn manifest_rejects_duplicate_unknown_suite_and_zero_timeout() {
        let mut duplicate = manifest(case());
        duplicate.cases.push(case());
        assert!(
            validate_manifest(&duplicate)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let mut unknown = manifest(case());
        unknown.cases[0].suites = vec!["nightly".to_owned()];
        assert!(
            validate_manifest(&unknown)
                .unwrap_err()
                .to_string()
                .contains("unknown suite")
        );
        let mut zero = manifest(case());
        zero.cases[0].timeout_seconds = 0;
        assert!(
            validate_manifest(&zero)
                .unwrap_err()
                .to_string()
                .contains("positive")
        );
    }

    #[test]
    fn coverage_expands_prefix_and_rejects_new_uncovered_requirement() {
        let requirements = vec![Requirement {
            id: "change/capability/one".to_owned(),
            change: "change".to_owned(),
            capability: "capability".to_owned(),
            title: "One".to_owned(),
            source: "spec.md".to_owned(),
            line: 1,
        }];
        let coverage = build_coverage(&manifest(case()), &requirements).unwrap();
        assert_eq!(coverage.by_requirement.len(), 1);
        let mut uncovered = requirements.clone();
        uncovered.push(Requirement {
            id: "other/capability/two".to_owned(),
            title: "Two".to_owned(),
            ..requirements[0].clone()
        });
        assert!(build_coverage(&manifest(case()), &uncovered).is_err());
    }

    #[test]
    fn requirement_scan_defers_untouched_proposals_until_implementation_starts() {
        let root = env::temp_dir().join(format!("explorer-uitest-{}", Uuid::new_v4()));
        for (change, tasks) in [
            ("proposal", "- [ ] 1.1 Register coverage\n"),
            (
                "started",
                "- [x] 1.1 Register coverage\n- [ ] 1.2 Implement\n",
            ),
        ] {
            let directory = root.join("openspec/changes").join(change);
            fs::create_dir_all(directory.join("specs/capability")).unwrap();
            fs::write(directory.join("tasks.md"), tasks).unwrap();
            fs::write(
                directory.join("specs/capability/spec.md"),
                "### Requirement: Covered behavior\n",
            )
            .unwrap();
        }

        let requirements = scan_requirements(&root).unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].change, "started");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xml_escaping_and_best_result_are_deterministic() {
        assert_eq!(xml_escape("<&\"'>"), "&lt;&amp;&quot;&apos;&gt;");
        assert_eq!(
            best_requirement_result(["SKIP", "FAIL"].into_iter()),
            "FAIL"
        );
        assert_eq!(best_requirement_result(std::iter::empty()), "NOT_RUN");
    }

    #[test]
    fn explicit_case_selection_does_not_require_suite_membership() {
        let manifest = manifest(case());
        let cli = Cli {
            cases: BTreeSet::from(["workspace-tests".to_owned()]),
            ..Cli::default()
        };
        assert_eq!(select_cases(&manifest, &cli).unwrap().len(), 1);
    }

    #[test]
    fn containment_refuses_root_and_sibling() {
        let base = env::temp_dir().join(format!("explorer-uitest-{}", Uuid::new_v4()));
        let child = base.join("child");
        let sibling = env::temp_dir().join(format!("explorer-uitest-sibling-{}", Uuid::new_v4()));
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        assert!(is_within_owned_root(&base, &child).unwrap());
        assert!(!is_within_owned_root(&base, &base).unwrap());
        assert!(!is_within_owned_root(&base, &sibling).unwrap());
        fs::remove_dir_all(&base).unwrap();
        fs::remove_dir_all(&sibling).unwrap();
    }

    #[test]
    fn malformed_manifest_and_zero_selection_have_path_aware_errors() {
        let root = env::temp_dir().join(format!("explorer-uitest-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("broken.json");
        fs::write(&path, br#"{"schema_version":1,"cases":[}"#).unwrap();
        let error = read_manifest(&path).unwrap_err().to_string();
        assert!(error.contains("parse manifest"));
        assert!(error.contains("broken.json"));

        let visual_only = Manifest {
            schema_version: 1,
            cases: vec![TestCase {
                suites: vec!["visual".to_owned()],
                ..case()
            }],
        };
        let error = select_cases(&visual_only, &Cli::default()).unwrap_err();
        assert!(error.to_string().contains("contains no cases"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prerequisite_reasons_and_fail_on_skip_are_truthful() {
        let root = env::temp_dir().join(format!("explorer-uitest-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let templates = Templates {
            workspace: &root,
            output: &root,
            fixtures: &root,
            evidence: &root,
        };
        let mut missing_path = case();
        missing_path.prerequisites = vec![Prerequisite::Path {
            path: root.join("missing-drive").display().to_string(),
        }];
        assert!(
            missing_prerequisite(&missing_path, &templates)
                .unwrap()
                .contains("missing path")
        );
        let mut missing_command = case();
        missing_command.prerequisites = vec![Prerequisite::Command {
            name: format!("definitely-missing-{}", Uuid::new_v4()),
        }];
        assert!(
            missing_prerequisite(&missing_command, &templates)
                .unwrap()
                .contains("missing command")
        );
        let mut missing_monitors = case();
        missing_monitors.prerequisites = vec![Prerequisite::MonitorCount {
            minimum: usize::MAX,
        }];
        assert!(
            missing_prerequisite(&missing_monitors, &templates)
                .unwrap()
                .contains("requires at least")
        );
        let mut invalid_monitors = manifest(case());
        invalid_monitors.cases[0].prerequisites = vec![Prerequisite::MonitorCount { minimum: 0 }];
        assert!(
            validate_manifest(&invalid_monitors)
                .unwrap_err()
                .to_string()
                .contains("monitor_count minimum")
        );
        let mut missing_python_module = case();
        missing_python_module.prerequisites = vec![Prerequisite::PythonModule {
            name: format!("missing_{}", Uuid::new_v4().simple()),
        }];
        assert!(
            missing_prerequisite(&missing_python_module, &templates)
                .unwrap()
                .contains("missing Python module")
        );
        let skipped = result("skip", CaseStatus::Skip);
        assert!(!case_is_failure(&skipped, false));
        assert!(case_is_failure(&skipped, true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn artifact_globs_are_recursive_sorted_and_require_a_match() {
        let root = env::temp_dir().join(format!("explorer-uitest-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("screenshots/nested")).unwrap();
        fs::write(root.join("report.json"), b"{}").unwrap();
        fs::write(root.join("screenshots/a.png"), b"a").unwrap();
        fs::write(root.join("screenshots/nested/b.png"), b"b").unwrap();
        assert_eq!(collect_artifacts(&root, "report.json").unwrap().len(), 1);
        assert_eq!(
            collect_artifacts(&root, "screenshots/**/*.png")
                .unwrap()
                .len(),
            2
        );
        assert!(collect_artifacts(&root, "*.missing").unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn process_descendants_are_transitive_and_do_not_include_root() {
        let census = BTreeMap::from([(10, 1), (11, 10), (12, 11), (13, 10), (20, 1)]);
        assert_eq!(descendants_of(10, &census), vec![11, 12, 13]);
    }

    fn result(id: &str, status: CaseStatus) -> CaseResult {
        CaseResult {
            id: id.to_owned(),
            description: id.to_owned(),
            status,
            started_utc: "unix:1".to_owned(),
            duration_ms: 10,
            command: "fixture".to_owned(),
            exit_code: (status == CaseStatus::Pass).then_some(0),
            terminal_reason: (status != CaseStatus::Pass).then(|| status_name(status).to_owned()),
            stdout: format!("logs/{id}.stdout.log"),
            stderr: format!("logs/{id}.stderr.log"),
            evidence_directory: format!("evidence/{id}"),
            artifacts: Vec::new(),
            requirements: Vec::new(),
            rerun_command: format!("--case {id}"),
            process: ProcessReport::default(),
        }
    }

    #[test]
    fn mixed_reports_keep_status_identities_and_counts_consistent() {
        let results = vec![
            result("pass", CaseStatus::Pass),
            result("fail", CaseStatus::Fail),
            result("skip", CaseStatus::Skip),
            result("timeout", CaseStatus::Timeout),
        ];
        let report = RunReport {
            schema_version: 2,
            run_id: "fixture".to_owned(),
            started_utc: "unix:1".to_owned(),
            workspace: "fixture".to_owned(),
            git_revision: "fixture".to_owned(),
            git_dirty: false,
            host: HostMetadata {
                windows_build: Some("26200".to_owned()),
                rustc: Some("rustc fixture".to_owned()),
                cargo: Some("cargo fixture".to_owned()),
                architecture: "x86_64".to_owned(),
            },
            selected_suites: vec!["quick".to_owned()],
            selected_cases: results.iter().map(|entry| entry.id.clone()).collect(),
            counts: result_counts(&results),
            results,
        };
        let xml = junit_xml(&report);
        let markdown = markdown_summary(&report);
        assert!(xml.contains("tests=\"4\" failures=\"2\" skipped=\"1\""));
        for id in ["pass", "fail", "skip", "timeout"] {
            assert!(xml.contains(&format!("name=\"{id}\"")));
            assert!(markdown.contains(&format!("`{id}`")));
        }
        assert_eq!(report.counts["PASS"], 1);
        assert_eq!(report.counts["FAIL"], 1);
        assert_eq!(report.counts["SKIP"], 1);
        assert_eq!(report.counts["TIMEOUT"], 1);
    }

    #[test]
    fn host_metadata_contains_tools_but_no_user_or_host_identity() {
        let metadata = host_metadata(Path::new(env!("CARGO_MANIFEST_DIR")));
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("rustc"));
        assert!(json.contains("cargo"));
        assert!(!json.contains("username"));
        assert!(!json.contains("hostname"));
        assert!(!json.contains("home"));
    }

    #[cfg(windows)]
    #[test]
    fn execution_captures_environment_cwd_large_output_failure_and_timeout() {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf();
        let output = env::temp_dir().join(format!("explorer-uitest-{}", Uuid::new_v4()));
        let fixtures = output.join("fixtures");
        let evidence = output.join("evidence");
        let logs = output.join("logs");
        fs::create_dir_all(&fixtures).unwrap();
        fs::create_dir_all(&evidence).unwrap();
        fs::create_dir_all(&logs).unwrap();
        let coverage = Coverage {
            by_requirement: BTreeMap::new(),
            by_case: BTreeMap::new(),
        };

        let mut passing = case();
        passing.id = "execution-pass".to_owned();
        passing.program = "powershell.exe".to_owned();
        passing.arguments = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "$ErrorActionPreference='Stop'; if($env:UITEST_SENTINEL -ne 'present'){exit 9}; (Get-Location).Path | Set-Content -Encoding utf8 (Join-Path $env:EXPLORER_UITEST_EVIDENCE_DIR 'cwd.txt'); 1..5000 | ForEach-Object { 'x' * 100 }; '{\"ok\":true}' | Set-Content -Encoding utf8 (Join-Path $env:EXPLORER_UITEST_EVIDENCE_DIR 'report.json')".to_owned(),
        ];
        passing.environment =
            BTreeMap::from([("UITEST_SENTINEL".to_owned(), "present".to_owned())]);
        passing.required_artifacts = vec!["*.json".to_owned(), "**/*.txt".to_owned()];
        let pass = execute_case(
            &workspace,
            &output,
            &fixtures,
            &evidence,
            &logs,
            "uitest/manifest.json",
            &coverage,
            &passing,
        )
        .unwrap();
        assert_eq!(pass.status, CaseStatus::Pass);
        assert_eq!(pass.artifacts.len(), 2);
        assert!(fs::metadata(output.join(&pass.stdout)).unwrap().len() > 100_000);
        assert!(
            fs::read_to_string(evidence.join("execution-pass/cwd.txt"))
                .unwrap()
                .contains(&workspace.display().to_string())
        );

        let mut failing = passing.clone();
        failing.id = "execution-fail".to_owned();
        failing.arguments = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "exit 7".to_owned(),
        ];
        failing.required_artifacts.clear();
        let fail = execute_case(
            &workspace,
            &output,
            &fixtures,
            &evidence,
            &logs,
            "uitest/manifest.json",
            &coverage,
            &failing,
        )
        .unwrap();
        assert_eq!(fail.status, CaseStatus::Fail);
        assert_eq!(fail.exit_code, Some(7));

        let mut timeout = failing;
        timeout.id = "execution-timeout".to_owned();
        timeout.arguments = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep 30' -WindowStyle Hidden; Start-Sleep 30".to_owned(),
        ];
        timeout.timeout_seconds = 1;
        let timed_out = execute_case(
            &workspace,
            &output,
            &fixtures,
            &evidence,
            &logs,
            "uitest/manifest.json",
            &coverage,
            &timeout,
        )
        .unwrap();
        assert_eq!(timed_out.status, CaseStatus::Timeout);
        assert!(timed_out.terminal_reason.unwrap().contains("timeout"));
        assert!(timed_out.process.launched_pid.is_some());
        fs::remove_dir_all(output).unwrap();
    }
}
