use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

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
