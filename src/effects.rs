use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::Diagnostic;
use crate::ir;

pub const KNOWN_EFFECTS: &[&str] = &[
    "io",
    "fs",
    "net",
    "time",
    "rand",
    "env",
    "proc",
    "concurrency",
];

pub const KNOWN_CAPABILITIES: &[&str] = &[
    "io",
    "fs",
    "net",
    "time",
    "rand",
    "env",
    "proc",
    "concurrency",
];

pub fn normalize_effect_declarations(program: &mut ir::Program, file: &str) -> Vec<Diagnostic> {
    normalize_effect_declarations_with_context(program, file, None, None)
}

pub fn normalize_effect_declarations_with_context(
    program: &mut ir::Program,
    file: &str,
    item_modules: Option<&[Option<Vec<String>>]>,
    module_files: Option<&BTreeMap<String, String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let known: BTreeSet<&str> = KNOWN_EFFECTS.iter().copied().collect();
    let entry_module = program.module.as_ref();

    for (index, item) in program.items.iter_mut().enumerate() {
        let ir::Item::Function(func) = item else {
            continue;
        };
        let module_name = module_for_item(entry_module, item_modules, index);
        let diag_file = file_for_module(&module_name, file, module_files);

        let mut seen = BTreeSet::new();
        let mut normalized = Vec::new();

        for effect in &func.effects {
            if !known.contains(effect.as_str()) {
                let mut diag = Diagnostic::error(
                    "E2003",
                    format!("unknown effect '{}'", effect),
                    diag_file,
                    func.span,
                )
                .with_help(format!("known effects: {}", KNOWN_EFFECTS.join(", ")));
                if let Some(suggestion) = closest_known_effect(effect) {
                    diag = diag.with_help(format!("did you mean '{}'?", suggestion));
                }
                diagnostics.push(diag);
                continue;
            }
            if !seen.insert(effect.clone()) {
                diagnostics.push(Diagnostic::error(
                    "E2004",
                    format!("duplicate effect '{}' in signature", effect),
                    diag_file,
                    func.span,
                ));
                continue;
            }
            normalized.push(effect.clone());
        }

        normalized.sort();
        func.effects = normalized;
    }

    diagnostics
}

pub fn check_effect_declarations(program: &ir::Program, file: &str) -> Vec<Diagnostic> {
    let mut cloned = program.clone();
    normalize_effect_declarations(&mut cloned, file)
}

pub fn normalize_capability_declarations(program: &mut ir::Program, file: &str) -> Vec<Diagnostic> {
    normalize_capability_declarations_with_context(program, file, None, None)
}

pub fn normalize_capability_declarations_with_context(
    program: &mut ir::Program,
    file: &str,
    item_modules: Option<&[Option<Vec<String>>]>,
    module_files: Option<&BTreeMap<String, String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let known: BTreeSet<&str> = KNOWN_CAPABILITIES.iter().copied().collect();
    let entry_module = program.module.as_ref();

    for (index, item) in program.items.iter_mut().enumerate() {
        let ir::Item::Function(func) = item else {
            continue;
        };
        let module_name = module_for_item(entry_module, item_modules, index);
        let diag_file = file_for_module(&module_name, file, module_files);

        let mut seen = BTreeSet::new();
        let mut normalized = Vec::new();

        for capability in &func.capabilities {
            if !known.contains(capability.as_str()) {
                let mut diag = Diagnostic::error(
                    "E2007",
                    format!("unknown capability '{}'", capability),
                    diag_file,
                    func.span,
                )
                .with_help(format!(
                    "known capabilities: {}",
                    KNOWN_CAPABILITIES.join(", ")
                ));
                if let Some(suggestion) = closest_known_capability(capability) {
                    diag = diag.with_help(format!("did you mean '{}'?", suggestion));
                }
                diagnostics.push(diag);
                continue;
            }
            if !seen.insert(capability.clone()) {
                diagnostics.push(Diagnostic::error(
                    "E2008",
                    format!("duplicate capability '{}' in signature", capability),
                    diag_file,
                    func.span,
                ));
                continue;
            }
            normalized.push(capability.clone());
        }

        normalized.sort();
        func.capabilities = normalized;
    }

    diagnostics
}

pub fn check_capability_declarations(program: &ir::Program, file: &str) -> Vec<Diagnostic> {
    let mut cloned = program.clone();
    normalize_capability_declarations(&mut cloned, file)
}

