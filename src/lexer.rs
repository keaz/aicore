use crate::diagnostics::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntLiteralSuffixToken {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
}

impl IntLiteralSuffixToken {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "i128" => Some(Self::I128),
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            "u128" => Some(Self::U128),
            _ => None,
        }
    }

    fn allowed_suffixes() -> &'static str {
        "i8, i16, i32, i64, i128, u8, u16, u32, u64, u128"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatLiteralSuffixToken {
    F32,
    F64,
}

impl FloatLiteralSuffixToken {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }

    fn allowed_suffixes() -> &'static str {
        "f32, f64"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntLiteralToken {
    pub value: i64,
    pub raw_value_span: Span,
    pub raw_text: String,
    pub suffix: Option<IntLiteralSuffixToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloatLiteralToken {
    pub value: f64,
    pub raw_value_span: Span,
    pub raw_text: String,
    pub suffix: Option<FloatLiteralSuffixToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Int(IntLiteralToken),
    Float(FloatLiteralToken),
    String(String),
    Template(String),
    Char(char),
    /// A loop label, `'outer` (the name without the quote).
    Label(String),

    KwModule,
    KwImport,
    KwAsync,
    KwExtern,
    KwIntrinsic,
    KwUnsafe,
    KwPub,
    KwPriv,
    KwCrate,
    KwFn,
    KwType,
    KwConst,
    KwStruct,
    KwEnum,
    KwTrait,
    KwImpl,
    KwDyn,
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
    KwCapabilities,
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
    /// `..=` (inclusive range).
    DotDotEq,
    /// `@` (pattern binding).
    At,
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
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
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
    Hash,
    Question,
    Underscore,

    Eof,
}

/// A `//` line comment kept as trivia so tools such as the formatter can
/// re-emit it. `text` starts with `//` and excludes the line terminator and
/// trailing whitespace; `span` covers exactly `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub text: String,
    pub span: Span,
}

/// Value of an integer literal's digits as written: decimal, `0x`, `0b`
/// or `0o`, with `_` separators (`1_000`, `0xFF_FF`, `0b1010_0101`).
pub fn parse_int_literal_text(text: &str) -> Option<u128> {
    let trimmed = text.trim();
    let (digits, radix) = match trimmed.get(..2) {
        Some("0x" | "0X") => (&trimmed[2..], 16),
        Some("0b" | "0B") => (&trimmed[2..], 2),
        Some("0o" | "0O") => (&trimmed[2..], 8),
        _ => (trimmed, 10),
    };
    let cleaned = digits.replace('_', "");
    if cleaned.is_empty() {
        return None;
    }
    u128::from_str_radix(&cleaned, radix).ok()
}

