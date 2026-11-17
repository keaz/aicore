use std::cell::Cell;

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::lexer::{lex, FloatLiteralSuffixToken, IntLiteralSuffixToken, Token, TokenKind};
use crate::span::Span;

const TUPLE_INTERNAL_NAME: &str = "Tuple";
const TUPLE_LET_TMP_PREFIX: &str = "__tuple_let_";
/// Prefix of the temporary of a destructured struct pattern in `let`; the
/// struct name follows `__` (`__struct_let_0__Point`).
pub const STRUCT_LET_TMP_PREFIX: &str = "__struct_let_";

/// The operator of an assignment token: `Some(None)` for `=`,
/// `Some(Some(op))` for a compound operator, `None` for anything else.
fn assignment_operator(kind: &TokenKind) -> Option<Option<BinOp>> {
    let op = match kind {
        TokenKind::Eq => return Some(None),
        TokenKind::PlusEq => BinOp::Add,
        TokenKind::MinusEq => BinOp::Sub,
        TokenKind::StarEq => BinOp::Mul,
        TokenKind::SlashEq => BinOp::Div,
        TokenKind::PercentEq => BinOp::Mod,
        TokenKind::AmpEq => BinOp::BitAnd,
        TokenKind::PipeEq => BinOp::BitOr,
        TokenKind::CaretEq => BinOp::BitXor,
        TokenKind::LShiftEq => BinOp::Shl,
        TokenKind::RShiftEq => BinOp::Shr,
        TokenKind::URShiftEq => BinOp::Ushr,
        _ => return None,
    };
    Some(Some(op))
}

/// The first sub-pattern that does not match every value: a literal, a
/// variant, or an or-pattern. Tuples and structs are irrefutable when their
/// parts are.
fn first_refutable_pattern(pattern: &Pattern) -> Option<&Pattern> {
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Var(_) => None,
        PatternKind::Variant { name, args } if name == TUPLE_INTERNAL_NAME => {
            args.iter().find_map(first_refutable_pattern)
        }
        PatternKind::Struct { fields, .. } => fields
            .iter()
            .find_map(|field| first_refutable_pattern(&field.pattern)),
        PatternKind::Binding { pattern, .. } => first_refutable_pattern(pattern),
        _ => Some(pattern),
    }
}

/// Names bound by `pattern`, in source order.
fn collect_pattern_binding_names(pattern: &Pattern, out: &mut Vec<String>) {
    match &pattern.kind {
        PatternKind::Var(name) => out.push(name.clone()),
        PatternKind::Variant { args, .. } => {
            for arg in args {
                collect_pattern_binding_names(arg, out);
            }
        }
        PatternKind::Struct { fields, .. } => {
            for field in fields {
                collect_pattern_binding_names(&field.pattern, out);
            }
        }
        PatternKind::Or { patterns } => {
            if let Some(first) = patterns.first() {
                collect_pattern_binding_names(first, out);
            }
        }
        PatternKind::Binding { name, pattern } => {
            out.push(name.clone());
            collect_pattern_binding_names(pattern, out);
        }
        _ => {}
    }
}

