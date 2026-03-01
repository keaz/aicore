use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostics::Diagnostic;
use crate::metrics::MetricsThresholds;
use crate::span::Span;

const LOCKFILE_NAME: &str = "aic.lock";
const WORKSPACE_MANIFEST_NAME: &str = "aic.workspace.toml";
const CACHE_DIR_NAME: &str = ".aic-cache";
const LOCKFILE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Default)]
pub struct PackageOptions {
    pub offline: bool,
}

#[derive(Debug, Clone)]
pub struct DependencyContext {
    pub roots: Vec<PathBuf>,
    pub source_roots: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub lockfile_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub package_name: String,
    pub main: String,
    pub dependencies: Vec<ManifestDependency>,
    pub native: NativeLinkConfig,
    pub metrics: MetricsThresholds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceManifest {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBuildPlan {
    pub root: PathBuf,
    pub members: Vec<WorkspaceBuildMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBuildMember {
    pub name: String,
    pub root: PathBuf,
    pub main: String,
    pub workspace_dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManifestDependency {
    pub name: String,
    pub path: String,
    pub resolved_version: Option<String>,
    pub source_provenance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativeLinkConfig {
    pub libs: Vec<String>,
    pub search_paths: Vec<String>,
    pub objects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    pub schema_version: u32,
    pub package: String,
    pub dependencies: Vec<LockedDependency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceLockMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LockedDependency {
    pub name: String,
    pub path: String,
    pub checksum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_provenance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceLockMetadata {
    pub members: Vec<LockedWorkspaceMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LockedWorkspaceMember {
    pub name: String,
    pub path: String,
    pub main: String,
    pub workspace_dependencies: Vec<String>,
    pub dependency_paths: Vec<String>,
}

#[derive(Debug, Clone)]
struct WorkspaceGraphNode {
    name: String,
    root: PathBuf,
    manifest: Manifest,
    workspace_dependencies: Vec<String>,
}

pub fn workspace_manifest_path(root: &Path) -> PathBuf {
    root.join(WORKSPACE_MANIFEST_NAME)
}

pub fn read_workspace_manifest(workspace_root: &Path) -> anyhow::Result<Option<WorkspaceManifest>> {
    let workspace_root = canonical_or_self(workspace_root.to_path_buf());
    let path = workspace_manifest_path(&workspace_root);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    Ok(Some(parse_workspace_manifest(&text, &path)?))
}

pub fn workspace_build_plan(
    workspace_root: &Path,
) -> Result<Option<WorkspaceBuildPlan>, Diagnostic> {
    let workspace_root = canonical_or_self(workspace_root.to_path_buf());
    let Some(manifest) = read_workspace_manifest(&workspace_root).map_err(|err| {
        Diagnostic::error(
            "E2125",
            format!("failed to read workspace manifest: {err}"),
            &workspace_manifest_path(&workspace_root).to_string_lossy(),
            Span::new(0, 0),
        )
        .with_help("fix aic.workspace.toml and retry")
    })?
    else {
        return Ok(None);
    };

    Ok(Some(build_workspace_plan(&workspace_root, &manifest)?))
}

fn build_workspace_plan(
    workspace_root: &Path,
    workspace_manifest: &WorkspaceManifest,
) -> Result<WorkspaceBuildPlan, Diagnostic> {
    if workspace_manifest.members.is_empty() {
        return Err(Diagnostic::error(
            "E2125",
            "workspace manifest must list at least one member",
            &workspace_manifest_path(workspace_root).to_string_lossy(),
            Span::new(0, 0),
        )
        .with_help("set `[workspace].members` to one or more package directories"));
    }

    let mut seen_member_paths = BTreeSet::new();
    let mut nodes = Vec::new();
    let mut name_to_root = BTreeMap::<String, PathBuf>::new();
    let mut root_to_name = BTreeMap::<PathBuf, String>::new();

    for member_path in &workspace_manifest.members {
        if !seen_member_paths.insert(member_path.clone()) {
            return Err(Diagnostic::error(
                "E2125",
                format!("workspace member path '{}' is duplicated", member_path),
                &workspace_manifest_path(workspace_root).to_string_lossy(),
                Span::new(0, 0),
            )
            .with_help("remove duplicate entries from `[workspace].members`"));
        }

        let member_root = canonical_or_self(workspace_root.join(member_path));
        let Some(member_manifest) = read_manifest(&member_root).map_err(|err| {
            Diagnostic::error(
                "E2125",
                format!(
                    "failed to read workspace member manifest '{}': {err}",
                    member_root.display()
                ),
                &workspace_manifest_path(workspace_root).to_string_lossy(),
                Span::new(0, 0),
            )
            .with_help("each workspace member must contain a valid aic.toml")
        })?
        else {
            return Err(Diagnostic::error(
                "E2125",
                format!(
                    "workspace member '{}' is missing aic.toml",
                    member_root.display()
                ),
                &workspace_manifest_path(workspace_root).to_string_lossy(),
                Span::new(0, 0),
            )
            .with_help("create aic.toml for the member or remove it from workspace.members"));
        };

        if let Some(existing) = name_to_root.get(&member_manifest.package_name) {
            return Err(Diagnostic::error(
                "E2125",
                format!(
                    "workspace package name '{}' is duplicated at '{}' and '{}'",
                    member_manifest.package_name,
                    existing.display(),
                    member_root.display()
                ),
                &workspace_manifest_path(workspace_root).to_string_lossy(),
                Span::new(0, 0),
            )
            .with_help("workspace package names must be globally unique"));
        }

        name_to_root.insert(member_manifest.package_name.clone(), member_root.clone());
        root_to_name.insert(member_root.clone(), member_manifest.package_name.clone());
        nodes.push(WorkspaceGraphNode {
            name: member_manifest.package_name.clone(),
            root: member_root,
            manifest: member_manifest,
            workspace_dependencies: Vec::new(),
        });
    }

    for node in &mut nodes {
        let mut deps = BTreeSet::new();
        for dep in &node.manifest.dependencies {
            let dep_root = canonical_or_self(node.root.join(&dep.path));
            if let Some(target) = root_to_name.get(&dep_root) {
                if target != &node.name {
                    deps.insert(target.clone());
                }
                continue;
            }
            if let Some(target_root) = name_to_root.get(&dep.name) {
                let target_name = root_to_name
                    .get(target_root)
                    .cloned()
                    .unwrap_or_else(|| dep.name.clone());
                if target_name != node.name {
                    deps.insert(target_name);
                }
            }
        }
        node.workspace_dependencies = deps.into_iter().collect();
    }

    let mut indegree = BTreeMap::new();
    let mut dependents = BTreeMap::<String, BTreeSet<String>>::new();
    let mut member_deps = BTreeMap::<String, Vec<String>>::new();
    for node in &nodes {
        indegree.insert(node.name.clone(), 0usize);
        dependents.entry(node.name.clone()).or_default();
    }
    for node in &nodes {
        member_deps.insert(node.name.clone(), node.workspace_dependencies.clone());
        for dep in &node.workspace_dependencies {
            dependents
                .entry(dep.clone())
                .or_default()
                .insert(node.name.clone());
            *indegree.entry(node.name.clone()).or_default() += 1;
        }
    }

    let mut ready = indegree
        .iter()
        .filter_map(|(name, degree)| {
            if *degree == 0 {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    let mut build_order = Vec::new();

    while let Some(name) = ready.pop_first() {
        build_order.push(name.clone());
        let Some(children) = dependents.get(&name) else {
            continue;
        };
        for child in children {
            if let Some(entry) = indegree.get_mut(child) {
                if *entry > 0 {
                    *entry -= 1;
                }
                if *entry == 0 {
                    ready.insert(child.clone());
                }
            }
        }
    }

    if build_order.len() != nodes.len() {
        let cycle = find_workspace_cycle(&member_deps);
        let cycle_text = if cycle.is_empty() {
            "unable to derive package cycle path".to_string()
        } else {
            cycle.join(" -> ")
        };
        return Err(Diagnostic::error(
            "E2126",
            format!("workspace package cycle detected: {cycle_text}"),
            &workspace_manifest_path(workspace_root).to_string_lossy(),
            Span::new(0, 0),
        )
        .with_help("remove cyclic package dependencies by extracting shared packages"));
    }

    let node_by_name = nodes
        .into_iter()
        .map(|node| (node.name.clone(), node))
        .collect::<BTreeMap<_, _>>();

    let mut members = Vec::new();
    for name in build_order {
        if let Some(node) = node_by_name.get(&name) {
            members.push(WorkspaceBuildMember {
                name: node.name.clone(),
                root: node.root.clone(),
                main: node.manifest.main.clone(),
                workspace_dependencies: node.workspace_dependencies.clone(),
            });
        }
    }

    Ok(WorkspaceBuildPlan {
        root: workspace_root.to_path_buf(),
        members,
    })
}

fn find_workspace_cycle(member_deps: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    fn dfs(
        name: &str,
        member_deps: &BTreeMap<String, Vec<String>>,
        state: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        state.insert(name.to_string(), 1);
        stack.push(name.to_string());

        if let Some(deps) = member_deps.get(name) {
            for dep in deps {
                match state.get(dep).copied().unwrap_or(0) {
                    0 => {
                        if let Some(cycle) = dfs(dep, member_deps, state, stack) {
                            return Some(cycle);
                        }
                    }
                    1 => {
                        if let Some(start) = stack.iter().position(|entry| entry == dep) {
                            let mut cycle = stack[start..].to_vec();
                            cycle.push(dep.clone());
                            return Some(canonicalize_named_cycle(&cycle));
                        }
                    }
                    _ => {}
                }
            }
        }

        stack.pop();
        state.insert(name.to_string(), 2);
        None
    }

    let mut state = BTreeMap::<String, u8>::new();
    let mut stack = Vec::new();

    for name in member_deps.keys() {
        if state.get(name).copied().unwrap_or(0) == 0 {
            if let Some(cycle) = dfs(name, member_deps, &mut state, &mut stack) {
                return cycle;
            }
        }
    }

    Vec::new()
}

fn canonicalize_named_cycle(cycle: &[String]) -> Vec<String> {
    let mut nodes = cycle.to_vec();
    if nodes.len() > 1 && nodes.first() == nodes.last() {
        nodes.pop();
    }
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut best: Option<Vec<String>> = None;
    for candidate_seq in [nodes.clone(), {
        let mut rev = nodes.clone();
        rev.reverse();
        rev
    }] {
        for start in 0..candidate_seq.len() {
            let mut rotated = candidate_seq[start..].to_vec();
            rotated.extend_from_slice(&candidate_seq[..start]);
            let replace = best
                .as_ref()
                .map(|current| rotated.join("|") < current.join("|"))
                .unwrap_or(true);
            if replace {
                best = Some(rotated);
            }
        }
    }

    let mut canonical = best.unwrap_or(nodes);
    canonical.push(canonical[0].clone());
    canonical
}

fn find_workspace_root(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    loop {
        if workspace_manifest_path(&dir).exists() {
            return Some(canonical_or_self(dir));
        }
        let Some(parent) = dir.parent() else {
            return None;
        };
        dir = parent.to_path_buf();
    }
}

pub fn resolve_dependency_context(
    project_root: &Path,
    options: PackageOptions,
) -> anyhow::Result<DependencyContext> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let mut diagnostics = Vec::new();
    let mut roots = BTreeSet::new();
    let mut source_roots = BTreeSet::new();
    let mut lockfile_used = false;

    let Some(manifest) = read_manifest(&project_root)? else {
        return Ok(DependencyContext {
            roots: Vec::new(),
            source_roots: Vec::new(),
            diagnostics,
            lockfile_used,
        });
    };

    let workspace_context = match workspace_context_for_project(&project_root) {
        Ok(context) => context,
        Err(diag) => {
            diagnostics.push(diag);
            return Ok(DependencyContext {
                roots: Vec::new(),
                source_roots: Vec::new(),
                diagnostics,
                lockfile_used,
            });
        }
    };

    let lock_root = workspace_context
        .as_ref()
        .map(|ctx| ctx.plan.root.clone())
        .unwrap_or_else(|| project_root.clone());
    let workspace_member_name = workspace_context
        .as_ref()
        .map(|ctx| ctx.member.name.clone());

    let lock_path = lockfile_path(&project_root);
    let expected = if options.offline {
        None
    } else if let Some(context) = &workspace_context {
        match generate_workspace_lockfile_from_plan(&context.plan) {
            Ok(lock) => Some(lock),
            Err(diag) => {
                diagnostics.push(diag);
                return Ok(DependencyContext {
                    roots: Vec::new(),
                    source_roots: Vec::new(),
                    diagnostics,
                    lockfile_used,
                });
            }
        }
    } else {
        Some(generate_lockfile_from_manifest(
            &project_root,
            &project_root,
            &manifest,
        )?)
    };

    if let Some(lock) = read_lockfile(&lock_root)? {
        lockfile_used = true;
        if let Some(expected_lock) = &expected {
            if &lock != expected_lock {
                diagnostics.push(
                    Diagnostic::error(
                        "E2106",
                        "lockfile drift detected between aic.toml and aic.lock",
                        &lock_path.to_string_lossy(),
                        Span::new(0, 0),
                    )
                    .with_help("run `aic lock` to regenerate aic.lock from the manifest"),
                );
            }
        }

        let allowed_paths =
            workspace_dependency_paths_for_member(&lock, workspace_member_name.as_deref());
        if matches!(
            allowed_paths,
            DependencyPathSelection::MissingMemberMetadata
        ) {
            diagnostics.push(
                Diagnostic::error(
                    "E2125",
                    "workspace lockfile is missing dependency metadata for the current package",
                    &lock_path.to_string_lossy(),
                    Span::new(0, 0),
                )
                .with_help("run `aic lock` from the workspace root to regenerate lock metadata"),
            );
        }

        for dep in &lock.dependencies {
            if let DependencyPathSelection::Filter(paths) = &allowed_paths {
                if !paths.contains(&dep.path) {
                    continue;
                }
            }

            let source_root = resolve_locked_path(&lock_root, &dep.path);
            if source_root == project_root {
                continue;
            }

            source_roots.insert(source_root.clone());
            let cache_root = cache_path_for_dep(&lock_root, dep);
            if options.offline {
                if !cache_root.exists() {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2108",
                            format!("offline cache entry missing for dependency '{}'", dep.name),
                            &lock_path.to_string_lossy(),
                            Span::new(0, 0),
                        )
                        .with_help("run `aic lock` online to populate the dependency cache"),
                    );
                    continue;
                }
                let cache_checksum = compute_package_checksum(&cache_root)?;
                if cache_checksum != dep.checksum {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2109",
                            format!(
                                "offline cache checksum mismatch for dependency '{}': expected {}, found {}",
                                dep.name, dep.checksum, cache_checksum
                            ),
                            &cache_root.to_string_lossy(),
                            Span::new(0, 0),
                        )
                        .with_help("run `aic lock` online to refresh the corrupted cache entry"),
                    );
                    continue;
                }
                roots.insert(cache_root);
            } else {
                if !source_root.exists() {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2107",
                            format!(
                                "dependency '{}' not found at '{}'",
                                dep.name,
                                source_root.display()
                            ),
                            &lock_path.to_string_lossy(),
                            Span::new(0, 0),
                        )
                        .with_help("ensure dependency paths in aic.lock still exist or regenerate lockfile"),
                    );
                    continue;
                }
                let current_checksum = compute_package_checksum(&source_root)?;
                if current_checksum != dep.checksum {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2107",
                            format!(
                                "checksum mismatch for dependency '{}': expected {}, found {}",
                                dep.name, dep.checksum, current_checksum
                            ),
                            &source_root.to_string_lossy(),
                            Span::new(0, 0),
                        )
                        .with_help("run `aic lock` if this change is intentional"),
                    );
                    continue;
                }
                sync_cache_entry(&source_root, &cache_root)?;
                roots.insert(source_root);
            }
        }
    } else {
        if options.offline {
            diagnostics.push(
                Diagnostic::error(
                    "E2108",
                    "offline mode requires an existing aic.lock lockfile",
                    &lock_root.to_string_lossy(),
                    Span::new(0, 0),
                )
                .with_help("run `aic lock` online first"),
            );
        }
        let expected = if let Some(expected) = expected {
            expected
        } else {
            generate_lockfile_from_manifest(&project_root, &project_root, &manifest)?
        };
        let allowed_paths =
            workspace_dependency_paths_for_member(&expected, workspace_member_name.as_deref());
        for dep in &expected.dependencies {
            if let DependencyPathSelection::Filter(paths) = &allowed_paths {
                if !paths.contains(&dep.path) {
                    continue;
                }
            }
            let root = resolve_locked_path(&lock_root, &dep.path);
            if root == project_root {
                continue;
            }
            source_roots.insert(root.clone());
            if root.exists() {
                roots.insert(root);
            }
        }
    }

    Ok(DependencyContext {
        roots: roots.into_iter().collect(),
        source_roots: source_roots.into_iter().collect(),
        diagnostics,
        lockfile_used,
    })
}

#[derive(Debug, Clone)]
struct WorkspaceContext {
    plan: WorkspaceBuildPlan,
    member: WorkspaceBuildMember,
}

fn workspace_context_for_project(
    project_root: &Path,
) -> Result<Option<WorkspaceContext>, Diagnostic> {
    let Some(workspace_root) = find_workspace_root(project_root) else {
        return Ok(None);
    };
    let Some(plan) = workspace_build_plan(&workspace_root)? else {
        return Ok(None);
    };
    let Some(member) = plan
        .members
        .iter()
        .find(|member| member.root == project_root)
        .cloned()
    else {
        return Err(Diagnostic::error(
            "E2125",
            format!(
                "package '{}' is not declared in workspace members",
                project_root.display()
            ),
            &workspace_manifest_path(&workspace_root).to_string_lossy(),
            Span::new(0, 0),
        )
        .with_help("add this package path to `[workspace].members`"));
    };
    Ok(Some(WorkspaceContext { plan, member }))
}

enum DependencyPathSelection {
    All,
    Filter(BTreeSet<String>),
    MissingMemberMetadata,
}

fn workspace_dependency_paths_for_member(
    lock: &Lockfile,
    member_name: Option<&str>,
) -> DependencyPathSelection {
    let Some(member_name) = member_name else {
        return DependencyPathSelection::All;
    };
    let Some(workspace) = lock.workspace.as_ref() else {
        return DependencyPathSelection::All;
    };
    let Some(member) = workspace
        .members
        .iter()
        .find(|member| member.name == member_name)
    else {
        return DependencyPathSelection::MissingMemberMetadata;
    };
    DependencyPathSelection::Filter(member.dependency_paths.iter().cloned().collect())
}

pub fn generate_and_write_lockfile(project_root: &Path) -> anyhow::Result<PathBuf> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let lock = generate_lockfile(&project_root)?;
    let lock_path = lockfile_path(&project_root);
    let json = serde_json::to_string_pretty(&lock)?;
    fs::write(&lock_path, format!("{json}\n"))?;
    let lock_root = lock_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_root.clone());

