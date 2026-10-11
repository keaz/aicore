//! Canonical AICore source printer.
//!
//! [`format_program`] prints an IR program with no access to the original
//! text (used for generated code). [`format_source`] is what `aic fmt` uses:
//! it formats source text, re-emits `//` comments, keeps literal spellings,
//! and verifies the result before handing it back.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write};

use serde_json::Value;

use crate::ast::{
    self, decode_internal_const, decode_internal_type_alias, Attribute, AttributeArg,
    AttributeValue, AttributeValueKind, BinOp, BoolLiteral, UnaryOp, Visibility,
};
use crate::diagnostics::Diagnostic;
use crate::ir;
use crate::ir_builder;
use crate::lexer::{self, Comment, Token, TokenKind};
use crate::parser;
use crate::span::Span;

const TUPLE_INTERNAL_NAME: &str = "Tuple";
/// Prefix of the temporary the parser introduces for `let (a, b) = expr;`.
/// `__` cannot start a user identifier, so the name is unambiguous.
const TUPLE_LET_TMP_PREFIX: &str = "__tuple_let_";
/// Type repr the IR builder uses for a typed hole (`_`).
const HOLE_TYPE_REPR: &str = "<?>";

/// Prints `program` in canonical form without source trivia.
///
/// Comments are not part of the IR, so use [`format_source`] when formatting
/// a file a person wrote.
pub fn format_program(program: &ir::Program) -> String {
    Printer::new(program, None).program(program)
}

/// Why [`format_source`] could not produce formatted text.
#[derive(Debug)]
pub enum FormatError {
    /// The input has parse errors.
    Diagnostics(Vec<Diagnostic>),
    /// The formatted text did not pass self-verification (it did not parse,
    /// lost a comment, or changed the program). Nothing should be written.
    Verification(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Diagnostics(diagnostics) => {
                write!(
                    f,
                    "cannot format file with {} diagnostic(s)",
                    diagnostics.len()
                )
            }
            FormatError::Verification(reason) => {
                write!(f, "formatter self-check failed: {reason}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Formats AICore source text, keeping every `//` comment.
///
/// Comments are re-emitted on their own lines before the item, statement,
/// field, variant, method, or match arm that follows them, or at the end of
/// the line they trailed. Single blank lines between those elements are kept.
/// Literals keep their source spelling (`0xFF`, `\u{301}`).
///
/// Before returning, the output is re-parsed and compared with the input:
/// the comment texts must be identical and the parsed program must be the
/// same up to spans and the documented canonicalizations. A mismatch is a
/// formatter bug and is reported as [`FormatError::Verification`].
pub fn format_source(source: &str, file: &str) -> Result<String, FormatError> {
    let (program, diagnostics) = parser::parse(source, file);
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Err(FormatError::Diagnostics(diagnostics));
    }
    let Some(program) = program else {
        return Err(FormatError::Diagnostics(diagnostics));
    };
    let ir = ir_builder::build(&program);
    let (tokens, comments, _) = lexer::lex_with_comments(source, file);
    let original_comments = comments.clone();
    let context = SourceContext::new(source, &tokens, comments, &program);
    let formatted = Printer::new(&ir, Some(context)).program(&ir);
    if formatted != source {
        verify_formatted(&program, source, &original_comments, &formatted, file)?;
    }
    Ok(formatted)
}

fn verify_formatted(
    original: &ast::Program,
    original_source: &str,
    original_comments: &[Comment],
    formatted: &str,
    file: &str,
) -> Result<(), FormatError> {
    let (program, diagnostics) = parser::parse(formatted, file);
    if let Some(diagnostic) = diagnostics.iter().find(|d| d.is_error()) {
        return Err(FormatError::Verification(format!(
            "formatted output does not parse ({}: {})",
            diagnostic.code, diagnostic.message
        )));
    }
    let Some(program) = program else {
        return Err(FormatError::Verification(
            "formatted output does not parse".to_string(),
        ));
    };

    let (_, comments, _) = lexer::lex_with_comments(formatted, file);
    let mut expected = original_comments
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>();
    let mut actual = comments.iter().map(|c| c.text.as_str()).collect::<Vec<_>>();
    expected.sort_unstable();
    actual.sort_unstable();
    if expected != actual {
        return Err(FormatError::Verification(format!(
            "formatted output has {} comment(s) but the input has {}, or their text differs",
            actual.len(),
            expected.len()
        )));
    }

    let original_tokens = lexer::lex(original_source, file).0;
    let formatted_tokens = lexer::lex(formatted, file).0;
    let original_literals = literal_spellings(original_source, &original_tokens);
    let formatted_literals = literal_spellings(formatted, &formatted_tokens);
    if original_literals != formatted_literals {
        let position = original_literals
            .iter()
            .zip(&formatted_literals)
            .position(|(a, b)| a != b)
            .unwrap_or(original_literals.len().min(formatted_literals.len()));
        return Err(FormatError::Verification(format!(
            "formatted output changes literal #{} ({:?} -> {:?})",
            position + 1,
            original_literals.get(position).copied().unwrap_or("<none>"),
            formatted_literals
                .get(position)
                .copied()
                .unwrap_or("<none>")
        )));
    }

    let before = normalized_ast(original)?;
    let after = normalized_ast(&program)?;
    if let Some(path) = first_difference(&before, &after, "program") {
        return Err(FormatError::Verification(format!(
            "formatted output changes the program at `{path}`"
        )));
    }
    Ok(())
}

/// AST as JSON with spans removed and the formatter's canonicalizations
/// applied (sorted imports, sorted/deduplicated effects and capabilities,
/// canonical primitive type names).
fn normalized_ast(program: &ast::Program) -> Result<Value, FormatError> {
    let mut value = serde_json::to_value(program)
        .map_err(|err| FormatError::Verification(format!("cannot compare programs: {err}")))?;
    normalize_ast_value(&mut value);
    if let Some(Value::Array(imports)) = value.get_mut("imports") {
        imports.sort_by_key(|import| import.to_string());
    }
    Ok(value)
}

/// Source spelling of every numeric, string and char literal token, in
/// order. JSON comparison alone cannot see spelling (`0xFF` vs `255`) or
/// non-finite floats, which serialize as `null`.
fn literal_spellings<'a>(text: &'a str, tokens: &[Token]) -> Vec<&'a str> {
    tokens
        .iter()
        .filter(|token| {
            matches!(
                token.kind,
                TokenKind::Int(_) | TokenKind::Float(_) | TokenKind::String(_) | TokenKind::Char(_)
            )
        })
        .filter_map(|token| text.get(token.span.start..token.span.end))
        .collect()
}

fn normalize_ast_value(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(normalize_ast_value),
        Value::Object(map) => {
            let is_span = map.len() == 2 && map.contains_key("start") && map.contains_key("end");
            if is_span {
                *value = Value::Null;
                return;
            }
            for key in ["effects", "capabilities"] {
                if let Some(Value::Array(list)) = map.get_mut(key) {
                    list.sort_by_key(|item| item.to_string());
                    list.dedup();
                }
            }
            if let Some(Value::Object(named)) = map.get_mut("Named") {
                if let Some(Value::String(name)) = named.get_mut("name") {
                    *name = ast::canonical_primitive_type_name(name).to_string();
                }
            }
            map.values_mut().for_each(normalize_ast_value);
        }
        _ => {}
    }
}

fn first_difference(left: &Value, right: &Value, path: &str) -> Option<String> {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let child = format!("{path}.{key}");
                match b.get(key) {
                    Some(other) => {
                        if let Some(found) = first_difference(value, other, &child) {
                            return Some(found);
                        }
                    }
                    None => return Some(child),
                }
            }
            b.keys()
                .find(|key| !a.contains_key(*key))
                .map(|key| format!("{path}.{key}"))
        }
        (Value::Array(a), Value::Array(b)) => {
            for (idx, (x, y)) in a.iter().zip(b).enumerate() {
                if let Some(found) = first_difference(x, y, &format!("{path}[{idx}]")) {
                    return Some(found);
                }
            }
            (a.len() != b.len()).then(|| format!("{path}.len"))
        }
        _ => (left != right).then(|| path.to_string()),
    }
}

/// Original text, tokens and comments of the file being formatted.
struct SourceContext<'a> {
    text: &'a str,
    tokens: &'a [Token],
    comments: Vec<Comment>,
    /// Index of the first comment not yet printed. Comments after it may
    /// already be printed out of order (see [`Printer::hoist_interior`]).
    next_comment: usize,
    printed: Vec<bool>,
    /// Span of each literal token mapped to its index in `tokens`.
    literals: HashMap<(usize, usize), usize>,
    module_span: Option<Span>,
    import_spans: Vec<Span>,
}

impl<'a> SourceContext<'a> {
    fn new(
        text: &'a str,
        tokens: &'a [Token],
        comments: Vec<Comment>,
        program: &ast::Program,
    ) -> Self {
        let literals = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| {
                matches!(
                    token.kind,
                    TokenKind::Int(_)
                        | TokenKind::Float(_)
                        | TokenKind::String(_)
                        | TokenKind::Template(_)
                        | TokenKind::Char(_)
                )
            })
            .map(|(idx, token)| ((token.span.start, token.span.end), idx))
            .collect();
        Self {
            text,
            tokens,
            printed: vec![false; comments.len()],
            comments,
            next_comment: 0,
            literals,
            module_span: program.module.as_ref().map(|module| module.span),
            import_spans: program.imports.iter().map(|import| import.span).collect(),
        }
    }

    /// Index of the first comment that has not been printed yet.
    fn pending(&self) -> Option<usize> {
        (self.next_comment..self.comments.len()).find(|&idx| !self.printed[idx])
    }

    /// Marks comment `idx` as printed and moves the cursor past every
    /// printed comment.
    fn mark_printed(&mut self, idx: usize) {
        self.printed[idx] = true;
        while self.printed.get(self.next_comment) == Some(&true) {
            self.next_comment += 1;
        }
    }

    fn token_index_at_or_after(&self, pos: usize) -> usize {
        self.tokens.partition_point(|token| token.span.start < pos)
    }

    /// Source text of the literal token spanning exactly `span`, if `matches`
    /// accepts that token.
    fn literal_text(&self, span: Span, matches: impl Fn(&TokenKind) -> bool) -> Option<&'a str> {
        let idx = *self.literals.get(&(span.start, span.end))?;
        if !matches(&self.tokens[idx].kind) {
            return None;
        }
        self.text.get(span.start..span.end)
    }

    /// Start of the first `{` token at or after `pos`.
    fn open_brace_at_or_after(&self, pos: usize) -> Option<usize> {
        let idx = self.token_index_at_or_after(pos);
        self.tokens[idx..]
            .iter()
            .find(|token| matches!(token.kind, TokenKind::LBrace))
            .map(|token| token.span.start)
    }

    /// Start of the `}` matching the `{` that starts at `open`.
    fn matching_close_brace(&self, open: usize) -> Option<usize> {
        let idx = self.token_index_at_or_after(open);
        let mut depth = 0usize;
        for token in &self.tokens[idx..] {
            match token.kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(token.span.start);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Source spelling of a `self` receiver parameter (`self`, `&self`,
    /// `&mut self`, `self: Self`) with canonical spacing. The IR replaces the
    /// receiver type with the impl target and drops the borrow, so the
    /// source is the only record of how it was written.
    fn receiver_text(&self, span: Span) -> Option<String> {
        let start = self.token_index_at_or_after(span.start);
        let end = self
            .tokens
            .partition_point(|token| token.span.end <= span.end);
        let tokens = self.tokens.get(start..end)?;
        if !tokens
            .iter()
            .any(|token| matches!(&token.kind, TokenKind::Ident(name) if name == "self"))
        {
            return None;
        }
        let mut text = String::new();
        for token in tokens {
            text.push_str(self.text.get(token.span.start..token.span.end)?);
            if matches!(
                token.kind,
                TokenKind::KwMut | TokenKind::Colon | TokenKind::Comma
            ) {
                text.push(' ');
            }
        }
        Some(text)
    }

    /// Text of the tuple pattern of the `let` statement starting at `start`,
    /// rendered with canonical spacing, plus the binding names it contains.
    fn tuple_let_pattern(&self, start: usize) -> Option<(String, Vec<String>)> {
        let idx = self.token_index_at_or_after(start);
        let tokens = self.tokens.get(idx..)?;
        if !matches!(tokens.first()?.kind, TokenKind::KwLet) {
            return None;
        }
        let mut text = String::new();
        let mut names = Vec::new();
        let mut depth = 0usize;
        for (pos, token) in tokens.iter().enumerate().skip(1) {
            match &token.kind {
                TokenKind::LParen => {
                    depth += 1;
                    text.push('(');
                }
                TokenKind::RParen => {
                    depth = depth.checked_sub(1)?;
                    text.push(')');
                    if depth == 0 {
                        return Some((text, names));
                    }
                }
                TokenKind::Comma => {
                    text.push(',');
                    if !matches!(tokens.get(pos + 1)?.kind, TokenKind::RParen) {
                        text.push(' ');
                    }
                }
                TokenKind::Underscore => text.push('_'),
                TokenKind::Ident(name) => {
                    text.push_str(name);
                    names.push(name.clone());
                }
                _ => return None,
            }
            if depth == 0 {
                return None;
            }
        }
        None
    }
}

/// True when a whitespace-only line lies strictly between `from` and `to`.
fn has_blank_line(text: &str, from: usize, to: usize) -> bool {
    let Some(segment) = (from < to).then(|| text.get(from..to)).flatten() else {
        return false;
    };
    let lines = segment.split('\n').collect::<Vec<_>>();
    lines.len() >= 3
        && lines[1..lines.len() - 1]
            .iter()
            .any(|line| line.trim().is_empty())
}

fn visibility_prefix(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "",
        Visibility::Public => "pub ",
        Visibility::Crate => "pub(crate) ",
    }
}

fn stmt_span(stmt: &ir::Stmt) -> Span {
    match stmt {
        ir::Stmt::Let { span, .. }
        | ir::Stmt::Assign { span, .. }
        | ir::Stmt::Expr { span, .. }
        | ir::Stmt::Return { span, .. }
        | ir::Stmt::Assert { span, .. } => *span,
    }
}

/// The IR builder adds `Name::default()` for structs whose fields all have
/// defaults. It has no source syntax, so it is not printed.
fn is_synthesized_struct_default(f: &ir::Function, program: &ir::Program) -> bool {
    let Some(struct_name) = f.name.strip_suffix("::default") else {
        return false;
    };
    let body_is_empty_init = f.body.stmts.is_empty()
        && f.body.tail.as_ref().is_some_and(|tail| {
            matches!(&tail.kind, ir::ExprKind::StructInit { name, fields }
                if name == struct_name && fields.is_empty())
        });
    body_is_empty_init
        && program.items.iter().any(|item| {
            matches!(item, ir::Item::Struct(s)
                if s.name == struct_name
                    && s.span == f.span
                    && s.fields.iter().all(|field| field.default_value.is_some()))
        })
}

/// Bindings of a desugared `let (a, b) = expr;` group: the temporary and the
/// `name = tmp.i.j` lets that follow it.
struct TupleLetGroup<'s> {
    ty: Option<ir::TypeId>,
    expr: &'s ir::Expr,
    span: Span,
    bindings: Vec<(String, Vec<usize>)>,
}

