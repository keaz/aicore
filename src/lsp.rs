use std::collections::{BTreeMap, BTreeSet};
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
use crate::std_policy;

struct LspServer {
    root_uri: Option<String>,
    shutdown_requested: bool,
    documents: BTreeMap<String, String>,
    inlay_type_hints: bool,
    inlay_effect_hints: bool,
    inlay_contract_hints: bool,
}

impl Default for LspServer {
    fn default() -> Self {
        Self {
            root_uri: None,
            shutdown_requested: false,
            documents: BTreeMap::new(),
            inlay_type_hints: true,
            inlay_effect_hints: true,
            inlay_contract_hints: false,
        }
    }
}

#[derive(Debug, Clone)]
struct SymbolDecl {
    name: String,
    kind: String,
    signature: String,
    file: PathBuf,
    span: Span,
}

#[derive(Debug, Clone)]
struct CallHierarchyDecl {
    name: String,
    module: Option<String>,
    file: PathBuf,
    span: Span,
}

#[derive(Debug, Clone)]
struct CallHierarchyCallSite {
    caller_name: String,
    caller_module: Option<String>,
    callee_name: String,
    callee_module: Option<String>,
    file: PathBuf,
    span: Span,
}

#[derive(Debug, Clone)]
struct CallHierarchyContext {
    call_graph: BTreeMap<String, Vec<String>>,
    declarations_by_name: BTreeMap<String, Vec<CallHierarchyDecl>>,
    call_sites_by_caller: BTreeMap<String, Vec<CallHierarchyCallSite>>,
}

const LSP_KEYWORDS: &[&str] = &[
    "module",
    "import",
    "async",
    "extern",
    "unsafe",
    "fn",
    "type",
    "const",
    "struct",
    "enum",
    "trait",
    "impl",
    "let",
    "mut",
    "return",
    "if",
    "else",
    "match",
    "for",
    "in",
    "while",
    "loop",
    "break",
    "continue",
    "true",
    "false",
    "requires",
    "ensures",
    "where",
    "invariant",
    "effects",
    "null",
    "await",
];

const SEMANTIC_TOKEN_TYPES: &[&str] = &[
    "namespace",
    "function",
    "struct",
    "enum",
    "interface",
    "keyword",
    "variable",
    "parameter",
    "property",
    "enumMember",
    "typeParameter",
    "comment",
    "decorator",
];

const SEMANTIC_TOKEN_MODIFIERS: &[&str] = &[
    "declaration",
    "definition",
    "mutable",
    "readonly",
    "deprecated",
    "async",
    "effectful",
];

const TOKEN_FUNCTION: usize = 1;
const TOKEN_STRUCT: usize = 2;
const TOKEN_ENUM: usize = 3;
const TOKEN_INTERFACE: usize = 4;
const TOKEN_KEYWORD: usize = 5;
const TOKEN_VARIABLE: usize = 6;
const TOKEN_PARAMETER: usize = 7;
const TOKEN_PROPERTY: usize = 8;
const TOKEN_ENUM_MEMBER: usize = 9;
const TOKEN_TYPE_PARAMETER: usize = 10;
const TOKEN_COMMENT: usize = 11;
const TOKEN_DECORATOR: usize = 12;

const MOD_DECLARATION: u32 = 1 << 0;
const MOD_DEFINITION: u32 = 1 << 1;
const MOD_MUTABLE: u32 = 1 << 2;
const MOD_READONLY: u32 = 1 << 3;
const MOD_DEPRECATED: u32 = 1 << 4;
const MOD_ASYNC: u32 = 1 << 5;
const MOD_EFFECTFUL: u32 = 1 << 6;

