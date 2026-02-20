use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::ast;
use crate::driver::run_frontend;
use crate::formatter::format_program;
use crate::ir_builder;
use crate::parser;
use crate::span::Span;

#[derive(Default)]
struct LspServer {
    root_uri: Option<String>,
    shutdown_requested: bool,
    documents: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct SymbolDecl {
    name: String,
    kind: String,
    signature: String,
    file: PathBuf,
    span: Span,
}

pub fn run_stdio() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut server = LspServer::default();

    loop {
        let Some(message) = read_message(&mut reader)? else {
            break;
        };
        let outbound = server.handle_message(&message)?;
        for out in outbound {
            write_message(&mut writer, &out)?;
        }
        if server.shutdown_requested {
            break;
        }
    }

    Ok(())
}

impl LspServer {
    fn handle_message(&mut self, message: &Value) -> anyhow::Result<Vec<Value>> {
        let mut outbound = Vec::new();
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match method {
            "initialize" => {
                self.root_uri = message
                    .get("params")
                    .and_then(|p| p.get("rootUri"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);

                if let Some(id) = id {
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "capabilities": {
                                "textDocumentSync": {
                                    "openClose": true,
                                    "change": 1,
                                    "save": true
                                },
                                "hoverProvider": true,
                                "definitionProvider": true,
                                "documentFormattingProvider": true
                            },
                            "serverInfo": {
                                "name": "aic-lsp",
                                "version": env!("CARGO_PKG_VERSION")
                            }
                        }
                    }));
                }
            }
            "shutdown" => {
                self.shutdown_requested = true;
                if let Some(id) = id {
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    }));
                }
            }
            "exit" => {
                self.shutdown_requested = true;
            }
            "textDocument/didOpen" => {
                if let Some(text_doc) = message
                    .get("params")
                    .and_then(|p| p.get("textDocument"))
                    .cloned()
                {
                    if let (Some(uri), Some(text)) = (
                        text_doc.get("uri").and_then(Value::as_str),
                        text_doc.get("text").and_then(Value::as_str),
                    ) {
                        self.documents.insert(uri.to_string(), text.to_string());
                        if let Some(diag_notification) = self.publish_diagnostics(uri)? {
                            outbound.push(diag_notification);
                        }
                    }
                }
            }
            "textDocument/didChange" => {
                let uri = message
                    .get("params")
                    .and_then(|p| p.get("textDocument"))
                    .and_then(|td| td.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();

                if !uri.is_empty() {
                    if let Some(changes) = message
                        .get("params")
                        .and_then(|p| p.get("contentChanges"))
                        .and_then(Value::as_array)
                    {
                        if let Some(last_text) = changes
                            .last()
                            .and_then(|v| v.get("text"))
                            .and_then(Value::as_str)
                        {
                            self.documents
                                .insert(uri.to_string(), last_text.to_string());
                        }
                    }
                    if let Some(diag_notification) = self.publish_diagnostics(uri)? {
                        outbound.push(diag_notification);
                    }
                }
            }
            "textDocument/didSave" => {
                if let Some(uri) = message
                    .get("params")
                    .and_then(|p| p.get("textDocument"))
                    .and_then(|td| td.get("uri"))
                    .and_then(Value::as_str)
                {
                    if let Some(diag_notification) = self.publish_diagnostics(uri)? {
                        outbound.push(diag_notification);
                    }
                }
            }
            "textDocument/hover" => {
                if let Some(id) = id {
                    let result = self.hover_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/definition" => {
                if let Some(id) = id {
                    let result = self.definition_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/formatting" => {
                if let Some(id) = id {
                    let result = self.formatting_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            _ => {
                if let Some(id) = id {
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("method not found: {method}")
                        }
                    }));
                }
            }
        }

        Ok(outbound)
    }

    fn hover_response(&self, message: &Value) -> anyhow::Result<Value> {
        let (uri, line, character) = request_position(message)?;
        let text = self.document_text(&uri)?;
        let Some(symbol) = word_at_position(&text, line, character) else {
            return Ok(Value::Null);
        };
        let Some(path) = uri_to_path(&uri) else {
            return Ok(Value::Null);
        };

        let declarations = build_symbol_index(&path)?;
        let Some(decls) = declarations.get(&symbol) else {
            return Ok(Value::Null);
        };
        let first = &decls[0];

        Ok(json!({
            "contents": {
                "kind": "markdown",
                "value": format!("`{}`\n\n{}", first.signature, first.kind)
            }
        }))
    }

    fn definition_response(&self, message: &Value) -> anyhow::Result<Value> {
        let (uri, line, character) = request_position(message)?;
        let text = self.document_text(&uri)?;
        let Some(symbol) = word_at_position(&text, line, character) else {
            return Ok(Value::Null);
        };
        let Some(path) = uri_to_path(&uri) else {
            return Ok(Value::Null);
        };

        let declarations = build_symbol_index(&path)?;
        let Some(decls) = declarations.get(&symbol) else {
            return Ok(Value::Null);
        };
        let first = &decls[0];
        Ok(json!({
            "uri": path_to_uri(&first.file),
            "range": span_to_lsp_range(&first.file, first.span)
        }))
    }

    fn formatting_response(&self, message: &Value) -> anyhow::Result<Value> {
        let uri = message
            .get("params")
            .and_then(|p| p.get("textDocument"))
            .and_then(|td| td.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if uri.is_empty() {
            return Ok(json!([]));
        }

        let source = self.document_text(&uri)?;
        let (program, diagnostics) = parser::parse(&source, &uri);
        if diagnostics.iter().any(|d| d.is_error()) {
            return Ok(json!([]));
        }
        let Some(program) = program else {
            return Ok(json!([]));
        };
        let ir = ir_builder::build(&program);
        let formatted = format_program(&ir);

        let range = full_document_range(&source);
        Ok(json!([
            {
                "range": range,
                "newText": formatted
            }
        ]))
    }

    fn publish_diagnostics(&self, uri: &str) -> anyhow::Result<Option<Value>> {
        let Some(path) = uri_to_path(uri) else {
            return Ok(None);
        };

        let front = run_frontend(&path)?;
        let canonical_target = fs::canonicalize(&path).unwrap_or(path.clone());
        let diagnostics = front
            .diagnostics
            .iter()
            .filter_map(|d| lsp_diagnostic_for_file(d, &canonical_target))
            .collect::<Vec<_>>();

        Ok(Some(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": diagnostics
            }
        })))
    }

    fn document_text(&self, uri: &str) -> anyhow::Result<String> {
        if let Some(text) = self.documents.get(uri) {
            return Ok(text.clone());
        }
        let path = uri_to_path(uri).ok_or_else(|| anyhow::anyhow!("unsupported URI: {uri}"))?;
        Ok(fs::read_to_string(path)?)
    }
}