fn module_for_item(
    entry_module: Option<&Vec<String>>,
    item_modules: Option<&[Option<Vec<String>>]>,
    index: usize,
) -> String {
    if let Some(module) = item_modules
        .and_then(|mods| mods.get(index))
        .and_then(|module| module.as_ref())
    {
        return module.join(".");
    }
    if let Some(module) = entry_module {
        return module.join(".");
    }
    "<root>".to_string()
}

fn file_for_module<'a>(
    module: &str,
    fallback_file: &'a str,
    module_files: Option<&'a BTreeMap<String, String>>,
) -> &'a str {
    module_files
        .and_then(|files| files.get(module).map(|path| path.as_str()))
        .unwrap_or(fallback_file)
}

fn closest_known_effect(effect: &str) -> Option<&'static str> {
    let mut best: Option<(&str, usize)> = None;
    for candidate in KNOWN_EFFECTS {
        let distance = levenshtein(effect, candidate);
        if distance > 2 {
            continue;
        }
        match best {
            Some((_, best_distance)) if best_distance <= distance => {}
            _ => best = Some((candidate, distance)),
        }
    }
    best.map(|(candidate, _)| candidate)
}

fn closest_known_capability(capability: &str) -> Option<&'static str> {
    let mut best: Option<(&str, usize)> = None;
    for candidate in KNOWN_CAPABILITIES {
        let distance = levenshtein(capability, candidate);
        if distance > 2 {
            continue;
        }
        match best {
            Some((_, best_distance)) if best_distance <= distance => {}
            _ => best = Some((candidate, distance)),
        }
    }
    best.map(|(candidate, _)| candidate)
}

fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let a_chars = a.chars().collect::<Vec<_>>();
    let b_chars = b.chars().collect::<Vec<_>>();
    let mut prev = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut curr = vec![0; b_chars.len() + 1];

    for (i, a_ch) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, b_ch) in b_chars.iter().enumerate() {
            let cost = usize::from(a_ch != b_ch);
            let insert = curr[j] + 1;
            let delete = prev[j + 1] + 1;
            let replace = prev[j] + cost;
            curr[j + 1] = insert.min(delete).min(replace);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_chars.len()]
}

#[cfg(test)]
mod tests {
    use crate::{ir_builder::build, parser::parse};

    use super::{
        check_capability_declarations, check_effect_declarations,
        normalize_capability_declarations, normalize_effect_declarations,
    };

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

    #[test]
    fn normalizes_effect_signature_order() {
        let src = "fn f() -> () effects { time, io, fs } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let mut ir = build(&program.expect("program"));
        let diags = normalize_effect_declarations(&mut ir, "test.aic");
        assert!(diags.is_empty());
        let func = match &ir.items[0] {
            crate::ir::Item::Function(func) => func,
            _ => panic!("expected function"),
        };
        assert_eq!(
            func.effects,
            vec!["fs".to_string(), "io".to_string(), "time".to_string()]
        );
    }

    #[test]
    fn unknown_effect_diagnostic_suggests_known_taxonomy() {
        let src = "fn f() -> () effects { oi } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let mut ir = build(&program.expect("program"));
        let diags = normalize_effect_declarations(&mut ir, "test.aic");
        let diag = diags
            .iter()
            .find(|d| d.code == "E2003")
            .expect("missing unknown effect diagnostic");
        assert!(
            diag.help.iter().any(|h| h.contains("known effects")),
            "help={:?}",
            diag.help
        );
        assert!(
            diag.help.iter().any(|h| h.contains("did you mean 'io'?")),
            "help={:?}",
            diag.help
        );
    }

    #[test]
    fn catches_unknown_capability() {
        let src = "fn f() -> () capabilities { mystery } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = check_capability_declarations(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E2007"));
    }

    #[test]
    fn catches_duplicate_capabilities() {
        let src = "fn f() -> () capabilities { io, io } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let ir = build(&program.expect("program"));
        let diags = check_capability_declarations(&ir, "test.aic");
        assert!(diags.iter().any(|d| d.code == "E2008"));
    }

    #[test]
    fn normalizes_capability_signature_order() {
        let src = "fn f() -> () capabilities { time, io, fs } { () }";
        let (program, d) = parse(src, "test.aic");
        assert!(d.is_empty());
        let mut ir = build(&program.expect("program"));
        let diags = normalize_capability_declarations(&mut ir, "test.aic");
        assert!(diags.is_empty());
        let func = match &ir.items[0] {
            crate::ir::Item::Function(func) => func,
            _ => panic!("expected function"),
        };
        assert_eq!(
            func.capabilities,
            vec!["fs".to_string(), "io".to_string(), "time".to_string()]
        );
    }
}
