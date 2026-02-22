use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::lexer::{lex, Token, TokenKind};
use crate::span::Span;

pub fn parse(source: &str, file: &str) -> (Option<Program>, Vec<Diagnostic>) {
    let (tokens, mut diagnostics) = lex(source, file);
    let mut parser = Parser {
        file,
        tokens,
        index: 0,
        diagnostics: Vec::new(),
        for_counter: 0,
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
    disallow_struct_literal: bool,
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
        if self.at_kind(|k| matches!(k, TokenKind::KwExtern)) {
            self.parse_extern_function().map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwType)) {
            self.parse_type_alias().map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwConst)) {
            self.parse_const_item().map(Item::Function)
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
            self.parse_function(false, true, start).map(Item::Function)
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
            self.parse_function(true, false, start).map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwFn)) {
            let start = self.current_span().start;
            self.parse_function(false, false, start).map(Item::Function)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwStruct)) {
            self.parse_struct().map(Item::Struct)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwEnum)) {
            self.parse_enum().map(Item::Enum)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwTrait)) {
            self.parse_trait().map(Item::Trait)
        } else if self.at_kind(|k| matches!(k, TokenKind::KwImpl)) {
            self.parse_impl().map(Item::Impl)
        } else {
            let span = self.current_span();
            self.diagnostics.push(
                Diagnostic::error(
                    "E1003",
                    "expected item declaration (`fn`, `async fn`, `unsafe fn`, `extern \"C\" fn`, `type`, `const`, `struct`, `enum`, `trait`, `impl`)",
                    self.file,
                    span,
                )
                .with_help("define functions or types at module scope"),
            );
            None
        }
    }

    fn parse_function(
        &mut self,
        is_async: bool,
        is_unsafe: bool,
        start: usize,
    ) -> Option<Function> {
        self.bump(); // fn
        let (name, _) = self.expect_ident("E1004", "expected function name")?;
        let mut generics = self.parse_generics();
        self.expect(
            |k| matches!(k, TokenKind::LParen),
            "E1005",
            "expected '(' after function name",
        )?;
        let params = self.parse_params()?;
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1006",
            "expected '->' with function return type",
        )?;
        let ret_type = self.parse_type()?;
        self.parse_where_clause(&mut generics);
        let effects = if self.at_kind(|k| matches!(k, TokenKind::KwEffects)) {
            self.bump();
            self.expect(
                |k| matches!(k, TokenKind::LBrace),
                "E1007",
                "expected '{' after effects",
            )?;
            let mut effs = Vec::new();
            while !self.at_kind(|k| matches!(k, TokenKind::RBrace)) {
                let (name, _) = self.expect_ident("E1008", "expected effect name")?;
                effs.push(name);
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
            effs
        } else {
            Vec::new()
        };

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
            is_async,
            is_unsafe,
            is_extern: false,
            extern_abi: None,
            generics,
            params,
            ret_type,
            effects,
            requires,
            ensures,
            body,
            span,
        })
    }

    fn parse_extern_function(&mut self) -> Option<Function> {
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
        let params = self.parse_params()?;
        self.expect(
            |k| matches!(k, TokenKind::Arrow),
            "E1006",
            "expected '->' with function return type",
        )?;
        let ret_type = self.parse_type()?;

        if self.at_kind(|k| {
            matches!(
                k,
                TokenKind::KwEffects | TokenKind::KwRequires | TokenKind::KwEnsures
            )
        }) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E1066",
                    "extern function declarations cannot have effects/contracts",
                    self.file,
                    self.current_span(),
                )
                .with_help("declare `extern` signatures only; wrap them in normal functions for effects/contracts"),
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
            is_async: false,
            is_unsafe: false,
            is_extern: true,
            extern_abi: Some(abi),
            generics,
            params,
            ret_type,
            effects: Vec::new(),
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

    fn parse_struct(&mut self) -> Option<StructDef> {
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
            let (field_name, _) = self.expect_ident("E1012", "expected field name")?;
            self.expect(
                |k| matches!(k, TokenKind::Colon),
                "E1013",
                "expected ':' after field name",
            )?;
            let ty = self.parse_type()?;
            fields.push(Field {
                name: field_name,
                ty: ty.clone(),
                span: Span::new(field_start, ty.span.end),
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
            generics,
            fields,
            invariant,
            span: Span::new(start, close.end),
        })
    }

    fn parse_enum(&mut self) -> Option<EnumDef> {
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
            generics,
            variants,
            span: Span::new(start, close.end),
        })
    }

    fn parse_trait(&mut self) -> Option<TraitDef> {
        let start = self.current_span().start;
        self.bump(); // trait
        let (name, _) = self.expect_ident("E1053", "expected trait name")?;
        let generics = self.parse_generics();
        let end = self
            .expect(
                |k| matches!(k, TokenKind::Semi),
                "E1054",
                "expected ';' after trait declaration",
            )?
            .end;
        Some(TraitDef {
            name,
            generics,
            span: Span::new(start, end),
        })
    }

    fn parse_impl(&mut self) -> Option<ImplDef> {
        let start = self.current_span().start;
        self.bump(); // impl
        let (trait_name, _) = self.expect_ident("E1055", "expected trait name after impl")?;
        self.expect(
            |k| matches!(k, TokenKind::LBracket),
            "E1056",
            "expected '[' after trait name in impl",
        )?;
        let mut trait_args = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RBracket)) {
            trait_args.push(self.parse_type()?);
            if self.at_kind(|k| matches!(k, TokenKind::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(
            |k| matches!(k, TokenKind::RBracket),
            "E1057",
            "expected ']' after impl type arguments",
        )?;
        let end = self
            .expect(
                |k| matches!(k, TokenKind::Semi),
                "E1058",
                "expected ';' after impl declaration",
            )?
            .end;
        Some(ImplDef {
            trait_name,
            trait_args,
            span: Span::new(start, end),
        })
    }

    fn parse_type_alias(&mut self) -> Option<Function> {
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
            is_async: false,
            is_unsafe: false,
            is_extern: false,
            extern_abi: None,
            generics,
            params: Vec::new(),
            ret_type: target_ty,
            effects: Vec::new(),
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

    fn parse_const_item(&mut self) -> Option<Function> {
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
            is_async: false,
            is_unsafe: false,
            is_extern: false,
            extern_abi: None,
            generics: Vec::new(),
            params: Vec::new(),
            ret_type: const_ty,
            effects: Vec::new(),
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

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
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

    fn parse_type(&mut self) -> Option<TypeExpr> {
        let start = self.current_span().start;
        if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
            self.bump();
            self.expect(
                |k| matches!(k, TokenKind::RParen),
                "E1025",
                "expected ')' for unit type",
            )?;
            let end = self.previous_span().end;
            return Some(TypeExpr {
                kind: TypeKind::Unit,
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
                    Some(stmt) => stmts.push(stmt),
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

    fn parse_let_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        self.bump(); // let
        let mutable = if self.at_kind(|k| matches!(k, TokenKind::KwMut)) {
            self.bump();
            true
        } else {
            false
        };
        let (name, _) = self.expect_ident("E1031", "expected binding name after let")?;
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
        Some(Stmt::Let {
            name,
            mutable,
            ty,
            expr,
            span: Span::new(start, end),
        })
    }

    fn parse_assign_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        let (target, _) = self.expect_ident("E1060", "expected assignment target")?;
        self.expect(
            |k| matches!(k, TokenKind::Eq),
            "E1061",
            "expected '=' in assignment",
        )?;
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
        let mut expr = self.parse_equality()?;
        while self.at_kind(|k| matches!(k, TokenKind::AndAnd)) {
            self.bump();
            let rhs = self.parse_equality()?;
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
        let mut expr = self.parse_term()?;
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
        if self.at_kind(|k| matches!(k, TokenKind::Pipe)) {
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
        self.parse_postfix()
    }

    fn parse_closure_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(
            |k| matches!(k, TokenKind::Pipe),
            "E1071",
            "expected '|' to start closure expression",
        )?;

        let mut params = Vec::new();
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
                while !self.at_kind(|k| matches!(k, TokenKind::RParen)) {
                    let arg = self.parse_expr()?;
                    args.push(arg);
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
                let span = Span::new(expr.span.start, close.end);
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
                continue;
            }

            if self.at_kind(|k| matches!(k, TokenKind::Dot)) {
                self.bump();
                let (field, field_span) =
                    self.expect_ident("E1036", "expected field name after '.'")?;
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
            TokenKind::Int(value) => {
                self.bump();
                Some(Expr {
                    kind: ExprKind::Int(value),
                    span: token.span,
                })
            }
            TokenKind::Float(value) => {
                self.bump();
                Some(Expr {
                    kind: ExprKind::Float(value),
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
                let expr = self.parse_expr()?;
                let close = self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1037",
                    "expected ')' after expression",
                )?;
                Some(Expr {
                    kind: expr.kind,
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
                if !self.disallow_struct_literal
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
                        kind: ExprKind::StructInit { name, fields },
                        span: Span::new(token.span.start, close.end),
                    });
                }
                Some(Expr {
                    kind: ExprKind::Var(name),
                    span: token.span,
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

        Some(self.desugar_for_vec(binding, iterable, body, start))
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
        format!("__aic_for_{prefix}_{id}")
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

    fn desugar_for_vec(
        &mut self,
        binding: String,
        iterable: Expr,
        body: Block,
        start: usize,
    ) -> Expr {
        let id = self.next_for_id();
        let span = Span::new(start, body.span.end);
        let iter_name = self.make_for_name("iter", id);
        let index_name = self.make_for_name("index", id);

        let mut some_body_stmts = vec![Stmt::Assign {
            target: index_name.clone(),
            expr: Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Add,
                    lhs: Box::new(self.make_var_expr(index_name.clone(), span)),
                    rhs: Box::new(self.make_int_expr(1, span)),
                },
                span,
            },
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
            stmts: vec![Stmt::Expr {
                expr: Expr {
                    kind: ExprKind::Match {
                        expr: Box::new(Expr {
                            kind: ExprKind::Call {
                                callee: Box::new(self.make_var_expr("aic_vec_get_intrinsic", span)),
                                args: vec![
                                    self.make_var_expr(iter_name.clone(), span),
                                    self.make_var_expr(index_name.clone(), span),
                                ],
                            },
                            span,
                        }),
                        arms: vec![some_arm, none_arm],
                    },
                    span,
                },
                span,
            }],
            tail: None,
            span,
        };

        let then_block = Block {
            stmts: vec![
                Stmt::Let {
                    name: iter_name.clone(),
                    mutable: false,
                    ty: None,
                    expr: iterable,
                    span,
                },
                Stmt::Let {
                    name: index_name,
                    mutable: true,
                    ty: None,
                    expr: self.make_int_expr(0, span),
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
            TokenKind::Int(value) => {
                self.bump();
                Some(Pattern {
                    kind: PatternKind::Int(value),
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
                self.expect(
                    |k| matches!(k, TokenKind::RParen),
                    "E1046",
                    "expected ')' for unit pattern",
                )?;
                Some(Pattern {
                    kind: PatternKind::Unit,
                    span: Span::new(start, self.previous_span().end),
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                if self.at_kind(|k| matches!(k, TokenKind::LParen)) {
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
                .map(|token| matches!(token.kind, TokenKind::Eq))
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
    use crate::ast::{Expr, ExprKind, Item, PatternKind, Stmt};

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
    fn parses_float_literals() {
        let src = r#"
fn main() -> Float {
    3.125 + 2.5e-3
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
        let ExprKind::Binary { lhs, rhs, .. } = &tail.kind else {
            panic!("expected binary expression");
        };
        assert!(matches!(lhs.kind, ExprKind::Float(v) if (v - 3.125).abs() < 1e-12));
        assert!(matches!(rhs.kind, ExprKind::Float(v) if (v - 2.5e-3).abs() < 1e-12));
    }
}