fn request_position(message: &Value) -> anyhow::Result<(String, usize, usize)> {
    let params = message
        .get("params")
        .ok_or_else(|| anyhow::anyhow!("missing params"))?;
    let uri = params
        .get("textDocument")
        .and_then(|td| td.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing textDocument.uri"))?
        .to_string();
    let line = params
        .get("position")
        .and_then(|p| p.get("line"))
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing position.line"))? as usize;
    let character = params
        .get("position")
        .and_then(|p| p.get("character"))
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing position.character"))? as usize;
    Ok((uri, line, character))
}

fn lsp_diagnostic_for_file(
    diag: &crate::diagnostics::Diagnostic,
    target_file: &Path,
) -> Option<Value> {
    let span = diag.spans.first()?;
    let span_path = PathBuf::from(&span.file);
    let canonical_span = fs::canonicalize(&span_path).unwrap_or(span_path);
    if canonical_span != target_file {
        return None;
    }

    let range = span_to_lsp_range(target_file, Span::new(span.start, span.end));
    let severity = match diag.severity {
        crate::diagnostics::Severity::Error => 1,
        crate::diagnostics::Severity::Warning => 2,
        crate::diagnostics::Severity::Note => 3,
    };

    let mut message = diag.message.clone();
    if !diag.help.is_empty() {
        message.push_str("\nhelp:\n");
        for h in &diag.help {
            message.push_str("- ");
            message.push_str(h);
            message.push('\n');
        }
        message = message.trim_end().to_string();
    }

    Some(json!({
        "range": range,
        "severity": severity,
        "code": diag.code,
        "source": "aic",
        "message": message
    }))
}

fn build_symbol_index(entry_path: &Path) -> anyhow::Result<BTreeMap<String, Vec<SymbolDecl>>> {
    let root = find_project_root(entry_path);
    let mut files = Vec::new();
    collect_aic_files(&root, &mut files)?;
    files.sort();

    let mut map: BTreeMap<String, Vec<SymbolDecl>> = BTreeMap::new();
    for file in files {
        let source = match fs::read_to_string(&file) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let (program, diagnostics) = parser::parse(&source, &file.to_string_lossy());
        if diagnostics.iter().any(|d| d.is_error()) {
            continue;
        }
        let Some(program) = program else {
            continue;
        };

        for item in program.items {
            let decl = match item {
                ast::Item::Function(func) => SymbolDecl {
                    name: func.name.clone(),
                    kind: "function".to_string(),
                    signature: render_function_signature(&func),
                    file: file.clone(),
                    span: func.span,
                },
                ast::Item::Struct(strukt) => SymbolDecl {
                    name: strukt.name.clone(),
                    kind: "struct".to_string(),
                    signature: render_struct_signature(&strukt),
                    file: file.clone(),
                    span: strukt.span,
                },
                ast::Item::Enum(enm) => SymbolDecl {
                    name: enm.name.clone(),
                    kind: "enum".to_string(),
                    signature: render_enum_signature(&enm),
                    file: file.clone(),
                    span: enm.span,
                },
            };
            map.entry(decl.name.clone()).or_default().push(decl);
        }
    }

    for values in map.values_mut() {
        values.sort_by(|a, b| a.file.cmp(&b.file).then(a.span.start.cmp(&b.span.start)));
    }

    Ok(map)
}

fn render_function_signature(func: &ast::Function) -> String {
    let generics = if func.generics.is_empty() {
        String::new()
    } else {
        format!(
            "<{}>",
            func.generics
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let params = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, render_type_expr(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ");

    let effects = if func.effects.is_empty() {
        String::new()
    } else {
        format!(" effects {{ {} }}", func.effects.join(", "))
    };

    format!(
        "fn {}{}({}) -> {}{}",
        func.name,
        generics,
        params,
        render_type_expr(&func.ret_type),
        effects
    )
}

fn render_struct_signature(strukt: &ast::StructDef) -> String {
    let fields = strukt
        .fields
        .iter()
        .map(|f| format!("{}: {}", f.name, render_type_expr(&f.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("struct {} {{ {} }}", strukt.name, fields)
}

fn render_enum_signature(enm: &ast::EnumDef) -> String {
    let variants = enm
        .variants
        .iter()
        .map(|v| {
            if let Some(payload) = &v.payload {
                format!("{}({})", v.name, render_type_expr(payload))
            } else {
                v.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("enum {} {{ {} }}", enm.name, variants)
}

fn render_type_expr(ty: &ast::TypeExpr) -> String {
    match &ty.kind {
        ast::TypeKind::Unit => "Unit".to_string(),
        ast::TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let inner = args
                    .iter()
                    .map(render_type_expr)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", name, inner)
            }
        }
    }
}

fn find_project_root(entry: &Path) -> PathBuf {
    let mut dir = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    loop {
        if dir.join("aic.toml").exists() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            return entry
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
        };
        dir = parent.to_path_buf();
    }
}

fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        if path.is_dir() {
            if name == ".git" || name == "target" || name == ".aic-cache" {
                continue;
            }
            collect_aic_files(&path, out)?;
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("aic") {
            out.push(path);
        }
    }
    Ok(())
}

fn word_at_position(source: &str, line: usize, character: usize) -> Option<String> {
    let offset = line_char_to_offset(source, line, character)?;
    if offset > source.len() {
        return None;
    }

    let bytes = source.as_bytes();
    let mut start = offset.min(bytes.len());
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }

    let mut end = offset.min(bytes.len());
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }

    if end <= start {
        return None;
    }

    Some(source[start..end].to_string())
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn line_char_to_offset(source: &str, line: usize, character: usize) -> Option<usize> {
    let mut current_line = 0usize;
    let mut line_start = 0usize;

    for (idx, ch) in source.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = idx + 1;
        }
    }

    if current_line != line {
        return Some(source.len());
    }

    let mut col = 0usize;
    for (idx, ch) in source[line_start..].char_indices() {
        if col == character {
            return Some(line_start + idx);
        }
        if ch == '\n' {
            return Some(line_start + idx);
        }
        col += 1;
    }

    Some(source.len())
}

fn offset_to_line_char(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let mut line = 0usize;
    let mut char_pos = 0usize;

    for (idx, ch) in source.char_indices() {
        if idx >= clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            char_pos = 0;
        } else {
            char_pos += 1;
        }
    }

    (line, char_pos)
}

fn full_document_range(source: &str) -> Value {
    let (end_line, end_char) = offset_to_line_char(source, source.len());
    json!({
        "start": {"line": 0, "character": 0},
        "end": {"line": end_line, "character": end_char}
    })
}

fn span_to_lsp_range(file: &Path, span: Span) -> Value {
    let source = fs::read_to_string(file).unwrap_or_default();
    let (start_line, start_char) = offset_to_line_char(&source, span.start);
    let (end_line, end_char) = offset_to_line_char(&source, span.end.max(span.start + 1));

    json!({
        "start": {"line": start_line, "character": start_char},
        "end": {"line": end_line, "character": end_char}
    })
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    Some(PathBuf::from(path))
}

fn path_to_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

fn read_message(reader: &mut dyn BufRead) -> anyhow::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>()?);
        }
    }

    let len = content_length.ok_or_else(|| anyhow::anyhow!("missing Content-Length header"))?;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice::<Value>(&body)?;
    Ok(Some(value))
}

fn write_message(writer: &mut dyn Write, message: &Value) -> anyhow::Result<()> {
    let body = serde_json::to_string(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{full_document_range, line_char_to_offset, word_at_position};

    #[test]
    fn extracts_word_at_cursor() {
        let source = "fn main() -> Int { helper() }\n";
        let symbol = word_at_position(source, 0, 19).expect("symbol");
        assert_eq!(symbol, "helper");
    }

    #[test]
    fn maps_line_character_to_offset() {
        let source = "a\nxyz\n";
        assert_eq!(line_char_to_offset(source, 1, 2), Some(4));
    }

    #[test]
    fn full_document_range_covers_entire_text() {
        let source = "a\nxyz\n";
        let range = full_document_range(source);
        assert_eq!(range["start"]["line"], 0);
        assert_eq!(range["end"]["line"], 2);
    }
}
