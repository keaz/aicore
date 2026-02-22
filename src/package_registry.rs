use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::package_workflow::{compute_package_checksum_for_path, generate_and_write_lockfile};
use crate::span::Span;

const ENV_REGISTRY_ROOT: &str = "AIC_PKG_REGISTRY";
const REGISTRY_INDEX_DIR: &str = "index";
const REGISTRY_PACKAGES_DIR: &str = "packages";
const DEPS_DIR: &str = "deps";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishResult {
    pub package: String,
    pub version: String,
    pub checksum: String,
    pub registry_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub package: String,
    pub latest: String,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPackage {
    pub package: String,
    pub requirement: String,
    pub version: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallResult {
    pub project_root: String,
    pub installed: Vec<InstalledPackage>,
    pub lockfile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RegistryIndex {
    package: String,
    releases: Vec<RegistryRelease>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RegistryRelease {
    version: String,
    checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSpec {
    package: String,
    requirement_raw: String,
    requirement: VersionReq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageMeta {
    name: String,
    version: SemVer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl SemVer {
    fn parse(input: &str) -> Result<Self, String> {
        let text = input.trim();
        let mut parts = text.split('.');
        let Some(major_raw) = parts.next() else {
            return Err(format!("invalid semantic version '{input}'"));
        };
        let Some(minor_raw) = parts.next() else {
            return Err(format!("invalid semantic version '{input}'"));
        };
        let Some(patch_raw) = parts.next() else {
            return Err(format!("invalid semantic version '{input}'"));
        };
        if parts.next().is_some() {
            return Err(format!("invalid semantic version '{input}'"));
        }

        let major = major_raw
            .parse::<u64>()
            .map_err(|_| format!("invalid semantic version '{input}'"))?;
        let minor = minor_raw
            .parse::<u64>()
            .map_err(|_| format!("invalid semantic version '{input}'"))?;
        let patch = patch_raw
            .parse::<u64>()
            .map_err(|_| format!("invalid semantic version '{input}'"))?;

        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    fn checked_next_major(self) -> Result<Self, String> {
        let major = self
            .major
            .checked_add(1)
            .ok_or_else(|| "semantic version overflow while resolving requirement".to_string())?;
        Ok(Self {
            major,
            minor: 0,
            patch: 0,
        })
    }

    fn checked_next_minor(self) -> Result<Self, String> {
        let minor = self
            .minor
            .checked_add(1)
            .ok_or_else(|| "semantic version overflow while resolving requirement".to_string())?;
        Ok(Self {
            major: self.major,
            minor,
            patch: 0,
        })
    }

    fn checked_next_patch(self) -> Result<Self, String> {
        let patch = self
            .patch
            .checked_add(1)
            .ok_or_else(|| "semantic version overflow while resolving requirement".to_string())?;
        Ok(Self {
            major: self.major,
            minor: self.minor,
            patch,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Comparator {
    Eq(SemVer),
    Lt(SemVer),
    Lte(SemVer),
    Gt(SemVer),
    Gte(SemVer),
}

impl Comparator {
    fn matches(&self, version: SemVer) -> bool {
        match self {
            Comparator::Eq(v) => version == *v,
            Comparator::Lt(v) => version < *v,
            Comparator::Lte(v) => version <= *v,
            Comparator::Gt(v) => version > *v,
            Comparator::Gte(v) => version >= *v,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VersionReq {
    raw: String,
    comparators: Vec<Comparator>,
}

impl VersionReq {
    fn parse(input: &str) -> Result<Self, String> {
        let raw = input.trim();
        if raw.is_empty() || raw == "*" {
            return Ok(Self {
                raw: if raw.is_empty() {
                    "*".to_string()
                } else {
                    raw.to_string()
                },
                comparators: Vec::new(),
            });
        }

        if let Some(rest) = raw.strip_prefix('^') {
            let base = SemVer::parse(rest)?;
            let upper = if base.major > 0 {
                base.checked_next_major()?
            } else if base.minor > 0 {
                base.checked_next_minor()?
            } else {
                base.checked_next_patch()?
            };
            return Ok(Self {
                raw: raw.to_string(),
                comparators: vec![Comparator::Gte(base), Comparator::Lt(upper)],
            });
        }

        if let Some(rest) = raw.strip_prefix('~') {
            let base = SemVer::parse(rest)?;
            let upper = base.checked_next_minor()?;
            return Ok(Self {
                raw: raw.to_string(),
                comparators: vec![Comparator::Gte(base), Comparator::Lt(upper)],
            });
        }

        let mut comparators = Vec::new();
        let mut saw_operator = false;

        for token in raw.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }

            if let Some(rest) = token.strip_prefix(">=") {
                comparators.push(Comparator::Gte(SemVer::parse(rest)?));
                saw_operator = true;
            } else if let Some(rest) = token.strip_prefix("<=") {
                comparators.push(Comparator::Lte(SemVer::parse(rest)?));
                saw_operator = true;
            } else if let Some(rest) = token.strip_prefix('>') {
                comparators.push(Comparator::Gt(SemVer::parse(rest)?));
                saw_operator = true;
            } else if let Some(rest) = token.strip_prefix('<') {
                comparators.push(Comparator::Lt(SemVer::parse(rest)?));
                saw_operator = true;
            } else if let Some(rest) = token.strip_prefix('=') {
                comparators.push(Comparator::Eq(SemVer::parse(rest)?));
                saw_operator = true;
            } else {
                if saw_operator || raw.contains(',') {
                    return Err(format!("invalid version requirement '{raw}'"));
                }
                comparators.push(Comparator::Eq(SemVer::parse(token)?));
            }
        }

        if comparators.is_empty() {
            return Err(format!("invalid version requirement '{raw}'"));
        }

        Ok(Self {
            raw: raw.to_string(),
            comparators,
        })
    }

    fn matches(&self, version: SemVer) -> bool {
        self.comparators
            .iter()
            .all(|comparator| comparator.matches(version))
    }
}

fn diag(code: &str, message: impl Into<String>, file: &Path) -> Diagnostic {
    Diagnostic::error(
        code,
        message.into(),
        &file.to_string_lossy(),
        Span::new(0, 0),
    )
}

fn diag_with_help(
    code: &str,
    message: impl Into<String>,
    file: &Path,
    help: impl Into<String>,
) -> Diagnostic {
    diag(code, message, file).with_help(help)
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn canonical_or_self(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn registry_root(override_root: Option<&Path>) -> Result<PathBuf, Diagnostic> {
    if let Some(path) = override_root {
        return Ok(canonical_or_self(path.to_path_buf()));
    }

    if let Ok(path) = std::env::var(ENV_REGISTRY_ROOT) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(canonical_or_self(PathBuf::from(trimmed)));
        }
    }

    let Some(home) = std::env::var_os("HOME") else {
        return Err(diag_with_help(
            "E2116",
            "registry root is not configured",
            Path::new("."),
            format!(
                "set {ENV_REGISTRY_ROOT} or HOME to resolve the default local package registry"
            ),
        ));
    };

    Ok(canonical_or_self(
        PathBuf::from(home).join(".aic").join("registry"),
    ))
}

fn index_dir(root: &Path) -> PathBuf {
    root.join(REGISTRY_INDEX_DIR)
}

fn package_store_dir(root: &Path) -> PathBuf {
    root.join(REGISTRY_PACKAGES_DIR)
}

fn package_index_path(root: &Path, package: &str) -> PathBuf {
    index_dir(root).join(format!("{package}.json"))
}

fn package_version_path(root: &Path, package: &str, version: &str) -> PathBuf {
    package_store_dir(root).join(package).join(version)
}

fn ensure_registry_layout(root: &Path) -> Result<(), Diagnostic> {
    fs::create_dir_all(index_dir(root)).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("failed to create registry index dir: {err}"),
            &index_dir(root),
            "check write permissions for the configured registry root",
        )
    })?;
    fs::create_dir_all(package_store_dir(root)).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("failed to create registry package store dir: {err}"),
            &package_store_dir(root),
            "check write permissions for the configured registry root",
        )
    })?;
    Ok(())
}

fn parse_manifest_string(
    value: &str,
    manifest_path: &Path,
    line_no: usize,
) -> Result<String, Diagnostic> {
    let text = value.trim();
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        return Ok(text[1..text.len() - 1].to_string());
    }

    Err(diag_with_help(
        "E2111",
        format!(
            "expected quoted string at {}:{line_no}",
            manifest_path.display()
        ),
        manifest_path,
        "use TOML quoted string values in package metadata",
    ))
}

fn parse_package_meta(project_root: &Path) -> Result<PackageMeta, Diagnostic> {
    let manifest_path = project_root.join("aic.toml");
    let text = fs::read_to_string(&manifest_path).map_err(|err| {
        diag_with_help(
            "E2111",
            format!(
                "failed to read manifest '{}': {err}",
                manifest_path.display()
            ),
            &manifest_path,
            "ensure the project contains a readable aic.toml",
        )
    })?;

    let mut section = String::new();
    let mut name = None;
    let mut version = None;

    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }

        if section != "package" {
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };

        let key = raw_key.trim();
        let value = raw_value.trim();
        match key {
            "name" => name = Some(parse_manifest_string(value, &manifest_path, index + 1)?),
            "version" => version = Some(parse_manifest_string(value, &manifest_path, index + 1)?),
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        diag_with_help(
            "E2113",
            "manifest is missing [package].name",
            &manifest_path,
            "add `name = \"...\"` inside [package]",
        )
    })?;

    let version = version.ok_or_else(|| {
        diag_with_help(
            "E2113",
            "manifest is missing [package].version",
            &manifest_path,
            "add `version = \"MAJOR.MINOR.PATCH\"` inside [package]",
        )
    })?;

    let version = SemVer::parse(&version).map_err(|msg| {
        diag_with_help(
            "E2113",
            msg,
            &manifest_path,
            "use semantic versions like 1.2.3",
        )
    })?;

    Ok(PackageMeta { name, version })
}

fn read_index(path: &Path, package: &str) -> Result<RegistryIndex, Diagnostic> {
    if !path.exists() {
        return Ok(RegistryIndex {
            package: package.to_string(),
            releases: Vec::new(),
        });
    }

    let text = fs::read_to_string(path).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("failed to read registry index '{}': {err}", path.display()),
            path,
            "ensure registry index files are readable",
        )
    })?;

    let index: RegistryIndex = serde_json::from_str(&text).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("invalid registry index '{}': {err}", path.display()),
            path,
            "repair or remove the corrupt index file",
        )
    })?;

    if index.package != package {
        return Err(diag_with_help(
            "E2116",
            format!(
                "registry index '{}' has mismatched package '{}' (expected '{}')",
                path.display(),
                index.package,
                package
            ),
            path,
            "re-publish the package to regenerate a consistent index",
        ));
    }

    Ok(index)
}

