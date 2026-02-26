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
    Float(f64),
    String(String),
    Template(String),
    Char(char),

    KwModule,
    KwImport,
    KwAsync,
    KwExtern,
    KwUnsafe,
    KwFn,
    KwType,
    KwConst,
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
    KwFor,
    KwIn,
    KwWhile,
    KwLoop,
    KwBreak,
    KwContinue,
    KwTrue,
    KwFalse,
    KwRequires,
    KwEnsures,
    KwWhere,
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
    DotDot,
    Arrow,
    FatArrow,
    ColonColon,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Tilde,
    Eq,
    EqEq,
    AmpEq,
    PipeEq,
    CaretEq,
    LShift,
    RShift,
    URShift,
    LShiftEq,
    RShiftEq,
    URShiftEq,
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
                'f' if self.peek_next() == Some('"') => self.lex_template_string(),
                '$' if self.peek_next() == Some('"') => self.lex_template_string(),
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
                '0'..='9' => self.lex_number(),
                '"' => self.lex_string(),
                '\'' => self.lex_char(),
                '(' => self.single(TokenKind::LParen),
                ')' => self.single(TokenKind::RParen),
                '{' => self.single(TokenKind::LBrace),
                '}' => self.single(TokenKind::RBrace),
                '[' => self.single(TokenKind::LBracket),
                ']' => self.single(TokenKind::RBracket),
                ',' => self.single(TokenKind::Comma),
                ';' => self.single(TokenKind::Semi),
                '.' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('.') {
                        self.bump();
                        self.push(TokenKind::DotDot, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Dot, Span::new(start, self.offset));
                    }
                }
                '+' => self.single(TokenKind::Plus),
                '*' => self.single(TokenKind::Star),
                '%' => self.single(TokenKind::Percent),
                '~' => self.single(TokenKind::Tilde),
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
                    if self.peek() == Some('<') {
                        self.bump();
                        if self.peek() == Some('=') {
                            self.bump();
                            self.push(TokenKind::LShiftEq, Span::new(start, self.offset));
                        } else {
                            self.push(TokenKind::LShift, Span::new(start, self.offset));
                        }
                    } else if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::Le, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Lt, Span::new(start, self.offset));
                    }
                }
                '>' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('>') {
                        self.bump();
                        if self.peek() == Some('>') {
                            self.bump();
                            if self.peek() == Some('=') {
                                self.bump();
                                self.push(TokenKind::URShiftEq, Span::new(start, self.offset));
                            } else {
                                self.push(TokenKind::URShift, Span::new(start, self.offset));
                            }
                        } else if self.peek() == Some('=') {
                            self.bump();
                            self.push(TokenKind::RShiftEq, Span::new(start, self.offset));
                        } else {
                            self.push(TokenKind::RShift, Span::new(start, self.offset));
                        }
                    } else if self.peek() == Some('=') {
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
                    } else if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::AmpEq, Span::new(start, self.offset));
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
                    } else if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::PipeEq, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Pipe, Span::new(start, self.offset));
                    }
                }
                '^' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::CaretEq, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Caret, Span::new(start, self.offset));
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
            "type" => TokenKind::KwType,
            "const" => TokenKind::KwConst,
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
            "for" => TokenKind::KwFor,
            "in" => TokenKind::KwIn,
            "while" => TokenKind::KwWhile,
            "loop" => TokenKind::KwLoop,
            "break" => TokenKind::KwBreak,
            "continue" => TokenKind::KwContinue,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "requires" => TokenKind::KwRequires,
            "ensures" => TokenKind::KwEnsures,
            "where" => TokenKind::KwWhere,
            "invariant" => TokenKind::KwInvariant,
            "effects" => TokenKind::KwEffects,
            "null" => TokenKind::KwNull,
            "await" => TokenKind::KwAwait,
            _ => TokenKind::Ident(text.to_string()),
        };
        self.push(kind, Span::new(start, self.offset));
    }

    fn lex_number(&mut self) {
        let start = self.offset;
        if self.peek() == Some('0') && matches!(self.peek_next(), Some('x' | 'X')) {
            self.bump();
            self.bump();
            let digits_start = self.offset;
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    self.bump();
                } else {
                    break;
                }
            }
            if digits_start == self.offset {
                self.error(
                    "E0004",
                    "invalid hex integer literal",
                    Span::new(start, self.offset),
                );
                return;
            }
            let text = &self.source[digits_start..self.offset];
            match i64::from_str_radix(text, 16) {
                Ok(value) => self.push(TokenKind::Int(value), Span::new(start, self.offset)),
                Err(_) => self.error(
                    "E0004",
                    format!("invalid integer literal '0x{}'", text),
                    Span::new(start, self.offset),
                ),
            }
            return;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else {
                break;
            }
        }
        let mut is_float = false;
        if self.peek() == Some('.')
            && self
                .peek_next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            is_float = true;
            self.bump(); // '.'
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let sign = self.peek_nth(1);
            let exp_digit = match sign {
                Some('+') | Some('-') => self.peek_nth(2),
                _ => sign,
            };
            if exp_digit.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                is_float = true;
                self.bump(); // e/E
                if matches!(self.peek(), Some('+' | '-')) {
                    self.bump();
                }
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }
        let text = &self.source[start..self.offset];
        if is_float {
            match text.parse::<f64>() {
                Ok(value) => self.push(TokenKind::Float(value), Span::new(start, self.offset)),
                Err(_) => self.error(
                    "E0007",
                    format!("invalid float literal '{}'", text),
                    Span::new(start, self.offset),
                ),
            }
        } else {
            match text.parse::<i64>() {
                Ok(value) => self.push(TokenKind::Int(value), Span::new(start, self.offset)),
                Err(_) => self.error(
                    "E0004",
                    format!("invalid integer literal '{}'", text),
                    Span::new(start, self.offset),
                ),
            }
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
                    if let Some(value) = self.lex_escape_sequence() {
                        out.push(value);
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

    fn lex_template_string(&mut self) {
        let start = self.offset;
        self.bump(); // template prefix
        self.bump(); // opening quote

        let content_start = self.offset;
        let mut terminated = false;
        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    terminated = true;
                    break;
                }
                '\\' => {
                    self.bump();
                    if self.peek().is_none() {
                        break;
                    }
                    self.bump();
                }
                '\n' => break,
                _ => self.bump(),
            }
        }

        if !terminated {
            self.error(
                "E0006",
                "unterminated template string literal",
                Span::new(start, self.offset),
            );
            return;
        }

        let content_end = self.offset;
        self.bump(); // closing quote
        let raw = self.source[content_start..content_end].to_string();
        self.push(TokenKind::Template(raw), Span::new(start, self.offset));
    }

    fn lex_char(&mut self) {
        let start = self.offset;
        self.bump(); // opening quote

        if self.peek() == Some('\'') {
            self.bump();
            self.error(
                "E0008",
                "char literal must contain exactly one Unicode codepoint",
                Span::new(start, self.offset),
            );
            return;
        }

        let value = match self.peek() {
            Some('\n') | None => {
                self.error(
                    "E0006",
                    "unterminated char literal",
                    Span::new(start, self.offset),
                );
                return;
            }
            Some('\\') => {
                self.bump();
                self.lex_escape_sequence()
            }
            Some(ch) => {
                self.bump();
                Some(ch)
            }
        };

        if self.peek() == Some('\'') {
            self.bump();
        } else {
            while let Some(ch) = self.peek() {
                if ch == '\'' {
                    self.bump();
                    break;
                }
                if ch == '\n' {
                    break;
                }
                self.bump();
            }
            self.error(
                "E0008",
                "char literal must contain exactly one Unicode codepoint",
                Span::new(start, self.offset),
            );
            return;
        }

        if let Some(ch) = value {
            self.push(TokenKind::Char(ch), Span::new(start, self.offset));
        }
    }

    fn lex_escape_sequence(&mut self) -> Option<char> {
        match self.peek() {
            Some('n') => {
                self.bump();
                Some('\n')
            }
            Some('r') => {
                self.bump();
                Some('\r')
            }
            Some('t') => {
                self.bump();
                Some('\t')
            }
            Some('0') => {
                self.bump();
                Some('\0')
            }
            Some('"') => {
                self.bump();
                Some('"')
            }
            Some('\'') => {
                self.bump();
                Some('\'')
            }
            Some('\\') => {
                self.bump();
                Some('\\')
            }
            Some('u') => self.lex_unicode_escape(),
            Some(other) => {
                let esc_start = self.offset;
                self.bump();
                self.error(
                    "E0005",
                    format!("unsupported escape sequence '\\\\{}'", other),
                    Span::new(esc_start.saturating_sub(1), self.offset),
                );
                None
            }
            None => None,
        }
    }

    fn lex_unicode_escape(&mut self) -> Option<char> {
        let escape_start = self.offset.saturating_sub(1); // include '\\'
        self.bump(); // consume 'u'
        if self.peek() != Some('{') {
            self.error(
                "E0005",
                "invalid Unicode escape, expected `\\u{...}`",
                Span::new(escape_start, self.offset),
            );
            return None;
        }
        self.bump(); // consume '{'
        let digits_start = self.offset;
        while let Some(c) = self.peek() {
            if c.is_ascii_hexdigit() {
                self.bump();
            } else {
                break;
            }
        }
        let digits_end = self.offset;
        if self.peek() != Some('}') {
            self.error(
                "E0005",
                "invalid Unicode escape, expected closing `}`",
                Span::new(escape_start, self.offset),
            );
            return None;
        }
        self.bump(); // consume '}'

        if digits_start == digits_end {
            self.error(
                "E0005",
                "invalid Unicode escape, missing codepoint digits",
                Span::new(escape_start, self.offset),
            );
            return None;
        }

        let digits = &self.source[digits_start..digits_end];
        if digits.len() > 6 {
            self.error(
                "E0005",
                "invalid Unicode escape, codepoint has too many hex digits",
                Span::new(escape_start, self.offset),
            );
            return None;
        }

        let Some(codepoint) = u32::from_str_radix(digits, 16).ok() else {
            self.error(
                "E0005",
                "invalid Unicode escape codepoint",
                Span::new(escape_start, self.offset),
            );
            return None;
        };
        let Some(ch) = char::from_u32(codepoint) else {
            self.error(
                "E0005",
                "invalid Unicode codepoint in escape sequence",
                Span::new(escape_start, self.offset),
            );
            return None;
        };
        Some(ch)
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

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.source[self.offset..].chars().nth(n)
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
        let src = "type Id = Int; const BASE: Int = 1; async fn main() -> Int effects { io } { let mut x = await ping()?; let y = &mut x; while true { continue; } loop { break; } match x { Some(v) | None => v } for i in 0..1 { break; } }";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty());
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwType)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwConst)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwAsync)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwFn)));
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
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwFor)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwIn)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::DotDot)));
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
    fn lexes_template_literals_with_prefixes() {
        let src = r#"let a = f"Hello, {name}! \{ok\}"; let b = $"x{y}";"#;
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens.iter().any(
            |t| matches!(&t.kind, TokenKind::Template(raw) if raw == "Hello, {name}! \\{ok\\}")
        ));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Template(raw) if raw == "x{y}")));
    }
    #[test]
    fn lexes_char_literals_and_escapes() {
        let src = r#"let a = 'x'; let b = '\n'; let c = '\u{1F600}'; let d = '\'';"#;
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Char('x'))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Char('\n'))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Char('😀'))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Char('\''))));
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

    #[test]
    fn lexes_float_literals() {
        let src = "let a = 3.125; let b = 0.5; let c = 1e10; let d = 2.5e-3;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Float(v) if (v - 3.125).abs() < 1e-12)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Float(v) if (v - 0.5).abs() < 1e-12)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Float(v) if (v - 1.0e10).abs() < 1.0)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Float(v) if (v - 2.5e-3).abs() < 1e-12)));
    }

    #[test]
    fn lexes_bitwise_and_shift_tokens_with_hex_literals() {
        let src = "let x = 0xFF & 0x0F | 0xF0 ^ 0x0A; let y = ~x; let z = x << 2; let w = z >> 1; let u = w >>> 3; x &= 1; x |= 2; x ^= 3; x <<= 1; x >>= 1; x >>>= 1;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Int(255))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Ampersand)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Pipe)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Caret)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Tilde)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::LShift)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::RShift)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::URShift)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::AmpEq)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::PipeEq)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::CaretEq)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::LShiftEq)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::RShiftEq)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::URShiftEq)));
    }
}