    for dep in &lock.dependencies {
        let src = resolve_locked_path(&lock_root, &dep.path);
        if src.exists() {
            let cache = cache_path_for_dep(&lock_root, dep);
            sync_cache_entry(&src, &cache)?;
        }
    }

    Ok(lock_path)
}

pub fn generate_lockfile(project_root: &Path) -> anyhow::Result<Lockfile> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    if let Some(workspace_root) = find_workspace_root(&project_root) {
        if let Some(plan) = workspace_build_plan(&workspace_root)
            .map_err(|diag| anyhow::anyhow!("error[{}]: {}", diag.code, diag.message))?
        {
            if project_root == workspace_root
                || plan
                    .members
                    .iter()
                    .any(|member| member.root == project_root)
            {
                return generate_workspace_lockfile_from_plan(&plan)
                    .map_err(|diag| anyhow::anyhow!("error[{}]: {}", diag.code, diag.message));
            }
        }
    }

    let manifest = read_manifest(&project_root)?
        .ok_or_else(|| anyhow::anyhow!("missing aic.toml in {}", project_root.display()))?;
    generate_lockfile_from_manifest(&project_root, &project_root, &manifest)
}

pub fn read_manifest(project_root: &Path) -> anyhow::Result<Option<Manifest>> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let path = project_root.join("aic.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    Ok(Some(parse_manifest(&text, &path)?))
}