#[derive(Debug, Clone, Copy)]
struct SemanticToken {
    line: usize,
    character: usize,
    length: usize,
    token_type: usize,
    token_modifiers: u32,
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
                                "documentSymbolProvider": true,
                                "workspaceSymbolProvider": true,
                                "documentFormattingProvider": true,
                                "completionProvider": {
                                    "resolveProvider": false,
                                    "triggerCharacters": [".", ":"]
                                },
                                "renameProvider": true,
                                "codeActionProvider": true,
                                "semanticTokensProvider": {
                                    "legend": {
                                        "tokenTypes": SEMANTIC_TOKEN_TYPES,
                                        "tokenModifiers": SEMANTIC_TOKEN_MODIFIERS
                                    },
                                    "full": true
                                },
                                "inlayHintProvider": true,
                                "callHierarchyProvider": true,
                                "foldingRangeProvider": true,
                                "selectionRangeProvider": true
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
            "workspace/didChangeConfiguration" => {
                self.update_inlay_hint_settings(message);
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
            "textDocument/prepareCallHierarchy" => {
                if let Some(id) = id {
                    let result = self.prepare_call_hierarchy_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "callHierarchy/incomingCalls" => {
                if let Some(id) = id {
                    let result = self.call_hierarchy_incoming_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "callHierarchy/outgoingCalls" => {
                if let Some(id) = id {
                    let result = self.call_hierarchy_outgoing_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/documentSymbol" => {
                if let Some(id) = id {
                    let result = self.document_symbol_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/inlayHint" => {
                if let Some(id) = id {
                    let result = self.inlay_hint_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/foldingRange" => {
                if let Some(id) = id {
                    let result = self.folding_range_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/selectionRange" => {
                if let Some(id) = id {
                    let result = self.selection_range_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "workspace/symbol" => {
                if let Some(id) = id {
                    let result = self.workspace_symbol_response(message)?;
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
            "textDocument/completion" => {
                if let Some(id) = id {
                    let result = self.completion_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/rename" => {
                if let Some(id) = id {
                    let result = self.rename_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/codeAction" => {
                if let Some(id) = id {
                    let result = self.code_action_response(message)?;
                    outbound.push(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    }));
                }
            }
            "textDocument/semanticTokens/full" => {
                if let Some(id) = id {
                    let result = self.semantic_tokens_full_response(message)?;
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

    fn prepare_call_hierarchy_response(&self, message: &Value) -> anyhow::Result<Value> {
        let (uri, line, character) = request_position(message)?;
        let source = self.document_text(&uri)?;
        let Some(symbol) = word_at_position(&source, line, character) else {
            return Ok(json!([]));
        };
        let Some(path) = uri_to_path(&uri) else {
            return Ok(json!([]));
        };

        let context = build_call_hierarchy_context(&path)?;
        let Some(decls) = context.declarations_by_name.get(&symbol) else {
            return Ok(json!([]));
        };

        let offset = line_char_to_offset(&source, line, character).unwrap_or(0);
        let canonical_path = fs::canonicalize(&path).unwrap_or(path.clone());
        let mut in_place = Vec::new();
        let mut fallback = Vec::new();

        for decl in decls {
            let canonical_decl = fs::canonicalize(&decl.file).unwrap_or(decl.file.clone());
            if canonical_decl == canonical_path
                && offset >= decl.span.start
                && offset <= decl.span.end.max(decl.span.start + 1)
            {
                in_place.push(call_hierarchy_item_for_decl(decl));
            } else {
                fallback.push(call_hierarchy_item_for_decl(decl));
            }
        }

        if !in_place.is_empty() {
            return Ok(json!(in_place));
        }
        Ok(json!(fallback))
    }

    fn call_hierarchy_incoming_response(&self, message: &Value) -> anyhow::Result<Value> {
        let item = message
            .get("params")
            .and_then(|params| params.get("item"))
            .cloned()
            .unwrap_or(Value::Null);
        let Some(uri) = item.get("uri").and_then(Value::as_str) else {
            return Ok(json!([]));
        };
        let Some(path) = uri_to_path(uri) else {
            return Ok(json!([]));
        };
        let Some(target_name) = call_hierarchy_item_name(&item) else {
            return Ok(json!([]));
        };
        let target_module = call_hierarchy_item_module(&item);

        let context = build_call_hierarchy_context(&path)?;
        let inverse = build_inverse_call_graph(&context.call_graph);
        let Some(caller_names) = inverse.get(&target_name) else {
            return Ok(json!([]));
        };

        let mut incoming = Vec::new();
        for caller_name in caller_names {
            let Some(caller_decls) = context.declarations_by_name.get(caller_name) else {
                continue;
            };
            let caller_sites = context
                .call_sites_by_caller
                .get(caller_name)
                .cloned()
                .unwrap_or_default();

            for caller_decl in caller_decls {
                let mut from_ranges = caller_sites
                    .iter()
                    .filter(|site| {
                        site.caller_name == caller_decl.name
                            && modules_match(
                                caller_decl.module.as_deref(),
                                site.caller_module.as_deref(),
                            )
                            && site_targets_function(site, &target_name, target_module.as_deref())
                    })
                    .map(|site| span_to_lsp_range(&site.file, site.span))
                    .collect::<Vec<_>>();
                if from_ranges.is_empty() {
                    continue;
                }

                from_ranges.sort_by_key(call_hierarchy_range_sort_key);
                from_ranges.dedup_by(|lhs, rhs| lhs == rhs);

                incoming.push(json!({
                    "from": call_hierarchy_item_for_decl(caller_decl),
                    "fromRanges": from_ranges
                }));
            }
        }

        incoming.sort_by(|lhs, rhs| {
            lhs.get("from")
                .and_then(|from| from.get("name"))
                .and_then(Value::as_str)
                .cmp(
                    &rhs.get("from")
                        .and_then(|from| from.get("name"))
                        .and_then(Value::as_str),
                )
        });
        Ok(json!(incoming))
    }

    fn call_hierarchy_outgoing_response(&self, message: &Value) -> anyhow::Result<Value> {
        let item = message
            .get("params")
            .and_then(|params| params.get("item"))
            .cloned()
            .unwrap_or(Value::Null);
        let Some(uri) = item.get("uri").and_then(Value::as_str) else {
            return Ok(json!([]));
        };
        let Some(path) = uri_to_path(uri) else {
            return Ok(json!([]));
        };
        let Some(caller_name) = call_hierarchy_item_name(&item) else {
            return Ok(json!([]));
        };
        let caller_module = call_hierarchy_item_module(&item);

        let context = build_call_hierarchy_context(&path)?;
        let outgoing_names = context
            .call_graph
            .get(&caller_name)
            .cloned()
            .unwrap_or_default();
        if outgoing_names.is_empty() {
            return Ok(json!([]));
        }

        let caller_sites = context
            .call_sites_by_caller
            .get(&caller_name)
            .cloned()
            .unwrap_or_default();
        let mut outgoing = Vec::new();

        for callee_name in outgoing_names {
            let Some(callee_decls) = context.declarations_by_name.get(&callee_name) else {
                continue;
            };
            for callee_decl in callee_decls {
                let mut from_ranges = caller_sites
                    .iter()
                    .filter(|site| {
                        modules_match(caller_module.as_deref(), site.caller_module.as_deref())
                            && site_targets_decl(site, callee_decl)
                    })
                    .map(|site| span_to_lsp_range(&site.file, site.span))
                    .collect::<Vec<_>>();
                if from_ranges.is_empty() {
                    continue;
                }
                from_ranges.sort_by_key(call_hierarchy_range_sort_key);
                from_ranges.dedup_by(|lhs, rhs| lhs == rhs);
                outgoing.push(json!({
                    "to": call_hierarchy_item_for_decl(callee_decl),
                    "fromRanges": from_ranges
                }));
            }
        }

        outgoing.sort_by(|lhs, rhs| {
            lhs.get("to")
                .and_then(|to| to.get("name"))
                .and_then(Value::as_str)
                .cmp(
                    &rhs.get("to")
                        .and_then(|to| to.get("name"))
                        .and_then(Value::as_str),
                )
        });
        Ok(json!(outgoing))
    }

    fn document_symbol_response(&self, message: &Value) -> anyhow::Result<Value> {
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
        let Some(path) = uri_to_path(&uri) else {
            return Ok(json!([]));
        };

        let source = self.document_text(&uri)?;
        let mut symbols = document_symbols_for_file(&path, &source)?;
        symbols.sort_by_key(|entry| {
            entry
                .get("range")
                .and_then(|range| range.get("start"))
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        });
        Ok(json!(symbols))
    }

    fn workspace_symbol_response(&self, message: &Value) -> anyhow::Result<Value> {
        let query = message
            .get("params")
            .and_then(|p| p.get("query"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_lowercase();

        let root = self
            .root_uri
            .as_deref()
            .and_then(uri_to_path)
            .or_else(|| {
                self.documents
                    .keys()
                    .next()
                    .and_then(|uri| uri_to_path(uri))
                    .map(|path| find_project_root(&path))
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let declarations = build_symbol_index(&root)?;
        let mut symbols = Vec::<(&SymbolDecl, Value)>::new();
        for decls in declarations.values() {
            for decl in decls {
                if !query.is_empty() {
                    let name_match = decl.name.to_lowercase().contains(&query);
                    let sig_match = decl.signature.to_lowercase().contains(&query);
                    if !name_match && !sig_match {
                        continue;
                    }
                }
                symbols.push((decl, symbol_decl_to_workspace_symbol(decl)));
            }
        }

        symbols.sort_by(|(a, _), (b, _)| {
            a.name
                .cmp(&b.name)
                .then(a.file.cmp(&b.file))
                .then(a.span.start.cmp(&b.span.start))
        });

        Ok(json!(symbols
            .into_iter()
            .take(200)
            .map(|(_, value)| value)
            .collect::<Vec<_>>()))
    }

    fn update_inlay_hint_settings(&mut self, message: &Value) {
        let settings = message
            .get("params")
            .and_then(|params| params.get("settings"))
            .and_then(|settings| settings.get("aic"))
            .and_then(|aic| aic.get("inlayHints"));
        let Some(settings) = settings else {
            return;
        };

        if let Some(value) = settings.get("typeAnnotations").and_then(Value::as_bool) {
            self.inlay_type_hints = value;
        }
        if let Some(value) = settings.get("effectAnnotations").and_then(Value::as_bool) {
            self.inlay_effect_hints = value;
        }
        if let Some(value) = settings.get("contractAnnotations").and_then(Value::as_bool) {
            self.inlay_contract_hints = value;
        }
    }

    fn inlay_hint_response(&self, message: &Value) -> anyhow::Result<Value> {
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
        let Some(path) = uri_to_path(&uri) else {
            return Ok(json!([]));
        };

        let source = self.document_text(&uri)?;
        let requested_range = message
            .get("params")
            .and_then(|p| p.get("range"))
            .cloned()
            .unwrap_or_else(|| full_document_range(&source));

        let mut hints = collect_inlay_hints(
            &path,
            &source,
            self.inlay_type_hints,
            self.inlay_effect_hints,
            self.inlay_contract_hints,
        )?;
        hints.retain(|hint| inlay_hint_in_range(hint, &requested_range));
        hints.sort_by(|a, b| {
            a["position"]["line"]
                .as_u64()
                .cmp(&b["position"]["line"].as_u64())
                .then(
                    a["position"]["character"]
                        .as_u64()
                        .cmp(&b["position"]["character"].as_u64()),
                )
                .then(a["label"].as_str().cmp(&b["label"].as_str()))
        });
        Ok(json!(hints))
    }

    fn folding_range_response(&self, message: &Value) -> anyhow::Result<Value> {
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

        let mut ranges = collect_ast_folding_ranges(&program, &source);
        ranges.extend(collect_comment_folding_ranges(&source));
        ranges.sort_by_key(folding_range_sort_key);
        ranges.dedup_by(|lhs, rhs| lhs == rhs);
        Ok(json!(ranges))
    }

    fn selection_range_response(&self, message: &Value) -> anyhow::Result<Value> {
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
        let positions = message
            .get("params")
            .and_then(|p| p.get("positions"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if positions.is_empty() {
            return Ok(json!([]));
        }

        let (program, diagnostics) = parser::parse(&source, &uri);
        if diagnostics.iter().any(|d| d.is_error()) {
            return Ok(json!(positions
                .into_iter()
                .map(|position| fallback_selection_range(&position))
                .collect::<Vec<_>>()));
        }
        let Some(program) = program else {
            return Ok(json!(positions
                .into_iter()
                .map(|position| fallback_selection_range(&position))
                .collect::<Vec<_>>()));
        };

        let mut ranges = Vec::new();
        for position in positions {
            let line = position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
            let character = position
                .get("character")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let offset = line_char_to_offset(&source, line, character).unwrap_or(source.len());
            ranges.push(selection_chain_for_offset(
                &program, &source, offset, line, character,
            ));
        }

        Ok(json!(ranges))
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

    fn completion_response(&self, message: &Value) -> anyhow::Result<Value> {
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

        let Some(path) = uri_to_path(&uri) else {
            return Ok(json!([]));
        };
        let declarations = build_symbol_index(&path)?;
        let mut items = Vec::new();
        for (name, decls) in declarations {
            let first = &decls[0];
            items.push(json!({
                "label": name,
                "kind": completion_kind(&first.kind),
                "detail": first.signature,
                "sortText": format!("1-{}", first.name)
            }));
        }
        for keyword in LSP_KEYWORDS {
            items.push(json!({
                "label": keyword,
                "kind": 14,
                "detail": "keyword",
                "sortText": format!("2-{}", keyword)
            }));
        }
        items.sort_by(|a, b| {
            a["sortText"]
                .as_str()
                .cmp(&b["sortText"].as_str())
                .then(a["label"].as_str().cmp(&b["label"].as_str()))
        });
        Ok(json!(items))
    }

    fn rename_response(&self, message: &Value) -> anyhow::Result<Value> {
        let (uri, line, character) = request_position(message)?;
        let new_name = message
            .get("params")
            .and_then(|p| p.get("newName"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if new_name.is_empty() {
            return Ok(Value::Null);
        }

        let source = self.document_text(&uri)?;
        let Some(old_name) = word_at_position(&source, line, character) else {
            return Ok(Value::Null);
        };
        if old_name == new_name {
            return Ok(json!({ "changes": {} }));
        }

        let Some(entry_path) = uri_to_path(&uri) else {
            return Ok(Value::Null);
        };
        let root = find_project_root(&entry_path);
        let mut files = Vec::new();
        collect_aic_files(&root, &mut files)?;
        files.sort();

        let mut changes = BTreeMap::<String, Vec<Value>>::new();
        for file in files {
            let file_source = fs::read_to_string(&file)?;
            let mut edits = find_word_occurrences(&file_source, &old_name)
                .into_iter()
                .map(|(start, end)| {
                    json!({
                        "range": offset_range_to_lsp_range(&file_source, start, end),
                        "newText": new_name.clone()
                    })
                })
                .collect::<Vec<_>>();
            if edits.is_empty() {
                continue;
            }
            edits.sort_by(|a, b| {
                a["range"]["start"]["line"]
                    .as_u64()
                    .cmp(&b["range"]["start"]["line"].as_u64())
                    .then(
                        a["range"]["start"]["character"]
                            .as_u64()
                            .cmp(&b["range"]["start"]["character"].as_u64()),
                    )
            });
            changes.insert(path_to_uri(&file), edits);
        }

        Ok(json!({ "changes": changes }))
    }

    fn code_action_response(&self, message: &Value) -> anyhow::Result<Value> {
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

        let Some(path) = uri_to_path(&uri) else {
            return Ok(json!([]));
        };
        let front = run_frontend(&path)?;
        let canonical_target = fs::canonicalize(&path).unwrap_or(path.clone());
        let source = self.document_text(&uri)?;

        let requested_range = message
            .get("params")
            .and_then(|p| p.get("range"))
            .cloned()
            .unwrap_or_else(|| full_document_range(&source));

        let mut actions = Vec::new();
        for diag in &front.diagnostics {
            let Some(diag_span) = diag.spans.first() else {
                continue;
            };
            let span_path = PathBuf::from(&diag_span.file);
            let canonical_span = fs::canonicalize(&span_path).unwrap_or(span_path);
            if canonical_span != canonical_target {
                continue;
            }
            let Some(lsp_diag) = lsp_diagnostic_for_file(diag, &canonical_target) else {
                continue;
            };
            for fix in &diag.suggested_fixes {
                let (Some(start), Some(end), Some(replacement)) =
                    (fix.start, fix.end, fix.replacement.clone())
                else {
                    continue;
                };
                let range = offset_range_to_lsp_range(&source, start, end);
                if !ranges_intersect(&requested_range, &range) {
                    continue;
                }
                actions.push(json!({
                    "title": format!("aic: {}", fix.message),
                    "kind": "quickfix",
                    "diagnostics": [lsp_diag.clone()],
                    "edit": {
                        "changes": {
                            uri.clone(): [
                                {
                                    "range": range,
                                    "newText": replacement
                                }
                            ]
                        }
                    }
                }));
            }
        }

        actions.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
        Ok(json!(actions))
    }

    fn semantic_tokens_full_response(&self, message: &Value) -> anyhow::Result<Value> {
        let uri = message
            .get("params")
            .and_then(|p| p.get("textDocument"))
            .and_then(|td| td.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if uri.is_empty() {
            return Ok(json!({ "data": [] }));
        }
        let source = self.document_text(&uri)?;
        let (program, diagnostics) = parser::parse(&source, &uri);
        if diagnostics.iter().any(|d| d.is_error()) {
            return Ok(json!({ "data": [] }));
        }
        let Some(program) = program else {
            return Ok(json!({ "data": [] }));
        };

        let mut tokens = Vec::new();
        let effectful_functions = collect_effectful_function_names(&program);

        for item in &program.items {
            match item {
                ast::Item::Function(func) => {
                    let mut modifiers = MOD_DECLARATION | MOD_DEFINITION;
                    if func.is_async {
                        modifiers |= MOD_ASYNC;
                    }
                    if !func.effects.is_empty() {
                        modifiers |= MOD_EFFECTFUL;
                    }
                    if is_deprecated_symbol(&func.name) {
                        modifiers |= MOD_DEPRECATED;
                    }
                    push_named_semantic_token(
                        &mut tokens,
                        &source,
                        &func.name,
                        func.span,
                        TOKEN_FUNCTION,
                        modifiers,
                    );
                    push_generic_param_tokens(&mut tokens, &source, &func.generics);
                    for param in &func.params {
                        push_named_semantic_token(
                            &mut tokens,
                            &source,
                            &param.name,
                            param.span,
                            TOKEN_PARAMETER,
                            MOD_DECLARATION | MOD_DEFINITION,
                        );
                    }
                    collect_block_semantic_tokens(
                        &mut tokens,
                        &source,
                        &func.body,
                        &effectful_functions,
                    );
                }
                ast::Item::Struct(strukt) => {
                    push_named_semantic_token(
                        &mut tokens,
                        &source,
                        &strukt.name,
                        strukt.span,
                        TOKEN_STRUCT,
                        MOD_DECLARATION | MOD_DEFINITION,
                    );
                    push_generic_param_tokens(&mut tokens, &source, &strukt.generics);
                    for field in &strukt.fields {
                        push_named_semantic_token(
                            &mut tokens,
                            &source,
                            &field.name,
                            field.span,
                            TOKEN_PROPERTY,
                            MOD_DECLARATION | MOD_DEFINITION,
                        );
                    }
                }
                ast::Item::Enum(enm) => {
                    push_named_semantic_token(
                        &mut tokens,
                        &source,
                        &enm.name,
                        enm.span,
                        TOKEN_ENUM,
                        MOD_DECLARATION | MOD_DEFINITION,
                    );
                    push_generic_param_tokens(&mut tokens, &source, &enm.generics);
                    for variant in &enm.variants {
                        push_named_semantic_token(
                            &mut tokens,
                            &source,
                            &variant.name,
                            variant.span,
                            TOKEN_ENUM_MEMBER,
                            MOD_DECLARATION | MOD_DEFINITION,
                        );
                    }
                }
                ast::Item::Trait(trait_def) => {
                    push_named_semantic_token(
                        &mut tokens,
                        &source,
                        &trait_def.name,
                        trait_def.span,
                        TOKEN_INTERFACE,
                        MOD_DECLARATION | MOD_DEFINITION,
                    );
                    push_generic_param_tokens(&mut tokens, &source, &trait_def.generics);
                    for method in &trait_def.methods {
                        let mut modifiers = MOD_DECLARATION | MOD_DEFINITION;
                        if method.is_async {
                            modifiers |= MOD_ASYNC;
                        }
                        if !method.effects.is_empty() {
                            modifiers |= MOD_EFFECTFUL;
                        }
                        if is_deprecated_symbol(&method.name) {
                            modifiers |= MOD_DEPRECATED;
                        }
                        push_named_semantic_token(
                            &mut tokens,
                            &source,
                            &method.name,
                            method.span,
                            TOKEN_FUNCTION,
                            modifiers,
                        );
                        push_generic_param_tokens(&mut tokens, &source, &method.generics);
                        for param in &method.params {
                            push_named_semantic_token(
                                &mut tokens,
                                &source,
                                &param.name,
                                param.span,
                                TOKEN_PARAMETER,
                                MOD_DECLARATION | MOD_DEFINITION,
                            );
                        }
                        collect_block_semantic_tokens(
                            &mut tokens,
                            &source,
                            &method.body,
                            &effectful_functions,
                        );
                    }
                }
                ast::Item::Impl(impl_def) => {
                    for method in &impl_def.methods {
                        let mut modifiers = MOD_DECLARATION | MOD_DEFINITION;
                        if method.is_async {
                            modifiers |= MOD_ASYNC;
                        }
                        if !method.effects.is_empty() {
                            modifiers |= MOD_EFFECTFUL;
                        }
                        if is_deprecated_symbol(&method.name) {
                            modifiers |= MOD_DEPRECATED;
                        }
                        push_named_semantic_token(
                            &mut tokens,
                            &source,
                            &method.name,
                            method.span,
                            TOKEN_FUNCTION,
                            modifiers,
                        );
                        push_generic_param_tokens(&mut tokens, &source, &method.generics);
                        for param in &method.params {
                            push_named_semantic_token(
                                &mut tokens,
                                &source,
                                &param.name,
                                param.span,
                                TOKEN_PARAMETER,
                                MOD_DECLARATION | MOD_DEFINITION,
                            );
                        }
                        collect_block_semantic_tokens(
                            &mut tokens,
                            &source,
                            &method.body,
                            &effectful_functions,
                        );
                    }
                }
            }
        }

        collect_const_semantic_tokens(&mut tokens, &source);
        collect_comment_and_decorator_tokens(&mut tokens, &source);

        for keyword in LSP_KEYWORDS {
            for (start, end) in find_word_occurrences(&source, keyword) {
                push_offset_semantic_token(&mut tokens, &source, start, end, TOKEN_KEYWORD, 0);
            }
        }

        tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.character.cmp(&b.character)));
        tokens.dedup_by(|lhs, rhs| {
            lhs.line == rhs.line
                && lhs.character == rhs.character
                && lhs.length == rhs.length
                && lhs.token_type == rhs.token_type
                && lhs.token_modifiers == rhs.token_modifiers
        });

        let mut data = Vec::<u32>::new();
        let mut prev_line = 0usize;
        let mut prev_char = 0usize;
        for token in tokens {
            let delta_line = token.line.saturating_sub(prev_line);
            let delta_start = if delta_line == 0 {
                token.character.saturating_sub(prev_char)
            } else {
                token.character
            };
            data.push(delta_line as u32);
            data.push(delta_start as u32);
            data.push(token.length as u32);
            data.push(token.token_type as u32);
            data.push(token.token_modifiers);
            prev_line = token.line;
            prev_char = token.character;
        }

        Ok(json!({ "data": data }))
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

fn collect_effectful_function_names(program: &ast::Program) -> Vec<String> {
    let mut names = Vec::new();
    for item in &program.items {
        match item {
            ast::Item::Function(func) => {
                if !func.effects.is_empty() {
                    names.push(func.name.clone());
                }
            }
            ast::Item::Trait(trait_def) => {
                for method in &trait_def.methods {
                    if !method.effects.is_empty() {
                        names.push(method.name.clone());
                    }
                }
            }
            ast::Item::Impl(impl_def) => {
                for method in &impl_def.methods {
                    if !method.effects.is_empty() {
                        names.push(method.name.clone());
                    }
                }
            }
            ast::Item::Struct(_) | ast::Item::Enum(_) => {}
        }
    }
    names.sort();
    names.dedup();
    names
}

fn is_effectful_call(name: &str, effectful_functions: &[String]) -> bool {
    effectful_functions
        .iter()
        .any(|candidate| candidate == name)
}

fn is_deprecated_symbol(name: &str) -> bool {
    std_policy::DEPRECATED_APIS
        .iter()
        .any(|entry| entry.symbol == name)
}

fn push_named_semantic_token(
    tokens: &mut Vec<SemanticToken>,
    source: &str,
    name: &str,
    span: Span,
    token_type: usize,
    token_modifiers: u32,
) {
    if let Some(offset) = find_name_offset_in_span(source, name, span) {
        let end = offset.saturating_add(name.len());
        push_offset_semantic_token(tokens, source, offset, end, token_type, token_modifiers);
    }
}

fn push_offset_semantic_token(
    tokens: &mut Vec<SemanticToken>,
    source: &str,
    start: usize,
    end: usize,
    token_type: usize,
    token_modifiers: u32,
) {
    let (line, character) = offset_to_line_char(source, start);
    let length = end.saturating_sub(start);
    if length == 0 {
        return;
    }
    tokens.push(SemanticToken {
        line,
        character,
        length,
        token_type,
        token_modifiers,
    });
}

fn push_generic_param_tokens(
    tokens: &mut Vec<SemanticToken>,
    source: &str,
    generic_params: &[ast::GenericParam],
) {
    for generic in generic_params {
        push_named_semantic_token(
            tokens,
            source,
            &generic.name,
            generic.span,
            TOKEN_TYPE_PARAMETER,
            MOD_DECLARATION | MOD_DEFINITION,
        );
    }
}

fn collect_block_semantic_tokens(
    tokens: &mut Vec<SemanticToken>,
    source: &str,
    block: &ast::Block,
    effectful_functions: &[String],
) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let {
                name,
                mutable,
                expr,
                span,
                ..
            } => {
                let mut modifiers = MOD_DECLARATION;
                if *mutable {
                    modifiers |= MOD_MUTABLE;
                } else {
                    modifiers |= MOD_READONLY;
                }
                push_named_semantic_token(tokens, source, name, *span, TOKEN_VARIABLE, modifiers);
                collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
            }
            ast::Stmt::Assign { expr, .. } => {
                collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
            }
            ast::Stmt::Expr { expr, .. } => {
                collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
            }
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
                }
            }
            ast::Stmt::Assert { expr, .. } => {
                collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
            }
        }
    }

    if let Some(tail) = &block.tail {
        collect_expr_semantic_tokens(tokens, source, tail, effectful_functions);
    }
}

fn collect_expr_semantic_tokens(
    tokens: &mut Vec<SemanticToken>,
    source: &str,
    expr: &ast::Expr,
    effectful_functions: &[String],
) {
    match &expr.kind {
        ast::ExprKind::Call { callee, args } => {
            if let ast::ExprKind::Var(name) = &callee.kind {
                let mut modifiers = 0u32;
                if is_effectful_call(name, effectful_functions) {
                    modifiers |= MOD_EFFECTFUL;
                }
                if is_deprecated_symbol(name) {
                    modifiers |= MOD_DEPRECATED;
                }
                push_named_semantic_token(
                    tokens,
                    source,
                    name,
                    callee.span,
                    TOKEN_FUNCTION,
                    modifiers,
                );
            }
            collect_expr_semantic_tokens(tokens, source, callee, effectful_functions);
            for arg in args {
                collect_expr_semantic_tokens(tokens, source, arg, effectful_functions);
            }
        }
        ast::ExprKind::Closure {
            params,
            ret_type: _,
            body,
        } => {
            for param in params {
                push_named_semantic_token(
                    tokens,
                    source,
                    &param.name,
                    param.span,
                    TOKEN_PARAMETER,
                    MOD_DECLARATION | MOD_DEFINITION,
                );
            }
            collect_block_semantic_tokens(tokens, source, body, effectful_functions);
        }
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr_semantic_tokens(tokens, source, cond, effectful_functions);
            collect_block_semantic_tokens(tokens, source, then_block, effectful_functions);
            collect_block_semantic_tokens(tokens, source, else_block, effectful_functions);
        }
        ast::ExprKind::While { cond, body } => {
            collect_expr_semantic_tokens(tokens, source, cond, effectful_functions);
            collect_block_semantic_tokens(tokens, source, body, effectful_functions);
        }
        ast::ExprKind::Loop { body } => {
            collect_block_semantic_tokens(tokens, source, body, effectful_functions);
        }
        ast::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
            }
        }
        ast::ExprKind::Match { expr, arms } => {
            collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
            for arm in arms {
                collect_pattern_semantic_tokens(tokens, source, &arm.pattern);
                if let Some(guard) = &arm.guard {
                    collect_expr_semantic_tokens(tokens, source, guard, effectful_functions);
                }
                collect_expr_semantic_tokens(tokens, source, &arm.body, effectful_functions);
            }
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_semantic_tokens(tokens, source, lhs, effectful_functions);
            collect_expr_semantic_tokens(tokens, source, rhs, effectful_functions);
        }
        ast::ExprKind::Unary { expr, .. } => {
            collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
        }
        ast::ExprKind::Borrow { expr, .. } => {
            collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
        }
        ast::ExprKind::Await { expr } => {
            collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
        }
        ast::ExprKind::Try { expr } => {
            collect_expr_semantic_tokens(tokens, source, expr, effectful_functions);
        }
        ast::ExprKind::UnsafeBlock { block } => {
            collect_block_semantic_tokens(tokens, source, block, effectful_functions);
        }
        ast::ExprKind::StructInit { name, fields } => {
            push_named_semantic_token(tokens, source, name, expr.span, TOKEN_STRUCT, 0);
            for (field_name, field_expr, field_span) in fields {
                push_named_semantic_token(
                    tokens,
                    source,
                    field_name,
                    *field_span,
                    TOKEN_PROPERTY,
                    0,
                );
                collect_expr_semantic_tokens(tokens, source, field_expr, effectful_functions);
            }
        }
        ast::ExprKind::FieldAccess { base, field } => {
            collect_expr_semantic_tokens(tokens, source, base, effectful_functions);
            push_named_semantic_token(tokens, source, field, expr.span, TOKEN_PROPERTY, 0);
        }
        ast::ExprKind::Var(name) => {
            push_named_semantic_token(tokens, source, name, expr.span, TOKEN_VARIABLE, 0);
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Continue
        | ast::ExprKind::Unit => {}
    }
}

fn collect_pattern_semantic_tokens(
    tokens: &mut Vec<SemanticToken>,
    source: &str,
    pattern: &ast::Pattern,
) {
    match &pattern.kind {
        ast::PatternKind::Or { patterns } => {
            for inner in patterns {
                collect_pattern_semantic_tokens(tokens, source, inner);
            }
        }
        ast::PatternKind::Variant { name, args } => {
            push_named_semantic_token(tokens, source, name, pattern.span, TOKEN_ENUM_MEMBER, 0);
            for arg in args {
                collect_pattern_semantic_tokens(tokens, source, arg);
            }
        }
        ast::PatternKind::Var(name) => {
            push_named_semantic_token(
                tokens,
                source,
                name,
                pattern.span,
                TOKEN_VARIABLE,
                MOD_DECLARATION,
            );
        }
        ast::PatternKind::Wildcard
        | ast::PatternKind::Int(_)
        | ast::PatternKind::Bool(_)
        | ast::PatternKind::Unit => {}
    }
}

fn collect_const_semantic_tokens(tokens: &mut Vec<SemanticToken>, source: &str) {
    for (_start, end) in find_word_occurrences(source, "const") {
        let Some((name_start, name_end)) = identifier_after(source, end) else {
            continue;
        };
        push_offset_semantic_token(
            tokens,
            source,
            name_start,
            name_end,
            TOKEN_VARIABLE,
            MOD_DECLARATION | MOD_DEFINITION | MOD_READONLY,
        );
    }
}

fn collect_comment_and_decorator_tokens(tokens: &mut Vec<SemanticToken>, source: &str) {
    let mut line_start = 0usize;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len().saturating_sub(trimmed.len());
        if trimmed.starts_with("///") {
            let start = line_start + indent;
            let end = line_start + line.trim_end_matches(['\r', '\n']).len();
            push_offset_semantic_token(tokens, source, start, end, TOKEN_COMMENT, 0);
        } else if trimmed.starts_with("#[") {
            let decorator_len = trimmed
                .find(']')
                .map(|idx| idx + 1)
                .unwrap_or_else(|| trimmed.trim_end_matches(['\r', '\n']).len());
            let start = line_start + indent;
            let end = start.saturating_add(decorator_len);
            push_offset_semantic_token(tokens, source, start, end, TOKEN_DECORATOR, 0);
        }
        line_start = line_start.saturating_add(line.len());
    }
}

fn identifier_after(source: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut cursor = from.min(bytes.len());
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if cursor >= bytes.len() {
        return None;
    }
    if !(bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_') {
        return None;
    }
    let start = cursor;
    cursor += 1;
    while cursor < bytes.len() && is_word_byte(bytes[cursor]) {
        cursor += 1;
    }
    Some((start, cursor))
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
    let root = symbol_index_root(entry_path);
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
                ast::Item::Trait(trait_def) => SymbolDecl {
                    name: trait_def.name.clone(),
                    kind: "trait".to_string(),
                    signature: render_trait_signature(&trait_def),
                    file: file.clone(),
                    span: trait_def.span,
                },
                ast::Item::Impl(impl_def) => SymbolDecl {
                    name: impl_def.trait_name.clone(),
                    kind: "impl".to_string(),
                    signature: render_impl_signature(&impl_def),
                    file: file.clone(),
                    span: impl_def.span,
                },
            };
            map.entry(decl.name.clone()).or_default().push(decl);
        }

        for decl in extract_text_symbol_decls(&source, &file) {
            map.entry(decl.name.clone()).or_default().push(decl);
        }
    }

    for values in map.values_mut() {
        values.sort_by(|a, b| a.file.cmp(&b.file).then(a.span.start.cmp(&b.span.start)));
    }

    Ok(map)
}

fn build_call_hierarchy_context(entry_path: &Path) -> anyhow::Result<CallHierarchyContext> {
    let root = symbol_index_root(entry_path);
    let mut files = Vec::new();
    collect_aic_files(&root, &mut files)?;
    files.sort();

    let mut call_graph = BTreeMap::<String, Vec<String>>::new();
    let mut declarations_by_name = BTreeMap::<String, Vec<CallHierarchyDecl>>::new();
    let mut call_sites_by_caller = BTreeMap::<String, Vec<CallHierarchyCallSite>>::new();

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

        let module_name = program.module.as_ref().map(|module| module.path.join("."));

        for item in program.items {
            match item {
                ast::Item::Function(func) => {
                    call_graph.entry(func.name.clone()).or_default();
                    declarations_by_name
                        .entry(func.name.clone())
                        .or_default()
                        .push(CallHierarchyDecl {
                            name: func.name.clone(),
                            module: module_name.clone(),
                            file: file.clone(),
                            span: func.span,
                        });

                    collect_block_call_sites(
                        &func.body,
                        &file,
                        module_name.clone(),
                        &func.name,
                        &mut call_sites_by_caller,
                    );
                }
                ast::Item::Trait(trait_def) => {
                    for method in trait_def.methods {
                        call_graph.entry(method.name.clone()).or_default();
                        declarations_by_name
                            .entry(method.name.clone())
                            .or_default()
                            .push(CallHierarchyDecl {
                                name: method.name.clone(),
                                module: module_name.clone(),
                                file: file.clone(),
                                span: method.span,
                            });

                        collect_block_call_sites(
                            &method.body,
                            &file,
                            module_name.clone(),
                            &method.name,
                            &mut call_sites_by_caller,
                        );
                    }
                }
                ast::Item::Impl(impl_def) => {
                    for method in impl_def.methods {
                        call_graph.entry(method.name.clone()).or_default();
                        declarations_by_name
                            .entry(method.name.clone())
                            .or_default()
                            .push(CallHierarchyDecl {
                                name: method.name.clone(),
                                module: module_name.clone(),
                                file: file.clone(),
                                span: method.span,
                            });

                        collect_block_call_sites(
                            &method.body,
                            &file,
                            module_name.clone(),
                            &method.name,
                            &mut call_sites_by_caller,
                        );
                    }
                }
                ast::Item::Struct(_) | ast::Item::Enum(_) => {}
            }
        }
    }

    for (caller, sites) in &call_sites_by_caller {
        let mut callees = sites
            .iter()
            .map(|site| site.callee_name.clone())
            .collect::<Vec<_>>();
        callees.sort();
        callees.dedup();
        call_graph.insert(caller.clone(), callees);
    }

    for decls in declarations_by_name.values_mut() {
        decls.sort_by(|lhs, rhs| {
            lhs.name
                .cmp(&rhs.name)
                .then(lhs.file.cmp(&rhs.file))
                .then(lhs.span.start.cmp(&rhs.span.start))
        });
    }
    for sites in call_sites_by_caller.values_mut() {
        sites.sort_by(|lhs, rhs| {
            lhs.file
                .cmp(&rhs.file)
                .then(lhs.span.start.cmp(&rhs.span.start))
        });
    }

    Ok(CallHierarchyContext {
        call_graph,
        declarations_by_name,
        call_sites_by_caller,
    })
}

fn collect_block_call_sites(
    block: &ast::Block,
    file: &Path,
    caller_module: Option<String>,
    caller_name: &str,
    call_sites_by_caller: &mut BTreeMap<String, Vec<CallHierarchyCallSite>>,
) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let { expr, .. } => collect_expr_call_sites(
                expr,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            ),
            ast::Stmt::Assign { expr, .. } => collect_expr_call_sites(
                expr,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            ),
            ast::Stmt::Expr { expr, .. } => collect_expr_call_sites(
                expr,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            ),
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_expr_call_sites(
                        expr,
                        file,
                        caller_module.clone(),
                        caller_name,
                        call_sites_by_caller,
                    );
                }
            }
            ast::Stmt::Assert { expr, .. } => collect_expr_call_sites(
                expr,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            ),
        }
    }

    if let Some(tail) = &block.tail {
        collect_expr_call_sites(tail, file, caller_module, caller_name, call_sites_by_caller);
    }
}

fn collect_expr_call_sites(
    expr: &ast::Expr,
    file: &Path,
    caller_module: Option<String>,
    caller_name: &str,
    call_sites_by_caller: &mut BTreeMap<String, Vec<CallHierarchyCallSite>>,
) {
    match &expr.kind {
        ast::ExprKind::Call { callee, args } => {
            if let Some((callee_module, callee_name)) = extract_callee_reference(callee) {
                call_sites_by_caller
                    .entry(caller_name.to_string())
                    .or_default()
                    .push(CallHierarchyCallSite {
                        caller_name: caller_name.to_string(),
                        caller_module: caller_module.clone(),
                        callee_name,
                        callee_module,
                        file: file.to_path_buf(),
                        span: callee.span,
                    });
            }
            collect_expr_call_sites(
                callee,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
            for arg in args {
                collect_expr_call_sites(
                    arg,
                    file,
                    caller_module.clone(),
                    caller_name,
                    call_sites_by_caller,
                );
            }
        }
        ast::ExprKind::Closure { body, .. } => {
            collect_block_call_sites(
                body,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr_call_sites(
                cond,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
            collect_block_call_sites(
                then_block,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
            collect_block_call_sites(
                else_block,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::While { cond, body } => {
            collect_expr_call_sites(
                cond,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
            collect_block_call_sites(
                body,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::Loop { body } => {
            collect_block_call_sites(
                body,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_expr_call_sites(
                    expr,
                    file,
                    caller_module.clone(),
                    caller_name,
                    call_sites_by_caller,
                );
            }
        }
        ast::ExprKind::Continue => {}
        ast::ExprKind::Match { expr, arms } => {
            collect_expr_call_sites(
                expr,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_call_sites(
                        guard,
                        file,
                        caller_module.clone(),
                        caller_name,
                        call_sites_by_caller,
                    );
                }
                collect_expr_call_sites(
                    &arm.body,
                    file,
                    caller_module.clone(),
                    caller_name,
                    call_sites_by_caller,
                );
            }
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_call_sites(
                lhs,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
            collect_expr_call_sites(
                rhs,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::Unary { expr, .. }
        | ast::ExprKind::Borrow { expr, .. }
        | ast::ExprKind::Await { expr }
        | ast::ExprKind::Try { expr } => {
            collect_expr_call_sites(
                expr,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::UnsafeBlock { block } => {
            collect_block_call_sites(
                block,
                file,
                caller_module.clone(),
                caller_name,
                call_sites_by_caller,
            );
        }
        ast::ExprKind::StructInit { fields, .. } => {
            for (_, field_expr, _) in fields {
                collect_expr_call_sites(
                    field_expr,
                    file,
                    caller_module.clone(),
                    caller_name,
                    call_sites_by_caller,
                );
            }
        }
        ast::ExprKind::FieldAccess { base, .. } => {
            collect_expr_call_sites(base, file, caller_module, caller_name, call_sites_by_caller);
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Unit
        | ast::ExprKind::Var(_) => {}
    }
}

fn extract_callee_reference(expr: &ast::Expr) -> Option<(Option<String>, String)> {
    let mut segments = Vec::new();
    if !collect_expr_path_segments(expr, &mut segments) {
        return None;
    }
    let name = segments.pop()?;
    let module = if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    };
    Some((module, name))
}

fn collect_expr_path_segments(expr: &ast::Expr, out: &mut Vec<String>) -> bool {
    match &expr.kind {
        ast::ExprKind::Var(name) => {
            out.push(name.clone());
            true
        }
        ast::ExprKind::FieldAccess { base, field } => {
            if collect_expr_path_segments(base, out) {
                out.push(field.clone());
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn call_hierarchy_item_for_decl(decl: &CallHierarchyDecl) -> Value {
    json!({
        "name": decl.name,
        "kind": 12,
        "detail": decl.module.clone().unwrap_or_default(),
        "uri": path_to_uri(&decl.file),
        "range": span_to_lsp_range(&decl.file, decl.span),
        "selectionRange": span_to_lsp_range(&decl.file, decl.span),
        "data": {
            "name": decl.name,
            "module": decl.module,
            "uri": path_to_uri(&decl.file)
        }
    })
}

fn call_hierarchy_item_name(item: &Value) -> Option<String> {
    item.get("data")
        .and_then(|data| data.get("name"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            item.get("name")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn call_hierarchy_item_module(item: &Value) -> Option<String> {
    item.get("data")
        .and_then(|data| data.get("module"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            item.get("detail")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|detail| !detail.is_empty())
                .map(ToString::to_string)
        })
}

fn build_inverse_call_graph(
    call_graph: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut inverse = BTreeMap::<String, BTreeSet<String>>::new();
    for (caller, callees) in call_graph {
        inverse.entry(caller.clone()).or_default();
        for callee in callees {
            inverse
                .entry(callee.clone())
                .or_default()
                .insert(caller.clone());
        }
    }
    inverse
}

fn modules_match(expected: Option<&str>, actual: Option<&str>) -> bool {
    match expected {
        Some(expected) => actual == Some(expected),
        None => true,
    }
}

fn site_targets_function(
    site: &CallHierarchyCallSite,
    target_name: &str,
    target_module: Option<&str>,
) -> bool {
    if site.callee_name != target_name {
        return false;
    }
    match target_module {
        Some(module) => match site.callee_module.as_deref() {
            Some(callee_module) => callee_module == module,
            None => true,
        },
        None => true,
    }
}

fn site_targets_decl(site: &CallHierarchyCallSite, decl: &CallHierarchyDecl) -> bool {
    if site.callee_name != decl.name {
        return false;
    }
    match (site.callee_module.as_deref(), decl.module.as_deref()) {
        (Some(lhs), Some(rhs)) => lhs == rhs,
        (Some(_), None) => false,
        _ => true,
    }
}

fn call_hierarchy_range_sort_key(range: &Value) -> (u64, u64, u64, u64) {
    let start_line = range
        .get("start")
        .and_then(|start| start.get("line"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let start_char = range
        .get("start")
        .and_then(|start| start.get("character"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let end_line = range
        .get("end")
        .and_then(|end| end.get("line"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let end_char = range
        .get("end")
        .and_then(|end| end.get("character"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (start_line, start_char, end_line, end_char)
}

fn collect_ast_folding_ranges(program: &ast::Program, source: &str) -> Vec<Value> {
    let mut ranges = Vec::new();
    for item in &program.items {
        match item {
            ast::Item::Function(func) => {
                push_folding_range_for_span(&mut ranges, source, func.body.span, Some("region"));
                collect_block_folding_ranges(&func.body, source, &mut ranges);
            }
            ast::Item::Struct(def) => {
                push_folding_range_for_span(&mut ranges, source, def.span, Some("region"));
            }
            ast::Item::Enum(def) => {
                push_folding_range_for_span(&mut ranges, source, def.span, Some("region"));
            }
            ast::Item::Trait(def) => {
                push_folding_range_for_span(&mut ranges, source, def.span, Some("region"));
                for method in &def.methods {
                    push_folding_range_for_span(
                        &mut ranges,
                        source,
                        method.body.span,
                        Some("region"),
                    );
                    collect_block_folding_ranges(&method.body, source, &mut ranges);
                }
            }
            ast::Item::Impl(def) => {
                push_folding_range_for_span(&mut ranges, source, def.span, Some("region"));
                for method in &def.methods {
                    push_folding_range_for_span(
                        &mut ranges,
                        source,
                        method.body.span,
                        Some("region"),
                    );
                    collect_block_folding_ranges(&method.body, source, &mut ranges);
                }
            }
        }
    }
    ranges
}

fn collect_block_folding_ranges(block: &ast::Block, source: &str, ranges: &mut Vec<Value>) {
    push_folding_range_for_span(ranges, source, block.span, Some("region"));

    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let { expr, .. }
            | ast::Stmt::Assign { expr, .. }
            | ast::Stmt::Expr { expr, .. }
            | ast::Stmt::Assert { expr, .. } => collect_expr_folding_ranges(expr, source, ranges),
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_expr_folding_ranges(expr, source, ranges);
                }
            }
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_folding_ranges(tail, source, ranges);
    }
}

fn collect_expr_folding_ranges(expr: &ast::Expr, source: &str, ranges: &mut Vec<Value>) {
    match &expr.kind {
        ast::ExprKind::Call { callee, args } => {
            collect_expr_folding_ranges(callee, source, ranges);
            for arg in args {
                collect_expr_folding_ranges(arg, source, ranges);
            }
        }
        ast::ExprKind::Closure { body, .. } => collect_block_folding_ranges(body, source, ranges),
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr_folding_ranges(cond, source, ranges);
            collect_block_folding_ranges(then_block, source, ranges);
            collect_block_folding_ranges(else_block, source, ranges);
        }
        ast::ExprKind::While { cond, body } => {
            collect_expr_folding_ranges(cond, source, ranges);
            collect_block_folding_ranges(body, source, ranges);
        }
        ast::ExprKind::Loop { body } => collect_block_folding_ranges(body, source, ranges),
        ast::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_expr_folding_ranges(expr, source, ranges);
            }
        }
        ast::ExprKind::Continue => {}
        ast::ExprKind::Match { expr, arms } => {
            collect_expr_folding_ranges(expr, source, ranges);
            for arm in arms {
                push_folding_range_for_span(ranges, source, arm.span, Some("region"));
                if let Some(guard) = &arm.guard {
                    collect_expr_folding_ranges(guard, source, ranges);
                }
                collect_expr_folding_ranges(&arm.body, source, ranges);
            }
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_folding_ranges(lhs, source, ranges);
            collect_expr_folding_ranges(rhs, source, ranges);
        }
        ast::ExprKind::Unary { expr, .. }
        | ast::ExprKind::Borrow { expr, .. }
        | ast::ExprKind::Await { expr }
        | ast::ExprKind::Try { expr } => collect_expr_folding_ranges(expr, source, ranges),
        ast::ExprKind::UnsafeBlock { block } => collect_block_folding_ranges(block, source, ranges),
        ast::ExprKind::StructInit { fields, .. } => {
            for (_, field_expr, _) in fields {
                collect_expr_folding_ranges(field_expr, source, ranges);
            }
        }
        ast::ExprKind::FieldAccess { base, .. } => {
            collect_expr_folding_ranges(base, source, ranges)
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Unit
        | ast::ExprKind::Var(_) => {}
    }
}

fn push_folding_range_for_span(
    ranges: &mut Vec<Value>,
    source: &str,
    span: Span,
    kind: Option<&str>,
) {
    let end_offset = span.end.saturating_sub(1).max(span.start);
    let (start_line, _) = offset_to_line_char(source, span.start);
    let (end_line, _) = offset_to_line_char(source, end_offset);
    if end_line <= start_line {
        return;
    }
    let mut range = json!({
        "startLine": start_line,
        "endLine": end_line
    });
    if let Some(kind) = kind {
        range["kind"] = json!(kind);
    }
    ranges.push(range);
}

fn collect_comment_folding_ranges(source: &str) -> Vec<Value> {
    let mut ranges = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut run_end: usize = 0;

    for (idx, line) in source.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            if run_start.is_none() {
                run_start = Some(idx);
            }
            run_end = idx;
            continue;
        }

        if let Some(start) = run_start.take() {
            if run_end > start {
                ranges.push(json!({
                    "startLine": start,
                    "endLine": run_end,
                    "kind": "comment"
                }));
            }
        }
    }

    if let Some(start) = run_start {
        if run_end > start {
            ranges.push(json!({
                "startLine": start,
                "endLine": run_end,
                "kind": "comment"
            }));
        }
    }

    ranges
}

fn folding_range_sort_key(range: &Value) -> (u64, u64, String) {
    let start_line = range.get("startLine").and_then(Value::as_u64).unwrap_or(0);
    let end_line = range.get("endLine").and_then(Value::as_u64).unwrap_or(0);
    let kind = range
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (start_line, end_line, kind)
}

fn fallback_selection_range(position: &Value) -> Value {
    let line = position.get("line").and_then(Value::as_u64).unwrap_or(0);
    let character = position
        .get("character")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "range": {
            "start": { "line": line, "character": character },
            "end": { "line": line, "character": character }
        }
    })
}

fn selection_chain_for_offset(
    program: &ast::Program,
    source: &str,
    offset: usize,
    line: usize,
    character: usize,
) -> Value {
    let mut expr_spans = Vec::new();
    let mut stmt_spans = Vec::new();
    let mut block_spans = Vec::new();
    let mut function_spans = Vec::new();

    for item in &program.items {
        match item {
            ast::Item::Function(func) => {
                if span_contains_offset(func.span, offset) {
                    function_spans.push(func.span);
                }
                collect_block_selection_spans(
                    &func.body,
                    offset,
                    &mut stmt_spans,
                    &mut block_spans,
                    &mut expr_spans,
                );
            }
            ast::Item::Trait(def) => {
                for method in &def.methods {
                    if span_contains_offset(method.span, offset) {
                        function_spans.push(method.span);
                    }
                    collect_block_selection_spans(
                        &method.body,
                        offset,
                        &mut stmt_spans,
                        &mut block_spans,
                        &mut expr_spans,
                    );
                }
            }
            ast::Item::Impl(def) => {
                for method in &def.methods {
                    if span_contains_offset(method.span, offset) {
                        function_spans.push(method.span);
                    }
                    collect_block_selection_spans(
                        &method.body,
                        offset,
                        &mut stmt_spans,
                        &mut block_spans,
                        &mut expr_spans,
                    );
                }
            }
            ast::Item::Struct(_) | ast::Item::Enum(_) => {}
        }
    }

    let mut chain = Vec::<Span>::new();
    for candidate in [
        smallest_span(&expr_spans),
        smallest_span(&stmt_spans),
        smallest_span(&block_spans),
        smallest_span(&function_spans),
        if span_contains_offset(program.span, offset) {
            Some(program.span)
        } else {
            None
        },
    ]
    .into_iter()
    .flatten()
    {
        if chain.last().copied() != Some(candidate) {
            chain.push(candidate);
        }
    }

    if chain.is_empty() {
        return json!({
            "range": {
                "start": { "line": line, "character": character },
                "end": { "line": line, "character": character }
            }
        });
    }

    let mut parent: Option<Value> = None;
    for span in chain.into_iter().rev() {
        let range = offset_range_to_lsp_range(source, span.start, span.end.max(span.start + 1));
        let node = if let Some(parent) = parent {
            json!({
                "range": range,
                "parent": parent
            })
        } else {
            json!({
                "range": range
            })
        };
        parent = Some(node);
    }
    parent
        .unwrap_or_else(|| fallback_selection_range(&json!({"line": line, "character": character})))
}

fn collect_block_selection_spans(
    block: &ast::Block,
    offset: usize,
    stmt_spans: &mut Vec<Span>,
    block_spans: &mut Vec<Span>,
    expr_spans: &mut Vec<Span>,
) {
    if span_contains_offset(block.span, offset) {
        block_spans.push(block.span);
    }

    for stmt in &block.stmts {
        let span = stmt.span();
        if span_contains_offset(span, offset) {
            stmt_spans.push(span);
        }
        match stmt {
            ast::Stmt::Let { expr, .. }
            | ast::Stmt::Assign { expr, .. }
            | ast::Stmt::Expr { expr, .. }
            | ast::Stmt::Assert { expr, .. } => {
                collect_expr_selection_spans(expr, offset, stmt_spans, block_spans, expr_spans)
            }
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_expr_selection_spans(expr, offset, stmt_spans, block_spans, expr_spans);
                }
            }
        }
    }

    if let Some(tail) = &block.tail {
        collect_expr_selection_spans(tail, offset, stmt_spans, block_spans, expr_spans);
    }
}

fn collect_expr_selection_spans(
    expr: &ast::Expr,
    offset: usize,
    stmt_spans: &mut Vec<Span>,
    block_spans: &mut Vec<Span>,
    expr_spans: &mut Vec<Span>,
) {
    if span_contains_offset(expr.span, offset) {
        expr_spans.push(expr.span);
    }

    match &expr.kind {
        ast::ExprKind::Call { callee, args } => {
            collect_expr_selection_spans(callee, offset, stmt_spans, block_spans, expr_spans);
            for arg in args {
                collect_expr_selection_spans(arg, offset, stmt_spans, block_spans, expr_spans);
            }
        }
        ast::ExprKind::Closure { body, .. } => {
            collect_block_selection_spans(body, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr_selection_spans(cond, offset, stmt_spans, block_spans, expr_spans);
            collect_block_selection_spans(then_block, offset, stmt_spans, block_spans, expr_spans);
            collect_block_selection_spans(else_block, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::While { cond, body } => {
            collect_expr_selection_spans(cond, offset, stmt_spans, block_spans, expr_spans);
            collect_block_selection_spans(body, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::Loop { body } => {
            collect_block_selection_spans(body, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_expr_selection_spans(expr, offset, stmt_spans, block_spans, expr_spans);
            }
        }
        ast::ExprKind::Continue => {}
        ast::ExprKind::Match { expr, arms } => {
            collect_expr_selection_spans(expr, offset, stmt_spans, block_spans, expr_spans);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_selection_spans(
                        guard,
                        offset,
                        stmt_spans,
                        block_spans,
                        expr_spans,
                    );
                }
                collect_expr_selection_spans(
                    &arm.body,
                    offset,
                    stmt_spans,
                    block_spans,
                    expr_spans,
                );
            }
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_selection_spans(lhs, offset, stmt_spans, block_spans, expr_spans);
            collect_expr_selection_spans(rhs, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::Unary { expr, .. }
        | ast::ExprKind::Borrow { expr, .. }
        | ast::ExprKind::Await { expr }
        | ast::ExprKind::Try { expr } => {
            collect_expr_selection_spans(expr, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::UnsafeBlock { block } => {
            collect_block_selection_spans(block, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::StructInit { fields, .. } => {
            for (_, field_expr, _) in fields {
                collect_expr_selection_spans(
                    field_expr,
                    offset,
                    stmt_spans,
                    block_spans,
                    expr_spans,
                );
            }
        }
        ast::ExprKind::FieldAccess { base, .. } => {
            collect_expr_selection_spans(base, offset, stmt_spans, block_spans, expr_spans);
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Unit
        | ast::ExprKind::Var(_) => {}
    }
}

fn span_contains_offset(span: Span, offset: usize) -> bool {
    offset >= span.start && offset <= span.end.max(span.start + 1)
}

fn smallest_span(spans: &[Span]) -> Option<Span> {
    spans
        .iter()
        .copied()
        .filter(|span| span.end >= span.start)
        .min_by_key(|span| (span.end.saturating_sub(span.start), span.start))
}

fn symbol_index_root(entry_path: &Path) -> PathBuf {
    if entry_path.is_dir() {
        return entry_path.to_path_buf();
    }
    find_project_root(entry_path)
}

fn extract_text_symbol_decls(source: &str, file: &Path) -> Vec<SymbolDecl> {
    let mut decls = Vec::new();
    let mut offset = 0usize;
    for line in source.lines() {
        let trimmed = line.trim_start();
        let leading = line.len().saturating_sub(trimmed.len());

        if let Some(rest) = trimmed.strip_prefix("module ") {
            let name = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
                .collect::<String>();
            if !name.is_empty() {
                let start = offset + leading + "module ".len();
                decls.push(SymbolDecl {
                    name: name.clone(),
                    kind: "module".to_string(),
                    signature: format!("module {}", name),
                    file: file.to_path_buf(),
                    span: Span::new(start, start + name.len()),
                });
            }
        }

        if let Some(rest) = trimmed.strip_prefix("const ") {
            let name = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>();
            if !name.is_empty() {
                let start = offset + leading + "const ".len();
                decls.push(SymbolDecl {
                    name: name.clone(),
                    kind: "constant".to_string(),
                    signature: format!("const {}", name),
                    file: file.to_path_buf(),
                    span: Span::new(start, start + name.len()),
                });
            }
        }

        offset += line.len() + 1;
    }
    decls
}

fn document_symbols_for_file(path: &Path, source: &str) -> anyhow::Result<Vec<Value>> {
    let mut top_level = Vec::<(usize, String, Value)>::new();
    let mut struct_indices = BTreeMap::<String, usize>::new();
    let mut pending_impls = Vec::<(usize, Option<String>, Value)>::new();

    for decl in extract_text_symbol_decls(source, path) {
        let range = offset_range_to_lsp_range(source, decl.span.start, decl.span.end);
        top_level.push((
            decl.span.start,
            decl.name.clone(),
            json!({
                "name": decl.name,
                "detail": decl.signature,
                "kind": symbol_kind(&decl.kind),
                "range": range.clone(),
                "selectionRange": range,
                "children": []
            }),
        ));
    }

    let (program, diagnostics) = parser::parse(source, &path.to_string_lossy());
    if diagnostics.iter().any(|d| d.is_error()) {
        top_level.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        return Ok(top_level.into_iter().map(|(_, _, value)| value).collect());
    }
    let Some(program) = program else {
        top_level.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        return Ok(top_level.into_iter().map(|(_, _, value)| value).collect());
    };

    for item in program.items {
        match item {
            ast::Item::Function(func) => {
                let range = offset_range_to_lsp_range(source, func.span.start, func.span.end);
                top_level.push((
                    func.span.start,
                    func.name.clone(),
                    json!({
                        "name": func.name,
                        "detail": render_function_signature(&func),
                        "kind": symbol_kind("function"),
                        "range": range.clone(),
                        "selectionRange": range,
                        "children": []
                    }),
                ));
            }
            ast::Item::Struct(strukt) => {
                let range = offset_range_to_lsp_range(source, strukt.span.start, strukt.span.end);
                let children = strukt
                    .fields
                    .iter()
                    .map(|field| {
                        let field_range =
                            offset_range_to_lsp_range(source, field.span.start, field.span.end);
                        json!({
                            "name": field.name,
                            "detail": render_type_expr(&field.ty),
                            "kind": 8,
                            "range": field_range.clone(),
                            "selectionRange": field_range,
                            "children": []
                        })
                    })
                    .collect::<Vec<_>>();
                struct_indices.insert(strukt.name.clone(), top_level.len());
                top_level.push((
                    strukt.span.start,
                    strukt.name.clone(),
                    json!({
                        "name": strukt.name,
                        "detail": render_struct_signature(&strukt),
                        "kind": symbol_kind("struct"),
                        "range": range.clone(),
                        "selectionRange": range,
                        "children": children
                    }),
                ));
            }
            ast::Item::Enum(enm) => {
                let range = offset_range_to_lsp_range(source, enm.span.start, enm.span.end);
                let children = enm
                    .variants
                    .iter()
                    .map(|variant| {
                        let variant_range = offset_range_to_lsp_range(
                            source,
                            variant.span.start,
                            variant.span.end,
                        );
                        json!({
                            "name": variant.name,
                            "detail": variant.payload.as_ref().map(render_type_expr).unwrap_or_default(),
                            "kind": 22,
                            "range": variant_range.clone(),
                            "selectionRange": variant_range,
                            "children": []
                        })
                    })
                    .collect::<Vec<_>>();
                top_level.push((
                    enm.span.start,
                    enm.name.clone(),
                    json!({
                        "name": enm.name,
                        "detail": render_enum_signature(&enm),
                        "kind": symbol_kind("enum"),
                        "range": range.clone(),
                        "selectionRange": range,
                        "children": children
                    }),
                ));
            }
            ast::Item::Trait(trait_def) => {
                let range =
                    offset_range_to_lsp_range(source, trait_def.span.start, trait_def.span.end);
                let children = trait_def
                    .methods
                    .iter()
                    .map(|method| {
                        let method_range =
                            offset_range_to_lsp_range(source, method.span.start, method.span.end);
                        json!({
                            "name": method.name,
                            "detail": render_function_signature(method),
                            "kind": 6,
                            "range": method_range.clone(),
                            "selectionRange": method_range,
                            "children": []
                        })
                    })
                    .collect::<Vec<_>>();
                top_level.push((
                    trait_def.span.start,
                    trait_def.name.clone(),
                    json!({
                        "name": trait_def.name,
                        "detail": render_trait_signature(&trait_def),
                        "kind": symbol_kind("trait"),
                        "range": range.clone(),
                        "selectionRange": range,
                        "children": children
                    }),
                ));
            }
            ast::Item::Impl(impl_def) => {
                let range =
                    offset_range_to_lsp_range(source, impl_def.span.start, impl_def.span.end);
                let impl_name = if impl_def.is_inherent {
                    let target_name = impl_def
                        .target
                        .as_ref()
                        .map(render_type_expr)
                        .unwrap_or_else(|| impl_def.trait_name.clone());
                    format!("impl {}", target_name)
                } else if let Some(target) = impl_def.target.as_ref().map(render_type_expr) {
                    format!("impl {} for {}", impl_def.trait_name, target)
                } else {
                    format!("impl {}", impl_def.trait_name)
                };
                let children = impl_def
                    .methods
                    .iter()
                    .map(|method| {
                        let method_range =
                            offset_range_to_lsp_range(source, method.span.start, method.span.end);
                        json!({
                            "name": method.name,
                            "detail": render_function_signature(method),
                            "kind": 6,
                            "range": method_range.clone(),
                            "selectionRange": method_range,
                            "children": []
                        })
                    })
                    .collect::<Vec<_>>();
                let target_name = impl_def
                    .target
                    .as_ref()
                    .and_then(type_expr_base_name)
                    .or_else(|| {
                        if impl_def.is_inherent {
                            Some(impl_def.trait_name.clone())
                        } else {
                            None
                        }
                    });
                let symbol = json!({
                    "name": impl_name,
                    "detail": render_impl_signature(&impl_def),
                    "kind": symbol_kind("impl"),
                    "range": range.clone(),
                    "selectionRange": range,
                    "children": children
                });
                pending_impls.push((impl_def.span.start, target_name, symbol));
            }
        }
    }

    for (start, target, symbol) in pending_impls {
        let Some(target_name) = target else {
            top_level.push((start, format!("impl:{start}"), symbol));
            continue;
        };
        let Some(idx) = struct_indices.get(&target_name).copied() else {
            top_level.push((start, format!("impl:{start}"), symbol));
            continue;
        };

        if let Some(children) = top_level[idx]
            .2
            .get_mut("children")
            .and_then(Value::as_array_mut)
        {
            children.push(symbol);
        }
    }

    top_level.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok(top_level.into_iter().map(|(_, _, value)| value).collect())
}

fn type_expr_base_name(ty: &ast::TypeExpr) -> Option<String> {
    match &ty.kind {
        ast::TypeKind::Named { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn symbol_decl_to_workspace_symbol(decl: &SymbolDecl) -> Value {
    json!({
        "name": decl.name,
        "kind": symbol_kind(&decl.kind),
        "location": {
            "uri": path_to_uri(&decl.file),
            "range": span_to_lsp_range(&decl.file, decl.span)
        },
        "containerName": decl.kind
    })
}

fn collect_inlay_hints(
    path: &Path,
    source: &str,
    show_type_hints: bool,
    show_effect_hints: bool,
    show_contract_hints: bool,
) -> anyhow::Result<Vec<Value>> {
    let mut signature_lookup = BTreeMap::<String, String>::new();
    let declarations = build_symbol_index(path)?;
    for (name, decls) in declarations {
        if let Some(first) = decls.first() {
            signature_lookup.insert(name, first.signature.clone());
        }
    }

    let mut hints = Vec::new();
    let (program, diagnostics) = parser::parse(source, &path.to_string_lossy());
    if diagnostics.iter().any(|diag| diag.is_error()) {
        return Ok(hints);
    }
    let Some(program) = program else {
        return Ok(hints);
    };

    for item in &program.items {
        match item {
            ast::Item::Function(func) => collect_block_inlay_hints(
                &func.body,
                source,
                show_type_hints,
                show_effect_hints,
                &signature_lookup,
                &mut hints,
            ),
            ast::Item::Trait(trait_def) => {
                for method in &trait_def.methods {
                    collect_block_inlay_hints(
                        &method.body,
                        source,
                        show_type_hints,
                        show_effect_hints,
                        &signature_lookup,
                        &mut hints,
                    );
                }
            }
            ast::Item::Impl(impl_def) => {
                for method in &impl_def.methods {
                    collect_block_inlay_hints(
                        &method.body,
                        source,
                        show_type_hints,
                        show_effect_hints,
                        &signature_lookup,
                        &mut hints,
                    );
                }
            }
            ast::Item::Struct(_) | ast::Item::Enum(_) => {}
        }
    }

    if show_contract_hints {
        let canonical_target = fs::canonicalize(path).unwrap_or(path.to_path_buf());
        if let Ok(front) = run_frontend(path) {
            for diag in &front.diagnostics {
                let is_contract = diag.message.to_lowercase().contains("contract")
                    || diag.code.to_uppercase().contains("CONTRACT");
                if !is_contract {
                    continue;
                }
                let Some(span) = diag.spans.first() else {
                    continue;
                };
                let span_path = PathBuf::from(&span.file);
                let canonical_span = fs::canonicalize(&span_path).unwrap_or(span_path);
                if canonical_span != canonical_target {
                    continue;
                }
                let (line, character) = offset_to_line_char(source, span.start);
                hints.push(inlay_hint(
                    line,
                    character,
                    format!("contract: {}", diag.message),
                    2,
                    false,
                ));
            }
        }
    }

    Ok(hints)
}

fn collect_block_inlay_hints(
    block: &ast::Block,
    source: &str,
    show_type_hints: bool,
    show_effect_hints: bool,
    signature_lookup: &BTreeMap<String, String>,
    hints: &mut Vec<Value>,
) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let {
                name,
                ty,
                expr,
                span,
                ..
            } => {
                if show_type_hints && ty.is_none() {
                    if let Some(inferred) = infer_expr_type(expr, signature_lookup) {
                        if let Some(offset) = find_name_offset_in_span(source, name, *span) {
                            let (line, character) =
                                offset_to_line_char(source, offset + name.len());
                            hints.push(inlay_hint(
                                line,
                                character,
                                format!(": {inferred}"),
                                1,
                                true,
                            ));
                        }
                    }
                }
                collect_expr_inlay_hints(expr, source, show_effect_hints, signature_lookup, hints);
            }
            ast::Stmt::Assign { expr, .. }
            | ast::Stmt::Expr { expr, .. }
            | ast::Stmt::Assert { expr, .. } => {
                collect_expr_inlay_hints(expr, source, show_effect_hints, signature_lookup, hints);
            }
            ast::Stmt::Return {
                expr: Some(expr), ..
            } => {
                collect_expr_inlay_hints(expr, source, show_effect_hints, signature_lookup, hints);
            }
            ast::Stmt::Return { expr: None, .. } => {}
        }
    }

    if let Some(tail) = &block.tail {
        collect_expr_inlay_hints(tail, source, show_effect_hints, signature_lookup, hints);
    }
}

fn collect_expr_inlay_hints(
    expr: &ast::Expr,
    source: &str,
    show_effect_hints: bool,
    signature_lookup: &BTreeMap<String, String>,
    hints: &mut Vec<Value>,
) {
    match &expr.kind {
        ast::ExprKind::Call { callee, args } => {
            if show_effect_hints {
                if let ast::ExprKind::Var(name) = &callee.kind {
                    if let Some(signature) = signature_lookup.get(name) {
                        if let Some(effects) = parse_signature_effects(signature) {
                            let (line, character) = offset_to_line_char(source, expr.span.end);
                            hints.push(inlay_hint(line, character, effects, 1, true));
                        }
                    }
                }
            }
            collect_expr_inlay_hints(callee, source, show_effect_hints, signature_lookup, hints);
            for arg in args {
                collect_expr_inlay_hints(arg, source, show_effect_hints, signature_lookup, hints);
            }
        }
        ast::ExprKind::Closure { body, .. } => {
            collect_block_inlay_hints(
                body,
                source,
                false,
                show_effect_hints,
                signature_lookup,
                hints,
            );
        }
        ast::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr_inlay_hints(cond, source, show_effect_hints, signature_lookup, hints);
            collect_block_inlay_hints(
                then_block,
                source,
                false,
                show_effect_hints,
                signature_lookup,
                hints,
            );
            collect_block_inlay_hints(
                else_block,
                source,
                false,
                show_effect_hints,
                signature_lookup,
                hints,
            );
        }
        ast::ExprKind::While { cond, body } => {
            collect_expr_inlay_hints(cond, source, show_effect_hints, signature_lookup, hints);
            collect_block_inlay_hints(
                body,
                source,
                false,
                show_effect_hints,
                signature_lookup,
                hints,
            );
        }
        ast::ExprKind::Loop { body } => {
            collect_block_inlay_hints(
                body,
                source,
                false,
                show_effect_hints,
                signature_lookup,
                hints,
            );
        }
        ast::ExprKind::Break { expr: Some(value) }
        | ast::ExprKind::Await { expr: value }
        | ast::ExprKind::Try { expr: value }
        | ast::ExprKind::Unary { expr: value, .. }
        | ast::ExprKind::Borrow { expr: value, .. } => {
            collect_expr_inlay_hints(value, source, show_effect_hints, signature_lookup, hints);
        }
        ast::ExprKind::Match { expr: value, arms } => {
            collect_expr_inlay_hints(value, source, show_effect_hints, signature_lookup, hints);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_inlay_hints(
                        guard,
                        source,
                        show_effect_hints,
                        signature_lookup,
                        hints,
                    );
                }
                collect_expr_inlay_hints(
                    &arm.body,
                    source,
                    show_effect_hints,
                    signature_lookup,
                    hints,
                );
            }
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_inlay_hints(lhs, source, show_effect_hints, signature_lookup, hints);
            collect_expr_inlay_hints(rhs, source, show_effect_hints, signature_lookup, hints);
        }
        ast::ExprKind::UnsafeBlock { block } => {
            collect_block_inlay_hints(
                block,
                source,
                false,
                show_effect_hints,
                signature_lookup,
                hints,
            );
        }
        ast::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_expr_inlay_hints(value, source, show_effect_hints, signature_lookup, hints);
            }
        }
        ast::ExprKind::FieldAccess { base, .. } => {
            collect_expr_inlay_hints(base, source, show_effect_hints, signature_lookup, hints);
        }
        ast::ExprKind::Break { expr: None }
        | ast::ExprKind::Continue
        | ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Unit
        | ast::ExprKind::Var(_) => {}
    }
}

fn infer_expr_type(
    expr: &ast::Expr,
    signature_lookup: &BTreeMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ast::ExprKind::Int(_) => Some("Int".to_string()),
        ast::ExprKind::Float(_) => Some("Float".to_string()),
        ast::ExprKind::Bool(_) => Some("Bool".to_string()),
        ast::ExprKind::String(_) => Some("String".to_string()),
        ast::ExprKind::Unit => Some("Unit".to_string()),
        ast::ExprKind::StructInit { name, .. } => Some(name.clone()),
        ast::ExprKind::Call { callee, .. } => {
            if let ast::ExprKind::Var(name) = &callee.kind {
                signature_lookup
                    .get(name)
                    .and_then(|signature| parse_signature_return_type(signature))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_signature_return_type(signature: &str) -> Option<String> {
    let (_, tail) = signature.split_once("->")?;
    let return_ty = tail
        .split(" effects ")
        .next()
        .map(str::trim)
        .unwrap_or_default();
    if return_ty.is_empty() {
        None
    } else {
        Some(return_ty.to_string())
    }
}

fn parse_signature_effects(signature: &str) -> Option<String> {
    let (_, tail) = signature.split_once(" effects {")?;
    let body = tail.split('}').next().map(str::trim).unwrap_or_default();
    if body.is_empty() {
        None
    } else {
        Some(format!("effects {{ {body} }}"))
    }
}

fn inlay_hint(
    line: usize,
    character: usize,
    label: String,
    kind: i32,
    padding_left: bool,
) -> Value {
    json!({
        "position": {
            "line": line,
            "character": character
        },
        "label": label,
        "kind": kind,
        "paddingLeft": padding_left,
        "paddingRight": false
    })
}

fn inlay_hint_in_range(hint: &Value, range: &Value) -> bool {
    let line = hint
        .get("position")
        .and_then(|position| position.get("line"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let character = hint
        .get("position")
        .and_then(|position| position.get("character"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let point = json!({
        "start": {
            "line": line,
            "character": character
        },
        "end": {
            "line": line,
            "character": character
        }
    });
    ranges_intersect(range, &point)
}

fn render_function_signature(func: &ast::Function) -> String {
    let generics = render_generic_params(&func.generics, "<", ">");

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
    let generics = render_generic_params(&strukt.generics, "<", ">");
    let fields = strukt
        .fields
        .iter()
        .map(|f| format!("{}: {}", f.name, render_type_expr(&f.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("struct {}{} {{ {} }}", strukt.name, generics, fields)
}

fn render_enum_signature(enm: &ast::EnumDef) -> String {
    let generics = render_generic_params(&enm.generics, "<", ">");
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
    format!("enum {}{} {{ {} }}", enm.name, generics, variants)
}

fn render_trait_signature(trait_def: &ast::TraitDef) -> String {
    let generics = render_generic_params(&trait_def.generics, "<", ">");
    if trait_def.methods.is_empty() {
        return format!("trait {}{};", trait_def.name, generics);
    }
    let methods = trait_def
        .methods
        .iter()
        .map(|method| format!("{};", render_function_signature(method)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("trait {}{} {{ {} }}", trait_def.name, generics, methods)
}

fn render_impl_signature(impl_def: &ast::ImplDef) -> String {
    let methods = impl_def
        .methods
        .iter()
        .map(|method| format!("{};", render_function_signature(method)))
        .collect::<Vec<_>>()
        .join(" ");
    if impl_def.is_inherent {
        let target = impl_def
            .target
            .as_ref()
            .map(render_type_expr)
            .unwrap_or_else(|| impl_def.trait_name.clone());
        if methods.is_empty() {
            return format!("impl {} {{}}", target);
        }
        return format!("impl {} {{ {} }}", target, methods);
    }

    let args = impl_def
        .trait_args
        .iter()
        .map(render_type_expr)
        .collect::<Vec<_>>()
        .join(", ");
    if methods.is_empty() {
        return format!("impl {}<{}>;", impl_def.trait_name, args);
    }
    format!("impl {}<{}> {{ {} }}", impl_def.trait_name, args, methods)
}

fn render_generic_params(generics: &[ast::GenericParam], open: &str, close: &str) -> String {
    if generics.is_empty() {
        return String::new();
    }
    format!(
        "{}{}{}",
        open,
        generics
            .iter()
            .map(|g| {
                if g.bounds.is_empty() {
                    g.name.clone()
                } else {
                    format!("{}: {}", g.name, g.bounds.join(" + "))
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
        close
    )
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
        ast::TypeKind::Hole => "_".to_string(),
    }
}

fn symbol_kind(kind: &str) -> i32 {
    match kind {
        "module" => 2,
        "function" => 12,
        "struct" => 23,
        "enum" => 10,
        "trait" => 11,
        "impl" => 5,
        "constant" => 14,
        _ => 13,
    }
}

fn completion_kind(kind: &str) -> i32 {
    match kind {
        "function" => 3,
        "struct" => 22,
        "enum" => 13,
        "trait" => 8,
        "impl" => 9,
        _ => 1,
    }
}

fn find_word_occurrences(source: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor <= source.len() {
        let hay = &source[cursor..];
        let Some(rel) = hay.find(needle) else {
            break;
        };
        let start = cursor + rel;
        let end = start + needle.len();
        if is_word_boundary(source.as_bytes(), start, end) {
            out.push((start, end));
        }
        cursor = end;
    }
    out
}

fn is_word_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let left_ok = if start == 0 {
        true
    } else {
        !is_word_byte(bytes[start - 1])
    };
    let right_ok = if end >= bytes.len() {
        true
    } else {
        !is_word_byte(bytes[end])
    };
    left_ok && right_ok
}

fn offset_range_to_lsp_range(source: &str, start: usize, end: usize) -> Value {
    let (start_line, start_char) = offset_to_line_char(source, start);
    let (end_line, end_char) = offset_to_line_char(source, end.max(start));
    json!({
        "start": { "line": start_line, "character": start_char },
        "end": { "line": end_line, "character": end_char }
    })
}

fn find_name_offset_in_span(source: &str, name: &str, span: Span) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    let start = span.start.min(source.len());
    let end = span.end.min(source.len());
    if start >= end {
        return source.find(name);
    }
    source[start..end].find(name).map(|rel| start + rel)
}

fn ranges_intersect(lhs: &Value, rhs: &Value) -> bool {
    let a = range_tuple(lhs);
    let b = range_tuple(rhs);
    match (a, b) {
        (
            Some((as_line, as_char, ae_line, ae_char)),
            Some((bs_line, bs_char, be_line, be_char)),
        ) => {
            let a_start = (as_line, as_char);
            let a_end = (ae_line, ae_char);
            let b_start = (bs_line, bs_char);
            let b_end = (be_line, be_char);
            a_start < b_end && b_start < a_end
        }
        _ => true,
    }
}

fn range_tuple(range: &Value) -> Option<(u64, u64, u64, u64)> {
    Some((
        range.get("start")?.get("line")?.as_u64()?,
        range.get("start")?.get("character")?.as_u64()?,
        range.get("end")?.get("line")?.as_u64()?,
        range.get("end")?.get("character")?.as_u64()?,
    ))
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
    use super::{
        full_document_range, line_char_to_offset, offset_to_line_char, word_at_position, LspServer,
        MOD_DECLARATION, MOD_DEPRECATED, MOD_EFFECTFUL, MOD_MUTABLE, MOD_READONLY,
        TOKEN_ENUM_MEMBER, TOKEN_FUNCTION, TOKEN_PROPERTY, TOKEN_TYPE_PARAMETER, TOKEN_VARIABLE,
    };
    use serde_json::{json, Value};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("aic-lsp-{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    #[derive(Debug, Clone, Copy)]
    struct DecodedToken {
        line: usize,
        character: usize,
        token_type: usize,
        modifiers: u32,
    }

    fn decode_semantic_tokens(data: &serde_json::Value) -> Vec<DecodedToken> {
        let raw = data.as_array().expect("semantic token data array");
        assert_eq!(raw.len() % 5, 0, "semantic token payload must be 5-tuples");

        let mut tokens = Vec::new();
        let mut line = 0usize;
        let mut character = 0usize;
        for chunk in raw.chunks(5) {
            let delta_line = chunk[0].as_u64().expect("delta line") as usize;
            let delta_start = chunk[1].as_u64().expect("delta start") as usize;
            if delta_line == 0 {
                character += delta_start;
            } else {
                line += delta_line;
                character = delta_start;
            }
            tokens.push(DecodedToken {
                line,
                character,
                token_type: chunk[3].as_u64().expect("token type") as usize,
                modifiers: chunk[4].as_u64().expect("token modifiers") as u32,
            });
        }
        tokens
    }

    fn find_decoded_token(
        tokens: &[DecodedToken],
        line: usize,
        character: usize,
        token_type: usize,
    ) -> Option<DecodedToken> {
        tokens.iter().copied().find(|token| {
            token.line == line && token.character == character && token.token_type == token_type
        })
    }

    fn selection_chain_depth(selection: &Value) -> usize {
        let mut depth = 0usize;
        let mut cursor = selection;
        loop {
            depth += 1;
            let Some(parent) = cursor.get("parent") else {
                break;
            };
            cursor = parent;
        }
        depth
    }

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

    #[test]
    fn initialize_advertises_completion_triggers() {
        let mut server = LspServer::default();
        let responses = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "rootUri": null
                }
            }))
            .expect("initialize response");
        let completion_provider = &responses[0]["result"]["capabilities"]["completionProvider"];
        assert_eq!(completion_provider["resolveProvider"], false);
        assert_eq!(completion_provider["triggerCharacters"], json!([".", ":"]));
        assert_eq!(
            responses[0]["result"]["capabilities"]["documentSymbolProvider"],
            true
        );
        assert_eq!(
            responses[0]["result"]["capabilities"]["workspaceSymbolProvider"],
            true
        );
        assert_eq!(
            responses[0]["result"]["capabilities"]["inlayHintProvider"],
            true
        );
        assert_eq!(
            responses[0]["result"]["capabilities"]["callHierarchyProvider"],
            true
        );
        assert_eq!(
            responses[0]["result"]["capabilities"]["foldingRangeProvider"],
            true
        );
        assert_eq!(
            responses[0]["result"]["capabilities"]["selectionRangeProvider"],
            true
        );
        assert_eq!(
            responses[0]["result"]["capabilities"]["semanticTokensProvider"]["legend"]
                ["tokenModifiers"],
            json!([
                "declaration",
                "definition",
                "mutable",
                "readonly",
                "deprecated",
                "async",
                "effectful"
            ])
        );
    }

    #[test]
    fn document_and_workspace_symbol_requests_return_expected_results() {
        let workspace = temp_workspace("symbols");
        let src_dir = workspace.join("src");
        fs::create_dir_all(&src_dir).expect("create src directory");

        let main_path = src_dir.join("main.aic");
        let worker_path = src_dir.join("worker.aic");
        let main_source = r#"module sample.main;
import std.io;

const MAGIC: Int = 40;

struct Worker {
    id: Int,
}

impl Worker {
    fn score(self) -> Int {
        self.id + 2
    }
}

fn main() -> Int effects { io } {
    print_int(Worker { id: MAGIC }.score());
    0
}
"#;
        let worker_source = r#"module sample.worker;

fn worker_task() -> Int {
    0
}
"#;
        fs::write(&main_path, main_source).expect("write main source");
        fs::write(&worker_path, worker_source).expect("write worker source");

        let workspace_uri = format!("file://{}", workspace.display());
        let main_uri = format!("file://{}", main_path.display());

        let mut server = LspServer::default();
        let init_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "rootUri": workspace_uri
                }
            }))
            .expect("initialize response");
        assert_eq!(init_response.len(), 1);

        let doc_symbols_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "textDocument/documentSymbol",
                "params": {
                    "textDocument": {
                        "uri": main_uri
                    }
                }
            }))
            .expect("document symbol response");
        let doc_symbols = doc_symbols_response[0]["result"]
            .as_array()
            .expect("document symbols array");
        assert!(
            doc_symbols
                .iter()
                .any(|symbol| symbol["name"] == "sample.main" && symbol["kind"] == 2),
            "module symbol should be present in document symbols"
        );
        let worker_symbol = doc_symbols
            .iter()
            .find(|symbol| symbol["name"] == "Worker")
            .expect("Worker symbol should be present");
        let worker_children = worker_symbol["children"]
            .as_array()
            .expect("Worker symbol children array");
        assert!(
            worker_children.iter().any(|child| child["name"]
                .as_str()
                .unwrap_or_default()
                .starts_with("impl ")),
            "Worker symbol should include nested impl block"
        );

        let workspace_symbols_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "workspace/symbol",
                "params": {
                    "query": "worker"
                }
            }))
            .expect("workspace symbol response");
        let workspace_symbols = workspace_symbols_response[0]["result"]
            .as_array()
            .expect("workspace symbols array");
        assert!(
            workspace_symbols
                .iter()
                .any(|symbol| symbol["name"] == "worker_task"),
            "workspace symbol search should include worker_task"
        );

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn call_hierarchy_incoming_and_outgoing_calls_work_across_modules() {
        let workspace = temp_workspace("call_hierarchy");
        let src_dir = workspace.join("src");
        fs::create_dir_all(&src_dir).expect("create src directory");

        let main_path = src_dir.join("main.aic");
        let runner_path = src_dir.join("runner.aic");
        let math_path = src_dir.join("math.aic");

        let main_source = r#"module sample.main;
import std.io;

fn main() -> Int effects { io } {
    let value = sample.runner.invoke();
    print_int(value);
    0
}
"#;
        let runner_source = r#"module sample.runner;

fn invoke() -> Int {
    sample.math.normalize(41)
}
"#;
        let math_source = r#"module sample.math;

fn normalize(x: Int) -> Int {
    x + 1
}
"#;
        fs::write(&main_path, main_source).expect("write main source");
        fs::write(&runner_path, runner_source).expect("write runner source");
        fs::write(&math_path, math_source).expect("write math source");

        let workspace_uri = format!("file://{}", workspace.display());
        let runner_uri = format!("file://{}", runner_path.display());
        let math_uri = format!("file://{}", math_path.display());

        let mut server = LspServer::default();
        let init_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "rootUri": workspace_uri
                }
            }))
            .expect("initialize response");
        assert_eq!(init_response.len(), 1);

        let normalize_offset = math_source
            .find("normalize")
            .expect("normalize declaration");
        let (normalize_line, normalize_char) = offset_to_line_char(math_source, normalize_offset);
        let prepare_normalize_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "textDocument/prepareCallHierarchy",
                "params": {
                    "textDocument": { "uri": math_uri },
                    "position": {
                        "line": normalize_line,
                        "character": normalize_char
                    }
                }
            }))
            .expect("prepare normalize response");
        let normalize_items = prepare_normalize_response[0]["result"]
            .as_array()
            .expect("call hierarchy prepare result array");
        assert!(
            normalize_items
                .iter()
                .any(|item| item["name"].as_str() == Some("normalize")),
            "prepare call hierarchy should return normalize item"
        );
        let normalize_item = normalize_items
            .iter()
            .find(|item| item["name"].as_str() == Some("normalize"))
            .expect("normalize call hierarchy item");

        let incoming_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "callHierarchy/incomingCalls",
                "params": {
                    "item": normalize_item.clone()
                }
            }))
            .expect("incoming call hierarchy response");
        let incoming = incoming_response[0]["result"]
            .as_array()
            .expect("incoming call hierarchy array");
        let runner_incoming = incoming
            .iter()
            .find(|entry| {
                entry["from"]["name"].as_str() == Some("invoke")
                    && entry["from"]["uri"].as_str() == Some(runner_uri.as_str())
            })
            .expect("invoke should appear as incoming caller for normalize");
        assert!(
            runner_incoming["fromRanges"]
                .as_array()
                .is_some_and(|ranges| !ranges.is_empty()),
            "incoming entry for invoke should include call ranges"
        );

        let invoke_offset = runner_source.find("invoke").expect("invoke declaration");
        let (invoke_line, invoke_char) = offset_to_line_char(runner_source, invoke_offset);
        let prepare_invoke_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "textDocument/prepareCallHierarchy",
                "params": {
                    "textDocument": { "uri": runner_uri },
                    "position": {
                        "line": invoke_line,
                        "character": invoke_char
                    }
                }
            }))
            .expect("prepare invoke response");
        let invoke_item = prepare_invoke_response[0]["result"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["name"].as_str() == Some("invoke"))
            })
            .expect("invoke call hierarchy item")
            .clone();

        let outgoing_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "callHierarchy/outgoingCalls",
                "params": {
                    "item": invoke_item
                }
            }))
            .expect("outgoing call hierarchy response");
        let outgoing = outgoing_response[0]["result"]
            .as_array()
            .expect("outgoing call hierarchy array");
        let normalize_outgoing = outgoing
            .iter()
            .find(|entry| {
                entry["to"]["name"].as_str() == Some("normalize")
                    && entry["to"]["uri"].as_str() == Some(math_uri.as_str())
            })
            .expect("normalize should appear as outgoing callee for invoke");
        assert!(
            normalize_outgoing["fromRanges"]
                .as_array()
                .is_some_and(|ranges| !ranges.is_empty()),
            "outgoing entry for normalize should include call ranges"
        );

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn folding_ranges_include_functions_match_arms_and_comment_blocks() {
        let source = r#"module sample.folding;
// first comment line
// second comment line

struct User {
    id: Int,
    score: Int,
}

fn classify(x: Int) -> Int {
    match x {
        0 => if x == 0 {
            0
        } else {
            1
        },
        _ => if x > 10 {
            x
        } else {
            x + 1
        },
    }
}
"#;
        let uri = "file:///folding_demo.aic".to_string();
        let mut server = LspServer::default();
        server.documents.insert(uri.clone(), source.to_string());

        let responses = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 120,
                "method": "textDocument/foldingRange",
                "params": {
                    "textDocument": { "uri": uri }
                }
            }))
            .expect("folding range response");
        let ranges = responses[0]["result"]
            .as_array()
            .expect("folding range array");
        assert!(!ranges.is_empty(), "folding ranges should not be empty");

        let comment_offset = source.find("// first comment line").expect("comment");
        let (comment_line, _) = offset_to_line_char(source, comment_offset);
        assert!(
            ranges.iter().any(|range| {
                range.get("kind").and_then(Value::as_str) == Some("comment")
                    && range.get("startLine").and_then(Value::as_u64) == Some(comment_line as u64)
            }),
            "folding ranges should include multi-line comment block"
        );

        let fn_offset = source.find("fn classify").expect("classify declaration");
        let (fn_line, _) = offset_to_line_char(source, fn_offset);
        assert!(
            ranges.iter().any(|range| {
                let start = range.get("startLine").and_then(Value::as_u64).unwrap_or(0) as usize;
                let end = range.get("endLine").and_then(Value::as_u64).unwrap_or(0) as usize;
                start <= fn_line && end > fn_line
            }),
            "folding ranges should include classify function body"
        );

        let arm_offset = source.find("0 => if x == 0 {").expect("match arm");
        let (arm_line, _) = offset_to_line_char(source, arm_offset);
        assert!(
            ranges.iter().any(|range| {
                let start = range.get("startLine").and_then(Value::as_u64).unwrap_or(0) as usize;
                let end = range.get("endLine").and_then(Value::as_u64).unwrap_or(0) as usize;
                start == arm_line && end > arm_line
            }),
            "folding ranges should include multi-line match arm block"
        );
    }

    #[test]
    fn selection_ranges_expand_from_expression_to_module_scope() {
        let source = r#"module sample.selection;

fn compute(x: Int) -> Int {
    let value = x + 1;
    value
}
"#;
        let uri = "file:///selection_demo.aic".to_string();
        let mut server = LspServer::default();
        server.documents.insert(uri.clone(), source.to_string());

        let expr_offset = source.find("x + 1").expect("selection expression");
        let (line, character) = offset_to_line_char(source, expr_offset);
        let responses = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 121,
                "method": "textDocument/selectionRange",
                "params": {
                    "textDocument": { "uri": uri },
                    "positions": [
                        { "line": line, "character": character }
                    ]
                }
            }))
            .expect("selection range response");
        let selections = responses[0]["result"]
            .as_array()
            .expect("selection range result array");
        let selection = selections.first().expect("first selection range entry");
        assert!(
            selection_chain_depth(selection) >= 5,
            "selection chain should expand expression -> statement -> block -> function -> module"
        );

        let mut cursor = selection;
        loop {
            let child_start_line = cursor["range"]["start"]["line"].as_u64().unwrap_or(0);
            let child_start_char = cursor["range"]["start"]["character"].as_u64().unwrap_or(0);
            let child_end_line = cursor["range"]["end"]["line"].as_u64().unwrap_or(0);
            let child_end_char = cursor["range"]["end"]["character"].as_u64().unwrap_or(0);
            let Some(parent) = cursor.get("parent") else {
                break;
            };
            let parent_start_line = parent["range"]["start"]["line"].as_u64().unwrap_or(0);
            let parent_start_char = parent["range"]["start"]["character"].as_u64().unwrap_or(0);
            let parent_end_line = parent["range"]["end"]["line"].as_u64().unwrap_or(0);
            let parent_end_char = parent["range"]["end"]["character"].as_u64().unwrap_or(0);
            assert!(
                parent_start_line < child_start_line
                    || (parent_start_line == child_start_line
                        && parent_start_char <= child_start_char),
                "parent selection should start before or at child range"
            );
            assert!(
                parent_end_line > child_end_line
                    || (parent_end_line == child_end_line && parent_end_char >= child_end_char),
                "parent selection should end after or at child range"
            );
            cursor = parent;
        }
    }

    #[test]
    fn inlay_hints_report_type_and_effects_and_respect_contract_toggle() {
        let workspace = temp_workspace("inlay");
        let src_dir = workspace.join("src");
        fs::create_dir_all(&src_dir).expect("create src directory");

        let main_path = src_dir.join("main.aic");
        let main_source = r#"module sample.inlay;
import std.io;

fn checked(x: Int) -> Int effects { io } requires x >= 0 ensures result >= 0 {
    x
}

fn main() -> Int effects { io } {
    let value = 41;
    let parsed = checked(value);
    print_int(parsed + 1);
    0
}
"#;
        fs::write(&main_path, main_source).expect("write main source");

        let workspace_uri = format!("file://{}", workspace.display());
        let main_uri = format!("file://{}", main_path.display());

        let mut server = LspServer::default();
        let init_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "rootUri": workspace_uri
                }
            }))
            .expect("initialize response");
        assert_eq!(init_response.len(), 1);

        let hints_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "textDocument/inlayHint",
                "params": {
                    "textDocument": { "uri": main_uri },
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 50, "character": 0 }
                    }
                }
            }))
            .expect("inlay hint response");
        let hints = hints_response[0]["result"]
            .as_array()
            .expect("inlay hint array");
        assert!(
            hints
                .iter()
                .any(|hint| hint["label"].as_str() == Some(": Int")),
            "type hint should include : Int for inferred let binding"
        );
        assert!(
            hints
                .iter()
                .any(|hint| hint["label"].as_str() == Some("effects { io }")),
            "effect hint should include effects {{ io }} at call site"
        );
        assert!(
            !hints.iter().any(|hint| {
                hint["label"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("contract:")
            }),
            "contract hints should be disabled by default"
        );

        let _ = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "method": "workspace/didChangeConfiguration",
                "params": {
                    "settings": {
                        "aic": {
                            "inlayHints": {
                                "contractAnnotations": true
                            }
                        }
                    }
                }
            }))
            .expect("configuration update");

        let contract_hints_response = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "textDocument/inlayHint",
                "params": {
                    "textDocument": { "uri": main_uri },
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 50, "character": 0 }
                    }
                }
            }))
            .expect("inlay hint response with contracts");
        let contract_hints = contract_hints_response[0]["result"]
            .as_array()
            .expect("inlay hint array");
        assert!(
            contract_hints.iter().any(|hint| {
                hint["label"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("contract:")
            }),
            "contract hints should appear when contractAnnotations is enabled"
        );

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn semantic_tokens_emit_extended_types_and_modifiers() {
        let source = r#"module sample.semantic;
import std.io;
import std.time;

struct Counter {
    value: Int,
}

enum Status {
    Value(Int),
    Empty,
}

fn compute[T](x: T) -> T effects { io } {
    x
}

fn main() -> Int effects { io, time } {
    let mut x = 1;
    let y = x;
    let timestamp = now();
    let next = compute(y);
    print_int(next + timestamp - timestamp);
    0
}
"#;
        let (_, parse_diags) = crate::parser::parse(source, "semantic_tokens_fixture");
        assert!(
            !parse_diags.iter().any(|diag| diag.is_error()),
            "semantic token fixture must parse cleanly: {:?}",
            parse_diags
                .iter()
                .map(|diag| format!("{} {}", diag.code, diag.message))
                .collect::<Vec<_>>()
        );
        let uri = "file:///semantic_tokens_demo.aic".to_string();
        let mut server = LspServer::default();
        server.documents.insert(uri.clone(), source.to_string());

        let responses = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 99,
                "method": "textDocument/semanticTokens/full",
                "params": {
                    "textDocument": {
                        "uri": uri
                    }
                }
            }))
            .expect("semantic token response");
        let data = &responses[0]["result"]["data"];
        let tokens = decode_semantic_tokens(data);
        assert!(
            !tokens.is_empty(),
            "semantic token response should not be empty"
        );

        let mut_x_offset = source
            .find("let mut x")
            .expect("let mut x")
            .saturating_add("let mut ".len());
        let (mut_x_line, mut_x_char) = offset_to_line_char(source, mut_x_offset);
        let mut_x_token = find_decoded_token(&tokens, mut_x_line, mut_x_char, TOKEN_VARIABLE)
            .expect("mutable variable token");
        assert_ne!(
            mut_x_token.modifiers & MOD_MUTABLE,
            0,
            "mutable variable token should carry mutable modifier"
        );
        assert_ne!(
            mut_x_token.modifiers & MOD_DECLARATION,
            0,
            "mutable variable token should carry declaration modifier"
        );

        let y_offset = source
            .find("let y =")
            .expect("let y")
            .saturating_add("let ".len());
        let (y_line, y_char) = offset_to_line_char(source, y_offset);
        let y_token = find_decoded_token(&tokens, y_line, y_char, TOKEN_VARIABLE)
            .expect("immutable variable token");
        assert_ne!(
            y_token.modifiers & MOD_READONLY,
            0,
            "immutable let binding should carry readonly modifier"
        );

        let compute_call_offset = source.find("compute(y)").expect("compute call");
        let (compute_line, compute_char) = offset_to_line_char(source, compute_call_offset);
        let compute_call_token =
            find_decoded_token(&tokens, compute_line, compute_char, TOKEN_FUNCTION)
                .expect("effectful function call token");
        assert_ne!(
            compute_call_token.modifiers & MOD_EFFECTFUL,
            0,
            "effectful call should carry effectful modifier"
        );

        let deprecated_call_offset = source.find("now()").expect("deprecated call");
        let (deprecated_line, deprecated_char) =
            offset_to_line_char(source, deprecated_call_offset);
        let deprecated_call_token =
            find_decoded_token(&tokens, deprecated_line, deprecated_char, TOKEN_FUNCTION)
                .expect("deprecated function call token");
        assert_ne!(
            deprecated_call_token.modifiers & MOD_DEPRECATED,
            0,
            "deprecated call should carry deprecated modifier"
        );

        let type_param_offset = source
            .find("compute[T]")
            .expect("type parameter")
            .saturating_add("compute[".len());
        let (type_line, type_char) = offset_to_line_char(source, type_param_offset);
        let type_param_token =
            find_decoded_token(&tokens, type_line, type_char, TOKEN_TYPE_PARAMETER)
                .expect("type parameter token");
        assert_ne!(
            type_param_token.modifiers & MOD_DECLARATION,
            0,
            "type parameter token should carry declaration modifier"
        );

        let enum_member_offset = source.find("Value(Int),").expect("enum member declaration");
        let (enum_member_line, enum_member_char) = offset_to_line_char(source, enum_member_offset);
        let enum_member_token = find_decoded_token(
            &tokens,
            enum_member_line,
            enum_member_char,
            TOKEN_ENUM_MEMBER,
        )
        .expect("enum member declaration token");
        assert_ne!(
            enum_member_token.modifiers & MOD_DECLARATION,
            0,
            "enum member declaration should carry declaration modifier"
        );

        let property_offset = source.find("value: Int").expect("property declaration");
        let (property_line, property_char) = offset_to_line_char(source, property_offset);
        let property_token =
            find_decoded_token(&tokens, property_line, property_char, TOKEN_PROPERTY)
                .expect("property declaration token");
        assert_ne!(
            property_token.modifiers & MOD_DECLARATION,
            0,
            "struct field declaration should carry declaration modifier"
        );
    }
}
