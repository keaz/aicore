use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostics::Diagnostic;
use crate::span::Span;

const LOCKFILE_NAME: &str = "aic.lock";
const CACHE_DIR_NAME: &str = ".aic-cache";
const LOCKFILE_SCHEMA_VERSION: u32 = 1;

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
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManifestDependency {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    pub schema_version: u32,
    pub package: String,
    pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LockedDependency {
    pub name: String,
    pub path: String,
    pub checksum: String,
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

    let lock_path = lockfile_path(&project_root);
    let expected = if options.offline {
        None
    } else {
        Some(generate_lockfile_from_manifest(&project_root, &manifest)?)
    };

    if let Some(lock) = read_lockfile(&project_root)? {
        lockfile_used = true;
        if let Some(expected) = expected {
            if lock != expected {
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

        for dep in &lock.dependencies {
            let source_root = resolve_locked_path(&project_root, &dep.path);
            source_roots.insert(source_root.clone());
            let cache_root = cache_path_for_dep(&project_root, dep);
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
                    &project_root.to_string_lossy(),
                    Span::new(0, 0),
                )
                .with_help("run `aic lock` online first"),
            );
        }
        let expected = generate_lockfile_from_manifest(&project_root, &manifest)?;
        for dep in &expected.dependencies {
            let root = resolve_locked_path(&project_root, &dep.path);
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

pub fn generate_and_write_lockfile(project_root: &Path) -> anyhow::Result<PathBuf> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let lock = generate_lockfile(&project_root)?;
    let lock_path = lockfile_path(&project_root);
    let json = serde_json::to_string_pretty(&lock)?;
    fs::write(&lock_path, format!("{json}\n"))?;

    for dep in &lock.dependencies {
        let src = resolve_locked_path(&project_root, &dep.path);
        if src.exists() {
            let cache = cache_path_for_dep(&project_root, dep);
            sync_cache_entry(&src, &cache)?;
        }
    }

    Ok(lock_path)
}

pub fn generate_lockfile(project_root: &Path) -> anyhow::Result<Lockfile> {
    let project_root = canonical_or_self(project_root.to_path_buf());
    let manifest = read_manifest(&project_root)?
        .ok_or_else(|| anyhow::anyhow!("missing aic.toml in {}", project_root.display()))?;
    generate_lockfile_from_manifest(&project_root, &manifest)
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

pub fn lockfile_path(project_root: &Path) -> PathBuf {
    project_root.join(LOCKFILE_NAME)
}

fn read_lockfile(project_root: &Path) -> anyhow::Result<Option<Lockfile>> {
    let path = lockfile_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let lock = serde_json::from_str::<Lockfile>(&text)
        .map_err(|err| anyhow::anyhow!("invalid lockfile '{}': {}", path.display(), err))?;
    if lock.schema_version != LOCKFILE_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported lockfile schema version {} in {}",
            lock.schema_version,
            path.display()
        );
    }
    Ok(Some(lock))
}

fn generate_lockfile_from_manifest(
    project_root: &Path,
    manifest: &Manifest,
) -> anyhow::Result<Lockfile> {
    let mut visited = BTreeSet::new();
    let mut dependencies = Vec::new();

    let mut deps = manifest.dependencies.clone();
    deps.sort();
    for dep in deps {
        let dep_root = canonical_or_self(project_root.join(&dep.path));
        collect_dependency_nodes(
            project_root,
            dep_root,
            dep.name,
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
    })
}

fn collect_dependency_nodes(
    project_root: &Path,
    dep_root: PathBuf,
    fallback_name: String,
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
    });

    if let Some(manifest) = dep_manifest {
        let mut children = manifest.dependencies;
        children.sort();
        for child in children {
            let child_root = canonical_or_self(dep_root.join(child.path));
            collect_dependency_nodes(project_root, child_root, child.name, visited, dependencies)?;
        }
    }

    Ok(())
}

fn parse_manifest(text: &str, path: &Path) -> anyhow::Result<Manifest> {
    let mut section = String::new();
    let mut package_name: Option<String> = None;
    let mut main: Option<String> = None;
    let mut dependencies = Vec::new();

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
            let dep_path = if value.starts_with('{') {
                parse_inline_dep_path(value, path, line_no + 1)?
            } else {
                parse_string(value, path, line_no + 1)?
            };
            dependencies.push(ManifestDependency {
                name: key.to_string(),
                path: dep_path,
            });
        }
    }

    dependencies.sort();
    dependencies.dedup();

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
    })
}

fn parse_inline_dep_path(value: &str, path: &Path, line_no: usize) -> anyhow::Result<String> {
    let inner = value.trim();
    if !inner.starts_with('{') || !inner.ends_with('}') {
        anyhow::bail!(
            "invalid dependency table at {}:{} (expected {{ path = \"...\" }})",
            path.display(),
            line_no
        );
    }
    let inner = inner[1..inner.len() - 1].trim();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key.trim() == "path" {
            return parse_string(value.trim(), path, line_no);
        }
    }
    anyhow::bail!(
        "dependency table missing `path` at {}:{}",
        path.display(),
        line_no
    )
}

fn parse_string(value: &str, path: &Path, line_no: usize) -> anyhow::Result<String> {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return Ok(value[1..value.len() - 1].to_string());
    }
    anyhow::bail!("expected quoted string at {}:{}", path.display(), line_no)
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
        compute_package_checksum, generate_and_write_lockfile, generate_lockfile, read_manifest,
        resolve_dependency_context, PackageOptions,
    };

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
"#,
        )
        .expect("write manifest");

        let manifest = read_manifest(dir.path())
            .expect("manifest")
            .expect("manifest present");
        assert_eq!(manifest.package_name, "app");
        assert_eq!(manifest.main, "src/main.aic");
        assert_eq!(manifest.dependencies.len(), 2);
    }

    #[test]
    fn lockfile_generation_is_deterministic() {
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

        let lock1 = generate_lockfile(dir.path()).expect("lockfile");
        let lock2 = generate_lockfile(dir.path()).expect("lockfile");
        assert_eq!(lock1, lock2);
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