pub fn metrics_thresholds_for_input(input: &Path) -> anyhow::Result<MetricsThresholds> {
    let mut current = if input.is_dir() {
        input.to_path_buf()
    } else {
        input
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if current.as_os_str().is_empty() {
        current = PathBuf::from(".");
    }
    current = canonical_or_self(current);

    loop {
        if let Some(manifest) = read_manifest(&current)? {
            return Ok(manifest.metrics);
        }
        if !current.pop() {
            break;
        }
    }
    Ok(MetricsThresholds::default())
}

pub fn native_link_config(project_root: &Path) -> anyhow::Result<NativeLinkConfig> {
    let Some(manifest) = read_manifest(project_root)? else {
        return Ok(NativeLinkConfig::default());
    };
    Ok(manifest.native)
}

pub fn lockfile_path(project_root: &Path) -> PathBuf {
    effective_lock_root(project_root).join(LOCKFILE_NAME)
}

fn effective_lock_root(project_root: &Path) -> PathBuf {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let Some(workspace_root) = find_workspace_root(&project_root) else {
        return project_root;
    };

    match workspace_build_plan(&workspace_root) {
        Ok(Some(plan)) => {
            if project_root == workspace_root
                || plan
                    .members
                    .iter()
                    .any(|member| member.root == project_root)
            {
                workspace_root
            } else {
                project_root
            }
        }
        _ => workspace_root,
    }
}

fn read_lockfile(project_root: &Path) -> anyhow::Result<Option<Lockfile>> {
    let path = lockfile_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let mut lock = serde_json::from_str::<Lockfile>(&text)
        .map_err(|err| anyhow::anyhow!("invalid lockfile '{}': {}", path.display(), err))?;
    match lock.schema_version {
        1 => {
            // Schema v1 did not include dependency traceability metadata.
            lock.schema_version = LOCKFILE_SCHEMA_VERSION;
        }
        LOCKFILE_SCHEMA_VERSION => {}
        _ => {
            anyhow::bail!(
                "unsupported lockfile schema version {} in {}",
                lock.schema_version,
                path.display()
            );
        }
    }
    Ok(Some(lock))
}

fn generate_workspace_lockfile_from_plan(
    plan: &WorkspaceBuildPlan,
) -> Result<Lockfile, Diagnostic> {
    let mut dependency_map = BTreeMap::<String, LockedDependency>::new();
    let mut members = Vec::new();

    for member in &plan.members {
        let Some(member_manifest) = read_manifest(&member.root).map_err(|err| {
            Diagnostic::error(
                "E2125",
                format!(
                    "failed to read workspace member manifest '{}': {err}",
                    member.root.display()
                ),
                &workspace_manifest_path(&plan.root).to_string_lossy(),
                Span::new(0, 0),
            )
            .with_help("workspace members must have valid aic.toml manifests")
        })?
        else {
            return Err(Diagnostic::error(
                "E2125",
                format!(
                    "workspace member '{}' is missing aic.toml",
                    member.root.display()
                ),
                &workspace_manifest_path(&plan.root).to_string_lossy(),
                Span::new(0, 0),
            )
            .with_help("add aic.toml to the package directory or remove it from workspace"));
        };

        let member_lock =
            generate_lockfile_from_manifest(&plan.root, &member.root, &member_manifest).map_err(
                |err| {
                    Diagnostic::error(
                        "E2125",
                        format!(
                    "failed to compute lockfile dependencies for workspace member '{}': {err}",
                    member.name
                ),
                        &workspace_manifest_path(&plan.root).to_string_lossy(),
                        Span::new(0, 0),
                    )
                    .with_help("check package dependency paths and manifest syntax")
                },
            )?;

        let mut dependency_paths = member_lock
            .dependencies
            .iter()
            .map(|dep| dep.path.clone())
            .collect::<Vec<_>>();
        dependency_paths.sort();
        dependency_paths.dedup();

        for dep in member_lock.dependencies {
            dependency_map.entry(dep.path.clone()).or_insert(dep);
        }

        let mut workspace_dependencies = member.workspace_dependencies.clone();
        workspace_dependencies.sort();
        workspace_dependencies.dedup();

        members.push(LockedWorkspaceMember {
            name: member.name.clone(),
            path: display_lock_path(&plan.root, &member.root),
            main: member.main.clone(),
            workspace_dependencies,
            dependency_paths,
        });
    }

    members.sort();

    let dependencies = dependency_map
        .into_values()
        .collect::<Vec<LockedDependency>>();

    let package = plan
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_string();

    Ok(Lockfile {
        schema_version: LOCKFILE_SCHEMA_VERSION,
        package,
        dependencies,
        workspace: Some(WorkspaceLockMetadata { members }),
    })
}

fn generate_lockfile_from_manifest(
    project_root: &Path,
    manifest_root: &Path,
    manifest: &Manifest,
) -> anyhow::Result<Lockfile> {
    let mut visited = BTreeSet::new();
    let mut dependencies = Vec::new();

    let mut deps = manifest.dependencies.clone();
    deps.sort();
    for dep in deps {
        let dep_root = canonical_or_self(manifest_root.join(&dep.path));
        collect_dependency_nodes(
            project_root,
            dep_root,
            dep.name,
            dep.resolved_version,
            dep.source_provenance,
            &mut visited,
            &mut dependencies,
        )?;
    }

    dependencies.sort();
    dependencies.dedup();

    Ok(Lockfile {
        schema_version: LOCKFILE_SCHEMA_VERSION,
        package: manifest.package_name.clone(),
        dependencies,
        workspace: None,
    })
}

fn collect_dependency_nodes(
    project_root: &Path,
    dep_root: PathBuf,
    fallback_name: String,
    resolved_version: Option<String>,
    source_provenance: Option<String>,
    visited: &mut BTreeSet<PathBuf>,
    dependencies: &mut Vec<LockedDependency>,
) -> anyhow::Result<()> {
    let dep_root = canonical_or_self(dep_root);
    if !visited.insert(dep_root.clone()) {
        return Ok(());
    }

    let dep_manifest = read_manifest(&dep_root)?;
    let dep_package_name = dep_manifest
        .as_ref()
        .map(|m| m.package_name.clone())
        .unwrap_or(fallback_name);

    let checksum = compute_package_checksum(&dep_root)?;
    let rel_path = display_lock_path(project_root, &dep_root);

    dependencies.push(LockedDependency {
        name: dep_package_name,
        path: rel_path,
        checksum,
        resolved_version,
        source_provenance,
    });

    if let Some(manifest) = dep_manifest {
        let mut children = manifest.dependencies;
        children.sort();
        for child in children {
            let child_root = canonical_or_self(dep_root.join(&child.path));
            collect_dependency_nodes(
                project_root,
                child_root,
                child.name,
                child.resolved_version,
                child.source_provenance,
                visited,
                dependencies,
            )?;
        }
    }

    Ok(())
}

fn parse_manifest(text: &str, path: &Path) -> anyhow::Result<Manifest> {
    let mut section = String::new();
    let mut package_name: Option<String> = None;
    let mut main: Option<String> = None;
    let mut dependencies = Vec::new();
    let mut native = NativeLinkConfig::default();
    let mut metrics = MetricsThresholds::default();

    for (line_no, raw_line) in text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };

        let key = raw_key.trim();
        let value = raw_value.trim();

        if section == "package" {
            match key {
                "name" => package_name = Some(parse_string(value, path, line_no + 1)?),
                "main" => main = Some(parse_string(value, path, line_no + 1)?),
                _ => {}
            }
            continue;
        }

        if section == "dependencies" {
            let (dep_path, resolved_version, source_provenance) = if value.starts_with('{') {
                parse_inline_dependency_fields(value, path, line_no + 1)?
            } else {
                (parse_string(value, path, line_no + 1)?, None, None)
            };
            dependencies.push(ManifestDependency {
                name: key.to_string(),
                path: dep_path,
                resolved_version,
                source_provenance,
            });
            continue;
        }

        if section == "native" {
            match key {
                "libs" => native.libs = parse_string_list(value, path, line_no + 1)?,
                "search" | "search_paths" => {
                    native.search_paths = parse_string_list(value, path, line_no + 1)?
                }
                "objects" => native.objects = parse_string_list(value, path, line_no + 1)?,
                _ => {}
            }
            continue;
        }

        if section == "metrics" {
            match key {
                "max_cyclomatic" => {
                    metrics.max_cyclomatic = Some(parse_u32(value, path, line_no + 1)?)
                }
                "max_cognitive" => {
                    metrics.max_cognitive = Some(parse_u32(value, path, line_no + 1)?)
                }
                "max_lines" => metrics.max_lines = Some(parse_u32(value, path, line_no + 1)?),
                "max_params" => metrics.max_params = Some(parse_u32(value, path, line_no + 1)?),
                "max_nesting_depth" => {
                    metrics.max_nesting_depth = Some(parse_u32(value, path, line_no + 1)?)
                }
                _ => {}
            }
        }
    }

    dependencies.sort();
    dependencies.dedup();
    native.libs.sort();
    native.libs.dedup();
    native.search_paths.sort();
    native.search_paths.dedup();
    native.objects.sort();
    native.objects.dedup();

    let package_name = package_name.unwrap_or_else(|| {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("package")
            .to_string()
    });

    Ok(Manifest {
        package_name,
        main: main.unwrap_or_else(|| "src/main.aic".to_string()),
        dependencies,
        native,
        metrics,
    })
}