fn write_index(path: &Path, mut index: RegistryIndex) -> Result<(), Diagnostic> {
    index
        .releases
        .sort_by(|a, b| compare_release_versions(&a.version, &b.version));
    index
        .releases
        .dedup_by(|a, b| a.version == b.version && a.checksum == b.checksum);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            diag_with_help(
                "E2116",
                format!(
                    "failed to create index parent dir '{}': {err}",
                    parent.display()
                ),
                parent,
                "check write permissions for the local registry",
            )
        })?;
    }

    let content = serde_json::to_string_pretty(&index).map_err(|err| {
        diag_with_help(
            "E2116",
            format!(
                "failed to serialize registry index '{}': {err}",
                path.display()
            ),
            path,
            "ensure registry metadata values are valid UTF-8",
        )
    })?;

    fs::write(path, format!("{content}\n")).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("failed to write registry index '{}': {err}", path.display()),
            path,
            "check write permissions for the configured registry root",
        )
    })?;

    Ok(())
}

fn compare_release_versions(a: &str, b: &str) -> Ordering {
    match (SemVer::parse(a), SemVer::parse(b)) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => Ordering::Greater,
        (Err(_), Ok(_)) => Ordering::Less,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), Diagnostic> {
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|err| {
            diag_with_help(
                "E2116",
                format!("failed to clear destination '{}': {err}", dst.display()),
                dst,
                "check destination directory permissions",
            )
        })?;
    }

    copy_tree_recursive(src, dst)
}

