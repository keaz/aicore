use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const ENV_AIC_HOME: &str = "AIC_HOME";
pub const ENV_AIC_STD_ROOT: &str = "AIC_STD_ROOT";

pub fn bundled_std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("std")
}

pub fn configured_std_root() -> Option<PathBuf> {
    non_empty_env(ENV_AIC_STD_ROOT)
        .map(PathBuf::from)
        .map(canonical_or_self)
}

pub fn aic_home_dir() -> Option<PathBuf> {
    if let Some(path) = non_empty_env(ENV_AIC_HOME) {
        return Some(canonical_or_self(PathBuf::from(path)));
    }

    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| canonical_or_self(PathBuf::from(home).join(".aic")))
}

pub fn default_global_std_root() -> Option<PathBuf> {
    aic_home_dir().map(|home| {
        home.join("toolchains")
            .join(env!("CARGO_PKG_VERSION"))
            .join("std")
    })
}

pub fn preferred_global_std_root() -> Option<PathBuf> {
    configured_std_root().or_else(default_global_std_root)
}

pub fn std_import_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(global_root) = preferred_global_std_root() {
        roots.push(global_root);
    }

    roots.push(project_root.join("std"));
    roots.push(bundled_std_root());

    dedup_paths(roots)
}

pub fn default_std_root_for_tools() -> PathBuf {
    if let Some(global_root) = preferred_global_std_root().filter(|path| path.exists()) {
        return global_root;
    }

    bundled_std_root()
}

pub fn install_std(std_root_override: Option<&Path>) -> anyhow::Result<PathBuf> {
    let source = canonical_or_self(bundled_std_root());
    if !source.exists() {
        anyhow::bail!("bundled std sources were not found at {}", source.display());
    }

    let destination = std_root_override
        .map(|path| canonical_or_self(path.to_path_buf()))
        .or_else(preferred_global_std_root)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve global std install location; set {} or {}",
                ENV_AIC_STD_ROOT,
                ENV_AIC_HOME
            )
        })?;

    if source == destination {
        return Ok(destination);
    }

    if destination.is_file() {
        anyhow::bail!(
            "cannot install std into {} because it is a file",
            destination.display()
        );
    }

    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    copy_dir_all(&source, &destination)?;

    Ok(destination)
}

fn copy_dir_all(source: &Path, destination: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(destination)?;

    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }

    Ok(())
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        let canonical = canonical_or_self(path);
        if deduped.iter().all(|known| known != &canonical) {
            deduped.push(canonical);
        }
    }
    deduped
}

fn canonical_or_self(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn non_empty_env(name: &str) -> Option<String> {
    let value = env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{install_std, std_import_roots, ENV_AIC_STD_ROOT};
    use std::fs;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    #[test]
    fn install_std_writes_bundled_std_tree_to_destination() {
        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("toolchain/std");

        let installed = install_std(Some(&destination)).expect("install std");
        assert_eq!(installed, destination);
        assert!(installed.join("io.aic").exists());

        let installed_io = fs::read_to_string(installed.join("io.aic")).expect("read io");
        let bundled_io =
            fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("std/io.aic"))
                .expect("read bundled io");
        assert_eq!(installed_io, bundled_io);
    }

    #[test]
    fn std_import_roots_honor_explicit_std_override_first() {
        let lock = env_lock();

        let temp = tempdir().expect("tempdir");
        let explicit_root = temp.path().join("custom-std");
        let project_root = temp.path().join("project");

        let previous = std::env::var(ENV_AIC_STD_ROOT).ok();
        std::env::set_var(
            ENV_AIC_STD_ROOT,
            explicit_root.to_string_lossy().to_string(),
        );

        let roots = std_import_roots(&project_root);

        if let Some(value) = previous {
            std::env::set_var(ENV_AIC_STD_ROOT, value);
        } else {
            std::env::remove_var(ENV_AIC_STD_ROOT);
        }

        drop(lock);

        assert!(!roots.is_empty());
        assert_eq!(roots[0], explicit_root);
        assert!(roots.iter().any(|root| root == &project_root.join("std")));
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }
}
