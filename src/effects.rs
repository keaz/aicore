use std::collections::BTreeSet;

use crate::diagnostics::Diagnostic;
use crate::ir;

pub const KNOWN_EFFECTS: &[&str] = &["io", "fs", "net", "time", "rand"];

pub fn check_effect_declarations(program: &ir::Program, file: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let known: BTreeSet<&str> = KNOWN_EFFECTS.iter().copied().collect();

    for item in &program.items {
        if let ir::Item::Function(func) = item {
            let mut seen = BTreeSet::new();
            for effect in &func.effects {
                if !known.contains(effect.as_str()) {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2003",
                            format!("unknown effect '{}'", effect),
                            file,
                            func.span,
                        )
                        .with_help(format!("known effects: {}", KNOWN_EFFECTS.join(", "))),
                    );
                }
                if !seen.insert(effect) {
                    diagnostics.push(Diagnostic::error(
                        "E2004",
                        format!("duplicate effect '{}' in signature", effect),
                        file,
                        func.span,
                    ));
                }
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use crate::{ir_builder::build, parser::parse};

    use super::check_effect_declarations;

    #[test]
    fn catches_unknown_effect() {
        let src = "fn f() -> () effects { mystery } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = check_effect_declarations(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E2003"));
    }

    #[test]
    fn catches_duplicate_effects() {
        let src = "fn f() -> () effects { io, io } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = check_effect_declarations(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E2004"));
    }
}