/// The value of a multiline string from its raw contents: see
/// `Lexer::lex_multiline_string`.
fn dedent_multiline(raw: &str) -> String {
    let body = raw
        .strip_prefix("\r\n")
        .or_else(|| raw.strip_prefix('\n'))
        .unwrap_or(raw);
    let Some(last_break) = body.rfind('\n') else {
        return body.to_string();
    };
    let indent = &body[last_break + 1..];
    if !indent.chars().all(|c| c == ' ' || c == '\t') {
        return body.to_string();
    }
    let content = body[..last_break]
        .strip_suffix('\r')
        .unwrap_or(&body[..last_break]);
    content
        .split('\n')
        .map(|line| line.strip_prefix(indent).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn lex(source: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let (tokens, _comments, diagnostics) = lex_with_comments(source, file);
    (tokens, diagnostics)
}

/// Lexes `source` like [`lex`] and also returns every line comment, in source
/// order.
pub fn lex_with_comments(source: &str, file: &str) -> (Vec<Token>, Vec<Comment>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        source,
        file,
        offset: 0,
        diagnostics: Vec::new(),
        tokens: Vec::new(),
        comments: Vec::new(),
    };
    lexer.run();
    (lexer.tokens, lexer.comments, lexer.diagnostics)
}

struct Lexer<'a> {
    source: &'a str,
    file: &'a str,
    offset: usize,
    diagnostics: Vec<Diagnostic>,
    tokens: Vec<Token>,
    comments: Vec<Comment>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\n' | '\r' | '\t' => {
                    self.bump();
                }
                '/' if self.peek_next() == Some('/') => {
                    let start = self.offset;
                    self.bump();
                    self.bump();
                    while let Some(next) = self.peek() {
                        if next == '\n' {
                            break;
                        }
                        self.bump();
                    }
                    let text = self.source[start..self.offset].trim_end();
                    self.comments.push(Comment {
                        text: text.to_string(),
                        span: Span::new(start, start + text.len()),
                    });
                }
                '/' if self.peek_next() == Some('*') => self.lex_block_comment(),
                'f' if self.peek_next() == Some('"') => self.lex_template_string(),
                // `r"..."`, `r#"..."#`: raw strings.
                'r' if self.raw_string_hashes().is_some() => self.lex_raw_string(),
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
                '"' if self.source[self.offset..].starts_with("\"\"\"") => {
                    self.lex_multiline_string()
                }
                '"' => self.lex_string(),
                '\'' => self.lex_char(),
                '(' => self.single(TokenKind::LParen),
                ')' => self.single(TokenKind::RParen),
                '{' => self.single(TokenKind::LBrace),
                '}' => self.single(TokenKind::RBrace),
                '[' => self.single(TokenKind::LBracket),
                ']' => self.single(TokenKind::RBracket),
                ',' => self.single(TokenKind::Comma),
                '@' => self.single(TokenKind::At),
                ';' => self.single(TokenKind::Semi),
                '.' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('.') {
                        self.bump();
                        if self.peek() == Some('=') {
                            self.bump();
                            self.push(TokenKind::DotDotEq, Span::new(start, self.offset));
                        } else {
                            self.push(TokenKind::DotDot, Span::new(start, self.offset));
                        }
                    } else {
                        self.push(TokenKind::Dot, Span::new(start, self.offset));
                    }
                }
                '+' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::PlusEq, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Plus, Span::new(start, self.offset));
                    }
                }
                '*' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::StarEq, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Star, Span::new(start, self.offset));
                    }
                }
                '%' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::PercentEq, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Percent, Span::new(start, self.offset));
                    }
                }
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
                    } else if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::MinusEq, Span::new(start, self.offset));
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
                '#' => self.single(TokenKind::Hash),
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
                '/' => {
                    let start = self.offset;
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        self.push(TokenKind::SlashEq, Span::new(start, self.offset));
                    } else {
                        self.push(TokenKind::Slash, Span::new(start, self.offset));
                    }
                }
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
            "intrinsic" => TokenKind::KwIntrinsic,
            "unsafe" => TokenKind::KwUnsafe,
            "pub" => TokenKind::KwPub,
            "priv" => TokenKind::KwPriv,
            "crate" => TokenKind::KwCrate,
            "fn" => TokenKind::KwFn,
            "type" => TokenKind::KwType,
            "const" => TokenKind::KwConst,
            "struct" => TokenKind::KwStruct,
            "enum" => TokenKind::KwEnum,
            "trait" => TokenKind::KwTrait,
            "impl" => TokenKind::KwImpl,
            "dyn" => TokenKind::KwDyn,
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
            "capabilities" => TokenKind::KwCapabilities,
            "null" => TokenKind::KwNull,
            "await" => TokenKind::KwAwait,
            _ => TokenKind::Ident(text.to_string()),
        };
        self.push(kind, Span::new(start, self.offset));
    }

    /// `/* ... */`, nested comments included; kept as one comment.
    fn lex_block_comment(&mut self) {
        let start = self.offset;
        self.bump(); // /
        self.bump(); // *
        let mut depth = 1usize;
        while depth > 0 {
            match (self.peek(), self.peek_next()) {
                (Some('*'), Some('/')) => {
                    self.bump();
                    self.bump();
                    depth -= 1;
                }
                (Some('/'), Some('*')) => {
                    self.bump();
                    self.bump();
                    depth += 1;
                }
                (Some(_), _) => self.bump(),
                (None, _) => {
                    self.error(
                        "E0002",
                        "unterminated block comment: missing `*/`",
                        Span::new(start, self.offset),
                    );
                    return;
                }
            }
        }
        self.comments.push(Comment {
            text: self.source[start..self.offset].to_string(),
            span: Span::new(start, self.offset),
        });
    }

    /// Consumes digits of `radix` and `_` separators. Returns false (after
    /// reporting E0003) when the run has no digit or a digit too large for
    /// the radix.
    fn lex_digit_run(&mut self, radix: u32, literal_start: usize) -> bool {
        let mut digits = 0usize;
        let mut valid = true;
        while let Some(c) = self.peek() {
            if c == '_' {
                // `1_000_i64`: a separator directly before a suffix is fine,
                // but the separator must follow a digit or the prefix.
                self.bump();
            } else if c.is_digit(radix) {
                digits += 1;
                self.bump();
            } else if radix < 10 && c.is_ascii_digit() {
                let at = self.offset;
                self.bump();
                self.error(
                    "E0003",
                    format!("digit '{c}' is not valid in a base-{radix} literal"),
                    Span::new(at, self.offset),
                );
                digits += 1;
                valid = false;
            } else {
                break;
            }
        }
        if digits == 0 {
            self.error(
                "E0003",
                format!(
                    "number literal '{}' has no digits",
                    &self.source[literal_start..self.offset]
                ),
                Span::new(literal_start, self.offset),
            );
        }
        valid && digits > 0
    }

    fn lex_number(&mut self) {
        let start = self.offset;
        let radix = if self.peek() == Some('0') {
            match self.peek_next() {
                Some('x' | 'X') => 16,
                Some('b' | 'B') => 2,
                Some('o' | 'O') => 8,
                _ => 10,
            }
        } else {
            10
        };
        if radix != 10 {
            self.bump();
            self.bump();
            if !self.lex_digit_run(radix, start) {
                return;
            }
            let raw_end = self.offset;
            let raw_text = self.source[start..raw_end].to_string();
            let (suffix, consumed_suffix) = self.lex_integer_suffix();
            self.push_int_literal(start, raw_end, raw_text, suffix, consumed_suffix);
            return;
        }
        self.lex_digit_run(10, start);
        let mut is_float = false;
        if self.peek() == Some('.')
            && self
                .peek_next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            is_float = true;
            self.bump(); // '.'
            self.lex_digit_run(10, start);
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
                self.lex_digit_run(10, start);
            }
        }
        let raw_end = self.offset;
        let raw_text = self.source[start..raw_end].to_string();
        if is_float {
            let (suffix, consumed_suffix) = self.lex_float_suffix();
            let text = raw_text.replace('_', "");
            match text.parse::<f64>() {
                Ok(value) => {
                    let token = FloatLiteralToken {
                        value,
                        raw_value_span: Span::new(start, raw_end),
                        raw_text,
                        suffix,
                    };
                    let end = if token.suffix.is_some() {
                        self.offset
                    } else {
                        raw_end
                    };
                    if consumed_suffix && token.suffix.is_none() {
                        self.push(TokenKind::Float(token), Span::new(start, raw_end));
                    } else {
                        self.push(TokenKind::Float(token), Span::new(start, end));
                    }
                }
                Err(_) => self.error(
                    "E0007",
                    format!("invalid float literal '{}'", raw_text),
                    Span::new(start, raw_end),
                ),
            }
        } else {
            let (suffix, consumed_suffix) = self.lex_integer_suffix();
            self.push_int_literal(start, raw_end, raw_text, suffix, consumed_suffix);
        }
    }

    /// Pushes an integer token for `raw_text` (any radix, `_` separators).
    fn push_int_literal(
        &mut self,
        start: usize,
        raw_end: usize,
        raw_text: String,
        suffix: Option<IntLiteralSuffixToken>,
        consumed_suffix: bool,
    ) {
        let Some(raw_value) = parse_int_literal_text(&raw_text) else {
            self.error(
                "E0004",
                format!("invalid integer literal '{}'", raw_text),
                Span::new(start, raw_end),
            );
            return;
        };
        if raw_value > i64::MAX as u128
            && !matches!(
                suffix,
                Some(IntLiteralSuffixToken::I128 | IntLiteralSuffixToken::U128)
            )
        {
            self.error(
                "E0004",
                format!("invalid integer literal '{}'", raw_text),
                Span::new(start, raw_end),
            );
            return;
        }
        let token = IntLiteralToken {
            value: raw_value as i64,
            raw_value_span: Span::new(start, raw_end),
            raw_text,
            suffix,
        };
        let end = if token.suffix.is_some() {
            self.offset
        } else {
            raw_end
        };
        if consumed_suffix && token.suffix.is_none() {
            self.push(TokenKind::Int(token), Span::new(start, raw_end));
        } else {
            self.push(TokenKind::Int(token), Span::new(start, end));
        }
    }

    fn lex_integer_suffix(&mut self) -> (Option<IntLiteralSuffixToken>, bool) {
        let Some(first) = self.peek() else {
            return (None, false);
        };
        if !Self::is_ident_start(first) {
            return (None, false);
        }
        let suffix_start = self.offset;
        self.bump();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        let suffix_text = &self.source[suffix_start..self.offset];
        let suffix = IntLiteralSuffixToken::parse(suffix_text);
        if suffix.is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E0009",
                    format!("invalid integer literal suffix '{suffix_text}'"),
                    self.file,
                    Span::new(suffix_start, self.offset),
                )
                .with_help(format!(
                    "use one of: {}",
                    IntLiteralSuffixToken::allowed_suffixes()
                )),
            );
        }
        (suffix, true)
    }

    fn lex_float_suffix(&mut self) -> (Option<FloatLiteralSuffixToken>, bool) {
        let Some(first) = self.peek() else {
            return (None, false);
        };
        if !Self::is_ident_start(first) {
            return (None, false);
        }
        let suffix_start = self.offset;
        self.bump();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        let suffix_text = &self.source[suffix_start..self.offset];
        let suffix = FloatLiteralSuffixToken::parse(suffix_text);
        if suffix.is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "E0010",
                    format!("invalid float literal suffix '{suffix_text}'"),
                    self.file,
                    Span::new(suffix_start, self.offset),
                )
                .with_help(format!(
                    "use one of: {}",
                    FloatLiteralSuffixToken::allowed_suffixes()
                )),
            );
        }
        (suffix, true)
    }

    fn is_ident_start(c: char) -> bool {
        c.is_ascii_alphabetic() || c == '_'
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

    /// The number of `#` of a raw string starting here (`r"`, `r#"`).
    fn raw_string_hashes(&self) -> Option<usize> {
        let rest = self.source[self.offset..].strip_prefix('r')?;
        let hashes = rest.chars().take_while(|c| *c == '#').count();
        rest[hashes..].starts_with('"').then_some(hashes)
    }

    /// `r"C:\dir"` or `r#"say "hi""#`: no escapes; the closing quote is
    /// followed by as many `#` as the opening one.
    fn lex_raw_string(&mut self) {
        let start = self.offset;
        let hashes = self.raw_string_hashes().unwrap_or(0);
        self.offset += 1 + hashes + 1; // r, #..., "
        let content_start = self.offset;
        let closing = format!("\"{}", "#".repeat(hashes));
        match self.source[content_start..].find(&closing) {
            Some(len) => {
                let value = self.source[content_start..content_start + len].to_string();
                self.offset = content_start + len + closing.len();
                self.push(TokenKind::String(value), Span::new(start, self.offset));
            }
            None => {
                self.offset = self.source.len();
                self.error(
                    "E0006",
                    format!("unterminated raw string literal: missing `{closing}`"),
                    Span::new(start, self.offset),
                );
            }
        }
    }

    /// `""".."""`: may span lines and contain `"`; escapes work as in
    /// `"..."`. A line break right after the opening quotes is dropped, and
    /// when the closing quotes start their own line, that line's indentation
    /// is removed from every line (and the final line break is dropped).
    fn lex_multiline_string(&mut self) {
        let start = self.offset;
        self.offset += 3;
        let mut out = String::new();
        let mut terminated = false;
        while let Some(c) = self.peek() {
            if self.source[self.offset..].starts_with("\"\"\"") {
                self.offset += 3;
                terminated = true;
                break;
            }
            if c == '\\' {
                self.bump();
                if let Some(value) = self.lex_escape_sequence() {
                    out.push(value);
                }
            } else {
                self.bump();
                out.push(c);
            }
        }
        if !terminated {
            self.error(
                "E0006",
                "unterminated multiline string literal: missing `\"\"\"`",
                Span::new(start, self.offset),
            );
            return;
        }
        self.push(
            TokenKind::String(dedent_multiline(&out)),
            Span::new(start, self.offset),
        );
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

        // `'name` not closed by a quote is a loop label (`'a'` stays a char).
        let label_len = self.source[self.offset..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(self.source.len() - self.offset);
        if label_len > 0
            && self.peek().is_some_and(Self::is_ident_start)
            && self.source[self.offset + label_len..].chars().next() != Some('\'')
        {
            let name = self.source[self.offset..self.offset + label_len].to_string();
            self.offset += label_len;
            self.push(TokenKind::Label(name), Span::new(start, self.offset));
            return;
        }

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
    use super::{
        lex, lex_with_comments, parse_int_literal_text, Comment, FloatLiteralSuffixToken,
        IntLiteralSuffixToken, TokenKind,
    };
    use crate::span::Span;

    #[test]
    fn keeps_line_comments_as_trivia_with_spans() {
        let src = "// head   \nfn main() -> Int { // brace\r\n    \"// not a comment\" /// doc\n}";
        let (tokens, comments, diags) = lex_with_comments(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert_eq!(
            comments,
            vec![
                Comment {
                    text: "// head".to_string(),
                    span: Span::new(0, 7),
                },
                Comment {
                    text: "// brace".to_string(),
                    span: Span::new(30, 38),
                },
                Comment {
                    text: "/// doc".to_string(),
                    span: Span::new(63, 70),
                },
            ]
        );
        for comment in &comments {
            assert_eq!(&src[comment.span.start..comment.span.end], comment.text);
        }
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::String(s) if s == "// not a comment")));

        let (plain_tokens, plain_diags) = lex(src, "test.aic");
        assert_eq!(plain_tokens, tokens);
        assert_eq!(plain_diags, diags);
    }

    #[test]
    fn comment_at_end_of_input_without_newline() {
        let (tokens, comments, _) = lex_with_comments("fn // tail", "test.aic");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "// tail");
        assert!(matches!(
            tokens.last().map(|t| &t.kind),
            Some(TokenKind::Eof)
        ));
    }

    #[test]
    fn lexes_keywords_and_symbols() {
        let src = "type Id = Int; const BASE: Int = 1; async fn main() -> Int effects { io } capabilities { io } { let mut x = await ping()?; let y = &mut x; while true { continue; } loop { break; } match x { Some(v) | None => v } for i in 0..1 { break; } }";
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
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::KwCapabilities)));
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
    fn lexes_size_primitive_names_as_identifiers() {
        let src = "type SI = ISize; type SU = USize; type AU = UInt;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Ident(name) if name == "ISize")));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Ident(name) if name == "USize")));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Ident(name) if name == "UInt")));
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
    fn lexes_range_operators_and_at() {
        let (tokens, diags) = lex("0..5 0..=5 x @ 1..", "test.aic");
        assert!(diags.is_empty(), "{diags:#?}");
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds
                .iter()
                .filter(|k| matches!(k, TokenKind::DotDot))
                .count(),
            2
        );
        assert_eq!(
            kinds
                .iter()
                .filter(|k| matches!(k, TokenKind::DotDotEq))
                .count(),
            1
        );
        assert!(kinds.contains(&TokenKind::At));
    }

    #[test]
    fn lexes_loop_labels_apart_from_char_literals() {
        let (tokens, diags) = lex(
            "'outer: loop { break 'outer 'x'; continue 'a_1; } 'b'",
            "test.aic",
        );
        assert!(diags.is_empty(), "{diags:#?}");
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(kinds[0], TokenKind::Label("outer".to_string()));
        assert!(kinds.contains(&TokenKind::Char('x')));
        assert!(kinds.contains(&TokenKind::Label("a_1".to_string())));
        assert!(kinds.contains(&TokenKind::Char('b')));
        assert_eq!(
            kinds
                .iter()
                .filter(|k| matches!(k, TokenKind::Label(name) if name == "outer"))
                .count(),
            2
        );
        // A quoted word is still a malformed char literal, not a label.
        let (_, diags) = lex("let c = 'ab';", "test.aic");
        assert!(diags.iter().any(|d| d.code == "E0008"), "{diags:#?}");
    }

    #[test]
    fn lexes_raw_and_multiline_strings() {
        let src = "r\"C:\\\\dir\\n\" r#\"say \"hi\"\"# \"\"\"\n    a \"b\"\n      c\\td\n    \"\"\" \"\"\"x\"\"\"";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "{diags:#?}");
        let strings: Vec<String> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            strings,
            vec![
                "C:\\\\dir\\n".to_string(),
                "say \"hi\"".to_string(),
                "a \"b\"\n  c\td".to_string(),
                "x".to_string(),
            ]
        );
        // `r` alone is still an identifier.
        let (tokens, _) = lex("let r = 1;", "test.aic");
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Ident(name) if name == "r")));
        for src in ["r\"open", "r#\"open\"", "\"\"\"open"] {
            let (_, diags) = lex(src, "test.aic");
            assert!(diags.iter().any(|d| d.code == "E0006"), "{src}: {diags:#?}");
        }
    }

    #[test]
    fn lexes_binary_octal_literals_and_digit_separators() {
        let (tokens, diags) = lex(
            "1_000_000 0b1010_0101 0xFF_FF 0o17 0B11 1_000_i64 1_000.5_5 2_5e1_0 0b1111u8",
            "test.aic",
        );
        assert!(diags.is_empty(), "{diags:#?}");
        let ints: Vec<(i64, String)> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Int(lit) => Some((lit.value, lit.raw_text.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            ints,
            vec![
                (1_000_000, "1_000_000".to_string()),
                (0b1010_0101, "0b1010_0101".to_string()),
                (0xFF_FF, "0xFF_FF".to_string()),
                (0o17, "0o17".to_string()),
                (3, "0B11".to_string()),
                (1000, "1_000_".to_string()),
                (15, "0b1111".to_string()),
            ]
        );
        let floats: Vec<f64> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Float(lit) => Some(lit.value),
                _ => None,
            })
            .collect();
        assert_eq!(floats, vec![1000.55, 25e10]);
        assert_eq!(parse_int_literal_text("0b1_0"), Some(2));
        assert_eq!(parse_int_literal_text("0x"), None);

        for (src, code, text) in [
            ("0b102", "E0003", "not valid in a base-2 literal"),
            ("0o9", "E0003", "not valid in a base-8 literal"),
            ("0x_", "E0003", "has no digits"),
            ("0b", "E0003", "has no digits"),
            (
                "1_000_000_000_000_000_000_000",
                "E0004",
                "invalid integer literal",
            ),
        ] {
            let (_, diags) = lex(src, "test.aic");
            assert!(
                diags
                    .iter()
                    .any(|d| d.code == code && d.message.contains(text)),
                "{src}: {diags:#?}"
            );
        }
    }

    #[test]
    fn lexes_nested_block_comments_as_comments() {
        let src = "/* a /* nested */ b */ fn /* inline */ f() -> Int { 1 }";
        let (tokens, comments, diags) = lex_with_comments(src, "test.aic");
        assert!(diags.is_empty(), "{diags:#?}");
        assert!(matches!(tokens[0].kind, TokenKind::KwFn));
        assert_eq!(
            comments.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            vec!["/* a /* nested */ b */", "/* inline */"]
        );
        let (_, diags) = lex("fn f() -> Int { 1 } /* never /* closed */", "test.aic");
        assert!(diags.iter().any(|d| d.code == "E0002"), "{diags:#?}");
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
    fn lexes_extern_intrinsic_and_unsafe_keywords() {
        let src = r#"extern "C" fn c_abs(x: Int) -> Int; intrinsic fn aic_math_abs_intrinsic(x: Int) -> Int; unsafe fn wrap(x: Int) -> Int { unsafe { c_abs(x) } }"#;
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty());
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwExtern)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::KwIntrinsic)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::KwUnsafe)));
    }

    #[test]
    fn lexes_attribute_tokens() {
        let src = r#"#[get("/health")] fn main() -> Int { 0 }"#;
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Hash)));
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::LBracket)));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Ident(name) if name == "get")));
    }

    #[test]
    fn lexes_float_literals() {
        let src = "let a = 3.125; let b = 0.5; let c = 1e10; let d = 2.5e-3;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Float(lit) if (lit.value - 3.125).abs() < 1e-12 && lit.suffix.is_none())));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Float(lit) if (lit.value - 0.5).abs() < 1e-12 && lit.suffix.is_none())));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Float(lit) if (lit.value - 1.0e10).abs() < 1.0 && lit.suffix.is_none())));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Float(lit) if (lit.value - 2.5e-3).abs() < 1e-12 && lit.suffix.is_none())));
    }

    #[test]
    fn lexes_typed_float_literal_suffixes() {
        let src = "let a = 1.0f32; let b = 2.5e1f64;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens.iter().any(|t| {
            matches!(
                &t.kind,
                TokenKind::Float(lit)
                    if (lit.value - 1.0).abs() < 1e-12
                        && lit.suffix == Some(FloatLiteralSuffixToken::F32)
                        && lit.raw_text == "1.0"
                        && lit.raw_value_span.end < t.span.end
            )
        }));
        assert!(tokens.iter().any(|t| {
            matches!(
                &t.kind,
                TokenKind::Float(lit)
                    if (lit.value - 25.0).abs() < 1e-9
                        && lit.suffix == Some(FloatLiteralSuffixToken::F64)
                        && lit.raw_text == "2.5e1"
                        && lit.raw_value_span.end < t.span.end
            )
        }));
    }

    #[test]
    fn lexes_bitwise_and_shift_tokens_with_hex_literals() {
        let src = "let x = 0xFF & 0x0F | 0xF0 ^ 0x0A; let y = ~x; let z = x << 2; let w = z >> 1; let u = w >>> 3; x &= 1; x |= 2; x ^= 3; x <<= 1; x >>= 1; x >>>= 1;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Int(ref lit) if lit.value == 255)));
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

    #[test]
    fn lexes_typed_integer_literal_suffixes() {
        let src = "let a = 1i8; let b = 2u16; let c = 0xFFu8; let d = 3i128; let e = 4u128;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens.iter().any(|t| {
            matches!(
                t.kind,
                TokenKind::Int(ref lit)
                    if lit.value == 1
                        && lit.suffix == Some(IntLiteralSuffixToken::I8)
                        && lit.raw_value_span.end < t.span.end
            )
        }));
        assert!(tokens.iter().any(|t| {
            matches!(
                t.kind,
                TokenKind::Int(ref lit)
                    if lit.value == 2
                        && lit.suffix == Some(IntLiteralSuffixToken::U16)
                        && lit.raw_value_span.end < t.span.end
            )
        }));
        assert!(tokens.iter().any(|t| {
            matches!(
                t.kind,
                TokenKind::Int(ref lit)
                    if lit.value == 255
                        && lit.suffix == Some(IntLiteralSuffixToken::U8)
                        && lit.raw_value_span.end < t.span.end
            )
        }));
        assert!(tokens.iter().any(|t| {
            matches!(
                t.kind,
                TokenKind::Int(ref lit)
                    if lit.value == 3
                        && lit.suffix == Some(IntLiteralSuffixToken::I128)
                        && lit.raw_value_span.end < t.span.end
            )
        }));
        assert!(tokens.iter().any(|t| {
            matches!(
                t.kind,
                TokenKind::Int(ref lit)
                    if lit.value == 4
                        && lit.suffix == Some(IntLiteralSuffixToken::U128)
                        && lit.raw_value_span.end < t.span.end
            )
        }));
    }

    #[test]
    fn lexes_large_u128_suffix_without_overflow_diagnostic() {
        let src = "let max = 340282366920938463463374607431768211455u128;";
        let (tokens, diags) = lex(src, "test.aic");
        assert!(diags.is_empty(), "diags={diags:#?}");
        assert!(tokens.iter().any(|t| {
            matches!(
                t.kind,
                TokenKind::Int(ref lit)
                    if lit.suffix == Some(IntLiteralSuffixToken::U128)
                && lit.raw_text == "340282366920938463463374607431768211455"
            )
        }));
    }

    #[test]
    fn reports_out_of_range_u128_literal_deterministically() {
        let src = "let too_big = 340282366920938463463374607431768211456u128;";
        let (_tokens, diags) = lex(src, "test.aic");
        assert!(diags.iter().any(|d| {
            d.code == "E0004"
                && d.message.contains("invalid integer literal")
                && d.message
                    .contains("340282366920938463463374607431768211456")
        }));
    }

    #[test]
    fn reports_invalid_integer_suffix() {
        let src = "let a = 1i32x;";
        let (_tokens, diags) = lex(src, "test.aic");
        assert!(diags.iter().any(|d| {
            d.code == "E0009"
                && d.message.contains("invalid integer literal suffix")
                && d.message.contains("i32x")
        }));
    }

    #[test]
    fn reports_float_suffix_deterministically() {
        let src = "let a = 1.5u8;";
        let (_tokens, diags) = lex(src, "test.aic");
        assert!(diags.iter().any(|d| {
            d.code == "E0010"
                && d.message.contains("invalid float literal suffix")
                && d.message.contains("u8")
        }));
    }
}
