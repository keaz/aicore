use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

fn send_message(writer: &mut dyn Write, message: &Value) {
    let body = serde_json::to_string(message).expect("serialize message");
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body).expect("write frame");
    writer.flush().expect("flush frame");
}

fn read_message(reader: &mut dyn BufRead) -> Value {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).expect("read header line");
        assert!(n > 0, "unexpected EOF while reading LSP header");
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>().expect("parse content length"));
        }
    }

    let len = content_length.expect("missing Content-Length");
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).expect("read body");
    serde_json::from_slice(&body).expect("parse json")
}

#[test]
fn lsp_hover_definition_and_formatting_smoke() {
    let repo = repo_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aic"))
        .arg("lsp")
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lsp");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "rootUri": file_uri(&repo),
                "capabilities": {}
            }
        }),
    );
    let init = read_message(&mut stdout);
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    assert_eq!(init["result"]["capabilities"]["definitionProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    assert!(init["result"]["capabilities"]["completionProvider"].is_object());
    assert_eq!(init["result"]["capabilities"]["renameProvider"], true);
    assert_eq!(init["result"]["capabilities"]["codeActionProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["semanticTokensProvider"]["full"],
        true
    );

    let file = repo.join("examples/e7/lsp_project/src/main.aic");
    let uri = file_uri(&file);
    let text = fs::read_to_string(&file).expect("read source");

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "aic",
                    "version": 1,
                    "text": text
                }
            }
        }),
    );

    let publish = read_message(&mut stdout);
    assert_eq!(publish["method"], "textDocument/publishDiagnostics");
    assert!(publish["params"]["diagnostics"].as_array().is_some());

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/hover",
            "params": {
                "textDocument": {"uri": uri},
                "position": {"line": 4, "character": 3}
            }
        }),
    );
    let hover = read_message(&mut stdout);
    assert_eq!(hover["id"], 2);
    let hover_value = hover["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(hover_value.contains("fn add"), "hover={hover:#}");

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/definition",
            "params": {
                "textDocument": {"uri": uri},
                "position": {"line": 4, "character": 3}
            }
        }),
    );
    let definition = read_message(&mut stdout);
    assert_eq!(definition["id"], 3);
    let def_uri = definition["result"]["uri"].as_str().unwrap_or_default();
    assert!(
        def_uri.ends_with("examples/e7/lsp_project/src/math.aic"),
        "definition={definition:#}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": {"uri": uri},
                "options": {"tabSize": 2, "insertSpaces": true}
            }
        }),
    );
    let formatting = read_message(&mut stdout);
    assert_eq!(formatting["id"], 4);
    assert!(formatting["result"].is_array());

    let completion_start = Instant::now();
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/completion",
            "params": {
                "textDocument": {"uri": uri},
                "position": {"line": 4, "character": 3}
            }
        }),
    );
    let completion = read_message(&mut stdout);
    assert_eq!(completion["id"], 6);
    let completion_items = completion["result"].as_array().expect("completion array");
    assert!(
        completion_items.iter().any(|item| item["label"] == "add"),
        "completion={completion:#}"
    );
    assert!(
        completion_start.elapsed() <= Duration::from_millis(750),
        "completion latency too high: {:?}",
        completion_start.elapsed()
    );

    let rename_start = Instant::now();
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/rename",
            "params": {
                "textDocument": {"uri": uri},
                "position": {"line": 4, "character": 3},
                "newName": "sum"
            }
        }),
    );
    let rename = read_message(&mut stdout);
    assert_eq!(rename["id"], 7);
    let changes = rename["result"]["changes"]
        .as_object()
        .expect("rename changes");
    let has_main = changes
        .keys()
        .any(|k| k.ends_with("examples/e7/lsp_project/src/main.aic"));
    let has_math = changes
        .keys()
        .any(|k| k.ends_with("examples/e7/lsp_project/src/math.aic"));
    assert!(has_main && has_math, "rename={rename:#}");
    assert!(
        rename_start.elapsed() <= Duration::from_millis(750),
        "rename latency too high: {:?}",
        rename_start.elapsed()
    );

    let semantic_start = Instant::now();
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "textDocument/semanticTokens/full",
            "params": {
                "textDocument": {"uri": uri}
            }
        }),
    );
    let semantic = read_message(&mut stdout);
    assert_eq!(semantic["id"], 8);
    assert!(semantic["result"]["data"].as_array().is_some());
    assert!(
        !semantic["result"]["data"]
            .as_array()
            .expect("token array")
            .is_empty(),
        "semantic={semantic:#}"
    );
    assert!(
        semantic_start.elapsed() <= Duration::from_millis(750),
        "semantic token latency too high: {:?}",
        semantic_start.elapsed()
    );

    let fix_dir = tempdir().expect("tempdir");
    let fix_file = fix_dir.path().join("fixable.aic");
    let fix_uri = file_uri(&fix_file);
    let fix_source = "module fix.main;\nfn main() -> Int {\n  let x = 1\n  x\n}\n";
    fs::write(&fix_file, fix_source).expect("write fix source");
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": fix_uri,
                    "languageId": "aic",
                    "version": 1,
                    "text": fix_source
                }
            }
        }),
    );
    let _fix_publish = read_message(&mut stdout);

    let action_start = Instant::now();
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": {"uri": fix_uri},
                "range": {
                    "start": {"line": 2, "character": 2},
                    "end": {"line": 2, "character": 12}
                },
                "context": { "diagnostics": [] }
            }
        }),
    );
    let actions = read_message(&mut stdout);
    assert_eq!(actions["id"], 9);
    let action_items = actions["result"].as_array().expect("action array");
    assert!(!action_items.is_empty(), "actions={actions:#}");
    assert_eq!(action_items[0]["kind"], "quickfix");
    assert!(
        action_start.elapsed() <= Duration::from_millis(750),
        "code action latency too high: {:?}",
        action_start.elapsed()
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "shutdown",
            "params": null
        }),
    );
    let shutdown = read_message(&mut stdout);
    assert_eq!(shutdown["id"], 5);

    drop(stdin);
    let status = child.wait().expect("wait lsp");
    assert!(status.success(), "status={status:?}");
}

#[test]
fn lsp_diagnostics_codes_match_cli_check() {
    let repo = repo_root();
    let file = repo.join("examples/e7/diag_errors.aic");
    let uri = file_uri(&file);
    let text = fs::read_to_string(&file).expect("read source");

    let mut child = Command::new(env!("CARGO_BIN_EXE_aic"))
        .arg("lsp")
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lsp");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"rootUri": file_uri(&repo), "capabilities": {}}
        }),
    );
    let _ = read_message(&mut stdout);

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "aic",
                    "version": 1,
                    "text": text
                }
            }
        }),
    );
    let publish = read_message(&mut stdout);
    let lsp_codes = publish["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|d| {
            d.get("code")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect::<BTreeSet<_>>();

    let cli = Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(["check", "examples/e7/diag_errors.aic", "--json"])
        .current_dir(&repo)
        .output()
        .expect("run cli check");
    assert_eq!(cli.status.code(), Some(1));
    let cli_json: Value = serde_json::from_slice(&cli.stdout).expect("cli json");
    let cli_codes = cli_json
        .as_array()
        .expect("cli diagnostics array")
        .iter()
        .filter_map(|d| {
            d.get("code")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(lsp_codes, cli_codes);

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "shutdown",
            "params": null
        }),
    );
    let _ = read_message(&mut stdout);

    drop(stdin);
    let status = child.wait().expect("wait lsp");
    assert!(status.success(), "status={status:?}");
}
