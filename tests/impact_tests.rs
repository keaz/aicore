use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};
use tempfile::tempdir;

fn run_aic_in_dir(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run aic in dir")
}

fn write_manifest(root: &Path) {
    fs::write(
        root.join("aic.toml"),
        "[package]\nname = \"impact_demo\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write manifest");
}

fn write_base_modules(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        concat!(
            "module impact.main;\n",
            "import impact.math;\n",
            "fn run() -> Int {\n",
            "  compute(4)\n",
            "}\n",
            "fn main() -> Int {\n",
            "  run()\n",
            "}\n",
        ),
    )
    .expect("write main module");
    fs::write(
        root.join("src/math.aic"),
        concat!(
            "module impact.math;\n",
            "fn normalize(v: Int) -> Int ensures result >= 0 {\n",
            "  if v < 0 {\n",
            "    0\n",
            "  } else {\n",
            "    v\n",
            "  }\n",
            "}\n",
            "fn compute(v: Int) -> Int ensures result >= 0 {\n",
            "  normalize(v)\n",
            "}\n",
        ),
    )
    .expect("write math module");
}

#[test]
fn impact_reports_direct_transitive_tests_contracts_and_blast_radius() {
    let project = tempdir().expect("project");
    write_manifest(project.path());
    write_base_modules(project.path());
    fs::write(
        project.path().join("src/main.aic"),
        concat!(
            "module impact.main;\n",
            "import impact.math;\n",
            "import impact.tests;\n",
            "fn run() -> Int {\n",
            "  compute(4)\n",
            "}\n",
            "fn main() -> Int {\n",
            "  run()\n",
            "}\n",
        ),
    )
    .expect("rewrite main module with tests import");
    fs::write(
        project.path().join("src/tests.aic"),
        concat!(
            "module impact.tests;\n",
            "import impact.main;\n",
            "fn test_run() -> Int {\n",
            "  run()\n",
            "}\n",
        ),
    )
    .expect("write tests module");

    let out = run_aic_in_dir(project.path(), &["impact", "normalize"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("impact json");
    for key in [
        "function",
        "direct_callers",
        "transitive_callers",
        "affected_tests",
        "affected_contracts",
        "blast_radius",
    ] {
        assert!(
            payload.get(key).is_some(),
            "missing key `{key}`: {payload:#}"
        );
    }
    assert_eq!(payload["function"], "impact.math.normalize");
    assert_eq!(payload["direct_callers"], json!(["impact.math.compute"]));
    assert_eq!(
        payload["transitive_callers"],
        json!([
            "impact.main.main",
            "impact.main.run",
            "impact.tests.test_run"
        ])
    );
    assert_eq!(payload["affected_tests"], json!(["impact.tests.test_run"]));
    assert_eq!(
        payload["affected_contracts"],
        json!(["impact.math.compute", "impact.math.normalize"])
    );
    assert_eq!(payload["blast_radius"], "medium");
}

#[test]
fn impact_marks_untested_call_chains_as_large_blast_radius() {
    let project = tempdir().expect("project");
    write_manifest(project.path());
    write_base_modules(project.path());

    let out = run_aic_in_dir(project.path(), &["impact", "normalize"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("impact json");
    assert_eq!(payload["direct_callers"], json!(["impact.math.compute"]));
    assert_eq!(
        payload["transitive_callers"],
        json!(["impact.main.main", "impact.main.run"])
    );
    assert_eq!(payload["affected_tests"], json!([]));
    assert_eq!(payload["blast_radius"], "large");
}