fn parse_workspace_manifest(text: &str, path: &Path) -> anyhow::Result<WorkspaceManifest> {
    let mut section = String::new();
    let mut members: Vec<String> = Vec::new();

    for (line_no, raw_line) in text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }

        if section != "workspace" {
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let value = raw_value.trim();
        if key == "members" {
            members = parse_string_list(value, path, line_no + 1)?;
        }
    }

    members.sort();
    members.dedup();
    Ok(WorkspaceManifest { members })
}

fn parse_inline_dependency_fields(
    value: &str,
    path: &Path,
    line_no: usize,
) -> anyhow::Result<(String, Option<String>, Option<String>)> {
    let inner = value.trim();
    if !inner.starts_with('{') || !inner.ends_with('}') {
        anyhow::bail!(
            "invalid dependency table at {}:{} (expected {{ path = \"...\", ... }})",
            path.display(),
            line_no
        );
    }
    let inner = inner[1..inner.len() - 1].trim();
    let mut dep_path = None;
    let mut resolved_version = None;
    let mut source_provenance = None;
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let parsed = parse_string(value.trim(), path, line_no)?;
        match key {
            "path" => dep_path = Some(parsed),
            "resolved_version" => resolved_version = Some(parsed),
            "source_provenance" => source_provenance = Some(parsed),
            _ => {}
        }
    }
    let Some(dep_path) = dep_path else {
        anyhow::bail!(
            "dependency table missing `path` at {}:{}",
            path.display(),
            line_no
        );
    };
    Ok((dep_path, resolved_version, source_provenance))
}

