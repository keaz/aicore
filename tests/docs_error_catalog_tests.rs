use std::collections::BTreeSet;
use std::path::PathBuf;

use aicore::diagnostic_codes::REGISTERED_DIAGNOSTIC_CODES;

fn extract_codes(markdown: &str) -> BTreeSet<String> {
    let bytes = markdown.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;

    while i + 4 < bytes.len() {
        if bytes[i] == b'E'
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4].is_ascii_digit()
        {
            let code = std::str::from_utf8(&bytes[i..i + 5]).expect("ascii diagnostic code");
            out.insert(code.to_string());
            i += 5;
            continue;
        }
        i += 1;
    }

    out
}

#[test]
fn error_catalog_covers_registered_codes() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let catalog_path = manifest_dir.join("docs/errors/catalog.md");
    let catalog = std::fs::read_to_string(&catalog_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", catalog_path.display()));

    let documented = extract_codes(&catalog);
    let registered = REGISTERED_DIAGNOSTIC_CODES
        .iter()
        .map(|code| (*code).to_string())
        .collect::<BTreeSet<_>>();

    let missing = registered
        .difference(&documented)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "catalog is missing registered diagnostic codes: {}",
        missing.join(", ")
    );

    let unknown = documented
        .difference(&registered)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        unknown.is_empty(),
        "catalog contains unregistered diagnostic codes: {}",
        unknown.join(", ")
    );
}
