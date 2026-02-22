use crate::diagnostics::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Int(i64),
    String(String),

    KwModule,
    KwImport,
    KwAsync,
    KwExtern,
    KwUnsafe,
    KwFn,
    KwStruct,
    KwEnum,
    KwTrait,
    KwImpl,
    KwLet,
    KwMut,
    KwReturn,
    KwIf,
    KwElse,
    KwMatch,
    KwWhile,
    KwLoop,
    KwBreak,
    KwContinue,
    KwTrue,
    KwFalse,
    KwRequires,
    KwEnsures,
    KwInvariant,
    KwEffects,
    KwNull,
    KwAwait,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    Arrow,
    FatArrow,
    ColonColon,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    Ampersand,
    Pipe,
    OrOr,
    Bang,
    Question,
    Underscore,

    Eof,
}

pub fn lex(source: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        source,
        file,
        offset: 0,
        diagnostics: Vec::new(),
        tokens: Vec::new(),
    };
    lexer.run();
    (lexer.tokens, lexer.diagnostics)
}

struct Lexer<'a> {
    source: &'a str,
    file: &'a str,
    offset: usize,
    diagnostics: Vec<Diagnostic>,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\n' | '\r' | '\t' => {
                    self.bump();
                }
                '/' if self.peek_next() == Some('/') => {
                    self.bump();
                    self.bump();
                    while let Some(next) = self.peek() {
                        if next == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                'a'..='z' | 'A'..='Z' => self.lex_ident_or_keyword(),
                '_' => {
                    let start = self.offset;
                    self.bump();
                    if self
                        .peek()
                        .map(|c| c.is_ascii_alphanumeric())
                        .unwrap_or(false)
                    {
                        self.lex_ident_rest(start);
                    } else {
                        self.push(TokenKind::Underscore, Span::new(start, start + 1));
                    }
                }
                '0'..='9' => self.lex_int(),
                '"' => self.lex_string(),
                '(' => self.single(TokenKind::LParen),
                ')' => self.single(TokenKind::RParen),
                '{' => self.single(TokenKind::LBrace),
                '}' => self.single(TokenKind::RBrace),
                '[' => self.single(TokenKind::LBracket),
                ']' => self.single(TokenKind::RBracket),
                ',' => self.single(TokenKind::Comma),
                ';' => self.single(TokenKind::Semi),
                '.' => self.single(TokenKind::Dot),
                '+' => self.single(TokenKind::Plus),
                '*' => self.single(TokenKind::Star),
                '%' => self.single(TokenKind::Percent),
                ':' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some(':') {
                        self.bump();
                        self.push(TokenKind::ColonColon, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Colon, Span::new(start, self.offset));
                    }
                }
                '-' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('>') {
                        self.bump();
                        self.push(TokenKind::Arrow, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Minus, Span::new(start, self.offset));
                    }
                }
                '=' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::EqEq, Span::new(start, self.offset));
                    } else if self.peek() == Some('>') {
                        self.bump();
                        self.push(TokenKind::FatArrow, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Eq, Span::new(start, self.offset));
                    }
                }
                '!' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Ne, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Bang, Span::new(start, self.offset));
                    }
                }
                '?' => self.single(TokenKind::Question),
                '<' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Le, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Lt, Span::new(start, self.offset));
                    }
                }
                '>' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Ge, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Gt, Span::new(start, self.offset));
                    }
                }
                '&' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('&') {
                        self.bump();
                        self.push(TokenKind::AndAnd, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Ampersand, Span::new(start, self.offset));
                    }
                }
                '|' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('|') {
                        self.bump();
                        self.push(TokenKind::OrOr, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Pipe, Span::new(start, self.offset));
                    }
                }
                '/' => self.single(TokenKind::Slash),
                _ => {
                    let start = self.offset;
                    self.bump();
                    self.error(
                        "E0001",
                        format!("unexpected character '{}'", c),
                        Span::new(start, self.offset),
                    );
                }
            }
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.offset, self.offset),
        });
    }

    fn lex_ident_or_keyword(&mut self) {
        let start = self.offset;
        self.bump();
        self.lex_ident_rest(start);
    }

    fn lex_ident_rest(&mut self, start: usize) {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        let text = &self.source[start..self.offset];
        let kind = match text {
            "module" => TokenKind::KwModule,
            "import" => TokenKind::KwImport,
            "async" => TokenKind::KwAsync,
            "extern" => TokenKind::KwExtern,
            "unsafe" => TokenKind::KwUnsafe,
            "fn" => TokenKind::KwFn,
            "struct" => TokenKind::KwStruct,
            "enum" => TokenKind::KwEnum,
            "trait" => TokenKind::KwTrait,
            "impl" => TokenKind::KwImpl,
            "let" => TokenKind::KwLet,
            "mut" => TokenKind::KwMut,
            "return" => TokenKind::KwReturn,
            "if" => TokenKind::KwIf,
            "else" => TokenKind::KwElse,
            "match" => TokenKind::KwMatch,
            "while" => TokenKind::KwWhile,
            "loop" => TokenKind::KwLoop,
            "break" => TokenKind::KwBreak,
            "continue" => TokenKind::KwContinue,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "requires" => TokenKind::KwRequires,
            "ensures" => TokenKind::KwEnsures,
            "invariant" => TokenKind::KwInvariant,
            "effects" => TokenKind::KwEffects,
            "null" => TokenKind::KwNull,
            "await" => TokenKind::KwAwait,
            _ => TokenKind::Ident(text.to_string()),
        };
        self.push(kind, Span::new(start, self.offset));
    }

    fn lex_int(&mut self) {
        let start = self.offset;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else {
                break;
            }
        }
        let text = &self.source[start..self.offset];
        match text.parse::<i64>() {
            Ok(value) => self.push(TokenKind::Int(value), Span::new(start, self.offset)),
            Err(_) => self.error(
                "E0004",
                format!("invalid integer literal '{}'", text),
                Span::new(start, self.offset),
            ),
        }
    }

    fn lex_string(&mut self) {
        let start = self.offset;
        self.bump();
        let mut out = String::new();
        let mut terminated = false;
        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    self.bump();
                    terminated = true;
                    break;
                }
                '\\' => {
                    self.bump();
                    match self.peek() {
                        Some('n') => {
                            self.bump();
                            out.push('\n');
                        }
                        Some('t') => {
                            self.bump();
                            out.push('\t');
                        }
                        Some('"') => {
                            self.bump();
                            out.push('"');
                        }
                        Some('\\') => {
                            self.bump();
                            out.push('\\');
                        }
                        Some(other) => {
                            let esc_start = self.offset;
                            self.bump();
                            self.error(
                                "E0005",
                                format!("unsupported escape sequence '\\{}'", other),
                                Span::new(esc_start.saturating_sub(1), self.offset),
                            );
                        }
                        None => break,
                    }
                }
                '\n' => {
                    self.error(
                        "E0006",
                        "unterminated string literal",
                        Span::new(start, self.offset),
                    );
                    break;
                }
                _ => {
                    self.bump();
                    out.push(c);
                }
            }
        }
        if !terminated {
            self.error(
                "E0006",
                "unterminated string literal",
                Span::new(start, self.offset),
            );
            return;
        }
        self.push(TokenKind::String(out), Span::new(start, self.offset));
    }

    fn single(&mut self, kind: TokenKind) {
        let start = self.offset;
        self.bump();
        self.push(kind, Span::new(start, self.offset));
    }

    fn push(&mut self, kind: TokenKind, span: Span) {
        self.tokens.push(Token { kind, span });
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.file, span));
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut iter = self.source[self.offset..].chars();
        iter.next()?;
        iter.next()
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.offset += c.len_utf8();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{lex, TokenKind};

    #[test]
    fn lexes_keywords_and_symbols() {
        let src = "async fn main() -> Int effects { io } { let mut x = await ping()?; let y = &mut x; while true { continue; } loop { break; } match x { Some(v) | None => v } }";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty());
        assert!(matches!(tokens[0].kind, TokenKind::KwAsync));
        assert!(matches!(tokens[1].kind, TokenKind::KwFn));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Arrow)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::KwEffects)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwMut)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Ampersand)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Pipe)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwAwait)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Question)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwWhile)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwLoop)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwBreak)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::KwContinue)));
    }

    #[test]
    fn lexes_strings() {
        let src = r#"let x = "hello";"#;
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty());
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::String(s) if s == "hello")));
    }

    #[test]
    fn lexes_trait_and_impl_keywords() {
        let src = "trait Order[T]; impl Order[Int];";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty());
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwTrait)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwImpl)));
    }

    #[test]
    fn lexes_extern_and_unsafe_keywords() {
        let src = r#"extern "C" fn c_abs(x: Int) -> Int; unsafe fn wrap(x: Int) -> Int { unsafe { c_abs(x) } }"#;
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty());
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwExtern)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwUnsafe)));
    }
}