fn copy_tree_recursive(src: &Path, dst: &Path) -> Result<(), Diagnostic> {
    fs::create_dir_all(dst).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("failed to create directory '{}': {err}", dst.display()),
            dst,
            "check destination directory permissions",
        )
    })?;

    let mut entries = fs::read_dir(src)
        .map_err(|err| {
            diag_with_help(
                "E2116",
                format!("failed to read directory '{}': {err}", src.display()),
                src,
                "ensure package source is readable",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            diag_with_help(
                "E2116",
                format!("failed to scan directory '{}': {err}", src.display()),
                src,
                "ensure package source is readable",
            )
        })?;

    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let src_path = entry.path();
        let Some(name) = src_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if name == ".git" || name == "target" || name == ".aic-cache" {
            continue;
        }

        let dst_path = dst.join(name);
        if src_path.is_dir() {
            copy_tree_recursive(&src_path, &dst_path)?;
        } else {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    diag_with_help(
                        "E2116",
                        format!("failed to create directory '{}': {err}", parent.display()),
                        parent,
                        "check destination directory permissions",
                    )
                })?;
            }

            fs::copy(&src_path, &dst_path).map_err(|err| {
                diag_with_help(
                    "E2116",
                    format!(
                        "failed to copy '{}' -> '{}': {err}",
                        src_path.display(),
                        dst_path.display()
                    ),
                    &src_path,
                    "ensure package sources are readable and destination is writable",
                )
            })?;
        }
    }

    Ok(())
}

