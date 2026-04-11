use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::ir;
use crate::lexer::{lex, FloatLiteralSuffixToken, IntLiteralSuffixToken, Token, TokenKind};
use crate::span::Span;

const TUPLE_INTERNAL_NAME: &str = "Tuple";
const TUPLE_LET_TMP_PREFIX: &str = "__tuple_let_";

pub fn parse(source: &str, file: &str) -> (Option<Program>, Vec<Diagnostic>) {
    clear_int_literal_metadata();
    clear_float_literal_metadata();
    ir::clear_int_literal_metadata();
    ir::clear_float_literal_metadata();
    let (tokens, mut diagnostics) = lex(source, file);
    let mut parser = Parser {
        file,
        tokens,
        index: 0,
        diagnostics: Vec::new(),
        for_counter: 0,
        tuple_binding_counter: 0,
        disallow_struct_literal: false,
    };
    let program = parser.parse_program();
    diagnostics.extend(parser.diagnostics);
    (program, diagnostics)
}

struct Parser<'a> {
    file: &'a str,
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
    for_counter: usize,
    tuple_binding_counter: usize,
    disallow_struct_literal: bool,
}

#[derive(Debug, Clone)]
enum TupleLetPattern {
    Binding { name: String, span: Span },
    Wildcard,
    Tuple { items: Vec<TupleLetPattern> },
}

