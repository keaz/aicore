use crate::span::Span;
use serde::{Deserialize, Serialize};

use crate::diagnostic_reasoning::DiagnosticReasoning;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSpan {
    pub file: String,
    /// Byte offset of the first byte of the span.
    pub start: usize,
    /// Byte offset just past the last byte of the span (exclusive).
    pub end: usize,
    pub label: Option<String>,
    /// 1-based line of `start`. Filled by [`attach_line_columns`] when the
    /// source text is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    /// 1-based column of `start`, counted in Unicode scalar values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    /// 1-based line of `end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    /// 1-based column of `end` (exclusive, like `end`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

/// Converts a byte offset into a 1-based `(line, column)` pair. Columns count
/// Unicode scalar values. Offsets past the end clamp to the end of `source`;
/// offsets inside a multi-byte character resolve to that character.
pub fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    let clamped = offset.min(source.len());
    for (idx, ch) in source.char_indices() {
        if idx >= clamped || idx + ch.len_utf8() > clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Fills `start_line`/`start_column`/`end_line`/`end_column` for spans in
/// `file` using the given source text. Spans that already have positions are
/// left unchanged.
pub fn attach_line_columns_with_source(diagnostics: &mut [Diagnostic], file: &str, source: &str) {
    for span in diagnostics
        .iter_mut()
        .flat_map(|diag| diag.spans.iter_mut())
        .filter(|span| span.file == file)
    {
        fill_span_position(span, source);
    }
}

/// Fills line/column positions for every span whose file can be read from
/// disk. Each file is read at most once. Spans of unreadable files keep only
/// their byte offsets.
pub fn attach_line_columns(diagnostics: &mut [Diagnostic]) {
    let mut sources: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();
    for span in diagnostics
        .iter_mut()
        .flat_map(|diag| diag.spans.iter_mut())
        .filter(|span| span.start_line.is_none() && !span.file.is_empty())
    {
        let source = sources
            .entry(span.file.clone())
            .or_insert_with(|| std::fs::read_to_string(&span.file).ok());
        if let Some(source) = source {
            fill_span_position(span, source);
        }
    }
}

/// Removes exact duplicate diagnostics, keeping the first occurrence and the
/// original order.
pub fn dedup_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: Vec<Diagnostic> = Vec::with_capacity(diagnostics.len());
    diagnostics.retain(|diag| {
        if seen.contains(diag) {
            false
        } else {
            seen.push(diag.clone());
            true
        }
    });
}

fn fill_span_position(span: &mut DiagnosticSpan, source: &str) {
    if span.start_line.is_some() {
        return;
    }
    let (start_line, start_column) = offset_to_line_col(source, span.start);
    let (end_line, end_column) = offset_to_line_col(source, span.end.max(span.start));
    span.start_line = Some(start_line);
    span.start_column = Some(start_column);
    span.end_line = Some(end_line);
    span.end_column = Some(end_column);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestedFix {
    pub message: String,
    pub replacement: Option<String>,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub spans: Vec<DiagnosticSpan>,
    pub help: Vec<String>,
    pub suggested_fixes: Vec<SuggestedFix>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Box<DiagnosticReasoning>>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>, file: &str, span: Span) -> Self {
        crate::diagnostic_codes::assert_registered(code);
        let mut diagnostic = Self {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.into(),
            spans: vec![DiagnosticSpan {
                file: file.to_string(),
                start: span.start,
                end: span.end,
                label: None,
                ..DiagnosticSpan::default()
            }],
            help: Vec::new(),
            suggested_fixes: Vec::new(),
            reasoning: None,
        };
        diagnostic.refresh_reasoning();
        diagnostic
    }

    pub fn warning(code: &str, message: impl Into<String>, file: &str, span: Span) -> Self {
        crate::diagnostic_codes::assert_registered(code);
        let mut diagnostic = Self {
            code: code.to_string(),
            severity: Severity::Warning,
            message: message.into(),
            spans: vec![DiagnosticSpan {
                file: file.to_string(),
                start: span.start,
                end: span.end,
                label: None,
                ..DiagnosticSpan::default()
            }],
            help: Vec::new(),
            suggested_fixes: Vec::new(),
            reasoning: None,
        };
        diagnostic.refresh_reasoning();
        diagnostic
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        if let Some(first) = self.spans.first_mut() {
            first.label = Some(label.into());
        }
        self.refresh_reasoning();
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self.refresh_reasoning();
        self
    }

    pub fn with_fix(mut self, fix: SuggestedFix) -> Self {
        self.suggested_fixes.push(fix);
        self.refresh_reasoning();
        self
    }

    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    pub fn refresh_reasoning(&mut self) {
        self.reasoning = crate::diagnostic_reasoning::derive_reasoning(self).map(Box::new);
    }
}

#[derive(Debug, Default, Clone)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        attach_line_columns, attach_line_columns_with_source, dedup_diagnostics,
        offset_to_line_col, Diagnostic, DiagnosticSpan,
    };
    use crate::span::Span;

    #[test]
    fn offset_to_line_col_is_one_based_and_counts_characters() {
        let source = "fn a() {}\nlet é = 1;\n\nend";
        assert_eq!(offset_to_line_col(source, 0), (1, 1));
        assert_eq!(offset_to_line_col(source, 3), (1, 4));
        // Offset of the newline itself stays on line 1; the next byte starts line 2.
        assert_eq!(offset_to_line_col(source, 9), (1, 10));
        assert_eq!(offset_to_line_col(source, 10), (2, 1));
        // `é` is two bytes but one column.
        let e_offset = source.find('é').expect("e");
        assert_eq!(offset_to_line_col(source, e_offset), (2, 5));
        assert_eq!(offset_to_line_col(source, e_offset + 2), (2, 6));
        // An offset inside a multi-byte character resolves to that character.
        assert_eq!(offset_to_line_col(source, e_offset + 1), (2, 5));
        assert_eq!(offset_to_line_col(source, source.len()), (4, 4));
        // Offsets past the end clamp to the end of the source.
        assert_eq!(offset_to_line_col(source, source.len() + 50), (4, 4));
        assert_eq!(offset_to_line_col("", 7), (1, 1));
    }

    #[test]
    fn offset_to_line_col_treats_crlf_as_one_line_break() {
        let source = "a\r\nbc";
        assert_eq!(offset_to_line_col(source, 3), (2, 1));
        assert_eq!(offset_to_line_col(source, 4), (2, 2));
    }

    #[test]
    fn attach_with_source_fills_matching_file_only() {
        let mut diags = vec![
            Diagnostic::error("E1001", "x", "a.aic", Span::new(4, 9)),
            Diagnostic::error("E1001", "y", "b.aic", Span::new(0, 1)),
        ];
        attach_line_columns_with_source(&mut diags, "a.aic", "one\ntwo three\n");
        let span = &diags[0].spans[0];
        assert_eq!(
            (
                span.start_line,
                span.start_column,
                span.end_line,
                span.end_column
            ),
            (Some(2), Some(1), Some(2), Some(6))
        );
        assert_eq!(diags[1].spans[0].start_line, None);
    }

    #[test]
    fn attach_reads_files_and_skips_unreadable_paths() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("main.aic");
        fs::write(&file, "fn main() -> Int {\n  missing\n}\n").expect("write");
        let file = file.to_string_lossy().to_string();
        let mut diags = vec![
            Diagnostic::error("E1208", "unknown", &file, Span::new(21, 28)),
            Diagnostic::error(
                "E1208",
                "gone",
                "/definitely/missing/file.aic",
                Span::new(3, 4),
            ),
        ];
        diags[0].spans.push(DiagnosticSpan {
            file: file.clone(),
            start: 0,
            end: 2,
            label: Some("second".to_string()),
            ..DiagnosticSpan::default()
        });
        attach_line_columns(&mut diags);
        let first = &diags[0].spans[0];
        assert_eq!((first.start_line, first.start_column), (Some(2), Some(3)));
        assert_eq!((first.end_line, first.end_column), (Some(2), Some(10)));
        let second = &diags[0].spans[1];
        assert_eq!((second.start_line, second.start_column), (Some(1), Some(1)));
        assert_eq!(diags[1].spans[0].start_line, None);

        // Positions serialize next to byte offsets; missing ones are omitted.
        let json = serde_json::to_value(&diags).expect("json");
        assert_eq!(json[0]["spans"][0]["start_line"], 2);
        assert_eq!(json[0]["spans"][0]["end_column"], 10);
        assert!(json[1]["spans"][0].get("start_line").is_none());

        // Attaching again keeps existing positions, even if the file changed.
        fs::write(dir.path().join("main.aic"), "").expect("truncate");
        attach_line_columns(&mut diags);
        assert_eq!(diags[0].spans[0].start_line, Some(2));
    }

    #[test]
    fn spans_without_positions_still_deserialize() {
        let raw = r#"{"file":"a.aic","start":1,"end":2,"label":null}"#;
        let span: DiagnosticSpan = serde_json::from_str(raw).expect("span");
        assert_eq!(span.start_line, None);
    }

    #[test]
    fn dedup_keeps_first_occurrence_in_order() {
        let a = Diagnostic::error("E1001", "a", "x.aic", Span::new(1, 2));
        let b = Diagnostic::error("E1002", "b", "x.aic", Span::new(0, 1));
        let a_other_span = Diagnostic::error("E1001", "a", "x.aic", Span::new(5, 6));
        let mut diags = vec![
            a.clone(),
            b.clone(),
            a.clone(),
            a_other_span.clone(),
            b.clone(),
        ];
        dedup_diagnostics(&mut diags);
        assert_eq!(diags, vec![a, b, a_other_span]);
    }
}
