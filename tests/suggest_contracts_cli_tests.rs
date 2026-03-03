use std::fs;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn run_aic(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .output()
        .expect("run aic")
}

fn write_fixture_source() -> (TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source_path = dir.path().join("suggest_contracts_demo.aic");
    fs::write(
        &source_path,
        concat!(
            "module tests.suggest.contracts;\n",
            "import std.io;\n",
            "fn bounded(i: Int, n: Int) -> Int {\n",
            "    if i >= 0 && i < n {\n",
            "        i\n",
            "    } else {\n",
            "        0\n",
            "    }\n",
            "}\n",
            "fn passthrough[T](x: T) -> T effects { io } capabilities { io } {\n",
            "    print_int(1);\n",
            "    x\n",
            "}\n",
        ),
    )
    .expect("write source");
    (dir, source_path.to_string_lossy().to_string())
}

#[test]
fn suggest_contracts_json_output_is_deterministic_and_confidence_bounded() {
    let (_dir, source_path) = write_fixture_source();

    let first = run_aic(&["suggest-contracts", &source_path, "--json"]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_aic(&["suggest-contracts", &source_path, "--json"]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    assert_eq!(
        first.stdout, second.stdout,
        "json output should be deterministic"
    );

    let payload: Value = serde_json::from_slice(&first.stdout).expect("parse json payload");
    let suggestions = payload["suggestions"]
        .as_array()
        .expect("suggestions array");

    let bounded = suggestions
        .iter()
        .find(|entry| entry["function"] == "bounded")
        .expect("bounded suggestion");
    let requires = bounded["suggested_requires"]
        .as_array()
        .expect("requires array");
    assert!(requires.iter().any(|entry| entry["expr"] == "i >= 0"));
    assert!(requires.iter().any(|entry| entry["expr"] == "i < n"));

    let passthrough = suggestions
        .iter()
        .find(|entry| entry["function"] == "passthrough")
        .expect("passthrough suggestion");
    let ensures = passthrough["suggested_ensures"]
        .as_array()
        .expect("ensures array");
    assert!(ensures.iter().any(|entry| entry["expr"] == "result == x"));

    for suggestion in suggestions {
        for key in ["suggested_requires", "suggested_ensures"] {
            for clause in suggestion[key].as_array().expect("clause array") {
                let confidence = clause["confidence"].as_f64().expect("confidence");
                assert!(
                    (0.0..=1.0).contains(&confidence),
                    "confidence out of range: {confidence}"
                );
            }
        }
    }
}

#[test]
fn suggest_contracts_text_mode_is_human_readable() {
    let (_dir, source_path) = write_fixture_source();
    let output = run_aic(&["suggest-contracts", &source_path]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("text output");
    assert!(stdout.contains("function bounded"));
    assert!(stdout.contains("requires:"));
    assert!(stdout.contains("ensures:"));
    assert!(stdout.contains("confidence="));
}
