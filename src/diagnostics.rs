use crate::span::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSpan {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub label: Option<String>,
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
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>, file: &str, span: Span) -> Self {
        crate::diagnostic_codes::assert_registered(code);
        Self {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.into(),
            spans: vec![DiagnosticSpan {
                file: file.to_string(),
                start: span.start,
                end: span.end,
                label: None,
            }],
            help: Vec::new(),
            suggested_fixes: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        if let Some(first) = self.spans.first_mut() {
            first.label = Some(label.into());
        }
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    pub fn with_fix(mut self, fix: SuggestedFix) -> Self {
        self.suggested_fixes.push(fix);
        self
    }

    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
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