fn tuple_let_group(stmts: &[ir::Stmt]) -> Option<TupleLetGroup<'_>> {
    let ir::Stmt::Let {
        name,
        mutable: false,
        ty,
        expr,
        span,
        ..
    } = stmts.first()?
    else {
        return None;
    };
    if !name.starts_with(TUPLE_LET_TMP_PREFIX) {
        return None;
    }
    let mut bindings = Vec::new();
    for stmt in &stmts[1..] {
        let ir::Stmt::Let {
            name: binding,
            mutable: false,
            ty: None,
            expr: field_expr,
            ..
        } = stmt
        else {
            break;
        };
        let Some(path) = tuple_field_path(field_expr, name) else {
            break;
        };
        bindings.push((binding.clone(), path));
    }
    Some(TupleLetGroup {
        ty: *ty,
        expr,
        span: *span,
        bindings,
    })
}

/// `tmp.0.1` → `[0, 1]` when the chain is rooted at `tmp`.
fn tuple_field_path(expr: &ir::Expr, root: &str) -> Option<Vec<usize>> {
    match &expr.kind {
        ir::ExprKind::Var(name) if name == root => Some(Vec::new()),
        ir::ExprKind::FieldAccess { base, field } => {
            let mut path = tuple_field_path(base, root)?;
            path.push(field.parse().ok()?);
            Some(path)
        }
        _ => None,
    }
}

