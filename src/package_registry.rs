use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostics::Diagnostic;
use crate::package_workflow::{compute_package_checksum_for_path, generate_and_write_lockfile};
use crate::span::Span;

const ENV_REGISTRY_ROOT: &str = "AIC_PKG_REGISTRY";
const ENV_REGISTRY_CONFIG: &str = "AIC_PKG_REGISTRY_CONFIG";
const ENV_SIGNING_KEY: &str = "AIC_PKG_SIGNING_KEY";
const ENV_SIGNING_KEY_ID: &str = "AIC_PKG_SIGNING_KEY_ID";
const REGISTRY_INDEX_DIR: &str = "index";
const REGISTRY_PACKAGES_DIR: &str = "packages";
const DEPS_DIR: &str = "deps";

#[derive(Debug, Clone, Default)]
pub struct RegistryClientOptions {
    pub registry: Option<String>,
    pub registry_config: Option<PathBuf>,
    pub token: Option<String>,
}

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
    pub audit: Vec<TrustAuditRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustAuditRecord {
    pub package: String,
    pub version: String,
    pub decision: String,
    pub reason: String,
    pub checksum_verified: bool,
    pub signature_verified: bool,
    pub signature_key_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature_alg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature_key_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RegistryConfig {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    registries: BTreeMap<String, RegistryConfigEntry>,
    #[serde(default)]
    scopes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RegistryConfigEntry {
    path: String,
    #[serde(default)]
    mirrors: Vec<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    token_env: Option<String>,
    #[serde(default)]
    token_file: Option<String>,
    #[serde(default)]
    trust: RegistryTrustPolicy,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RegistryTrustPolicy {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
    #[serde(default)]
    require_signed: bool,
    #[serde(default)]
    require_signed_for: Vec<String>,
    #[serde(default)]
    trusted_keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct LoadedRegistryConfig {
    path: PathBuf,
    config: RegistryConfig,
}

#[derive(Debug, Clone)]
struct ResolvedRegistry {
    roots: Vec<PathBuf>,
    private: bool,
    token_env: Option<String>,
    token_file: Option<PathBuf>,
    display_name: String,
    trust_policy: RegistryTrustPolicy,
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

fn resolve_config_path(
    project_root: &Path,
    options: &RegistryClientOptions,
) -> Result<Option<PathBuf>, Diagnostic> {
    if let Some(path) = &options.registry_config {
        return Ok(Some(canonical_or_self(path.clone())));
    }

    if let Ok(path) = std::env::var(ENV_REGISTRY_CONFIG) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(Some(canonical_or_self(PathBuf::from(trimmed))));
        }
    }

    let candidate = project_root.join("aic.registry.json");
    if candidate.exists() {
        return Ok(Some(canonical_or_self(candidate)));
    }

    Ok(None)
}

fn load_registry_config(
    project_root: &Path,
    options: &RegistryClientOptions,
) -> Result<Option<LoadedRegistryConfig>, Diagnostic> {
    let Some(path) = resolve_config_path(project_root, options)? else {
        return Ok(None);
    };

    let text = fs::read_to_string(&path).map_err(|err| {
        diag_with_help(
            "E2118",
            format!("failed to read registry config '{}': {err}", path.display()),
            &path,
            "set --registry-config to a readable JSON config, or remove the broken file",
        )
    })?;

    let config = serde_json::from_str::<RegistryConfig>(&text).map_err(|err| {
        diag_with_help(
            "E2118",
            format!("invalid registry config '{}': {err}", path.display()),
            &path,
            "registry config must be valid JSON with `registries`, optional `default`, and optional `scopes`",
        )
    })?;

    Ok(Some(LoadedRegistryConfig { path, config }))
}

fn is_path_like(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || value.starts_with('.')
        || value.starts_with('~')
        || Path::new(value).is_absolute()
}

fn resolve_config_path_value(base_dir: &Path, value: &str) -> PathBuf {
    let candidate = PathBuf::from(value.trim());
    if candidate.is_absolute() {
        candidate
    } else {
        base_dir.join(candidate)
    }
}

fn scoped_registry_alias(config: &RegistryConfig, package: &str) -> Option<String> {
    config
        .scopes
        .iter()
        .filter(|(prefix, _)| package.starts_with(prefix.as_str()))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, alias)| alias.clone())
}

fn resolve_registry_alias(
    package: Option<&str>,
    loaded: &LoadedRegistryConfig,
    options: &RegistryClientOptions,
) -> Result<Option<String>, Diagnostic> {
    if let Some(selected) = options.registry.as_deref() {
        if loaded.config.registries.contains_key(selected) {
            return Ok(Some(selected.to_string()));
        }
        if is_path_like(selected) {
            return Ok(None);
        }
        return Err(diag_with_help(
            "E2118",
            format!(
                "unknown registry alias '{selected}' in '{}'",
                loaded.path.display()
            ),
            &loaded.path,
            "declare the alias under `registries`, or pass an explicit registry path",
        ));
    }

    if let Some(pkg) = package {
        if let Some(alias) = scoped_registry_alias(&loaded.config, pkg) {
            return Ok(Some(alias));
        }
    }

    Ok(loaded.config.default.clone())
}

fn resolve_registry(
    project_root: &Path,
    package: Option<&str>,
    options: &RegistryClientOptions,
) -> Result<ResolvedRegistry, Diagnostic> {
    let loaded = load_registry_config(project_root, options)?;
    if let Some(loaded) = loaded.as_ref() {
        if let Some(alias) = resolve_registry_alias(package, loaded, options)? {
            let Some(entry) = loaded.config.registries.get(&alias) else {
                return Err(diag_with_help(
                    "E2118",
                    format!(
                        "registry alias '{}' is referenced but not defined in '{}'",
                        alias,
                        loaded.path.display()
                    ),
                    &loaded.path,
                    "define the alias under `registries` with at least a `path` value",
                ));
            };

            let base = loaded
                .path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let mut roots = Vec::new();
            let primary = canonical_or_self(resolve_config_path_value(&base, &entry.path));
            roots.push(primary);
            for mirror in &entry.mirrors {
                let mirror_path = canonical_or_self(resolve_config_path_value(&base, mirror));
                if !roots.contains(&mirror_path) {
                    roots.push(mirror_path);
                }
            }

            let token_file = entry
                .token_file
                .as_deref()
                .map(|value| canonical_or_self(resolve_config_path_value(&base, value)));

            return Ok(ResolvedRegistry {
                roots,
                private: entry.private,
                token_env: entry.token_env.clone(),
                token_file,
                display_name: alias,
                trust_policy: entry.trust.clone(),
            });
        }
    }

    if let Some(selected) = options.registry.as_deref() {
        let root = canonical_or_self(PathBuf::from(selected));
        return Ok(ResolvedRegistry {
            roots: vec![root],
            private: false,
            token_env: None,
            token_file: None,
            display_name: selected.to_string(),
            trust_policy: RegistryTrustPolicy::default(),
        });
    }

    let root = registry_root(None)?;
    Ok(ResolvedRegistry {
        roots: vec![root],
        private: false,
        token_env: None,
        token_file: None,
        display_name: "default".to_string(),
        trust_policy: RegistryTrustPolicy::default(),
    })
}

fn resolve_auth_token(
    registry: &ResolvedRegistry,
    options: &RegistryClientOptions,
) -> Option<String> {
    if let Some(token) = options.token.as_deref() {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let Some(env_name) = registry.token_env.as_deref() else {
        return None;
    };
    let Ok(value) = std::env::var(env_name) else {
        return None;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn authorize_registry(
    action: &str,
    registry: &ResolvedRegistry,
    options: &RegistryClientOptions,
) -> Result<(), Diagnostic> {
    if !registry.private {
        return Ok(());
    }

    let token = resolve_auth_token(registry, options).ok_or_else(|| {
        diag_with_help(
            "E2117",
            format!(
                "registry '{}' requires credentials for `{action}`",
                registry.display_name
            ),
            registry.roots.first().map(PathBuf::as_path).unwrap_or(Path::new(".")),
            "provide --token or configure `token_env` and set the expected environment variable in CI",
        )
    })?;

    if let Some(token_file) = registry.token_file.as_deref() {
        let expected = fs::read_to_string(token_file).map_err(|err| {
            diag_with_help(
                "E2118",
                format!(
                    "failed to read registry token file '{}': {err}",
                    token_file.display()
                ),
                token_file,
                "fix `token_file` path in registry config or update file permissions",
            )
        })?;

        if token != expected.trim() {
            return Err(diag_with_help(
                "E2117",
                format!(
                    "unauthorized access to private registry '{}' for `{action}`",
                    registry.display_name
                ),
                token_file,
                "verify token source (--token or token_env) and retry",
            ));
        }
    }

    Ok(())
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

fn package_signature_payload(package: &str, version: &str, checksum: &str) -> String {
    format!("aicore-pkg-v1\npackage={package}\nversion={version}\nchecksum={checksum}\n")
}

fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    let mut normalized_key = [0u8; 64];
    if key.len() > 64 {
        let mut hashed = Sha256::new();
        hashed.update(key);
        let digest = hashed.finalize();
        normalized_key[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = [0u8; 64];
    let mut i_key_pad = [0u8; 64];
    for (idx, key_byte) in normalized_key.iter().copied().enumerate() {
        o_key_pad[idx] = key_byte ^ 0x5c;
        i_key_pad[idx] = key_byte ^ 0x36;
    }

    let mut inner = Sha256::new();
    inner.update(i_key_pad);
    inner.update(data);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(o_key_pad);
    outer.update(inner_digest);
    format!("{:x}", outer.finalize())
}

fn maybe_sign_release(package: &str, version: &str, checksum: &str) -> Option<RegistryRelease> {
    let key = std::env::var(ENV_SIGNING_KEY).ok()?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    let key_id = std::env::var(ENV_SIGNING_KEY_ID)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".to_string());
    let payload = package_signature_payload(package, version, checksum);
    let signature = hmac_sha256_hex(key.as_bytes(), payload.as_bytes());

    Some(RegistryRelease {
        version: version.to_string(),
        checksum: checksum.to_string(),
        signature: Some(signature),
        signature_alg: Some("hmac-sha256".to_string()),
        signature_key_id: Some(key_id),
    })
}

fn trust_pattern_matches(pattern: &str, package: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return package.starts_with(prefix);
    }
    package == pattern
}

fn matches_any_pattern(patterns: &[String], package: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| trust_pattern_matches(pattern, package))
}

fn trust_default_allows(policy: &RegistryTrustPolicy, path: &Path) -> Result<bool, Diagnostic> {
    match policy.default.as_deref().unwrap_or("allow") {
        "allow" => Ok(true),
        "deny" => Ok(false),
        other => Err(diag_with_help(
            "E2118",
            format!("invalid trust policy default action '{other}' (expected 'allow' or 'deny')"),
            path,
            "set `trust.default` to `allow` or `deny` in aic.registry.json",
        )),
    }
}

fn verify_release_signature(
    package: &str,
    version: &str,
    checksum: &str,
    release: &RegistryRelease,
    policy: &RegistryTrustPolicy,
    source_path: &Path,
) -> Result<(bool, Option<String>), Diagnostic> {
    let Some(signature) = release.signature.as_deref() else {
        return Ok((false, None));
    };

    let alg = release.signature_alg.as_deref().unwrap_or("hmac-sha256");
    if alg != "hmac-sha256" {
        return Err(diag_with_help(
            "E2124",
            format!(
                "unsupported package signature algorithm '{}' for '{}@{}'",
                alg, package, version
            ),
            source_path,
            "supported algorithm is hmac-sha256",
        ));
    }

    let key_id = release
        .signature_key_id
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let env_name = policy
        .trusted_keys
        .get(&key_id)
        .cloned()
        .unwrap_or_else(|| {
            if key_id == "default" {
                ENV_SIGNING_KEY.to_string()
            } else {
                String::new()
            }
        });
    if env_name.is_empty() {
        return Err(diag_with_help(
            "E2124",
            format!(
                "no trusted key mapping found for signature key id '{}' on '{}@{}'",
                key_id, package, version
            ),
            source_path,
            "add `trust.trusted_keys.<key_id> = \"ENV_VAR\"` in aic.registry.json",
        ));
    }

    let trusted_key = std::env::var(&env_name).map_err(|_| {
        diag_with_help(
            "E2124",
            format!(
                "trusted key env '{}' is not set for verifying '{}@{}'",
                env_name, package, version
            ),
            source_path,
            format!("set environment variable '{}' before install", env_name),
        )
    })?;
    let trusted_key = trusted_key.trim();
    if trusted_key.is_empty() {
        return Err(diag_with_help(
            "E2124",
            format!(
                "trusted key env '{}' is empty for verifying '{}@{}'",
                env_name, package, version
            ),
            source_path,
            format!("set environment variable '{}' to a non-empty key", env_name),
        ));
    }

    let payload = package_signature_payload(package, version, checksum);
    let expected = hmac_sha256_hex(trusted_key.as_bytes(), payload.as_bytes());
    if expected != signature {
        return Err(diag_with_help(
            "E2124",
            format!(
                "package signature mismatch for '{}@{}' (key id '{}')",
                package, version, key_id
            ),
            source_path,
            "verify trusted key configuration and re-publish package if metadata was tampered",
        ));
    }

    Ok((true, Some(key_id)))
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencyUpdate {
    path: String,
    resolved_version: Option<String>,
    source_provenance: Option<String>,
}

fn parse_quoted_string(value: &str) -> Option<String> {
    let raw = value.trim();
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        Some(raw[1..raw.len() - 1].to_string())
    } else {
        None
    }
}

fn parse_inline_dependency(value: &str) -> Option<DependencyUpdate> {
    let text = value.trim();
    if !text.starts_with('{') || !text.ends_with('}') {
        return None;
    }

    let mut path = None;
    let mut resolved_version = None;
    let mut source_provenance = None;

    let inner = text[1..text.len() - 1].trim();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let Some(parsed) = parse_quoted_string(value) else {
            continue;
        };
        match key.trim() {
            "path" => path = Some(parsed),
            "resolved_version" => resolved_version = Some(parsed),
            "source_provenance" => source_provenance = Some(parsed),
            _ => {}
        }
    }
    path.map(|path| DependencyUpdate {
        path,
        resolved_version,
        source_provenance,
    })
}

fn parse_existing_dependencies(text: &str) -> BTreeMap<String, DependencyUpdate> {
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

        if let Some(dep) = parse_inline_dependency(value) {
            dependencies.insert(key.to_string(), dep);
            continue;
        }

        if let Some(path) = parse_quoted_string(value) {
            dependencies.insert(
                key.to_string(),
                DependencyUpdate {
                    path,
                    resolved_version: None,
                    source_provenance: None,
                },
            );
        }
    }

    dependencies
}

fn format_dependency_update(name: &str, dep: &DependencyUpdate) -> String {
    let mut fields = vec![format!("path = \"{}\"", dep.path.replace('\\', "/"))];
    if let Some(version) = &dep.resolved_version {
        fields.push(format!(
            "resolved_version = \"{}\"",
            version.replace('\\', "/")
        ));
    }
    if let Some(provenance) = &dep.source_provenance {
        fields.push(format!(
            "source_provenance = \"{}\"",
            provenance.replace('\\', "/")
        ));
    }
    format!("{name} = {{ {} }}", fields.join(", "))
}

fn rewrite_dependencies_section(
    manifest_path: &Path,
    updates: &BTreeMap<String, DependencyUpdate>,
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
        .map(|(name, dep)| format_dependency_update(name, dep))
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

#[derive(Debug, Clone)]
struct ReleaseSource {
    root: PathBuf,
    release: RegistryRelease,
    index_path: PathBuf,
}

fn collect_release_sources(
    roots: &[PathBuf],
    package: &str,
) -> Result<BTreeMap<SemVer, Vec<ReleaseSource>>, Diagnostic> {
    let mut releases = BTreeMap::<SemVer, Vec<ReleaseSource>>::new();
    for root in roots {
        let index_path = package_index_path(root, package);
        let index = read_index(&index_path, package)?;
        for (version, release) in parse_releases(&index_path, &index)? {
            releases.entry(version).or_default().push(ReleaseSource {
                root: root.clone(),
                release,
                index_path: index_path.clone(),
            });
        }
    }
    Ok(releases)
}

fn scan_registry_packages(
    roots: &[PathBuf],
    filter: Option<&str>,
) -> Result<BTreeMap<String, BTreeSet<SemVer>>, Diagnostic> {
    let mut merged = BTreeMap::<String, BTreeSet<SemVer>>::new();
    let filter = filter
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_ascii_lowercase);

    for root in roots {
        let index_root = index_dir(root);
        if !index_root.exists() {
            continue;
        }
        let mut entries = Vec::new();
        collect_index_json_files(&index_root, &mut entries)?;
        entries.sort();

        for path in entries {
            let package_name = path
                .strip_prefix(&index_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/")
                .strip_suffix(".json")
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
            let bucket = merged.entry(package_name).or_default();
            for (version, _) in parse_releases(&path, &index)? {
                bucket.insert(version);
            }
        }
    }

    Ok(merged)
}

fn collect_index_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    let mut entries = fs::read_dir(dir)
        .map_err(|err| {
            diag_with_help(
                "E2116",
                format!("failed to read registry index '{}': {err}", dir.display()),
                dir,
                "ensure registry index files are readable",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            diag_with_help(
                "E2116",
                format!("failed to scan registry index '{}': {err}", dir.display()),
                dir,
                "ensure registry index files are readable",
            )
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_index_json_files(&path, out)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            out.push(path);
        }
    }

    Ok(())
}

pub fn publish(
    project_root: &Path,
    registry_override: Option<&Path>,
) -> Result<PublishResult, Diagnostic> {
    let options = RegistryClientOptions {
        registry: registry_override.map(normalize_path),
        ..RegistryClientOptions::default()
    };
    publish_with_options(project_root, &options)
}

pub fn publish_with_options(
    project_root: &Path,
    options: &RegistryClientOptions,
) -> Result<PublishResult, Diagnostic> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let meta = parse_package_meta(&project_root)?;
    let registry = resolve_registry(&project_root, Some(&meta.name), options)?;
    authorize_registry("publish", &registry, options)?;

    let Some(primary) = registry.roots.first() else {
        return Err(diag_with_help(
            "E2118",
            "resolved registry has no root path",
            &project_root,
            "fix registry config by setting `path` and optional `mirrors`",
        ));
    };

    ensure_registry_layout(primary)?;

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

    let index_path = package_index_path(primary, &meta.name);
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

    let dest = package_version_path(primary, &meta.name, &version_str);
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

    let release =
        maybe_sign_release(&meta.name, &version_str, &checksum).unwrap_or(RegistryRelease {
            version: version_str.clone(),
            checksum: checksum.clone(),
            signature: None,
            signature_alg: None,
            signature_key_id: None,
        });

    index.releases.push(release);
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
    let options = RegistryClientOptions {
        registry: registry_override.map(normalize_path),
        ..RegistryClientOptions::default()
    };
    let cwd = std::env::current_dir().map_err(|err| {
        diag_with_help(
            "E2118",
            format!("failed to resolve current directory for package search: {err}"),
            Path::new("."),
            "run from a readable working directory or pass --registry-config explicitly",
        )
    })?;
    search_with_options(&cwd, query, &options)
}

pub fn search_with_options(
    project_root: &Path,
    query: Option<&str>,
    options: &RegistryClientOptions,
) -> Result<Vec<SearchResult>, Diagnostic> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let registry = resolve_registry(&project_root, None, options)?;
    authorize_registry("search", &registry, options)?;
    let merged = scan_registry_packages(&registry.roots, query)?;

    let mut results = Vec::new();
    for (package, versions) in merged {
        if versions.is_empty() {
            continue;
        }
        let versions = versions
            .iter()
            .rev()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        results.push(SearchResult {
            package,
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
    let options = RegistryClientOptions {
        registry: registry_override.map(normalize_path),
        ..RegistryClientOptions::default()
    };
    install_with_options(project_root, specs, &options)
}

pub fn install_with_options(
    project_root: &Path,
    specs: &[String],
    options: &RegistryClientOptions,
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

    let mut parsed = Vec::new();
    for spec in specs {
        parsed.push(parse_spec(spec)?);
    }

    let mut grouped = BTreeMap::<String, Vec<ParsedSpec>>::new();
    for spec in parsed {
        grouped.entry(spec.package.clone()).or_default().push(spec);
    }

    let mut selected = Vec::<(
        String,
        String,
        String,
        String,
        PathBuf,
        String,
        TrustAuditRecord,
    )>::new();
    for (package, requirements) in grouped {
        let registry = resolve_registry(&project_root, Some(&package), options)?;
        authorize_registry("install", &registry, options)?;

        let release_sources = collect_release_sources(&registry.roots, &package)?;
        if release_sources.is_empty() {
            let searched_roots = registry
                .roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(diag_with_help(
                "E2115",
                format!(
                    "package '{package}' not found in configured registry roots: [{searched_roots}]"
                ),
                &project_root,
                "publish the package, or configure mirrors/scopes to include the expected registry",
            ));
        }

        let mut compatible = release_sources
            .keys()
            .copied()
            .filter(|version| {
                requirements
                    .iter()
                    .all(|spec| spec.requirement.matches(*version))
            })
            .collect::<Vec<_>>();
        compatible.sort();

        let Some(resolved_version) = compatible.last().copied() else {
            let reqs = requirements
                .iter()
                .map(|r| r.requirement_raw.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            let fallback_path = release_sources
                .values()
                .next()
                .and_then(|sources| sources.first())
                .map(|source| source.index_path.clone())
                .unwrap_or_else(|| package_index_path(&registry.roots[0], &package));
            return Err(diag_with_help(
                "E2114",
                format!(
                    "resolution conflict for package '{package}': no version satisfies [{reqs}]"
                ),
                &fallback_path,
                "align dependency requirements to a common semantic version range",
            ));
        };

        let version_text = resolved_version.to_string();
        let Some(sources) = release_sources.get(&resolved_version) else {
            return Err(diag_with_help(
                "E2116",
                format!("internal resolution error for package '{package}'"),
                &project_root,
                "retry install; if this persists, regenerate registry index metadata",
            ));
        };

        let mut chosen: Option<(PathBuf, String, RegistryRelease, String)> = None;
        let mut checksum_mismatch: Option<(PathBuf, String, String)> = None;
        for source in sources {
            let source_path = package_version_path(&source.root, &package, &version_text);
            if !source_path.exists() {
                continue;
            }

            let source_checksum =
                compute_package_checksum_for_path(&source_path).map_err(|err| {
                    diag_with_help(
                        "E2116",
                        format!(
                            "failed to checksum registry package '{}': {err}",
                            source_path.display()
                        ),
                        &source_path,
                        "repair or re-publish the corrupted registry package",
                    )
                })?;

            if source_checksum == source.release.checksum {
                let provenance = format!(
                    "registry_root={};index={}",
                    normalize_path(&source.root),
                    normalize_path(&source.index_path)
                );
                chosen = Some((
                    source_path,
                    source.release.checksum.clone(),
                    source.release.clone(),
                    provenance,
                ));
                break;
            }

            checksum_mismatch = Some((
                source_path,
                source.release.checksum.clone(),
                source_checksum,
            ));
        }

        let Some((resolved_source, resolved_checksum, resolved_release, source_provenance)) =
            chosen
        else {
            if let Some((source_path, expected, actual)) = checksum_mismatch {
                return Err(diag_with_help(
                    "E2116",
                    format!(
                        "registry checksum mismatch for '{}@{}': index={}, actual={}",
                        package, version_text, expected, actual
                    ),
                    &source_path,
                    "re-publish package metadata/content or repair mirror synchronization",
                ));
            }
            return Err(diag_with_help(
                "E2115",
                format!(
                    "registry package content missing for '{}@{}' in configured roots",
                    package, version_text
                ),
                &project_root,
                "ensure the package exists in primary registry or configured mirrors",
            ));
        };

        if matches_any_pattern(&registry.trust_policy.deny, &package) {
            return Err(diag_with_help(
                "E2119",
                format!(
                    "trust policy denied package '{}@{}' via deny rules",
                    package, version_text
                ),
                &resolved_source,
                "remove matching deny rule or install an allowed package/version",
            ));
        }
        let allow_match = matches_any_pattern(&registry.trust_policy.allow, &package);
        let default_allow = trust_default_allows(&registry.trust_policy, &resolved_source)?;
        if !allow_match && !default_allow {
            return Err(diag_with_help(
                "E2119",
                format!(
                    "trust policy denied package '{}@{}' (default action is deny)",
                    package, version_text
                ),
                &resolved_source,
                "add an allow rule or change trust.default to allow",
            ));
        }

        let (signature_verified, signature_key_id) = verify_release_signature(
            &package,
            &version_text,
            &resolved_checksum,
            &resolved_release,
            &registry.trust_policy,
            &resolved_source,
        )?;

        let require_signed = registry.trust_policy.require_signed
            || matches_any_pattern(&registry.trust_policy.require_signed_for, &package);
        if require_signed && !signature_verified {
            return Err(diag_with_help(
                "E2119",
                format!(
                    "trust policy requires signed package for '{}@{}', but the release is unsigned",
                    package, version_text
                ),
                &resolved_source,
                "publish with AIC_PKG_SIGNING_KEY and configure trust.trusted_keys for verification",
            ));
        }

        let requirement = requirements
            .iter()
            .map(|r| r.requirement_raw.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");

        let mut reason = if allow_match {
            "allowed by trust.allow".to_string()
        } else {
            "allowed by trust.default=allow".to_string()
        };
        if signature_verified {
            reason.push_str("; signature verified");
        } else if require_signed {
            reason.push_str("; signature required");
        } else {
            reason.push_str("; unsigned allowed by policy");
        }

        selected.push((
            package,
            requirement,
            version_text,
            resolved_checksum,
            resolved_source,
            source_provenance,
            TrustAuditRecord {
                package: String::new(),
                version: String::new(),
                decision: "allow".to_string(),
                reason,
                checksum_verified: true,
                signature_verified,
                signature_key_id,
            },
        ));
    }

    selected.sort_by(|a, b| a.0.cmp(&b.0));

    let mut dep_updates = BTreeMap::<String, DependencyUpdate>::new();
    let mut installed = Vec::<InstalledPackage>::new();
    let mut audit = Vec::<TrustAuditRecord>::new();
    for (package, requirement, version, _checksum, source, source_provenance, mut audit_record) in
        selected
    {
        let rel_path = PathBuf::from(DEPS_DIR).join(&package);
        let destination = project_root.join(&rel_path);
        copy_tree(&source, &destination)?;

        audit_record.package = package.clone();
        audit_record.version = version.clone();
        dep_updates.insert(
            package.clone(),
            DependencyUpdate {
                path: normalize_path(&rel_path),
                resolved_version: Some(version.clone()),
                source_provenance: Some(source_provenance),
            },
        );
        installed.push(InstalledPackage {
            package,
            requirement,
            version,
            path: normalize_path(&destination),
        });
        audit.push(audit_record);
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
        audit,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use super::{
        install, install_with_options, package_index_path, parse_existing_dependencies, parse_spec,
        publish, rewrite_dependencies_section, search, DependencyUpdate, RegistryClientOptions,
        SemVer, VersionReq,
    };

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
        assert_eq!(installed.audit.len(), 1);
        assert_eq!(installed.audit[0].decision, "allow");
        assert!(installed.audit[0].checksum_verified);
        assert!(consumer.path().join("deps/http_client/aic.toml").exists());
        assert!(consumer.path().join("aic.lock").exists());

        let manifest = fs::read_to_string(consumer.path().join("aic.toml")).expect("manifest");
        assert!(
            manifest.contains("resolved_version = \"1.2.0\""),
            "manifest={manifest}"
        );
        assert!(
            manifest.contains("source_provenance = \"registry_root="),
            "manifest={manifest}"
        );

        let lock: crate::package_workflow::Lockfile = serde_json::from_str(
            &fs::read_to_string(consumer.path().join("aic.lock")).expect("lock"),
        )
        .expect("parse lock");
        let dep = lock
            .dependencies
            .iter()
            .find(|dep| dep.name == "http_client")
            .expect("http_client dependency");
        assert_eq!(dep.resolved_version.as_deref(), Some("1.2.0"));
        assert!(dep
            .source_provenance
            .as_deref()
            .unwrap_or_default()
            .contains("registry_root="));
    }

    #[test]
    fn rewrite_dependencies_section_preserves_traceability_metadata() {
        let project = tempdir().expect("project");
        let manifest_path = project.path().join("aic.toml");
        fs::write(
            &manifest_path,
            concat!(
                "[package]\n",
                "name = \"app\"\n",
                "version = \"0.1.0\"\n",
                "main = \"src/main.aic\"\n\n",
                "[dependencies]\n",
                "alpha = { path = \"deps/alpha\", resolved_version = \"1.0.0\", source_provenance = \"registry_root=/tmp/r1;index=/tmp/r1/index/alpha.json\" }\n",
                "beta = { path = \"deps/beta\" }\n",
            ),
        )
        .expect("write manifest");

        let mut updates = BTreeMap::new();
        updates.insert(
            "beta".to_string(),
            DependencyUpdate {
                path: "deps/beta".to_string(),
                resolved_version: Some("2.1.0".to_string()),
                source_provenance: Some(
                    "registry_root=/tmp/r2;index=/tmp/r2/index/beta.json".to_string(),
                ),
            },
        );
        rewrite_dependencies_section(&manifest_path, &updates).expect("rewrite dependencies");

        let parsed = parse_existing_dependencies(
            &fs::read_to_string(&manifest_path).expect("read rewritten manifest"),
        );
        let alpha = parsed.get("alpha").expect("alpha dependency");
        assert_eq!(alpha.path, "deps/alpha");
        assert_eq!(alpha.resolved_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            alpha.source_provenance.as_deref(),
            Some("registry_root=/tmp/r1;index=/tmp/r1/index/alpha.json")
        );

        let beta = parsed.get("beta").expect("beta dependency");
        assert_eq!(beta.path, "deps/beta");
        assert_eq!(beta.resolved_version.as_deref(), Some("2.1.0"));
        assert_eq!(
            beta.source_provenance.as_deref(),
            Some("registry_root=/tmp/r2;index=/tmp/r2/index/beta.json")
        );
    }

    #[test]
    fn install_blocks_tampered_registry_package() {
        let registry = tempdir().expect("registry");
        let package = tempdir().expect("package");
        let consumer = tempdir().expect("consumer");

        write_package(package.path(), "tamper_pkg", "1.0.0", "tamper_pkg", 1);
        let published = publish(package.path(), Some(registry.path())).expect("publish");
        let published_root = PathBuf::from(&published.registry_path);
        fs::write(
            published_root.join("src/main.aic"),
            "module tamper_pkg.main;\nfn value() -> Int { 999 }\n",
        )
        .expect("tamper package");

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
            &["tamper_pkg@^1.0.0".to_string()],
            Some(registry.path()),
        )
        .expect_err("tampered install should fail");
        assert_eq!(err.code, "E2116");
        assert!(err.message.contains("checksum mismatch"));
    }

    #[test]
    fn trust_policy_deny_rule_blocks_install() {
        let registry = tempdir().expect("registry");
        let package = tempdir().expect("package");
        let consumer = tempdir().expect("consumer");

        write_package(package.path(), "blocked_pkg", "1.0.0", "blocked_pkg", 1);
        publish(package.path(), Some(registry.path())).expect("publish");

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
        fs::write(
            consumer.path().join("aic.registry.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"default\": \"local\",\n",
                    "  \"registries\": {{\n",
                    "    \"local\": {{\n",
                    "      \"path\": \"{}\",\n",
                    "      \"trust\": {{\n",
                    "        \"default\": \"allow\",\n",
                    "        \"deny\": [\"blocked_pkg\"]\n",
                    "      }}\n",
                    "    }}\n",
                    "  }}\n",
                    "}}\n"
                ),
                registry.path().display()
            ),
        )
        .expect("registry config");

        let err = install_with_options(
            consumer.path(),
            &["blocked_pkg@^1.0.0".to_string()],
            &RegistryClientOptions::default(),
        )
        .expect_err("deny policy should fail");
        assert_eq!(err.code, "E2119");
        assert!(err.message.contains("trust policy denied"));
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

    #[test]
    fn private_registry_requires_valid_token() {
        let registry = tempdir().expect("registry");
        let package = tempdir().expect("package");
        let consumer = tempdir().expect("consumer");

        write_package(package.path(), "private_pkg", "1.0.0", "private_pkg", 1);
        publish(package.path(), Some(registry.path())).expect("publish");

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

        let token_file = consumer.path().join("private.token");
        fs::write(&token_file, "secret-token\n").expect("token file");
        fs::write(
            consumer.path().join("aic.registry.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"default\": \"private\",\n",
                    "  \"registries\": {{\n",
                    "    \"private\": {{\n",
                    "      \"path\": \"{}\",\n",
                    "      \"private\": true,\n",
                    "      \"token_file\": \"{}\"\n",
                    "    }}\n",
                    "  }}\n",
                    "}}\n"
                ),
                registry.path().display(),
                token_file.display()
            ),
        )
        .expect("registry config");

        let missing = install_with_options(
            consumer.path(),
            &["private_pkg@^1.0.0".to_string()],
            &RegistryClientOptions::default(),
        )
        .expect_err("missing token should fail");
        assert_eq!(missing.code, "E2117");

        let wrong = install_with_options(
            consumer.path(),
            &["private_pkg@^1.0.0".to_string()],
            &RegistryClientOptions {
                token: Some("wrong".to_string()),
                ..RegistryClientOptions::default()
            },
        )
        .expect_err("wrong token should fail");
        assert_eq!(wrong.code, "E2117");

        let ok = install_with_options(
            consumer.path(),
            &["private_pkg@^1.0.0".to_string()],
            &RegistryClientOptions {
                token: Some("secret-token".to_string()),
                ..RegistryClientOptions::default()
            },
        )
        .expect("token should authorize");
        assert_eq!(ok.installed.len(), 1);
    }

    #[test]
    fn mirror_fallback_installs_when_primary_missing_package() {
        let primary = tempdir().expect("primary");
        let mirror = tempdir().expect("mirror");
        let package = tempdir().expect("package");
        let consumer = tempdir().expect("consumer");

        write_package(package.path(), "mirror_pkg", "1.0.0", "mirror_pkg", 1);
        publish(package.path(), Some(mirror.path())).expect("publish mirror");

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

        fs::write(
            consumer.path().join("aic.registry.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"default\": \"corp\",\n",
                    "  \"registries\": {{\n",
                    "    \"corp\": {{\n",
                    "      \"path\": \"{}\",\n",
                    "      \"mirrors\": [\"{}\"]\n",
                    "    }}\n",
                    "  }}\n",
                    "}}\n"
                ),
                primary.path().display(),
                mirror.path().display()
            ),
        )
        .expect("registry config");

        let installed = install_with_options(
            consumer.path(),
            &["mirror_pkg@^1.0.0".to_string()],
            &RegistryClientOptions::default(),
        )
        .expect("mirror fallback install");
        assert_eq!(installed.installed.len(), 1);
        assert!(consumer.path().join("deps/mirror_pkg/aic.toml").exists());
    }
}