/// Source text of a type: `Int64`, `Option[UInt8]`.
fn type_repr_text(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Named { name, args } if args.is_empty() => name.clone(),
        TypeKind::Named { name, args } => format!(
            "{}[{}]",
            name,
            args.iter()
                .map(type_repr_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeKind::Unit => "()".to_string(),
        TypeKind::DynTrait { trait_name } => format!("dyn {trait_name}"),
        TypeKind::Hole => "_".to_string(),
    }
}

/// `(start, end, inclusive)` of a bounded range expression (`a..b`,
/// `a..=b`); any other expression is handed back.
fn split_range_call(expr: Expr) -> Result<(Expr, Expr, bool), Expr> {
    let range_kind = |callee: &Expr| match &callee.kind {
        ExprKind::Var(name) if name == RANGE_BUILTIN => Some(false),
        ExprKind::Var(name) if name == RANGE_INCLUSIVE_BUILTIN => Some(true),
        _ => None,
    };
    let span = expr.span;
    match expr.kind {
        ExprKind::Call {
            callee,
            args,
            arg_names,
            type_args,
        } => match (range_kind(&callee), <[Expr; 2]>::try_from(args)) {
            (Some(inclusive), Ok([lo, hi])) => Ok((lo, hi, inclusive)),
            (_, Ok(args)) => Err(Expr {
                kind: ExprKind::Call {
                    callee,
                    args: args.into(),
                    arg_names,
                    type_args,
                },
                span,
            }),
            (_, Err(args)) => Err(Expr {
                kind: ExprKind::Call {
                    callee,
                    args,
                    arg_names,
                    type_args,
                },
                span,
            }),
        },
        kind => Err(Expr { kind, span }),
    }
}

/// Puts `label` on the loop of a parsed loop form: `while` and `loop`
/// directly, and the loop inside the expression a `for` desugars to.
fn set_loop_label(expr: &mut Expr, label: String) -> bool {
    match &mut expr.kind {
        ExprKind::While { label: slot, .. } | ExprKind::Loop { label: slot, .. } => {
            *slot = Some(label);
            true
        }
        // A `for` loop desugars to `if true { let ..; loop { .. } }`.
        ExprKind::If {
            then_block: block, ..
        }
        | ExprKind::Block { block } => {
            for stmt in &mut block.stmts {
                if let Stmt::Expr { expr, .. } = stmt {
                    if matches!(expr.kind, ExprKind::While { .. } | ExprKind::Loop { .. }) {
                        return set_loop_label(expr, label);
                    }
                }
            }
            match block.tail.as_deref_mut() {
                Some(tail)
                    if matches!(tail.kind, ExprKind::While { .. } | ExprKind::Loop { .. }) =>
                {
                    set_loop_label(tail, label)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Expressions that end in a block: they can be statements without `;`,
/// and a match arm with such a body needs no trailing comma.
fn is_block_like(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::While { .. }
            | ExprKind::Loop { .. }
            | ExprKind::Block { .. }
            | ExprKind::UnsafeBlock { .. }
    )
}

/// Splits the float token of a nested tuple index (`t.0.1` lexes `0.1` as
/// one float) into its two element indexes.
fn split_nested_tuple_index(literal: &crate::lexer::FloatLiteralToken) -> Option<(String, String)> {
    if literal.suffix.is_some() {
        return None;
    }
    let (first, second) = literal.raw_text.split_once('.')?;
    let is_index = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !is_index(first) || !is_index(second) {
        return None;
    }
    let normalize = |part: &str| part.parse::<u64>().ok().map(|value| value.to_string());
    Some((normalize(first)?, normalize(second)?))
}

pub fn parse(source: &str, file: &str) -> (Option<Program>, Vec<Diagnostic>) {
    let (program, diagnostics, _stats) = parse_with_stats(source, file);
    (program, diagnostics)
}

/// Token-proportional parser step budget: every token may be inspected this many
/// times on average before the parser is considered stuck.
pub const PARSER_STEPS_PER_TOKEN: usize = 128;
/// Constant headroom added to the parser step budget for tiny inputs.
pub const PARSER_STEP_BUDGET_BASE: usize = 4096;

/// Work counters for one parse, used by termination regression tests and fuzzing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseStats {
    /// Number of tokens produced by the lexer, including the trailing EOF token.
    pub tokens: usize,
    /// Number of token inspections the parser performed.
    pub steps: usize,
    /// Maximum number of token inspections allowed for this input.
    pub budget: usize,
    /// True when the parser hit the budget and stopped early with `E1099`.
    pub budget_exhausted: bool,
}

pub fn parser_step_budget(token_count: usize) -> usize {
    token_count
        .saturating_mul(PARSER_STEPS_PER_TOKEN)
        .saturating_add(PARSER_STEP_BUDGET_BASE)
}

pub fn parse_with_stats(
    source: &str,
    file: &str,
) -> (Option<Program>, Vec<Diagnostic>, ParseStats) {
    parse_with_step_budget(source, file, None)
}

fn parse_with_step_budget(
    source: &str,
    file: &str,
    step_budget: Option<usize>,
) -> (Option<Program>, Vec<Diagnostic>, ParseStats) {
    let (tokens, mut diagnostics) = lex(source, file);
    let mut parser = Parser::new(file, tokens);
    if let Some(step_budget) = step_budget {
        parser.step_budget = step_budget;
    }
    let program = parser.parse_program();
    let stats = parser.stats();
    if stats.budget_exhausted {
        parser.diagnostics.push(
            Diagnostic::error(
                "E1099",
                format!(
                    "parser stopped after {} steps without making progress ({} tokens)",
                    stats.steps, stats.tokens
                ),
                file,
                parser.tokens[parser.stuck_at.get().min(parser.tokens.len() - 1)].span,
            )
            .with_help(
                "this is a compiler bug; report the input and simplify the code near this location",
            ),
        );
    }
    diagnostics.extend(parser.diagnostics);
    (program, diagnostics, stats)
}

struct Parser<'a> {
    file: &'a str,
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
    for_counter: usize,
    tuple_binding_counter: usize,
    /// Items synthesized while parsing another item (the payload structs of
    /// struct-like enum variants); emitted right after that item.
    pending_items: Vec<Item>,
    disallow_struct_literal: bool,
    /// Set while parsing an expression that a block follows (`if` and
    /// `while` conditions, `match` scrutinees, contracts): a `{` there
    /// starts that block, not a block expression operand.
    disallow_block_expr: bool,
    // Termination guard: every token inspection counts as one step. Once the
    // token-proportional budget is spent the parser only sees EOF, so every
    // parsing loop unwinds and `E1099` is reported instead of hanging.
    steps: Cell<usize>,
    step_budget: usize,
    stuck_at: Cell<usize>,
}

enum ReturnInBlock {
    Stmt(Stmt),
    Tail(Expr),
}

#[derive(Debug, Clone)]
enum TupleLetPattern {
    Binding {
        name: String,
        span: Span,
    },
    Wildcard,
    Tuple {
        items: Vec<TupleLetPattern>,
    },
    /// `Point { x, y: py, .. }`: each field is projected by name.
    Struct {
        name: String,
        fields: Vec<(String, TupleLetPattern)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttributeTarget {
    Function,
    Struct,
    Field,
    Enum,
    Variant,
    Trait,
    Impl,
    Param,
    TypeAlias,
    Const,
}

impl<'a> Parser<'a> {
    fn new(file: &'a str, tokens: Vec<Token>) -> Self {
        let step_budget = parser_step_budget(tokens.len());
        Parser {
            file,
            tokens,
            index: 0,
            diagnostics: Vec::new(),
            for_counter: 0,
            tuple_binding_counter: 0,
            pending_items: Vec::new(),
            disallow_struct_literal: false,
            disallow_block_expr: false,
            steps: Cell::new(0),
            step_budget,
            stuck_at: Cell::new(0),
        }
    }

    fn stats(&self) -> ParseStats {
        ParseStats {
            tokens: self.tokens.len(),
            steps: self.steps.get(),
            budget: self.step_budget,
            budget_exhausted: self.budget_exhausted(),
        }
    }

    fn budget_exhausted(&self) -> bool {
        self.steps.get() > self.step_budget
    }

    fn parse_program(&mut self) -> Option<Program> {
        let start = self.current_span().start;
        let module = if self.at_kind(|k| matches!(k, TokenKind::KwModule)) {
            self.bump();
            let (path, span) = self.parse_path()?;
            self.expect(
                |k| matches!(k, TokenKind::Semi),
                "E1001",
                "expected ';' after module declaration",
            )?;
            Some(ModuleDecl { path, span })
        } else {
            None
        };

        let mut imports = Vec::new();
        while self.at_kind(|k| matches!(k, TokenKind::KwImport)) {
            let start = self.current_span().start;
            self.bump();
            let (path, path_span) = self.parse_path()?;
            self.expect(
                |k| matches!(k, TokenKind::Semi),
                "E1002",
                "expected ';' after import declaration",
            )?;
            imports.push(ImportDecl {
                path,
                span: Span::new(start, path_span.end),
            });
        }

        let mut items = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::Eof)) {
            let item_start = self.index;
            match self.parse_item() {
                Some(item) => {
                    items.push(item);
                    items.append(&mut self.pending_items);
                }
                None => {
                    if self.at_kind(|k| matches!(k, TokenKind::Eof)) {
                        break;
                    }
                    // Recovery must make progress: a failed item that consumed
                    // nothing would otherwise be retried forever.
                    if self.index == item_start {
                        self.bump();
                    }
                    self.recover_item();
                }
            }
        }

        let end = self.current_span().end;
        Some(Program {
            module,
            imports,
            items,
            span: Span::new(start, end),
        })
    }

    fn parse_item(&mut self) -> Option<Item> {
        let attrs = self.parse_attributes();
        let visibility = self.parse_visibility_modifier();
        if self.at_kind(|k| matches!(k, TokenKind::KwExtern)) {
            self.validate_attributes(&attrs, AttributeTarget::Function);
            self.parse_extern_function(attrs, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwIntrinsic)) {
            self.validate_attributes(&attrs, AttributeTarget::Function);
            self.parse_intrinsic_function(attrs, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwType)) {
            self.validate_attributes(&attrs, AttributeTarget::TypeAlias);
            if visibility != Visibility::Private {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1091",
                        "visibility modifiers are not supported on `type` aliases",
                        self.file,
                        self.current_span(),
                    )
                    .with_help("remove the visibility modifier from this `type` alias"),
                );
            }
            self.parse_type_alias(attrs).map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwConst)) {
            self.validate_attributes(&attrs, AttributeTarget::Const);
            if visibility != Visibility::Private {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1091",
                        "visibility modifiers are not supported on `const` items",
                        self.file,
                        self.current_span(),
                    )
                    .with_help("remove the visibility modifier from this `const` item"),
                );
            }
            self.parse_const_item(attrs).map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwUnsafe)) {
            let start = self.current_span().start;
            self.bump();
            if !self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
                self.diagnostics.push(Diagnostic::error(
                    "E1068",
                    "expected `fn` after `unsafe` item modifier",
                    self.file,
                    self.current_span(),
                ));
                return None;
            }
            self.validate_attributes(&attrs, AttributeTarget::Function);
            self.parse_function(attrs, false, true, start, false, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwAsync)) {
            let start = self.current_span().start;
            self.bump();
            if !self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
                self.diagnostics.push(Diagnostic::error(
                    "E1052",
                    "expected `fn` after `async`",
                    self.file,
                    self.current_span(),
                ));
                return None;
            }
            self.validate_attributes(&attrs, AttributeTarget::Function);
            self.parse_function(attrs, true, false, start, false, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
            let start = self.current_span().start;
            self.validate_attributes(&attrs, AttributeTarget::Function);
            self.parse_function(attrs, false, false, start, false, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwStruct)) {
            self.validate_attributes(&attrs, AttributeTarget::Struct);
            self.parse_struct(attrs, visibility).map(Item::Struct)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwEnum)) {
            self.validate_attributes(&attrs, AttributeTarget::Enum);
            self.parse_enum(attrs, visibility).map(Item::Enum)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwTrait)) {
            self.validate_attributes(&attrs, AttributeTarget::Trait);
            self.parse_trait(attrs, visibility).map(Item::Trait)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwImpl)) {
            self.validate_attributes(&attrs, AttributeTarget::Impl);
            self.parse_impl(attrs, visibility).map(Item::Impl)
        } else {
            let span = self.current_span();
            self.diagnostics.push(
                Diagnostic::error(
                    "E1003",
                    "expected item declaration (`fn`, `async fn`, `unsafe fn`, `extern \"C\" fn`, `intrinsic fn`, `type`, `const`, `struct`, `enum`, `trait`, `impl`)",
                    self.file,
                    span,
                )
                .with_help("define functions or types at module scope"),
            );
            None
        }
    }

    fn parse_visibility_modifier(&mut self) -> Visibility {
        if self.at_kind(|k| matches!(k, TokenKind::KwPriv)) {
            self.bump();
            return Visibility::Private;
        }
        if !self.at_kind(|k| matches!(k, TokenKind::KwPub)) {
            return Visibility::Private;
        }

        self.bump();
        if !self.at_kind(|k| matches!(k, TokenKind::LParen)) {
            return Visibility::Public;
        }

        let marker_span = self.current_span();
        self.bump(); // (
        if !self.at_kind(|k| matches!(k, TokenKind::KwCrate)) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1090",
                    "expected `crate` in visibility modifier `pub(crate)`",
                    self.file,
                    marker_span,
                )
                .with_help("use `pub` or `pub(crate)`"),
            );
            while !self.at_kind(|k| matches!(k, TokenKind::RParen | TokenKind::Eof)) {
                self.bump();
            }
            let _ = self.expect(
                |k| matches!(k, TokenKind::RParen),
                "E1090",
                "expected ')' after visibility modifier",
            );
            return Visibility::Public;
        }

        self.bump(); // crate
        let _ = self.expect(
            |k| matches!(k, TokenKind::RParen),
            "E1090",
            "expected ')' after visibility modifier",
        );
        Visibility::Crate
    }

    fn validate_attributes(&mut self, attrs: &[Attribute], target: AttributeTarget) {
        for (index, attr) in attrs.iter().enumerate() {
            if !self.is_known_web_attribute(&attr.name) {
                continue;
            }
            self.validate_web_attribute_target(attr, target);
            self.validate_web_attribute_shape(attr);
            if self.is_singleton_web_attribute(&attr.name) {
                for earlier in &attrs[..index] {
                    if earlier.name == attr.name {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1096",
                                format!("duplicate framework attribute `#[{}]`", attr.name),
                                self.file,
                                attr.span,
                            )
                            .with_help("remove the duplicate attribute on this declaration"),
                        );
                        break;
                    }
                }
            }
        }
    }

    fn validate_web_attribute_target(&mut self, attr: &Attribute, target: AttributeTarget) {
        let allowed = match attr.name.as_str() {
            "get" | "post" | "put" | "patch" | "delete" => target == AttributeTarget::Function,
            "filter" => target == AttributeTarget::Struct,
            "body" | "header" | "path" | "query" => target == AttributeTarget::Param,
            "validate" => target == AttributeTarget::Field || target == AttributeTarget::Param,
            _ => true,
        };
        if !allowed {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1096",
                    format!(
                        "framework attribute `#[{}]` is not supported on {}",
                        attr.name,
                        self.attribute_target_label(target)
                    ),
                    self.file,
                    attr.span,
                )
                .with_help(self.attribute_target_help(&attr.name)),
            );
        }
    }

    fn validate_web_attribute_shape(&mut self, attr: &Attribute) {
        let valid = match attr.name.as_str() {
            "get" | "post" | "put" | "patch" | "delete" => self.has_one_positional_string(attr),
            "filter" => self.has_named_int(attr, "order"),
            "body" => attr.args.is_empty(),
            "header" | "path" | "query" => self.has_one_positional_string(attr),
            "validate" => !attr.args.is_empty(),
            _ => true,
        };
        if !valid {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1097",
                    format!(
                        "invalid argument shape for framework attribute `#[{}]`",
                        attr.name
                    ),
                    self.file,
                    attr.span,
                )
                .with_help(self.attribute_shape_help(&attr.name)),
            );
        }
    }

    fn is_known_web_attribute(&self, name: &str) -> bool {
        matches!(
            name,
            "get"
                | "post"
                | "put"
                | "patch"
                | "delete"
                | "filter"
                | "body"
                | "header"
                | "path"
                | "query"
                | "validate"
        )
    }

    fn is_singleton_web_attribute(&self, name: &str) -> bool {
        self.is_known_web_attribute(name)
    }

    fn has_one_positional_string(&self, attr: &Attribute) -> bool {
        if attr.args.len() != 1 {
            return false;
        }
        matches!(
            &attr.args[0],
            AttributeArg::Positional(AttributeValue {
                kind: AttributeValueKind::String(_),
                ..
            })
        )
    }

    fn has_named_int(&self, attr: &Attribute, expected_name: &str) -> bool {
        if attr.args.len() != 1 {
            return false;
        }
        matches!(
            &attr.args[0],
            AttributeArg::Named {
                name,
                value:
                    AttributeValue {
                        kind: AttributeValueKind::Int(_),
                        ..
                    },
                ..
            } if name == expected_name
        )
    }

    fn attribute_target_label(&self, target: AttributeTarget) -> &'static str {
        match target {
            AttributeTarget::Function => "a function",
            AttributeTarget::Struct => "a struct",
            AttributeTarget::Field => "a struct field",
            AttributeTarget::Enum => "an enum",
            AttributeTarget::Variant => "an enum variant",
            AttributeTarget::Trait => "a trait",
            AttributeTarget::Impl => "an impl block",
            AttributeTarget::Param => "a parameter",
            AttributeTarget::TypeAlias => "a type alias",
            AttributeTarget::Const => "a const item",
        }
    }

    fn attribute_target_help(&self, name: &str) -> &'static str {
        match name {
            "get" | "post" | "put" | "patch" | "delete" => {
                "place route attributes on handler functions"
            }
            "filter" => "place filter attributes on filter structs",
            "body" | "header" | "path" | "query" => {
                "place extractor attributes on handler parameters"
            }
            "validate" => "place validation attributes on DTO fields or handler parameters",
            _ => "move the attribute to a supported declaration",
        }
    }

    fn attribute_shape_help(&self, name: &str) -> &'static str {
        match name {
            "get" | "post" | "put" | "patch" | "delete" => {
                "use a single path literal, for example #[get(\"/users/:id\")]"
            }
            "filter" => "use #[filter(order = 10)]",
            "body" => "use #[body] without arguments",
            "header" => "use a single header literal, for example #[header(\"x-request-id\")]",
            "path" => "use a single path-parameter literal, for example #[path(\"id\")]",
            "query" => "use a single query-parameter literal, for example #[query(\"page\")]",
            "validate" => "use at least one validation rule, for example #[validate(required)]",
            _ => "use a supported framework attribute shape",
        }
    }

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        while self.at_kind(|k| matches!(k, TokenKind::Hash))
            && self
                .peek(1)
                .map(|token| matches!(token.kind, TokenKind::LBracket))
                .unwrap_or(false)
        {
            let start = self.current_span().start;
            self.bump(); // #
            self.bump(); // [
            let Some((name, _)) = self.expect_ident("E1094", "expected attribute name after '#['")
            else {
                self.recover_to_attribute_end();
                continue;
            };

            let mut args = Vec::new();
            if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
                self.bump();
                while !self.at_kind(|k| matches!(k, TokenKind::RParen | TokenKind::Eof)) {
                    let arg_start = self.current_span().start;
                    if matches!(self.current().kind, TokenKind::Ident(_))
                        && self
                            .peek(1)
                            .map(|token| matches!(token.kind, TokenKind::Eq))
                            .unwrap_or(false)
                    {
                        let Some((arg_name, arg_name_span)) =
                            self.expect_ident("E1095", "expected attribute argument name")
                        else {
                            self.recover_to_attribute_delimiter();
                            continue;
                        };
                        self.bump(); // =
                        let Some(value) = self.parse_attribute_value() else {
                            self.recover_to_attribute_delimiter();
                            continue;
                        };
                        args.push(AttributeArg::Named {
                            name: arg_name,
                            value: value.clone(),
                            span: Span::new(arg_name_span.start, value.span.end),
                        });
                    } else if let Some(value) = self.parse_attribute_value() {
                        let _ = arg_start;
                        args.push(AttributeArg::Positional(value));
                    } else {
                        self.recover_to_attribute_delimiter();
                        continue;
                    }

                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let _ = self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1095",
                    "expected ')' after attribute arguments",
                );
            }

            let close = match self.expect(
                |k| matches!(k, TokenKind::RBracket),
                "E1094",
                "expected ']' after attribute",
            ) {
                Some(span) => span,
                None => {
                    self.recover_to_attribute_end();
                    continue;
                }
            };
            attrs.push(Attribute {
                name,
                args,
                span: Span::new(start, close.end),
            });
        }
        attrs
    }

    fn parse_attribute_value(&mut self) -> Option<AttributeValue> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::String(value) => {
                self.bump();
                Some(AttributeValue {
                    kind: AttributeValueKind::String(value),
                    span: token.span,
                })
            }
            TokenKind::Int(value) => {
                self.bump();
                Some(AttributeValue {
                    kind: AttributeValueKind::Int(value.value),
                    span: token.span,
                })
            }
            TokenKind::Minus => {
                let start = token.span.start;
                self.bump();
                let next = self.current().clone();
                match next.kind {
                    TokenKind::Int(value) => {
                        self.bump();
                        Some(AttributeValue {
                            kind: AttributeValueKind::Int(0 - value.value),
                            span: Span::new(start, next.span.end),
                        })
                    }
                    _ => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E1095",
                                "expected integer literal after '-' in attribute argument",
                                self.file,
                                next.span,
                            )
                            .with_help("use literals like -1 or 42 inside attribute arguments"),
                        );
                        None
                    }
                }
            }
            TokenKind::KwTrue => {
                self.bump();
                Some(AttributeValue {
                    kind: AttributeValueKind::Bool(BoolLiteral::True),
                    span: token.span,
                })
            }
            TokenKind::KwFalse => {
                self.bump();
                Some(AttributeValue {
                    kind: AttributeValueKind::Bool(BoolLiteral::False),
                    span: token.span,
                })
            }
            TokenKind::Ident(value) => {
                self.bump();
                Some(AttributeValue {
                    kind: AttributeValueKind::Ident(value),
                    span: token.span,
                })
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1095",
                        "expected attribute argument value",
                        self.file,
                        token.span,
                    )
                    .with_help(
                        "use string, int, bool, or identifier values in attribute arguments",
                    ),
                );
                None
            }
        }
    }

    fn recover_to_attribute_delimiter(&mut self) {
        while !self.at_kind(|k| matches!(k, TokenKind::Comma | TokenKind::RParen | TokenKind::Eof))
        {
            self.bump();
        }
    }

    fn recover_to_attribute_end(&mut self) {
        while !self.at_kind(|k| matches!(k, TokenKind::RBracket | TokenKind::Eof)) {
            self.bump();
        }
        if self.at_kind(|k| matches!(k, TokenKind::RBracket)) {
            self.bump();
        }
    }

    fn parse_function(
        &mut self,
        attrs: Vec<Attribute>,
        is_async: bool,
        is_unsafe: bool,
        start: usize,
        allow_self_receiver: bool,
        visibility: Visibility,
    ) -> Option<Function> {
        self.bump(); // fn
        let (name, _) = self.expect_ident("E1004", "expected function name")?;
        let mut generics = self.parse_generics();
        self.expect(
            |k| matches!(k, TokenKind::LParen),
            "E1005",
            "expected '(' after function name",
        )?;
        let params = self.parse_params(allow_self_receiver)?;
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1006",
            "expected '->' with function return type",
        )?;
        let ret_type = self.parse_type()?;
        self.parse_where_clause(&mut generics);
        let effects = self.parse_effects_clause()?;
        let capabilities = self.parse_capabilities_clause()?;

        let mut requires = None;
        let mut ensures = None;
        loop {
            if self.at_kind(|k| matches!(k, TokenKind::KwRequires)) {
                self.bump();
                requires = Some(self.parse_expr_before_block()?);
                continue;
            }
            if self.at_kind(|k| matches!(k, TokenKind::KwEnsures)) {
                self.bump();
                ensures = Some(self.parse_expr_before_block()?);
                continue;
            }
            break;
        }

        let body = self.parse_block()?;
        let span = Span::new(start, body.span.end);
        Some(Function {
            name,
            visibility,
            attrs,
            is_async,
            is_unsafe,
            is_extern: false,
            extern_abi: None,
            is_intrinsic: false,
            intrinsic_abi: None,
            generics,
            params,
            ret_type,
            effects,
            capabilities,
            requires,
            ensures,
            body,
            span,
        })
    }

    fn parse_extern_function(
        &mut self,
        attrs: Vec<Attribute>,
        visibility: Visibility,
    ) -> Option<Function> {
        let start = self.current_span().start;
        self.bump(); // extern
        let abi_token = self.current().clone();
        let abi = match abi_token.kind {
            TokenKind::String(abi) => {
                self.bump();
                abi
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1063",
                        "expected ABI string after `extern` (for example `extern \"C\" fn ...;`)",
                        self.file,
                        abi_token.span,
                    )
                    .with_help("use `extern \"C\" fn name(...) -> Ret;` for C ABI declarations"),
                );
                return None;
            }
        };

        if !self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
            self.diagnostics.push(Diagnostic::error(
                "E1064",
                "expected `fn` after extern ABI declaration",
                self.file,
                self.current_span(),
            ));
            return None;
        }
        self.bump(); // fn

        let (name, _) = self.expect_ident("E1004", "expected function name")?;
        let generics = self.parse_generics();
        if !generics.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1065",
                    "extern function declarations cannot declare generic parameters",
                    self.file,
                    self.previous_span(),
                )
                .with_help("declare concrete C ABI parameter and return types"),
            );
        }
        self.expect(
            |k| matches!(k, TokenKind::LParen),
            "E1005",
            "expected '(' after function name",
        )?;
        let params = self.parse_params(false)?;
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1006",
            "expected '->' with function return type",
        )?;
        let ret_type = self.parse_type()?;
        let signature_end = self.previous_span().end;

        // Foreign code is opaque to effect analysis, so every extern
        // declaration states its effects. `effects { }` declares a pure
        // C function; callers inherit whatever is declared here.
        let effects_declared = self.at_kind(|k| matches!(k, TokenKind::KwEffects));
        let effects = self.parse_effects_clause()?;
        if !effects_declared {
            self.diagnostics.push(
                Diagnostic::error(
                    "E2010",
                    format!(
                        "extern function '{}' must declare its effects",
                        name
                    ),
                    self.file,
                    Span::new(start, signature_end),
                )
                .with_help(
                    "add `effects { ... }` naming what the C function does (for example `effects { io }` for `puts`), or `effects { }` if it has no side effects",
                ),
            );
        }

        if self.at_kind(|k| {
            matches!(
                k,
                TokenKind::KwEffects
                    | TokenKind::KwCapabilities
                    | TokenKind::KwRequires
                    | TokenKind::KwEnsures
            )
        }) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1066",
                    "extern function declarations cannot have capabilities/contracts or a second effects clause",
                    self.file,
                    self.current_span(),
                )
                .with_help("declare `extern` signatures with one `effects { ... }` clause only; callers hold the capabilities, and contracts belong on wrapper functions"),
            );
            while !self.at_kind(|k| matches!(k, TokenKind::Semi | TokenKind::Eof)) {
                self.bump();
            }
        }

        let semi = self.expect(
            |k| matches!(k, TokenKind::Semi),
            "E1067",
            "expected ';' after extern function declaration",
        )?;
        let span = Span::new(start, semi.end);
        Some(Function {
            name,
            visibility,
            attrs,
            is_async: false,
            is_unsafe: false,
            is_extern: true,
            extern_abi: Some(abi),
            is_intrinsic: false,
            intrinsic_abi: None,
            generics,
            params,
            ret_type,
            effects,
            capabilities: Vec::new(),
            requires: None,
            ensures: None,
            body: Block {
                stmts: Vec::new(),
                tail: None,
                span: Span::new(semi.start, semi.end),
            },
            span,
        })
    }

    fn parse_intrinsic_function(
        &mut self,
        attrs: Vec<Attribute>,
        visibility: Visibility,
    ) -> Option<Function> {
        let start = self.current_span().start;
        self.bump(); // intrinsic

        if !self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1093",
                    "expected `fn` after `intrinsic`",
                    self.file,
                    self.current_span(),
                )
                .with_help("declare intrinsics as `intrinsic fn name(...) -> Ret;`"),
            );
            return None;
        }
        self.bump(); // fn

        let (name, _) = self.expect_ident("E1004", "expected function name")?;
        let generics = self.parse_generics();

        self.expect(
            |k| matches!(k, TokenKind::LParen),
            "E1005",
            "expected '(' after function name",
        )?;
        let params = self.parse_params(false)?;
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1006",
            "expected '->' with function return type",
        )?;
        let ret_type = self.parse_type()?;
        let effects = self.parse_effects_clause()?;
        let capabilities = self.parse_capabilities_clause()?;

        if self.at_kind(|k| matches!(k, TokenKind::KwRequires | TokenKind::KwEnsures)) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1093",
                    "intrinsic declarations cannot declare requires/ensures contracts",
                    self.file,
                    self.current_span(),
                )
                .with_help("move contracts to wrapper functions around intrinsic calls"),
            );
            while !self
                .at_kind(|k| matches!(k, TokenKind::Semi | TokenKind::LBrace | TokenKind::Eof))
            {
                self.bump();
            }
        }

        if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1093",
                    "intrinsic declarations cannot have function bodies",
                    self.file,
                    self.current_span(),
                )
                .with_help("declare intrinsics as `intrinsic fn name(...) -> Ret;`"),
            );
            let _ = self.parse_block();
            return None;
        }

        let semi = self.expect(
            |k| matches!(k, TokenKind::Semi),
            "E1093",
            "expected ';' after intrinsic declaration",
        )?;
        let span = Span::new(start, semi.end);
        Some(Function {
            name,
            visibility,
            attrs,
            is_async: false,
            is_unsafe: false,
            is_extern: false,
            extern_abi: None,
            is_intrinsic: true,
            intrinsic_abi: Some("runtime".to_string()),
            generics,
            params,
            ret_type,
            effects,
            capabilities,
            requires: None,
            ensures: None,
            body: Block {
                stmts: Vec::new(),
                tail: None,
                span: Span::new(semi.start, semi.end),
            },
            span,
        })
    }
    fn parse_struct(&mut self, attrs: Vec<Attribute>, visibility: Visibility) -> Option<StructDef> {
        let start = self.current_span().start;
        self.bump(); // struct
        let (name, _) = self.expect_ident("E1010", "expected struct name")?;
        let generics = self.parse_generics();
        // `struct S;` and `struct S(Int, Bool);`.
        if self.at_kind(|k| matches!(k, TokenKind::Semi | TokenKind::LParen)) {
            return self.parse_tuple_or_unit_struct(start, name, attrs, visibility, generics);
        }
        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1011",
            "expected '{', '(' or ';' after struct name",
        )?;
        let mut fields = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let field_start = self.current_span().start;
            let field_attrs = self.parse_attributes();
            self.validate_attributes(&field_attrs, AttributeTarget::Field);
            let field_visibility = self.parse_visibility_modifier();
            let (field_name, _) = self.expect_ident("E1012", "expected field name")?;
            self.expect(
                |k| matches!(k, TokenKind::Colon),
                "E1013",
                "expected ':' after field name",
            )?;
            let ty = self.parse_type()?;
            let mut field_end = ty.span.end;
            let default_value = if self.at_kind(|k| matches!(k, TokenKind::Eq)) {
                self.bump();
                let expr = self.parse_expr()?;
                field_end = expr.span.end;
                Some(expr)
            } else {
                None
            };
            fields.push(Field {
                name: field_name,
                visibility: field_visibility,
                attrs: field_attrs,
                ty: ty.clone(),
                default_value,
                span: Span::new(field_start, field_end),
            });
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        let close = self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1014",
            "expected '}' after struct fields",
        )?;

        let invariant = if self.at_kind(|k| matches!(k, TokenKind::KwInvariant)) {
            self.bump();
            Some(self.parse_expr_before_block()?)
        } else {
            None
        };

        Some(StructDef {
            name,
            shape: StructShape::Named,
            visibility,
            attrs,
            generics,
            fields,
            invariant,
            span: Span::new(start, close.end),
        })
    }

    /// `struct S;` (no fields) or `struct S(T, U);` (fields `0`, `1`, ...),
    /// after the name and generics.
    fn parse_tuple_or_unit_struct(
        &mut self,
        start: usize,
        name: String,
        attrs: Vec<Attribute>,
        visibility: Visibility,
        generics: Vec<GenericParam>,
    ) -> Option<StructDef> {
        let mut fields = Vec::new();
        let shape = if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
            self.bump();
            while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                let field_start = self.current_span().start;
                let field_attrs = self.parse_attributes();
                self.validate_attributes(&field_attrs, AttributeTarget::Field);
                let field_visibility = self.parse_visibility_modifier();
                let ty = self.parse_type()?;
                fields.push(Field {
                    name: fields.len().to_string(),
                    visibility: field_visibility,
                    attrs: field_attrs,
                    span: Span::new(field_start, ty.span.end),
                    ty,
                    default_value: None,
                });
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(
                |k| matches!(k, TokenKind::RParen),
                "E1014",
                "expected ')' after tuple struct fields",
            )?;
            if fields.is_empty() {
                let span = self.previous_span();
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1011",
                        "a tuple struct needs at least one field",
                        self.file,
                        span,
                    )
                    .with_help("write `struct S;` for a struct without fields"),
                );
            }
            StructShape::Tuple
        } else {
            StructShape::Unit
        };
        let invariant = if self.at_kind(|k| matches!(k, TokenKind::KwInvariant)) {
            self.bump();
            Some(self.parse_expr()?)
        } else {
            None
        };
        let semi = self.expect(
            |k| matches!(k, TokenKind::Semi),
            "E1014",
            "expected ';' after a tuple or unit struct",
        )?;
        Some(StructDef {
            name,
            shape,
            visibility,
            attrs,
            generics,
            fields,
            invariant,
            span: Span::new(start, semi.end),
        })
    }

    fn parse_enum(&mut self, attrs: Vec<Attribute>, visibility: Visibility) -> Option<EnumDef> {
        let start = self.current_span().start;
        self.bump(); // enum
        let (name, _) = self.expect_ident("E1015", "expected enum name")?;
        let generics = self.parse_generics();
        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1016",
            "expected '{' after enum name",
        )?;
        let mut variants = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let var_start = self.current_span().start;
            let variant_attrs = self.parse_attributes();
            self.validate_attributes(&variant_attrs, AttributeTarget::Variant);
            let (var_name, _) = self.expect_ident("E1017", "expected enum variant name")?;
            let payload = if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
                // `V(A)` carries one payload; `V(A, B, ..)` carries the tuple
                // `(A, B, ..)`.
                let open = self.current_span();
                self.bump();
                let mut items = vec![self.parse_type()?];
                while self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                    if self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                        break;
                    }
                    items.push(self.parse_type()?);
                }
                let close = self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1018",
                    "expected ')' after variant payload types",
                )?;
                if items.len() == 1 {
                    items.pop()
                } else {
                    Some(TypeExpr {
                        kind: TypeKind::Named {
                            name: TUPLE_INTERNAL_NAME.to_string(),
                            args: items,
                        },
                        span: Span::new(open.start, close.end),
                    })
                }
            } else if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
                Some(self.parse_struct_variant_payload(&name, &var_name, &generics, visibility)?)
            } else {
                None
            };
            let end = payload
                .as_ref()
                .map(|p| p.span.end)
                .unwrap_or(self.previous_span().end);
            variants.push(VariantDef {
                name: var_name,
                attrs: variant_attrs,
                payload,
                span: Span::new(var_start, end),
            });
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        let close = self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1019",
            "expected '}' after enum variants",
        )?;
        Some(EnumDef {
            name,
            visibility,
            attrs,
            generics,
            variants,
            span: Span::new(start, close.end),
        })
    }

    /// `Variant { a: A, b: B }` in an enum: the fields become the hidden
    /// payload struct `Enum__Variant` (generic over the enum's parameters),
    /// emitted after the enum, and the variant carries one value of it.
    fn parse_struct_variant_payload(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        generics: &[GenericParam],
        visibility: Visibility,
    ) -> Option<TypeExpr> {
        let open = self.current_span();
        self.bump(); // {
        let mut fields = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let (field_name, field_span) =
                self.expect_ident("E1012", "expected field name in struct-like variant")?;
            self.expect(
                |k| matches!(k, TokenKind::Colon),
                "E1013",
                "expected ':' after variant field name",
            )?;
            let ty = self.parse_type()?;
            let span = Span::new(field_span.start, ty.span.end);
            fields.push(Field {
                name: field_name,
                visibility: Visibility::Public,
                attrs: Vec::new(),
                ty,
                default_value: None,
                span,
            });
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        let close = self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1019",
            "expected '}' after variant fields",
        )?;
        let span = Span::new(open.start, close.end);
        let struct_name = struct_variant_payload_name(enum_name, variant_name);
        self.pending_items.push(Item::Struct(StructDef {
            name: struct_name.clone(),
            shape: StructShape::Named,
            visibility,
            attrs: Vec::new(),
            generics: generics.to_vec(),
            fields,
            invariant: None,
            span,
        }));
        Some(TypeExpr {
            kind: TypeKind::Named {
                name: struct_name,
                args: generics
                    .iter()
                    .map(|generic| TypeExpr {
                        kind: TypeKind::Named {
                            name: generic.name.clone(),
                            args: Vec::new(),
                        },
                        span,
                    })
                    .collect(),
            },
            span,
        })
    }

    fn parse_trait(&mut self, attrs: Vec<Attribute>, visibility: Visibility) -> Option<TraitDef> {
        let start = self.current_span().start;
        self.bump(); // trait
        let (name, _) = self.expect_ident("E1053", "expected trait name")?;
        let generics = self.parse_generics();
        if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            let end = self
                .expect(
                    |k| matches!(k, TokenKind::Semi),
                    "E1054",
                    "expected ';' after trait declaration",
                )?
                .end;
            return Some(TraitDef {
                name,
                visibility,
                attrs,
                generics,
                methods: Vec::new(),
                span: Span::new(start, end),
            });
        }

        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1054",
            "expected ';' or '{' after trait declaration",
        )?;
        let mut methods = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace | TokenKind::Eof)) {
            if self.at_container_terminator() {
                // A module-level item inside a trait block means the closing `}`
                // is missing; stop here so the item is parsed at module scope.
                break;
            }
            let member_index = self.index;
            let method_start = self.current_span().start;
            let method_attrs = self.parse_attributes();
            self.validate_attributes(&method_attrs, AttributeTarget::Function);
            let method_visibility = self.parse_visibility_modifier();
            if self.at_container_terminator() {
                break;
            }
            if self.report_unsupported_associated_item("trait") {
                self.recover_member(member_index);
                continue;
            }
            let mut is_async = false;
            let mut is_unsafe = false;
            if self.at_kind(|k| matches!(k, TokenKind::KwAsync)) {
                self.bump();
                is_async = true;
            }
            if self.at_kind(|k| matches!(k, TokenKind::KwUnsafe)) {
                self.bump();
                is_unsafe = true;
            }
            if !self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1082",
                        "expected trait method signature (`fn`, `async fn`, or `unsafe fn`) in trait block",
                        self.file,
                        self.current_span(),
                    )
                    .with_help("define signatures like `fn name(...) -> Ret;` inside trait blocks"),
                );
                self.recover_member(member_index);
                continue;
            }
            if let Some(method) = self.parse_trait_method_signature(
                method_attrs,
                is_async,
                is_unsafe,
                method_start,
                method_visibility,
            ) {
                methods.push(method);
            } else {
                self.recover_member(member_index);
            }
        }
        let close = self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1086",
            "expected '}' after trait block",
        )?;
        Some(TraitDef {
            name,
            visibility,
            attrs,
            generics,
            methods,
            span: Span::new(start, close.end),
        })
    }

    fn parse_impl(&mut self, attrs: Vec<Attribute>, visibility: Visibility) -> Option<ImplDef> {
        let start = self.current_span().start;
        self.bump(); // impl
        let head_ty = self.parse_type()?;
        if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
            self.bump();
            let mut methods = Vec::new();
            while !self.at_kind(|k| matches!(k, TokenKind::RBrace | TokenKind::Eof)) {
                if self.at_container_terminator() {
                    break;
                }
                let member_index = self.index;
                let method_start = self.current_span().start;
                let method_attrs = self.parse_attributes();
                self.validate_attributes(&method_attrs, AttributeTarget::Function);
                let method_visibility = self.parse_visibility_modifier();
                if self.at_container_terminator() {
                    break;
                }
                if self.report_unsupported_associated_item("impl") {
                    self.recover_member(member_index);
                    continue;
                }
                let mut is_async = false;
                let mut is_unsafe = false;
                if self.at_kind(|k| matches!(k, TokenKind::KwAsync)) {
                    self.bump();
                    is_async = true;
                }
                if self.at_kind(|k| matches!(k, TokenKind::KwUnsafe)) {
                    self.bump();
                    is_unsafe = true;
                }
                if !self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E1082",
                            "expected method declaration (`fn`, `async fn`, or `unsafe fn`) in impl block",
                            self.file,
                            self.current_span(),
                        )
                        .with_help("define methods inside `impl Type { ... }` blocks"),
                    );
                    self.recover_member(member_index);
                    continue;
                }
                if let Some(method) = self.parse_function(
                    method_attrs,
                    is_async,
                    is_unsafe,
                    method_start,
                    true,
                    method_visibility,
                ) {
                    methods.push(method);
                } else {
                    self.recover_member(member_index);
                }
            }
            let close = self.expect(
                |k| matches!(k, TokenKind::RBrace),
                "E1086",
                "expected '}' after impl block",
            )?;
            let (head_name, head_args) = match &head_ty.kind {
                TypeKind::Named { name, args } => (name.clone(), args.clone()),
                TypeKind::DynTrait { .. } | TypeKind::Unit | TypeKind::Hole => {
                    self.diagnostics.push(Diagnostic::error(
                        "E1087",
                        "impl target must be a named type",
                        self.file,
                        head_ty.span,
                    ));
                    ("<?>".to_string(), Vec::new())
                }
            };
            if !head_args.is_empty() {
                return Some(ImplDef {
                    trait_name: head_name,
                    visibility,
                    attrs,
                    trait_args: head_args,
                    target: None,
                    methods,
                    is_inherent: false,
                    span: Span::new(start, close.end),
                });
            }
            return Some(ImplDef {
                trait_name: head_name,
                visibility,
                attrs,
                trait_args: Vec::new(),
                target: Some(head_ty),
                methods,
                is_inherent: true,
                span: Span::new(start, close.end),
            });
        }

        let (trait_name, trait_args) = match head_ty.kind {
            TypeKind::Named { name, args } => (name, args),
            TypeKind::DynTrait { .. } | TypeKind::Unit | TypeKind::Hole => {
                self.diagnostics.push(Diagnostic::error(
                    "E1055",
                    "expected trait name after impl",
                    self.file,
                    head_ty.span,
                ));
                return None;
            }
        };

        let end = self
            .expect(
                |k| matches!(k, TokenKind::Semi),
                "E1058",
                "expected ';' after impl declaration",
            )?
            .end;
        Some(ImplDef {
            trait_name,
            visibility,
            attrs,
            trait_args,
            target: None,
            methods: Vec::new(),
            is_inherent: false,
            span: Span::new(start, end),
        })
    }

    fn parse_type_alias(&mut self, attrs: Vec<Attribute>) -> Option<Function> {
        let start = self.current_span().start;
        self.bump(); // type
        let (alias_name, _) = self.expect_ident("E1075", "expected alias name after `type`")?;
        let generics = self.parse_generics();
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1076",
            "expected '=' in type alias declaration",
        )?;
        let target_ty = self.parse_type()?;
        let semi = self.expect(
            |k| matches!(k, TokenKind::Semi),
            "E1077",
            "expected ';' after type alias declaration",
        )?;

        Some(Function {
            name: encode_internal_type_alias(&alias_name),
            visibility: Visibility::Private,
            attrs,
            is_async: false,
            is_unsafe: false,
            is_extern: false,
            extern_abi: None,
            is_intrinsic: false,
            intrinsic_abi: None,
            generics,
            params: Vec::new(),
            ret_type: target_ty,
            effects: Vec::new(),
            capabilities: Vec::new(),
            requires: None,
            ensures: None,
            body: Block {
                stmts: Vec::new(),
                tail: None,
                span: Span::new(semi.start, semi.end),
            },
            span: Span::new(start, semi.end),
        })
    }

    fn parse_const_item(&mut self, attrs: Vec<Attribute>) -> Option<Function> {
        let start = self.current_span().start;
        self.bump(); // const
        let (const_name, _) = self.expect_ident("E1078", "expected constant name after `const`")?;
        self.expect(
            |k| matches!(k, TokenKind::Colon),
            "E1079",
            "expected ':' after const name",
        )?;
        let const_ty = self.parse_type()?;
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1080",
            "expected '=' in const declaration",
        )?;
        let expr = self.parse_expr()?;
        let semi = self.expect(
            |k| matches!(k, TokenKind::Semi),
            "E1081",
            "expected ';' after const declaration",
        )?;
        let expr_span = expr.span;

        Some(Function {
            name: encode_internal_const(&const_name),
            visibility: Visibility::Private,
            attrs,
            is_async: false,
            is_unsafe: false,
            is_extern: false,
            extern_abi: None,
            is_intrinsic: false,
            intrinsic_abi: None,
            generics: Vec::new(),
            params: Vec::new(),
            ret_type: const_ty,
            effects: Vec::new(),
            capabilities: Vec::new(),
            requires: None,
            ensures: None,
            body: Block {
                stmts: Vec::new(),
                tail: Some(Box::new(expr)),
                span: Span::new(expr_span.start, expr_span.end),
            },
            span: Span::new(start, semi.end),
        })
    }

    fn parse_generics(&mut self) -> Vec<GenericParam> {
        if !self.at_kind(|k| matches!(k, TokenKind::LBracket)) {
            return Vec::new();
        }
        self.bump();
        let mut params = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBracket)) {
            if let Some((name, span)) = self.expect_ident("E1020", "expected generic parameter") {
                let mut bounds = Vec::new();
                if self.at_kind(|k| matches!(k, TokenKind::Colon)) {
                    self.bump();
                    loop {
                        let Some((bound, _)) =
                            self.expect_ident("E1059", "expected trait bound after ':'")
                        else {
                            break;
                        };
                        bounds.push(bound);
                        if self.at_kind(|k| matches!(k, TokenKind::Plus)) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                params.push(GenericParam { name, bounds, span });
            } else {
                break;
            }
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        let _ = self.expect(
            |k| matches!(k, TokenKind::RBracket),
            "E1021",
            "expected ']' after generic parameters",
        );
        params
    }

    fn parse_where_clause(&mut self, generics: &mut [GenericParam]) {
        if !self.at_kind(|k| matches!(k, TokenKind::KwWhere)) {
            return;
        }
        self.bump();

        loop {
            let Some((generic_name, generic_span)) =
                self.expect_ident("E1020", "expected generic parameter in where clause")
            else {
                break;
            };
            if self
                .expect(
                    |k| matches!(k, TokenKind::Colon),
                    "E1023",
                    "expected ':' after generic parameter in where clause",
                )
                .is_none()
            {
                break;
            }

            let mut parsed_bounds = Vec::new();
            loop {
                let Some((bound, _)) =
                    self.expect_ident("E1059", "expected trait bound in where clause")
                else {
                    break;
                };
                parsed_bounds.push(bound);
                if self.at_kind(|k| matches!(k, TokenKind::Plus)) {
                    self.bump();
                } else {
                    break;
                }
            }

            if let Some(param) = generics.iter_mut().find(|g| g.name == generic_name) {
                for bound in parsed_bounds {
                    if !param.bounds.iter().any(|existing| existing == &bound) {
                        param.bounds.push(bound);
                    }
                }
            } else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1259",
                        format!(
                            "where clause references unknown generic parameter '{}'",
                            generic_name
                        ),
                        self.file,
                        generic_span,
                    )
                    .with_help("declare the generic parameter in the function signature"),
                );
            }

            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
                continue;
            }
            break;
        }
    }

    fn parse_effects_clause(&mut self) -> Option<Vec<String>> {
        if !self.at_kind(|k| matches!(k, TokenKind::KwEffects)) {
            return Some(Vec::new());
        }

        self.bump();
        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1007",
            "expected '{' after effects",
        )?;
        let mut effects = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let (name, _) = self.expect_ident("E1008", "expected effect name")?;
            effects.push(name);
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1009",
            "expected '}' to close effects list",
        )?;
        Some(effects)
    }

    fn parse_capabilities_clause(&mut self) -> Option<Vec<String>> {
        if !self.at_kind(|k| matches!(k, TokenKind::KwCapabilities)) {
            return Some(Vec::new());
        }

        self.bump();
        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1017",
            "expected '{' after capabilities",
        )?;
        let mut capabilities = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let (name, _) = self.expect_ident("E1018", "expected capability name")?;
            capabilities.push(name);
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1019",
            "expected '}' to close capabilities list",
        )?;
        Some(capabilities)
    }

    fn parse_trait_method_signature(
        &mut self,
        attrs: Vec<Attribute>,
        is_async: bool,
        is_unsafe: bool,
        start: usize,
        visibility: Visibility,
    ) -> Option<Function> {
        self.bump(); // fn
        let (name, _) = self.expect_ident("E1004", "expected method name")?;
        let mut generics = self.parse_generics();
        self.expect(
            |k| matches!(k, TokenKind::LParen),
            "E1005",
            "expected '(' after method name",
        )?;
        let params = self.parse_params(true)?;
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1006",
            "expected '->' with method return type",
        )?;
        let ret_type = self.parse_type()?;
        self.parse_where_clause(&mut generics);
        let effects = self.parse_effects_clause()?;
        let capabilities = self.parse_capabilities_clause()?;
        if self.at_kind(|k| matches!(k, TokenKind::KwRequires | TokenKind::KwEnsures)) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1089",
                    "trait method signatures cannot declare requires/ensures contracts",
                    self.file,
                    self.current_span(),
                )
                .with_help("move contracts to concrete impl methods"),
            );
            while !self
                .at_kind(|k| matches!(k, TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof))
            {
                self.bump();
            }
        }
        if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
            let body_start = self.current_span();
            self.skip_balanced_braces();
            let body_span = Span::new(body_start.start, self.previous_span().end);
            self.diagnostics.push(
                Diagnostic::error(
                    "E1098",
                    format!("default method bodies are not supported in traits yet (`{name}`)"),
                    self.file,
                    body_span,
                )
                .with_help(
                    "end the trait signature with `;` and implement the body in each `impl Trait[Type] { ... }` block",
                ),
            );
            return Some(Function {
                name,
                visibility,
                attrs,
                is_async,
                is_unsafe,
                is_extern: false,
                extern_abi: None,
                is_intrinsic: false,
                intrinsic_abi: None,
                generics,
                params,
                ret_type,
                effects,
                capabilities,
                requires: None,
                ensures: None,
                body: Block {
                    stmts: Vec::new(),
                    tail: None,
                    span: body_span,
                },
                span: Span::new(start, body_span.end),
            });
        }
        let semi = self.expect(
            |k| matches!(k, TokenKind::Semi),
            "E1089",
            "expected ';' after trait method signature",
        )?;
        Some(Function {
            name,
            visibility,
            attrs,
            is_async,
            is_unsafe,
            is_extern: false,
            extern_abi: None,
            is_intrinsic: false,
            intrinsic_abi: None,
            generics,
            params,
            ret_type,
            effects,
            capabilities,
            requires: None,
            ensures: None,
            body: Block {
                stmts: Vec::new(),
                tail: None,
                span: Span::new(semi.start, semi.end),
            },
            span: Span::new(start, semi.end),
        })
    }

    fn parse_params(&mut self, allow_self_receiver: bool) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
            let attrs = self.parse_attributes();
            self.validate_attributes(&attrs, AttributeTarget::Param);
            if allow_self_receiver && params.is_empty() {
                if self.at_kind(|k| matches!(k, TokenKind::Ampersand)) {
                    let recv_start = self.current_span().start;
                    self.bump();
                    let mutable_receiver = self.at_kind(|k| matches!(k, TokenKind::KwMut));
                    if mutable_receiver {
                        self.bump();
                    }
                    let (recv_name, recv_span) =
                        self.expect_ident("E1083", "expected `self` after receiver reference")?;
                    if recv_name != "self" {
                        self.diagnostics.push(Diagnostic::error(
                            "E1083",
                            "expected `self` after receiver reference",
                            self.file,
                            recv_span,
                        ));
                        return None;
                    }
                    let span = Span::new(recv_start, recv_span.end);
                    params.push(Param {
                        name: "self".to_string(),
                        attrs,
                        ty: self.self_type_expr(span),
                        mutable_receiver,
                        span,
                    });
                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    }
                    continue;
                }
                if matches!(self.current().kind, TokenKind::Ident(ref name) if name == "self")
                    && !self
                        .peek(1)
                        .map(|token| matches!(token.kind, TokenKind::Colon))
                        .unwrap_or(false)
                {
                    let span = self.current_span();
                    self.bump();
                    params.push(Param {
                        name: "self".to_string(),
                        attrs,
                        ty: self.self_type_expr(span),
                        mutable_receiver: false,
                        span,
                    });
                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    }
                    continue;
                }
            }

            let start = self.current_span().start;
            let (name, _) = self.expect_ident("E1022", "expected parameter name")?;
            self.expect(
                |k| matches!(k, TokenKind::Colon),
                "E1023",
                "expected ':' after parameter name",
            )?;
            let ty = self.parse_type()?;
            params.push(Param {
                name,
                attrs,
                ty: ty.clone(),
                mutable_receiver: false,
                span: Span::new(start, ty.span.end),
            });
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(
            |k| matches!(k, TokenKind::RParen),
            "E1024",
            "expected ')' after parameters",
        )?;
        Some(params)
    }

    fn self_type_expr(&self, span: Span) -> TypeExpr {
        TypeExpr {
            kind: TypeKind::Named {
                name: "Self".to_string(),
                args: Vec::new(),
            },
            span,
        }
    }

    fn parse_type(&mut self) -> Option<TypeExpr> {
        let start = self.current_span().start;
        if self.at_kind(|k| matches!(k, TokenKind::Underscore)) {
            let hole_span = self.current_span();
            self.bump();
            return Some(TypeExpr {
                kind: TypeKind::Hole,
                span: hole_span,
            });
        }
        if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
            self.bump();
            if self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                self.bump();
                let end = self.previous_span().end;
                return Some(TypeExpr {
                    kind: TypeKind::Unit,
                    span: Span::new(start, end),
                });
            }
            let first = self.parse_type()?;
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
                let mut items = vec![first];
                while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                    let item = self.parse_type()?;
                    items.push(item);
                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1025",
                    "expected ')' after tuple type",
                )?;
                let end = self.previous_span().end;
                return Some(TypeExpr {
                    kind: TypeKind::Named {
                        name: TUPLE_INTERNAL_NAME.to_string(),
                        args: items,
                    },
                    span: Span::new(start, end),
                });
            }
            self.expect(
                |k| matches!(k, TokenKind::RParen),
                "E1025",
                "expected ')' after parenthesized type",
            )?;
            let end = self.previous_span().end;
            return Some(TypeExpr {
                kind: first.kind,
                span: Span::new(start, end),
            });
        }

        if self.at_kind(|k| matches!(k, TokenKind::KwDyn)) {
            self.bump();
            let (name, first_span) =
                self.expect_ident("E1026", "expected trait name after 'dyn'")?;
            let mut full_name = name;
            let mut end = first_span.end;
            while self.at_kind(|k| matches!(k, TokenKind::ColonColon)) {
                self.bump();
                let (seg, seg_span) = self.expect_ident("E1027", "expected path segment")?;
                full_name.push_str("::");
                full_name.push_str(&seg);
                end = seg_span.end;
            }
            return Some(TypeExpr {
                kind: TypeKind::DynTrait {
                    trait_name: full_name,
                },
                span: Span::new(start, end),
            });
        }

        let (name, first_span) = self.expect_ident("E1026", "expected type name")?;
        let mut full_name = name;
        let mut end = first_span.end;
        while self.at_kind(|k| matches!(k, TokenKind::ColonColon)) {
            self.bump();
            let (seg, seg_span) = self.expect_ident("E1027", "expected path segment")?;
            full_name.push_str("::");
            full_name.push_str(&seg);
            end = seg_span.end;
        }
        let canonical = canonical_primitive_type_name(&full_name);
        if full_name != "Float" {
            full_name = canonical.to_string();
        }

        if full_name == "Fn" && self.at_kind(|k| matches!(k, TokenKind::LParen)) {
            self.bump();
            let mut args = Vec::new();
            while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                let arg = self.parse_type()?;
                args.push(arg);
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(
                |k| matches!(k, TokenKind::RParen),
                "E1069",
                "expected ')' after Fn parameter types",
            )?;
            self.expect(
                |k| matches!(k, TokenKind::Arrow),
                "E1070",
                "expected '->' after Fn parameter list",
            )?;
            let ret = self.parse_type()?;
            end = ret.span.end;
            args.push(ret);
            return Some(TypeExpr {
                kind: TypeKind::Named {
                    name: full_name,
                    args,
                },
                span: Span::new(start, end),
            });
        }

        let mut args = Vec::new();
        if self.at_kind(|k| matches!(k, TokenKind::LBracket)) {
            self.bump();
            while !self.at_kind(|k| matches!(k, TokenKind::RBracket)) {
                let arg = self.parse_type()?;
                args.push(arg);
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(
                |k| matches!(k, TokenKind::RBracket),
                "E1028",
                "expected ']' after type arguments",
            )?;
            end = self.previous_span().end;
        }

        Some(TypeExpr {
            kind: TypeKind::Named {
                name: full_name,
                args,
            },
            span: Span::new(start, end),
        })
    }

    fn parse_block(&mut self) -> Option<Block> {
        // Inside a block, `{` starts block expressions again.
        let prev = std::mem::replace(&mut self.disallow_block_expr, false);
        let block = self.parse_block_contents();
        self.disallow_block_expr = prev;
        block
    }

    fn parse_block_contents(&mut self) -> Option<Block> {
        let start = self
            .expect(
                |k| matches!(k, TokenKind::LBrace),
                "E1029",
                "expected '{' to start block",
            )?
            .start;

        let mut stmts = Vec::new();
        let mut tail = None;

        while !self.at_kind(|k| matches!(k, TokenKind::RBrace | TokenKind::Eof)) {
            if self.at_kind(|k| matches!(k, TokenKind::KwLet)) {
                match self.parse_let_stmt() {
                    Some(parsed) => stmts.extend(parsed),
                    None => self.recover_statement(),
                }
                continue;
            }
            if self.at_assignment_stmt_start() {
                match self.parse_assign_stmt() {
                    Some(stmt) => stmts.push(stmt),
                    None => self.recover_statement(),
                }
                continue;
            }
            if self.at_kind(|k| matches!(k, TokenKind::KwReturn)) {
                match self.parse_return_in_block() {
                    Some(ReturnInBlock::Stmt(stmt)) => stmts.push(stmt),
                    Some(ReturnInBlock::Tail(expr)) => {
                        tail = Some(Box::new(expr));
                        break;
                    }
                    None => self.recover_statement(),
                }
                continue;
            }

            match self.parse_expr() {
                Some(expr) => {
                    if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
                        let span = expr.span.join(self.current_span());
                        self.bump();
                        stmts.push(Stmt::Expr { expr, span });
                    } else if is_block_like(&expr)
                        && !self.at_kind(|k| matches!(k, TokenKind::RBrace | TokenKind::Eof))
                    {
                        // `if`/`match`/`while`/`loop`/`for`/block expressions
                        // that end in `}` are statements without a `;`.
                        let span = expr.span;
                        stmts.push(Stmt::Expr { expr, span });
                    } else {
                        tail = Some(Box::new(expr));
                        break;
                    }
                }
                None => self.recover_statement(),
            }
        }

        let close = self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1030",
            "expected '}' to close block",
        )?;

        Some(Block {
            stmts,
            tail,
            span: Span::new(start, close.end),
        })
    }

    fn parse_let_stmt(&mut self) -> Option<Vec<Stmt>> {
        let start = self.current_span().start;
        self.bump(); // let
        let mutable = if self.at_kind(|k| matches!(k, TokenKind::KwMut)) {
            self.bump();
            true
        } else {
            false
        };

        // `let (a, b) = ..`, `let P { x, y } = ..`, `let Some(x) = .. else ..`.
        let starts_pattern = self.at_kind(|k| matches!(k, TokenKind::LParen))
            || (matches!(self.current().kind, TokenKind::Ident(_))
                && matches!(
                    self.peek(1).map(|token| &token.kind),
                    Some(
                        TokenKind::LBrace
                            | TokenKind::LParen
                            | TokenKind::ColonColon
                            | TokenKind::At
                    )
                ));
        if starts_pattern {
            return self.parse_pattern_let(start, mutable);
        }
        let name = self
            .expect_ident("E1031", "expected binding name after let")?
            .0;

        let ty = if self.at_kind(|k| matches!(k, TokenKind::Colon)) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1032",
            "expected '=' in let binding",
        )?;
        let expr = self.parse_expr()?;
        let end = if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            let span = self.current_span();
            self.bump();
            span.end
        } else {
            let span = self.current_span();
            self.diagnostics.push(
                Diagnostic::error("E1033", "expected ';' after let binding", self.file, span)
                    .with_fix(crate::diagnostics::SuggestedFix {
                        message: "insert ';' after let binding".to_string(),
                        replacement: Some(";".to_string()),
                        start: Some(expr.span.end),
                        end: Some(expr.span.end),
                    }),
            );
            expr.span.end
        };

        let span = Span::new(start, end);
        Some(vec![Stmt::Let {
            name,
            mutable,
            ty,
            expr,
            span,
        }])
    }

    /// `let PATTERN [: T] = expr [else { .. }];` for tuple, struct and
    /// refutable patterns, after `let [mut]`.
    fn parse_pattern_let(&mut self, start: usize, mutable: bool) -> Option<Vec<Stmt>> {
        let pattern = self.parse_pattern()?;
        let ty = if self.at_kind(|k| matches!(k, TokenKind::Colon)) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1032",
            "expected '=' in let binding",
        )?;
        let expr = self.parse_expr()?;
        let else_block = if self.at_kind(|k| matches!(k, TokenKind::KwElse)) {
            self.bump();
            Some(self.parse_block()?)
        } else {
            None
        };
        let end = if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            let span = self.current_span();
            self.bump();
            span.end
        } else {
            let span = self.current_span();
            let at = else_block
                .as_ref()
                .map(|block| block.span.end)
                .unwrap_or(expr.span.end);
            self.diagnostics.push(
                Diagnostic::error("E1033", "expected ';' after let binding", self.file, span)
                    .with_fix(crate::diagnostics::SuggestedFix {
                        message: "insert ';' after let binding".to_string(),
                        replacement: Some(";".to_string()),
                        start: Some(at),
                        end: Some(at),
                    }),
            );
            at
        };
        let span = Span::new(start, end);

        if let Some(else_block) = else_block {
            if let Some(ty) = &ty {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1031",
                        "`let ... else` does not take a type annotation",
                        self.file,
                        ty.span,
                    )
                    .with_help(
                        "annotate the expression instead, for example with a typed helper binding",
                    ),
                );
                // The statement is consumed; keep parsing the block.
                return Some(Vec::new());
            }
            return Some(self.expand_let_else(pattern, mutable, expr, else_block, span));
        }

        if let Some(refutable) = first_refutable_pattern(&pattern) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1110",
                    "refutable pattern in `let`: it does not match every value",
                    self.file,
                    refutable.span,
                )
                .with_help(
                    "use `let PATTERN = expr else { return ...; };` or `if let PATTERN = expr { ... }`",
                ),
            );
            // The statement is consumed; keep parsing the block.
            return Some(Vec::new());
        }
        // `let whole @ PATTERN = expr;` binds `whole`, then destructures it.
        if let PatternKind::Binding {
            name,
            pattern: inner,
        } = pattern.kind
        {
            let whole = Expr::var(name.clone(), span);
            let mut stmts = vec![Stmt::Let {
                name,
                mutable,
                ty,
                expr,
                span,
            }];
            match inner.kind {
                PatternKind::Wildcard => {}
                PatternKind::Var(binding) => stmts.push(Stmt::Let {
                    name: binding,
                    mutable: false,
                    ty: None,
                    expr: whole,
                    span,
                }),
                _ => {
                    let converted = self.convert_let_pattern(&inner)?;
                    stmts.extend(self.expand_tuple_let(converted, None, whole, span));
                }
            }
            return Some(stmts);
        }
        let converted = self.convert_let_pattern(&pattern)?;
        if mutable {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1031",
                    "destructuring `let` does not support `let mut`; mark individual bindings mutable later",
                    self.file,
                    span,
                )
                .with_help("remove `mut` here and use separate mutable bindings if needed"),
            );
        }
        Some(self.expand_tuple_let(converted, ty, expr, span))
    }

    /// `let P = e else { B };` desugars to a match whose catch-all arm
    /// (bound to `_$let_else`) runs `B`, which must diverge:
    /// - no bindings: `match e { P => (), _$let_else => B };`
    /// - one binding `x`: `let x = match e { P => x, _$let_else => B };`
    /// - more: `let tmp = match e { P => (x, y), .. }; let x = tmp.0; ..`
    fn expand_let_else(
        &mut self,
        pattern: Pattern,
        mutable: bool,
        expr: Expr,
        else_block: Block,
        span: Span,
    ) -> Vec<Stmt> {
        let mut names = Vec::new();
        collect_pattern_binding_names(&pattern, &mut names);
        let value_span = pattern.span;
        let matched_value = match names.as_slice() {
            [] => Expr {
                kind: ExprKind::Unit,
                span: value_span,
            },
            [name] => Expr::var(name.clone(), value_span),
            _ => Expr {
                kind: ExprKind::StructInit {
                    name: TUPLE_INTERNAL_NAME.to_string(),
                    fields: names
                        .iter()
                        .enumerate()
                        .map(|(idx, name)| {
                            (
                                idx.to_string(),
                                Expr::var(name.clone(), value_span),
                                value_span,
                            )
                        })
                        .collect(),
                },
                span: value_span,
            },
        };
        let else_span = else_block.span;
        let matched = Expr {
            kind: ExprKind::Match {
                expr: Box::new(expr),
                arms: vec![
                    MatchArm {
                        span: pattern.span,
                        pattern,
                        guard: None,
                        body: matched_value,
                    },
                    MatchArm {
                        pattern: Pattern {
                            kind: PatternKind::Var(LET_ELSE_BINDING.to_string()),
                            span: else_span,
                        },
                        guard: None,
                        body: Expr {
                            kind: ExprKind::Block { block: else_block },
                            span: else_span,
                        },
                        span: else_span,
                    },
                ],
            },
            span,
        };
        match names.len() {
            0 => vec![Stmt::Expr {
                expr: matched,
                span,
            }],
            1 => vec![Stmt::Let {
                name: names.remove(0),
                mutable,
                ty: None,
                expr: matched,
                span,
            }],
            _ => {
                let temp = format!("{LET_ELSE_TMP_PREFIX}{}", self.tuple_binding_counter);
                self.tuple_binding_counter += 1;
                let mut out = vec![Stmt::Let {
                    name: temp.clone(),
                    mutable: false,
                    ty: None,
                    expr: matched,
                    span,
                }];
                for (idx, name) in names.into_iter().enumerate() {
                    out.push(Stmt::Let {
                        name,
                        mutable,
                        ty: None,
                        expr: Expr {
                            kind: ExprKind::FieldAccess {
                                base: Box::new(Expr::var(temp.clone(), span)),
                                field: idx.to_string(),
                            },
                            span,
                        },
                        span,
                    });
                }
                out
            }
        }
    }

    fn convert_let_pattern(&mut self, pattern: &Pattern) -> Option<TupleLetPattern> {
        match &pattern.kind {
            PatternKind::Var(name) => Some(TupleLetPattern::Binding {
                name: name.clone(),
                span: pattern.span,
            }),
            PatternKind::Wildcard => Some(TupleLetPattern::Wildcard),
            PatternKind::Variant { name, args } if name == TUPLE_INTERNAL_NAME => {
                let mut items = Vec::new();
                for arg in args {
                    items.push(self.convert_let_pattern(arg)?);
                }
                Some(TupleLetPattern::Tuple { items })
            }
            PatternKind::Struct { name, fields, .. } => {
                let mut converted = Vec::new();
                for field in fields {
                    converted.push((
                        field.name.clone(),
                        self.convert_let_pattern(&field.pattern)?,
                    ));
                }
                Some(TupleLetPattern::Struct {
                    name: name.clone(),
                    fields: converted,
                })
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1031",
                        "tuple let destructuring supports only names, `_`, and nested tuples",
                        self.file,
                        pattern.span,
                    )
                    .with_help("rewrite the pattern as `(a, b, ...)` with bindings/wildcards only"),
                );
                None
            }
        }
    }

    fn expand_tuple_let(
        &mut self,
        pattern: TupleLetPattern,
        ty: Option<TypeExpr>,
        expr: Expr,
        span: Span,
    ) -> Vec<Stmt> {
        if matches!(pattern, TupleLetPattern::Struct { .. }) && ty.is_none() {
            // A struct pattern binds the value to its own temporary.
            let mut out = Vec::new();
            self.expand_tuple_let_pattern(pattern, expr, span, &mut out);
            return out;
        }
        let temp_name = self.next_tuple_let_name();
        let mut out = vec![Stmt::Let {
            name: temp_name.clone(),
            mutable: false,
            ty,
            expr: expr.clone(),
            span,
        }];
        let source = Expr::var(temp_name, expr.span);
        self.expand_tuple_let_pattern(pattern, source, span, &mut out);
        out
    }

    fn expand_tuple_let_pattern(
        &mut self,
        pattern: TupleLetPattern,
        source: Expr,
        span: Span,
        out: &mut Vec<Stmt>,
    ) {
        match pattern {
            TupleLetPattern::Binding {
                name,
                span: binding_span,
            } => {
                out.push(Stmt::Let {
                    name,
                    mutable: false,
                    ty: None,
                    expr: source,
                    span: Span::new(binding_span.start, span.end),
                });
            }
            TupleLetPattern::Wildcard => {}
            TupleLetPattern::Tuple { items } => {
                for (idx, item) in items.into_iter().enumerate() {
                    let field_expr = Expr {
                        kind: ExprKind::FieldAccess {
                            base: Box::new(source.clone()),
                            field: idx.to_string(),
                        },
                        span: source.span,
                    };
                    self.expand_tuple_let_pattern(item, field_expr, span, out);
                }
            }
            TupleLetPattern::Struct { name, fields } => {
                // The value is bound to a temporary whose name records the
                // struct, so the checker can match it against the type.
                let id = self.tuple_binding_counter;
                self.tuple_binding_counter += 1;
                let temp = format!("{STRUCT_LET_TMP_PREFIX}{id}__{name}");
                out.push(Stmt::Let {
                    name: temp.clone(),
                    mutable: false,
                    ty: None,
                    expr: source.clone(),
                    span,
                });
                let base = Expr::var(temp, source.span);
                for (field, item) in fields {
                    let field_expr = Expr {
                        kind: ExprKind::FieldAccess {
                            base: Box::new(base.clone()),
                            field,
                        },
                        span: source.span,
                    };
                    self.expand_tuple_let_pattern(item, field_expr, span, out);
                }
            }
        }
    }

    fn next_tuple_let_name(&mut self) -> String {
        let id = self.tuple_binding_counter;
        self.tuple_binding_counter += 1;
        format!("{TUPLE_LET_TMP_PREFIX}{id}")
    }

    fn parse_assign_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        let (target, _) = self.expect_ident("E1060", "expected assignment target")?;
        let path = self.parse_place_path()?;
        let Some(op) = assignment_operator(&self.current().kind) else {
            self.diagnostics.push(Diagnostic::error(
                "E1061",
                "expected assignment operator ('=', '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '<<=', '>>=', '>>>=')",
                self.file,
                self.current_span(),
            ));
            return None;
        };
        self.bump();
        let expr = self.parse_expr()?;
        let end = if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            let span = self.current_span();
            self.bump();
            span.end
        } else {
            let span = self.current_span();
            self.diagnostics.push(
                Diagnostic::error("E1062", "expected ';' after assignment", self.file, span)
                    .with_fix(crate::diagnostics::SuggestedFix {
                        message: "insert ';' after assignment".to_string(),
                        replacement: Some(";".to_string()),
                        start: Some(expr.span.end),
                        end: Some(expr.span.end),
                    }),
            );
            expr.span.end
        };
        Some(Stmt::Assign {
            target,
            path,
            op,
            expr,
            span: Span::new(start, end),
        })
    }

    /// Parses the projections after an assignment target: `.field`, `.0`
    /// (tuple element) and `[index]`.
    fn parse_place_path(&mut self) -> Option<Vec<PlaceProjection>> {
        let mut path = Vec::new();
        loop {
            if self.at_kind(|k| matches!(k, TokenKind::Dot)) {
                self.bump();
                let token = self.current().clone();
                match token.kind {
                    TokenKind::Ident(name) => {
                        self.bump();
                        path.push(PlaceProjection::Field {
                            name,
                            span: token.span,
                        });
                    }
                    TokenKind::Int(literal) if literal.value >= 0 && literal.suffix.is_none() => {
                        self.bump();
                        path.push(PlaceProjection::Field {
                            name: literal.value.to_string(),
                            span: token.span,
                        });
                    }
                    TokenKind::Float(literal) => {
                        // `t.0.1` lexes `0.1` as one float token.
                        let Some((first, second)) = split_nested_tuple_index(&literal) else {
                            self.diagnostics.push(Diagnostic::error(
                                "E1036",
                                "expected field name or tuple index after '.'",
                                self.file,
                                token.span,
                            ));
                            return None;
                        };
                        self.bump();
                        path.push(PlaceProjection::Field {
                            name: first,
                            span: token.span,
                        });
                        path.push(PlaceProjection::Field {
                            name: second,
                            span: token.span,
                        });
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E1036",
                            "expected field name or tuple index after '.'",
                            self.file,
                            token.span,
                        ));
                        return None;
                    }
                }
                continue;
            }
            if self.at_kind(|k| matches!(k, TokenKind::LBracket)) {
                let open = self.current_span();
                self.bump();
                let index = self.parse_expr()?;
                let close = self.expect(
                    |k| matches!(k, TokenKind::RBracket),
                    "E1036",
                    "expected ']' after index",
                )?;
                path.push(PlaceProjection::Index {
                    index,
                    span: Span::new(open.start, close.end),
                });
                continue;
            }
            return Some(path);
        }
    }

    /// Parses `return [expr]` at statement position. With a trailing `;` it
    /// is a return statement. Directly before the block's closing `}` it is the
    /// block's tail expression (`if x > 0 { x } else { return 0 }`).
    fn parse_return_in_block(&mut self) -> Option<ReturnInBlock> {
        let start = self.current_span().start;
        let ret = self.parse_return_expr()?;
        let ExprKind::Return { expr } = ret.kind else {
            unreachable!("parse_return_expr builds a return expression");
        };
        if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            let end = self.current_span().end;
            self.bump();
            return Some(ReturnInBlock::Stmt(Stmt::Return {
                expr: expr.map(|expr| *expr),
                span: Span::new(start, end),
            }));
        }
        if self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            return Some(ReturnInBlock::Tail(Expr {
                kind: ExprKind::Return { expr },
                span: ret.span,
            }));
        }
        let span = self.current_span();
        let insert_at = expr.as_ref().map(|e| e.span.end).unwrap_or(ret.span.end);
        self.diagnostics.push(
            Diagnostic::error("E1034", "expected ';' after return", self.file, span).with_fix(
                crate::diagnostics::SuggestedFix {
                    message: "insert ';' after return".to_string(),
                    replacement: Some(";".to_string()),
                    start: Some(insert_at),
                    end: Some(insert_at),
                },
            ),
        );
        Some(ReturnInBlock::Stmt(Stmt::Return {
            expr: expr.map(|expr| *expr),
            span: Span::new(start, insert_at),
        }))
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_range()
    }

    /// Range expressions bind loosest: `a..b`, `a..=b`, `a..`, `..b`,
    /// `..=b`, `..`. Each is a builtin call (`ast::RANGE_BUILTIN` etc.).
    fn parse_range(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        let lower = if self.at_kind(|k| matches!(k, TokenKind::DotDot | TokenKind::DotDotEq)) {
            None
        } else {
            let lhs = self.parse_or()?;
            if !self.at_kind(|k| matches!(k, TokenKind::DotDot | TokenKind::DotDotEq)) {
                return Some(lhs);
            }
            Some(lhs)
        };
        let inclusive = self.at_kind(|k| matches!(k, TokenKind::DotDotEq));
        let op_end = self.current_span().end;
        self.bump(); // .. or ..=
        let upper = if self.range_end_follows() {
            Some(self.parse_or()?)
        } else {
            None
        };
        let (callee, args) = match (lower, upper, inclusive) {
            (Some(lo), Some(hi), false) => (RANGE_BUILTIN, vec![lo, hi]),
            (Some(lo), Some(hi), true) => (RANGE_INCLUSIVE_BUILTIN, vec![lo, hi]),
            (Some(lo), None, false) => (RANGE_FROM_BUILTIN, vec![lo]),
            (None, Some(hi), false) => (RANGE_TO_BUILTIN, vec![hi]),
            (None, Some(hi), true) => (RANGE_TO_INCLUSIVE_BUILTIN, vec![hi]),
            (None, None, false) => (RANGE_FULL_BUILTIN, Vec::new()),
            (_, None, true) => {
                self.diagnostics.push(Diagnostic::error(
                    "E1310",
                    "`..=` needs an end value",
                    self.file,
                    Span::new(start, op_end),
                ));
                return None;
            }
        };
        let end = args.last().map_or(op_end, |arg| arg.span.end);
        let span = Span::new(start, end);
        Some(Expr {
            kind: ExprKind::Call {
                callee: Box::new(Expr::var(callee.to_string(), Span::new(start, op_end))),
                args,
                arg_names: Vec::new(),
                type_args: Vec::new(),
            },
            span,
        })
    }

    /// Whether an end value follows `..` (not a closing bracket, separator
    /// or the block of a `for`/`if`/`while` header).
    fn range_end_follows(&self) -> bool {
        !self.at_kind(|k| {
            matches!(
                k,
                TokenKind::RBracket
                    | TokenKind::RParen
                    | TokenKind::RBrace
                    | TokenKind::Comma
                    | TokenKind::Semi
                    | TokenKind::FatArrow
                    | TokenKind::Eof
            )
        }) && !(self.disallow_block_expr && self.at_kind(|k| matches!(k, TokenKind::LBrace)))
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut expr = self.parse_and()?;
        while self.at_kind(|k| matches!(k, TokenKind::OrOr)) {
            self.bump();
            let rhs = self.parse_and()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let mut expr = self.parse_bit_or()?;
        while self.at_kind(|k| matches!(k, TokenKind::AndAnd)) {
            self.bump();
            let rhs = self.parse_bit_or()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_bit_or(&mut self) -> Option<Expr> {
        let mut expr = self.parse_bit_xor()?;
        while self.at_kind(|k| matches!(k, TokenKind::Pipe)) {
            self.bump();
            let rhs = self.parse_bit_xor()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::BitOr,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_bit_xor(&mut self) -> Option<Expr> {
        let mut expr = self.parse_bit_and()?;
        while self.at_kind(|k| matches!(k, TokenKind::Caret)) {
            self.bump();
            let rhs = self.parse_bit_and()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::BitXor,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_bit_and(&mut self) -> Option<Expr> {
        let mut expr = self.parse_equality()?;
        while self.at_kind(|k| matches!(k, TokenKind::Ampersand)) {
            self.bump();
            let rhs = self.parse_equality()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::BitAnd,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_equality(&mut self) -> Option<Expr> {
        let mut expr = self.parse_comparison()?;
        loop {
            let op = if self.at_kind(|k| matches!(k, TokenKind::EqEq)) {
                Some(BinOp::Eq)
            } else if self.at_kind(|k| matches!(k, TokenKind::Ne)) {
                Some(BinOp::Ne)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_comparison()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_comparison(&mut self) -> Option<Expr> {
        let mut expr = self.parse_shift()?;
        loop {
            let op = if self.at_kind(|k| matches!(k, TokenKind::Lt)) {
                Some(BinOp::Lt)
            } else if self.at_kind(|k| matches!(k, TokenKind::Le)) {
                Some(BinOp::Le)
            } else if self.at_kind(|k| matches!(k, TokenKind::Gt)) {
                Some(BinOp::Gt)
            } else if self.at_kind(|k| matches!(k, TokenKind::Ge)) {
                Some(BinOp::Ge)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_shift()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_shift(&mut self) -> Option<Expr> {
        let mut expr = self.parse_term()?;
        loop {
            let op = if self.at_kind(|k| matches!(k, TokenKind::LShift)) {
                Some(BinOp::Shl)
            } else if self.at_kind(|k| matches!(k, TokenKind::RShift)) {
                Some(BinOp::Shr)
            } else if self.at_kind(|k| matches!(k, TokenKind::URShift)) {
                Some(BinOp::Ushr)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_term()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_term(&mut self) -> Option<Expr> {
        let mut expr = self.parse_factor()?;
        loop {
            let op = if self.at_kind(|k| matches!(k, TokenKind::Plus)) {
                Some(BinOp::Add)
            } else if self.at_kind(|k| matches!(k, TokenKind::Minus)) {
                Some(BinOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_factor()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_factor(&mut self) -> Option<Expr> {
        let mut expr = self.parse_cast()?;
        loop {
            let op = if self.at_kind(|k| matches!(k, TokenKind::Star)) {
                Some(BinOp::Mul)
            } else if self.at_kind(|k| matches!(k, TokenKind::Slash)) {
                Some(BinOp::Div)
            } else if self.at_kind(|k| matches!(k, TokenKind::Percent)) {
                Some(BinOp::Mod)
            } else {
                None
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_cast()?;
            let span = expr.span.join(rhs.span);
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(expr),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(expr)
    }

    /// `operand as Type`: binds tighter than binary operators and looser
    /// than unary ones (`-x as Int64` is `(-x) as Int64`). `as` is a
    /// contextual keyword, so it stays usable as an identifier elsewhere.
    /// The cast is the builtin call `$as:Type(operand)`.
    fn parse_cast(&mut self) -> Option<Expr> {
        let mut expr = self.parse_unary()?;
        while matches!(&self.current().kind, TokenKind::Ident(name) if name == "as") {
            self.bump();
            let ty = self.parse_type()?;
            let span = Span::new(expr.span.start, ty.span.end);
            let callee = format!("{}{}", CAST_BUILTIN_PREFIX, type_repr_text(&ty));
            expr = Expr {
                kind: ExprKind::Call {
                    callee: Box::new(Expr::var(callee, ty.span)),
                    args: vec![expr],
                    arg_names: Vec::new(),
                    type_args: Vec::new(),
                },
                span,
            };
        }
        Some(expr)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.at_kind(|k| matches!(k, TokenKind::Pipe | TokenKind::OrOr)) {
            return self.parse_closure_expr();
        }
        if self.at_kind(|k| matches!(k, TokenKind::Ampersand)) {
            let start = self.current_span().start;
            self.bump();
            let mutable = if self.at_kind(|k| matches!(k, TokenKind::KwMut)) {
                self.bump();
                true
            } else {
                false
            };
            let expr = self.parse_unary()?;
            return Some(Expr {
                span: Span::new(start, expr.span.end),
                kind: ExprKind::Borrow {
                    mutable,
                    expr: Box::new(expr),
                },
            });
        }
        if self.at_kind(|k| matches!(k, TokenKind::KwAwait)) {
            let start = self.current_span().start;
            self.bump();
            let expr = self.parse_unary()?;
            return Some(Expr {
                span: Span::new(start, expr.span.end),
                kind: ExprKind::Await {
                    expr: Box::new(expr),
                },
            });
        }
        if self.at_kind(|k| matches!(k, TokenKind::Minus)) {
            let start = self.current_span().start;
            self.bump();
            let expr = self.parse_unary()?;
            return Some(Expr {
                span: Span::new(start, expr.span.end),
                kind: ExprKind::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                },
            });
        }
        if self.at_kind(|k| matches!(k, TokenKind::Bang)) {
            let start = self.current_span().start;
            self.bump();
            let expr = self.parse_unary()?;
            return Some(Expr {
                span: Span::new(start, expr.span.end),
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                },
            });
        }
        if self.at_kind(|k| matches!(k, TokenKind::Tilde)) {
            let start = self.current_span().start;
            self.bump();
            let expr = self.parse_unary()?;
            return Some(Expr {
                span: Span::new(start, expr.span.end),
                kind: ExprKind::Unary {
                    op: UnaryOp::BitNot,
                    expr: Box::new(expr),
                },
            });
        }
        self.parse_postfix()
    }

    fn parse_closure_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        let mut params = Vec::new();
        if self.at_kind(|k| matches!(k, TokenKind::OrOr)) {
            self.bump();
        } else {
            self.expect(
                |k| matches!(k, TokenKind::Pipe),
                "E1071",
                "expected '|' to start closure expression",
            )?;

            while !self.at_kind(|k| matches!(k, TokenKind::Pipe)) {
                let param_start = self.current_span().start;
                let (name, name_span) =
                    self.expect_ident("E1072", "expected closure parameter name")?;
                let ty = if self.at_kind(|k| matches!(k, TokenKind::Colon)) {
                    self.bump();
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let param_end = ty.as_ref().map(|t| t.span.end).unwrap_or(name_span.end);
                params.push(ClosureParam {
                    name,
                    ty,
                    span: Span::new(param_start, param_end),
                });
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                } else {
                    break;
                }
            }

            self.expect(
                |k| matches!(k, TokenKind::Pipe),
                "E1073",
                "expected '|' to end closure parameters",
            )?;
        }
        // `-> T { .. }` annotates the return type; otherwise the body is a
        // block or a single expression and the return type is inferred.
        let (ret_type, body) = if self.at_kind(|k| matches!(k, TokenKind::Arrow)) {
            self.bump();
            let ret_type = self.parse_type()?;
            if !self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
                let span = self.current_span();
                self.diagnostics.push(Diagnostic::error(
                    "E1074",
                    "a closure with a return type needs a block body: `|x| -> T { .. }`",
                    self.file,
                    span,
                ));
                return None;
            }
            (Some(ret_type), self.parse_block()?)
        } else if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
            (None, self.parse_block()?)
        } else {
            let value = self.parse_nested_expr()?;
            let span = value.span;
            (
                None,
                Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(value)),
                    span,
                },
            )
        };
        let end = body.span.end;
        Some(Expr {
            kind: ExprKind::Closure {
                params,
                ret_type,
                body,
            },
            span: Span::new(start, end),
        })
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        // `::[T, U]` before a call's arguments: explicit generic arguments.
        let mut type_args: Option<Vec<TypeExpr>> = None;
        loop {
            if type_args.is_none()
                && self.at_kind(|k| matches!(k, TokenKind::ColonColon))
                && matches!(
                    self.peek(1).map(|token| &token.kind),
                    Some(TokenKind::LBracket)
                )
            {
                self.bump(); // ::
                self.bump(); // [
                let mut types = Vec::new();
                while !self.at_kind(|k| matches!(k, TokenKind::RBracket)) {
                    types.push(self.parse_type()?);
                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let close = self.expect(
                    |k| matches!(k, TokenKind::RBracket),
                    "E1312",
                    "expected ']' after explicit type arguments",
                )?;
                if types.is_empty() || !self.at_kind(|k| matches!(k, TokenKind::LParen)) {
                    self.diagnostics.push(Diagnostic::error(
                        "E1312",
                        if types.is_empty() {
                            "explicit type arguments `::[..]` need at least one type"
                        } else {
                            "explicit type arguments `::[..]` must be followed by call arguments"
                        },
                        self.file,
                        Span::new(expr.span.end, close.end),
                    ));
                    return None;
                }
                type_args = Some(types);
                continue;
            }
            if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
                self.bump();
                let mut args = Vec::new();
                let mut arg_names: Vec<Option<String>> = Vec::new();
                let mut saw_named = false;
                while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                    let mut parsed_named = false;
                    if let TokenKind::Ident(name) = self.current().kind.clone() {
                        if self
                            .peek(1)
                            .map(|tok| matches!(tok.kind, TokenKind::Colon))
                            .unwrap_or(false)
                        {
                            self.bump();
                            self.bump();
                            let value = self.parse_nested_expr()?;
                            if arg_names.is_empty() && !args.is_empty() {
                                arg_names.resize(args.len(), None);
                            }
                            args.push(value);
                            arg_names.push(Some(name));
                            saw_named = true;
                            parsed_named = true;
                        }
                    }

                    if !parsed_named {
                        let arg = self.parse_nested_expr()?;
                        if saw_named {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "E1092",
                                    "positional arguments cannot follow named arguments",
                                    self.file,
                                    arg.span,
                                )
                                .with_help(
                                    "place positional arguments first, then named arguments",
                                ),
                            );
                        }
                        args.push(arg);
                        if !arg_names.is_empty() {
                            arg_names.push(None);
                        }
                    }

                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let close = self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1035",
                    "expected ')' after function arguments",
                )?;
                if arg_names.iter().all(|name| name.is_none()) {
                    arg_names.clear();
                }
                let span = Span::new(expr.span.start, close.end);
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                        arg_names,
                        type_args: type_args.take().unwrap_or_default(),
                    },
                    span,
                };
                continue;
            }

            if self.at_kind(|k| matches!(k, TokenKind::Dot)) {
                self.bump();
                let (field, field_span) = match self.current().kind.clone() {
                    TokenKind::Ident(name) => {
                        let span = self.current_span();
                        self.bump();
                        (name, span)
                    }
                    TokenKind::Int(literal) if literal.value >= 0 && literal.suffix.is_none() => {
                        let span = self.current_span();
                        self.bump();
                        (literal.value.to_string(), span)
                    }
                    TokenKind::Float(literal) => {
                        // `t.0.1` lexes `0.1` as one float token.
                        let span = self.current_span();
                        let Some((first, second)) = split_nested_tuple_index(&literal) else {
                            self.diagnostics.push(Diagnostic::error(
                                "E1036",
                                "expected field name or tuple index after '.'",
                                self.file,
                                span,
                            ));
                            return None;
                        };
                        self.bump();
                        let inner_span = Span::new(expr.span.start, span.end);
                        expr = Expr {
                            kind: ExprKind::FieldAccess {
                                base: Box::new(expr),
                                field: first,
                            },
                            span: inner_span,
                        };
                        (second, span)
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E1036",
                            "expected field name or tuple index after '.'",
                            self.file,
                            self.current_span(),
                        ));
                        return None;
                    }
                };
                let span = Span::new(expr.span.start, field_span.end);
                expr = Expr {
                    kind: ExprKind::FieldAccess {
                        base: Box::new(expr),
                        field,
                    },
                    span,
                };
                continue;
            }

            if self.at_kind(|k| matches!(k, TokenKind::LBracket)) {
                // `base[index]` is the builtin `$index(base, index)`.
                self.bump();
                let index = self.parse_nested_expr()?;
                let close = self.expect(
                    |k| matches!(k, TokenKind::RBracket),
                    "E1035",
                    "expected ']' after index expression",
                )?;
                let span = Span::new(expr.span.start, close.end);
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(Expr::var(INDEX_BUILTIN.to_string(), span)),
                        args: vec![expr, index],
                        arg_names: Vec::new(),
                        type_args: Vec::new(),
                    },
                    span,
                };
                continue;
            }

            if self.at_kind(|k| matches!(k, TokenKind::Question)) {
                let end = self.current_span().end;
                self.bump();
                expr = Expr {
                    span: Span::new(expr.span.start, end),
                    kind: ExprKind::Try {
                        expr: Box::new(expr),
                    },
                };
                continue;
            }

            break;
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Int(literal) => {
                self.bump();
                let meta = Self::typed_int_literal_metadata(
                    literal.suffix,
                    literal.raw_value_span,
                    literal.raw_text,
                );
                Some(Expr {
                    kind: ExprKind::Int(literal.value, meta),
                    span: token.span,
                })
            }
            TokenKind::Float(literal) => {
                self.bump();
                let float_value = literal.value;
                let meta = Self::typed_float_literal_metadata(
                    literal.suffix,
                    literal.raw_value_span,
                    literal.raw_text,
                );
                Some(Expr {
                    kind: ExprKind::Float(float_value, meta),
                    span: token.span,
                })
            }
            TokenKind::String(value) => {
                self.bump();
                Some(Expr {
                    kind: ExprKind::String(value),
                    span: token.span,
                })
            }
            TokenKind::Template(raw) => {
                self.bump();
                self.parse_template_literal(&raw, token.span)
            }
            TokenKind::Char(value) => {
                self.bump();
                Some(Expr {
                    kind: ExprKind::Char(value),
                    span: token.span,
                })
            }
            TokenKind::KwTrue => {
                self.bump();
                Some(Expr {
                    kind: ExprKind::Bool(true),
                    span: token.span,
                })
            }
            TokenKind::KwFalse => {
                self.bump();
                Some(Expr {
                    kind: ExprKind::Bool(false),
                    span: token.span,
                })
            }
            TokenKind::KwNull => {
                self.bump();
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1051",
                        "null is not a language value; use Option and None/Some instead",
                        self.file,
                        token.span,
                    )
                    .with_help("replace `null` with `None` or a concrete `Some(...)` value"),
                );
                Some(Expr {
                    kind: ExprKind::Unit,
                    span: token.span,
                })
            }
            TokenKind::LParen => {
                self.bump();
                if self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                    let end = self.current_span().end;
                    self.bump();
                    return Some(Expr {
                        kind: ExprKind::Unit,
                        span: Span::new(token.span.start, end),
                    });
                }
                let first = self.parse_nested_expr()?;
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                    let mut items = vec![first];
                    while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                        let item = self.parse_nested_expr()?;
                        items.push(item);
                        if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let close = self.expect(
                        |k| matches!(k, TokenKind::RParen),
                        "E1037",
                        "expected ')' after tuple expression",
                    )?;
                    let fields = items
                        .into_iter()
                        .enumerate()
                        .map(|(idx, item)| (idx.to_string(), item.clone(), item.span))
                        .collect::<Vec<_>>();
                    return Some(Expr {
                        kind: ExprKind::StructInit {
                            name: TUPLE_INTERNAL_NAME.to_string(),
                            fields,
                        },
                        span: Span::new(token.span.start, close.end),
                    });
                }
                let close = self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1037",
                    "expected ')' after expression",
                )?;
                Some(Expr {
                    kind: first.kind,
                    span: Span::new(token.span.start, close.end),
                })
            }
            TokenKind::KwIf => self.parse_if_expr(),
            TokenKind::Label(_) => self.parse_labeled_loop(),
            TokenKind::KwFor => self.parse_for_expr(),
            TokenKind::KwWhile => self.parse_while_expr(),
            TokenKind::KwLoop => self.parse_loop_expr(),
            TokenKind::KwBreak => self.parse_break_expr(),
            TokenKind::KwContinue => self.parse_continue_expr(),
            TokenKind::KwReturn => self.parse_return_expr(),
            TokenKind::KwMatch => self.parse_match_expr(),
            TokenKind::KwUnsafe => {
                self.bump();
                let block = self.parse_block()?;
                Some(Expr {
                    span: Span::new(token.span.start, block.span.end),
                    kind: ExprKind::UnsafeBlock { block },
                })
            }
            TokenKind::LBracket => {
                // `[a, b, c]` is the builtin `$vec_literal(a, b, c)`.
                self.bump();
                let mut items = Vec::new();
                while !self.at_kind(|k| matches!(k, TokenKind::RBracket)) {
                    items.push(self.parse_nested_expr()?);
                    if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let close = self.expect(
                    |k| matches!(k, TokenKind::RBracket),
                    "E1037",
                    "expected ']' after list items",
                )?;
                let span = Span::new(token.span.start, close.end);
                Some(Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(Expr::var(VEC_LITERAL_BUILTIN.to_string(), span)),
                        args: items,
                        arg_names: Vec::new(),
                        type_args: Vec::new(),
                    },
                    span,
                })
            }
            TokenKind::LBrace if !self.disallow_block_expr => {
                let block = self.parse_block()?;
                Some(Expr {
                    span: block.span,
                    kind: ExprKind::Block { block },
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                let mut full_name = name;
                let mut end = token.span.end;
                // `f::[Int](x)` ends the path at `::[`.
                while self.at_kind(|k| matches!(k, TokenKind::ColonColon))
                    && !matches!(
                        self.peek(1).map(|token| &token.kind),
                        Some(TokenKind::LBracket)
                    )
                {
                    self.bump();
                    let (segment, seg_span) =
                        self.expect_ident("E1088", "expected path segment after '::'")?;
                    full_name.push_str("::");
                    full_name.push_str(&segment);
                    end = seg_span.end;
                }
                // `Enum::Variant { .. }` builds a struct-like variant; right
                // after an `if`/`while` condition or `match` scrutinee the
                // `{` starts the following block instead.
                if !self.disallow_struct_literal
                    && !(full_name.contains("::") && self.disallow_block_expr)
                    && self.at_kind(|k| matches!(k, TokenKind::LBrace))
                    && self.looks_like_struct_literal()
                {
                    self.bump();
                    let mut fields = Vec::new();
                    while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
                        let (field_name, field_span) =
                            self.expect_ident("E1038", "expected struct field name")?;
                        self.expect(
                            |k| matches!(k, TokenKind::Colon),
                            "E1039",
                            "expected ':' after field name",
                        )?;
                        let value = self.parse_expr()?;
                        fields.push((field_name, value.clone(), field_span.join(value.span)));
                        if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let close = self.expect(
                        |k| matches!(k, TokenKind::RBrace),
                        "E1040",
                        "expected '}' after struct literal",
                    )?;
                    return Some(Expr {
                        kind: ExprKind::StructInit {
                            name: full_name,
                            fields,
                        },
                        span: Span::new(token.span.start, close.end),
                    });
                }
                Some(Expr {
                    kind: ExprKind::Var(full_name),
                    span: Span::new(token.span.start, end),
                })
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error("E1041", "expected expression", self.file, token.span)
                        .with_fix(crate::diagnostics::SuggestedFix {
                            message: "insert an expression".to_string(),
                            replacement: Some("0".to_string()),
                            start: Some(token.span.start),
                            end: Some(token.span.start),
                        }),
                );
                None
            }
        }
    }

    fn parse_if_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // if
        if self.at_kind(|k| matches!(k, TokenKind::KwLet)) {
            return self.parse_if_let_expr(start);
        }
        let cond = self.parse_expr_before_block()?;
        let then_block = self.parse_block()?;
        if !self.at_kind(|k| matches!(k, TokenKind::KwElse)) {
            // `if cond { ... }` without `else`: the missing branch is an
            // empty block, so the `if` has type `()`.
            let end = then_block.span.end;
            return Some(Expr {
                kind: ExprKind::If {
                    cond: Box::new(cond),
                    then_block,
                    else_block: Block {
                        stmts: Vec::new(),
                        tail: None,
                        span: Span::new(end, end),
                    },
                },
                span: Span::new(start, end),
            });
        }
        self.bump(); // else

        let else_block = if self.at_kind(|k| matches!(k, TokenKind::KwIf)) {
            let nested = self.parse_if_expr()?;
            Block {
                span: nested.span,
                stmts: Vec::new(),
                tail: Some(Box::new(nested)),
            }
        } else {
            self.parse_block()?
        };

        let end = else_block.span.end;
        Some(Expr {
            kind: ExprKind::If {
                cond: Box::new(cond),
                then_block,
                else_block,
            },
            span: Span::new(start, end),
        })
    }

    fn parse_for_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // for
        let (binding, _) = self.expect_ident("E1031", "expected loop binding name after `for`")?;
        self.expect(
            |k| matches!(k, TokenKind::KwIn),
            "E1041",
            "expected `in` after loop binding in `for` expression",
        )?;

        let iterable = self.parse_expr_without_struct_literals()?;
        let body = self.parse_block()?;
        // `for i in a..b` and `for i in a..=b` count directly; other
        // iterables (including `Range` values) use the iterator protocol.
        match split_range_call(iterable) {
            Ok((lo, hi, inclusive)) => {
                Some(self.desugar_for_range(binding, lo, hi, inclusive, body, start))
            }
            Err(iterable) => Some(self.desugar_for_iter(binding, iterable, body, start)),
        }
    }

    /// `if let P = e { .. } [else ..]` after `if`: desugars to
    /// `match e { P => { .. }, _$if_let_else => { .. } }`.
    fn parse_if_let_expr(&mut self, start: usize) -> Option<Expr> {
        self.bump(); // let
        let pattern = self.parse_pattern()?;
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1032",
            "expected '=' after the pattern in `if let`",
        )?;
        let scrutinee = self.parse_expr_without_struct_literals()?;
        let then_block = self.parse_block()?;
        let else_block = if self.at_kind(|k| matches!(k, TokenKind::KwElse)) {
            self.bump();
            if self.at_kind(|k| matches!(k, TokenKind::KwIf)) {
                let nested = self.parse_if_expr()?;
                Block {
                    span: nested.span,
                    stmts: Vec::new(),
                    tail: Some(Box::new(nested)),
                }
            } else {
                self.parse_block()?
            }
        } else {
            let end = then_block.span.end;
            Block {
                stmts: Vec::new(),
                tail: None,
                span: Span::new(end, end),
            }
        };
        let end = else_block.span.end;
        let then_span = then_block.span;
        let else_span = else_block.span;
        Some(Expr {
            kind: ExprKind::Match {
                expr: Box::new(scrutinee),
                arms: vec![
                    MatchArm {
                        span: Span::new(pattern.span.start, then_span.end),
                        pattern,
                        guard: None,
                        body: Expr {
                            kind: ExprKind::Block { block: then_block },
                            span: then_span,
                        },
                    },
                    MatchArm {
                        pattern: Pattern {
                            kind: PatternKind::Var(IF_LET_ELSE_BINDING.to_string()),
                            span: else_span,
                        },
                        guard: None,
                        body: Expr {
                            kind: ExprKind::Block { block: else_block },
                            span: else_span,
                        },
                        span: else_span,
                    },
                ],
            },
            span: Span::new(start, end),
        })
    }

    fn parse_while_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // while
        if self.at_kind(|k| matches!(k, TokenKind::KwLet)) {
            return self.parse_while_let_expr(start);
        }
        let cond = self.parse_expr_before_block()?;
        let body = self.parse_block()?;
        let end = body.span.end;
        Some(Expr {
            kind: ExprKind::While {
                label: None,
                cond: Box::new(cond),
                body,
            },
            span: Span::new(start, end),
        })
    }

    /// `while let P = e { .. }` after `while`: desugars to
    /// `loop { match e { P => { .. }, _$while_let_end => break } }`.
    fn parse_while_let_expr(&mut self, start: usize) -> Option<Expr> {
        self.bump(); // let
        let pattern = self.parse_pattern()?;
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1032",
            "expected '=' after the pattern in `while let`",
        )?;
        let scrutinee = self.parse_expr_without_struct_literals()?;
        let body = self.parse_block()?;
        let span = Span::new(start, body.span.end);
        let body_span = body.span;
        let end_marker = Span::new(body_span.end, body_span.end);
        let matched = Expr {
            kind: ExprKind::Match {
                expr: Box::new(scrutinee),
                arms: vec![
                    MatchArm {
                        span: Span::new(pattern.span.start, body_span.end),
                        pattern,
                        guard: None,
                        body: Expr {
                            kind: ExprKind::Block { block: body },
                            span: body_span,
                        },
                    },
                    MatchArm {
                        pattern: Pattern {
                            kind: PatternKind::Var(WHILE_LET_END_BINDING.to_string()),
                            span: end_marker,
                        },
                        guard: None,
                        body: Expr {
                            kind: ExprKind::Break {
                                expr: None,
                                label: None,
                            },
                            span: end_marker,
                        },
                        span: end_marker,
                    },
                ],
            },
            span,
        };
        Some(Expr {
            kind: ExprKind::Loop {
                label: None,
                body: Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(matched)),
                    span,
                },
            },
            span,
        })
    }

    fn parse_loop_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // loop
        let body = self.parse_block()?;
        let end = body.span.end;
        Some(Expr {
            kind: ExprKind::Loop { body, label: None },
            span: Span::new(start, end),
        })
    }

    /// `'label: while ..`, `'label: loop ..` or `'label: for ..`.
    fn parse_labeled_loop(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        let label = self.parse_loop_label()?;
        self.expect(
            |k| matches!(k, TokenKind::Colon),
            "E1306",
            "expected ':' after a loop label",
        )?;
        let mut expr = match &self.current().kind {
            TokenKind::KwWhile => self.parse_while_expr()?,
            TokenKind::KwLoop => self.parse_loop_expr()?,
            TokenKind::KwFor => self.parse_for_expr()?,
            _ => {
                let span = self.current_span();
                self.diagnostics.push(Diagnostic::error(
                    "E1306",
                    format!("label '{label} must be followed by `while`, `loop` or `for`"),
                    self.file,
                    span,
                ));
                return None;
            }
        };
        let labeled = set_loop_label(&mut expr, label);
        debug_assert!(labeled, "every loop form contains a loop");
        expr.span = Span::new(start, expr.span.end);
        Some(expr)
    }

    /// Consumes a `'label` token and returns its name.
    fn parse_loop_label(&mut self) -> Option<String> {
        let TokenKind::Label(name) = &self.current().kind else {
            return None;
        };
        let name = name.clone();
        self.bump();
        Some(name)
    }

    fn parse_break_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // break
        let label = if matches!(self.current().kind, TokenKind::Label(_)) {
            self.parse_loop_label()
        } else {
            None
        };
        let expr = if self.break_has_value_start() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let end = expr
            .as_ref()
            .map(|expr| expr.span.end)
            .unwrap_or_else(|| self.previous_span().end);
        Some(Expr {
            kind: ExprKind::Break { expr, label },
            span: Span::new(start, end),
        })
    }

    /// `return` used as an expression, for example `None => return 0,`.
    /// Statement-level `return expr;` is parsed by the statement parser.
    fn parse_return_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // return
        let expr = if self.break_has_value_start() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let end = expr
            .as_ref()
            .map(|expr| expr.span.end)
            .unwrap_or_else(|| self.previous_span().end);
        Some(Expr {
            kind: ExprKind::Return { expr },
            span: Span::new(start, end),
        })
    }

    fn parse_continue_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // continue
        let label = if matches!(self.current().kind, TokenKind::Label(_)) {
            self.parse_loop_label()
        } else {
            None
        };
        Some(Expr {
            kind: ExprKind::Continue { label },
            span: Span::new(start, self.previous_span().end),
        })
    }

    fn parse_template_literal(&mut self, raw: &str, token_span: Span) -> Option<Expr> {
        let content_offset = token_span.start.saturating_add(2);
        let mut template = String::new();
        let mut literal = String::new();
        let mut args = Vec::new();
        let mut cursor = 0usize;

        while cursor < raw.len() {
            let Some(ch) = raw[cursor..].chars().next() else {
                break;
            };
            match ch {
                '\\' => {
                    if let Some((escaped, consumed)) =
                        self.parse_template_escape(raw, cursor, token_span)
                    {
                        literal.push(escaped);
                        cursor += consumed;
                    } else {
                        return None;
                    }
                }
                '{' => {
                    if cursor + 1 < raw.len() && raw.as_bytes()[cursor + 1] == b'{' {
                        literal.push('{');
                        cursor += 2;
                        continue;
                    }

                    template.push_str(&literal);
                    literal.clear();

                    let expr_start = cursor + ch.len_utf8();
                    let mut depth = 0usize;
                    let mut idx = expr_start;
                    let mut in_string = false;
                    let mut in_char = false;
                    let mut escaped = false;
                    let mut closing = None;

                    while idx < raw.len() {
                        let next = raw[idx..]
                            .chars()
                            .next()
                            .expect("template interpolation cursor");
                        let step = next.len_utf8();

                        if in_string {
                            if escaped {
                                escaped = false;
                            } else if next == '\\' {
                                escaped = true;
                            } else if next == '"' {
                                in_string = false;
                            }
                            idx += step;
                            continue;
                        }

                        if in_char {
                            if escaped {
                                escaped = false;
                            } else if next == '\\' {
                                escaped = true;
                            } else if next == '\'' {
                                in_char = false;
                            }
                            idx += step;
                            continue;
                        }

                        match next {
                            '"' => in_string = true,
                            '\'' => in_char = true,
                            '{' => depth += 1,
                            '}' => {
                                if depth == 0 {
                                    closing = Some(idx);
                                    break;
                                }
                                depth -= 1;
                            }
                            _ => {}
                        }
                        idx += step;
                    }

                    let Some(expr_end) = closing else {
                        self.diagnostics.push(Diagnostic::error(
                            "E1001",
                            "unterminated interpolation expression",
                            self.file,
                            Span::new(content_offset + cursor, token_span.end),
                        ));
                        return None;
                    };

                    let expr_src = raw[expr_start..expr_end].trim();
                    if expr_src.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            "E1001",
                            "empty interpolation expression",
                            self.file,
                            Span::new(content_offset + expr_start, content_offset + expr_end),
                        ));
                        return None;
                    }

                    let interp_span =
                        Span::new(content_offset + expr_start, content_offset + expr_end);
                    let expr = self.parse_template_expr_fragment(expr_src, interp_span)?;
                    template.push_str(&format!("{{{}}}", args.len()));
                    args.push(expr);

                    cursor = expr_end + 1;
                }
                '}' => {
                    if cursor + 1 < raw.len() && raw.as_bytes()[cursor + 1] == b'}' {
                        literal.push('}');
                        cursor += 2;
                        continue;
                    }

                    self.diagnostics.push(Diagnostic::error(
                        "E1001",
                        "unescaped '}' in template string; use '\\}' or '}}' for a literal brace",
                        self.file,
                        Span::new(content_offset + cursor, content_offset + cursor + 1),
                    ));
                    return None;
                }
                _ => {
                    literal.push(ch);
                    cursor += ch.len_utf8();
                }
            }
        }

        template.push_str(&literal);
        if args.is_empty() {
            return Some(Expr {
                kind: ExprKind::String(template),
                span: token_span,
            });
        }
        Some(Expr {
            kind: ExprKind::TemplateLiteral { template, args },
            span: token_span,
        })
    }

    fn parse_template_expr_fragment(&mut self, expr_src: &str, interp_span: Span) -> Option<Expr> {
        let (tokens, lex_diags) = lex(expr_src, self.file);
        for diag in lex_diags {
            self.diagnostics
                .push(Self::shift_diagnostic(diag, interp_span.start));
        }

        let mut parser = Parser::new(self.file, tokens);
        parser.for_counter = self.for_counter;
        parser.tuple_binding_counter = self.tuple_binding_counter;

        let expr = parser.parse_expr();
        if expr.is_some() && !parser.at_kind(|k| matches!(k, TokenKind::Eof)) {
            parser.diagnostics.push(Diagnostic::error(
                "E1001",
                "expected end of interpolation expression",
                self.file,
                parser.current_span(),
            ));
        }

        for diag in parser.diagnostics {
            self.diagnostics
                .push(Self::shift_diagnostic(diag, interp_span.start));
        }

        self.for_counter = parser.for_counter;
        self.tuple_binding_counter = parser.tuple_binding_counter;

        let mut expr = expr?;
        expr.span = interp_span;
        Some(expr)
    }

    fn shift_diagnostic(mut diag: Diagnostic, offset: usize) -> Diagnostic {
        for span in &mut diag.spans {
            span.start = span.start.saturating_add(offset);
            span.end = span.end.saturating_add(offset);
        }
        for fix in &mut diag.suggested_fixes {
            if let Some(start) = fix.start.as_mut() {
                *start = start.saturating_add(offset);
            }
            if let Some(end) = fix.end.as_mut() {
                *end = end.saturating_add(offset);
            }
        }
        diag
    }

    fn parse_template_escape(
        &mut self,
        raw: &str,
        index: usize,
        token_span: Span,
    ) -> Option<(char, usize)> {
        let content_offset = token_span.start.saturating_add(2);
        let mut chars = raw[index..].chars();
        let slash = chars.next()?;
        if slash != '\\' {
            return None;
        }
        let Some(next) = chars.next() else {
            self.diagnostics.push(Diagnostic::error(
                "E0006",
                "unterminated template string literal",
                self.file,
                Span::new(content_offset + index, token_span.end),
            ));
            return None;
        };

        let simple = match next {
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            '0' => Some('\0'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '\\' => Some('\\'),
            '{' => Some('{'),
            '}' => Some('}'),
            _ => None,
        };
        if let Some(value) = simple {
            return Some((value, 1 + next.len_utf8()));
        }

        if next != 'u' {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                format!("unsupported escape sequence '\\\\{}'", next),
                self.file,
                Span::new(
                    content_offset + index,
                    content_offset + index + 1 + next.len_utf8(),
                ),
            ));
            return None;
        }

        let mut cursor = index + 1 + next.len_utf8();
        if cursor >= raw.len() || raw.as_bytes()[cursor] != b'{' {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                "invalid Unicode escape, expected `\\u{...}`",
                self.file,
                Span::new(
                    content_offset + index,
                    content_offset + cursor.min(raw.len()),
                ),
            ));
            return None;
        }
        cursor += 1;
        let digits_start = cursor;
        while cursor < raw.len() {
            let c = raw[cursor..]
                .chars()
                .next()
                .expect("template unicode escape cursor");
            if c.is_ascii_hexdigit() {
                cursor += c.len_utf8();
            } else {
                break;
            }
        }
        let digits_end = cursor;
        if cursor >= raw.len() || raw.as_bytes()[cursor] != b'}' {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                "invalid Unicode escape, expected closing `}`",
                self.file,
                Span::new(
                    content_offset + index,
                    content_offset + cursor.min(raw.len()),
                ),
            ));
            return None;
        }

        if digits_start == digits_end {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                "invalid Unicode escape, missing codepoint digits",
                self.file,
                Span::new(content_offset + index, content_offset + cursor + 1),
            ));
            return None;
        }

        let digits = &raw[digits_start..digits_end];
        if digits.len() > 6 {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                "invalid Unicode escape, codepoint has too many hex digits",
                self.file,
                Span::new(content_offset + index, content_offset + cursor + 1),
            ));
            return None;
        }

        let Some(codepoint) = u32::from_str_radix(digits, 16).ok() else {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                "invalid Unicode escape codepoint",
                self.file,
                Span::new(content_offset + index, content_offset + cursor + 1),
            ));
            return None;
        };
        let Some(ch) = char::from_u32(codepoint) else {
            self.diagnostics.push(Diagnostic::error(
                "E0005",
                "invalid Unicode codepoint in escape sequence",
                self.file,
                Span::new(content_offset + index, content_offset + cursor + 1),
            ));
            return None;
        };

        Some((ch, cursor + 1 - index))
    }
    fn parse_expr_without_struct_literals(&mut self) -> Option<Expr> {
        let prev = self.disallow_struct_literal;
        self.disallow_struct_literal = true;
        let expr = self.parse_expr_before_block();
        self.disallow_struct_literal = prev;
        expr
    }

    /// Parses an expression inside parentheses (call arguments, tuples):
    /// struct literals and block expressions are allowed again there.
    fn parse_nested_expr(&mut self) -> Option<Expr> {
        let struct_literal = std::mem::replace(&mut self.disallow_struct_literal, false);
        let block_expr = std::mem::replace(&mut self.disallow_block_expr, false);
        let expr = self.parse_expr();
        self.disallow_struct_literal = struct_literal;
        self.disallow_block_expr = block_expr;
        expr
    }

    /// Parses an expression that a block follows; see `disallow_block_expr`.
    fn parse_expr_before_block(&mut self) -> Option<Expr> {
        let prev = self.disallow_block_expr;
        self.disallow_block_expr = true;
        let expr = self.parse_expr();
        self.disallow_block_expr = prev;
        expr
    }

    fn next_for_id(&mut self) -> usize {
        let id = self.for_counter;
        self.for_counter += 1;
        id
    }

    fn make_for_name(&self, prefix: &str, id: usize) -> String {
        format!("aic_for_{prefix}_{id}")
    }

    fn make_bool_expr(&self, value: bool, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Bool(value),
            span,
        }
    }

    fn make_int_expr(&self, value: i64, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Int(value, None),
            span,
        }
    }

    fn int_literal_suffix_to_ast(
        suffix: IntLiteralSuffixToken,
    ) -> (IntLiteralSuffix, IntLiteralKind, &'static str) {
        match suffix {
            IntLiteralSuffixToken::I8 => (IntLiteralSuffix::I8, IntLiteralSuffix::I8.kind(), "i8"),
            IntLiteralSuffixToken::I16 => {
                (IntLiteralSuffix::I16, IntLiteralSuffix::I16.kind(), "i16")
            }
            IntLiteralSuffixToken::I32 => {
                (IntLiteralSuffix::I32, IntLiteralSuffix::I32.kind(), "i32")
            }
            IntLiteralSuffixToken::I64 => {
                (IntLiteralSuffix::I64, IntLiteralSuffix::I64.kind(), "i64")
            }
            IntLiteralSuffixToken::I128 => (
                IntLiteralSuffix::I64,
                IntLiteralKind {
                    signedness: IntLiteralSignedness::Signed,
                    width: IntLiteralWidth::W128,
                },
                "i128",
            ),
            IntLiteralSuffixToken::U8 => (IntLiteralSuffix::U8, IntLiteralSuffix::U8.kind(), "u8"),
            IntLiteralSuffixToken::U16 => {
                (IntLiteralSuffix::U16, IntLiteralSuffix::U16.kind(), "u16")
            }
            IntLiteralSuffixToken::U32 => {
                (IntLiteralSuffix::U32, IntLiteralSuffix::U32.kind(), "u32")
            }
            IntLiteralSuffixToken::U64 => {
                (IntLiteralSuffix::U64, IntLiteralSuffix::U64.kind(), "u64")
            }
            IntLiteralSuffixToken::U128 => (
                IntLiteralSuffix::U64,
                IntLiteralKind {
                    signedness: IntLiteralSignedness::Unsigned,
                    width: IntLiteralWidth::W128,
                },
                "u128",
            ),
        }
    }

    fn float_literal_suffix_to_ast(
        suffix: FloatLiteralSuffixToken,
    ) -> (FloatLiteralSuffix, FloatLiteralKind, &'static str) {
        match suffix {
            FloatLiteralSuffixToken::F32 => (
                FloatLiteralSuffix::F32,
                FloatLiteralSuffix::F32.kind(),
                "f32",
            ),
            FloatLiteralSuffixToken::F64 => (
                FloatLiteralSuffix::F64,
                FloatLiteralSuffix::F64.kind(),
                "f64",
            ),
        }
    }

    fn typed_int_literal_metadata(
        suffix: Option<IntLiteralSuffixToken>,
        raw_value_span: Span,
        raw_literal_text: String,
    ) -> Option<Box<IntLiteralMetadata>> {
        let suffix = suffix?;
        let (ast_suffix, ast_kind, suffix_text) = Self::int_literal_suffix_to_ast(suffix);
        let ast_meta = IntLiteralMetadata::with_kind_and_suffix_text(
            ast_suffix,
            ast_kind,
            suffix_text,
            raw_value_span,
            raw_literal_text,
        );
        Some(Box::new(ast_meta))
    }

    fn typed_float_literal_metadata(
        suffix: Option<FloatLiteralSuffixToken>,
        raw_value_span: Span,
        raw_literal_text: String,
    ) -> Option<Box<FloatLiteralMetadata>> {
        let suffix = suffix?;
        let (ast_suffix, ast_kind, suffix_text) = Self::float_literal_suffix_to_ast(suffix);
        let ast_meta = FloatLiteralMetadata::with_kind_and_suffix_text(
            ast_suffix,
            ast_kind,
            suffix_text,
            raw_value_span,
            raw_literal_text,
        );
        Some(Box::new(ast_meta))
    }

    fn make_unit_expr(&self, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Unit,
            span,
        }
    }

    fn make_var_expr(&self, name: impl Into<String>, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Var(name.into()),
            span,
        }
    }

    fn make_unit_block(&self, span: Span) -> Block {
        Block {
            stmts: Vec::new(),
            tail: Some(Box::new(self.make_unit_expr(span))),
            span,
        }
    }

    fn wrap_if_true_expr(&self, then_block: Block, span: Span) -> Expr {
        Expr {
            kind: ExprKind::If {
                cond: Box::new(self.make_bool_expr(true, span)),
                then_block,
                else_block: self.make_unit_block(span),
            },
            span,
        }
    }

    fn for_body_to_stmts(&self, body: &Block) -> Vec<Stmt> {
        let mut stmts = body.stmts.clone();
        if let Some(tail) = &body.tail {
            stmts.push(Stmt::Expr {
                expr: (**tail).clone(),
                span: tail.span,
            });
        }
        stmts
    }

    fn desugar_for_iter(
        &mut self,
        binding: String,
        iterable: Expr,
        body: Block,
        start: usize,
    ) -> Expr {
        let id = self.next_for_id();
        let span = Span::new(start, body.span.end);
        let iter_name = self.make_for_name("iter", id);
        let step_name = self.make_for_name("step", id);
        let step_item_expr = Expr {
            kind: ExprKind::FieldAccess {
                base: Box::new(self.make_var_expr(step_name.clone(), span)),
                field: "item".to_string(),
            },
            span,
        };
        let step_iter_expr = Expr {
            kind: ExprKind::FieldAccess {
                base: Box::new(self.make_var_expr(step_name.clone(), span)),
                field: "iter".to_string(),
            },
            span,
        };

        let mut some_body_stmts = vec![Stmt::Assign {
            target: iter_name.clone(),
            path: Vec::new(),
            op: None,
            expr: step_iter_expr,
            span,
        }];
        some_body_stmts.extend(self.for_body_to_stmts(&body));

        let some_arm_then = Block {
            stmts: some_body_stmts,
            tail: None,
            span: body.span,
        };

        let some_arm = MatchArm {
            pattern: Pattern {
                kind: PatternKind::Variant {
                    name: "Some".to_string(),
                    args: vec![Pattern {
                        kind: PatternKind::Var(binding),
                        span,
                    }],
                },
                span,
            },
            guard: None,
            body: self.wrap_if_true_expr(some_arm_then, span),
            span,
        };

        let none_arm = MatchArm {
            pattern: Pattern {
                kind: PatternKind::Variant {
                    name: "None".to_string(),
                    args: Vec::new(),
                },
                span,
            },
            guard: None,
            body: Expr {
                kind: ExprKind::Break {
                    expr: None,
                    label: None,
                },
                span,
            },
            span,
        };

        let loop_body = Block {
            stmts: vec![
                Stmt::Let {
                    name: step_name.clone(),
                    mutable: false,
                    ty: None,
                    expr: Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(self.make_var_expr("aic_for_next_iter", span)),
                            args: vec![self.make_var_expr(iter_name.clone(), span)],
                            arg_names: Vec::new(),
                            type_args: Vec::new(),
                        },
                        span,
                    },
                    span,
                },
                Stmt::Expr {
                    expr: Expr {
                        kind: ExprKind::Match {
                            expr: Box::new(step_item_expr),
                            arms: vec![some_arm, none_arm],
                        },
                        span,
                    },
                    span,
                },
            ],
            tail: None,
            span,
        };

        let then_block = Block {
            stmts: vec![Stmt::Let {
                name: iter_name.clone(),
                mutable: true,
                ty: None,
                expr: Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(self.make_var_expr("aic_for_into_iter", span)),
                        args: vec![iterable],
                        arg_names: Vec::new(),
                        type_args: Vec::new(),
                    },
                    span,
                },
                span,
            }],
            tail: Some(Box::new(Expr {
                kind: ExprKind::Loop {
                    body: loop_body,
                    label: None,
                },
                span,
            })),
            span,
        };

        self.wrap_if_true_expr(then_block, span)
    }

    /// `for i in a..b` counts `cur` from `a` while `cur < end`. `a..=b`
    /// also stops after `end` without computing `end + 1` (which may not
    /// exist): a `done` flag is set instead of incrementing at the end.
    fn desugar_for_range(
        &mut self,
        binding: String,
        start_expr: Expr,
        end_expr: Expr,
        inclusive: bool,
        body: Block,
        start: usize,
    ) -> Expr {
        let id = self.next_for_id();
        let span = Span::new(start, body.span.end);
        let cur_name = self.make_for_name("range_cur", id);
        let end_name = self.make_for_name("range_end", id);
        let done_name = self.make_for_name("range_done", id);

        let increment = Stmt::Assign {
            target: cur_name.clone(),
            path: Vec::new(),
            op: None,
            expr: Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Add,
                    lhs: Box::new(self.make_var_expr(cur_name.clone(), span)),
                    rhs: Box::new(self.make_int_expr(1, span)),
                },
                span,
            },
            span,
        };
        let advance = if inclusive {
            // `if cur == end { done = true; } else { cur = cur + 1; }`
            Stmt::Expr {
                expr: Expr {
                    kind: ExprKind::If {
                        cond: Box::new(Expr {
                            kind: ExprKind::Binary {
                                op: BinOp::Eq,
                                lhs: Box::new(self.make_var_expr(cur_name.clone(), span)),
                                rhs: Box::new(self.make_var_expr(end_name.clone(), span)),
                            },
                            span,
                        }),
                        then_block: Block {
                            stmts: vec![Stmt::Assign {
                                target: done_name.clone(),
                                path: Vec::new(),
                                op: None,
                                expr: Expr {
                                    kind: ExprKind::Bool(true),
                                    span,
                                },
                                span,
                            }],
                            tail: None,
                            span,
                        },
                        else_block: Block {
                            stmts: vec![increment],
                            tail: None,
                            span,
                        },
                    },
                    span,
                },
                span,
            }
        } else {
            increment
        };
        let mut range_then_stmts = vec![
            Stmt::Let {
                name: binding,
                mutable: false,
                ty: None,
                expr: self.make_var_expr(cur_name.clone(), span),
                span,
            },
            advance,
        ];
        range_then_stmts.extend(self.for_body_to_stmts(&body));

        let in_range = Expr {
            kind: ExprKind::Binary {
                op: if inclusive { BinOp::Le } else { BinOp::Lt },
                lhs: Box::new(self.make_var_expr(cur_name.clone(), span)),
                rhs: Box::new(self.make_var_expr(end_name.clone(), span)),
            },
            span,
        };
        let cond = if inclusive {
            // `!done && cur <= end`
            Expr {
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(Expr {
                        kind: ExprKind::Unary {
                            op: UnaryOp::Not,
                            expr: Box::new(self.make_var_expr(done_name.clone(), span)),
                        },
                        span,
                    }),
                    rhs: Box::new(in_range),
                },
                span,
            }
        } else {
            in_range
        };

        let range_if_expr = Expr {
            kind: ExprKind::If {
                cond: Box::new(cond),
                then_block: Block {
                    stmts: range_then_stmts,
                    tail: None,
                    span: body.span,
                },
                else_block: Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(Expr {
                        kind: ExprKind::Break {
                            expr: None,
                            label: None,
                        },
                        span,
                    })),
                    span,
                },
            },
            span,
        };

        let loop_body = Block {
            stmts: vec![Stmt::Expr {
                expr: range_if_expr,
                span,
            }],
            tail: None,
            span,
        };

        let then_block = Block {
            stmts: vec![
                Stmt::Let {
                    name: cur_name,
                    mutable: true,
                    ty: None,
                    expr: start_expr,
                    span,
                },
                Stmt::Let {
                    name: end_name,
                    mutable: false,
                    ty: None,
                    expr: end_expr,
                    span,
                },
            ]
            .into_iter()
            .chain(inclusive.then(|| Stmt::Let {
                name: done_name,
                mutable: true,
                ty: None,
                expr: Expr {
                    kind: ExprKind::Bool(false),
                    span,
                },
                span,
            }))
            .collect(),
            tail: Some(Box::new(Expr {
                kind: ExprKind::Loop {
                    body: loop_body,
                    label: None,
                },
                span,
            })),
            span,
        };

        self.wrap_if_true_expr(then_block, span)
    }

    fn break_has_value_start(&self) -> bool {
        !self.at_kind(|k| {
            matches!(
                k,
                TokenKind::Semi
                    | TokenKind::RBrace
                    | TokenKind::RParen
                    | TokenKind::Comma
                    | TokenKind::Eof
            )
        })
    }

    fn parse_match_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // match
        let expr = self.parse_expr_before_block()?;
        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1043",
            "expected '{' for match arms",
        )?;
        let mut arms = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let arm_start = self.current_span().start;
            let pattern = self.parse_pattern()?;
            let guard = if self.at_kind(|k| matches!(k, TokenKind::KwIf)) {
                self.bump();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(
                |k| matches!(k, TokenKind::FatArrow),
                "E1044",
                "expected '=>' in match arm",
            )?;
            let body = self.parse_expr()?;
            let arm_span = Span::new(arm_start, body.span.end);
            arms.push(MatchArm {
                pattern,
                guard,
                body,
                span: arm_span,
            });
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else if !(is_block_like(&arms.last().expect("arm").body)
                && !self.at_kind(|k| matches!(k, TokenKind::RBrace)))
            {
                // The comma after an arm is optional when its body ends in `}`.
                break;
            }
        }
        let close = self.expect(
            |k| matches!(k, TokenKind::RBrace),
            "E1045",
            "expected '}' after match arms",
        )?;
        Some(Expr {
            kind: ExprKind::Match {
                expr: Box::new(expr),
                arms,
            },
            span: Span::new(start, close.end),
        })
    }

    /// An integer, char or float literal pattern, with an optional `-`.
    fn parse_literal_pattern(&mut self) -> Option<Pattern> {
        let start = self.current_span().start;
        let negative = self.at_kind(|k| matches!(k, TokenKind::Minus));
        if negative {
            self.bump();
        }
        let token = self.current().clone();
        let span = Span::new(start, token.span.end);
        let kind = match token.kind {
            TokenKind::Int(literal) => {
                let meta = Self::typed_int_literal_metadata(
                    literal.suffix,
                    literal.raw_value_span,
                    literal.raw_text,
                );
                let value = if negative {
                    literal.value.checked_neg()?
                } else {
                    literal.value
                };
                PatternKind::Int(value, meta)
            }
            TokenKind::Float(literal) => PatternKind::Float(if negative {
                -literal.value
            } else {
                literal.value
            }),
            TokenKind::Char(value) if !negative => PatternKind::Char(value),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E1310",
                    "expected a number after '-' in a pattern",
                    self.file,
                    token.span,
                ));
                return None;
            }
        };
        self.bump();
        Some(Pattern { kind, span })
    }

    /// The bound a literal pattern gives a range pattern.
    fn range_bound(&mut self, literal: &Pattern) -> Option<RangeBound> {
        match &literal.kind {
            PatternKind::Int(value, _) => Some(RangeBound::Int(*value)),
            PatternKind::Char(value) => Some(RangeBound::Char(*value)),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E1310",
                    "range pattern bounds must be integer or char literals",
                    self.file,
                    literal.span,
                ));
                None
            }
        }
    }

    /// The rest of a range pattern at `..` / `..=`: `lo..=hi`, `lo..hi`,
    /// `lo..` or `..=hi`.
    fn parse_range_pattern_rest(
        &mut self,
        start: usize,
        lower: Option<RangeBound>,
    ) -> Option<Pattern> {
        let inclusive = self.at_kind(|k| matches!(k, TokenKind::DotDotEq));
        let op_span = self.current_span();
        self.bump(); // .. or ..=
        let has_upper = matches!(
            self.current().kind,
            TokenKind::Int(_) | TokenKind::Char(_) | TokenKind::Float(_)
        ) || (self.at_kind(|k| matches!(k, TokenKind::Minus))
            && matches!(
                self.peek(1).map(|token| &token.kind),
                Some(TokenKind::Int(_) | TokenKind::Float(_))
            ));
        let upper = if has_upper {
            let literal = self.parse_literal_pattern()?;
            Some(self.range_bound(&literal)?)
        } else {
            None
        };
        let problem = match (&lower, &upper) {
            (_, None) if inclusive => Some("`..=` needs an upper bound"),
            (None, _) if !inclusive => Some("a range pattern without a lower bound uses `..=`"),
            (Some(RangeBound::Int(_)), Some(RangeBound::Char(_)))
            | (Some(RangeBound::Char(_)), Some(RangeBound::Int(_))) => {
                Some("range pattern bounds must both be integers or both be chars")
            }
            _ => None,
        };
        if let Some(problem) = problem {
            self.diagnostics.push(Diagnostic::error(
                "E1310",
                format!("invalid range pattern: {problem}"),
                self.file,
                Span::new(start, self.previous_span().end.max(op_span.end)),
            ));
            // Keep parsing the match after the reported pattern.
            return Some(Pattern {
                kind: PatternKind::Wildcard,
                span: Span::new(start, self.previous_span().end),
            });
        }
        Some(Pattern {
            kind: PatternKind::Range {
                start: lower,
                end: upper,
                inclusive,
            },
            span: Span::new(start, self.previous_span().end),
        })
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let mut patterns = vec![self.parse_pattern_atom()?];
        while self.at_kind(|k| matches!(k, TokenKind::Pipe)) {
            self.bump();
            patterns.push(self.parse_pattern_atom()?);
        }
        if patterns.len() == 1 {
            return patterns.into_iter().next();
        }
        let span = Span::new(
            patterns.first().expect("first pattern").span.start,
            patterns.last().expect("last pattern").span.end,
        );
        Some(Pattern {
            kind: PatternKind::Or { patterns },
            span,
        })
    }

    fn parse_pattern_atom(&mut self) -> Option<Pattern> {
        let token = self.current().clone();
        // Literal patterns that can start a range: `-3`, `7`, `'a'`, `1.5`.
        let starts_literal = match &token.kind {
            TokenKind::Minus => matches!(
                self.peek(1).map(|token| &token.kind),
                Some(TokenKind::Int(_) | TokenKind::Float(_))
            ),
            TokenKind::Int(_) | TokenKind::Char(_) | TokenKind::Float(_) => true,
            _ => false,
        };
        if starts_literal {
            let start = token.span.start;
            let literal = self.parse_literal_pattern()?;
            if !self.at_kind(|k| matches!(k, TokenKind::DotDot | TokenKind::DotDotEq)) {
                return Some(literal);
            }
            let lower = self.range_bound(&literal)?;
            return self.parse_range_pattern_rest(start, Some(lower));
        }
        if matches!(token.kind, TokenKind::DotDotEq | TokenKind::DotDot) {
            return self.parse_range_pattern_rest(token.span.start, None);
        }
        match token.kind {
            TokenKind::Underscore => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::Wildcard,
                    span: token.span,
                })
            }
            TokenKind::String(value) => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::String(value),
                    span: token.span,
                })
            }
            TokenKind::KwTrue => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::Bool(true),
                    span: token.span,
                })
            }
            TokenKind::KwFalse => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::Bool(false),
                    span: token.span,
                })
            }
            TokenKind::LParen => {
                let start = token.span.start;
                self.bump();
                if self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                    self.bump();
                    return Some(Pattern {
                        kind: PatternKind::Unit,
                        span: Span::new(start, self.previous_span().end),
                    });
                }
                let first = self.parse_pattern()?;
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                    let mut args = vec![first];
                    while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                        args.push(self.parse_pattern()?);
                        if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let close = self.expect(
                        |k| matches!(k, TokenKind::RParen),
                        "E1046",
                        "expected ')' after tuple pattern",
                    )?;
                    return Some(Pattern {
                        kind: PatternKind::Variant {
                            name: TUPLE_INTERNAL_NAME.to_string(),
                            args,
                        },
                        span: Span::new(start, close.end),
                    });
                }
                let close = self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1046",
                    "expected ')' after parenthesized pattern",
                )?;
                Some(Pattern {
                    kind: first.kind,
                    span: Span::new(start, close.end),
                })
            }
            TokenKind::Ident(name)
                if matches!(self.peek(1).map(|token| &token.kind), Some(TokenKind::At)) =>
            {
                self.bump(); // name
                self.bump(); // @
                let inner = self.parse_pattern_atom()?;
                let span = Span::new(token.span.start, inner.span.end);
                Some(Pattern {
                    kind: PatternKind::Binding {
                        name,
                        pattern: Box::new(inner),
                    },
                    span,
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                // `Enum::Variant` paths name a variant of that enum.
                let mut name = name;
                let mut name_end = token.span.end;
                while self.at_kind(|k| matches!(k, TokenKind::ColonColon)) {
                    self.bump();
                    let (segment, segment_span) =
                        self.expect_ident("E1088", "expected path segment after '::'")?;
                    name.push_str("::");
                    name.push_str(&segment);
                    name_end = segment_span.end;
                }
                let qualified = name.contains("::");
                let name_span = Span::new(token.span.start, name_end);
                if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
                    let start = name_span.start;
                    self.bump();
                    let mut fields = Vec::new();
                    let mut has_rest = false;
                    while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
                        if self.at_kind(|k| matches!(k, TokenKind::DotDot)) {
                            has_rest = true;
                            self.bump();
                            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                                self.bump();
                            }
                            break;
                        }
                        let (field_name, field_span) =
                            self.expect_ident("E1049", "expected struct pattern field")?;
                        let pattern = if self.at_kind(|k| matches!(k, TokenKind::Colon)) {
                            self.bump();
                            self.parse_pattern()?
                        } else {
                            Pattern {
                                kind: PatternKind::Var(field_name.clone()),
                                span: field_span,
                            }
                        };
                        fields.push(StructPatternField {
                            name: field_name,
                            pattern,
                        });
                        if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let close = self.expect(
                        |k| matches!(k, TokenKind::RBrace),
                        "E1047",
                        "expected '}' after struct pattern",
                    )?;
                    Some(Pattern {
                        kind: PatternKind::Struct {
                            name,
                            fields,
                            has_rest,
                        },
                        span: Span::new(start, close.end),
                    })
                } else if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
                    let start = name_span.start;
                    self.bump();
                    let mut args = Vec::new();
                    while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                        args.push(self.parse_pattern()?);
                        if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let close = self.expect(
                        |k| matches!(k, TokenKind::RParen),
                        "E1047",
                        "expected ')' after variant pattern",
                    )?;
                    let span = Span::new(start, close.end);
                    // `V(a, b)` matches the tuple payload of a multi-field
                    // variant: `V((a, b))`.
                    let args = if args.len() > 1 {
                        vec![Pattern {
                            kind: PatternKind::Variant {
                                name: TUPLE_INTERNAL_NAME.to_string(),
                                args,
                            },
                            span,
                        }]
                    } else {
                        args
                    };
                    Some(Pattern {
                        kind: PatternKind::Variant { name, args },
                        span,
                    })
                } else if qualified
                    || name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                {
                    Some(Pattern {
                        kind: PatternKind::Variant {
                            name,
                            args: Vec::new(),
                        },
                        span: name_span,
                    })
                } else {
                    Some(Pattern {
                        kind: PatternKind::Var(name),
                        span: name_span,
                    })
                }
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E1048",
                    "expected match pattern",
                    self.file,
                    token.span,
                ));
                None
            }
        }
    }

    fn parse_path(&mut self) -> Option<(Vec<String>, Span)> {
        let (first, first_span) = self.expect_ident("E1049", "expected path segment")?;
        let mut path = vec![first];
        let mut end = first_span.end;
        while self.at_kind(|k| matches!(k, TokenKind::Dot)) {
            self.bump();
            let (segment, span) = self.expect_ident("E1050", "expected path segment")?;
            path.push(segment);
            end = span.end;
        }
        Some((path, Span::new(first_span.start, end)))
    }

    fn recover_item(&mut self) {
        while !self.at_kind(|k| matches!(k, TokenKind::Eof)) {
            if self.at_kind(|k| {
                matches!(
                    k,
                    TokenKind::KwAsync
                        | TokenKind::KwFn
                        | TokenKind::KwType
                        | TokenKind::KwConst
                        | TokenKind::KwExtern
                        | TokenKind::KwUnsafe
                        | TokenKind::KwStruct
                        | TokenKind::KwEnum
                        | TokenKind::KwTrait
                        | TokenKind::KwImpl
                )
            }) {
                break;
            }
            self.bump();
        }
    }

    /// Reports `type`/`const` members of a trait or impl block, which are not
    /// supported yet. Returns true when such a member starts at the current token.
    fn report_unsupported_associated_item(&mut self, container: &str) -> bool {
        let (code, kind, help) = if self.at_kind(|k| matches!(k, TokenKind::KwType)) {
            (
                "E1084",
                "associated types",
                "add a generic parameter to the trait instead, for example `trait Shape[T, Output]`",
            )
        } else if self.at_kind(|k| matches!(k, TokenKind::KwConst)) {
            (
                "E1085",
                "associated constants",
                "declare a method that returns the value, for example `fn max_size(self: T) -> Int;`, or use a module-level `const`",
            )
        } else {
            return false;
        };
        let span = match self.peek(1) {
            Some(token) if matches!(token.kind, TokenKind::Ident(_)) => {
                Span::new(self.current_span().start, token.span.end)
            }
            _ => self.current_span(),
        };
        self.diagnostics.push(
            Diagnostic::error(
                code,
                format!("{kind} are not supported in {container} blocks yet"),
                self.file,
                span,
            )
            .with_help(help),
        );
        true
    }

    /// Skips the rest of a malformed trait or impl member.
    ///
    /// Recovery always makes progress: it consumes at least one token when the
    /// member consumed nothing. It skips balanced `{ ... }` blocks and stops at
    /// the container's closing `}`, after a `;`, after a skipped block, or before
    /// the next member or module-level item.
    fn recover_member(&mut self, member_start: usize) {
        let mut consumed = self.index > member_start;
        let mut depth = 0usize;
        while !self.at_kind(|k| matches!(k, TokenKind::Eof)) {
            if depth == 0 {
                if self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
                    break;
                }
                if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
                    self.bump();
                    break;
                }
                if consumed && (self.at_member_start() || self.at_container_terminator()) {
                    break;
                }
            }
            if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
                depth += 1;
            } else if self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    break;
                }
            }
            self.bump();
            consumed = true;
        }
    }

    /// Consumes a `{ ... }` block including nested blocks. The current token must
    /// be `{`. Stops at EOF when the block is unterminated.
    fn skip_balanced_braces(&mut self) {
        let mut depth = 0usize;
        while !self.at_kind(|k| matches!(k, TokenKind::Eof)) {
            if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
                depth += 1;
            } else if self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    self.bump();
                    return;
                }
            }
            self.bump();
        }
    }

    fn at_member_start(&self) -> bool {
        self.at_kind(|k| {
            matches!(
                k,
                TokenKind::KwFn
                    | TokenKind::KwAsync
                    | TokenKind::KwUnsafe
                    | TokenKind::KwType
                    | TokenKind::KwConst
                    | TokenKind::KwPub
                    | TokenKind::KwPriv
            )
        }) || (self.at_kind(|k| matches!(k, TokenKind::Hash))
            && self
                .peek(1)
                .map(|token| matches!(token.kind, TokenKind::LBracket))
                .unwrap_or(false))
    }

    /// Tokens that can only start a module-level item, never a trait/impl member.
    fn at_container_terminator(&self) -> bool {
        self.at_kind(|k| {
            matches!(
                k,
                TokenKind::KwStruct
                    | TokenKind::KwEnum
                    | TokenKind::KwTrait
                    | TokenKind::KwImpl
                    | TokenKind::KwExtern
                    | TokenKind::KwIntrinsic
                    | TokenKind::KwModule
                    | TokenKind::KwImport
            )
        })
    }

    fn recover_statement(&mut self) {
        while !self.at_kind(|k| matches!(k, TokenKind::Eof | TokenKind::RBrace)) {
            if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
                self.bump();
                break;
            }
            if self
                .at_kind(|k| matches!(k, TokenKind::KwLet | TokenKind::KwReturn | TokenKind::KwFor))
            {
                break;
            }
            self.bump();
        }
    }

    /// Returns `true` at `name`, `name.field...` or `name[index]...`
    /// followed by an assignment operator.
    fn at_assignment_stmt_start(&self) -> bool {
        if !matches!(self.current().kind, TokenKind::Ident(_)) {
            return false;
        }
        let mut offset = 1;
        loop {
            match self.peek(offset).map(|token| &token.kind) {
                Some(TokenKind::Dot) => {
                    let is_projection = matches!(
                        self.peek(offset + 1).map(|token| &token.kind),
                        Some(TokenKind::Ident(_) | TokenKind::Int(_) | TokenKind::Float(_))
                    );
                    if !is_projection {
                        return false;
                    }
                    offset += 2;
                }
                Some(TokenKind::LBracket) => {
                    let mut depth = 0usize;
                    loop {
                        match self.peek(offset).map(|token| &token.kind) {
                            Some(TokenKind::LBracket) => depth += 1,
                            Some(TokenKind::RBracket) => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            Some(TokenKind::Semi | TokenKind::LBrace | TokenKind::RBrace)
                            | Some(TokenKind::Eof)
                            | None => return false,
                            Some(_) => {}
                        }
                        offset += 1;
                    }
                    offset += 1;
                }
                Some(kind) => return assignment_operator(kind).is_some(),
                None => return false,
            }
        }
    }

    fn looks_like_struct_literal(&self) -> bool {
        // Current token is expected to be '{' after an identifier.
        if !self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
            return false;
        }
        let next = self.peek(1).map(|t| &t.kind);
        match next {
            Some(TokenKind::RBrace) => true,
            Some(TokenKind::Ident(_)) => {
                matches!(self.peek(2).map(|t| &t.kind), Some(TokenKind::Colon))
            }
            _ => false,
        }
    }

    fn expect_ident(&mut self, code: &str, message: &str) -> Option<(String, Span)> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Ident(name) => {
                self.bump();
                Some((name, token.span))
            }
            _ => {
                self.diagnostics
                    .push(Diagnostic::error(code, message, self.file, token.span));
                None
            }
        }
    }

    fn expect(
        &mut self,
        predicate: impl Fn(&TokenKind) -> bool,
        code: &str,
        message: &str,
    ) -> Option<Span> {
        if predicate(&self.current().kind) {
            let span = self.current().span;
            self.bump();
            Some(span)
        } else {
            let span = self.current().span;
            self.diagnostics
                .push(Diagnostic::error(code, message, self.file, span));
            None
        }
    }

    fn at_kind(&self, predicate: impl Fn(&TokenKind) -> bool) -> bool {
        predicate(&self.current().kind)
    }

    fn current(&self) -> &Token {
        let last = self.tokens.len().saturating_sub(1);
        let steps = self.steps.get().saturating_add(1);
        self.steps.set(steps);
        if steps > self.step_budget {
            if steps == self.step_budget + 1 {
                self.stuck_at.set(self.index.min(last));
            }
            return &self.tokens[last];
        }
        &self.tokens[self.index.min(last)]
    }

    fn peek(&self, n: usize) -> Option<&Token> {
        if self.budget_exhausted() {
            return self.tokens.last();
        }
        self.tokens.get(self.index + n)
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn previous_span(&self) -> Span {
        let idx = self.index.saturating_sub(1);
        self.tokens[idx].span
    }

    fn bump(&mut self) {
        if self.budget_exhausted() {
            self.index = self.tokens.len().saturating_sub(1);
            return;
        }
        if self.index < self.tokens.len().saturating_sub(1) {
            self.index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::ast::{
        BinOp, Expr, ExprKind, FloatLiteralSuffix, FloatLiteralWidth, IntLiteralSignedness,
        IntLiteralSuffix, IntLiteralWidth, Item, PatternKind, Stmt, TypeKind,
    };

    fn body_stmts(src: &str) -> Vec<Stmt> {
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        f.body.stmts.clone()
    }

    fn rendered_place(stmt: &Stmt) -> String {
        let Stmt::Assign { target, path, .. } = stmt else {
            panic!("expected assignment, found {stmt:?}");
        };
        let mut out = target.clone();
        for projection in path {
            match projection {
                crate::ast::PlaceProjection::Field { name, .. } => {
                    out.push('.');
                    out.push_str(name);
                }
                crate::ast::PlaceProjection::Index { .. } => out.push_str("[_]"),
            }
        }
        out
    }

    #[test]
    fn parses_place_assignment_targets() {
        let stmts = body_stmts(
            "fn f() -> () { p.x = 1; p.a.b = 2; t.0 = 3; t.1.0 = 4; v[i + 1] = 5; items[0].qty = 6; self.n |= 7; x = 8; }",
        );
        let places = stmts.iter().map(rendered_place).collect::<Vec<_>>();
        assert_eq!(
            places,
            [
                "p.x",
                "p.a.b",
                "t.0",
                "t.1.0",
                "v[_]",
                "items[_].qty",
                "self.n",
                "x"
            ]
        );
        // `self.n |= 7` keeps its operator; the value is the right operand.
        let Stmt::Assign { op, expr, .. } = &stmts[6] else {
            panic!("expected assignment");
        };
        assert_eq!(*op, Some(BinOp::BitOr));
        assert!(matches!(&expr.kind, ExprKind::Int(7, ..)));
    }

    #[test]
    fn parses_every_compound_assignment_operator() {
        let stmts = body_stmts(
            "fn f() -> () { x += 1; x -= 1; x *= 1; x /= 1; x %= 1; x &= 1; x |= 1; x ^= 1; x <<= 1; x >>= 1; x >>>= 1; v[0] += 1; x = 1; }",
        );
        let ops = stmts
            .iter()
            .map(|stmt| match stmt {
                Stmt::Assign { op, .. } => *op,
                other => panic!("expected assignment, found {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ops,
            [
                Some(BinOp::Add),
                Some(BinOp::Sub),
                Some(BinOp::Mul),
                Some(BinOp::Div),
                Some(BinOp::Mod),
                Some(BinOp::BitAnd),
                Some(BinOp::BitOr),
                Some(BinOp::BitXor),
                Some(BinOp::Shl),
                Some(BinOp::Shr),
                Some(BinOp::Ushr),
                Some(BinOp::Add),
                None,
            ]
        );
        // `x + = 1` is not an assignment operator.
        let (_, diagnostics) = parse("fn f() -> () { x + = 1; }", "test.aic");
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn place_assignment_lookahead_leaves_expressions_alone() {
        // Comparisons and method calls are expression statements.
        let stmts = body_stmts("fn f() -> () { p.x == 1; p.go(); p.a.b < 2; }");
        assert!(
            stmts.iter().all(|stmt| matches!(stmt, Stmt::Expr { .. })),
            "{stmts:#?}"
        );
    }

    #[test]
    fn nested_tuple_index_reads_parse() {
        let (program, diagnostics) =
            parse("fn f(t: (Int, (Int, Int))) -> Int { t.1.0 }", "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let tail = f.body.tail.as_ref().expect("tail");
        let ExprKind::FieldAccess { base, field } = &tail.kind else {
            panic!("expected field access");
        };
        assert_eq!(field, "0");
        assert!(matches!(&base.kind, ExprKind::FieldAccess { field, .. } if field == "1"));
    }

    #[test]
    fn mutable_self_receiver_is_recorded() {
        let (program, diagnostics) = parse(
            "struct C { n: Int } impl C { fn a(&mut self) -> () { } fn b(&self) -> () { } fn c(self) -> () { } }",
            "test.aic",
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Impl(impl_def) = &program.items[1] else {
            panic!("expected impl");
        };
        let receivers = impl_def
            .methods
            .iter()
            .map(|method| method.params[0].mutable_receiver)
            .collect::<Vec<_>>();
        assert_eq!(receivers, [true, false, false]);
    }

    #[test]
    fn parses_block_expressions_and_if_without_else() {
        let src = "fn f(x: Int) -> Int { let a = { let b = x; b + 1 }; if a > 1 { g(); } let c = match a { 1 => { let d = 2; d } _ => 0 }; while a > 9 { a; } { a; } c }";
        let stmts = body_stmts(src);
        // let a, if (no `;`), let c, while (no `;`), block (no `;`).
        assert_eq!(stmts.len(), 5, "{stmts:#?}");
        let Stmt::Let { expr, .. } = &stmts[0] else {
            panic!("expected let");
        };
        let ExprKind::Block { block } = &expr.kind else {
            panic!("expected block expression, found {expr:?}");
        };
        assert_eq!(block.stmts.len(), 1);
        assert!(block.tail.is_some());
        let Stmt::Expr { expr, .. } = &stmts[1] else {
            panic!("expected if statement");
        };
        let ExprKind::If { else_block, .. } = &expr.kind else {
            panic!("expected if");
        };
        assert!(else_block.stmts.is_empty() && else_block.tail.is_none());
        assert_eq!(else_block.span.start, else_block.span.end);
        let Stmt::Let { expr, .. } = &stmts[2] else {
            panic!("expected let");
        };
        let ExprKind::Match { arms, .. } = &expr.kind else {
            panic!("expected match");
        };
        // The comma after a block arm is optional.
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[0].body.kind, ExprKind::Block { .. }));
        assert!(
            matches!(&stmts[3], Stmt::Expr { expr, .. } if matches!(expr.kind, ExprKind::While { .. }))
        );
        assert!(
            matches!(&stmts[4], Stmt::Expr { expr, .. } if matches!(expr.kind, ExprKind::Block { .. }))
        );
    }

    #[test]
    fn non_block_expressions_still_need_semicolons() {
        let (_, diagnostics) = parse("fn f() -> Int { g() h() }", "test.aic");
        assert!(!diagnostics.is_empty());
        // A match arm whose body is not a block still needs its comma.
        let (_, diagnostics) = parse(
            "fn f(x: Int) -> Int { match x { 1 => 2 _ => 3 } }",
            "test.aic",
        );
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn parses_qualified_variant_patterns_and_values() {
        let src = "fn f(c: Color) -> Int { let d = Color::Red; match c { Color::Red => 1, Color::Blue(n) => n, Green => 2, other => 3 } }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let Stmt::Let { expr, .. } = &f.body.stmts[0] else {
            panic!("expected let");
        };
        assert!(matches!(&expr.kind, ExprKind::Var(name) if name == "Color::Red"));
        let ExprKind::Match { arms, .. } = &f.body.tail.as_ref().expect("tail").kind else {
            panic!("expected match");
        };
        let kinds = arms
            .iter()
            .map(|arm| match &arm.pattern.kind {
                PatternKind::Variant { name, args } => format!("variant {name}/{}", args.len()),
                PatternKind::Var(name) => format!("binding {name}"),
                other => format!("{other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "variant Color::Red/0",
                "variant Color::Blue/1",
                "variant Green/0",
                "binding other"
            ]
        );
    }

    #[test]
    fn desugars_if_let_while_let_and_let_else() {
        let stmts = body_stmts(
            "fn f(o: Option[Int]) -> Int { if let Some(x) = o { g(x); } while let Some(y) = o { break; } let Some(z) = o else { return 0; }; let (a, Some(b)) = (1, o) else { return 1; }; z }",
        );
        // if let -> match with the hidden else arm.
        let Stmt::Expr { expr, .. } = &stmts[0] else {
            panic!("expected if let statement");
        };
        let ExprKind::Match { arms, .. } = &expr.kind else {
            panic!("expected match, found {expr:?}");
        };
        assert!(
            matches!(&arms[1].pattern.kind, PatternKind::Var(name) if name == crate::ast::IF_LET_ELSE_BINDING)
        );
        // while let -> loop { match .. { .., _$while_let_end => break } }.
        let Stmt::Expr { expr, .. } = &stmts[1] else {
            panic!("expected while let statement");
        };
        let ExprKind::Loop { body, .. } = &expr.kind else {
            panic!("expected loop");
        };
        let ExprKind::Match { arms, .. } = &body.tail.as_ref().expect("tail").kind else {
            panic!("expected match in loop");
        };
        assert!(matches!(
            arms[1].body.kind,
            ExprKind::Break { expr: None, .. }
        ));
        // let-else with one binding -> `let z = match ..`.
        assert!(matches!(&stmts[2], Stmt::Let { name, .. } if name == "z"));
        // let-else with two bindings -> temporary plus projections.
        assert!(
            matches!(&stmts[3], Stmt::Let { name, .. } if name.starts_with(crate::ast::LET_ELSE_TMP_PREFIX))
        );
        assert!(matches!(&stmts[4], Stmt::Let { name, .. } if name == "a"));
        assert!(matches!(&stmts[5], Stmt::Let { name, .. } if name == "b"));
    }

    #[test]
    fn destructures_structs_in_let_and_rejects_refutable_patterns() {
        let stmts = body_stmts("fn f(p: P) -> Int { let P { x, y: (a, _), .. } = p; x }");
        let names = stmts
            .iter()
            .map(|stmt| match stmt {
                Stmt::Let { name, .. } => name.clone(),
                other => panic!("expected let, found {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(names[0], "__struct_let_0__P");
        assert_eq!(&names[1..], ["x", "a"]);
        for src in [
            "fn f(o: Option[Int]) -> Int { let Some(x) = o; x }",
            "fn f(t: (Int, Int)) -> Int { let (1, y) = t; y }",
        ] {
            let (_, diagnostics) = parse(src, "test.aic");
            assert!(
                diagnostics.iter().any(|d| d.code == "E1110"),
                "{src}: {diagnostics:#?}"
            );
            assert_eq!(diagnostics.len(), 1, "{src}: {diagnostics:#?}");
        }
    }

    #[test]
    fn parses_multi_field_and_struct_like_variants() {
        let src = "enum Shape[T] { Rect(Int, T), Move { x: Int, y: T }, Empty } fn f(s: Shape[Int]) -> Int { match s { Rect(w, h) => w, Shape::Move { x, .. } => x, Empty => 0 } }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Enum(shape) = &program.items[0] else {
            panic!("expected enum");
        };
        let payloads = shape
            .variants
            .iter()
            .map(|variant| match &variant.payload {
                Some(ty) => match &ty.kind {
                    TypeKind::Named { name, args } => format!("{name}/{}", args.len()),
                    other => format!("{other:?}"),
                },
                None => "none".to_string(),
            })
            .collect::<Vec<_>>();
        assert_eq!(payloads, ["Tuple/2", "Shape__Move/1", "none"]);
        // The payload struct follows the enum, generic over its parameters.
        let Item::Struct(payload) = &program.items[1] else {
            panic!("expected the payload struct");
        };
        assert_eq!(payload.name, "Shape__Move");
        assert_eq!(payload.generics.len(), 1);
        assert_eq!(payload.fields.len(), 2);
        // `Rect(w, h)` matches the tuple payload.
        let Item::Function(f) = &program.items[2] else {
            panic!("expected function");
        };
        let ExprKind::Match { arms, .. } = &f.body.tail.as_ref().expect("tail").kind else {
            panic!("expected match");
        };
        let PatternKind::Variant { args, .. } = &arms[0].pattern.kind else {
            panic!("expected variant pattern");
        };
        assert!(
            matches!(&args[..], [p] if matches!(&p.kind, PatternKind::Variant { name, args } if name == "Tuple" && args.len() == 2))
        );
        assert!(
            matches!(&arms[1].pattern.kind, PatternKind::Struct { name, .. } if name == "Shape::Move")
        );
    }

    #[test]
    fn parses_index_expressions_and_list_literals() {
        let (program, diagnostics) = parse(
            "fn f(v: Vec[Vec[Int]]) -> Int { let w = [1, 2, 3]; let e = []; v[0][1] }",
            "test.aic",
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let callee_name = |expr: &Expr| match &expr.kind {
            ExprKind::Call { callee, args, .. } => match &callee.kind {
                ExprKind::Var(name) => (name.clone(), args.len()),
                other => panic!("unexpected callee {other:?}"),
            },
            other => panic!("expected call, found {other:?}"),
        };
        let Stmt::Let { expr, .. } = &f.body.stmts[0] else {
            panic!("expected let");
        };
        assert_eq!(
            callee_name(expr),
            (crate::ast::VEC_LITERAL_BUILTIN.to_string(), 3)
        );
        let Stmt::Let { expr, .. } = &f.body.stmts[1] else {
            panic!("expected let");
        };
        assert_eq!(
            callee_name(expr),
            (crate::ast::VEC_LITERAL_BUILTIN.to_string(), 0)
        );
        let tail = f.body.tail.as_ref().expect("tail");
        assert_eq!(
            callee_name(tail),
            (crate::ast::INDEX_BUILTIN.to_string(), 2)
        );
        let ExprKind::Call { args, .. } = &tail.kind else {
            panic!("expected call");
        };
        assert_eq!(
            callee_name(&args[0]),
            (crate::ast::INDEX_BUILTIN.to_string(), 2)
        );
    }

    #[test]
    fn parses_casts_between_unary_and_binary_operators() {
        let (program, diagnostics) = parse(
            "fn f(x: Int32, as: Int) -> Int64 { let a = -x as Int64; let b = as * x as Int; a }",
            "test.aic",
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let cast_of = |expr: &Expr| match &expr.kind {
            ExprKind::Call { callee, args, .. } => match &callee.kind {
                ExprKind::Var(name) => (name.clone(), args[0].clone()),
                other => panic!("unexpected callee {other:?}"),
            },
            other => panic!("expected cast call, found {other:?}"),
        };
        // `-x as Int64` is `(-x) as Int64`.
        let Stmt::Let { expr, .. } = &f.body.stmts[0] else {
            panic!("expected let");
        };
        let (callee, operand) = cast_of(expr);
        assert_eq!(callee, format!("{}Int64", crate::ast::CAST_BUILTIN_PREFIX));
        assert!(
            matches!(operand.kind, ExprKind::Unary { .. }),
            "{operand:?}"
        );
        // `as * x as Int` is `as * (x as Int)`; `as` is still an identifier.
        let Stmt::Let { expr, .. } = &f.body.stmts[1] else {
            panic!("expected let");
        };
        let ExprKind::Binary { lhs, rhs, .. } = &expr.kind else {
            panic!("expected binary, found {expr:?}");
        };
        assert!(matches!(&lhs.kind, ExprKind::Var(name) if name == "as"));
        assert_eq!(
            cast_of(rhs).0,
            format!("{}Int", crate::ast::CAST_BUILTIN_PREFIX)
        );

        let (_, diagnostics) = parse("fn f(x: Int) -> Int { x as }", "test.aic");
        assert!(!diagnostics.is_empty(), "a cast needs a target type");
    }

    #[test]
    fn parses_simple_function() {
        let src = r#"
module demo.main;

fn add(x: Int, y: Int) -> Int {
    x + y
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let program = program.expect("program");
        assert_eq!(program.items.len(), 1);
        match &program.items[0] {
            Item::Function(f) => {
                assert_eq!(f.name, "add");
                assert_eq!(f.params.len(), 2);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parses_match_expression() {
        let src = r#"
fn f(x: Option[Int]) -> Int {
    match x {
        None => 0,
        Some(v) => v,
    }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty());
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!(),
        };
        let tail = f.body.tail.as_ref().expect("tail");
        assert!(matches!(tail.kind, ExprKind::Match { .. }));
    }

    #[test]
    fn parses_match_or_pattern_and_guard() {
        let src = r#"
fn f(x: Option[Int], ready: Bool) -> Int {
    match x {
        None | Some(v) if ready => 1,
        _ => 0,
    }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!(),
        };
        let tail = f.body.tail.as_ref().expect("tail");
        let ExprKind::Match { arms, .. } = &tail.kind else {
            panic!("expected match expression");
        };
        assert_eq!(arms.len(), 2);
        assert!(arms[0].guard.is_some());
        assert!(matches!(arms[0].pattern.kind, PatternKind::Or { .. }));
    }

    #[test]
    fn parses_extended_match_patterns() {
        let src = r#"
struct User {
    name: String,
    age: Int,
}

fn f(method: String, c: Char, u: User) -> Int {
    let m = match method {
        "GET" => 1,
        "POST" => 2,
        _ => 0,
    };
    let n = match c {
        'a' => 10,
        _ => 20,
    };
    match u {
        User { name, age: years, .. } => m + n + years,
    }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };

        let first_match = match &f.body.stmts[0] {
            Stmt::Let { expr, .. } => expr,
            _ => panic!("expected first let"),
        };
        let ExprKind::Match { arms, .. } = &first_match.kind else {
            panic!("expected match expression for first let");
        };
        assert!(matches!(arms[0].pattern.kind, PatternKind::String(ref v) if v == "GET"));

        let second_match = match &f.body.stmts[1] {
            Stmt::Let { expr, .. } => expr,
            _ => panic!("expected second let"),
        };
        let ExprKind::Match { arms, .. } = &second_match.kind else {
            panic!("expected match expression for second let");
        };
        assert!(matches!(arms[0].pattern.kind, PatternKind::Char('a')));

        let tail = f.body.tail.as_ref().expect("tail expression");
        let ExprKind::Match { arms, .. } = &tail.kind else {
            panic!("expected tail match expression");
        };
        let PatternKind::Struct {
            name,
            fields,
            has_rest,
        } = &arms[0].pattern.kind
        else {
            panic!("expected struct pattern");
        };
        assert_eq!(name, "User");
        assert!(*has_rest);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "name");
        assert!(matches!(fields[0].pattern.kind, PatternKind::Var(ref v) if v == "name"));
        assert_eq!(fields[1].name, "age");
        assert!(matches!(fields[1].pattern.kind, PatternKind::Var(ref v) if v == "years"));
    }

    use crate::ast::{
        RangeBound, StructShape, RANGE_BUILTIN, RANGE_FROM_BUILTIN, RANGE_FULL_BUILTIN,
        RANGE_TO_BUILTIN, RANGE_TO_INCLUSIVE_BUILTIN,
    };

    #[test]
    fn parses_tuple_and_unit_structs() {
        let src = "struct Meters(Int);\npub struct Pair[T](T, pub T) invariant 0 == 0;\nstruct Origin;\nstruct Named { x: Int }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let shapes: Vec<(String, StructShape, Vec<String>)> = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(s) => Some((
                    s.name.clone(),
                    s.shape,
                    s.fields.iter().map(|f| f.name.clone()).collect(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            shapes,
            vec![
                (
                    "Meters".to_string(),
                    StructShape::Tuple,
                    vec!["0".to_string()]
                ),
                (
                    "Pair".to_string(),
                    StructShape::Tuple,
                    vec!["0".to_string(), "1".to_string()]
                ),
                ("Origin".to_string(), StructShape::Unit, Vec::new()),
                (
                    "Named".to_string(),
                    StructShape::Named,
                    vec!["x".to_string()]
                ),
            ]
        );
        for bad in ["struct S();", "struct S(Int)", "struct S"] {
            let (_, diagnostics) = parse(bad, "test.aic");
            assert!(!diagnostics.is_empty(), "{bad} should not parse");
        }
    }

    #[test]
    fn parses_explicit_type_arguments_on_calls() {
        let src = "fn f() -> Int { let a = make::[Int](); let b = pair::[Int, Vec[Bool]](1, v); let c = x.m::[Int](2); let d = Box::Full::[Int](3); 0 }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let call = |idx: usize| match &f.body.stmts[idx] {
            Stmt::Let { expr, .. } => match &expr.kind {
                ExprKind::Call {
                    callee, type_args, ..
                } => (
                    match &callee.kind {
                        ExprKind::Var(name) => name.clone(),
                        ExprKind::FieldAccess { field, .. } => format!(".{field}"),
                        other => panic!("unexpected callee {other:?}"),
                    },
                    type_args.len(),
                ),
                other => panic!("expected call, found {other:?}"),
            },
            other => panic!("expected let, found {other:?}"),
        };
        assert_eq!(call(0), ("make".to_string(), 1));
        assert_eq!(call(1), ("pair".to_string(), 2));
        assert_eq!(call(2), (".m".to_string(), 1));
        assert_eq!(call(3), ("Box::Full".to_string(), 1));

        for bad in [
            "fn f() -> Int { make::[Int] }",
            "fn f() -> Int { make::[]() }",
        ] {
            let (_, diagnostics) = parse(bad, "test.aic");
            assert!(
                diagnostics.iter().any(|d| d.code == "E1312"),
                "{bad}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn parses_closures_with_and_without_return_annotations() {
        let src = "fn f() -> Int { let a = |x: Int| -> Int { x }; let b = |x| { x }; let c = |x, y| x + y; let d = || 1; 0 }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let closure = |idx: usize| match &f.body.stmts[idx] {
            Stmt::Let { expr, .. } => match &expr.kind {
                ExprKind::Closure {
                    params,
                    ret_type,
                    body,
                } => (
                    params.len(),
                    ret_type.is_some(),
                    body.stmts.len(),
                    body.tail.is_some(),
                ),
                other => panic!("expected closure, found {other:?}"),
            },
            other => panic!("expected let, found {other:?}"),
        };
        assert_eq!(closure(0), (1, true, 0, true));
        assert_eq!(closure(1), (1, false, 0, true));
        assert_eq!(closure(2), (2, false, 0, true));
        assert_eq!(closure(3), (0, false, 0, true));

        let (_, diagnostics) = parse("fn f() -> Int { let a = |x: Int| -> Int x; 0 }", "test.aic");
        assert!(
            diagnostics.iter().any(|d| d.code == "E1074"),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn parses_range_float_negative_and_binding_patterns() {
        let src = "fn f(n: Int) -> Int { match n { -5..=-1 => 1, 0..10 => 2, 100.. => 3, ..=-6 => 4, x @ 10..=99 => x, -7 => 5, _ => 0 } }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let tail = f.body.tail.as_deref().expect("match");
        let ExprKind::Match { arms, .. } = &tail.kind else {
            panic!("expected match");
        };
        let kinds: Vec<&PatternKind> = arms.iter().map(|arm| &arm.pattern.kind).collect();
        assert!(matches!(
            kinds[0],
            PatternKind::Range {
                start: Some(RangeBound::Int(-5)),
                end: Some(RangeBound::Int(-1)),
                inclusive: true
            }
        ));
        assert!(matches!(
            kinds[1],
            PatternKind::Range {
                start: Some(RangeBound::Int(0)),
                end: Some(RangeBound::Int(10)),
                inclusive: false
            }
        ));
        assert!(matches!(
            kinds[2],
            PatternKind::Range {
                start: Some(RangeBound::Int(100)),
                end: None,
                ..
            }
        ));
        assert!(matches!(
            kinds[3],
            PatternKind::Range {
                start: None,
                end: Some(RangeBound::Int(-6)),
                inclusive: true
            }
        ));
        assert!(
            matches!(kinds[4], PatternKind::Binding { name, pattern } if name == "x" && matches!(pattern.kind, PatternKind::Range { .. }))
        );
        assert!(matches!(kinds[5], PatternKind::Int(-7, _)));

        let (program, diagnostics) = parse(
            "fn g(c: Char, x: Float64) -> Int { match c { 'a'..='z' => 1, _ => 0 } + match x { -0.5 => 1, 2.5 => 2, _ => 0 } }",
            "test.aic",
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        assert!(program.is_some());

        for bad in [
            "fn f(n: Int) -> Int { match n { ..5 => 1, _ => 0 } }",
            "fn f(n: Int) -> Int { match n { 1..= => 1, _ => 0 } }",
            "fn f(n: Int) -> Int { match n { 1..='z' => 1, _ => 0 } }",
            "fn f(n: Int) -> Int { match n { 1.5..2.5 => 1, _ => 0 } }",
        ] {
            let (_, diagnostics) = parse(bad, "test.aic");
            assert!(
                diagnostics.iter().any(|d| d.code == "E1310"),
                "{bad}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn parses_range_expressions_with_lowest_precedence() {
        let src = "fn f(a: Int, v: Vec[Int]) -> Int { let r = a + 1..a * 2; let s = v[2..]; let t = v[..=3]; let u = v[..]; let w = ..a; for i in 0..=a { }; 0 }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let callee = |stmt: &Stmt| match stmt {
            Stmt::Let { expr, .. } => match &expr.kind {
                ExprKind::Call { callee, args, .. } => match &callee.kind {
                    ExprKind::Var(name) => (name.clone(), args.clone()),
                    other => panic!("unexpected callee {other:?}"),
                },
                other => panic!("expected call, found {other:?}"),
            },
            other => panic!("expected let, found {other:?}"),
        };
        let (name, args) = callee(&f.body.stmts[0]);
        assert_eq!(name, RANGE_BUILTIN);
        assert!(matches!(
            args[0].kind,
            ExprKind::Binary { op: BinOp::Add, .. }
        ));
        assert!(matches!(
            args[1].kind,
            ExprKind::Binary { op: BinOp::Mul, .. }
        ));
        let index_arg = |stmt: &Stmt| {
            let (_, args) = callee(stmt);
            match &args[1].kind {
                ExprKind::Call { callee, args, .. } => match &callee.kind {
                    ExprKind::Var(name) => (name.clone(), args.len()),
                    other => panic!("unexpected callee {other:?}"),
                },
                other => panic!("expected range, found {other:?}"),
            }
        };
        assert_eq!(
            index_arg(&f.body.stmts[1]),
            (RANGE_FROM_BUILTIN.to_string(), 1)
        );
        assert_eq!(
            index_arg(&f.body.stmts[2]),
            (RANGE_TO_INCLUSIVE_BUILTIN.to_string(), 1)
        );
        assert_eq!(
            index_arg(&f.body.stmts[3]),
            (RANGE_FULL_BUILTIN.to_string(), 0)
        );
        assert_eq!(callee(&f.body.stmts[4]).0, RANGE_TO_BUILTIN);
        // `for i in 0..=a` counts directly, with a `done` flag instead of `a + 1`.
        let Stmt::Expr { expr, .. } = &f.body.stmts[5] else {
            panic!("expected for statement");
        };
        let ExprKind::If { then_block, .. } = &expr.kind else {
            panic!("expected desugared for");
        };
        assert_eq!(then_block.stmts.len(), 3, "start, end and done bindings");

        let (_, diagnostics) = parse("fn f() -> Int { let r = 1..=; 0 }", "test.aic");
        assert!(
            diagnostics.iter().any(|d| d.code == "E1310"),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn parses_loop_labels_on_while_loop_and_for() {
        let src = "fn f() -> Int { 'a: while true { continue 'a; } 'b: for i in 0..3 { break 'b; } 'c: loop { break 'c 1; } }";
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected function");
        };
        let Stmt::Expr { expr, .. } = &f.body.stmts[0] else {
            panic!("expected while statement");
        };
        let ExprKind::While { label, body, .. } = &expr.kind else {
            panic!("expected while, found {expr:?}");
        };
        assert_eq!(label.as_deref(), Some("a"));
        let Some(Stmt::Expr { expr: cont, .. }) = body.stmts.first() else {
            panic!("expected continue");
        };
        assert!(matches!(&cont.kind, ExprKind::Continue { label: Some(l) } if l == "a"));
        // `for` desugars to `if true { ..; loop { .. } }`; the label is on the loop.
        let Stmt::Expr { expr, .. } = &f.body.stmts[1] else {
            panic!("expected for statement");
        };
        let ExprKind::If { then_block, .. } = &expr.kind else {
            panic!("expected desugared for, found {expr:?}");
        };
        let tail = then_block.tail.as_deref().expect("loop");
        assert!(matches!(&tail.kind, ExprKind::Loop { label: Some(l), .. } if l == "b"));
        let tail = f.body.tail.as_deref().expect("tail loop");
        let ExprKind::Loop { label, body } = &tail.kind else {
            panic!("expected loop");
        };
        assert_eq!(label.as_deref(), Some("c"));
        let Some(Stmt::Expr { expr: brk, .. }) = body.stmts.first() else {
            panic!("expected break");
        };
        assert!(matches!(&brk.kind, ExprKind::Break { label: Some(l), expr: Some(_) } if l == "c"));

        for bad in [
            "fn f() -> () { 'a: if true { } }",
            "fn f() -> () { 'a while true { } }",
        ] {
            let (_, diagnostics) = parse(bad, "test.aic");
            assert!(
                diagnostics.iter().any(|d| d.code == "E1306"),
                "{bad}: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn parses_while_loop_break_and_continue() {
        let src = r#"
fn f(mut_n: Int) -> Int {
    let mut n = mut_n;
    while n > 0 {
        if n == 2 {
            n = n - 1;
            continue;
        } else {
            ()
        };
        n = n - 1;
    };
    loop {
        break 42
    }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert!(matches!(
            f.body.stmts[1],
            crate::ast::Stmt::Expr {
                expr: Expr {
                    kind: ExprKind::While { .. },
                    ..
                },
                ..
            }
        ));
        let tail = f.body.tail.as_ref().expect("tail expression");
        let ExprKind::Loop { body, .. } = &tail.kind else {
            panic!("expected loop tail");
        };
        assert!(matches!(
            body.tail,
            Some(ref expr)
                if matches!(expr.kind, ExprKind::Break { .. })
        ));
    }

    #[test]
    fn parses_for_in_vec_and_range_forms() {
        let src = r#"
import std.vec;

fn f(v: Vec[Int], n: Int) -> Int {
    let mut acc = 0;
    for item in v {
        acc = acc + item;
    };
    for i in 0..n {
        if i == 2 {
            continue;
        } else {
            ()
        };
        acc = acc + i;
    };
    for j in range(0, n) {
        if j == 3 {
            break;
        } else {
            ()
        };
        acc = acc + j;
    };
    acc
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert_eq!(f.body.stmts.len(), 4);
        for stmt in &f.body.stmts[1..] {
            assert!(matches!(
                stmt,
                Stmt::Expr {
                    expr: Expr {
                        kind: ExprKind::If { .. },
                        ..
                    },
                    ..
                }
            ));
        }
    }

    #[test]
    fn parses_intrinsic_function_declaration() {
        let src = r#"
    intrinsic fn aic_fs_exists_intrinsic(path: String) -> Bool effects { fs };
    "#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let intrinsic_fn = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert!(intrinsic_fn.is_intrinsic);
        assert_eq!(intrinsic_fn.intrinsic_abi.as_deref(), Some("runtime"));
        assert_eq!(intrinsic_fn.effects, vec!["fs".to_string()]);
        assert!(intrinsic_fn.body.stmts.is_empty());
        assert!(intrinsic_fn.body.tail.is_none());
    }

    #[test]
    fn parses_function_capabilities_clause() {
        let src = r#"
    fn run() -> Int effects { io, fs } capabilities { io, fs } {
        0
    }
    "#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let func = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert_eq!(func.effects, vec!["io".to_string(), "fs".to_string()]);
        assert_eq!(func.capabilities, vec!["io".to_string(), "fs".to_string()]);
    }

    #[test]
    fn reports_intrinsic_function_with_body() {
        let src = r#"
    intrinsic fn aic_bad_intrinsic() -> Int {
        1
    }
    "#;
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(
            diagnostics.iter().any(|d| d.code == "E1093"),
            "diags={diagnostics:#?}"
        );
    }

    #[test]
    fn reports_missing_arrow() {
        let src = "fn bad(x: Int) Int { x }";
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.iter().any(|d| d.code == "E1006"));
    }

    #[test]
    fn recovers_multiple_statement_errors_in_single_block() {
        let src = r#"
fn bad() -> Int {
    let x = ;
    let y = ;
    return 1
    let z = 2;
}

fn ok() -> Int { 1 }
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(program.is_some(), "program should still be produced");
        assert!(
            diagnostics.len() >= 3,
            "expected multiple diagnostics, got {:#?}",
            diagnostics
        );
        assert!(diagnostics.iter().any(|d| d.code == "E1041"));
    }

    #[test]
    fn parses_async_function_and_await() {
        let src = r#"
async fn ping() -> Int {
    41
}

async fn main() -> Int {
    await ping() + 1
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        match &program.items[0] {
            Item::Function(f) => assert!(f.is_async),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parses_closure_expression_and_fn_type() {
        let src = r#"
fn apply(f: Fn(Int) -> Int, x: Int) -> Int {
    f(x)
}

fn main() -> Int {
    let inc = |x: Int| -> Int { x + 1 };
    apply(inc, 41)
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let main_fn = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let Stmt::Let { expr, .. } = &main_fn.body.stmts[0] else {
            panic!("expected let stmt");
        };
        assert!(matches!(expr.kind, ExprKind::Closure { .. }));
    }

    #[test]
    fn parses_zero_arg_closure_expression() {
        let src = r#"
fn apply(f: Fn() -> Int) -> Int {
    f()
}

fn main() -> Int {
    let one = || -> Int { 1 };
    apply(one)
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let main_fn = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let Stmt::Let { expr, .. } = &main_fn.body.stmts[0] else {
            panic!("expected let stmt");
        };
        assert!(matches!(expr.kind, ExprKind::Closure { .. }));
    }

    #[test]
    fn parses_general_attributes_on_items_fields_params_and_variants() {
        let src = r#"
#[controller("/users")]
struct UsersController {
    #[required]
    name: String,
}

enum Reply {
    #[status(code = 200)]
    Ok(String),
}

#[get("/:id")]
async fn show_user(
    #[path("id")] id: String,
    #[header("x-request-id")] request_id: String
) -> Int {
    0
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let program = program.expect("program");

        let controller = match &program.items[0] {
            Item::Struct(value) => value,
            other => panic!("expected struct, got {other:?}"),
        };
        assert_eq!(controller.attrs.len(), 1);
        assert_eq!(controller.attrs[0].name, "controller");
        assert_eq!(controller.fields[0].attrs[0].name, "required");

        let reply = match &program.items[1] {
            Item::Enum(value) => value,
            other => panic!("expected enum, got {other:?}"),
        };
        assert_eq!(reply.variants[0].attrs[0].name, "status");

        let handler = match &program.items[2] {
            Item::Function(value) => value,
            other => panic!("expected function, got {other:?}"),
        };
        assert_eq!(handler.attrs[0].name, "get");
        assert_eq!(handler.params[0].attrs[0].name, "path");
        assert_eq!(handler.params[1].attrs[0].name, "header");
    }

    #[test]
    fn parses_framework_attribute_contract_shapes() {
        let src = r#"
#[filter(order = 10)]
struct RequestIdFilter {}

struct CreateUser {
    #[validate(required)]
    name: String,
}

#[post("/users")]
fn create_user(
    #[body] payload: CreateUser,
    #[query("verbose")] verbose: String,
    #[header("x-request-id")] request_id: String
) -> Int {
    0
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let program = program.expect("program");

        let filter = match &program.items[0] {
            Item::Struct(value) => value,
            other => panic!("expected struct, got {other:?}"),
        };
        assert_eq!(filter.attrs[0].name, "filter");

        let dto = match &program.items[1] {
            Item::Struct(value) => value,
            other => panic!("expected struct, got {other:?}"),
        };
        assert_eq!(dto.fields[0].attrs[0].name, "validate");

        let handler = match &program.items[2] {
            Item::Function(value) => value,
            other => panic!("expected function, got {other:?}"),
        };
        assert_eq!(handler.attrs[0].name, "post");
        assert_eq!(handler.params[0].attrs[0].name, "body");
        assert_eq!(handler.params[1].attrs[0].name, "query");
        assert_eq!(handler.params[2].attrs[0].name, "header");
    }

    #[test]
    fn reports_invalid_framework_attribute_targets_and_shapes() {
        let src = r#"
#[get("/bad")]
struct BadRoute {}

#[filter(10)]
struct BadFilter {}

fn bad(
    #[body("payload")] payload: Int,
    #[header("x-request-id")]
    #[header("x-request-id")] request_id: String
) -> Int {
    0
}
"#;
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "E1096" && d.message.contains("not supported")),
            "diagnostics={diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "E1096" && d.message.contains("duplicate framework attribute")),
            "diagnostics={diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "E1097" && d.message.contains("invalid argument shape")),
            "diagnostics={diagnostics:#?}"
        );
    }

    #[test]
    fn reports_async_without_fn_keyword() {
        let src = "async struct Bad { x: Int }";
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.iter().any(|d| d.code == "E1052"));
    }

    #[test]
    fn parses_extern_c_function_and_unsafe_block() {
        let src = r#"
extern "C" fn c_abs(x: Int) -> Int effects { };

fn wrap(x: Int) -> Int {
    unsafe { c_abs(x) }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        assert_eq!(program.items.len(), 2);
        let extern_fn = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert!(extern_fn.is_extern);
        assert_eq!(extern_fn.extern_abi.as_deref(), Some("C"));
        assert!(extern_fn.effects.is_empty());

        let wrap_fn = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = wrap_fn.body.tail.as_ref().expect("tail");
        assert!(matches!(tail.kind, ExprKind::UnsafeBlock { .. }));
    }

    #[test]
    fn parses_extern_effects_clause() {
        let src = r#"extern "C" fn puts(s: String) -> Int32 effects { io };"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(extern_fn) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(extern_fn.is_extern);
        assert_eq!(extern_fn.effects, vec!["io".to_string()]);
        assert!(extern_fn.capabilities.is_empty());
    }

    #[test]
    fn reports_extern_without_effects_clause() {
        let src = r#"extern "C" fn puts(s: String) -> Int32;"#;
        let (program, diagnostics) = parse(src, "test.aic");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "E2010")
            .unwrap_or_else(|| panic!("expected E2010, diags={diagnostics:#?}"));
        assert!(diag.message.contains("'puts'"), "message={}", diag.message);
        assert!(diag.help.iter().any(|h| h.contains("effects { io }")));
        assert_eq!(diagnostics.len(), 1, "diags={diagnostics:#?}");
        // The declaration still parses so later phases keep checking callers.
        assert!(program.is_some());
    }

    #[test]
    fn reports_extern_capabilities_and_contracts() {
        for clause in [
            "effects { io } capabilities { io }",
            "effects { } requires true",
            "effects { io } effects { fs }",
        ] {
            let src = format!(r#"extern "C" fn f(x: Int) -> Int {clause};"#);
            let (_program, diagnostics) = parse(&src, "test.aic");
            assert!(
                diagnostics.iter().any(|d| d.code == "E1066"),
                "clause={clause} diags={diagnostics:#?}"
            );
            assert!(
                !diagnostics.iter().any(|d| d.code == "E2010"),
                "clause={clause} diags={diagnostics:#?}"
            );
        }
    }

    #[test]
    fn reports_unsafe_item_without_fn() {
        let src = "unsafe struct Bad { x: Int }";
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.iter().any(|d| d.code == "E1068"));
    }

    #[test]
    fn reports_extern_without_abi_string() {
        let src = "extern fn c_abs(x: Int) -> Int;";
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.iter().any(|d| d.code == "E1063"));
    }

    #[test]
    fn parses_trait_impl_and_generic_bounds() {
        let src = r#"
trait Order[T];
impl Order[Int];

fn pick[T: Order](a: T, b: T) -> T {
    a
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        assert_eq!(program.items.len(), 3);
    }

    #[test]
    fn parses_where_clause_with_multiple_bounds() {
        let src = r#"
trait A[T];
trait B[T];

fn pick[T](x: T) -> T where T: A + B {
    x
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[2] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert_eq!(function.generics.len(), 1);
        assert_eq!(function.generics[0].name, "T");
        assert_eq!(
            function.generics[0].bounds,
            vec!["A".to_string(), "B".to_string()]
        );
    }

    #[test]
    fn parses_result_propagation_operator() {
        let src = r#"
fn parse(x: Int) -> Result[Int, Int] {
    Ok(x)
}

fn bump(x: Int) -> Result[Int, Int] {
    let v = parse(x)?;
    Ok(v + 1)
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        assert!(matches!(tail.kind, ExprKind::Call { .. }));
        assert!(matches!(
            function.body.stmts[0],
            crate::ast::Stmt::Let {
                expr: Expr {
                    kind: ExprKind::Try { .. },
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn parses_mutable_binding_assignment_and_borrow() {
        let src = r#"
fn main() -> Int {
    let mut x = 1;
    let r = &x;
    x = x + 1;
    x
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert!(matches!(
            function.body.stmts[0],
            crate::ast::Stmt::Let { mutable: true, .. }
        ));
        assert!(matches!(
            function.body.stmts[2],
            crate::ast::Stmt::Assign { .. }
        ));
    }

    #[test]
    fn reports_missing_assignment_semicolon() {
        let src = r#"
fn main() -> Int {
    let mut x = 1;
    x = 2
    x
}
"#;
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.iter().any(|d| d.code == "E1062"));
    }

    #[test]
    fn parses_float_literals_and_preserves_suffix_metadata() {
        let src = r#"
fn main() -> Float {
    3.125f32 + 2.5e-3f64
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert!(matches!(
            &function.ret_type.kind,
            TypeKind::Named { name, args } if name == "Float" && args.is_empty()
        ));
        let tail = function.body.tail.as_ref().expect("tail expression");
        let ExprKind::Binary { lhs, rhs, .. } = &tail.kind else {
            panic!("expected binary expression");
        };
        assert!(matches!(lhs.kind, ExprKind::Float(v, ..) if (v - 3.125).abs() < 1e-12));
        assert!(matches!(rhs.kind, ExprKind::Float(v, ..) if (v - 2.5e-3).abs() < 1e-12));

        let lhs_meta = lhs
            .float_literal_metadata()
            .expect("expected typed float metadata for lhs");
        assert_eq!(lhs_meta.suffix, FloatLiteralSuffix::F32);
        assert_eq!(lhs_meta.suffix_text, "f32");
        assert_eq!(lhs_meta.kind.width, FloatLiteralWidth::W32);
        assert_eq!(lhs_meta.raw_literal_text, "3.125");
        assert!(lhs_meta.raw_value_span.end < lhs.span.end);

        let rhs_meta = rhs
            .float_literal_metadata()
            .expect("expected typed float metadata for rhs");
        assert_eq!(rhs_meta.suffix, FloatLiteralSuffix::F64);
        assert_eq!(rhs_meta.suffix_text, "f64");
        assert_eq!(rhs_meta.kind.width, FloatLiteralWidth::W64);
        assert_eq!(rhs_meta.raw_literal_text, "2.5e-3");
        assert!(rhs_meta.raw_value_span.end < rhs.span.end);
    }

    #[test]
    fn parses_fixed_width_types_in_function_signatures() {
        let src = r#"
fn widen(
    a: Int8,
    b: Int16,
    c: Int32,
    d: Int64,
    i: Int128,
    e: UInt8,
    f: UInt16,
    g: UInt32,
    h: UInt64,
) -> UInt128 {
    h
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let expected = [
            "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32", "UInt64",
        ];
        for (param, expected_name) in f.params.iter().zip(expected.iter()) {
            assert!(matches!(
                &param.ty.kind,
                TypeKind::Named { name, args } if name == expected_name && args.is_empty()
            ));
        }
        assert!(matches!(
            &f.ret_type.kind,
            TypeKind::Named { name, args } if name == "UInt128" && args.is_empty()
        ));
    }

    #[test]
    fn parses_size_primitives_and_canonicalizes_uint_alias() {
        let src = r#"
fn widths(a: ISize, b: USize, c: UInt) -> UInt {
    c
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let expected = ["ISize", "USize", "USize"];
        for (param, expected_name) in f.params.iter().zip(expected.iter()) {
            assert!(matches!(
                &param.ty.kind,
                TypeKind::Named { name, args } if name == expected_name && args.is_empty()
            ));
        }
        assert!(matches!(
            &f.ret_type.kind,
            TypeKind::Named { name, args } if name == "USize" && args.is_empty()
        ));
    }

    #[test]
    fn parses_float_primitives_and_preserves_float_alias_spelling() {
        let src = r#"
fn widths(a: Float32, b: Float64, c: Float) -> Float {
    let _a: Float32 = a;
    let _b: Float64 = b;
    let _c: Float = c;
    c
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let expected = ["Float32", "Float64", "Float"];
        for (param, expected_name) in f.params.iter().zip(expected.iter()) {
            assert!(matches!(
                &param.ty.kind,
                TypeKind::Named { name, args } if name == expected_name && args.is_empty()
            ));
        }
        assert!(matches!(
            &f.ret_type.kind,
            TypeKind::Named { name, args } if name == "Float" && args.is_empty()
        ));
        let Stmt::Let {
            ty: Some(alias_ty), ..
        } = &f.body.stmts[2]
        else {
            panic!("expected third let with alias type");
        };
        assert!(matches!(
            &alias_ty.kind,
            TypeKind::Named { name, args } if name == "Float" && args.is_empty()
        ));
    }

    #[test]
    fn records_typed_integer_metadata_for_exprs_and_patterns() {
        let src = r#"
fn main() -> Int {
    let a: Int8 = 1i8;
    let b: UInt128 = 340282366920938463463374607431768211455u128;
    match a {
        1i8 => 1,
        _ => 0,
    }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let f = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let Stmt::Let { expr, .. } = &f.body.stmts[0] else {
            panic!("expected let statement");
        };
        let expr_meta = expr
            .int_literal_metadata()
            .expect("expected typed int metadata for expression literal");
        assert_eq!(expr_meta.suffix, IntLiteralSuffix::I8);
        assert_eq!(expr_meta.suffix_text, "i8");
        assert_eq!(expr_meta.kind.signedness, IntLiteralSignedness::Signed);
        assert_eq!(expr_meta.kind.width, IntLiteralWidth::W8);
        assert!(expr_meta.raw_value_span.end < expr.span.end);

        let Stmt::Let {
            expr: expr_u128, ..
        } = &f.body.stmts[1]
        else {
            panic!("expected second let statement");
        };
        let expr_u128_meta = expr_u128
            .int_literal_metadata()
            .expect("expected typed int metadata for u128 expression literal");
        assert_eq!(expr_u128_meta.suffix, IntLiteralSuffix::U64);
        assert_eq!(expr_u128_meta.suffix_text, "u128");
        assert_eq!(
            expr_u128_meta.kind.signedness,
            IntLiteralSignedness::Unsigned
        );
        assert_eq!(expr_u128_meta.kind.width, IntLiteralWidth::W128);

        let tail = f.body.tail.as_ref().expect("tail expression");
        let ExprKind::Match { arms, .. } = &tail.kind else {
            panic!("expected match");
        };
        let pattern_meta = arms[0]
            .pattern
            .int_literal_metadata()
            .expect("expected typed int metadata for pattern literal");
        assert_eq!(pattern_meta.suffix, IntLiteralSuffix::I8);
        assert_eq!(pattern_meta.suffix_text, "i8");
        assert_eq!(pattern_meta.kind.signedness, IntLiteralSignedness::Signed);
        assert_eq!(pattern_meta.kind.width, IntLiteralWidth::W8);
    }

    #[test]
    fn reports_invalid_integer_and_float_suffixes_deterministically() {
        let src = r#"
fn bad_int() -> Int { 1i33 }
fn bad_float() -> Float { 1.5u8 }
"#;
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "E0009" && d.message.contains("invalid integer literal suffix")),
            "diags={diagnostics:#?}"
        );
        assert!(
            diagnostics.iter().any(|d| d.code == "E0010"
                && d.message.contains("invalid float literal suffix")
                && d.message.contains("u8")),
            "diags={diagnostics:#?}"
        );
    }

    #[test]
    fn parses_char_literals() {
        let src = r#"
fn main() -> Char {
    '\u{1F600}'
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        assert!(matches!(tail.kind, ExprKind::Char('😀')));
    }

    #[test]
    fn parses_template_literals_and_nested_interpolations() {
        let src = r#"
fn main() -> String {
    let name = "Ada";
    f"Hello, {name} {int_to_string(20 + 22)}"
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };

        let tail = function.body.tail.as_ref().expect("tail expression");
        let ExprKind::TemplateLiteral { template, args } = &tail.kind else {
            panic!("expected parsed template literal AST node");
        };
        assert_eq!(template, "Hello, {0} {1}");
        assert_eq!(args.len(), 2);
        assert!(matches!(args[0].kind, ExprKind::Var(ref name) if name == "name"));
        let ExprKind::Call {
            callee: nested_callee,
            args: nested_args,
            ..
        } = &args[1].kind
        else {
            panic!("expected nested interpolation call");
        };
        assert!(matches!(
            nested_callee.kind,
            ExprKind::Var(ref name) if name == "int_to_string"
        ));
        assert_eq!(nested_args.len(), 1);
        assert!(matches!(
            nested_args[0].kind,
            ExprKind::Binary { op: BinOp::Add, .. }
        ));
    }

    #[test]
    fn parses_template_literal_escaped_braces_without_interpolation() {
        let src = r#"
fn main() -> String {
    f"left \{mid\} right"
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        assert!(matches!(
            &tail.kind,
            ExprKind::String(value) if value == "left {mid} right"
        ));
    }

    #[test]
    fn parses_template_literal_double_braces_without_interpolation() {
        let src = r#"
fn main() -> String {
    f"left {{mid}} right"
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        assert!(matches!(
            &tail.kind,
            ExprKind::String(value) if value == "left {mid} right"
        ));
    }

    #[test]
    fn reports_unescaped_closing_brace_in_template_literal() {
        let src = r#"
fn main() -> String {
    f"oops }"
}
"#;
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "E1001" && d.message.contains("unescaped '}'")),
            "diags={diagnostics:#?}"
        );
    }
    #[test]
    fn parses_typed_holes_in_all_supported_type_positions() {
        let src = r#"
struct Boxed {
    value: _,
}

fn passthrough(x: _) -> _ {
    let y: _ = x;
    y
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        assert_eq!(program.items.len(), 2);

        let strukt = match &program.items[0] {
            Item::Struct(s) => s,
            _ => panic!("expected struct"),
        };
        assert!(matches!(strukt.fields[0].ty.kind, TypeKind::Hole));

        let function = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        assert!(matches!(function.params[0].ty.kind, TypeKind::Hole));
        assert!(matches!(function.ret_type.kind, TypeKind::Hole));
        let Stmt::Let { ty, .. } = &function.body.stmts[0] else {
            panic!("expected let statement");
        };
        let Some(let_ty) = ty else {
            panic!("expected let type annotation");
        };
        assert!(matches!(let_ty.kind, TypeKind::Hole));
    }

    #[test]
    fn parses_bitwise_and_shift_precedence() {
        let src = r#"
fn main() -> Int {
    1 | 2 ^ 3 & 4 << 1
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = function.body.tail.as_ref().expect("tail expression");
        let ExprKind::Binary {
            op: BinOp::BitOr,
            lhs: _,
            rhs,
        } = &tail.kind
        else {
            panic!("expected top-level bitwise or");
        };
        let ExprKind::Binary {
            op: BinOp::BitXor,
            lhs: _,
            rhs,
        } = &rhs.kind
        else {
            panic!("expected xor nested under bitwise or");
        };
        let ExprKind::Binary {
            op: BinOp::BitAnd,
            lhs: _,
            rhs,
        } = &rhs.kind
        else {
            panic!("expected and nested under xor");
        };
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Shl, .. }));
    }

    #[test]
    fn parses_compound_bitwise_assignments() {
        let src = r#"
fn main() -> Int {
    let mut x = 0xFF;
    x &= 0x0F;
    x |= 0xF0;
    x ^= 0xAA;
    x <<= 1;
    x >>= 1;
    x >>>= 1;
    x
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[0] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        for (index, expected) in [
            BinOp::BitAnd,
            BinOp::BitOr,
            BinOp::BitXor,
            BinOp::Shl,
            BinOp::Shr,
            BinOp::Ushr,
        ]
        .iter()
        .enumerate()
        {
            let stmt = &function.body.stmts[index + 1];
            let Stmt::Assign { op, .. } = stmt else {
                panic!("expected assignment statement");
            };
            assert_eq!(op.as_ref(), Some(expected));
        }
    }

    #[test]
    fn parses_struct_field_defaults() {
        let src = r#"
struct Config {
    port: Int = 40 + 2,
    enabled: Bool = true,
    retries: Int,
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let strukt = match &program.items[0] {
            Item::Struct(s) => s,
            _ => panic!("expected struct"),
        };
        assert_eq!(strukt.fields.len(), 3);
        assert!(matches!(
            strukt.fields[0]
                .default_value
                .as_ref()
                .map(|expr| &expr.kind),
            Some(ExprKind::Binary { op: BinOp::Add, .. })
        ));
        assert!(matches!(
            strukt.fields[1]
                .default_value
                .as_ref()
                .map(|expr| &expr.kind),
            Some(ExprKind::Bool(true))
        ));
        assert!(strukt.fields[2].default_value.is_none());
    }
    #[test]
    fn parses_named_call_arguments_metadata() {
        let src = r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry { host + port + timeout_ms } else { 0 }
}

fn main() -> Int {
    connect(timeout_ms: 30, retry: true, host: 10, port: 2)
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        let program = program.expect("program");
        let function = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = function.body.tail.as_ref().expect("tail");
        let ExprKind::Call {
            arg_names, args, ..
        } = &tail.kind
        else {
            panic!("expected call expression");
        };
        assert_eq!(args.len(), 4);
        assert_eq!(
            arg_names,
            &vec![
                Some("timeout_ms".to_string()),
                Some("retry".to_string()),
                Some("host".to_string()),
                Some("port".to_string())
            ]
        );
    }

    #[test]
    fn reports_positional_argument_after_named_in_call() {
        let src = r#"
fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry { host + port + timeout_ms } else { 0 }
}

fn main() -> Int {
    connect(host: 10, 2, timeout_ms: 30, retry: true)
}
"#;
        let (_, diagnostics) = parse(src, "test.aic");
        assert!(
            diagnostics.iter().any(|d| d.code == "E1092"),
            "diags={diagnostics:#?}"
        );
    }

    fn parse_terminating(src: &str) -> (Option<crate::ast::Program>, Vec<String>) {
        let (program, diagnostics, stats) = super::parse_with_stats(src, "test.aic");
        assert!(
            !stats.budget_exhausted,
            "parser exhausted its step budget: {stats:?} diags={diagnostics:#?}"
        );
        assert!(
            stats.steps <= stats.tokens * 32 + 512,
            "parser used too many steps for {} tokens: {stats:?}",
            stats.tokens
        );
        let codes = diagnostics.iter().map(|d| d.code.clone()).collect();
        (program, codes)
    }

    fn item_names(program: &crate::ast::Program) -> Vec<String> {
        program
            .items
            .iter()
            .map(|item| match item {
                Item::Function(f) => format!("fn {}", f.name),
                Item::Struct(s) => format!("struct {}", s.name),
                Item::Enum(e) => format!("enum {}", e.name),
                Item::Trait(t) => format!("trait {}", t.name),
                Item::Impl(i) => format!("impl {}", i.trait_name),
            })
            .collect()
    }

    #[test]
    fn trait_associated_type_and_default_body_do_not_hang() {
        let src = r#"
trait Shape[T] {
    type Output;
    fn area(self: T) -> Int { 0 }
}

struct Sq { s: Int }

fn main() -> Int { 0 }
"#;
        let (program, codes) = parse_terminating(src);
        assert_eq!(codes, vec!["E1084", "E1098"]);
        let program = program.expect("program");
        assert_eq!(
            item_names(&program),
            vec!["trait Shape", "struct Sq", "fn main"]
        );
        let Item::Trait(shape) = &program.items[0] else {
            panic!("expected trait");
        };
        // The signature of a method with a default body is kept so later
        // phases still see the trait's full interface.
        assert_eq!(shape.methods.len(), 1);
        assert_eq!(shape.methods[0].name, "area");
    }

    #[test]
    fn trait_associated_type_forms_report_e1084() {
        for member in [
            "type Output;",
            "type Output = Int;",
            "pub type Output;",
            "#[doc(\"x\")] type Output;",
        ] {
            let src = format!(
                "trait Shape[T] {{\n    {member}\n    fn area(self: T) -> Int;\n}}\n\nfn main() -> Int {{ 0 }}\n"
            );
            let (program, codes) = parse_terminating(&src);
            assert!(
                codes.iter().any(|c| c == "E1084"),
                "member={member} codes={codes:?}"
            );
            assert!(
                !codes.iter().any(|c| c == "E1082"),
                "member={member} codes={codes:?}"
            );
            let program = program.expect("program");
            assert_eq!(item_names(&program), vec!["trait Shape", "fn main"]);
            let Item::Trait(shape) = &program.items[0] else {
                panic!("expected trait");
            };
            assert_eq!(shape.methods.len(), 1, "member={member}");
        }
    }

    #[test]
    fn trait_associated_const_reports_e1085() {
        for member in ["const MAX: Int;", "const MAX: Int = 3;"] {
            let src = format!(
                "trait Limits[T] {{\n    {member}\n    fn max(self: T) -> Int;\n}}\n\nfn main() -> Int {{ 0 }}\n"
            );
            let (program, codes) = parse_terminating(&src);
            assert_eq!(codes, vec!["E1085"], "member={member}");
            let program = program.expect("program");
            assert_eq!(item_names(&program), vec!["trait Limits", "fn main"]);
        }
    }

    #[test]
    fn trait_default_bodies_with_nested_blocks_report_e1098() {
        let src = r#"
trait Count[T] {
    fn count(self: T) -> Int {
        if true { 1 } else { match 2 { _ => { 3 } } }
    }
    async fn later(self: T) -> Int { 0 }
    fn plain(self: T) -> Int;
}

fn main() -> Int { 0 }
"#;
        let (program, codes) = parse_terminating(src);
        assert_eq!(codes, vec!["E1098", "E1098"]);
        let program = program.expect("program");
        let Item::Trait(count) = &program.items[0] else {
            panic!("expected trait");
        };
        let names: Vec<_> = count.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["count", "later", "plain"]);
        assert_eq!(item_names(&program), vec!["trait Count", "fn main"]);
    }

    #[test]
    fn impl_associated_items_report_and_keep_methods() {
        let src = r#"
struct Sq { s: Int }

impl Sq {
    type Output = Int;
    const SIDES: Int = 4;
    fn area(self: Sq) -> Int { self.s * self.s }
}

fn main() -> Int { 0 }
"#;
        let (program, codes) = parse_terminating(src);
        assert_eq!(codes, vec!["E1084", "E1085"]);
        let program = program.expect("program");
        assert_eq!(
            item_names(&program),
            vec!["struct Sq", "impl Sq", "fn main"]
        );
        let Item::Impl(imp) = &program.items[1] else {
            panic!("expected impl");
        };
        assert_eq!(imp.methods.len(), 1);
    }

    #[test]
    fn trait_missing_closing_brace_recovers_following_items() {
        let src = r#"
trait Shape[T] {
    type Output;
    fn area(self: T) -> Int;

struct Sq { s: Int }

fn main() -> Int { 0 }
"#;
        let (program, codes) = parse_terminating(src);
        assert!(codes.iter().any(|c| c == "E1084"), "codes={codes:?}");
        assert!(codes.iter().any(|c| c == "E1086"), "codes={codes:?}");
        let program = program.expect("program");
        assert_eq!(item_names(&program), vec!["struct Sq", "fn main"]);
    }

    #[test]
    fn malformed_trait_and_impl_members_terminate() {
        let cases = [
            "trait T[A] { type }",
            "trait T[A] { const }",
            "trait T[A] { type type type }",
            "trait T[A] { fn f(self: A) -> Int { ",
            "trait T[A] { fn f(self: A) -> Int { } } }",
            "trait T[A] { 1 2 3 ; ; } fn main() -> Int { 0 }",
            "trait T[A] { pub } struct S { x: Int }",
            "trait T[A] { #[x] } enum E { A }",
            "impl S { type const fn } fn main() -> Int { 0 }",
            "impl S { fn f() -> Int { { { } } struct X { a: Int }",
            "impl S { } } } type",
            "trait struct enum impl type const fn",
            "pub priv pub",
            "# [ # [",
        ];
        for src in cases {
            let (_program, codes) = parse_terminating(src);
            assert!(!codes.is_empty(), "expected diagnostics for {src:?}");
            assert!(
                !codes.iter().any(|c| c == "E1099"),
                "src={src:?} codes={codes:?}"
            );
        }
    }

    #[test]
    fn exhausted_step_budget_stops_parser_with_e1099() {
        let src = "struct S { x: Int }\nfn main() -> Int { let a = 1; let b = 2; a + b }\n";
        let (_program, diagnostics, stats) =
            super::parse_with_step_budget(src, "test.aic", Some(12));
        assert!(stats.budget_exhausted, "stats={stats:?}");
        assert_eq!(stats.budget, 12);
        let e1099: Vec<_> = diagnostics.iter().filter(|d| d.code == "E1099").collect();
        assert_eq!(e1099.len(), 1, "diags={diagnostics:#?}");
        assert!(e1099[0].message.contains("without making progress"));
        // Steps stay bounded after exhaustion: every loop sees EOF and unwinds.
        assert!(stats.steps < 12 + 64, "stats={stats:?}");
    }

    #[test]
    fn step_budget_is_token_proportional() {
        assert_eq!(super::parser_step_budget(0), super::PARSER_STEP_BUDGET_BASE);
        assert_eq!(
            super::parser_step_budget(10),
            10 * super::PARSER_STEPS_PER_TOKEN + super::PARSER_STEP_BUDGET_BASE
        );
        assert_eq!(super::parser_step_budget(usize::MAX), usize::MAX);

        let src = "fn main() -> Int { 0 }\n".repeat(200);
        let (_program, diagnostics, stats) = super::parse_with_stats(&src, "test.aic");
        assert!(diagnostics.is_empty(), "diags={diagnostics:#?}");
        assert!(!stats.budget_exhausted);
        assert_eq!(stats.budget, super::parser_step_budget(stats.tokens));
        assert!(stats.steps < stats.budget / 4, "stats={stats:?}");
    }

    #[test]
    fn return_parses_as_expression_in_arms_and_block_tails() {
        let src = r#"
fn f(x: Option[Int]) -> Int {
    let a = match x {
        Some(v) => v,
        None => return 0,
    };
    if a > 1 { a } else { return -1 }
}

fn g() -> () {
    if true { return } else { () }
}
"#;
        let (program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let program = program.expect("program");
        let Item::Function(f) = &program.items[0] else {
            panic!("expected fn");
        };
        let Stmt::Let { expr, .. } = &f.body.stmts[0] else {
            panic!("expected let");
        };
        let ExprKind::Match { arms, .. } = &expr.kind else {
            panic!("expected match");
        };
        assert!(matches!(
            &arms[1].body.kind,
            ExprKind::Return { expr: Some(value) } if matches!(value.kind, ExprKind::Int(0, ..))
        ));
        let ExprKind::If { else_block, .. } = &f.body.tail.as_ref().expect("tail").kind else {
            panic!("expected if tail");
        };
        assert!(else_block.stmts.is_empty());
        assert!(matches!(
            else_block.tail.as_deref().map(|e| &e.kind),
            Some(ExprKind::Return { expr: Some(_) })
        ));
        let Item::Function(g) = &program.items[1] else {
            panic!("expected fn");
        };
        let ExprKind::If { then_block, .. } = &g.body.tail.as_ref().expect("tail").kind else {
            panic!("expected if tail");
        };
        assert!(matches!(
            then_block.tail.as_deref().map(|e| &e.kind),
            Some(ExprKind::Return { expr: None })
        ));
    }

    #[test]
    fn return_statement_without_semicolon_mid_block_still_reports_e1034() {
        let src = "fn f() -> Int {\n    return 1\n    let x = 2;\n    x\n}\n";
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(
            diagnostics.iter().any(|d| d.code == "E1034"),
            "{diagnostics:#?}"
        );
    }
}