/// Rebuilds a tuple pattern from binding paths when the source is not
/// available. Missing positions become `_`.
fn render_tuple_pattern(bindings: &[(String, Vec<usize>)], depth: usize) -> String {
    let width = bindings
        .iter()
        .filter_map(|(_, path)| path.get(depth).map(|idx| idx + 1))
        .max()
        .unwrap_or(1);
    let mut parts = Vec::with_capacity(width);
    for idx in 0..width {
        let nested = bindings
            .iter()
            .filter(|(_, path)| path.get(depth) == Some(&idx))
            .cloned()
            .collect::<Vec<_>>();
        let part = match nested.as_slice() {
            [] => "_".to_string(),
            [(name, path)] if path.len() == depth + 1 => name.clone(),
            _ => render_tuple_pattern(&nested, depth + 1),
        };
        parts.push(part);
    }
    if parts.len() == 1 {
        format!("({},)", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

fn format_type_repr(repr: &str) -> String {
    let repr = repr.trim();
    if repr == HOLE_TYPE_REPR {
        return "_".to_string();
    }
    let Some(args) = extract_generic_args(repr) else {
        return ir::canonical_primitive_type_name(repr).to_string();
    };
    let rendered = args
        .iter()
        .map(|arg| format_type_repr(arg))
        .collect::<Vec<_>>();
    let base = ir::canonical_primitive_type_name(base_type_name(repr));
    if base == TUPLE_INTERNAL_NAME {
        if rendered.len() == 1 {
            format!("({},)", rendered[0])
        } else {
            format!("({})", rendered.join(", "))
        }
    } else {
        format!("{base}[{}]", rendered.join(", "))
    }
}

fn base_type_name(ty: &str) -> &str {
    ty.split_once('[').map(|(base, _)| base).unwrap_or(ty)
}

fn extract_generic_args(ty: &str) -> Option<Vec<String>> {
    let start = ty.find('[')?;
    if !ty.ends_with(']') || start + 1 >= ty.len() {
        return None;
    }
    let inner = &ty[start + 1..ty.len() - 1];
    Some(split_top_level(inner))
}

fn split_top_level(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (idx, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(input[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn format_attributes_inline(attrs: &[Attribute], source: Option<&SourceContext>) -> String {
    attrs
        .iter()
        .map(|attr| format_single_attribute(attr, source))
        .collect::<Vec<_>>()
        .join(" ")
        + " "
}

fn format_single_attribute(attr: &Attribute, source: Option<&SourceContext>) -> String {
    let mut out = String::new();
    out.push_str("#[");
    out.push_str(&attr.name);
    if !attr.args.is_empty() {
        out.push('(');
        out.push_str(
            &attr
                .args
                .iter()
                .map(|arg| match arg {
                    AttributeArg::Positional(value) => format_attribute_value(value, source),
                    AttributeArg::Named { name, value, .. } => {
                        format!("{name} = {}", format_attribute_value(value, source))
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push(')');
    }
    out.push(']');
    out
}

/// Literal values keep their source spelling (`0xFF`, `"\u{301}"`).
fn format_attribute_value(value: &AttributeValue, source: Option<&SourceContext>) -> String {
    let literal = source.and_then(|source| {
        source.literal_text(value.span, |kind| {
            matches!(
                (&value.kind, kind),
                (AttributeValueKind::Int(_), TokenKind::Int(_))
                    | (AttributeValueKind::String(_), TokenKind::String(_))
            )
        })
    });
    if let Some(text) = literal {
        return text.to_string();
    }
    match &value.kind {
        AttributeValueKind::String(text) => format!("\"{}\"", escape_aic_string_literal(text)),
        AttributeValueKind::Int(number) => number.to_string(),
        AttributeValueKind::Bool(BoolLiteral::True) => "true".to_string(),
        AttributeValueKind::Bool(BoolLiteral::False) => "false".to_string(),
        AttributeValueKind::Ident(name) => name.clone(),
    }
}

fn format_generic_params(generics: &[ir::GenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }
    let params = generics
        .iter()
        .map(|g| {
            if g.bounds.is_empty() {
                g.name.clone()
            } else {
                format!("{}: {}", g.name, g.bounds.join(" + "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{params}]")
}

fn sorted_unique(items: &[String]) -> Vec<String> {
    let mut items = items.to_vec();
    items.sort();
    items.dedup();
    items
}

struct Printer<'a> {
    out: String,
    type_map: BTreeMap<ir::TypeId, String>,
    source: Option<SourceContext<'a>>,
    /// Index expressions of place assignments (`v[i] = x`), which the IR
    /// builder binds to hidden `let`s; printed back inside the brackets.
    place_indexes: BTreeMap<String, ir::Expr>,
}

impl<'a> Printer<'a> {
    fn new(program: &ir::Program, source: Option<SourceContext<'a>>) -> Self {
        Self {
            out: String::new(),
            type_map: type_map(program),
            source,
            place_indexes: BTreeMap::new(),
        }
    }

    fn program(mut self, program: &ir::Program) -> String {
        if let Some(module) = &program.module {
            let span = self.source.as_ref().and_then(|source| source.module_span);
            if let Some(span) = span {
                self.leading(span.start, span.end, 0, None);
            }
            self.out.push_str("module ");
            self.out.push_str(&module.join("."));
            self.out.push(';');
            if let Some(span) = span {
                self.trailing(span.end);
            }
            self.out.push_str("\n\n");
        }

        self.imports(program);

        let mut first = true;
        let mut last_end = None;
        for item in &program.items {
            if let ir::Item::Function(f) = item {
                if is_synthesized_struct_default(f, program) {
                    continue;
                }
            }
            if !first {
                self.out.push('\n');
            }
            first = false;
            last_end = Some(match item {
                ir::Item::Function(f) => {
                    self.function(f);
                    f.span.end
                }
                ir::Item::Struct(s) => {
                    self.struct_def(s);
                    s.span.end
                }
                ir::Item::Enum(e) => {
                    self.enum_def(e);
                    e.span.end
                }
                ir::Item::Trait(t) => {
                    self.trait_def(t);
                    t.span.end
                }
                ir::Item::Impl(i) => {
                    self.impl_def(i);
                    i.span.end
                }
            });
        }

        self.comments_before(usize::MAX, 0, last_end);
        self.out
    }

    fn imports(&mut self, program: &ir::Program) {
        if program.imports.is_empty() {
            return;
        }
        let spans = self
            .source
            .as_ref()
            .map(|source| source.import_spans.clone())
            .filter(|spans| spans.len() == program.imports.len());
        // Each import is printed with its own comments, then the chunks are
        // sorted so comments travel with the import they describe.
        let mut chunks = Vec::with_capacity(program.imports.len());
        for (idx, import) in program.imports.iter().enumerate() {
            let saved = std::mem::take(&mut self.out);
            let span = spans.as_ref().map(|spans| spans[idx]);
            if let Some(span) = span {
                self.leading(span.start, span.end, 0, None);
            }
            self.out.push_str("import ");
            self.out.push_str(&import.join("."));
            self.out.push(';');
            if let Some(span) = span {
                self.trailing(span.end);
            }
            self.out.push('\n');
            chunks.push((import, std::mem::replace(&mut self.out, saved)));
        }
        chunks.sort_by(|a, b| a.0.cmp(b.0));
        for (_, chunk) in chunks {
            self.out.push_str(&chunk);
        }
        self.out.push('\n');
    }

    // ---- trivia -------------------------------------------------------

    fn push_indent(&mut self, indent: usize) {
        self.out.push_str(&" ".repeat(indent));
    }

    /// Ends the output with exactly one empty line, unless it already does or
    /// a block was just opened.
    fn blank_line(&mut self) {
        if self.out.ends_with('\n') && !self.out.ends_with("\n\n") && !self.out.ends_with("{\n") {
            self.out.push('\n');
        }
    }

    /// Emits every pending comment that starts before `anchor`, one per line
    /// at `indent`. Blank lines between consecutive elements are kept.
    /// Returns the end of the last emitted comment, or `prev_end`.
    fn comments_before(
        &mut self,
        anchor: usize,
        indent: usize,
        prev_end: Option<usize>,
    ) -> Option<usize> {
        let mut last = prev_end;
        loop {
            let Some(source) = self.source.as_mut() else {
                return last;
            };
            let Some(idx) = source.pending() else {
                return last;
            };
            let comment = &source.comments[idx];
            if comment.span.start >= anchor {
                return last;
            }
            let blank = last.is_some_and(|end| {
                // Only the whitespace directly before the comment counts:
                // `end` can lie before code that is printed elsewhere.
                let trivia_start = source.text[..comment.span.start].trim_end().len().max(end);
                has_blank_line(source.text, trivia_start, comment.span.start)
            });
            let text = comment.text.clone();
            let end = comment.span.end;
            source.mark_printed(idx);

            if !self.out.is_empty() && !self.out.ends_with('\n') {
                self.out.push('\n');
            }
            if blank {
                self.blank_line();
            }
            self.push_indent(indent);
            self.out.push_str(&text);
            self.out.push('\n');
            last = Some(end);
        }
    }

    /// Comments before the element starting at `start`, then a blank line if
    /// the source had one between them (or between the previous element and
    /// this one). Comments up to `anchor` (at or after `start`) are emitted
    /// too: they sit inside the element at a spot the canonical layout puts
    /// on the element's first line, so they move above it.
    fn leading(&mut self, start: usize, anchor: usize, indent: usize, prev_end: Option<usize>) {
        let last = self.comments_before(anchor.max(start), indent, prev_end);
        let blank = match (&self.source, last) {
            (Some(source), Some(end)) if end <= start => has_blank_line(source.text, end, start),
            _ => false,
        };
        if blank {
            self.blank_line();
        }
    }

    /// Comments before the element spanning `span`, then the comments
    /// inside it that lie outside every nested region in `regions` (blocks
    /// and `match` bodies print their own comments). The canonical layout
    /// prints those parts of the element inline, so their comments move
    /// above it.
    fn leading_hoisted(
        &mut self,
        span: Span,
        regions: &[(usize, usize)],
        indent: usize,
        prev_end: Option<usize>,
    ) {
        self.leading(span.start, span.start, indent, prev_end);
        let Some(source) = self.source.as_mut() else {
            return;
        };
        let mut hoisted = Vec::new();
        for idx in source.next_comment..source.comments.len() {
            let start = source.comments[idx].span.start;
            if start >= span.end {
                break;
            }
            let in_region = regions
                .iter()
                .any(|&(from, to)| from <= start && start < to);
            if !source.printed[idx] && start >= span.start && !in_region {
                hoisted.push(idx);
            }
        }
        let texts = hoisted
            .into_iter()
            .map(|idx| {
                source.mark_printed(idx);
                source.comments[idx].text.clone()
            })
            .collect::<Vec<_>>();
        for text in texts {
            self.push_indent(indent);
            self.out.push_str(&text);
            self.out.push('\n');
        }
    }

    /// Appends ` // ...` when the next comment follows `end` on the same
    /// source line with only `,`/`;` in between.
    fn trailing(&mut self, end: usize) {
        let Some(source) = self.source.as_mut() else {
            return;
        };
        let Some(idx) = source.pending() else {
            return;
        };
        let comment = &source.comments[idx];
        let Some(between) = source.text.get(end..comment.span.start) else {
            return;
        };
        if !between
            .chars()
            .all(|ch| matches!(ch, ' ' | '\t' | ',' | ';'))
        {
            return;
        }
        self.out.push(' ');
        self.out.push_str(&comment.text);
        source.mark_printed(idx);
    }

    /// Trailing comment on the line of the `{` that starts at `open`.
    fn trailing_after_open_brace(&mut self, open: Option<usize>) {
        let Some(open) = open else {
            return;
        };
        let is_brace = self
            .source
            .as_ref()
            .is_some_and(|source| source.text.as_bytes().get(open) == Some(&b'{'));
        if is_brace {
            self.trailing(open + 1);
        }
    }

    /// End of the token starting at or after `pos` when `matches` accepts
    /// it; `pos` otherwise.
    fn end_of_token_at(&self, pos: usize, matches: impl Fn(&TokenKind) -> bool) -> usize {
        self.source
            .as_ref()
            .and_then(|source| source.tokens.get(source.token_index_at_or_after(pos)))
            .filter(|token| matches(&token.kind))
            .map_or(pos, |token| token.span.end)
    }

    /// True when a pending comment lies between the `{` at `open` and its
    /// matching `}`.
    fn has_comments_inside(&self, open: Option<usize>) -> bool {
        let Some(source) = self.source.as_ref() else {
            return false;
        };
        let Some(close) = open.and_then(|open| source.matching_close_brace(open)) else {
            return false;
        };
        source
            .pending()
            .is_some_and(|idx| source.comments[idx].span.start < close)
    }

    fn open_brace_after(&self, pos: usize) -> Option<usize> {
        self.source.as_ref()?.open_brace_at_or_after(pos)
    }

    fn close_brace_for(&self, open: Option<usize>) -> Option<usize> {
        self.source.as_ref()?.matching_close_brace(open?)
    }

    /// Prints attributes one per line with their comments, then the comments
    /// between the last attribute and the element header at `header_start`,
    /// including those up to `header_anchor` (see [`Printer::leading`]).
    fn attributes(
        &mut self,
        attrs: &[Attribute],
        header_start: usize,
        header_anchor: usize,
        indent: usize,
        prev_end: Option<usize>,
    ) {
        let mut prev = prev_end;
        for attr in attrs {
            self.leading(attr.span.start, attr.span.start, indent, prev);
            self.push_indent(indent);
            self.out
                .push_str(&format_single_attribute(attr, self.source.as_ref()));
            self.trailing(attr.span.end);
            self.out.push('\n');
            prev = Some(attr.span.end);
        }
        let anchor = match attrs.last() {
            Some(last) => self
                .source
                .as_ref()
                .and_then(|source| {
                    source
                        .tokens
                        .get(source.token_index_at_or_after(last.span.end))
                        .map(|token| token.span.start)
                })
                .unwrap_or(header_start),
            None => header_start,
        };
        self.leading(anchor, header_anchor, indent, prev);
    }

    // ---- items ----------------------------------------------------------

    fn display_type(&self, ty: &ir::TypeId) -> String {
        self.type_map
            .get(ty)
            .map(|repr| format_type_repr(repr))
            .unwrap_or_else(|| "_".to_string())
    }

    fn params(&self, params: &[ir::Param]) -> String {
        params
            .iter()
            .map(|p| {
                let attrs = if p.attrs.is_empty() {
                    String::new()
                } else {
                    format_attributes_inline(&p.attrs, self.source.as_ref())
                };
                let receiver = (p.name == "self")
                    .then(|| self.source.as_ref()?.receiver_text(p.span))
                    .flatten();
                match receiver {
                    Some(receiver) => format!("{attrs}{receiver}"),
                    // Without source text, `self: Type` would take the
                    // receiver by value, so `&mut self` keeps its spelling.
                    None if p.mutable_receiver => format!("{attrs}&mut self"),
                    None => format!("{attrs}{}: {}", p.name, self.display_type(&p.ty)),
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn effects_and_capabilities(&mut self, f: &ir::Function) {
        if !f.effects.is_empty() {
            self.out.push_str(" effects { ");
            self.out.push_str(&sorted_unique(&f.effects).join(", "));
            self.out.push_str(" }");
        }
        if !f.capabilities.is_empty() {
            self.out.push_str(" capabilities { ");
            self.out
                .push_str(&sorted_unique(&f.capabilities).join(", "));
            self.out.push_str(" }");
        }
    }

    fn function(&mut self, f: &ir::Function) {
        let header_anchor = if f.is_extern || f.is_intrinsic {
            f.span.end
        } else if decode_internal_type_alias(&f.name).is_some()
            || decode_internal_const(&f.name).is_some()
        {
            f.body
                .tail
                .as_deref()
                .and_then(first_nested_region)
                .map_or(f.span.end, |region| region.min(f.span.end))
        } else {
            signature_anchor(f)
        };
        self.attributes(&f.attrs, f.span.start, header_anchor, 0, None);
        if let Some(name) = decode_internal_type_alias(&f.name) {
            self.out.push_str("type ");
            self.out.push_str(name);
            self.out.push_str(&format_generic_params(&f.generics));
            self.out.push_str(" = ");
            self.out.push_str(&self.display_type(&f.ret_type));
            self.out.push(';');
            self.trailing(f.span.end);
            self.out.push('\n');
            return;
        }
        if let Some(name) = decode_internal_const(&f.name) {
            self.out.push_str("const ");
            self.out.push_str(name);
            self.out.push_str(": ");
            self.out.push_str(&self.display_type(&f.ret_type));
            self.out.push_str(" = ");
            if let Some(expr) = &f.body.tail {
                self.expr(expr, 0, 0);
            } else {
                self.out.push_str("()");
            }
            self.out.push(';');
            self.trailing(f.span.end);
            self.out.push('\n');
            return;
        }

        self.out.push_str(visibility_prefix(f.visibility));
        if f.is_extern {
            self.out.push_str("extern \"");
            self.out.push_str(f.extern_abi.as_deref().unwrap_or("C"));
            self.out.push_str("\" fn ");
            self.out.push_str(&f.name);
            self.out.push_str(&format_generic_params(&f.generics));
            self.out.push('(');
            self.out.push_str(&self.params(&f.params));
            self.out.push_str(") -> ");
            self.out.push_str(&self.display_type(&f.ret_type));
            self.out.push(';');
            self.trailing(f.span.end);
            self.out.push('\n');
            return;
        }

        if f.is_intrinsic {
            self.out.push_str("intrinsic fn ");
            self.out.push_str(&f.name);
            self.out.push_str(&format_generic_params(&f.generics));
            self.out.push('(');
            self.out.push_str(&self.params(&f.params));
            self.out.push_str(") -> ");
            self.out.push_str(&self.display_type(&f.ret_type));
            self.effects_and_capabilities(f);
            self.out.push(';');
            self.trailing(f.span.end);
            self.out.push('\n');
            return;
        }

        self.signature(f, &f.name);
        self.out.push(' ');
        self.block(&f.body, 0);
        self.trailing(f.body.span.end);
        self.out.push('\n');
    }

    /// `[async ][unsafe ]fn name[generics](params) -> ret` plus effects,
    /// capabilities and contracts.
    fn signature(&mut self, f: &ir::Function, name: &str) {
        if f.is_async {
            self.out.push_str("async ");
        }
        if f.is_unsafe {
            self.out.push_str("unsafe ");
        }
        self.out.push_str("fn ");
        self.out.push_str(name);
        self.out.push_str(&format_generic_params(&f.generics));
        self.out.push('(');
        self.out.push_str(&self.params(&f.params));
        self.out.push_str(") -> ");
        self.out.push_str(&self.display_type(&f.ret_type));
        self.effects_and_capabilities(f);
        if let Some(req) = &f.requires {
            self.out.push_str(" requires ");
            self.expr(req, 0, 0);
        }
        if let Some(ens) = &f.ensures {
            self.out.push_str(" ensures ");
            self.expr(ens, 0, 0);
        }
    }

    fn method_name(method: &ir::Function) -> &str {
        method
            .name
            .rsplit("::")
            .next()
            .unwrap_or(method.name.as_str())
    }

    fn struct_def(&mut self, s: &ir::StructDef) {
        let open = self.open_brace_after(s.span.start);
        self.attributes(
            &s.attrs,
            s.span.start,
            open.unwrap_or(s.span.start),
            0,
            None,
        );
        self.out.push_str(visibility_prefix(s.visibility));
        self.out.push_str("struct ");
        self.out.push_str(&s.name);
        self.out.push_str(&format_generic_params(&s.generics));
        self.out.push_str(" {");
        self.trailing_after_open_brace(open);
        self.out.push('\n');
        let mut prev = None;
        for field in &s.fields {
            let anchor = field
                .default_value
                .as_ref()
                .and_then(first_nested_region)
                .map_or(field.span.end, |region| region.min(field.span.end));
            self.attributes(&field.attrs, field.span.start, anchor, 4, prev);
            self.out.push_str("    ");
            self.out.push_str(visibility_prefix(field.visibility));
            self.out.push_str(&field.name);
            self.out.push_str(": ");
            self.out.push_str(&self.display_type(&field.ty));
            if let Some(default) = &field.default_value {
                self.out.push_str(" = ");
                self.expr(default, 0, 4);
            }
            self.out.push(',');
            self.trailing(field.span.end);
            self.out.push('\n');
            prev = Some(field.span.end);
        }
        let close = self.close_brace_for(open);
        self.comments_before(close.unwrap_or(s.span.end), 4, prev);
        self.out.push('}');
        if let Some(inv) = &s.invariant {
            self.out.push_str(" invariant ");
            self.expr(inv, 0, 0);
        }
        self.trailing(s.span.end);
        self.out.push('\n');
    }

    fn enum_def(&mut self, e: &ir::EnumDef) {
        let open = self.open_brace_after(e.span.start);
        self.attributes(
            &e.attrs,
            e.span.start,
            open.unwrap_or(e.span.start),
            0,
            None,
        );
        self.out.push_str(visibility_prefix(e.visibility));
        self.out.push_str("enum ");
        self.out.push_str(&e.name);
        self.out.push_str(&format_generic_params(&e.generics));
        self.out.push_str(" {");
        self.trailing_after_open_brace(open);
        self.out.push('\n');
        let mut prev = None;
        for variant in &e.variants {
            self.attributes(
                &variant.attrs,
                variant.span.start,
                variant.span.end,
                4,
                prev,
            );
            self.out.push_str("    ");
            self.out.push_str(&variant.name);
            if let Some(ty) = variant.payload {
                self.out.push('(');
                self.out.push_str(&self.display_type(&ty));
                self.out.push(')');
            }
            self.out.push(',');
            // A variant's span stops at its payload type, before the `)`.
            let end = match variant.payload {
                Some(_) => {
                    self.end_of_token_at(variant.span.end, |kind| matches!(kind, TokenKind::RParen))
                }
                None => variant.span.end,
            };
            self.trailing(end);
            self.out.push('\n');
            prev = Some(end);
        }
        let close = self.close_brace_for(open);
        self.comments_before(close.unwrap_or(e.span.end), 4, prev);
        self.out.push('}');
        self.trailing(e.span.end);
        self.out.push('\n');
    }

    fn trait_def(&mut self, t: &ir::TraitDef) {
        let open = self
            .open_brace_after(t.span.start)
            .filter(|open| *open < t.span.end);
        self.attributes(&t.attrs, t.span.start, open.unwrap_or(t.span.end), 0, None);
        self.out.push_str(visibility_prefix(t.visibility));
        self.out.push_str("trait ");
        self.out.push_str(&t.name);
        self.out.push_str(&format_generic_params(&t.generics));
        if t.methods.is_empty() && !self.has_comments_inside(open) {
            self.out.push(';');
            self.trailing(t.span.end);
            self.out.push('\n');
            return;
        }
        self.out.push_str(" {");
        self.trailing_after_open_brace(open);
        self.out.push('\n');
        let mut prev = None;
        for method in &t.methods {
            self.attributes(&method.attrs, method.span.start, method.span.end, 4, prev);
            self.out.push_str("    ");
            self.out.push_str(visibility_prefix(method.visibility));
            self.signature(method, Self::method_name(method));
            self.out.push(';');
            self.trailing(method.span.end);
            self.out.push('\n');
            prev = Some(method.span.end);
        }
        let close = self.close_brace_for(open);
        self.comments_before(close.unwrap_or(t.span.end), 4, prev);
        self.out.push('}');
        self.trailing(t.span.end);
        self.out.push('\n');
    }

    fn impl_def(&mut self, i: &ir::ImplDef) {
        let open = self
            .open_brace_after(i.span.start)
            .filter(|open| *open < i.span.end);
        self.attributes(&i.attrs, i.span.start, open.unwrap_or(i.span.end), 0, None);
        self.out.push_str(visibility_prefix(i.visibility));
        self.out.push_str("impl ");
        if i.is_inherent {
            if let Some(target) = i.target.as_ref() {
                self.out.push_str(&self.display_type(target));
            } else {
                self.out.push_str(&i.trait_name);
            }
        } else {
            self.out.push_str(&i.trait_name);
            if !i.trait_args.is_empty() {
                self.out.push('[');
                self.out.push_str(
                    &i.trait_args
                        .iter()
                        .map(|ty| self.display_type(ty))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                self.out.push(']');
            }
            if i.methods.is_empty() && !self.has_comments_inside(open) {
                self.out.push(';');
                self.trailing(i.span.end);
                self.out.push('\n');
                return;
            }
        }
        self.out.push_str(" {");
        self.trailing_after_open_brace(open);
        self.out.push('\n');
        let mut prev = None;
        for method in &i.methods {
            self.attributes(
                &method.attrs,
                method.span.start,
                signature_anchor(method),
                4,
                prev,
            );
            self.out.push_str("    ");
            self.out.push_str(visibility_prefix(method.visibility));
            self.signature(method, Self::method_name(method));
            self.out.push(' ');
            self.block(&method.body, 4);
            self.trailing(method.body.span.end);
            self.out.push('\n');
            prev = Some(method.body.span.end);
        }
        let close = self.close_brace_for(open);
        self.comments_before(close.unwrap_or(i.span.end), 4, prev);
        self.out.push('}');
        self.trailing(i.span.end);
        self.out.push('\n');
    }

    // ---- blocks and statements -------------------------------------------

    fn block(&mut self, block: &ir::Block, indent: usize) {
        self.out.push('{');
        self.trailing_after_open_brace(Some(block.span.start));
        self.out.push('\n');
        let inner = indent + 4;
        let mut prev = None;
        let mut idx = 0;
        while idx < block.stmts.len() {
            if let ir::Stmt::Let { name, expr, .. } = &block.stmts[idx] {
                if name.starts_with(crate::ir_builder::PLACE_INDEX_BINDING_PREFIX) {
                    self.place_indexes.insert(name.clone(), expr.clone());
                    idx += 1;
                    continue;
                }
            }
            let span = stmt_span(&block.stmts[idx]);
            let regions = stmt_nested_regions(&block.stmts[idx]);
            self.leading_hoisted(span, &regions, inner, prev);
            self.push_indent(inner);
            idx += match tuple_let_group(&block.stmts[idx..]) {
                Some(group) => {
                    self.tuple_let(&group, inner);
                    1 + group.bindings.len()
                }
                None => {
                    self.stmt(&block.stmts[idx], inner);
                    1
                }
            };
            self.trailing(span.end);
            self.out.push('\n');
            prev = Some(span.end);
        }

        if let Some(tail) = &block.tail {
            let mut regions = Vec::new();
            nested_regions(tail, &mut regions);
            self.leading_hoisted(tail.span, &regions, inner, prev);
            self.push_indent(inner);
            self.expr(tail, 0, inner);
            self.trailing(tail.span.end);
            self.out.push('\n');
            prev = Some(tail.span.end);
        }
        self.comments_before(block.span.end, inner, prev);
        self.push_indent(indent);
        self.out.push('}');
    }

    fn stmt(&mut self, stmt: &ir::Stmt, indent: usize) {
        match stmt {
            ir::Stmt::Let {
                name,
                mutable,
                ty,
                expr,
                ..
            } => {
                self.out.push_str("let ");
                if *mutable {
                    self.out.push_str("mut ");
                }
                self.out.push_str(name);
                if let Some(ty) = ty {
                    self.out.push_str(": ");
                    self.out.push_str(&self.display_type(ty));
                }
                self.out.push_str(" = ");
                self.expr(expr, 0, indent);
                self.out.push(';');
            }
            ir::Stmt::Assign {
                target, path, expr, ..
            } => {
                self.out.push_str(target);
                for projection in path {
                    match projection {
                        ir::PlaceProjection::Field { name, .. } => {
                            self.out.push('.');
                            self.out.push_str(name);
                        }
                        ir::PlaceProjection::Index { binding, .. } => {
                            self.out.push('[');
                            match self.place_indexes.get(binding).cloned() {
                                Some(index) => self.expr(&index, 0, indent),
                                None => self.out.push_str(binding),
                            }
                            self.out.push(']');
                        }
                    }
                }
                self.out.push_str(" = ");
                self.expr(expr, 0, indent);
                self.out.push(';');
            }
            ir::Stmt::Expr { expr, .. } => {
                self.expr(expr, 0, indent);
                self.out.push(';');
            }
            ir::Stmt::Return { expr, .. } => {
                self.out.push_str("return");
                if let Some(expr) = expr {
                    self.out.push(' ');
                    self.expr(expr, 0, indent);
                }
                self.out.push(';');
            }
            ir::Stmt::Assert { expr, message, .. } => {
                self.out.push_str("assert ");
                self.expr(expr, 0, indent);
                self.out.push_str("; // ");
                self.out.push_str(message);
            }
        }
    }

    fn tuple_let(&mut self, group: &TupleLetGroup<'_>, indent: usize) {
        let names = group
            .bindings
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        let pattern = self
            .source
            .as_ref()
            .and_then(|source| source.tuple_let_pattern(group.span.start))
            .filter(|(_, source_names)| *source_names == names)
            .map(|(pattern, _)| pattern)
            .unwrap_or_else(|| render_tuple_pattern(&group.bindings, 0));
        self.out.push_str("let ");
        self.out.push_str(&pattern);
        if let Some(ty) = &group.ty {
            self.out.push_str(": ");
            self.out.push_str(&self.display_type(ty));
        }
        self.out.push_str(" = ");
        self.expr(group.expr, 0, indent);
        self.out.push(';');
    }

    // ---- expressions ------------------------------------------------------

    fn literal_text(&self, span: Span, matches: impl Fn(&TokenKind) -> bool) -> Option<&'a str> {
        self.source.as_ref()?.literal_text(span, matches)
    }

    fn expr(&mut self, expr: &ir::Expr, parent_prec: u8, indent: usize) {
        if let Some(rendered_for) = extract_for_syntax(expr) {
            self.out.push_str("for ");
            self.out.push_str(&rendered_for.binding);
            self.out.push_str(" in ");
            match &rendered_for.iterable {
                RenderedIterable::Expr(iterable) => self.expr(iterable, 0, indent),
                RenderedIterable::Range { start, end } => {
                    self.expr(start, 0, indent);
                    self.out.push_str("..");
                    self.expr(end, 0, indent);
                }
            }
            self.out.push(' ');
            self.block(&rendered_for.body, indent);
            return;
        }

        match &expr.kind {
            ir::ExprKind::Int(v, ..) => {
                let value = *v;
                if let Some(text) = self.literal_text(
                    expr.span,
                    |kind| matches!(kind, TokenKind::Int(token) if token.value == value),
                ) {
                    self.out.push_str(text);
                } else if let Some(meta) = expr.int_literal_metadata() {
                    self.out.push_str(&meta.raw_literal_text);
                    self.out.push_str(&meta.suffix_text);
                } else {
                    self.out.push_str(&v.to_string());
                }
            }
            ir::ExprKind::Float(v, ..) => {
                let value = *v;
                if let Some(text) = self.literal_text(expr.span, |kind| {
                    matches!(kind, TokenKind::Float(token)
                        if token.value.to_bits() == value.to_bits())
                }) {
                    self.out.push_str(text);
                } else if let Some(meta) = expr.float_literal_metadata() {
                    self.out.push_str(&meta.raw_literal_text);
                    self.out.push_str(&meta.suffix_text);
                } else {
                    self.out.push_str(&render_float_literal(*v));
                }
            }
            ir::ExprKind::Bool(v) => self.out.push_str(if *v { "true" } else { "false" }),
            ir::ExprKind::Char(v) => {
                let value = *v;
                if let Some(text) = self.literal_text(
                    expr.span,
                    |kind| matches!(kind, TokenKind::Char(c) if *c == value),
                ) {
                    self.out.push_str(text);
                } else {
                    self.out.push_str(&format!("{:?}", v));
                }
            }
            ir::ExprKind::String(v) => {
                if let Some(text) = self.literal_text(expr.span, |kind| {
                    // A template without interpolations parses to a plain string.
                    matches!(kind, TokenKind::String(s) if s == v)
                        || matches!(kind, TokenKind::Template(_))
                }) {
                    self.out.push_str(text);
                } else {
                    self.out.push('"');
                    self.out.push_str(&escape_aic_string_literal(v));
                    self.out.push('"');
                }
            }
            ir::ExprKind::Unit => self.out.push_str("()"),
            ir::ExprKind::Var(v) => self.out.push_str(v),
            ir::ExprKind::Call {
                callee,
                args,
                arg_names,
                ..
            } => {
                self.expr(callee, PREFIX_OPERAND_PREC, indent);
                self.out.push('(');
                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        self.out.push_str(", ");
                    }
                    if let Some(name) = arg_names.get(idx).and_then(|name| name.as_deref()) {
                        self.out.push_str(name);
                        self.out.push_str(": ");
                    }
                    self.expr(arg, 0, indent);
                }
                self.out.push(')');
            }
            ir::ExprKind::TemplateLiteral { template, args } => {
                // The whole template is one token, so its source spelling
                // (prefix, escapes, interpolation spacing) can be kept as is.
                match self.literal_text(expr.span, |kind| matches!(kind, TokenKind::Template(_))) {
                    Some(text) => self.out.push_str(text),
                    None => self.template_literal(template, args, indent),
                }
            }
            ir::ExprKind::Closure {
                params,
                ret_type,
                body,
            } => {
                self.out.push('|');
                for (idx, param) in params.iter().enumerate() {
                    if idx > 0 {
                        self.out.push_str(", ");
                    }
                    self.out.push_str(&param.name);
                    if let Some(ty) = param.ty {
                        self.out.push_str(": ");
                        self.out.push_str(&self.display_type(&ty));
                    }
                }
                self.out.push('|');
                self.out.push_str(" -> ");
                self.out.push_str(&self.display_type(ret_type));
                self.out.push(' ');
                self.block(body, indent);
            }
            ir::ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.out.push_str("if ");
                self.expr(cond, 0, indent);
                self.out.push(' ');
                self.block(then_block, indent);
                self.out.push_str(" else ");
                match else_if_expr(else_block) {
                    Some(nested) => self.expr(nested, 0, indent),
                    None => self.block(else_block, indent),
                }
            }
            ir::ExprKind::While { cond, body } => {
                self.out.push_str("while ");
                self.expr(cond, 0, indent);
                self.out.push(' ');
                self.block(body, indent);
            }
            ir::ExprKind::Loop { body } => {
                self.out.push_str("loop ");
                self.block(body, indent);
            }
            ir::ExprKind::Break { expr } => {
                self.out.push_str("break");
                if let Some(expr) = expr {
                    self.out.push(' ');
                    self.expr(expr, 0, indent);
                }
            }
            ir::ExprKind::Return { expr } => {
                self.out.push_str("return");
                if let Some(expr) = expr {
                    self.out.push(' ');
                    self.expr(expr, 0, indent);
                }
            }
            ir::ExprKind::Continue => self.out.push_str("continue"),
            ir::ExprKind::Match {
                expr: scrutinee,
                arms,
            } => {
                self.out.push_str("match ");
                self.expr(scrutinee, 0, indent);
                self.out.push_str(" {");
                let open = self.open_brace_after(scrutinee.span.end);
                if open.is_some_and(|open| open < expr.span.end) {
                    self.trailing_after_open_brace(open);
                }
                self.out.push('\n');
                let inner = indent + 4;
                let mut prev = None;
                for arm in arms {
                    let mut regions = Vec::new();
                    for part in arm.guard.iter().chain(std::iter::once(&arm.body)) {
                        nested_regions(part, &mut regions);
                    }
                    self.leading_hoisted(arm.span, &regions, inner, prev);
                    self.push_indent(inner);
                    self.pattern(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.out.push_str(" if ");
                        self.expr(guard, 0, inner);
                    }
                    self.out.push_str(" => ");
                    self.expr(&arm.body, 0, inner);
                    self.out.push(',');
                    self.trailing(arm.span.end);
                    self.out.push('\n');
                    prev = Some(arm.span.end);
                }
                self.comments_before(expr.span.end, inner, prev);
                self.push_indent(indent);
                self.out.push('}');
            }
            ir::ExprKind::Binary { op, lhs, rhs } => {
                let (prec, op_str) = binop_info(*op);
                let needs_paren = prec < parent_prec;
                if needs_paren {
                    self.out.push('(');
                }
                self.expr(lhs, prec, indent);
                self.out.push(' ');
                self.out.push_str(op_str);
                self.out.push(' ');
                self.expr(rhs, prec + 1, indent);
                if needs_paren {
                    self.out.push(')');
                }
            }
            ir::ExprKind::Unary { op, expr } => {
                let token = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "~",
                };
                self.out.push_str(token);
                self.expr(expr, PREFIX_OPERAND_PREC, indent);
            }
            ir::ExprKind::Borrow { mutable, expr } => {
                self.out.push('&');
                if *mutable {
                    self.out.push_str("mut ");
                }
                self.expr(expr, PREFIX_OPERAND_PREC, indent);
            }
            ir::ExprKind::Await { expr } => {
                let needs_paren = matches!(
                    expr.kind,
                    ir::ExprKind::Binary { .. }
                        | ir::ExprKind::If { .. }
                        | ir::ExprKind::Match { .. }
                );
                self.out.push_str("await ");
                if needs_paren {
                    self.out.push('(');
                }
                self.expr(expr, 9, indent);
                if needs_paren {
                    self.out.push(')');
                }
            }
            ir::ExprKind::Try { expr } => {
                let needs_paren = matches!(
                    expr.kind,
                    ir::ExprKind::Binary { .. }
                        | ir::ExprKind::If { .. }
                        | ir::ExprKind::Match { .. }
                );
                if needs_paren {
                    self.out.push('(');
                }
                self.expr(expr, 10, indent);
                if needs_paren {
                    self.out.push(')');
                }
                self.out.push('?');
            }
            ir::ExprKind::UnsafeBlock { block } => {
                self.out.push_str("unsafe ");
                self.block(block, indent);
            }
            ir::ExprKind::StructInit { name, fields } => {
                if name == TUPLE_INTERNAL_NAME {
                    let mut indexed = fields
                        .iter()
                        .filter_map(|(field, expr, _)| {
                            field.parse::<usize>().ok().map(|idx| (idx, expr))
                        })
                        .collect::<Vec<_>>();
                    indexed.sort_by_key(|(idx, _)| *idx);
                    self.out.push('(');
                    for (idx, (_field_idx, expr)) in indexed.iter().enumerate() {
                        if idx > 0 {
                            self.out.push_str(", ");
                        }
                        self.expr(expr, 0, indent);
                    }
                    if indexed.len() == 1 {
                        self.out.push(',');
                    }
                    self.out.push(')');
                } else {
                    self.out.push_str(name);
                    self.out.push_str(" {");
                    for (idx, (field, expr, _)) in fields.iter().enumerate() {
                        if idx > 0 {
                            self.out.push_str(", ");
                        }
                        self.out.push_str(field);
                        self.out.push_str(": ");
                        self.expr(expr, 0, indent);
                    }
                    self.out.push('}');
                }
            }
            ir::ExprKind::FieldAccess { base, field } => {
                self.expr(base, PREFIX_OPERAND_PREC, indent);
                self.out.push('.');
                self.out.push_str(field);
            }
        }
    }

    fn template_literal(&mut self, template: &str, args: &[ir::Expr], indent: usize) {
        self.out.push('f');
        self.out.push('"');
        let mut cursor = 0usize;
        for (idx, arg) in args.iter().enumerate() {
            let placeholder = format!("{{{idx}}}");
            let Some(rel) = template[cursor..].find(&placeholder) else {
                break;
            };
            let split_at = cursor + rel;
            escape_template_literal_segment_into(&mut self.out, &template[cursor..split_at]);
            self.out.push('{');
            self.expr(arg, 0, indent);
            self.out.push('}');
            cursor = split_at + placeholder.len();
        }
        escape_template_literal_segment_into(&mut self.out, &template[cursor..]);
        self.out.push('"');
    }

    fn pattern(&mut self, pattern: &ir::Pattern) {
        match &pattern.kind {
            ir::PatternKind::Wildcard => self.out.push('_'),
            ir::PatternKind::Var(v) => self.out.push_str(v),
            ir::PatternKind::Int(v, ..) => {
                let value = *v;
                if let Some(text) = self.literal_text(
                    pattern.span,
                    |kind| matches!(kind, TokenKind::Int(token) if token.value == value),
                ) {
                    self.out.push_str(text);
                } else if let Some(meta) = pattern.int_literal_metadata() {
                    self.out.push_str(&meta.raw_literal_text);
                    self.out.push_str(&meta.suffix_text);
                } else {
                    self.out.push_str(&v.to_string());
                }
            }
            ir::PatternKind::Char(v) => {
                let value = *v;
                if let Some(text) = self.literal_text(
                    pattern.span,
                    |kind| matches!(kind, TokenKind::Char(c) if *c == value),
                ) {
                    self.out.push_str(text);
                } else {
                    self.out.push_str(&format!("{:?}", v));
                }
            }
            ir::PatternKind::String(v) => {
                if let Some(text) = self.literal_text(
                    pattern.span,
                    |kind| matches!(kind, TokenKind::String(s) if s == v),
                ) {
                    self.out.push_str(text);
                } else {
                    self.out.push('"');
                    self.out.push_str(&escape_aic_string_literal(v));
                    self.out.push('"');
                }
            }
            ir::PatternKind::Bool(v) => self.out.push_str(if *v { "true" } else { "false" }),
            ir::PatternKind::Unit => self.out.push_str("()"),
            ir::PatternKind::Or { patterns } => {
                for (idx, part) in patterns.iter().enumerate() {
                    if idx > 0 {
                        self.out.push_str(" | ");
                    }
                    self.pattern(part);
                }
            }
            ir::PatternKind::Variant { name, args } => {
                if name == TUPLE_INTERNAL_NAME {
                    self.out.push('(');
                    for (idx, arg) in args.iter().enumerate() {
                        if idx > 0 {
                            self.out.push_str(", ");
                        }
                        self.pattern(arg);
                    }
                    if args.len() == 1 {
                        self.out.push(',');
                    }
                    self.out.push(')');
                } else {
                    self.out.push_str(name);
                    if !args.is_empty() {
                        self.out.push('(');
                        for (idx, arg) in args.iter().enumerate() {
                            if idx > 0 {
                                self.out.push_str(", ");
                            }
                            self.pattern(arg);
                        }
                        self.out.push(')');
                    }
                }
            }
            ir::PatternKind::Struct {
                name,
                fields,
                has_rest,
            } => {
                self.out.push_str(name);
                self.out.push_str(" { ");
                for (idx, field) in fields.iter().enumerate() {
                    if idx > 0 {
                        self.out.push_str(", ");
                    }
                    if matches!(&field.pattern.kind, ir::PatternKind::Var(binding) if binding == &field.name)
                    {
                        self.out.push_str(&field.name);
                    } else {
                        self.out.push_str(&field.name);
                        self.out.push_str(": ");
                        self.pattern(&field.pattern);
                    }
                }
                if *has_rest {
                    if !fields.is_empty() {
                        self.out.push_str(", ");
                    }
                    self.out.push_str("..");
                }
                self.out.push_str(" }");
            }
        }
    }
}

/// Where a function's signature ends: its body, or the first block nested
/// in a contract. Comments before this point are printed above the header.
fn signature_anchor(f: &ir::Function) -> usize {
    f.requires
        .iter()
        .chain(f.ensures.iter())
        .filter_map(first_nested_region)
        .chain(std::iter::once(f.body.span.start))
        .min()
        .unwrap_or(f.body.span.start)
}

fn stmt_nested_regions(stmt: &ir::Stmt) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    match stmt {
        ir::Stmt::Let { expr, .. }
        | ir::Stmt::Assign { expr, .. }
        | ir::Stmt::Expr { expr, .. }
        | ir::Stmt::Assert { expr, .. } => nested_regions(expr, &mut regions),
        ir::Stmt::Return { expr, .. } => {
            if let Some(expr) = expr {
                nested_regions(expr, &mut regions);
            }
        }
    }
    regions
}

/// Source ranges inside `expr` that the printer lays out over several lines
/// with their own comment slots: blocks, and `match` bodies (from the end of
/// the scrutinee). Each range is exactly what the nested printer consumes
/// comments from. Nested ranges are not listed separately.
fn nested_regions(expr: &ir::Expr, out: &mut Vec<(usize, usize)>) {
    let block_region = |block: &ir::Block| (block.span.start, block.span.end);
    if let Some(rendered) = extract_for_syntax(expr) {
        match &rendered.iterable {
            RenderedIterable::Expr(iterable) => nested_regions(iterable, out),
            RenderedIterable::Range { start, end } => {
                nested_regions(start, out);
                nested_regions(end, out);
            }
        }
        out.push(block_region(&rendered.body));
        return;
    }
    match &expr.kind {
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            nested_regions(cond, out);
            out.push(block_region(then_block));
            match else_if_expr(else_block) {
                Some(nested) => nested_regions(nested, out),
                None => out.push(block_region(else_block)),
            }
        }
        ir::ExprKind::While { cond, body } => {
            nested_regions(cond, out);
            out.push(block_region(body));
        }
        ir::ExprKind::Loop { body } | ir::ExprKind::Closure { body, .. } => {
            out.push(block_region(body));
        }
        ir::ExprKind::UnsafeBlock { block } => out.push(block_region(block)),
        ir::ExprKind::Match {
            expr: scrutinee, ..
        } => {
            nested_regions(scrutinee, out);
            out.push((scrutinee.span.end, expr.span.end));
        }
        ir::ExprKind::Call { callee, args, .. } => {
            nested_regions(callee, out);
            args.iter().for_each(|arg| nested_regions(arg, out));
        }
        ir::ExprKind::TemplateLiteral { args, .. } => {
            args.iter().for_each(|arg| nested_regions(arg, out));
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            nested_regions(lhs, out);
            nested_regions(rhs, out);
        }
        ir::ExprKind::Unary { expr, .. }
        | ir::ExprKind::Borrow { expr, .. }
        | ir::ExprKind::Await { expr }
        | ir::ExprKind::Try { expr } => nested_regions(expr, out),
        ir::ExprKind::Break { expr } | ir::ExprKind::Return { expr } => {
            if let Some(expr) = expr {
                nested_regions(expr, out);
            }
        }
        ir::ExprKind::StructInit { fields, .. } => {
            fields
                .iter()
                .for_each(|(_, expr, _)| nested_regions(expr, out));
        }
        ir::ExprKind::FieldAccess { base, .. } => nested_regions(base, out),
        ir::ExprKind::Int(..)
        | ir::ExprKind::Float(..)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::Char(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit
        | ir::ExprKind::Var(_)
        | ir::ExprKind::Continue => {}
    }
}

/// Start of the first part of `expr` that the printer lays out over several
/// lines with its own comment slots (a block or a `match`). Comments before
/// it stay above the line that holds `expr`; later ones go inside.
fn first_nested_region(expr: &ir::Expr) -> Option<usize> {
    if let Some(rendered) = extract_for_syntax(expr) {
        let iterable = match &rendered.iterable {
            RenderedIterable::Expr(iterable) => first_nested_region(iterable),
            RenderedIterable::Range { start, end } => {
                first_nested_region(start).or_else(|| first_nested_region(end))
            }
        };
        return Some(iterable.map_or(rendered.body.span.start, |region| {
            region.min(rendered.body.span.start)
        }));
    }
    let children: Vec<&ir::Expr> = match &expr.kind {
        ir::ExprKind::If {
            cond, then_block, ..
        } => {
            let start = then_block.span.start;
            return Some(first_nested_region(cond).map_or(start, |region| region.min(start)));
        }
        ir::ExprKind::While { cond, body } => {
            let start = body.span.start;
            return Some(first_nested_region(cond).map_or(start, |region| region.min(start)));
        }
        ir::ExprKind::Loop { body } => return Some(body.span.start),
        ir::ExprKind::Closure { body, .. } => return Some(body.span.start),
        ir::ExprKind::UnsafeBlock { block } => return Some(block.span.start),
        ir::ExprKind::Match { .. } => return Some(expr.span.start),
        ir::ExprKind::Call { callee, args, .. } => {
            std::iter::once(&**callee).chain(args.iter()).collect()
        }
        ir::ExprKind::TemplateLiteral { args, .. } => args.iter().collect(),
        ir::ExprKind::Binary { lhs, rhs, .. } => vec![lhs, rhs],
        ir::ExprKind::Unary { expr, .. }
        | ir::ExprKind::Borrow { expr, .. }
        | ir::ExprKind::Await { expr }
        | ir::ExprKind::Try { expr } => vec![expr],
        ir::ExprKind::Break { expr } | ir::ExprKind::Return { expr } => {
            expr.iter().map(|expr| &**expr).collect()
        }
        ir::ExprKind::StructInit { fields, .. } => fields.iter().map(|(_, expr, _)| expr).collect(),
        ir::ExprKind::FieldAccess { base, .. } => vec![base],
        ir::ExprKind::Int(..)
        | ir::ExprKind::Float(..)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::Char(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit
        | ir::ExprKind::Var(_)
        | ir::ExprKind::Continue => Vec::new(),
    };
    children.into_iter().filter_map(first_nested_region).min()
}

/// The nested `if` of an `else if` chain. The parser wraps it in a block
/// with the same span; an explicit `else { if .. }` block has its own span.
fn else_if_expr(else_block: &ir::Block) -> Option<&ir::Expr> {
    let tail = else_block.tail.as_deref()?;
    let is_else_if = else_block.stmts.is_empty()
        && matches!(tail.kind, ir::ExprKind::If { .. })
        && tail.span == else_block.span
        && extract_for_syntax(tail).is_none();
    is_else_if.then_some(tail)
}

#[derive(Debug, Clone)]
enum RenderedIterable {
    Expr(ir::Expr),
    Range { start: ir::Expr, end: ir::Expr },
}

#[derive(Debug, Clone)]
struct RenderedFor {
    binding: String,
    iterable: RenderedIterable,
    body: ir::Block,
}

fn extract_for_syntax(expr: &ir::Expr) -> Option<RenderedFor> {
    let ir::ExprKind::If {
        cond,
        then_block,
        else_block,
    } = &expr.kind
    else {
        return None;
    };
    if !matches!(cond.kind, ir::ExprKind::Bool(true)) || !is_unit_block(else_block) {
        return None;
    }
    let loop_expr = then_block.tail.as_ref()?;
    let ir::ExprKind::Loop { body: loop_body } = &loop_expr.kind else {
        return None;
    };

    match then_block.stmts.len() {
        1 => extract_iter_for(&then_block.stmts, loop_body),
        2 => extract_range_for(&then_block.stmts, loop_body)
            .or_else(|| extract_vec_for(&then_block.stmts, loop_body)),
        _ => None,
    }
}

fn is_var(expr: &ir::Expr, expected: &str) -> bool {
    matches!(&expr.kind, ir::ExprKind::Var(name) if name == expected)
}

fn is_field_of_var(expr: &ir::Expr, var: &str, expected_field: &str) -> bool {
    matches!(&expr.kind, ir::ExprKind::FieldAccess { base, field }
        if field == expected_field && is_var(base, var))
}

/// Single-argument call of the named function.
fn call_arg<'e>(expr: &'e ir::Expr, function: &str) -> Option<&'e ir::Expr> {
    let ir::ExprKind::Call { callee, args, .. } = &expr.kind else {
        return None;
    };
    (is_var(callee, function) && args.len() == 1).then(|| &args[0])
}

/// Recognizes the parser's desugaring of `for x in iterable { .. }`:
/// `let mut it = aic_for_into_iter(iterable); loop { let step =
/// aic_for_next_iter(it); match step.item { Some(x) => if true { it =
/// step.iter; .. } else { () }, None => break }; }`.
fn extract_iter_for(stmts: &[ir::Stmt], loop_body: &ir::Block) -> Option<RenderedFor> {
    let ir::Stmt::Let {
        name: iter_name,
        mutable: true,
        ty: None,
        expr: init,
        ..
    } = &stmts[0]
    else {
        return None;
    };
    let iterable = call_arg(init, "aic_for_into_iter")?;

    if loop_body.stmts.len() != 2 || loop_body.tail.is_some() {
        return None;
    }
    let ir::Stmt::Let {
        name: step_name,
        mutable: false,
        ty: None,
        expr: next,
        ..
    } = &loop_body.stmts[0]
    else {
        return None;
    };
    if !call_arg(next, "aic_for_next_iter").is_some_and(|arg| is_var(arg, iter_name)) {
        return None;
    }
    let ir::Stmt::Expr {
        expr: match_expr, ..
    } = &loop_body.stmts[1]
    else {
        return None;
    };
    let ir::ExprKind::Match {
        expr: scrutinee,
        arms,
    } = &match_expr.kind
    else {
        return None;
    };
    if !is_field_of_var(scrutinee, step_name, "item") || arms.len() != 2 {
        return None;
    }

    let (arm_some, arm_none) = (&arms[0], &arms[1]);
    let binding = match &arm_some.pattern.kind {
        ir::PatternKind::Variant { name, args } if name == "Some" && args.len() == 1 => {
            match &args[0].kind {
                ir::PatternKind::Var(binding) => binding.clone(),
                _ => return None,
            }
        }
        _ => return None,
    };
    if arm_some.guard.is_some()
        || arm_none.guard.is_some()
        || !matches!(&arm_none.pattern.kind,
            ir::PatternKind::Variant { name, args } if name == "None" && args.is_empty())
        || !is_break_none_expr(&arm_none.body)
    {
        return None;
    }

    let some_then = extract_if_true_then_block(&arm_some.body)?;
    if some_then.tail.is_some() {
        return None;
    }
    match some_then.stmts.first() {
        Some(ir::Stmt::Assign {
            target, path, expr, ..
        }) if path.is_empty()
            && target == iter_name
            && is_field_of_var(expr, step_name, "iter") => {}
        _ => return None,
    }

    Some(RenderedFor {
        binding,
        iterable: RenderedIterable::Expr(iterable.clone()),
        body: ir::Block {
            node: some_then.node,
            stmts: some_then.stmts[1..].to_vec(),
            tail: None,
            span: some_then.span,
        },
    })
}

fn extract_vec_for(stmts: &[ir::Stmt], loop_body: &ir::Block) -> Option<RenderedFor> {
    let (iter_name, iterable_expr) = match &stmts[0] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if !*mutable => (name.as_str(), expr.clone()),
        _ => return None,
    };
    let index_name = match &stmts[1] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if *mutable && matches!(expr.kind, ir::ExprKind::Int(0, ..)) => name.as_str(),
        _ => return None,
    };

    if loop_body.stmts.len() != 1 || loop_body.tail.is_some() {
        return None;
    }
    let ir::Stmt::Expr {
        expr: match_expr, ..
    } = &loop_body.stmts[0]
    else {
        return None;
    };
    let ir::ExprKind::Match {
        expr: scrutinee,
        arms,
    } = &match_expr.kind
    else {
        return None;
    };
    if arms.len() != 2 {
        return None;
    }

    let ir::ExprKind::Call { callee, args, .. } = &scrutinee.kind else {
        return None;
    };
    let ir::ExprKind::Var(callee_name) = &callee.kind else {
        return None;
    };
    if callee_name != "aic_vec_get_intrinsic" || args.len() != 2 {
        return None;
    }
    if !matches!(&args[0].kind, ir::ExprKind::Var(name) if name == iter_name) {
        return None;
    }
    if !matches!(&args[1].kind, ir::ExprKind::Var(name) if name == index_name) {
        return None;
    }

    let arm_some = &arms[0];
    let arm_none = &arms[1];

    let binding = match &arm_some.pattern.kind {
        ir::PatternKind::Variant { name, args } if name == "Some" && args.len() == 1 => {
            match &args[0].kind {
                ir::PatternKind::Var(binding) => binding.clone(),
                _ => return None,
            }
        }
        _ => return None,
    };
    if arm_some.guard.is_some() {
        return None;
    }
    if !matches!(
        arm_none.pattern.kind,
        ir::PatternKind::Variant { ref name, ref args }
            if name == "None" && args.is_empty()
    ) || arm_none.guard.is_some()
        || !is_break_none_expr(&arm_none.body)
    {
        return None;
    }

    let some_then = extract_if_true_then_block(&arm_some.body)?;
    if some_then.stmts.is_empty() || some_then.tail.is_some() {
        return None;
    }
    let ir::Stmt::Assign {
        target, path, expr, ..
    } = &some_then.stmts[0]
    else {
        return None;
    };
    if !path.is_empty() || target != index_name || !is_increment_expr(expr, index_name) {
        return None;
    }

    let body = ir::Block {
        node: some_then.node,
        stmts: some_then.stmts[1..].to_vec(),
        tail: None,
        span: some_then.span,
    };

    Some(RenderedFor {
        binding,
        iterable: RenderedIterable::Expr(iterable_expr),
        body,
    })
}

fn extract_range_for(stmts: &[ir::Stmt], loop_body: &ir::Block) -> Option<RenderedFor> {
    let (cur_name, start_expr) = match &stmts[0] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if *mutable => (name.as_str(), expr.clone()),
        _ => return None,
    };
    let (end_name, end_expr) = match &stmts[1] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if !*mutable => (name.as_str(), expr.clone()),
        _ => return None,
    };

    if loop_body.stmts.len() != 1 || loop_body.tail.is_some() {
        return None;
    }
    let ir::Stmt::Expr { expr: if_expr, .. } = &loop_body.stmts[0] else {
        return None;
    };
    let ir::ExprKind::If {
        cond,
        then_block,
        else_block,
    } = &if_expr.kind
    else {
        return None;
    };

    let ir::ExprKind::Binary { op, lhs, rhs } = &cond.kind else {
        return None;
    };
    if *op != BinOp::Lt {
        return None;
    }
    if !matches!(&lhs.kind, ir::ExprKind::Var(name) if name == cur_name) {
        return None;
    }
    if !matches!(&rhs.kind, ir::ExprKind::Var(name) if name == end_name) {
        return None;
    }
    if !else_block.stmts.is_empty()
        || else_block
            .tail
            .as_ref()
            .is_none_or(|tail| !is_break_none_expr(tail))
    {
        return None;
    }
    if then_block.stmts.len() < 2 || then_block.tail.is_some() {
        return None;
    }

    let binding = match &then_block.stmts[0] {
        ir::Stmt::Let {
            name,
            mutable,
            expr,
            ..
        } if !*mutable && matches!(&expr.kind, ir::ExprKind::Var(source) if source == cur_name) => {
            name.clone()
        }
        _ => return None,
    };
    match &then_block.stmts[1] {
        ir::Stmt::Assign {
            target, path, expr, ..
        } if path.is_empty() && target == cur_name && is_increment_expr(expr, cur_name) => {}
        _ => return None,
    }

    let body = ir::Block {
        node: then_block.node,
        stmts: then_block.stmts[2..].to_vec(),
        tail: None,
        span: then_block.span,
    };

    Some(RenderedFor {
        binding,
        iterable: RenderedIterable::Range {
            start: start_expr,
            end: end_expr,
        },
        body,
    })
}

fn extract_if_true_then_block(expr: &ir::Expr) -> Option<&ir::Block> {
    let ir::ExprKind::If {
        cond,
        then_block,
        else_block,
    } = &expr.kind
    else {
        return None;
    };
    if !matches!(cond.kind, ir::ExprKind::Bool(true)) || !is_unit_block(else_block) {
        return None;
    }
    Some(then_block)
}

fn is_unit_block(block: &ir::Block) -> bool {
    block.stmts.is_empty()
        && block
            .tail
            .as_ref()
            .is_none_or(|tail| matches!(tail.kind, ir::ExprKind::Unit))
}

fn is_break_none_expr(expr: &ir::Expr) -> bool {
    matches!(expr.kind, ir::ExprKind::Break { expr: None })
}

fn is_increment_expr(expr: &ir::Expr, target: &str) -> bool {
    let ir::ExprKind::Binary { op, lhs, rhs } = &expr.kind else {
        return false;
    };
    *op == BinOp::Add
        && matches!(&lhs.kind, ir::ExprKind::Var(name) if name == target)
        && matches!(&rhs.kind, ir::ExprKind::Int(1, ..))
}

fn render_float_literal(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        let mut text = format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    }
}

fn escape_aic_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            '\u{0001}'..='\u{001F}' | '\u{007F}' => {
                let _ = write!(&mut escaped, "\\u{{{:X}}}", ch as u32);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_template_literal_segment_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            '\u{0001}'..='\u{001F}' | '\u{007F}' => {
                let _ = write!(out, "\\u{{{:X}}}", ch as u32);
            }
            _ => out.push(ch),
        }
    }
}

fn type_map(program: &ir::Program) -> BTreeMap<ir::TypeId, String> {
    program
        .types
        .iter()
        .map(|ty| (ty.id, ty.repr.clone()))
        .collect()
}

/// Parent precedence for operands of prefix operators, field access and
/// calls: above every binary operator, so a binary operand keeps its parens.
const PREFIX_OPERAND_PREC: u8 = 11;

fn binop_info(op: BinOp) -> (u8, &'static str) {
    match op {
        BinOp::Or => (1, "||"),
        BinOp::And => (2, "&&"),
        BinOp::BitOr => (3, "|"),
        BinOp::BitXor => (4, "^"),
        BinOp::BitAnd => (5, "&"),
        BinOp::Eq => (6, "=="),
        BinOp::Ne => (6, "!="),
        BinOp::Lt => (7, "<"),
        BinOp::Le => (7, "<="),
        BinOp::Gt => (7, ">"),
        BinOp::Ge => (7, ">="),
        BinOp::Shl => (8, "<<"),
        BinOp::Shr => (8, ">>"),
        BinOp::Ushr => (8, ">>>"),
        BinOp::Add => (9, "+"),
        BinOp::Sub => (9, "-"),
        BinOp::Mul => (10, "*"),
        BinOp::Div => (10, "/"),
        BinOp::Mod => (10, "%"),
    }
}

#[cfg(test)]
mod tests {
    use crate::{ir_builder::build, lexer::lex_with_comments, parser::parse};

    use super::{format_program, format_source, verify_formatted, FormatError};

    /// Formats `src`, checks the result is a fixed point, and returns it.
    fn fmt(src: &str) -> String {
        let once = format_source(src, "test.aic").unwrap_or_else(|err| panic!("{err}"));
        let twice = format_source(&once, "test.aic").unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(once, twice, "formatting is not idempotent");
        once
    }

    #[test]
    fn place_assignments_and_mutable_receivers_round_trip() {
        let src = "struct C {\n    n: Int,\n    t: (Int, Int),\n}\n\nimpl C {\n    fn bump(&mut self, by: Int) -> () {\n        // keep this note\n        self.n = self.n + by;\n        self.t.1 = by;\n    }\n}\n\nfn f(items: Vec[C], i: Int) -> Vec[C] {\n    let mut v = items;\n    v[i + 1].t.0 = 2;\n    v\n}\n";
        let formatted = fmt(src);
        assert_eq!(formatted, src);
        // Without source text the receiver form still survives.
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let from_ir = format_program(&build(&program.expect("program")));
        assert!(
            from_ir.contains("fn bump(&mut self, by: Int) -> ()"),
            "{from_ir}"
        );
        assert!(from_ir.contains("v[i + 1].t.0 = 2;"), "{from_ir}");
        assert!(!from_ir.contains("$place_index"), "{from_ir}");
    }

    fn comment_texts(src: &str) -> Vec<String> {
        let (_, comments, _) = lex_with_comments(src, "test.aic");
        comments.into_iter().map(|comment| comment.text).collect()
    }

    #[test]
    fn return_expressions_round_trip() {
        let src = "fn f(x: Option[Int]) -> Int {\n    let a = match x { Some(v) => v, None => return 0 };\n    if a > 1 { a } else { return }\n}\n";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let formatted = format_program(&build(&program.expect("program")));
        assert!(formatted.contains("None => return 0"), "{formatted}");
        assert!(
            formatted.contains("return\n") || formatted.contains("return }"),
            "{formatted}"
        );
        let (reparsed, diagnostics) = parse(&formatted, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}\n{formatted}");
        assert_eq!(
            format_program(&build(&reparsed.expect("program"))),
            formatted
        );
    }

    #[test]
    fn deterministic_formatting() {
        let src = "import std.io;\nfn main()->Int effects{io}{print_int(1);0}";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let ir = build(&program.expect("program"));
        let a = format_program(&ir);
        let b = format_program(&ir);
        assert_eq!(a, b);
    }

    #[test]
    fn canonicalizes_uint_alias_to_usize_in_formatted_types() {
        let src = r#"
type Counter = UInt;

fn bump(index: USize, delta: UInt, signed: ISize) -> UInt {
    let _index: USize = index;
    let _delta: UInt = delta;
    let _signed: ISize = signed;
    delta
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted = format_program(&ir);
        assert!(formatted.contains("ISize"), "formatted={formatted}");
        assert!(formatted.contains("USize"), "formatted={formatted}");
        assert!(!formatted.contains("UInt"), "formatted={formatted}");
    }

    #[test]
    fn canonicalizes_float_alias_to_float64_in_formatted_types() {
        let src = r#"
type Scalar = Float;

fn choose(a: Float32, b: Float64, c: Float) -> Float {
    let _alias: Float = c;
    b
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted = format_program(&ir);
        assert!(formatted.contains("Float32"), "formatted={formatted}");
        assert!(formatted.contains("Float64"), "formatted={formatted}");
        assert!(!formatted.contains("= Float;"), "formatted={formatted}");
        assert!(!formatted.contains(": Float ="), "formatted={formatted}");
        assert!(!formatted.contains("-> Float {"), "formatted={formatted}");
    }

    #[test]
    fn preserves_typed_float_suffixes_in_formatted_output() {
        let src = r#"
fn main() -> Float {
    let a: Float32 = 1.25f32;
    let b: Float64 = 2.5f64;
    a + b
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted_once = format_program(&ir);
        assert!(
            formatted_once.contains("1.25f32"),
            "formatted={formatted_once}"
        );
        assert!(
            formatted_once.contains("2.5f64"),
            "formatted={formatted_once}"
        );
        assert!(
            formatted_once.contains("-> Float64"),
            "formatted={formatted_once}"
        );

        let (reparsed, reparsed_diags) = parse(&formatted_once, "formatted.aic");
        assert!(
            reparsed_diags.is_empty(),
            "reparse diagnostics={reparsed_diags:#?}\nformatted={formatted_once}"
        );
        let reformatted = format_program(&build(&reparsed.expect("program")));
        assert_eq!(formatted_once, reformatted);
    }

    #[test]
    fn formats_attributes_stably() {
        let src = r#"
#[controller("/users")]
struct UsersController {
    #[required]
    name: String,
}

#[get("/:id")]
async fn show_user(#[path("id")] id: String) -> Int {
    id;
    0
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let formatted_once = format_program(&build(&program.expect("program")));
        assert!(
            formatted_once.contains("#[controller(\"/users\")]"),
            "formatted={formatted_once}"
        );
        assert!(
            formatted_once.contains("#[required]"),
            "formatted={formatted_once}"
        );
        assert!(
            formatted_once.contains("show_user(#[path(\"id\")] id: String)"),
            "formatted={formatted_once}"
        );

        let (reparsed, reparsed_diags) = parse(&formatted_once, "formatted.aic");
        assert!(
            reparsed_diags.is_empty(),
            "reparse diagnostics={reparsed_diags:#?}\nformatted={formatted_once}"
        );
        let reformatted = format_program(&build(&reparsed.expect("program")));
        assert_eq!(formatted_once, reformatted);
    }

    #[test]
    fn formats_nested_if_and_match_with_block_indentation() {
        let src = r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
    Some(x)
} else {
    None()
}
}

fn main() -> Int effects { io } capabilities { io } {
    let v = maybe_even(10);
    let out = match v {
    Some(n) => n,
    None => 0,
};
    print_int(out);
    0
}
"#;
        let expected = r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
        Some(x)
    } else {
        None()
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let v = maybe_even(10);
    let out = match v {
        Some(n) => n,
        None => 0,
    };
    print_int(out);
    0
}
"#;

        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        assert_eq!(format_program(&ir), expected);
    }

    #[test]
    fn string_literal_escaping_is_valid_and_stable_for_exprs_and_patterns() {
        let src = r#"
fn classify(s: String) -> Int {
    match s {
        "\n\t\r\0\u{1}\u{7f}\"\\" => 1,
        _ => 0,
    }
}

fn main() -> Int {
    let value = "\n\t\r\0\u{1}\u{7f}\"\\";
    classify(value)
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted = format_program(&ir);

        assert!(
            formatted.contains("\"\\n\\t\\r\\0\\u{1}\\u{7F}\\\"\\\\\""),
            "formatted={formatted}"
        );

        let (reparsed, reparsed_diags) = parse(&formatted, "formatted.aic");
        assert!(
            reparsed_diags.is_empty(),
            "reparse diagnostics={reparsed_diags:#?}\nformatted={formatted}"
        );
        let reformatted = format_program(&build(&reparsed.expect("program")));
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn unicode_string_and_template_literals_roundtrip_with_control_escapes() {
        let src = r#"
fn main(name: String) -> String {
    let literal = "é🙂\u{10FFFF}\n\t\u{202F}";
    f"hi {name} {literal} é🙂\n\t"
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted = format_program(&ir);

        assert!(formatted.contains("é🙂"), "formatted={formatted}");
        assert!(
            formatted.contains("\\u{10FFFF}") || formatted.contains("\u{10FFFF}"),
            "formatted={formatted}"
        );
        assert!(formatted.contains("\\n\\t"), "formatted={formatted}");

        let (reparsed, reparsed_diags) = parse(&formatted, "formatted.aic");
        assert!(
            reparsed_diags.is_empty(),
            "reparse diagnostics={reparsed_diags:#?}\nformatted={formatted}"
        );
        let reformatted = format_program(&build(&reparsed.expect("program")));
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn template_literal_formatting_is_valid_and_stable() {
        let src = r#"
fn main(name: String) -> String {
    f"left \{literal\} {name} right"
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let ir = build(&program.expect("program"));
        let formatted = format_program(&ir);

        assert!(
            formatted.contains("f\"left {{literal}} {name} right\""),
            "formatted={formatted}"
        );

        let (reparsed, reparsed_diags) = parse(&formatted, "formatted.aic");
        assert!(
            reparsed_diags.is_empty(),
            "reparse diagnostics={reparsed_diags:#?}\nformatted={formatted}"
        );
        let reformatted = format_program(&build(&reparsed.expect("program")));
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn keeps_comment_before_first_item() {
        let src = "// keep me\nfn plus_one(x: Int) -> Int {\n    x + 1\n}\n\nfn main() -> Int {\n    plus_one(1)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn typed_holes_print_as_underscore() {
        let src =
            "struct Box {\n    v: Vec[_],\n}\n\nfn plus_one(x: _) -> _ { let y: _ = x + 1; y }\n";
        let expected = "struct Box {\n    v: Vec[_],\n}\n\nfn plus_one(x: _) -> _ {\n    let y: _ = x + 1;\n    y\n}\n";
        assert_eq!(fmt(src), expected);

        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let printed = format_program(&build(&program.expect("program")));
        assert!(!printed.contains("<?>"), "printed={printed}");
        assert_eq!(printed, expected);
    }

    #[test]
    fn keeps_comments_around_and_inside_items() {
        let src = r#"// file header

// second paragraph
module demo.comments; // module note

/// Doc comment.
#[derive_like]
// between attribute and item
pub struct Point { // open brace note
    // leading x
    pub x: Int, // trailing x

    y: Int = 0x10, // hex default
    // dangling field comment
}

enum Shape {
    // leading circle
    Circle(Int), // trailing circle
    Square,
    // dangling variant comment
}

trait Area {
    // method doc
    fn area(self: Self) -> Int; // trailing signature
}

trait Marker {
    // no methods yet
}

impl Point {
    // constructor
    fn new(x: Int) -> Point {
        Point {x: x, y: 0}
    }

    fn get(&self) -> Int { // body brace note
        self.x
    }
    // dangling method comment
}
// end of file
"#;
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn keeps_comments_in_statements_and_match_arms() {
        let src = r#"fn classify(n: Int) -> Int {
    // leading match
    let r = match n { // match brace note
        // arm leading
        0 => 10, // arm trailing
        1 => 0xFF,
        // guarded arm
        _ => if n > 5 {
            // inside arm block
            n
        } else {
            0
        },
        // dangling arm comment
    }; // statement trailing

    // after a blank line
    let (a, _) = (1, 2); // tuple let
    for i in 0..3 {
        // range body
        r = r + i;
    };
    if a == 1 {
        r
    } else if a == 2 {
        // else-if body
        r + 1
    } else {
        r + 2
    } // tail trailing
    // dangling block comment
}
"#;
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn import_comments_move_with_sorted_imports() {
        let src = "// about io\nimport std.io; // io note\n// about bytes\nimport std.bytes;\n\nfn main() -> Int {\n    0\n}\n";
        let expected = "// about bytes\nimport std.bytes;\n// about io\nimport std.io; // io note\n\nfn main() -> Int {\n    0\n}\n";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn comments_inside_collapsed_lines_move_above_them() {
        let src = r#"fn f(x: Int, // after x
    y: Int) -> Int {
    let z = add(x, // inside call
        y);
    let w = if x > 0 // in condition
    {
        1
    } else {
        2
    };
    z + w
}
"#;
        let expected = r#"// after x
fn f(x: Int, y: Int) -> Int {
    // inside call
    let z = add(x, y);
    // in condition
    let w = if x > 0 {
        1
    } else {
        2
    };
    z + w
}
"#;
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn collapses_blank_lines_and_keeps_comment_paragraphs() {
        let src = "fn main() -> Int {\n    let a = 1;\n\n\n    // first\n\n    // second\n    let b = 2;\n    a + b\n}\n";
        let expected = "fn main() -> Int {\n    let a = 1;\n\n    // first\n\n    // second\n    let b = 2;\n    a + b\n}\n";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn keeps_comment_in_empty_block_and_at_end_of_file() {
        let src = "fn empty() -> () {\n    // nothing here\n}\n\nfn last() -> Int { 0 } // last note\n// no newline at end";
        let expected = "fn empty() -> () {\n    // nothing here\n}\n\nfn last() -> Int {\n    0\n} // last note\n// no newline at end\n";
        assert_eq!(fmt(src), expected);
    }

    #[test]
    fn comment_like_text_in_strings_is_not_a_comment() {
        let src = "fn main() -> String {\n    let url = \"http://example.com\"; // real comment\n    url\n}\n";
        let formatted = fmt(src);
        assert_eq!(formatted, src);
        assert_eq!(comment_texts(&formatted), vec!["// real comment"]);
    }

    #[test]
    fn handles_crlf_line_endings() {
        let src = "// crlf\r\nfn main() -> Int {\r\n    // inner\r\n    0 // tail\r\n}\r\n";
        assert_eq!(
            fmt(src),
            "// crlf\nfn main() -> Int {\n    // inner\n    0 // tail\n}\n"
        );
    }

    #[test]
    fn preserves_syntax_the_ir_lowers_away() {
        let src = r#"pub(crate) struct Config {
    pub host: String = "0.0.0.0",
    port: Int = 8080,
}

pub trait Fetch {
    async fn get(self: Self) -> Int;
}

impl Config {
    pub async fn port(&mut self) -> Int {
        let mask = 0xFF;
        let (left, (_, right)) = (1, (2, 3));
        for value in range(0, 4) {
            left = left + value;
        };
        let banner = $"port={mask} \u{301}";
        let plain = f"left \{literal\} right";
        mask
    }
}
"#;
        let formatted = fmt(src);
        assert_eq!(formatted, src);
        assert!(
            !formatted.contains("Config::default"),
            "formatted={formatted}"
        );
    }

    #[test]
    fn format_program_without_source_prints_lowered_syntax() {
        let src = "pub struct P {\n    pub x: Int = 1,\n}\n\nfn f(t: (Int, Int)) -> Int {\n    let (_, a) = t;\n    for v in range(0, 2) {\n        a = a + v;\n    };\n    if a == 1 {\n        1\n    } else if a == 2 {\n        2\n    } else {\n        3\n    }\n}\n";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let printed = format_program(&build(&program.expect("program")));
        assert_eq!(printed, src);
    }

    #[test]
    fn format_source_reports_parse_errors() {
        match format_source("fn main( -> Int { 0 }", "test.aic") {
            Err(FormatError::Diagnostics(diagnostics)) => assert!(!diagnostics.is_empty()),
            other => panic!("expected diagnostics, got {other:?}"),
        }
    }

    #[test]
    fn verification_rejects_lost_comments_changed_programs_and_bad_syntax() {
        let src = "// note\nfn main() -> Int {\n    1\n}\n";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let program = program.expect("program");
        let (_, comments, _) = lex_with_comments(src, "test.aic");

        assert!(verify_formatted(&program, src, &comments, src, "test.aic").is_ok());
        let reworded = "// note\nfn main() -> Int {\n    1\n}\n// extra\n";
        let without_comment = "fn main() -> Int {\n    1\n}\n";
        let changed = "// note\nfn main() -> Int {\n    2\n}\n";
        let made_public = "// note\npub fn main() -> Int {\n    1\n}\n";
        let broken = "// note\nfn main() -> Int {\n";
        for bad in [reworded, without_comment, changed, made_public, broken] {
            match verify_formatted(&program, src, &comments, bad, "test.aic") {
                Err(FormatError::Verification(_)) => {}
                other => panic!("expected verification failure for {bad:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn verification_rejects_respelled_literals() {
        let src = "fn main() -> Int {\n    0x10\n}\n";
        let (program, _) = parse(src, "test.aic");
        let program = program.expect("program");
        let respelled = "fn main() -> Int {\n    16\n}\n";
        match verify_formatted(&program, src, &[], respelled, "test.aic") {
            Err(FormatError::Verification(message)) => {
                assert!(message.contains("0x10"), "{message}")
            }
            other => panic!("expected verification failure, got {other:?}"),
        }
    }

    #[test]
    fn comment_between_nested_blocks_moves_above_statement() {
        let src = "fn main() -> Int {\n    let r = pair(if a { 1 } else { 2 }, // second arg\n        |x| -> Int { x + 1 });\n    r\n}\n";
        let out = fmt(src);
        assert_eq!(
            out,
            "fn main() -> Int {\n    // second arg\n    let r = pair(if a {\n        1\n    } else {\n        2\n    }, |x| -> Int {\n        x + 1\n    });\n    r\n}\n"
        );
    }

    #[test]
    fn hoisted_comment_gets_no_blank_line_from_code_above_it() {
        let src = "fn main() -> Int {\n    let a = 1;\n    let z = add(\n        a,\n\n        // c\n        a);\n    z\n}\n";
        let out = fmt(src);
        assert_eq!(
            out,
            "fn main() -> Int {\n    let a = 1;\n    // c\n    let z = add(a, a);\n    z\n}\n"
        );
    }

    #[test]
    fn attribute_literals_keep_source_spelling() {
        let src = "#[limit(0xFF, name = \"\\u{301}\")]\nfn main() -> Int {\n    0\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn prefix_and_postfix_operands_keep_parentheses() {
        let src = "fn f(a: Int, b: Int) -> Int {\n    let n = -(a + b);\n    let r = &(a * b);\n    let v = (a * b).v;\n    (a + b).len()\n}\n";
        assert_eq!(fmt(src), src);
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        assert_eq!(format_program(&build(&program.expect("program"))), src);
    }

    #[test]
    fn verification_accepts_documented_canonicalizations() {
        let src = "import std.io;\nimport std.bytes;\n\nfn keep[T](value: T) -> T where T: Eq {\n    value\n}\n\nfn main(n: UInt) -> Float effects { io, io } {\n    1.0\n}\n";
        let formatted = fmt(src);
        assert!(
            formatted.contains("import std.bytes;\nimport std.io;"),
            "{formatted}"
        );
        assert!(
            formatted.contains("fn keep[T: Eq](value: T) -> T {"),
            "{formatted}"
        );
        assert!(
            formatted.contains("fn main(n: USize) -> Float64 effects { io } {"),
            "{formatted}"
        );
    }
}