impl<'a> Parser<'a> {
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
            match self.parse_item() {
                Some(item) => items.push(item),
                None => {
                    if self.at_kind(|k| matches!(k, TokenKind::Eof)) {
                        break;
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
            self.parse_extern_function(attrs, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwIntrinsic)) {
            self.parse_intrinsic_function(attrs, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwType)) {
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
            self.parse_function(attrs, true, false, start, false, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
            let start = self.current_span().start;
            self.parse_function(attrs, false, false, start, false, visibility)
                .map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwStruct)) {
            self.parse_struct(attrs, visibility).map(Item::Struct)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwEnum)) {
            self.parse_enum(attrs, visibility).map(Item::Enum)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwTrait)) {
            self.parse_trait(attrs, visibility).map(Item::Trait)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwImpl)) {
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
                requires = Some(self.parse_expr()?);
                continue;
            }
            if self.at_kind(|k| matches!(k, TokenKind::KwEnsures)) {
                self.bump();
                ensures = Some(self.parse_expr()?);
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
                    "extern function declarations cannot have effects/capabilities/contracts",
                    self.file,
                    self.current_span(),
                )
                .with_help("declare `extern` signatures only; wrap them in normal functions for effects/capabilities/contracts"),
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
            effects: Vec::new(),
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
        self.expect(
            |k| matches!(k, TokenKind::LBrace),
            "E1011",
            "expected '{' after struct name",
        )?;
        let mut fields = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
            let field_start = self.current_span().start;
            let field_attrs = self.parse_attributes();
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
            Some(self.parse_expr()?)
        } else {
            None
        };

        Some(StructDef {
            name,
            visibility,
            attrs,
            generics,
            fields,
            invariant,
            span: Span::new(start, close.end),
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
            let (var_name, _) = self.expect_ident("E1017", "expected enum variant name")?;
            let payload = if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
                self.bump();
                let ty = self.parse_type()?;
                self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1018",
                    "expected ')' after variant payload type",
                )?;
                Some(ty)
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
            let method_start = self.current_span().start;
            let method_attrs = self.parse_attributes();
            let method_visibility = self.parse_visibility_modifier();
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
                self.recover_item();
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
                self.recover_item();
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
                let method_start = self.current_span().start;
                let method_attrs = self.parse_attributes();
                let method_visibility = self.parse_visibility_modifier();
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
                    self.recover_item();
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
                    self.recover_item();
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
            if allow_self_receiver && params.is_empty() {
                if self.at_kind(|k| matches!(k, TokenKind::Ampersand)) {
                    let recv_start = self.current_span().start;
                    self.bump();
                    if self.at_kind(|k| matches!(k, TokenKind::KwMut)) {
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
                match self.parse_return_stmt() {
                    Some(stmt) => stmts.push(stmt),
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

        let tuple_pattern = if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
            let parsed = self.parse_pattern()?;
            Some(self.convert_let_pattern(&parsed)?)
        } else {
            None
        };

        let name = if tuple_pattern.is_none() {
            Some(
                self.expect_ident("E1031", "expected binding name after let")?
                    .0,
            )
        } else {
            None
        };

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
        if let Some(pattern) = tuple_pattern {
            if mutable {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E1031",
                        "tuple destructuring does not support `let mut`; mark individual bindings mutable later",
                        self.file,
                        span,
                    )
                    .with_help("remove `mut` here and use separate mutable bindings if needed"),
                );
            }
            return Some(self.expand_tuple_let(pattern, ty, expr, span));
        }

        Some(vec![Stmt::Let {
            name: name.expect("name present"),
            mutable,
            ty,
            expr,
            span,
        }])
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
        }
    }

    fn next_tuple_let_name(&mut self) -> String {
        let id = self.tuple_binding_counter;
        self.tuple_binding_counter += 1;
        format!("{TUPLE_LET_TMP_PREFIX}{id}")
    }

    fn parse_assign_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        let (target, target_span) = self.expect_ident("E1060", "expected assignment target")?;
        let compound_op = match &self.current().kind {
            TokenKind::Eq => None,
            TokenKind::AmpEq => Some(BinOp::BitAnd),
            TokenKind::PipeEq => Some(BinOp::BitOr),
            TokenKind::CaretEq => Some(BinOp::BitXor),
            TokenKind::LShiftEq => Some(BinOp::Shl),
            TokenKind::RShiftEq => Some(BinOp::Shr),
            TokenKind::URShiftEq => Some(BinOp::Ushr),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E1061",
                    "expected assignment operator ('=', '&=', '|=', '^=', '<<=', '>>=', '>>>=')",
                    self.file,
                    self.current_span(),
                ));
                return None;
            }
        };
        self.bump();
        let rhs_expr = self.parse_expr()?;
        let expr = if let Some(op) = compound_op {
            let lhs_expr = Expr::var(target.clone(), target_span);
            let span = lhs_expr.span.join(rhs_expr.span);
            Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs_expr),
                    rhs: Box::new(rhs_expr),
                },
                span,
            }
        } else {
            rhs_expr
        };
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
            expr,
            span: Span::new(start, end),
        })
    }

    fn parse_return_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        self.bump();
        let expr = if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        let end = if self.at_kind(|k| matches!(k, TokenKind::Semi)) {
            let span = self.current_span();
            self.bump();
            span.end
        } else {
            let span = self.current_span();
            let insert_at = expr.as_ref().map(|e| e.span.end).unwrap_or(span.start);
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
            insert_at
        };
        Some(Stmt::Return {
            expr,
            span: Span::new(start, end),
        })
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_or()
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
        let mut expr = self.parse_unary()?;
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
            let rhs = self.parse_unary()?;
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
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1074",
            "expected '->' after closure parameters",
        )?;
        let ret_type = self.parse_type()?;
        let body = self.parse_block()?;
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
        loop {
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
                            let value = self.parse_expr()?;
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
                        let arg = self.parse_expr()?;
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
                self.record_typed_int_literal(
                    token.span,
                    literal.value,
                    literal.suffix,
                    literal.raw_value_span,
                    literal.raw_text,
                );
                Some(Expr {
                    kind: ExprKind::Int(literal.value),
                    span: token.span,
                })
            }
            TokenKind::Float(literal) => {
                self.bump();
                let float_value = literal.value;
                self.record_typed_float_literal(
                    token.span,
                    literal.suffix,
                    literal.raw_value_span,
                    literal.raw_text,
                );
                Some(Expr {
                    kind: ExprKind::Float(float_value),
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
                let first = self.parse_expr()?;
                if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                    self.bump();
                    let mut items = vec![first];
                    while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                        let item = self.parse_expr()?;
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
            TokenKind::KwFor => self.parse_for_expr(),
            TokenKind::KwWhile => self.parse_while_expr(),
            TokenKind::KwLoop => self.parse_loop_expr(),
            TokenKind::KwBreak => self.parse_break_expr(),
            TokenKind::KwContinue => self.parse_continue_expr(),
            TokenKind::KwMatch => self.parse_match_expr(),
            TokenKind::KwUnsafe => {
                self.bump();
                let block = self.parse_block()?;
                Some(Expr {
                    span: Span::new(token.span.start, block.span.end),
                    kind: ExprKind::UnsafeBlock { block },
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                let mut full_name = name;
                let mut end = token.span.end;
                while self.at_kind(|k| matches!(k, TokenKind::ColonColon)) {
                    self.bump();
                    let (segment, seg_span) =
                        self.expect_ident("E1088", "expected path segment after '::'")?;
                    full_name.push_str("::");
                    full_name.push_str(&segment);
                    end = seg_span.end;
                }
                if !self.disallow_struct_literal
                    && !full_name.contains("::")
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
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        self.expect(
            |k| matches!(k, TokenKind::KwElse),
            "E1042",
            "expected else branch for if expression",
        )?;

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
        let body = if self.at_kind(|k| matches!(k, TokenKind::DotDot)) {
            self.bump(); // ..
            let end = self.parse_expr_without_struct_literals()?;
            let body = self.parse_block()?;
            return Some(self.desugar_for_range(binding, iterable, end, body, start));
        } else {
            self.parse_block()?
        };

        Some(self.desugar_for_iter(binding, iterable, body, start))
    }

    fn parse_while_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // while
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        let end = body.span.end;
        Some(Expr {
            kind: ExprKind::While {
                cond: Box::new(cond),
                body,
            },
            span: Span::new(start, end),
        })
    }

    fn parse_loop_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // loop
        let body = self.parse_block()?;
        let end = body.span.end;
        Some(Expr {
            kind: ExprKind::Loop { body },
            span: Span::new(start, end),
        })
    }

    fn parse_break_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // break
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
            kind: ExprKind::Break { expr },
            span: Span::new(start, end),
        })
    }

    fn parse_continue_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.bump(); // continue
        Some(Expr {
            kind: ExprKind::Continue,
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

        let mut parser = Parser {
            file: self.file,
            tokens,
            index: 0,
            diagnostics: Vec::new(),
            for_counter: self.for_counter,
            tuple_binding_counter: self.tuple_binding_counter,
            disallow_struct_literal: false,
        };

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
        let expr = self.parse_expr();
        self.disallow_struct_literal = prev;
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
            kind: ExprKind::Int(value),
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

    fn record_typed_int_literal(
        &mut self,
        span: Span,
        value: i64,
        suffix: Option<IntLiteralSuffixToken>,
        raw_value_span: Span,
        raw_literal_text: String,
    ) {
        let Some(suffix) = suffix else {
            return;
        };
        let (ast_suffix, ast_kind, suffix_text) = Self::int_literal_suffix_to_ast(suffix);
        let ast_meta = IntLiteralMetadata::with_kind_and_suffix_text(
            ast_suffix,
            ast_kind,
            suffix_text,
            raw_value_span,
            raw_literal_text,
        );
        record_int_literal_metadata(span, value, ast_meta.clone());
        ir::record_int_literal_metadata(span, value, ast_meta);
    }

    fn record_typed_float_literal(
        &mut self,
        span: Span,
        suffix: Option<FloatLiteralSuffixToken>,
        raw_value_span: Span,
        raw_literal_text: String,
    ) {
        let Some(suffix) = suffix else {
            return;
        };
        let (ast_suffix, ast_kind, suffix_text) = Self::float_literal_suffix_to_ast(suffix);
        let ast_meta = FloatLiteralMetadata::with_kind_and_suffix_text(
            ast_suffix,
            ast_kind,
            suffix_text,
            raw_value_span,
            raw_literal_text,
        );
        record_float_literal_metadata(span, ast_meta.clone());
        ir::record_float_literal_metadata(span, ast_meta);
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
                kind: ExprKind::Break { expr: None },
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
                    },
                    span,
                },
                span,
            }],
            tail: Some(Box::new(Expr {
                kind: ExprKind::Loop { body: loop_body },
                span,
            })),
            span,
        };

        self.wrap_if_true_expr(then_block, span)
    }

    fn desugar_for_range(
        &mut self,
        binding: String,
        start_expr: Expr,
        end_expr: Expr,
        body: Block,
        start: usize,
    ) -> Expr {
        let id = self.next_for_id();
        let span = Span::new(start, body.span.end);
        let cur_name = self.make_for_name("range_cur", id);
        let end_name = self.make_for_name("range_end", id);

        let mut range_then_stmts = vec![
            Stmt::Let {
                name: binding,
                mutable: false,
                ty: None,
                expr: self.make_var_expr(cur_name.clone(), span),
                span,
            },
            Stmt::Assign {
                target: cur_name.clone(),
                expr: Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(self.make_var_expr(cur_name.clone(), span)),
                        rhs: Box::new(self.make_int_expr(1, span)),
                    },
                    span,
                },
                span,
            },
        ];
        range_then_stmts.extend(self.for_body_to_stmts(&body));

        let range_if_expr = Expr {
            kind: ExprKind::If {
                cond: Box::new(Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Lt,
                        lhs: Box::new(self.make_var_expr(cur_name.clone(), span)),
                        rhs: Box::new(self.make_var_expr(end_name.clone(), span)),
                    },
                    span,
                }),
                then_block: Block {
                    stmts: range_then_stmts,
                    tail: None,
                    span: body.span,
                },
                else_block: Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(Expr {
                        kind: ExprKind::Break { expr: None },
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
            ],
            tail: Some(Box::new(Expr {
                kind: ExprKind::Loop { body: loop_body },
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
        let expr = self.parse_expr()?;
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
            } else {
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
        match token.kind {
            TokenKind::Underscore => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::Wildcard,
                    span: token.span,
                })
            }
            TokenKind::Int(literal) => {
                self.bump();
                self.record_typed_int_literal(
                    token.span,
                    literal.value,
                    literal.suffix,
                    literal.raw_value_span,
                    literal.raw_text,
                );
                Some(Pattern {
                    kind: PatternKind::Int(literal.value),
                    span: token.span,
                })
            }
            TokenKind::Char(value) => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::Char(value),
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
            TokenKind::Ident(name) => {
                self.bump();
                if self.at_kind(|k| matches!(k, TokenKind::LBrace)) {
                    let start = token.span.start;
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
                    let start = token.span.start;
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
                    Some(Pattern {
                        kind: PatternKind::Variant { name, args },
                        span: Span::new(start, close.end),
                    })
                } else if name
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
                        span: token.span,
                    })
                } else {
                    Some(Pattern {
                        kind: PatternKind::Var(name),
                        span: token.span,
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

    fn at_assignment_stmt_start(&self) -> bool {
        matches!(self.current().kind, TokenKind::Ident(_))
            && self
                .peek(1)
                .map(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Eq
                            | TokenKind::AmpEq
                            | TokenKind::PipeEq
                            | TokenKind::CaretEq
                            | TokenKind::LShiftEq
                            | TokenKind::RShiftEq
                            | TokenKind::URShiftEq
                    )
                })
                .unwrap_or(false)
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
        &self.tokens[self.index.min(self.tokens.len().saturating_sub(1))]
    }

    fn peek(&self, n: usize) -> Option<&Token> {
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
        let ExprKind::Loop { body } = &tail.kind else {
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
    return
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
    fn reports_async_without_fn_keyword() {
        let src = "async struct Bad { x: Int }";
        let (_program, diagnostics) = parse(src, "test.aic");
        assert!(diagnostics.iter().any(|d| d.code == "E1052"));
    }

    #[test]
    fn parses_extern_c_function_and_unsafe_block() {
        let src = r#"
extern "C" fn c_abs(x: Int) -> Int;

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

        let wrap_fn = match &program.items[1] {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let tail = wrap_fn.body.tail.as_ref().expect("tail");
        assert!(matches!(tail.kind, ExprKind::UnsafeBlock { .. }));
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
        assert!(matches!(lhs.kind, ExprKind::Float(v) if (v - 3.125).abs() < 1e-12));
        assert!(matches!(rhs.kind, ExprKind::Float(v) if (v - 2.5e-3).abs() < 1e-12));

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
            let Stmt::Assign { expr, .. } = stmt else {
                panic!("expected assignment statement");
            };
            let ExprKind::Binary { op, .. } = &expr.kind else {
                panic!("compound assignment should parse as binary expression");
            };
            assert_eq!(op, expected);
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
}
