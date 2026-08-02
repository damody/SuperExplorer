//! Bounded, fail-closed resolution of sealed package-validation results.
//!
//! Resolution deliberately accepts [`PackageValidationResultV1`], never raw
//! manifests.  The validation result is created only after the source content,
//! signature, target, and sealed generation have passed the pre-load checks.
//! The returned resolved set is consequently the only set eligible for a later
//! registrar stage.

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};

use crate::{PackageManifestV1, PackageValidationResultV1};

const MAX_RESOLVER_CANDIDATES_V1: usize = 128;
const MAX_RESOLVER_REQUIRED_EDGES_V1: usize = 512;
const MAX_RESOLVER_SEARCH_STATES_V1: usize = 65_536;

/// Stateless resolver for one discovered, sealed package generation.
#[derive(Clone, Copy, Debug, Default)]
pub struct PackageResolverV1;

impl PackageResolverV1 {
    /// Resolves sealed package-validation results into the registration set.
    ///
    /// The search is complete within fixed bounds of 128 candidates, 512 required
    /// edges, and 65,536 search states. It maximizes the number of selected
    /// package IDs, then breaks ties by package ID order and highest `SemVer`
    /// version. Required dependencies must be closed and acyclic. Optional
    /// dependencies never constrain selection or block their owner; a compatible
    /// optional edge is omitted when it would introduce a cycle and is reported
    /// as a diagnostic instead.
    ///
    /// ```compile_fail
    /// use explorer_extension_host::{PackageManifestV1, PackageResolverV1};
    ///
    /// let raw_manifests: Vec<PackageManifestV1> = Vec::new();
    /// let _ = PackageResolverV1::resolve(&raw_manifests);
    /// ```
    ///
    /// ```compile_fail
    /// use explorer_extension_host::PackageValidationResultV1;
    ///
    /// // A caller cannot forge an unsigned, tampered, or wrong-target result:
    /// // the sealed generation and its bound manifest are private to validation.
    /// let _ = PackageValidationResultV1 {
    ///     verified_publisher_id: None,
    ///     manifest_digest: String::new(),
    ///     data_version: 0,
    /// };
    /// ```
    #[must_use]
    pub fn resolve(candidates: &[PackageValidationResultV1]) -> PackageResolutionV1<'_> {
        resolve(candidates)
    }
}

/// Complete bounded search outcome and all excluded package candidates.
#[derive(Clone, Debug)]
pub struct PackageResolutionV1<'validated> {
    resolved_packages: Vec<ResolvedPackageV1<'validated>>,
    blocked_packages: Vec<BlockedPackageV1>,
    diagnostics: Vec<PackageResolutionDiagnosticV1>,
}

impl<'validated> PackageResolutionV1<'validated> {
    /// Returns the complete dependency-closed registration set, sorted by ID.
    #[must_use]
    pub fn resolved_packages(&self) -> &[ResolvedPackageV1<'validated>] {
        &self.resolved_packages
    }

    /// Returns every validation result excluded from registration as a whole.
    #[must_use]
    pub fn blocked_packages(&self) -> &[BlockedPackageV1] {
        &self.blocked_packages
    }

    /// Returns deterministic, structured resolution diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[PackageResolutionDiagnosticV1] {
        &self.diagnostics
    }
}

/// A sealed package result selected atomically for later registration.
#[derive(Clone, Debug)]
pub struct ResolvedPackageV1<'validated> {
    validation_result: &'validated PackageValidationResultV1,
    dependencies: Vec<ResolvedPackageDependencyV1>,
}

impl<'validated> ResolvedPackageV1<'validated> {
    /// Returns the sealed result bound to this registration-eligible package.
    #[must_use]
    pub const fn validation_result(&self) -> &'validated PackageValidationResultV1 {
        self.validation_result
    }

    /// Returns the manifest that was sealed by [`Self::validation_result`].
    #[must_use]
    pub fn manifest(&self) -> &PackageManifestV1 {
        self.validation_result.manifest()
    }

    /// Returns required and cycle-safe optional edges in the selected graph.
    #[must_use]
    pub fn dependencies(&self) -> &[ResolvedPackageDependencyV1] {
        &self.dependencies
    }
}

/// One selected dependency edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPackageDependencyV1 {
    package_id: String,
    package_version: String,
    optional: bool,
}

impl ResolvedPackageDependencyV1 {
    /// Returns the selected dependency package ID.
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the selected dependency package version.
    #[must_use]
    pub fn package_version(&self) -> &str {
        &self.package_version
    }

    /// Returns whether this edge is optional.
    #[must_use]
    pub const fn optional(&self) -> bool {
        self.optional
    }
}

/// A validated package candidate excluded from registration as a whole.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockedPackageV1 {
    package_id: String,
    package_version: String,
}

impl BlockedPackageV1 {
    /// Returns the candidate's package ID.
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the candidate's package version.
    #[must_use]
    pub fn package_version(&self) -> &str {
        &self.package_version
    }
}