fn parse_inline_dependency_path(value: &str) -> Option<String> {
    let text = value.trim();
    if !text.starts_with('{') || !text.ends_with('}') {
        return None;
    }
    let inner = text[1..text.len() - 1].trim();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key.trim() == "path" {
            let raw = value.trim();
            if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
                return Some(raw[1..raw.len() - 1].to_string());
            }
        }
    }
    None
}

fn parse_existing_dependencies(text: &str) -> BTreeMap<String, String> {
    let mut section = String::new();
    let mut dependencies = BTreeMap::new();

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }

        if section != "dependencies" {
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };

        let key = raw_key.trim();
        let value = raw_value.trim();

        if let Some(path) = parse_inline_dependency_path(value) {
            dependencies.insert(key.to_string(), path);
            continue;
        }

        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            dependencies.insert(key.to_string(), value[1..value.len() - 1].to_string());
        }
    }

    dependencies
}

fn rewrite_dependencies_section(
    manifest_path: &Path,
    updates: &BTreeMap<String, String>,
) -> Result<(), Diagnostic> {
    let text = fs::read_to_string(manifest_path).map_err(|err| {
        diag_with_help(
            "E2111",
            format!(
                "failed to read manifest '{}': {err}",
                manifest_path.display()
            ),
            manifest_path,
            "ensure the project contains a readable aic.toml",
        )
    })?;

    let mut merged = parse_existing_dependencies(&text);
    merged.extend(updates.clone());

    let mut lines = text
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }

    let mut dep_start = None;
    let mut dep_end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.split('#').next().unwrap_or_default().trim();
        if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
            continue;
        }

        let section = trimmed[1..trimmed.len() - 1].trim();
        if dep_start.is_none() && section == "dependencies" {
            dep_start = Some(index);
            continue;
        }

        if dep_start.is_some() {
            dep_end = index;
            break;
        }
    }

    let dep_lines = merged
        .iter()
        .map(|(name, path)| format!("{name} = {{ path = \"{}\" }}", path.replace('\\', "/")))
        .collect::<Vec<_>>();

    let output = if let Some(start) = dep_start {
        let mut rebuilt = Vec::new();
        rebuilt.extend(lines[..=start].iter().cloned());
        rebuilt.extend(dep_lines);
        rebuilt.extend(lines[dep_end..].iter().cloned());
        format!("{}\n", rebuilt.join("\n").trim_end())
    } else {
        let mut rebuilt = text.trim_end().to_string();
        if !rebuilt.is_empty() {
            rebuilt.push_str("\n\n");
        }
        rebuilt.push_str("[dependencies]\n");
        rebuilt.push_str(&dep_lines.join("\n"));
        rebuilt.push('\n');
        rebuilt
    };

    fs::write(manifest_path, output).map_err(|err| {
        diag_with_help(
            "E2111",
            format!(
                "failed to write manifest '{}': {err}",
                manifest_path.display()
            ),
            manifest_path,
            "check write permissions for aic.toml",
        )
    })?;

    Ok(())
}

