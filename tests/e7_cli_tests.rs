use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_aic(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run aic")
}

fn run_aic_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aic"));
    command.args(args).current_dir(repo_root());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run aic with env")
}

#[test]
fn cli_help_snapshots_are_stable() {
    let main_help = run_aic(&["--help"]);
    assert!(main_help.status.success());
    assert_eq!(
        String::from_utf8_lossy(&main_help.stdout),
        include_str!("golden/e7/help_main.txt")
    );

    let check_help = run_aic(&["check", "--help"]);
    assert!(check_help.status.success());
    assert_eq!(
        String::from_utf8_lossy(&check_help.stdout),
        include_str!("golden/e7/help_check.txt")
    );

    let test_help = run_aic(&["test", "--help"]);
    assert!(test_help.status.success());
    assert_eq!(
        String::from_utf8_lossy(&test_help.stdout),
        include_str!("golden/e7/help_test.txt")
    );
}

#[test]
fn cli_exit_codes_are_deterministic() {
    let ok = run_aic(&["check", "examples/e7/cli_smoke.aic"]);
    assert_eq!(ok.status.code(), Some(0));

    let diag_fail = run_aic(&["check", "examples/e7/diag_errors.aic"]);
    assert_eq!(diag_fail.status.code(), Some(1));

    let usage_fail = run_aic(&["check", "examples/e7/diag_errors.aic", "--json", "--sarif"]);
    assert_eq!(usage_fail.status.code(), Some(2));
}

#[test]
fn diagnostics_json_and_sarif_outputs_are_structured() {
    let json_out = run_aic(&["check", "examples/e7/diag_errors.aic", "--json"]);
    assert_eq!(json_out.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).expect("diagnostics json");
    assert!(diagnostics.is_array());
    let first = diagnostics
        .as_array()
        .and_then(|v| v.first())
        .expect("at least one diagnostic");
    assert!(first.get("code").is_some());
    assert!(first.get("severity").is_some());
    assert!(first.get("spans").is_some());

    let sarif_out = run_aic(&["diag", "examples/e7/diag_errors.aic", "--sarif"]);
    assert_eq!(sarif_out.status.code(), Some(1));
    let sarif: serde_json::Value = serde_json::from_slice(&sarif_out.stdout).expect("sarif json");
    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["runs"][0]["results"].is_array());
    assert!(sarif["runs"][0]["tool"]["driver"]["rules"].is_array());
    assert!(sarif["runs"][0]["results"][0]["ruleId"].is_string());
    assert!(sarif["runs"][0]["results"][0]["locations"].is_array());
}

#[test]
fn explain_and_contract_commands_work() {
    let explain_known = run_aic(&["explain", "E2001", "--json"]);
    assert_eq!(explain_known.status.code(), Some(0));
    let known: serde_json::Value =
        serde_json::from_slice(&explain_known.stdout).expect("explain json");
    assert_eq!(known["known"], true);
    assert_eq!(known["code"], "E2001");

    let unknown = format!("E{}{}{}{}", 9, 9, 9, 9);
    let explain_unknown = run_aic(&["explain", &unknown]);
    assert_eq!(explain_unknown.status.code(), Some(1));
    let text = String::from_utf8_lossy(&explain_unknown.stdout);
    assert!(text.contains("unknown diagnostic code"));

    let contract = run_aic(&["contract", "--json"]);
    assert_eq!(contract.status.code(), Some(0));
    let contract_json: serde_json::Value =
        serde_json::from_slice(&contract.stdout).expect("contract json");
    assert_eq!(contract_json["version"], "1.0");
    assert!(contract_json["commands"].is_array());
    assert!(contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|c| c["name"] == "lsp"));
}

#[test]
fn test_harness_runs_categories_and_reports_json() {
    let all = run_aic(&["test", "examples/e7/harness", "--json"]);
    assert_eq!(all.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&all.stdout).expect("harness report");
    assert_eq!(report["total"], 3);
    assert_eq!(report["failed"], 0);

    let compile_fail_mode = run_aic(&[
        "test",
        "examples/e7/harness",
        "--mode",
        "compile-fail",
        "--json",
    ]);
    assert_eq!(compile_fail_mode.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&compile_fail_mode.stdout).expect("compile-fail report");
    assert_eq!(report["total"], 1);
    assert_eq!(report["failed"], 0);
}

fn write_pkg_project(root: &std::path::Path, name: &str, version: &str, module: &str, body: &str) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("aic.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nmain = \"src/main.aic\"\n"),
    )
    .expect("write manifest");
    fs::write(
        root.join("src/main.aic"),
        format!("module {module}.main;\n{body}\n"),
    )
    .expect("write source");
}