fn parse_string(value: &str, path: &Path, line_no: usize) -> anyhow::Result<String> {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return Ok(value[1..value.len() - 1].to_string());
    }
    anyhow::bail!("expected quoted string at {}:{}", path.display(), line_no)
}

fn parse_u32(value: &str, path: &Path, line_no: usize) -> anyhow::Result<u32> {
    value.trim().parse::<u32>().map_err(|_| {
        anyhow::anyhow!(
            "expected unsigned integer at {}:{}",
            path.display(),
            line_no
        )
    })
}

fn parse_string_list(value: &str, path: &Path, line_no: usize) -> anyhow::Result<Vec<String>> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        anyhow::bail!(
            "expected string array literal at {}:{} (for example [\"foo\", \"bar\"])",
            path.display(),
            line_no
        );
    }
    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in inner.split(',') {
        let item = parse_string(part.trim(), path, line_no)?;
        out.push(item);
    }
    Ok(out)
}

fn compute_package_checksum(root: &Path) -> anyhow::Result<String> {
    let mut files = Vec::new();
    collect_checksum_files(root, root, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for rel in files {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        let content = fs::read(root.join(&rel))?;
        hasher.update(content);
        hasher.update([0]);
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{:02x}", byte);
    }

    Ok(format!("sha256:{hex}"))
}

pub fn compute_package_checksum_for_path(root: &Path) -> anyhow::Result<String> {
    compute_package_checksum(root)
}

fn collect_checksum_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if path.is_dir() {
            if name == ".git" || name == "target" || name == CACHE_DIR_NAME {
                continue;
            }
            collect_checksum_files(root, &path, out)?;
            continue;
        }

        if name == "aic.toml" || path.extension().and_then(|e| e.to_str()) == Some("aic") {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

fn resolve_locked_path(project_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}

fn display_lock_path(project_root: &Path, dep_root: &Path) -> String {
    let normalized = dep_root.to_string_lossy().replace('\\', "/");
    if let Ok(rel) = dep_root.strip_prefix(project_root) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        normalized
    }
}

fn cache_path_for_dep(project_root: &Path, dep: &LockedDependency) -> PathBuf {
    let short = dep
        .checksum
        .strip_prefix("sha256:")
        .unwrap_or(&dep.checksum)
        .chars()
        .take(16)
        .collect::<String>();
    project_root
        .join(CACHE_DIR_NAME)
        .join(format!("{}-{}", dep.name, short))
}

fn sync_cache_entry(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    copy_tree(src, dst)
}

fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    let mut entries = fs::read_dir(src)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let src_path = entry.path();
        let name = src_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == ".git" || name == "target" || name == CACHE_DIR_NAME {
            continue;
        }
        let dst_path = dst.join(name);
        if src_path.is_dir() {
            copy_tree(&src_path, &dst_path)?;
        } else {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn canonical_or_self(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        compute_package_checksum, generate_and_write_lockfile, generate_lockfile, lockfile_path,
        metrics_thresholds_for_input, native_link_config, read_lockfile, read_manifest,
        read_workspace_manifest, resolve_dependency_context, workspace_build_plan, PackageOptions,
        LOCKFILE_SCHEMA_VERSION,
    };

    fn write_workspace_demo(root: &std::path::Path) {
        fs::create_dir_all(root.join("packages/util/src")).expect("mkdir util");
        fs::create_dir_all(root.join("packages/app/src")).expect("mkdir app");
        fs::create_dir_all(root.join("packages/tool/src")).expect("mkdir tool");

        fs::write(
            root.join("aic.workspace.toml"),
            "[workspace]\nmembers = [\"packages/app\", \"packages/tool\", \"packages/util\"]\n",
        )
        .expect("write workspace manifest");

        fs::write(
            root.join("packages/util/aic.toml"),
            "[package]\nname = \"util_pkg\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write util manifest");
        fs::write(
            root.join("packages/util/src/main.aic"),
            "module util_pkg.main;\nfn value() -> Int { 41 }\n",
        )
        .expect("write util source");

        fs::write(
            root.join("packages/tool/aic.toml"),
            "[package]\nname = \"tool_pkg\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write tool manifest");
        fs::write(
            root.join("packages/tool/src/main.aic"),
            "module tool_pkg.main;\nfn ping() -> Int { 1 }\n",
        )
        .expect("write tool source");

        fs::write(
            root.join("packages/app/aic.toml"),
            concat!(
                "[package]\n",
                "name = \"app_pkg\"\n",
                "main = \"src/main.aic\"\n\n",
                "[dependencies]\n",
                "util_pkg = { path = \"../util\" }\n",
            ),
        )
        .expect("write app manifest");
        fs::write(
            root.join("packages/app/src/main.aic"),
            "module app_pkg.main;\nimport util_pkg.main;\nfn main() -> Int { value() }\n",
        )
        .expect("write app source");
    }

    #[test]
    fn parses_manifest_dependencies() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("aic.toml"),
            r#"[package]
name = "app"
main = "src/main.aic"

[dependencies]
util = { path = "deps/util" }
net = "deps/net"
http = { path = "deps/http", resolved_version = "1.2.3", source_provenance = "registry_root=/tmp/registry;index=/tmp/registry/index/http.json" }
"#,
        )
        .expect("write manifest");

        let manifest = read_manifest(dir.path())
            .expect("manifest")
            .expect("manifest present");
        assert_eq!(manifest.package_name, "app");
        assert_eq!(manifest.main, "src/main.aic");
        assert_eq!(manifest.dependencies.len(), 3);
        let http_dep = manifest
            .dependencies
            .iter()
            .find(|dep| dep.name == "http")
            .expect("http dependency");
        assert_eq!(http_dep.path, "deps/http");
        assert_eq!(http_dep.resolved_version.as_deref(), Some("1.2.3"));
        assert_eq!(
            http_dep.source_provenance.as_deref(),
            Some("registry_root=/tmp/registry;index=/tmp/registry/index/http.json")
        );
    }

    #[test]
    fn parses_native_link_configuration() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("aic.toml"),
            r#"[package]
