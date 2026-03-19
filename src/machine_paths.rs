use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Canonical machine-facing path string.
///
/// Policy:
/// - emit absolute paths,
/// - canonicalize existing filesystem prefixes (including symlink resolution),
/// - normalize separators to `/` for cross-platform stability.
pub fn canonical_machine_path(path: &Path) -> String {
    normalize_separators_path(&canonical_or_absolute(path))
}

/// Canonical machine-facing path buffer using the same policy as [`canonical_machine_path`].
pub fn canonical_machine_path_buf(path: &Path) -> PathBuf {
    canonical_or_absolute(path)
}

/// Normalize separators only (preserve path form and relativeness).
pub fn normalize_separators_path(path: &Path) -> String {
    normalize_separators(path.to_string_lossy().as_ref())
}

/// Normalize path separators to `/`.
pub fn normalize_separators(raw: &str) -> String {
    raw.replace('\\', "/")
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    let absolute = absolute_path(path);
    if let Ok(canonical) = fs::canonicalize(&absolute) {
        return canonical;
    }

    canonicalize_existing_prefix_or_self(&absolute)
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}

fn canonicalize_existing_prefix_or_self(path: &Path) -> PathBuf {
    let mut cursor = path.to_path_buf();
    let mut suffix = Vec::<OsString>::new();

    loop {
        match fs::canonicalize(&cursor) {
            Ok(mut canonical_prefix) => {
                for part in suffix.iter().rev() {
                    canonical_prefix.push(part);
                }
                return canonical_prefix;
            }
            Err(_) => {
                let Some(file_name) = cursor.file_name() else {
                    return path.to_path_buf();
                };
                let Some(parent) = cursor.parent() else {
                    return path.to_path_buf();
                };
                suffix.push(file_name.to_os_string());
                cursor = parent.to_path_buf();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn canonical_machine_path_is_absolute_and_separator_normalized() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("nested").join("main.aic");
        fs::create_dir_all(file.parent().expect("parent")).expect("mkdir nested");
        fs::write(&file, "fn main() -> Int { 0 }\n").expect("write source");

        let out = canonical_machine_path(&file);
        assert!(out.starts_with('/'));
        assert!(!out.contains('\\'));
        assert!(out.ends_with("/nested/main.aic"));
    }

    #[test]
    fn canonical_machine_path_resolves_existing_parent_for_missing_child() {
        let dir = tempdir().expect("tempdir");
        let existing = dir.path().join("workspace");
        fs::create_dir_all(&existing).expect("mkdir workspace");

        let missing = existing.join("src").join("main.aic");
        let out = canonical_machine_path_buf(&missing);

        assert!(out.is_absolute());
        assert!(normalize_separators_path(&out).ends_with("/workspace/src/main.aic"));
    }

    #[cfg(unix)]
    #[test]
    fn canonical_machine_path_resolves_symlink_prefixes() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().expect("tempdir");
        let real = dir.path().join("real");
        let link = dir.path().join("link");

        fs::create_dir_all(&real).expect("mkdir real");
        symlink(&real, &link).expect("create symlink");

        let through_symlink = link.join("src").join("main.aic");
        let normalized = canonical_machine_path(&through_symlink);

        assert!(normalized.contains("/real/src/main.aic"));
        assert!(!normalized.contains("/link/src/main.aic"));
    }
}