fn parse_spec(raw: &str) -> Result<ParsedSpec, Diagnostic> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(diag_with_help(
            "E2110",
            "empty package spec",
            Path::new("."),
            "use install specs like `http_client@^1.2.0`",
        ));
    }

    let (package_raw, req_raw) = match trimmed.split_once('@') {
        Some((name, req)) => (name.trim(), req.trim()),
        None => (trimmed, "*"),
    };

    if package_raw.is_empty() {
        return Err(diag_with_help(
            "E2110",
            format!("invalid package spec '{trimmed}'"),
            Path::new("."),
            "use install specs like `http_client@^1.2.0`",
        ));
    }

    let requirement = VersionReq::parse(req_raw).map_err(|msg| {
        diag_with_help(
            "E2110",
            msg,
            Path::new("."),
            "use requirements like `*`, `1.2.3`, `^1.2.0`, `~1.2.3`, or comparator sets",
        )
    })?;

    Ok(ParsedSpec {
        package: package_raw.to_string(),
        requirement_raw: if req_raw.is_empty() {
            "*".to_string()
        } else {
            req_raw.to_string()
        },
        requirement,
    })
}

fn parse_releases(
    path: &Path,
    index: &RegistryIndex,
) -> Result<Vec<(SemVer, RegistryRelease)>, Diagnostic> {
    let mut releases = Vec::new();
    for release in &index.releases {
        let version = SemVer::parse(&release.version).map_err(|msg| {
            diag_with_help(
                "E2116",
                format!(
                    "invalid release version '{}' in '{}': {msg}",
                    release.version,
                    path.display()
                ),
                path,
                "repair or remove the corrupt registry index file",
            )
        })?;
        releases.push((version, release.clone()));
    }
    releases.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(releases)
}

pub fn publish(
    project_root: &Path,
    registry_override: Option<&Path>,
) -> Result<PublishResult, Diagnostic> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let meta = parse_package_meta(&project_root)?;
    let registry_root = registry_root(registry_override)?;

    ensure_registry_layout(&registry_root)?;

    let checksum = compute_package_checksum_for_path(&project_root).map_err(|err| {
        diag_with_help(
            "E2116",
            format!(
                "failed to checksum package '{}': {err}",
                project_root.display()
            ),
            &project_root,
            "ensure package sources are readable before publishing",
        )
    })?;

    let index_path = package_index_path(&registry_root, &meta.name);
    let mut index = read_index(&index_path, &meta.name)?;

    let version_str = meta.version.to_string();
    if index.releases.iter().any(|r| r.version == version_str) {
        return Err(diag_with_help(
            "E2112",
            format!(
                "package '{}@{}' already exists in registry",
                meta.name, version_str
            ),
            &index_path,
            "bump [package].version before publishing",
        ));
    }

    let dest = package_version_path(&registry_root, &meta.name, &version_str);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            diag_with_help(
                "E2116",
                format!(
                    "failed to create package store '{}': {err}",
                    parent.display()
                ),
                parent,
                "check write permissions for the configured registry root",
            )
        })?;
    }

    copy_tree(&project_root, &dest)?;

    index.releases.push(RegistryRelease {
        version: version_str.clone(),
        checksum: checksum.clone(),
    });
    write_index(&index_path, index)?;

    Ok(PublishResult {
        package: meta.name,
        version: version_str,
        checksum,
        registry_path: normalize_path(&dest),
    })
}