/// Machine-readable package-resolution failure or non-blocking condition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageResolutionDiagnosticCodeV1 {
    /// The package's declared version is not valid `SemVer`.
    InvalidPackageVersion,
    /// A dependency requirement is not valid `SemVer` syntax.
    InvalidDependencyRequirement,
    /// More than one validation result declared the same ID and version.
    DuplicatePackageVersion,
    /// No installed candidate exists for a required dependency requirement.
    UnsatisfiedRequiredDependency,
    /// Required dependency candidates existed but no valid whole-package result remained.
    RejectedRequiredDependency,
    /// A required dependency graph candidate contained a cycle.
    DependencyCycle,
    /// The bounded complete search limit was reached; all candidates were rejected.
    SearchLimitExceeded,
    /// An optional dependency was missing or incompatible with the chosen version.
    OptionalDependencyUnavailable,
    /// A compatible optional edge was ignored because it would form a cycle.
    OptionalDependencyCycleIgnored,
    /// Every required edge was satisfied, but global deterministic selection omitted this candidate.
    NotSelectedByGlobalResolution,
    /// A valid alternate candidate lost deterministic version precedence.
    SupersededBySelectedVersion,
}

/// Structured diagnostic suitable for extension state and UI presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageResolutionDiagnosticV1 {
    /// The affected candidate's package ID.
    pub package_id: String,
    /// The affected candidate's package version text.
    pub package_version: String,
    /// The related dependency ID when applicable.
    pub dependency_package_id: Option<String>,
    /// The related manifest requirement when applicable.
    pub version_requirement: Option<String>,
    /// Stable diagnostic code.
    pub code: PackageResolutionDiagnosticCodeV1,
    /// Other related package IDs, such as a cycle's members.
    pub related_package_ids: Vec<String>,
}

struct Candidate<'validated> {
    validation_result: &'validated PackageValidationResultV1,
    version: Option<Version>,
    requirements: Vec<Option<VersionReq>>,
}

impl Candidate<'_> {
    fn manifest(&self) -> &PackageManifestV1 {
        self.validation_result.manifest()
    }
}

struct Search<'candidate> {
    candidates: &'candidate [Candidate<'candidate>],
    groups: Vec<(String, Vec<usize>)>,
    states_visited: usize,
    limit_exceeded: bool,
    optimal_solution_found: bool,
    best: Option<Vec<Option<usize>>>,
    cycle_witnesses: BTreeMap<usize, CycleWitness>,
}

struct CycleWitness {
    selected: Vec<Option<usize>>,
    related_package_ids: Vec<String>,
}

impl Search<'_> {
    fn run(&mut self) {
        self.visit(0, &mut Vec::new());
    }

    fn visit(&mut self, group_index: usize, selected: &mut Vec<Option<usize>>) {
        if self.limit_exceeded || self.optimal_solution_found {
            return;
        }
        if let Some(best) = &self.best {
            let maximum_selected =
                selected.iter().flatten().count() + self.groups.len() - group_index;
            if best.iter().flatten().count() > maximum_selected {
                return;
            }
        }
        self.states_visited += 1;
        if self.states_visited > MAX_RESOLVER_SEARCH_STATES_V1 {
            self.limit_exceeded = true;
            return;
        }
        if group_index == self.groups.len() {
            self.consider(selected);
            return;
        }
        let options = self.groups[group_index].1.clone();
        for option in options.into_iter().map(Some).chain(std::iter::once(None)) {
            selected.push(option);
            if self.assigned_edges_remain_possible(selected) {
                self.visit(group_index + 1, selected);
            }
            selected.pop();
            if self.limit_exceeded {
                return;
            }
        }
    }

    fn assigned_edges_remain_possible(&self, selected: &[Option<usize>]) -> bool {
        let selections = self.selections_by_id(selected);
        for &candidate_index in selected.iter().flatten() {
            let candidate = &self.candidates[candidate_index];
            for (dependency_index, dependency) in
                candidate.manifest().dependencies.iter().enumerate()
            {
                if dependency.optional {
                    continue;
                }
                let Some(selected_dependency) = selections.get(&dependency.package_id) else {
                    continue;
                };
                let Some(selected_dependency) = selected_dependency else {
                    return false;
                };
                let Some(requirement) = candidate.requirements[dependency_index].as_ref() else {
                    return false;
                };
                if !self.candidates[*selected_dependency]
                    .version
                    .as_ref()
                    .is_some_and(|version| requirement.matches(version))
                {
                    return false;
                }
            }
        }
        true
    }

    fn consider(&mut self, selected: &[Option<usize>]) {
        let selection = self
            .selections_by_id(selected)
            .into_iter()
            .filter_map(|(package_id, candidate_index)| {
                candidate_index.map(|index| (package_id, index))
            })
            .collect::<BTreeMap<_, _>>();
        let Some(graph) = required_graph(self.candidates, &selection) else {
            return;
        };
        let cycle_components = graph_cycle_components(self.candidates, &graph);
        if !cycle_components.is_empty() {
            for (candidate_index, related_package_ids) in cycle_components {
                let replace = self
                    .cycle_witnesses
                    .get(&candidate_index)
                    .is_none_or(|witness| self.is_better(selected, &witness.selected));
                if replace {
                    self.cycle_witnesses.insert(
                        candidate_index,
                        CycleWitness {
                            selected: selected.to_vec(),
                            related_package_ids,
                        },
                    );
                }
            }
            return;
        }
        if self
            .best
            .as_ref()
            .is_none_or(|best| self.is_better(selected, best))
        {
            self.best = Some(selected.to_vec());
        }
        if selected
            .iter()
            .enumerate()
            .all(|(group_index, selected_index)| {
                selected_index.is_some_and(|selected_index| {
                    self.groups[group_index].1.first() == Some(&selected_index)
                })
            })
        {
            self.optimal_solution_found = true;
        }
    }

    fn selections_by_id(&self, selected: &[Option<usize>]) -> BTreeMap<String, Option<usize>> {
        self.groups
            .iter()
            .zip(selected)
            .map(|((package_id, _), candidate_index)| (package_id.clone(), *candidate_index))
            .collect()
    }

    fn is_better(&self, candidate: &[Option<usize>], current: &[Option<usize>]) -> bool {
        let candidate_count = candidate.iter().flatten().count();
        let current_count = current.iter().flatten().count();
        if candidate_count != current_count {
            return candidate_count > current_count;
        }
        for (candidate_index, current_index) in candidate.iter().zip(current) {
            match (candidate_index, current_index) {
                (Some(candidate_index), Some(current_index)) => {
                    let candidate_version = self.candidates[*candidate_index].version.as_ref();
                    let current_version = self.candidates[*current_index].version.as_ref();
                    if candidate_version != current_version {
                        return candidate_version > current_version;
                    }
                }
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                (None, None) => {}
            }
        }
        false
    }
}