name = "ffi_app"
main = "src/main.aic"

[native]
libs = ["m", "z"]
search_paths = ["native/lib"]
objects = ["native/libextra.a"]
"#,
        )
        .expect("write manifest");
        let native = native_link_config(dir.path()).expect("native config");
        assert_eq!(native.libs, vec!["m".to_string(), "z".to_string()]);
        assert_eq!(native.search_paths, vec!["native/lib".to_string()]);
        assert_eq!(native.objects, vec!["native/libextra.a".to_string()]);
    }

    #[test]
    fn parses_metrics_threshold_configuration() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("aic.toml"),
            r#"[package]
name = "metrics_app"
main = "src/main.aic"

[metrics]
max_cyclomatic = 9
max_cognitive = 14
max_lines = 30
max_params = 3
max_nesting_depth = 2
"#,
        )
        .expect("write manifest");
        let manifest = read_manifest(dir.path())
            .expect("manifest")
            .expect("manifest present");
        assert_eq!(manifest.metrics.max_cyclomatic, Some(9));
        assert_eq!(manifest.metrics.max_cognitive, Some(14));
        assert_eq!(manifest.metrics.max_lines, Some(30));
        assert_eq!(manifest.metrics.max_params, Some(3));
        assert_eq!(manifest.metrics.max_nesting_depth, Some(2));
    }

    #[test]
    fn resolves_metrics_thresholds_from_nearest_manifest() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
        fs::write(
            dir.path().join("aic.toml"),
            r#"[package]
name = "metrics_app"
main = "src/main.aic"

[metrics]
max_cyclomatic = 11
"#,
        )
        .expect("write manifest");
        fs::write(
            dir.path().join("src/main.aic"),
            "fn main() -> Int { if true { 1 } else { 0 } }\n",
        )
        .expect("write source");

        let thresholds = metrics_thresholds_for_input(&dir.path().join("src/main.aic"))
            .expect("metrics thresholds");
        assert_eq!(thresholds.max_cyclomatic, Some(11));
    }

    #[test]
    fn parses_workspace_manifest_members() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("aic.workspace.toml"),
            "[workspace]\nmembers = [\"packages/app\", \"packages/util\"]\n",
        )
        .expect("write workspace manifest");
        let manifest = read_workspace_manifest(dir.path())
            .expect("workspace manifest")
            .expect("workspace manifest present");
        assert_eq!(
            manifest.members,
            vec!["packages/app".to_string(), "packages/util".to_string()]
        );
    }

    #[test]
    fn workspace_build_plan_is_deterministic_and_topological() {
        let dir = tempdir().expect("tempdir");
        write_workspace_demo(dir.path());

        let plan_a = workspace_build_plan(dir.path())
            .expect("workspace plan")
            .expect("workspace present");
        let plan_b = workspace_build_plan(dir.path())
            .expect("workspace plan")
            .expect("workspace present");
        assert_eq!(plan_a, plan_b);
        let names = plan_a
            .members
            .iter()
            .map(|member| member.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "tool_pkg".to_string(),
                "util_pkg".to_string(),
                "app_pkg".to_string()
            ]
        );
    }

    #[test]
    fn workspace_cycle_detection_is_actionable() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("packages/a/src")).expect("mkdir a");
        fs::create_dir_all(dir.path().join("packages/b/src")).expect("mkdir b");
        fs::write(
            dir.path().join("aic.workspace.toml"),
            "[workspace]\nmembers = [\"packages/a\", \"packages/b\"]\n",
        )
        .expect("write workspace manifest");

        fs::write(
            dir.path().join("packages/a/aic.toml"),
            concat!(
                "[package]\nname = \"a_pkg\"\nmain = \"src/main.aic\"\n\n",
                "[dependencies]\nb_pkg = { path = \"../b\" }\n",
            ),
        )
        .expect("write a manifest");
        fs::write(
            dir.path().join("packages/a/src/main.aic"),
            "module a_pkg.main;\nfn main() -> Int { 0 }\n",
        )
        .expect("write a source");

        fs::write(
            dir.path().join("packages/b/aic.toml"),
            concat!(
                "[package]\nname = \"b_pkg\"\nmain = \"src/main.aic\"\n\n",
                "[dependencies]\na_pkg = { path = \"../a\" }\n",
            ),
        )
        .expect("write b manifest");
        fs::write(
            dir.path().join("packages/b/src/main.aic"),
            "module b_pkg.main;\nfn main() -> Int { 0 }\n",
        )
        .expect("write b source");

        let diag = workspace_build_plan(dir.path()).expect_err("workspace cycle");
        assert_eq!(diag.code, "E2126");
        assert!(diag.message.contains("a_pkg -> b_pkg -> a_pkg"));
    }

    #[test]
    fn workspace_lockfile_is_shared_and_deterministic() {
        let dir = tempdir().expect("tempdir");
        write_workspace_demo(dir.path());

        let lock_a = generate_lockfile(dir.path()).expect("lock a");
        let lock_b = generate_lockfile(dir.path()).expect("lock b");
        assert_eq!(lock_a, lock_b);
        assert!(lock_a.workspace.is_some());

        let lock_path = generate_and_write_lockfile(&dir.path().join("packages/app"))
            .expect("write workspace lock");
        let lock_path_canonical = fs::canonicalize(&lock_path).expect("canonical lock path");
        let expected_canonical =
            fs::canonicalize(dir.path().join("aic.lock")).expect("canonical expected lock path");
        assert_eq!(lock_path_canonical, expected_canonical);
        assert!(!dir.path().join("packages/app/aic.lock").exists());
    }

    #[test]
    fn workspace_member_context_uses_shared_lockfile_dependencies() {
        let dir = tempdir().expect("tempdir");
        write_workspace_demo(dir.path());
        generate_and_write_lockfile(dir.path()).expect("write workspace lock");

        let app_root = dir.path().join("packages/app");
        let context = resolve_dependency_context(&app_root, PackageOptions::default())
            .expect("resolve workspace app");
        assert!(
            context.diagnostics.is_empty(),
            "diags={:#?}",
            context.diagnostics
        );

        let util_root = dir.path().join("packages/util");
        let tool_root = dir.path().join("packages/tool");
        let canonical_sources = context
            .source_roots
            .iter()
            .map(|root| fs::canonicalize(root).unwrap_or_else(|_| root.clone()))
            .collect::<Vec<_>>();
        let util_canonical = fs::canonicalize(&util_root).expect("canonical util");
        let tool_canonical = fs::canonicalize(&tool_root).expect("canonical tool");
        assert!(canonical_sources.contains(&util_canonical));
        assert!(!canonical_sources.contains(&tool_canonical));
        let lock_canonical = fs::canonicalize(lockfile_path(&app_root)).expect("canonical lock");
        let expected_lock = fs::canonicalize(dir.path().join("aic.lock")).expect("expected lock");
        assert_eq!(lock_canonical, expected_lock);
    }

    #[test]
    fn lockfile_generation_is_deterministic() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
        fs::create_dir_all(dir.path().join("deps/util/src")).expect("mkdir dep");

        fs::write(
            dir.path().join("aic.toml"),
            "[package]\nname = \"app\"\nmain = \"src/main.aic\"\n\n[dependencies]\nutil = { path = \"deps/util\", resolved_version = \"1.0.0\", source_provenance = \"registry_root=/tmp/registry;index=/tmp/registry/index/util.json\" }\n",
        )
        .expect("write app manifest");
        fs::write(dir.path().join("src/main.aic"), "fn main() -> Int { 0 }\n").expect("write app");

        fs::write(
            dir.path().join("deps/util/aic.toml"),
            "[package]\nname = \"util\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write dep manifest");
        fs::write(
            dir.path().join("deps/util/src/main.aic"),
            "module util.main;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write dep source");

        let lock1 = generate_lockfile(dir.path()).expect("lockfile");
        let lock2 = generate_lockfile(dir.path()).expect("lockfile");
        assert_eq!(lock1, lock2);
        assert_eq!(lock1.schema_version, LOCKFILE_SCHEMA_VERSION);
        let util = lock1
            .dependencies
            .iter()
            .find(|dep| dep.name == "util")
            .expect("util dependency");
        assert_eq!(util.resolved_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            util.source_provenance.as_deref(),
            Some("registry_root=/tmp/registry;index=/tmp/registry/index/util.json")
        );
    }

    #[test]
    fn read_lockfile_migrates_schema_version_one() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("aic.lock"),
            r#"{
  "schema_version": 1,
  "package": "app",
  "dependencies": [
    {
      "name": "util",
      "path": "deps/util",
      "checksum": "sha256:1234"
    }
  ]
}
"#,
        )
        .expect("write lockfile");

        let lock = read_lockfile(dir.path())
            .expect("read lockfile")
            .expect("lockfile present");
        assert_eq!(lock.schema_version, LOCKFILE_SCHEMA_VERSION);
        assert_eq!(lock.dependencies.len(), 1);
        assert_eq!(lock.dependencies[0].resolved_version, None);
        assert_eq!(lock.dependencies[0].source_provenance, None);
    }

    #[test]
    fn detects_lockfile_checksum_drift() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
        fs::create_dir_all(dir.path().join("deps/util/src")).expect("mkdir dep");

        fs::write(
            dir.path().join("aic.toml"),
            "[package]\nname = \"app\"\nmain = \"src/main.aic\"\n\n[dependencies]\nutil = { path = \"deps/util\" }\n",
        )
        .expect("write app manifest");
        fs::write(dir.path().join("src/main.aic"), "fn main() -> Int { 0 }\n").expect("write app");

        fs::write(
            dir.path().join("deps/util/aic.toml"),
            "[package]\nname = \"util\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write dep manifest");
        fs::write(
            dir.path().join("deps/util/src/main.aic"),
            "module util.main;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write dep source");

        generate_and_write_lockfile(dir.path()).expect("write lockfile");

        fs::write(
            dir.path().join("deps/util/src/main.aic"),
            "module util.main;\nfn answer() -> Int { 7 }\n",
        )
        .expect("tamper dep source");

        let context = resolve_dependency_context(dir.path(), PackageOptions::default())
            .expect("resolve context");
        assert!(context.diagnostics.iter().any(|d| d.code == "E2107"));
    }

    #[test]
    fn offline_mode_uses_cache() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
        fs::create_dir_all(dir.path().join("deps/util/src")).expect("mkdir dep");

        fs::write(
            dir.path().join("aic.toml"),
            "[package]\nname = \"app\"\nmain = \"src/main.aic\"\n\n[dependencies]\nutil = { path = \"deps/util\" }\n",
        )
        .expect("write app manifest");
        fs::write(dir.path().join("src/main.aic"), "fn main() -> Int { 0 }\n").expect("write app");

        fs::write(
            dir.path().join("deps/util/aic.toml"),
            "[package]\nname = \"util\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write dep manifest");
        fs::write(
            dir.path().join("deps/util/src/main.aic"),
            "module util.main;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write dep source");

        generate_and_write_lockfile(dir.path()).expect("write lockfile");

        fs::remove_dir_all(dir.path().join("deps/util")).expect("remove source dependency");

        let context = resolve_dependency_context(dir.path(), PackageOptions { offline: true })
            .expect("resolve context");
        assert!(
            context.diagnostics.is_empty(),
            "diags={:#?}",
            context.diagnostics
        );
        assert!(!context.roots.is_empty());
    }

    #[test]
    fn online_mode_recovers_corrupted_cache() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
        fs::create_dir_all(dir.path().join("deps/util/src")).expect("mkdir dep");

        fs::write(
            dir.path().join("aic.toml"),
            "[package]\nname = \"app\"\nmain = \"src/main.aic\"\n\n[dependencies]\nutil = { path = \"deps/util\" }\n",
        )
        .expect("write app manifest");
        fs::write(dir.path().join("src/main.aic"), "fn main() -> Int { 0 }\n").expect("write app");

        fs::write(
            dir.path().join("deps/util/aic.toml"),
            "[package]\nname = \"util\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write dep manifest");
        fs::write(
            dir.path().join("deps/util/src/main.aic"),
            "module util.main;\nfn answer() -> Int { 42 }\n",
        )
        .expect("write dep source");

        generate_and_write_lockfile(dir.path()).expect("write lockfile");

        let cache_root = dir.path().join(".aic-cache");
        let mut entries = fs::read_dir(&cache_root)
            .expect("read cache")
            .collect::<Result<Vec<_>, _>>()
            .expect("cache entries");
        entries.sort_by_key(|e| e.path());
        let first_cache = entries.first().expect("cache entry").path();
        fs::write(
            first_cache.join("src/main.aic"),
            "module util.main;\nfn answer() -> Int { 0 }\n",
        )
        .expect("corrupt cache source");

        let offline_context =
            resolve_dependency_context(dir.path(), PackageOptions { offline: true })
                .expect("offline context");
        assert!(offline_context
            .diagnostics
            .iter()
            .any(|d| d.code == "E2109"));

        let online_context = resolve_dependency_context(dir.path(), PackageOptions::default())
            .expect("online context");
        assert!(
            online_context.diagnostics.iter().all(|d| d.code != "E2109"),
            "online context should refresh cache"
        );

        let offline_context =
            resolve_dependency_context(dir.path(), PackageOptions { offline: true })
                .expect("offline context");
        assert!(
            offline_context.diagnostics.is_empty(),
            "offline context should pass after refresh: {:#?}",
            offline_context.diagnostics
        );
    }

    #[test]
    fn checksum_changes_when_source_changes() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
        fs::write(dir.path().join("src/main.aic"), "fn main() -> Int { 0 }\n")
            .expect("write source");
        let a = compute_package_checksum(dir.path()).expect("checksum a");
        fs::write(dir.path().join("src/main.aic"), "fn main() -> Int { 1 }\n")
            .expect("write source");
        let b = compute_package_checksum(dir.path()).expect("checksum b");
        assert_ne!(a, b);
    }
}