pub fn search(
    query: Option<&str>,
    registry_override: Option<&Path>,
) -> Result<Vec<SearchResult>, Diagnostic> {
    let registry_root = registry_root(registry_override)?;
    let index_root = index_dir(&registry_root);

    if !index_root.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(&index_root)
        .map_err(|err| {
            diag_with_help(
                "E2116",
                format!(
                    "failed to read registry index '{}': {err}",
                    index_root.display()
                ),
                &index_root,
                "ensure registry index files are readable",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            diag_with_help(
                "E2116",
                format!(
                    "failed to scan registry index '{}': {err}",
                    index_root.display()
                ),
                &index_root,
                "ensure registry index files are readable",
            )
        })?;

    entries.sort_by_key(|entry| entry.path());

    let filter = query
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| q.to_ascii_lowercase());

    let mut results = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let package_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        if package_name.is_empty() {
            continue;
        }

        if let Some(filter) = &filter {
            if !package_name.to_ascii_lowercase().contains(filter) {
                continue;
            }
        }

        let index = read_index(&path, &package_name)?;
        let mut releases = parse_releases(&path, &index)?;
        releases.sort_by(|a, b| b.0.cmp(&a.0));
        if releases.is_empty() {
            continue;
        }

        let versions = releases
            .iter()
            .map(|(_, release)| release.version.clone())
            .collect::<Vec<_>>();

        results.push(SearchResult {
            package: package_name,
            latest: versions
                .first()
                .cloned()
                .unwrap_or_else(|| "0.0.0".to_string()),
            versions,
        });
    }

    results.sort_by(|a, b| a.package.cmp(&b.package));
    Ok(results)
}