fn resolve(validated_candidates: &[PackageValidationResultV1]) -> PackageResolutionV1<'_> {
    if validated_candidates.len() > MAX_RESOLVER_CANDIDATES_V1 {
        return reject_input_bound(validated_candidates);
    }
    let candidates = validated_candidates
        .iter()
        .map(|validation_result| {
            let manifest = validation_result.manifest();
            Candidate {
                validation_result,
                version: Version::parse(&manifest.package.version).ok(),
                requirements: manifest
                    .dependencies
                    .iter()
                    .map(|dependency| VersionReq::parse(&dependency.version_requirement).ok())
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let mut invalid = BTreeSet::new();
    let mut diagnostics = Vec::new();
    validate_candidates(&candidates, &mut invalid, &mut diagnostics);

    if dependency_edge_count(&candidates) > MAX_RESOLVER_REQUIRED_EDGES_V1 {
        for (index, candidate) in candidates.iter().enumerate() {
            invalid.insert(index);
            diagnostics.push(diagnostic(
                candidate,
                PackageResolutionDiagnosticCodeV1::SearchLimitExceeded,
                None,
                None,
                Vec::new(),
            ));
        }
        return finish(&candidates, &BTreeMap::new(), diagnostics, &BTreeMap::new());
    }

    let groups = candidate_groups(&candidates, &invalid);
    let mut search = Search {
        candidates: &candidates,
        groups,
        states_visited: 0,
        limit_exceeded: false,
        optimal_solution_found: false,
        best: None,
        cycle_witnesses: BTreeMap::new(),
    };
    search.run();
    if search.limit_exceeded {
        for (index, candidate) in candidates.iter().enumerate() {
            invalid.insert(index);
            diagnostics.push(diagnostic(
                candidate,
                PackageResolutionDiagnosticCodeV1::SearchLimitExceeded,
                None,
                None,
                Vec::new(),
            ));
        }
        return finish(&candidates, &BTreeMap::new(), diagnostics, &BTreeMap::new());
    }

    let chosen = search
        .best
        .unwrap_or_else(|| vec![None; search.groups.len()]);
    let mut selection = search
        .groups
        .iter()
        .zip(chosen)
        .filter_map(|((package_id, _), candidate_index)| {
            candidate_index.map(|index| (package_id.clone(), index))
        })
        .collect::<BTreeMap<_, _>>();
    let mut graph = if let Some(graph) = required_graph(&candidates, &selection) {
        graph
    } else {
        selection.clear();
        BTreeMap::new()
    };
    append_optional_edges(&candidates, &selection, &mut graph, &mut diagnostics);
    emit_unselected_diagnostics(
        &candidates,
        &selection,
        &invalid,
        &search.cycle_witnesses,
        &mut diagnostics,
    );
    finish(&candidates, &selection, diagnostics, &graph)
}

fn dependency_edge_count(candidates: &[Candidate<'_>]) -> usize {
    candidates
        .iter()
        .map(|candidate| candidate.manifest().dependencies.len())
        .sum()
}

fn reject_input_bound(
    validated_candidates: &[PackageValidationResultV1],
) -> PackageResolutionV1<'_> {
    let mut blocked_packages = validated_candidates
        .iter()
        .map(|validation_result| {
            let manifest = validation_result.manifest();
            BlockedPackageV1 {
                package_id: manifest.package.id.clone(),
                package_version: manifest.package.version.clone(),
            }
        })
        .collect::<Vec<_>>();
    let mut diagnostics = validated_candidates
        .iter()
        .map(|validation_result| {
            let manifest = validation_result.manifest();
            PackageResolutionDiagnosticV1 {
                package_id: manifest.package.id.clone(),
                package_version: manifest.package.version.clone(),
                dependency_package_id: None,
                version_requirement: None,
                code: PackageResolutionDiagnosticCodeV1::SearchLimitExceeded,
                related_package_ids: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    blocked_packages.sort_by(|left, right| {
        (&left.package_id, &left.package_version).cmp(&(&right.package_id, &right.package_version))
    });
    sort_diagnostics(&mut diagnostics);
    PackageResolutionV1 {
        resolved_packages: Vec::new(),
        blocked_packages,
        diagnostics,
    }
}

fn validate_candidates(
    candidates: &[Candidate<'_>],
    invalid: &mut BTreeSet<usize>,
    diagnostics: &mut Vec<PackageResolutionDiagnosticV1>,
) {
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.version.is_none() {
            invalidate(
                index,
                candidates,
                invalid,
                diagnostics,
                PackageResolutionDiagnosticCodeV1::InvalidPackageVersion,
                None,
                None,
                Vec::new(),
            );
            continue;
        }
        for (dependency_index, requirement) in candidate.requirements.iter().enumerate() {
            if requirement.is_none() {
                let dependency = &candidate.manifest().dependencies[dependency_index];
                invalidate(
                    index,
                    candidates,
                    invalid,
                    diagnostics,
                    PackageResolutionDiagnosticCodeV1::InvalidDependencyRequirement,
                    Some(dependency.package_id.clone()),
                    Some(dependency.version_requirement.clone()),
                    Vec::new(),
                );
                break;
            }
        }
    }
    let mut duplicates = BTreeMap::<(&str, &Version), Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if !invalid.contains(&index)
            && let Some(version) = &candidate.version
        {
            duplicates
                .entry((&candidate.manifest().package.id, version))
                .or_default()
                .push(index);
        }
    }
    for ((package_id, _), indexes) in duplicates {
        if indexes.len() > 1 {
            for index in indexes {
                invalidate(
                    index,
                    candidates,
                    invalid,
                    diagnostics,
                    PackageResolutionDiagnosticCodeV1::DuplicatePackageVersion,
                    None,
                    None,
                    vec![package_id.to_owned()],
                );
            }
        }
    }
}

fn candidate_groups(
    candidates: &[Candidate<'_>],
    invalid: &BTreeSet<usize>,
) -> Vec<(String, Vec<usize>)> {
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if !invalid.contains(&index) && candidate.version.is_some() {
            groups
                .entry(candidate.manifest().package.id.clone())
                .or_default()
                .push(index);
        }
    }
    for indexes in groups.values_mut() {
        indexes.sort_by(|left, right| candidates[*right].version.cmp(&candidates[*left].version));
    }
    groups.into_iter().collect()
}

fn required_graph(
    candidates: &[Candidate<'_>],
    selection: &BTreeMap<String, usize>,
) -> Option<BTreeMap<usize, Vec<usize>>> {
    let mut graph = BTreeMap::new();
    for &candidate_index in selection.values() {
        let candidate = &candidates[candidate_index];
        let mut edges = Vec::new();
        for (dependency_index, dependency) in candidate.manifest().dependencies.iter().enumerate() {
            if dependency.optional {
                continue;
            }
            let requirement = candidate.requirements[dependency_index].as_ref()?;
            let selected_index = *selection.get(&dependency.package_id)?;
            let version = candidates[selected_index].version.as_ref()?;
            if !requirement.matches(version) {
                return None;
            }
            edges.push(selected_index);
        }
        graph.insert(candidate_index, edges);
    }
    Some(graph)
}

fn graph_cycle_components(
    candidates: &[Candidate<'_>],
    graph: &BTreeMap<usize, Vec<usize>>,
) -> BTreeMap<usize, Vec<String>> {
    let mut components = BTreeMap::new();
    let mut assigned = BTreeSet::new();
    for index in graph.keys().copied() {
        if assigned.contains(&index) {
            continue;
        }
        let members = graph
            .keys()
            .copied()
            .filter(|other| {
                graph_reaches(graph, index, *other) && graph_reaches(graph, *other, index)
            })
            .collect::<BTreeSet<_>>();
        let is_cycle = members.len() > 1
            || graph
                .get(&index)
                .is_some_and(|edges| edges.contains(&index));
        if !is_cycle {
            continue;
        }
        let mut related_package_ids = members
            .iter()
            .map(|member| candidates[*member].manifest().package.id.clone())
            .collect::<Vec<_>>();
        related_package_ids.sort();
        related_package_ids.dedup();
        for member in members {
            assigned.insert(member);
            components.insert(member, related_package_ids.clone());
        }
    }
    components
}

fn append_optional_edges(
    candidates: &[Candidate<'_>],
    selection: &BTreeMap<String, usize>,
    graph: &mut BTreeMap<usize, Vec<usize>>,
    diagnostics: &mut Vec<PackageResolutionDiagnosticV1>,
) {
    for &candidate_index in selection.values() {
        let candidate = &candidates[candidate_index];
        for (dependency_index, dependency) in candidate.manifest().dependencies.iter().enumerate() {
            if !dependency.optional {
                continue;
            }
            let Some(requirement) = candidate.requirements[dependency_index].as_ref() else {
                continue;
            };
            let Some(&selected_index) = selection.get(&dependency.package_id) else {
                diagnostics.push(diagnostic(
                    candidate,
                    PackageResolutionDiagnosticCodeV1::OptionalDependencyUnavailable,
                    Some(dependency.package_id.clone()),
                    Some(dependency.version_requirement.clone()),
                    Vec::new(),
                ));
                continue;
            };
            let compatible = candidates[selected_index]
                .version
                .as_ref()
                .is_some_and(|version| requirement.matches(version));
            if !compatible {
                diagnostics.push(diagnostic(
                    candidate,
                    PackageResolutionDiagnosticCodeV1::OptionalDependencyUnavailable,
                    Some(dependency.package_id.clone()),
                    Some(dependency.version_requirement.clone()),
                    Vec::new(),
                ));
            } else if graph_reaches(graph, selected_index, candidate_index) {
                diagnostics.push(diagnostic(
                    candidate,
                    PackageResolutionDiagnosticCodeV1::OptionalDependencyCycleIgnored,
                    Some(dependency.package_id.clone()),
                    Some(dependency.version_requirement.clone()),
                    vec![dependency.package_id.clone()],
                ));
            } else {
                graph
                    .entry(candidate_index)
                    .or_default()
                    .push(selected_index);
            }
        }
    }
}

fn graph_reaches(graph: &BTreeMap<usize, Vec<usize>>, from: usize, target: usize) -> bool {
    let mut pending = vec![from];
    let mut seen = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if !seen.insert(index) {
            continue;
        }
        if index == target {
            return true;
        }
        if let Some(edges) = graph.get(&index) {
            pending.extend(edges.iter().copied());
        }
    }
    false
}

fn emit_unselected_diagnostics(
    candidates: &[Candidate<'_>],
    selection: &BTreeMap<String, usize>,
    invalid: &BTreeSet<usize>,
    cycle_witnesses: &BTreeMap<usize, CycleWitness>,
    diagnostics: &mut Vec<PackageResolutionDiagnosticV1>,
) {
    let selected = selection.values().copied().collect::<BTreeSet<_>>();
    for (index, candidate) in candidates.iter().enumerate() {
        if invalid.contains(&index) || selected.contains(&index) {
            continue;
        }
        if selection.contains_key(&candidate.manifest().package.id) {
            diagnostics.push(diagnostic(
                candidate,
                PackageResolutionDiagnosticCodeV1::SupersededBySelectedVersion,
                None,
                None,
                vec![candidate.manifest().package.id.clone()],
            ));
            continue;
        }
        if let Some(witness) = cycle_witnesses.get(&index) {
            diagnostics.push(diagnostic(
                candidate,
                PackageResolutionDiagnosticCodeV1::DependencyCycle,
                None,
                None,
                witness.related_package_ids.clone(),
            ));
            continue;
        }
        let mut emitted = false;
        for (dependency_index, dependency) in candidate.manifest().dependencies.iter().enumerate() {
            if dependency.optional {
                continue;
            }
            let requirement = candidate.requirements[dependency_index].as_ref();
            let selected_compatible =
                selection
                    .get(&dependency.package_id)
                    .is_some_and(|selected_index| {
                        requirement.is_some_and(|requirement| {
                            candidates[*selected_index]
                                .version
                                .as_ref()
                                .is_some_and(|version| requirement.matches(version))
                        })
                    });
            if selected_compatible {
                continue;
            }
            let code = if candidates
                .iter()
                .any(|other| other.manifest().package.id == dependency.package_id)
            {
                PackageResolutionDiagnosticCodeV1::RejectedRequiredDependency
            } else {
                PackageResolutionDiagnosticCodeV1::UnsatisfiedRequiredDependency
            };
            diagnostics.push(diagnostic(
                candidate,
                code,
                Some(dependency.package_id.clone()),
                Some(dependency.version_requirement.clone()),
                Vec::new(),
            ));
            emitted = true;
        }
        if !emitted {
            diagnostics.push(diagnostic(
                candidate,
                PackageResolutionDiagnosticCodeV1::NotSelectedByGlobalResolution,
                None,
                None,
                Vec::new(),
            ));
        }
    }
}

fn finish<'validated>(
    candidates: &[Candidate<'validated>],
    selection: &BTreeMap<String, usize>,
    mut diagnostics: Vec<PackageResolutionDiagnosticV1>,
    graph: &BTreeMap<usize, Vec<usize>>,
) -> PackageResolutionV1<'validated> {
    let selected = selection.values().copied().collect::<BTreeSet<_>>();
    let resolved_packages = selection
        .values()
        .map(|&candidate_index| {
            let candidate = &candidates[candidate_index];
            let dependencies = candidate
                .manifest()
                .dependencies
                .iter()
                .filter_map(|dependency| {
                    let selected_index = *selection.get(&dependency.package_id)?;
                    graph
                        .get(&candidate_index)?
                        .contains(&selected_index)
                        .then(|| ResolvedPackageDependencyV1 {
                            package_id: dependency.package_id.clone(),
                            package_version: candidates[selected_index]
                                .manifest()
                                .package
                                .version
                                .clone(),
                            optional: dependency.optional,
                        })
                })
                .collect();
            ResolvedPackageV1 {
                validation_result: candidate.validation_result,
                dependencies,
            }
        })
        .collect();
    let mut blocked_packages = candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| !selected.contains(index))
        .map(|(_, candidate)| BlockedPackageV1 {
            package_id: candidate.manifest().package.id.clone(),
            package_version: candidate.manifest().package.version.clone(),
        })
        .collect::<Vec<_>>();
    blocked_packages.sort_by(|left, right| {
        (&left.package_id, &left.package_version).cmp(&(&right.package_id, &right.package_version))
    });
    sort_diagnostics(&mut diagnostics);
    PackageResolutionV1 {
        resolved_packages,
        blocked_packages,
        diagnostics,
    }
}

fn sort_diagnostics(diagnostics: &mut [PackageResolutionDiagnosticV1]) {
    diagnostics.sort_by(|left, right| {
        (
            &left.package_id,
            &left.package_version,
            left.code,
            &left.dependency_package_id,
        )
            .cmp(&(
                &right.package_id,
                &right.package_version,
                right.code,
                &right.dependency_package_id,
            ))
    });
}

fn invalidate(
    candidate_index: usize,
    candidates: &[Candidate<'_>],
    invalid: &mut BTreeSet<usize>,
    diagnostics: &mut Vec<PackageResolutionDiagnosticV1>,
    code: PackageResolutionDiagnosticCodeV1,
    dependency_package_id: Option<String>,
    version_requirement: Option<String>,
    related_package_ids: Vec<String>,
) {
    if invalid.insert(candidate_index) {
        diagnostics.push(diagnostic(
            &candidates[candidate_index],
            code,
            dependency_package_id,
            version_requirement,
            related_package_ids,
        ));
    }
}

fn diagnostic(
    candidate: &Candidate<'_>,
    code: PackageResolutionDiagnosticCodeV1,
    dependency_package_id: Option<String>,
    version_requirement: Option<String>,
    mut related_package_ids: Vec<String>,
) -> PackageResolutionDiagnosticV1 {
    related_package_ids.sort();
    related_package_ids.dedup();
    PackageResolutionDiagnosticV1 {
        package_id: candidate.manifest().package.id.clone(),
        package_version: candidate.manifest().package.version.clone(),
        dependency_package_id,
        version_requirement,
        code,
        related_package_ids,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PackageResolutionDiagnosticCodeV1, PackageResolverV1, PackageValidationResultV1};
    use crate::PackageManifestV1;

    fn candidate(
        id: &str,
        version: &str,
        dependencies: Vec<(&str, &str, bool)>,
    ) -> PackageValidationResultV1 {
        let manifest = PackageManifestV1::parse_json(&json!({
            "manifest_version": 1,
            "package": { "id": id, "version": version },
            "publisher": { "id": "example.publisher", "display_name": "Example Publisher", "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }] },
            "sdk": { "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc", "abi_schema": 1, "gpui": false, "ui_abi_fingerprint": null },
            "rust": [], "lua": [], "skins": [], "locales": [], "tools": [], "features": [],
            "dependencies": dependencies.into_iter().map(|(package_id, version_requirement, optional)| json!({ "package_id": package_id, "version_requirement": version_requirement, "optional": optional })).collect::<Vec<_>>(),
            "payloads": [], "signature": { "kind": "unsigned" }, "data_version": 1
        }).to_string()).expect("test manifest is structurally valid");
        PackageValidationResultV1::for_resolver_test(manifest)
    }

    fn resolved_ids<'validated>(
        resolution: &'validated super::PackageResolutionV1<'validated>,
    ) -> Vec<(&'validated str, &'validated str)> {
        resolution
            .resolved_packages()
            .iter()
            .map(|package| {
                (
                    package.manifest().package.id.as_str(),
                    package.manifest().package.version.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn finds_the_complete_solution_that_fixed_point_selection_misses() {
        let candidates = vec![
            candidate("example.a", "2.0.0", vec![("example.b", "=1.0.0", false)]),
            candidate("example.a", "1.0.0", vec![("example.b", "=2.0.0", false)]),
            candidate("example.b", "2.0.0", vec![("example.a", "=1.0.0", false)]),
            candidate("example.b", "1.0.0", vec![]),
        ];
        let resolution = PackageResolverV1::resolve(&candidates);
        assert_eq!(
            resolved_ids(&resolution),
            vec![("example.a", "2.0.0"), ("example.b", "1.0.0")]
        );
    }

    #[test]
    fn candidate_order_does_not_change_complete_search_selection() {
        let first = vec![
            candidate("example.base", "1.0.0", vec![]),
            candidate("example.base", "2.0.0", vec![]),
            candidate(
                "example.consumer",
                "1.0.0",
                vec![("example.base", "^1.0.0", false)],
            ),
        ];
        let second = vec![first[2].clone(), first[0].clone(), first[1].clone()];
        assert_eq!(
            resolved_ids(&PackageResolverV1::resolve(&first)),
            resolved_ids(&PackageResolverV1::resolve(&second))
        );
    }

    #[test]
    fn required_cycles_are_rejected_atomically_and_transitively() {
        let candidates = vec![
            candidate("example.a", "1.0.0", vec![("example.b", "^1.0.0", false)]),
            candidate("example.b", "1.0.0", vec![("example.a", "^1.0.0", false)]),
            candidate(
                "example.dependent",
                "1.0.0",
                vec![("example.a", "^1.0.0", false)],
            ),
        ];
        let resolution = PackageResolverV1::resolve(&candidates);
        assert!(resolution.resolved_packages().is_empty());
        assert_eq!(resolution.blocked_packages().len(), 3);
        assert!(resolution.diagnostics().iter().any(
            |diagnostic| diagnostic.code == PackageResolutionDiagnosticCodeV1::DependencyCycle
        ));
        assert!(
            resolution
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code
                    == PackageResolutionDiagnosticCodeV1::RejectedRequiredDependency
                    && diagnostic.package_id == "example.dependent")
        );
    }

    #[test]
    fn optional_cycle_edge_is_ignored_without_blocking_its_owner() {
        let candidates = vec![
            candidate("example.a", "1.0.0", vec![("example.b", "^1.0.0", true)]),
            candidate("example.b", "1.0.0", vec![("example.a", "^1.0.0", false)]),
        ];
        let resolution = PackageResolverV1::resolve(&candidates);
        assert_eq!(resolution.resolved_packages().len(), 2);
        assert!(
            resolution
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code
                    == PackageResolutionDiagnosticCodeV1::OptionalDependencyCycleIgnored
                    && diagnostic.package_id == "example.a")
        );
        let a = resolution
            .resolved_packages()
            .iter()
            .find(|package| package.manifest().package.id == "example.a")
            .expect("selected package");
        assert!(a.dependencies().is_empty());
    }

    #[test]
    fn malformed_versions_and_optional_missing_or_incompatible_dependencies_are_typed() {
        let candidates = vec![
            candidate("example.bad", "not-semver", vec![]),
            candidate(
                "example.bad-requirement",
                "1.0.0",
                vec![("example.base", "not-a-range", false)],
            ),
            candidate(
                "example.optional",
                "1.0.0",
                vec![("example.none", "^1.0.0", true)],
            ),
            candidate(
                "example.incompatible",
                "1.0.0",
                vec![("example.base", "^2.0.0", true)],
            ),
            candidate("example.base", "1.0.0", vec![]),
        ];
        let resolution = PackageResolverV1::resolve(&candidates);
        assert_eq!(resolution.resolved_packages().len(), 3);
        assert!(
            resolution
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code
                    == PackageResolutionDiagnosticCodeV1::InvalidPackageVersion)
        );
        assert!(
            resolution
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code
                    == PackageResolutionDiagnosticCodeV1::InvalidDependencyRequirement)
        );
        assert_eq!(
            resolution
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code
                    == PackageResolutionDiagnosticCodeV1::OptionalDependencyUnavailable)
                .count(),
            2
        );
    }

    #[test]
    fn candidate_bound_rejects_the_entire_unsearched_input() {
        let candidates = (0..129)
            .map(|index| candidate(&format!("example.limit{index}"), "1.0.0", vec![]))
            .collect::<Vec<_>>();

        let resolution = PackageResolverV1::resolve(&candidates);

        assert!(resolution.resolved_packages().is_empty());
        assert_eq!(resolution.blocked_packages().len(), 129);
        assert!(resolution.diagnostics().iter().all(|diagnostic| {
            diagnostic.code == PackageResolutionDiagnosticCodeV1::SearchLimitExceeded
        }));
    }

    #[test]
    fn duplicate_id_and_version_is_rejected_without_source_order_precedence() {
        let candidates = vec![
            candidate("example.duplicate", "1.0.0", vec![]),
            candidate("example.duplicate", "1.0.0", vec![]),
            candidate(
                "example.consumer",
                "1.0.0",
                vec![("example.duplicate", "^1.0.0", false)],
            ),
        ];

        let resolution = PackageResolverV1::resolve(&candidates);

        assert!(resolution.resolved_packages().is_empty());
        assert_eq!(resolution.blocked_packages().len(), 3);
        assert_eq!(
            resolution
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code
                    == PackageResolutionDiagnosticCodeV1::DuplicatePackageVersion)
                .count(),
            2
        );
    }

    #[test]
    fn independent_candidates_complete_without_exhaustive_subset_enumeration() {
        for candidate_count in [20, 128] {
            let candidates = (0..candidate_count)
                .map(|index| candidate(&format!("example.independent{index}"), "1.0.0", vec![]))
                .collect::<Vec<_>>();

            let resolution = PackageResolverV1::resolve(&candidates);

            assert_eq!(resolution.resolved_packages().len(), candidate_count);
            assert!(resolution.blocked_packages().is_empty());
            assert!(!resolution.diagnostics().iter().any(|diagnostic| {
                diagnostic.code == PackageResolutionDiagnosticCodeV1::SearchLimitExceeded
            }));
        }
    }

    #[test]
    fn unselected_diagnostics_consider_every_required_edge_independent_of_declaration_order() {
        let first = vec![
            candidate(
                "example.a",
                "1.0.0",
                vec![
                    ("example.b", "^1.0.0", false),
                    ("example.missing", "^1.0.0", false),
                ],
            ),
            candidate("example.b", "1.0.0", vec![]),
        ];
        let second = vec![
            candidate(
                "example.a",
                "1.0.0",
                vec![
                    ("example.missing", "^1.0.0", false),
                    ("example.b", "^1.0.0", false),
                ],
            ),
            candidate("example.b", "1.0.0", vec![]),
        ];

        let first_diagnostics = PackageResolverV1::resolve(&first).diagnostics().to_vec();
        let second_diagnostics = PackageResolverV1::resolve(&second).diagnostics().to_vec();

        assert_eq!(first_diagnostics, second_diagnostics);
        assert_eq!(
            first_diagnostics,
            vec![super::PackageResolutionDiagnosticV1 {
                package_id: "example.a".to_owned(),
                package_version: "1.0.0".to_owned(),
                dependency_package_id: Some("example.missing".to_owned()),
                version_requirement: Some("^1.0.0".to_owned()),
                code: PackageResolutionDiagnosticCodeV1::UnsatisfiedRequiredDependency,
                related_package_ids: Vec::new(),
            }]
        );
    }

    #[test]
    fn cycle_diagnostics_use_canonical_final_sccs_across_candidate_permutations() {
        let first = vec![
            candidate("example.a", "1.0.0", vec![("example.b", "^1.0.0", false)]),
            candidate("example.b", "1.0.0", vec![("example.a", "^1.0.0", false)]),
            candidate("example.c", "1.0.0", vec![("example.d", "^1.0.0", false)]),
            candidate("example.d", "1.0.0", vec![("example.c", "^1.0.0", false)]),
        ];
        let second = vec![
            first[3].clone(),
            first[1].clone(),
            first[2].clone(),
            first[0].clone(),
        ];

        let cycle_diagnostics = |candidates: &[PackageValidationResultV1]| {
            PackageResolverV1::resolve(candidates)
                .diagnostics()
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == PackageResolutionDiagnosticCodeV1::DependencyCycle
                })
                .map(|diagnostic| {
                    (
                        diagnostic.package_id.clone(),
                        diagnostic.related_package_ids.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(cycle_diagnostics(&first), cycle_diagnostics(&second));
        assert_eq!(
            cycle_diagnostics(&first),
            vec![
                (
                    "example.a".to_owned(),
                    vec!["example.a".to_owned(), "example.b".to_owned()]
                ),
                (
                    "example.b".to_owned(),
                    vec!["example.a".to_owned(), "example.b".to_owned()]
                ),
                (
                    "example.c".to_owned(),
                    vec!["example.c".to_owned(), "example.d".to_owned()]
                ),
                (
                    "example.d".to_owned(),
                    vec!["example.c".to_owned(), "example.d".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn joint_version_cycle_witnesses_are_preserved_across_input_permutations() {
        let first = vec![
            candidate("example.a", "2.0.0", vec![("example.b", "=1.0.0", false)]),
            candidate("example.a", "1.0.0", vec![("example.b", "=2.0.0", false)]),
            candidate("example.b", "2.0.0", vec![("example.a", "=1.0.0", false)]),
            candidate("example.b", "1.0.0", vec![("example.a", "=2.0.0", false)]),
        ];
        let second = vec![
            first[3].clone(),
            first[1].clone(),
            first[2].clone(),
            first[0].clone(),
        ];
        let cycles = |candidates: &[PackageValidationResultV1]| {
            PackageResolverV1::resolve(candidates)
                .diagnostics()
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == PackageResolutionDiagnosticCodeV1::DependencyCycle
                })
                .map(|diagnostic| {
                    (
                        diagnostic.package_id.clone(),
                        diagnostic.package_version.clone(),
                        diagnostic.related_package_ids.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };

        assert!(
            PackageResolverV1::resolve(&first)
                .resolved_packages()
                .is_empty()
        );
        assert_eq!(cycles(&first), cycles(&second));
        assert_eq!(
            cycles(&first),
            vec![
                (
                    "example.a".to_owned(),
                    "1.0.0".to_owned(),
                    vec!["example.a".to_owned(), "example.b".to_owned()],
                ),
                (
                    "example.a".to_owned(),
                    "2.0.0".to_owned(),
                    vec!["example.a".to_owned(), "example.b".to_owned()],
                ),
                (
                    "example.b".to_owned(),
                    "1.0.0".to_owned(),
                    vec!["example.a".to_owned(), "example.b".to_owned()],
                ),
                (
                    "example.b".to_owned(),
                    "2.0.0".to_owned(),
                    vec!["example.a".to_owned(), "example.b".to_owned()],
                ),
            ]
        );
    }

    #[test]
    fn optional_edges_count_toward_the_resolver_input_bound() {
        let dependency_ids = (0..128)
            .map(|index| format!("example.optional{index}"))
            .collect::<Vec<_>>();
        let dependencies = dependency_ids
            .iter()
            .map(|package_id| (package_id.as_str(), "^1.0.0", true))
            .collect::<Vec<_>>();
        let candidates = (0..5)
            .map(|index| {
                candidate(
                    &format!("example.owner{index}"),
                    "1.0.0",
                    dependencies.clone(),
                )
            })
            .collect::<Vec<_>>();

        let resolution = PackageResolverV1::resolve(&candidates);

        assert!(resolution.resolved_packages().is_empty());
        assert!(resolution.diagnostics().iter().all(|diagnostic| {
            diagnostic.code == PackageResolutionDiagnosticCodeV1::SearchLimitExceeded
        }));
    }
}