fn write_consumer_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        root.join("src/main.aic"),
        "module consumer_app.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write consumer source");
}

#[test]
fn pkg_publish_search_install_roundtrip() {
    let registry = tempdir().expect("registry");
    let package = tempdir().expect("package");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package.path(),
        "http_client",
        "1.2.0",
        "http_client",
        "fn get() -> Int { 42 }",
    );

    let publish = run_aic(&[
        "pkg",
        "publish",
        package.path().to_str().expect("pkg path"),
        "--registry",
        registry.path().to_str().expect("registry path"),
    ]);
    assert_eq!(publish.status.code(), Some(0));

    let search = run_aic(&[
        "pkg",
        "search",
        "http",
        "--registry",
        registry.path().to_str().expect("registry path"),
        "--json",
    ]);
    assert_eq!(search.status.code(), Some(0));
    let search_json: serde_json::Value =
        serde_json::from_slice(&search.stdout).expect("search json");
    assert!(search_json.is_array());
    assert_eq!(search_json[0]["package"], "http_client");
    assert_eq!(search_json[0]["latest"], "1.2.0");

    fs::create_dir_all(consumer.path().join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.path().join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.path().join("src/main.aic"),
        "module consumer_app.main;\nimport http_client.main;\nfn main() -> Int { http_client.main.get() }\n",
    )
    .expect("write consumer source");

    let install = run_aic(&[
        "pkg",
        "install",
        "http_client@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer path"),
        "--registry",
        registry.path().to_str().expect("registry path"),
    ]);
    assert_eq!(install.status.code(), Some(0));

    assert!(consumer.path().join("deps/http_client/aic.toml").exists());
    assert!(consumer.path().join("aic.lock").exists());

    let check = run_aic(&["check", consumer.path().to_str().expect("consumer path")]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "check stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn pkg_install_conflict_is_diagnostic_and_json_structured() {
    let registry = tempdir().expect("registry");
    let package_v1 = tempdir().expect("package v1");
    let package_v2 = tempdir().expect("package v2");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package_v1.path(),
        "netlib",
        "1.0.0",
        "netlib",
        "fn v() -> Int { 1 }",
    );
    write_pkg_project(
        package_v2.path(),
        "netlib",
        "2.0.0",
        "netlib",
        "fn v() -> Int { 2 }",
    );

    let publish_v1 = run_aic(&[
        "pkg",
        "publish",
        package_v1.path().to_str().expect("pkg v1"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v1.status.code(), Some(0));

    let publish_v2 = run_aic(&[
        "pkg",
        "publish",
        package_v2.path().to_str().expect("pkg v2"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v2.status.code(), Some(0));

    fs::create_dir_all(consumer.path().join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.path().join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.path().join("src/main.aic"),
        "module consumer_app.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write consumer source");

    let install = run_aic(&[
        "pkg",
        "install",
        "netlib@^1.0.0",
        "netlib@^2.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--registry",
        registry.path().to_str().expect("registry"),
        "--json",
    ]);
    assert_eq!(install.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&install.stdout).expect("diagnostics");
    assert!(diagnostics.is_array());
    assert_eq!(diagnostics[0]["code"], "E2114");
}

#[test]
fn pkg_install_lockfile_is_deterministic() {
    let registry = tempdir().expect("registry");
    let package_v1 = tempdir().expect("package v1");
    let package_v2 = tempdir().expect("package v2");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package_v1.path(),
        "utilpkg",
        "1.0.0",
        "utilpkg",
        "fn v() -> Int { 1 }",
    );
    write_pkg_project(
        package_v2.path(),
        "utilpkg",
        "1.1.0",
        "utilpkg",
        "fn v() -> Int { 2 }",
    );

    let publish_v1 = run_aic(&[
        "pkg",
        "publish",
        package_v1.path().to_str().expect("pkg v1"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v1.status.code(), Some(0));

    let publish_v2 = run_aic(&[
        "pkg",
        "publish",
        package_v2.path().to_str().expect("pkg v2"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v2.status.code(), Some(0));

    fs::create_dir_all(consumer.path().join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.path().join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.path().join("src/main.aic"),
        "module consumer_app.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write consumer source");

    let install_1 = run_aic(&[
        "pkg",
        "install",
        "utilpkg@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(install_1.status.code(), Some(0));
    let first_lock = fs::read_to_string(consumer.path().join("aic.lock")).expect("lock 1");

    let install_2 = run_aic(&[
        "pkg",
        "install",
        "utilpkg@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(install_2.status.code(), Some(0));
    let second_lock = fs::read_to_string(consumer.path().join("aic.lock")).expect("lock 2");

    assert_eq!(first_lock, second_lock);
}

#[test]
fn pkg_example_consumes_local_http_client_module() {
    let check = run_aic(&["check", "examples/pkg/consume_http_client.aic"]);
    assert_eq!(check.status.code(), Some(0));
}

#[test]
fn pkg_private_registry_auth_and_scope_workflow() {
    let public_registry = tempdir().expect("public");
    let private_registry = tempdir().expect("private");
    let public_pkg = tempdir().expect("public package");
    let private_pkg = tempdir().expect("private package");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        public_pkg.path(),
        "utilpkg",
        "1.0.0",
        "utilpkg",
        "fn v() -> Int { 1 }",
    );
    write_pkg_project(
        private_pkg.path(),
        "corp/http_client",
        "1.2.0",
        "corp_http_client",
        "fn v() -> Int { 2 }",
    );

    assert_eq!(
        run_aic(&[
            "pkg",
            "publish",
            public_pkg.path().to_str().expect("public pkg"),
            "--registry",
            public_registry.path().to_str().expect("public registry"),
        ])
        .status
        .code(),
        Some(0)
    );
    assert_eq!(
        run_aic(&[
            "pkg",
            "publish",
            private_pkg.path().to_str().expect("private pkg"),
            "--registry",
            private_registry.path().to_str().expect("private registry"),
        ])
        .status
        .code(),
        Some(0)
    );

    write_consumer_project(consumer.path());
    let token_file = consumer.path().join("private.token");
    fs::write(&token_file, "super-secret\n").expect("write token file");
    fs::write(
        consumer.path().join("aic.registry.json"),
        format!(
            concat!(
                "{{\n",
                "  \"default\": \"public\",\n",
                "  \"registries\": {{\n",
                "    \"public\": {{ \"path\": \"{}\" }},\n",
                "    \"private\": {{\n",
                "      \"path\": \"{}\",\n",
                "      \"private\": true,\n",
                "      \"token_env\": \"AIC_PRIVATE_TOKEN\",\n",
                "      \"token_file\": \"{}\"\n",
                "    }}\n",
                "  }},\n",
                "  \"scopes\": {{ \"corp/\": \"private\" }}\n",
                "}}\n"
            ),
            public_registry.path().display(),
            private_registry.path().display(),
            token_file.display()
        ),
    )
    .expect("write registry config");

    let unauthorized = run_aic(&[
        "pkg",
        "install",
        "corp/http_client@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--json",
    ]);
    assert_eq!(unauthorized.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&unauthorized.stdout).expect("unauthorized diagnostics");
    assert_eq!(diagnostics[0]["code"], "E2117");

    let install = run_aic_with_env(
        &[
            "pkg",
            "install",
            "utilpkg@^1.0.0",
            "corp/http_client@^1.0.0",
            "--path",
            consumer.path().to_str().expect("consumer"),
            "--json",
        ],
        &[("AIC_PRIVATE_TOKEN", "super-secret")],
    );
    assert_eq!(install.status.code(), Some(0));
    assert!(consumer.path().join("deps/utilpkg/aic.toml").exists());
    assert!(consumer
        .path()
        .join("deps/corp/http_client/aic.toml")
        .exists());
}

#[test]
fn pkg_mirror_fallback_and_misconfigured_credentials_are_diagnostic() {
    let primary_registry = tempdir().expect("primary");
    let mirror_registry = tempdir().expect("mirror");
    let package = tempdir().expect("package");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package.path(),
        "mirror_only",
        "1.0.0",
        "mirror_only",
        "fn v() -> Int { 1 }",
    );
    assert_eq!(
        run_aic(&[
            "pkg",
            "publish",
            package.path().to_str().expect("package"),
            "--registry",
            mirror_registry.path().to_str().expect("mirror"),
        ])
        .status
        .code(),
        Some(0)
    );

    write_consumer_project(consumer.path());
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
            primary_registry.path().display(),
            mirror_registry.path().display()
        ),
    )
    .expect("write mirror config");

    let install = run_aic(&[
        "pkg",
        "install",
        "mirror_only@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
    ]);
    assert_eq!(install.status.code(), Some(0));
    assert!(consumer.path().join("deps/mirror_only/aic.toml").exists());

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
                "      \"token_file\": \"missing.token\"\n",
                "    }}\n",
                "  }}\n",
                "}}\n"
            ),
            mirror_registry.path().display()
        ),
    )
    .expect("write broken private config");

    let misconfigured = run_aic(&[
        "pkg",
        "install",
        "mirror_only@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--token",
        "whatever",
        "--json",
    ]);
    assert_eq!(misconfigured.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&misconfigured.stdout).expect("misconfigured diagnostics");
    assert_eq!(diagnostics[0]["code"], "E2118");
}