pub fn install(
    project_root: &Path,
    specs: &[String],
    registry_override: Option<&Path>,
) -> Result<InstallResult, Diagnostic> {
    if specs.is_empty() {
        return Err(diag_with_help(
            "E2110",
            "install requires at least one package spec",
            Path::new("."),
            "use install specs like `http_client@^1.2.0`",
        ));
    }

    let project_root = canonical_or_self(project_root.to_path_buf());
    let manifest_path = project_root.join("aic.toml");
    if !manifest_path.exists() {
        return Err(diag_with_help(
            "E2111",
            format!(
                "missing manifest '{}'; cannot install package",
                manifest_path.display()
            ),
            &manifest_path,
            "run `aic init <dir>` or create aic.toml before installing packages",
        ));
    }

    let registry_root = registry_root(registry_override)?;

    let mut parsed = Vec::new();
    for spec in specs {
        parsed.push(parse_spec(spec)?);
    }

    let mut grouped = BTreeMap::<String, Vec<ParsedSpec>>::new();
    for spec in parsed {
        grouped.entry(spec.package.clone()).or_default().push(spec);
    }

    let mut selected = Vec::<(String, String, String, String)>::new();
    for (package, requirements) in grouped {
        let index_path = package_index_path(&registry_root, &package);
        let index = read_index(&index_path, &package)?;
        if index.releases.is_empty() {
            return Err(diag_with_help(
                "E2115",
                format!("package '{package}' not found in local registry"),
                &index_path,
                format!(
                    "publish '{package}' first with `aic pkg publish --registry {}`",
                    registry_root.display()
                ),
            ));
        }

        let releases = parse_releases(&index_path, &index)?;
        let mut matching = releases
            .into_iter()
            .filter(|(version, _)| {
                requirements
                    .iter()
                    .all(|spec| spec.requirement.matches(*version))
            })
            .collect::<Vec<_>>();
        matching.sort_by(|a, b| a.0.cmp(&b.0));

        let Some((resolved_version, resolved_release)) = matching.last().cloned() else {
            let reqs = requirements
                .iter()
                .map(|r| r.requirement_raw.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(diag_with_help(
                "E2114",
                format!(
                    "resolution conflict for package '{package}': no version satisfies [{reqs}]"
                ),
                &index_path,
                "align dependency requirements to a common semantic version range",
            ));
        };

        let requirement = requirements
            .iter()
            .map(|r| r.requirement_raw.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");

        selected.push((
            package,
            requirement,
            resolved_version.to_string(),
            resolved_release.checksum,
        ));
    }

    selected.sort_by(|a, b| a.0.cmp(&b.0));

    let mut dep_updates = BTreeMap::<String, String>::new();
    let mut installed = Vec::<InstalledPackage>::new();

    for (package, requirement, version, checksum) in selected {
        let source = package_version_path(&registry_root, &package, &version);
        if !source.exists() {
            return Err(diag_with_help(
                "E2115",
                format!(
                    "registry package content missing for '{}@{}' at '{}'",
                    package,
                    version,
                    source.display()
                ),
                &source,
                "re-publish the package into the local registry",
            ));
        }

        let source_checksum = compute_package_checksum_for_path(&source).map_err(|err| {
            diag_with_help(
                "E2116",
                format!(
                    "failed to checksum registry package '{}': {err}",
                    source.display()
                ),
                &source,
                "repair or re-publish the corrupted registry package",
            )
        })?;

        if source_checksum != checksum {
            return Err(diag_with_help(
                "E2116",
                format!(
                    "registry checksum mismatch for '{}@{}': index={}, actual={}",
                    package, version, checksum, source_checksum
                ),
                &source,
                "re-publish package metadata and content to restore consistency",
            ));
        }

        let rel_path = PathBuf::from(DEPS_DIR).join(&package);
        let destination = project_root.join(&rel_path);
        copy_tree(&source, &destination)?;

        dep_updates.insert(package.clone(), normalize_path(&rel_path));
        installed.push(InstalledPackage {
            package,
            requirement,
            version,
            path: normalize_path(&destination),
        });
    }

    rewrite_dependencies_section(&manifest_path, &dep_updates)?;

    let lock_path = generate_and_write_lockfile(&project_root).map_err(|err| {
        diag_with_help(
            "E2116",
            format!("failed to update lockfile after install: {err}"),
            &project_root,
            "fix manifest/dependency issues then re-run `aic pkg install`",
        )
    })?;

    Ok(InstallResult {
        project_root: normalize_path(&project_root),
        installed,
        lockfile: normalize_path(&lock_path),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{install, package_index_path, parse_spec, publish, search, SemVer, VersionReq};

    fn write_package(root: &Path, name: &str, version: &str, module: &str, value: i32) {
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("aic.toml"),
            format!(
                "[package]\nname = \"{}\"\nversion = \"{}\"\nmain = \"src/main.aic\"\n",
                name, version
            ),
        )
        .expect("write manifest");
        fs::write(
            root.join("src/main.aic"),
            format!(
                "module {}.main;\nfn value() -> Int {{ {} }}\n",
                module, value
            ),
        )
        .expect("write source");
    }

    #[test]
    fn semver_requirement_matching_is_deterministic() {
        let req = VersionReq::parse("^1.2.0").expect("caret requirement");
        assert!(req.matches(SemVer::parse("1.2.0").expect("version")));
        assert!(req.matches(SemVer::parse("1.9.4").expect("version")));
        assert!(!req.matches(SemVer::parse("2.0.0").expect("version")));

        let req = VersionReq::parse("~0.3.1").expect("tilde requirement");
        assert!(req.matches(SemVer::parse("0.3.9").expect("version")));
        assert!(!req.matches(SemVer::parse("0.4.0").expect("version")));

        let req = VersionReq::parse(">=1.0.0,<2.0.0").expect("range requirement");
        assert!(req.matches(SemVer::parse("1.5.2").expect("version")));
        assert!(!req.matches(SemVer::parse("2.0.0").expect("version")));
    }

    #[test]
    fn parse_spec_defaults_to_wildcard() {
        let spec = parse_spec("http_client").expect("spec");
        assert_eq!(spec.package, "http_client");
        assert_eq!(spec.requirement_raw, "*");
    }

    #[test]
    fn publish_search_and_install_roundtrip() {
        let registry = tempdir().expect("registry");
        let package = tempdir().expect("package");
        let consumer = tempdir().expect("consumer");

        write_package(package.path(), "http_client", "1.2.0", "http_client", 42);
        let published = publish(package.path(), Some(registry.path())).expect("publish");
        assert_eq!(published.package, "http_client");
        assert_eq!(published.version, "1.2.0");

        let hits = search(Some("http"), Some(registry.path())).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].latest, "1.2.0");

        fs::create_dir_all(consumer.path().join("src")).expect("mkdir src");
        fs::write(
            consumer.path().join("aic.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
        )
        .expect("manifest");
        fs::write(
            consumer.path().join("src/main.aic"),
            "module app.main;\nfn main() -> Int { 0 }\n",
        )
        .expect("source");

        let installed = install(
            consumer.path(),
            &["http_client@^1.0.0".to_string()],
            Some(registry.path()),
        )
        .expect("install");

        assert_eq!(installed.installed.len(), 1);
        assert!(consumer.path().join("deps/http_client/aic.toml").exists());
        assert!(consumer.path().join("aic.lock").exists());
    }

    #[test]
    fn install_reports_resolution_conflict() {
        let registry = tempdir().expect("registry");
        let package_v1 = tempdir().expect("package v1");
        let package_v2 = tempdir().expect("package v2");
        let consumer = tempdir().expect("consumer");

        write_package(package_v1.path(), "net", "1.0.0", "net", 1);
        write_package(package_v2.path(), "net", "2.0.0", "net", 2);
        publish(package_v1.path(), Some(registry.path())).expect("publish v1");
        publish(package_v2.path(), Some(registry.path())).expect("publish v2");

        fs::create_dir_all(consumer.path().join("src")).expect("mkdir src");
        fs::write(
            consumer.path().join("aic.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
        )
        .expect("manifest");
        fs::write(
            consumer.path().join("src/main.aic"),
            "module app.main;\nfn main() -> Int { 0 }\n",
        )
        .expect("source");

        let err = install(
            consumer.path(),
            &["net@^1.0.0".to_string(), "net@^2.0.0".to_string()],
            Some(registry.path()),
        )
        .expect_err("conflict expected");
        assert_eq!(err.code, "E2114");
    }

    #[test]
    fn publish_rejects_duplicate_version() {
        let registry = tempdir().expect("registry");
        let package = tempdir().expect("package");
        write_package(package.path(), "util", "1.0.0", "util", 7);
        publish(package.path(), Some(registry.path())).expect("publish first");

        let err = publish(package.path(), Some(registry.path())).expect_err("duplicate expected");
        assert_eq!(err.code, "E2112");

        let index_path = package_index_path(registry.path(), "util");
        assert!(index_path.exists());
    }

    #[test]
    fn install_is_deterministic_for_lockfile_generation() {
        let registry = tempdir().expect("registry");
        let package_v1 = tempdir().expect("package v1");
        let package_v2 = tempdir().expect("package v2");
        let consumer = tempdir().expect("consumer");

        write_package(package_v1.path(), "util", "1.0.0", "util", 1);
        write_package(package_v2.path(), "util", "1.1.0", "util", 2);
        publish(package_v1.path(), Some(registry.path())).expect("publish v1");
        publish(package_v2.path(), Some(registry.path())).expect("publish v2");

        fs::create_dir_all(consumer.path().join("src")).expect("mkdir src");
        fs::write(
            consumer.path().join("aic.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
        )
        .expect("manifest");
        fs::write(
            consumer.path().join("src/main.aic"),
            "module app.main;\nfn main() -> Int { 0 }\n",
        )
        .expect("source");

        install(
            consumer.path(),
            &["util@^1.0.0".to_string()],
            Some(registry.path()),
        )
        .expect("install first");
        let lock1 = fs::read_to_string(consumer.path().join("aic.lock")).expect("lock1");

        install(
            consumer.path(),
            &["util@^1.0.0".to_string()],
            Some(registry.path()),
        )
        .expect("install second");
        let lock2 = fs::read_to_string(consumer.path().join("aic.lock")).expect("lock2");

        assert_eq!(lock1, lock2);
    }
}
